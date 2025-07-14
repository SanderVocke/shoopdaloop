// Note: mostly taken from the example included with the minidumper crate.

use anyhow;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::sync::OnceLock;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("CrashHandling");

use minidumper::{Client, Server};

enum CrashHandlingMessageType {
    // Recursively set values in the metadata JSON.
    SetJson = 0,
}

struct CrashHandlingMessage {
    pub message_type: CrashHandlingMessageType,
    pub message: String,
}

struct CrashHandlerHandle {
    pub sender: mpsc::Sender<CrashHandlingMessage>,
    pub _handle: Option<thread::JoinHandle<()>>,
}

static CLIENTSIDE_HANDLE: OnceLock<Option<CrashHandlerHandle>> = OnceLock::new();

fn set_json(to: &mut JsonValue, from: &JsonValue) -> anyhow::Result<()> {
    if from.is_object() {
        if !to.is_object() {
            return Err(anyhow::anyhow!("expected object"));
        }

        for (k, v) in from.as_object().unwrap().iter() {
            if v.is_object() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!({});
                } else if !to.get(k).unwrap().is_object() {
                    return Err(anyhow::anyhow!("expected object"));
                }
                return set_json(&mut to[k], v);
            } else if v.is_array() {
                if to.get(k).is_none() {
                    to[k] = serde_json::json!([]);
                } else if !to.get(k).unwrap().is_array() {
                    return Err(anyhow::anyhow!("expected array"));
                }
                return set_json(&mut to[k], v);
            } else {
                to[k] = v.clone();
            }
        }
    } else {
        *to = from.clone();
    }
    Ok(())
}

fn crashhandling_server() {
    info!("Starting crash handling server");

    let socket_name = std::env::args().last().expect("no socket name provided");

    debug!("Server: crash handling socket: {socket_name}");

    let mut server =
        Server::with_name(socket_name.as_str()).expect("failed to create crash handling server");

    let ab = std::sync::atomic::AtomicBool::new(false);
    struct Handler {
        json: Mutex<JsonValue>,
    }

    impl minidumper::ServerHandler for Handler {
        /// Called when a crash has been received and a backing file needs to be
        /// created to store it.
        fn create_minidump_file(
            &self,
        ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
            let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();

            let (dumpfile, dumpfilepath) = (if maybe_dump_folder.is_some() {
                let path = std::path::PathBuf::from(maybe_dump_folder.as_ref().unwrap());
                tempfile::NamedTempFile::with_suffix_in(".dmp", &path)
            } else {
                tempfile::NamedTempFile::with_suffix(".dmp")
            })
            .expect("Could not open temp file for minidump")
            .into_parts();
            let dumpfilepath = dumpfilepath.keep().expect("Could not persist temp file");

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

                    info!("Writing crash metadata json...");
                    let path = &md_bin.path;

                    let result = || -> Result<(), anyhow::Error> {
                        let filename = path
                            .file_name()
                            .ok_or(anyhow::anyhow!("Could not get dump filename"))?
                            .to_str()
                            .ok_or(anyhow::anyhow!("Could not convert dump filename"))?;
                        let new_filename = format!("{filename}.metadata.json");
                        let mut json_path: std::path::PathBuf = path.clone();
                        json_path.set_file_name(new_filename);

                        // Write the metadata json to this file
                        let guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Failed to lock json: {e:?}"))?;
                        let content = serde_json::to_string_pretty(&*guard).map_err(|e| {
                            anyhow::anyhow!("Failed to serialize metadata json: {e:?}")
                        })?;
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

            // Tells the server to exit, which will in turn exit the process
            minidumper::LoopAction::Exit
        }

        fn on_message(&self, kind: u32, buffer: Vec<u8>) {
            let content = String::from_utf8(buffer).unwrap();
            match kind {
                v if v == CrashHandlingMessageType::SetJson as usize as u32 => {
                    let result = || -> anyhow::Result<()> {
                        debug!("Set JSON: {content:?}");
                        let mut guard = self
                            .json
                            .lock()
                            .map_err(|e| anyhow::anyhow!("Could not lock json: {e:?}"))?;
                        let incoming_json: JsonValue = serde_json::from_str(content.as_str())?;
                        set_json(&mut *guard, &incoming_json)?;
                        Ok(())
                    }();
                    match result {
                        Ok(()) => (),
                        Err(e) => error!("Failed to update crash metadata json: {e:?}"),
                    };
                }
                _ => error!("Unknown message kind {kind}, content {content}"),
            };
        }

        fn on_client_disconnected(&self, _num_clients: usize) -> minidumper::LoopAction {
            minidumper::LoopAction::Exit
        }
    }

    let maybe_base_json : Option<String> = std::env::var("SHOOP_CRASH_METADATA_BASE_JSON").ok();
    let base_json : JsonValue =
        if maybe_base_json.is_some() {
            serde_json::from_str(maybe_base_json.unwrap().as_str()).expect("invalid base crash handling json")
        } else {
            serde_json::json!({ "environment": "unknown" })
        };

    server
        .run(
            Box::new(Handler {
                json: Mutex::new(base_json),
            }),
            &ab,
            Some(std::time::Duration::from_millis(1000)),
        )
        .expect("failed to run server");

    info!("Crash handling server finished.");

    std::process::exit(0);
}

fn crashhandling_client(start_server_arg: &str) -> CrashHandlerHandle {
    let (sender, receiver) = mpsc::channel::<CrashHandlingMessage>();
    let start_server_arg = start_server_arg.to_string();

    let handle = thread::spawn(move || {
        let mut server = None;

        // Determine which socket to open
        let socket_name: String = if cfg!(target_os = "linux") {
            format!("shoopdaloop-server-{}", uuid::Uuid::new_v4())
        } else {
            let dir = tempfile::tempdir()
                .expect("Unable to create temporary dir for crash handling socket")
                .keep();
            let dirpath = dir.join("shoop.crashsocket");
            dirpath.to_str().unwrap().to_string()
        };

        debug!("Client: crash handling socket: {socket_name}");

        // Attempt to connect to the server
        let (client, _server) = loop {
            if let Ok(client) = Client::with_name(socket_name.as_str()) {
                break (client, server.unwrap());
            }

            let exe = std::env::current_exe().expect("Unable to find own .exe");

            server = Some(
                std::process::Command::new(exe)
                    .arg(start_server_arg.clone())
                    .arg(socket_name.as_str())
                    .spawn()
                    .expect("unable to spawn server process"),
            );

            // Give it time to start
            std::thread::sleep(Duration::from_millis(100));
        };

        let client_access: Arc<Mutex<Client>> = Arc::new(Mutex::new(client));
        let handler_client_access = client_access.clone();

        #[allow(unsafe_code)]
        let _handler = crash_handler::CrashHandler::attach(unsafe {
            crash_handler::make_crash_event(move |crash_context: &crash_handler::CrashContext| {
                let client = handler_client_access
                    .as_ref()
                    .lock()
                    .expect("Unable to lock IPC client");

                // Send a ping to the server, this ensures that all messages that have been sent
                // are "flushed" before the crash event is sent. This is only really useful
                // on macos where messages and crash events are sent via different, unsynchronized,
                // methods which can result in the crash event closing the server before
                // the non-crash messages are received/processed
                client.ping().unwrap();

                let result = crash_handler::CrashEventResult::Handled(
                    client.request_dump(crash_context).is_ok(),
                );

                error!("Crash detected - requested minidump");

                result
            })
        })
        .expect("failed to attach signal handler");

        // On linux we can explicitly allow only the server process to inspect the
        // process we are monitoring (this one) for crashes
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            _handler.set_ptracer(Some(_server.id()));
        }

        loop {
            match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(msg) => {
                    match client_access
                        .as_ref()
                        .lock()
                        .expect("Unable to lock IPC")
                        .send_message(msg.message_type as usize as u32, msg.message.as_bytes())
                    {
                        Ok(_) => (),
                        Err(err) => warn!("Failed to send message: {}", err),
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    // Received a signal to stop or sender was dropped
                    info!("Periodic task thread: Stopping.");
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Timeout occurred, means no stop signal, so perform the task
                    match client_access
                        .as_ref()
                        .lock()
                        .expect("Unable to lock IPC")
                        .ping()
                    {
                        Ok(_) => (),
                        Err(e) => warn!("Failed to ping crash handling server. {}", e),
                    }
                }
            }
        }
    });

    CrashHandlerHandle {
        sender: sender,
        _handle: Some(handle),
    }
}

pub fn init_crashhandling(is_server: bool, start_server_arg: &str) {
    let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();
    if maybe_dump_folder.is_some() {
        debug!(
            "Using dump folder: {:?}",
            maybe_dump_folder.as_ref().unwrap()
        );
        let path = std::path::PathBuf::from(maybe_dump_folder.unwrap());
        if !path.exists() || !path.is_dir() {
            warn!("Dump folder {path:?} does not exist or is not a directory - crash handler not enabled.");
            return;
        }
    } else {
        debug!("Dumps will go to OS-specific temp folder");
    }

    if is_server {
        crashhandling_server();

        // Unreachable
        return;
    }

    let handle = crashhandling_client(start_server_arg);
    let _ = CLIENTSIDE_HANDLE.get_or_init(|| -> Option<CrashHandlerHandle> { Some(handle) });
}

pub fn set_crash_json_partial(partial_json: JsonValue) {
    let handle = CLIENTSIDE_HANDLE.get_or_init(|| None);
    if handle.is_none() {
        error!("set_metadata called, but no crash handling client active");
    }

    let handle = handle.as_ref().unwrap();
    let content = partial_json.to_string();
    match handle.sender.send(CrashHandlingMessage {
        message_type: CrashHandlingMessageType::SetJson,
        message: content,
    }) {
        Ok(_) => (),
        Err(e) => error!("Failed to send crash handling message: {}", e),
    };
}

pub fn set_crash_json_toplevel_field(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({key: value}));
}

pub fn set_crash_json_tag(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({"tags": {key: value}}));
}
