// Note: mostly taken from the example included with the minidumper crate.

pub mod registered_threads;

use anyhow;
use serde;
use serde_json::Value as JsonValue;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
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
    // After a crash dump request, this is used to post additional
    // information about the crash
    AdditionalCrashAttachment = 1,
}

struct CrashHandlingMessage {
    pub message_type: CrashHandlingMessageType,
    pub message: String,
}

struct CrashHandlerHandle {
    pub sender: mpsc::Sender<CrashHandlingMessage>,
    pub _handle: Option<thread::JoinHandle<()>>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct AdditionalCrashAttachment {
    pub id: String,
    pub contents: String,
}

pub type CrashCallback = fn() -> Vec<AdditionalCrashAttachment>;

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
        last_written_dump: Mutex<Option<PathBuf>>, // For interior mutability
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
            let mut ref_last_written_dump = self.last_written_dump.lock().unwrap();
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

            // Tells the server to continue
            minidumper::LoopAction::Continue
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
                v if v == CrashHandlingMessageType::AdditionalCrashAttachment as usize as u32 => {
                    let attachment: AdditionalCrashAttachment =
                        serde_json::from_str(content.as_str()).unwrap();
                    let id = attachment.id;
                    let ref_last_written_dump = self.last_written_dump.lock().unwrap();
                    let last_written_dump = &*ref_last_written_dump;
                    if last_written_dump.is_none() {
                        warn!("Received additional crash data attachment ('{id}'), but no dump written. Ignoring.");
                    } else {
                        let result = || -> anyhow::Result<()> {
                            let path = last_written_dump
                                .as_ref()
                                .ok_or(anyhow::anyhow!("No dump file remembered"))?;
                            let filename = path.file_name().unwrap().to_string_lossy();
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
            info!("Client disconnected, # -> {_num_clients}");
            if _num_clients == 0 {
                minidumper::LoopAction::Exit
            } else {
                minidumper::LoopAction::Continue
            }
        }

        fn on_client_connected(&self, _num_clients: usize) -> minidumper::LoopAction {
            info!("Client connected, # -> {_num_clients}");
            minidumper::LoopAction::Continue
        }
    }

    let maybe_base_json: Option<String> = std::env::var("SHOOP_CRASH_METADATA_BASE_JSON").ok();
    let base_json: JsonValue = if maybe_base_json.is_some() {
        serde_json::from_str(maybe_base_json.unwrap().as_str())
            .expect("invalid base crash handling json")
    } else {
        serde_json::json!({ "environment": "unknown" })
    };

    server
        .run(
            Box::new(Handler {
                json: Mutex::new(base_json),
                last_written_dump: Mutex::new(None),
            }),
            &ab,
            Some(std::time::Duration::from_millis(5000)),
        )
        .expect("failed to run server");

    info!("Crash handling server finished.");

    std::process::exit(0);
}

fn crashhandling_client(
    start_server_arg: &str,
    on_crash_callback: Option<CrashCallback>,
) -> CrashHandlerHandle {
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

        // Instead of just one, we create two client connections to the crash
        // handling server. This is because:
        // - The first and most important thing we want to do on crash is to request a minidump.
        // - After requesting a minidump, the client connection used for that becomes unusable.
        // - After the minidump we want to salvage whatever information we can to report to
        //   the server, but this is risky and may re-crash.
        // Hence we use client 1 to send runtime info and request the minidump on crash,
        // and client 2 to send additional messages after crash.

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
        // Attempt to connect to the server (client 2)
        let client2 = loop {
            if let Ok(client) = Client::with_name(socket_name.as_str()) {
                break client;
            }

            // Give it time to start
            std::thread::sleep(Duration::from_millis(100));
        };

        let clients_access: Arc<Mutex<(Client, Client)>> = Arc::new(Mutex::new((client, client2)));
        let handler_clients_access = clients_access.clone();
        let ping_clients_access = clients_access.clone();

        #[allow(unsafe_code)]
        let _handler = crash_handler::CrashHandler::attach(unsafe {
            crash_handler::make_crash_event(move |crash_context: &crash_handler::CrashContext| {
                let guard = handler_clients_access
                    .as_ref()
                    .lock()
                    .expect("Unable to lock IPC clients");
                let (client1, client2) = &*guard;

                let thread_name = registered_threads::current_thread_registered_name()
                    .or(Some("unknown".to_string()))
                    .unwrap();
                client1
                    .send_message(
                        CrashHandlingMessageType::SetJson as u32,
                        format!("{{\"tags\":{{\"thread_name\":\"{thread_name}\"}}}}"),
                    )
                    .unwrap();

                // Send a ping to the server, this ensures that all messages that have been sent
                // are "flushed" before the crash event is sent. This is only really useful
                // on macos where messages and crash events are sent via different, unsynchronized,
                // methods which can result in the crash event closing the server before
                // the non-crash messages are received/processed
                client1.ping().unwrap();
                client2.ping().unwrap();

                println!("Crash detected - requesting minidump");

                let handled: bool;
                match client1.request_dump(crash_context) {
                    Ok(_) => {
                        handled = true;
                        println!("Requested minidump.");
                    }
                    Err(e) => {
                        handled = false;
                        println!("Failed to report crash to handling process: {e}");
                    }
                }

                // Send additional crash data if possible over the 2nd connection
                if let Some(on_crash_callback) = &on_crash_callback {
                    for additional_info in on_crash_callback() {
                        let id = &additional_info.id;
                        println!("Got additional crash info '{id}'");

                        let buf = serde_json::to_string(&additional_info).unwrap();
                        match client2.send_message(
                            CrashHandlingMessageType::AdditionalCrashAttachment as usize as u32,
                            buf,
                        ) {
                            Ok(_) => {
                                println!("Sent additional crash info message");
                            }
                            Err(err) => {
                                println!("Failed to send additional crash info message: {}", err)
                            }
                        }
                    }
                }

                crash_handler::CrashEventResult::Handled(handled)
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
                    let guard = ping_clients_access
                        .as_ref()
                        .lock()
                        .expect("Unable to lock IPC clients");
                    let (client1, _) = &*guard;
                    match client1
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
                    let guard = ping_clients_access
                        .as_ref()
                        .lock()
                        .expect("Unable to lock IPC clients");
                    let (client1, client2) = &*guard;
                    // Timeout occurred, means no stop signal, so perform the task
                    match client1.ping() {
                        Ok(_) => (),
                        Err(e) => warn!("Failed to ping crash handling server. {}", e),
                    }
                    match client2.ping() {
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

pub fn init_crashhandling(
    is_server: bool,
    start_server_arg: &str,
    on_crash_callback: Option<CrashCallback>,
) {
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

    let handle = crashhandling_client(start_server_arg, on_crash_callback);
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

pub fn set_crash_json_extra(key: &str, value: JsonValue) {
    set_crash_json_partial(serde_json::json!({"extra": {key: value}}));
}
