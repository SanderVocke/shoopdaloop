use crate::types::*;
use anyhow::anyhow;
use common::logging::macros::*;
use minidumper::Server;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
#[cfg(target_os = "windows")]
use std::time::Duration;
shoop_log_unit!("CrashHandlingServer");

fn set_json(to: &mut JsonValue, from: &JsonValue) -> anyhow::Result<()> {
    if from.is_object() {
        if !to.is_object() {
            return Err(anyhow!("expected object"));
        }

        for (k, v) in from.as_object().ok_or(anyhow!("expected object"))?.iter() {
            if v.is_object() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!({});
                } else if !to.get(k).and_then(|v| v.as_object()).is_some() {
                    return Err(anyhow!("expected object"));
                }
                set_json(&mut to[k], v)?;
            } else if v.is_array() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!([]);
                } else if !to.get(k).and_then(|v| v.as_array()).is_some() {
                    return Err(anyhow!("expected array"));
                }
                set_json(&mut to[k], v)?;
            } else {
                to[k] = v.clone();
            }
        }
    } else {
        *to = from.clone();
    }
    Ok(())
}

fn try_get_socket_name() -> Option<String> {
    std::env::args().last()
}

fn try_create_server(name: &str) -> Option<Server> {
    Server::with_name(name).ok()
}

pub fn crashhandling_server() {
    #[cfg(unix)]
    eprintln!("[CRASH_DBG] Server process starting: pid={}, ppid={}", std::process::id(), unsafe { libc::getppid() });
    #[cfg(not(unix))]
    eprintln!("[CRASH_DBG] Server process starting: pid={}", std::process::id());
    debug!("Starting crash handling server");

    let socket_name = match try_get_socket_name() {
        Some(s) => s,
        None => {
            error!("no socket name provided");
            eprintln!("[CRASH_DBG] Server: no socket name provided, exiting with code 1");
            std::process::exit(1);
        }
    };
    eprintln!("[CRASH_DBG] Server: socket name = {socket_name}");

    debug!("Server: crash handling socket: {socket_name}");

    let mut server = match try_create_server(&socket_name) {
        Some(s) => s,
        None => {
            error!("failed to create crash handling server");
            eprintln!("[CRASH_DBG] Server: failed to create server, exiting with code 1");
            std::process::exit(1);
        }
    };
    eprintln!("[CRASH_DBG] Server: server object created, about to run()", );

    let ab = std::sync::atomic::AtomicBool::new(false);
    struct Handler {
        json: Mutex<JsonValue>,
        last_written_dump: Mutex<Option<PathBuf>>, // For interior mutability
    }

    impl minidumper::ServerHandler for Handler {
        /// Called when a crash has been received and a backing file needs to be
        /// created to store it.
        fn create_minidump_file(
            &self,
        ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
            let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();

            let (dumpfile, dumpfilepath) = (if let Some(dump_folder) = maybe_dump_folder.as_ref() {
                let path = std::path::PathBuf::from(dump_folder);
                tempfile::NamedTempFile::with_suffix_in(".dmp", &path)
            } else {
                tempfile::NamedTempFile::with_suffix(".dmp")
            })
            .map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Could not open temp file for minidump: {e}"),
                )
            })
            .and_then(|f| Ok(f.into_parts()))?;
            let dumpfilepath = dumpfilepath.keep().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Could not persist temp file: {e}"),
                )
            })?;
            let mut ref_last_written_dump = self
                .last_written_dump
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *ref_last_written_dump = Some(dumpfilepath.clone());

            info!("Writing minidump to {}", dumpfilepath.display());
            Ok((dumpfile, dumpfilepath))
        }

        /// Called when a crash has been fully written as a minidump to the provided
        /// file. Also returns the full heap buffer as well.
        fn on_minidump_created(
            &self,
            result: Result<minidumper::MinidumpBinary, minidumper::Error>,
        ) -> minidumper::LoopAction {
            match result {
                Ok(mut md_bin) => {
                    use std::io::Write;
                    let _ = md_bin.file.flush();
                    info!("Done writing minidump");

                    debug!("Writing crash metadata json...");
                    let path = &md_bin.path;

                    let result = || -> Result<(), anyhow::Error> {
                        let filename = path
                            .file_name()
                            .ok_or(anyhow!("Could not get dump filename"))?
                            .to_str()
                            .ok_or(anyhow!("Could not convert dump filename"))?;
                        let new_filename = format!("{filename}.metadata.json");
                        let mut json_path: std::path::PathBuf = path.clone();
                        json_path.set_file_name(new_filename);

                        // Write the metadata json to this file
                        let guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow!("Failed to lock json: {e:?}"))?;
                        let content = serde_json::to_string_pretty(&*guard)
                            .map_err(|e| anyhow!("Failed to serialize metadata json: {e:?}"))?;
                        let content = format!("{content}\n");
                        let mut jsonfile = File::create(&json_path)?;
                        jsonfile.write_all(content.as_bytes())?;
                        jsonfile.flush()?;
                        info!("Wrote crash metadata json to {json_path:?}");

                        Ok(())
                    }();
                    match result {
                        Ok(()) => (),
                        Err(e) => error!("Failed to write crash metadata json: {e:?}"),
                    };
                }
                Err(e) => {
                    error!("Failed to write minidump: {:#}", e);
                }
            }

            // Tells the server to continue
            minidumper::LoopAction::Continue
        }

        fn on_message(&self, kind: u32, buffer: Vec<u8>) {
            let content = match String::from_utf8(buffer) {
                Ok(s) => s,
                Err(e) => {
                    error!("Invalid UTF-8 in message: {}", e);
                    return;
                }
            };
            match kind {
                v if v == CrashHandlingMessageType::SetJson as usize as u32 => {
                    let result = || -> anyhow::Result<()> {
                        debug!("Set JSON: {content:?}");
                        let mut guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow!("Could not lock json: {e:?}"))?;
                        let incoming_json: JsonValue = serde_json::from_str(content.as_str())?;
                        set_json(&mut *guard, &incoming_json)?;
                        Ok(())
                    }();
                    match result {
                        Ok(()) => (),
                        Err(e) => error!("Failed to update crash metadata json: {e:?}"),
                    };
                }
                v if v == CrashHandlingMessageType::AdditionalCrashAttachment as usize as u32 => {
                    let attachment: AdditionalCrashAttachment =
                        match serde_json::from_str(content.as_str()) {
                            Ok(a) => a,
                            Err(e) => {
                                error!("Failed to parse additional crash attachment: {}", e);
                                return;
                            }
                        };
                    let id = attachment.id;
                    let ref_last_written_dump = self
                        .last_written_dump
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let last_written_dump = &*ref_last_written_dump;
                    if last_written_dump.is_none() {
                        warn!("Received additional crash data attachment ('{id}'), but no dump written. Ignoring.");
                    } else {
                        let result = || -> anyhow::Result<()> {
                            let path = last_written_dump
                                .as_ref()
                                .ok_or(anyhow!("No dump file remembered"))?;
                            let filename = path
                                .file_name()
                                .ok_or(anyhow!("Invalid file name"))?
                                .to_string_lossy();
                            let new_filename = format!("{filename}.attach.{id}");
                            let mut attach_path: std::path::PathBuf = path.clone();
                            attach_path.set_file_name(new_filename);
                            let mut file = File::create(&attach_path)?;
                            file.write_all(attachment.contents.as_bytes())?;
                            file.flush()?;
                            info!("Wrote crash attachment '{id}' to {attach_path:?}");
                            Ok(())
                        }();
                        match result {
                            Ok(()) => (),
                            Err(e) => error!("Failed to write crash attachment '{id}': {e:?}"),
                        }
                    }
                }
                _ => error!("Unknown message kind {kind}, content {content}"),
            };
        }

        fn on_client_disconnected(&self, _num_clients: usize) -> minidumper::LoopAction {
            debug!("Client disconnected, # -> {_num_clients}");
            eprintln!("[CRASH_DBG] Server: client disconnected, remaining clients = {_num_clients}");
            if _num_clients == 0 {
                eprintln!("[CRASH_DBG] Server: no clients left, returning Exit");
                minidumper::LoopAction::Exit
            } else {
                minidumper::LoopAction::Continue
            }
        }

        fn on_client_connected(&self, _num_clients: usize) -> minidumper::LoopAction {
            debug!("Client connected, # -> {_num_clients}");
            eprintln!("[CRASH_DBG] Server: client connected, total clients = {_num_clients}");
            minidumper::LoopAction::Continue
        }
    }

    let maybe_base_json: Option<String> = std::env::var("SHOOP_CRASH_METADATA_BASE_JSON").ok();
    let base_json: JsonValue = if let Some(base_json_str) = maybe_base_json {
        serde_json::from_str(&base_json_str).unwrap_or_else(|_| {
            error!("invalid base crash handling json");
            serde_json::json!({ "environment": "unknown", "error": "invalid_base_json" })
        })
    } else {
        serde_json::json!({ "environment": "unknown" })
    };

    #[cfg(target_os = "windows")]
    watchdog::start();

    // NOTE: don't put any logging beyond this point. On Windows,
    // it may cause deadlocks because of trying to write to the
    // nonexistent parent process' output stream.

    let result = server.run(
        Box::new(Handler {
            json: Mutex::new(base_json),
            last_written_dump: Mutex::new(None),
        }),
        &ab,
        Some(std::time::Duration::from_millis(5000)),
    );

    if let Err(e) = result {
        error!("failed to run server: {}", e);
        eprintln!("[CRASH_DBG] Server: run() failed: {e}, exiting with code 1");
        std::process::exit(1);
    }

    // ensure that we will exit
    eprintln!("[CRASH_DBG] Server: run() completed normally, about to exit(0)");
    #[cfg(target_os = "windows")]
    watchdog::notify(Duration::from_millis(3000));
    std::process::exit(0);
}

#[cfg(target_os = "windows")]
mod watchdog {
    use once_cell::sync::OnceCell;
    use std::sync::Mutex;
    use std::thread;
    use std::time::Duration;
    use winapi::um::processthreadsapi::{GetCurrentProcess, TerminateProcess};
    use winapi::um::winnt::HANDLE;

    static DO_EXIT_IN: OnceCell<Mutex<Option<Duration>>> = OnceCell::new();

    pub fn start() {
        let _ = DO_EXIT_IN.get_or_init(|| Mutex::new(None));
        thread::spawn(move || {
            let get_timeout = || -> Option<Duration> {
                match DO_EXIT_IN.get() {
                    Some(mutex) => match mutex.lock() {
                        Ok(lock) => match lock.as_ref() {
                            Some(a) => Some(*a),
                            None => None,
                        },
                        Err(_) => None,
                    },
                    None => None,
                }
            };

            while get_timeout().is_none() {
                thread::sleep(Duration::from_millis(1000));
            }

            if let Some(timeout) = get_timeout() {
                thread::sleep(timeout);
                let process_handle: HANDLE = unsafe { GetCurrentProcess() };
                unsafe {
                    TerminateProcess(process_handle, 1);
                }
            }
        });
    }

    pub fn notify(timeout: std::time::Duration) {
        let lock = DO_EXIT_IN.get_or_init(|| Mutex::new(None)).lock();
        if let Ok(mut lock) = lock {
            lock.replace(timeout);
        }
    }
}
