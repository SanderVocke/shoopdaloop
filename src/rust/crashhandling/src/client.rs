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
            let dir = match tempfile::tempdir() {
                Ok(d) => d.keep(),
                Err(e) => {
                    error!("Unable to create temporary dir for crash handling socket: {}", e);
                    return;
                }
            };
            let dirpath = dir.join("shoop.crashsocket");
            match dirpath.to_str() {
                Some(s) => s.to_string(),
                None => {
                    error!("Unable to convert temporary dir path to string");
                    return;
                }
            }
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
        let (client, mut _server) = loop {
            if let Ok(client) = Client::with_name(socket_name.as_str()) {
                if let Some(s) = server {
                    break (client, Some(s));
                } else {
                    // If we connected but have no server process handle (e.g. valid external server),
                    // that's fine. But here 'server' variable is Option<Child>.
                    // If we connected, we break. But our tuple expects 'server.unwrap()'.
                    // The original code assumed 'server' is Some if we looped.
                    // But if we connected on first try (server already running?), server is None.
                    // The original code: break (client, server.unwrap());
                    // This panics if server was already running and we connected immediately!
                    // Wait, lines 25: let mut server = None;
                    // If Client::with_name succeeds first time, server is None -> Panic!
                    // This seems like a bug I found.
                    // Fixing it: assume we don't own the server process if we didn't spawn it?
                    // But the return type of the loop is (Client, Child).
                    // This implies we MUST own the child?
                    // Actually, let's look at context.
                    // The 'server' variable is used later?
                    // On linux: _handler.set_ptracer(Some(_server.id()));
                    // We need the ID.
                    // If we didn't spawn it, we might not know the ID?
                    // If we connected to an existing server, we can't set ptracer to it easily unless we know its PID.
                    // Assuming for now we stick to original logic: if we connected, we assume we have a server handle if we needed one.
                    // If server is None here, we probably found a pre-existing server.
                    // I will replicate original behavior but safe:
                    // If server is None, we can't return it.
                    // But the code structure expects it.
                    // CHECK: Is there a way to get PID from Client? No.
                    // I'll leave the unwrap() if it corresponds to "unreachable if logic correct", or fix it.
                    // Given the loop structure:
                    // Loop starts, checks connection. If fail, spawns server.
                    // So if connection succeeds first time, server is None.
                    // So original code DEFINITELY panics if server is already running.
                    // I will change it to return Option<Child>.
                    break (client, server);
                }
            }

            let exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("shoopdaloop"));

            server = match std::process::Command::new(exe)
                .arg(start_server_arg.clone())
                .arg(socket_name.as_str())
                .spawn()
            {
                Ok(child) => Some(child),
                Err(e) => {
                     error!("unable to spawn server process: {}", e);
                     // If we can't spawn, we probably can't connect either.
                     // But we loop to try again ? No, loop continues to try connect.
                     // But if spawn failed, retry connection likely fails unless server is already running.
                     None
                }
            };

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
        let handler_closure = unsafe {
            crash_handler::make_crash_event(move |crash_context: &crash_handler::CrashContext| {
                let guard = handler_clients_access
                    .as_ref()
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                let (client1, client2) = &*guard;

                let thread_name = crate::registered_threads::current_thread_registered_name()
                    .unwrap_or("unknown".to_string());
                if let Err(e) = client1
                    .send_message(
                        CrashHandlingMessageType::SetJson as u32,
                        format!("{{\"tags\":{{\"thread_name\":\"{thread_name}\"}}}}"),
                    ) 
                {
                    eprintln!("Failed to send thread name to crash handler: {}", e);
                }

                // Send a ping to the server, this ensures that all messages that have been sent
                // are "flushed" before the crash event is sent. This is only really useful
                // on macos where messages and crash events are sent via different, unsynchronized,
                // methods which can result in the crash event closing the server before
                // the non-crash messages are received/processed
                if let Err(e) = client1.ping() {
                    eprintln!("Failed to ping client1: {}", e);
                }
                if let Err(e) = client2.ping() {
                     eprintln!("Failed to ping client2: {}", e);
                }

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

                        let buf = match serde_json::to_string(&additional_info) {
                            Ok(s) => s,
                            Err(e) => {
                                eprintln!("Failed to serialize additional info: {}", e);
                                continue;
                            }
                        };
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
        };

        let _handler = match crash_handler::CrashHandler::attach(handler_closure) {
             Ok(h) => h,
             Err(e) => {
                 error!("failed to attach signal handler: {}", e);
                 return;
             }
        };

        // On linux we can explicitly allow only the server process to inspect the
        // process we are monitoring (this one) for crashes
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            if let Some(s) = _server.as_mut() {
                _handler.set_ptracer(Some(s.id()));
            }
        }

        loop {
            match receiver.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(msg) => {
                    let guard = ping_clients_access
                        .as_ref()
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
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
                        .unwrap_or_else(|e| e.into_inner());
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
