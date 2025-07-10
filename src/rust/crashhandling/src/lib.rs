// Note: mostly taken from the example included with the minidumper crate.

use common::logging::macros::*;
shoop_log_unit!("CrashHandling");

const SOCKET_NAME: &str = "shoopdaloop-crashhandling";

use minidumper::{Client, Server};

pub fn init_crashhandling(
    is_server: bool,
    start_server_arg: &str,
) -> Option<crash_handler::CrashHandler> {
    if is_server {
        info!("Server - Starting crash handling server");
        let mut server =
            Server::with_name(SOCKET_NAME).expect("failed to create crash handling server");

        let ab = std::sync::atomic::AtomicBool::new(false);

        struct Handler;

        impl minidumper::ServerHandler for Handler {
            /// Called when a crash has been received and a backing file needs to be
            /// created to store it.
            fn create_minidump_file(
                &self,
            ) -> Result<(std::fs::File, std::path::PathBuf), std::io::Error> {
                let uuid = uuid::Uuid::new_v4();

                let dump_path = std::path::PathBuf::from(format!("crashdumps/{uuid}.dmp"));

                if let Some(dir) = dump_path.parent() {
                    if !dir.try_exists()? {
                        std::fs::create_dir_all(dir)?;
                    }
                }
                let file = std::fs::File::create(&dump_path)?;
                Ok((file, dump_path))
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
                        info!("Server - Wrote minidump to disk");
                    }
                    Err(e) => {
                        error!("Server - Failed to write minidump: {:#}", e);
                    }
                }

                // Tells the server to exit, which will in turn exit the process
                minidumper::LoopAction::Exit
            }

            fn on_message(&self, kind: u32, buffer: Vec<u8>) {
                info!(
                    "Server - Crash handling message of kind {kind}, message: {}",
                    String::from_utf8(buffer).unwrap()
                );
            }
        }

        server
            .run(Box::new(Handler), &ab, None)
            .expect("failed to run server");

        info!("Crash handling server finished.");

        std::process::exit(0);
    }

    let mut server = None;

    // Attempt to connect to the server
    let (client, _server) = loop {
        if let Ok(client) = Client::with_name(SOCKET_NAME) {
            break (client, server.unwrap());
        }

        let exe = std::env::current_exe().expect("Unable to find own .exe");

        server = Some(
            std::process::Command::new(exe)
                .arg(start_server_arg)
                .spawn()
                .expect("unable to spawn server process"),
        );

        // Give it time to start
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    // Register our exception handler
    client.send_message(1, "Test message").unwrap();

    #[allow(unsafe_code)]
    let _handler = crash_handler::CrashHandler::attach(unsafe {
        crash_handler::make_crash_event(move |crash_context: &crash_handler::CrashContext| {
            println!("CRASH");

            // Before we request the crash, send a message to the server
            client.send_message(2, "Client - Detected a crash").unwrap();

            // Send a ping to the server, this ensures that all messages that have been sent
            // are "flushed" before the crash event is sent. This is only really useful
            // on macos where messages and crash events are sent via different, unsynchronized,
            // methods which can result in the crash event closing the server before
            // the non-crash messages are received/processed
            client.ping().unwrap();

            crash_handler::CrashEventResult::Handled(client.request_dump(crash_context).is_ok())
        })
    })
    .expect("failed to attach signal handler");

    // On linux we can explicitly allow only the server process to inspect the
    // process we are monitoring (this one) for crashes
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        _handler.set_ptracer(Some(_server.id()));
    }

    Some(_handler)
}
