// Note: mostly taken from the example included with the minidumper crate.

use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("CrashHandling");

use minidumper::{Client, Server};

pub struct CrashHandlerHandle {
    pub sender: mpsc::Sender<()>,
    pub handle: Option<thread::JoinHandle<()>>,
}

fn crashhandling_server() {
    info!("Starting crash handling server");

    let socket_name = std::env::args().last().expect("no socket name provided");

    debug!("Server: crash handling socket: {socket_name}");

    let mut server =
        Server::with_name(socket_name.as_str()).expect("failed to create crash handling server");

    let ab = std::sync::atomic::AtomicBool::new(false);

    struct Handler;

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
                }
                Err(e) => {
                    error!("Failed to write minidump: {:#}", e);
                }
            }

            // Tells the server to exit, which will in turn exit the process
            minidumper::LoopAction::Exit
        }

        fn on_message(&self, kind: u32, buffer: Vec<u8>) {
            info!(
                "Crash handling server: client sent msg of type {kind}, message: {}",
                String::from_utf8(buffer).unwrap()
            );
        }

        fn on_client_disconnected(&self, _num_clients: usize) -> minidumper::LoopAction {
            minidumper::LoopAction::Exit
        }
    }

    server
        .run(
            Box::new(Handler),
            &ab,
            Some(std::time::Duration::from_millis(1000)),
        )
        .expect("failed to run server");

    info!("Crash handling server finished.");

    std::process::exit(0);
}

pub fn crashhandling_client(start_server_arg: &str) -> CrashHandlerHandle {
    let (sender, receiver) = mpsc::channel();
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
                Ok(_) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
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
        handle: Some(handle),
    }
}

pub fn init_crashhandling(is_server: bool, start_server_arg: &str) -> Option<CrashHandlerHandle> {
    let maybe_dump_folder: Option<String> = std::env::var("SHOOP_CRASHDUMP_DIR").ok();
    if maybe_dump_folder.is_some() {
        debug!(
            "Using dump folder: {:?}",
            maybe_dump_folder.as_ref().unwrap()
        );
        let path = std::path::PathBuf::from(maybe_dump_folder.unwrap());
        if !path.exists() || !path.is_dir() {
            warn!("Dump folder {path:?} does not exist or is not a directory - crash handler not enabled.");
            return None;
        }
    } else {
        debug!("Dumps will go to OS-specific temp folder");
    }

    if is_server {
        crashhandling_server();

        // Unreachable
        return None;
    }

    return Some(crashhandling_client(start_server_arg));
}
