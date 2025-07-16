use crate::types::*;
use minidumper::Client;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("CrashHandlingClient");

pub struct CrashHandlerHandle {
    pub sender: mpsc::Sender<CrashHandlingMessage>,
    pub _handle: Option<thread::JoinHandle<()>>,
}

pub type CrashCallback = fn() -> Vec<AdditionalCrashAttachment>;

pub fn crashhandling_client(
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

                let thread_name = crate::registered_threads::current_thread_registered_name()
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
