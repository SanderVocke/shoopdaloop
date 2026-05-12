use crate::types::*;
use minidumper::Client;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;

use common::logging::macros::*;
shoop_log_unit!("CrashHandlingClient");

#[cfg(unix)]
use std::sync::atomic::{AtomicI32, AtomicBool, Ordering};

/// Stores the server child PID so an atexit handler can reap it on process exit.
/// The client thread may be killed before it can call wait(), so this ensures
/// the zombie is always cleaned up.
#[cfg(unix)]
static SERVER_PID: AtomicI32 = AtomicI32::new(-1);

/// Ensures the atexit handler is registered only once.
#[cfg(unix)]
static ATEXIT_REGISTERED: AtomicBool = AtomicBool::new(false);

/// atexit handler: kills and reaps the server child process on normal exit.
/// This is needed because the CrashHandlerHandle is stored in a OnceLock
/// static that is never dropped, so the client thread's cleanup code never
/// runs. The atexit handler ensures the server zombie is reaped.
#[cfg(unix)]
extern "C" fn reap_server_atexit() {
    let pid = SERVER_PID.load(Ordering::SeqCst);
    if pid <= 0 {
        eprintln!("[CRASH_DBG] atexit: no server child to reap");
        return;
    }
    eprintln!("[CRASH_DBG] atexit: cleaning up server child pid={pid}");

    // Send SIGKILL first to ensure the server terminates promptly.
    // It may have already exited (clients disconnected), but kill()
    // on a dead process is a no-op.
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }

    // Reap with retries. The server should exit very quickly after SIGKILL.
    for attempt in 0..50 {
        match unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) } {
            r if r == pid => {
                eprintln!("[CRASH_DBG] atexit: reaped server child pid={pid} (attempt {attempt})");
                SERVER_PID.store(-1, Ordering::SeqCst);
                return;
            }
            -1 => {
                // waitpid failed — child may have already been reaped by the thread's try_wait()
                eprintln!("[CRASH_DBG] atexit: waitpid failed for pid={pid} (already reaped or error)");
                SERVER_PID.store(-1, Ordering::SeqCst);
                return;
            }
            _ => {
                // Not yet exited, wait 20ms and retry
                let ts = libc::timespec { tv_sec: 0, tv_nsec: 20_000_000 };
                unsafe { libc::nanosleep(&ts, std::ptr::null_mut()); }
            }
        }
    }
    eprintln!("[CRASH_DBG] atexit: failed to reap server child pid={pid} after 1s");
}

pub struct CrashHandlerHandle {
    pub sender: mpsc::Sender<CrashHandlingMessage>,
    pub _handle: Option<thread::JoinHandle<()>>,
}

impl Drop for CrashHandlerHandle {
    fn drop(&mut self) {
        eprintln!("[CRASH_DBG] CrashHandlerHandle::drop() called — sender will be dropped, channel will disconnect");
    }
}

pub type CrashCallback = fn() -> Vec<AdditionalCrashAttachment>;

pub fn crashhandling_client(
    start_server_arg: &str,
    on_crash_callback: Option<CrashCallback>,
) -> CrashHandlerHandle {
    let (sender, receiver) = mpsc::channel::<CrashHandlingMessage>();
    let start_server_arg = start_server_arg.to_string();

    let handle = thread::spawn(move || {
        #[cfg(unix)]
        eprintln!("[CRASH_DBG] Client thread started: pid={}, ppid={}", std::process::id(), unsafe { libc::getppid() });
        #[cfg(not(unix))]
        eprintln!("[CRASH_DBG] Client thread started: pid={}", std::process::id());
        let mut server: Option<std::process::Child> = None;

        // Determine which socket to open
        let socket_name: String = if cfg!(target_os = "linux") {
            format!("shoopdaloop-server-{}", uuid::Uuid::new_v4())
        } else {
            let dir = match tempfile::tempdir() {
                Ok(d) => d.keep(),
                Err(e) => {
                    error!(
                        "Unable to create temporary dir for crash handling socket: {}",
                        e
                    );
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
                eprintln!("[CRASH_DBG] Connected to crash server on socket {socket_name}");
                if let Some(s) = server {
                    eprintln!("[CRASH_DBG] Server child pid={}", s.id());
                    break (client, Some(s));
                } else {
                    // Connected to a pre-existing server (not spawned by us).
                    eprintln!("[CRASH_DBG] Connected to external server (no child handle)");
                    break (client, server);
                }
            }

            let exe =
                std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("shoopdaloop"));

            #[cfg(any(target_os = "linux", target_os = "android"))]
            {
                use std::os::unix::process::CommandExt;
                server = match std::process::Command::new(&exe)
                    .arg(start_server_arg.as_str())
                    .arg(socket_name.as_str())
                    // Set PR_SET_PDEATHSIG so the server is killed if the parent dies
                    // (e.g. parent crashes). This prevents orphan server processes.
                    .pre_exec(|| {
                        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0); }
                        Ok(())
                    })
                    .spawn()
                {
                    Ok(child) => {
                        let pid = child.id();
                        eprintln!("[CRASH_DBG] Spawned server child: pid={}, cmd={:?} {:?}", pid, exe, [start_server_arg.as_str(), socket_name.as_str()]);

                        // Store the PID for the atexit handler to reap on process exit.
                        SERVER_PID.store(pid as i32, Ordering::SeqCst);
                        // Register atexit handler (once) to ensure the server child
                        // is reaped even though the OnceLock prevents Drop from running.
                        if !ATEXIT_REGISTERED.swap(true, Ordering::SeqCst) {
                            eprintln!("[CRASH_DBG] Registering atexit handler to reap server child");
                            unsafe { libc::atexit(reap_server_atexit); }
                        }

                        Some(child)
                    },
                    Err(e) => {
                        error!("unable to spawn server process: {}", e);
                        eprintln!("[CRASH_DBG] Failed to spawn server child: {}", e);
                        None
                    }
                };
            }

            #[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
            {
                server = match std::process::Command::new(&exe)
                    .arg(start_server_arg.as_str())
                    .arg(socket_name.as_str())
                    .spawn()
                {
                    Ok(child) => {
                        let pid = child.id();
                        eprintln!("[CRASH_DBG] Spawned server child: pid={}, cmd={:?} {:?}", pid, exe, [start_server_arg.as_str(), socket_name.as_str()]);
                        // Store PID for atexit handler (macOS etc.)
                        SERVER_PID.store(pid as i32, Ordering::SeqCst);
                        if !ATEXIT_REGISTERED.swap(true, Ordering::SeqCst) {
                            eprintln!("[CRASH_DBG] Registering atexit handler to reap server child");
                            unsafe { libc::atexit(reap_server_atexit); }
                        }
                        Some(child)
                    },
                    Err(e) => {
                        error!("unable to spawn server process: {}", e);
                        eprintln!("[CRASH_DBG] Failed to spawn server child: {}", e);
                        None
                    }
                };
            }

            #[cfg(not(unix))]
            {
                server = match std::process::Command::new(&exe)
                    .arg(start_server_arg.as_str())
                    .arg(socket_name.as_str())
                    .spawn()
                {
                    Ok(child) => {
                        eprintln!("[CRASH_DBG] Spawned server child: pid={}, cmd={:?} {:?}", child.id(), exe, [start_server_arg.as_str(), socket_name.as_str()]);
                        Some(child)
                    },
                    Err(e) => {
                        error!("unable to spawn server process: {}", e);
                        eprintln!("[CRASH_DBG] Failed to spawn server child: {}", e);
                        None
                    }
                };
            }

            // Give it time to start
            std::thread::sleep(Duration::from_millis(100));
        };
        // Attempt to connect to the server (client 2)
        eprintln!("[CRASH_DBG] Connecting second client to server...");
        let client2 = loop {
            if let Ok(client) = Client::with_name(socket_name.as_str()) {
                eprintln!("[CRASH_DBG] Second client connected");
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
                if let Err(e) = client1.send_message(
                    CrashHandlingMessageType::SetJson as u32,
                    format!("{{\"tags\":{{\"thread_name\":\"{thread_name}\"}}}}"),
                ) {
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
                eprintln!("[CRASH_DBG] Setting ptracer to server pid={}", s.id());
                _handler.set_ptracer(Some(s.id()));
            } else {
                eprintln!("[CRASH_DBG] No server child handle, cannot set ptracer");
            }
        }

        eprintln!("[CRASH_DBG] Crash handling client fully initialized, entering message loop."
            );

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
                    eprintln!("[CRASH_DBG] Client thread: sender disconnected, stopping loop.");
                    if let Some(s) = _server.as_mut() {
                        match s.try_wait() {
                            Ok(Some(status)) => eprintln!("[CRASH_DBG] Server child already exited with status: {}", status),
                            Ok(None) => eprintln!("[CRASH_DBG] Server child is still running (pid={})", s.id()),
                            Err(e) => eprintln!("[CRASH_DBG] Cannot check server child status: {}", e),
                        }
                    } else {
                        eprintln!("[CRASH_DBG] No server child handle (external server)");
                    }
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

                    // Periodically check if the server child has exited and reap it.
                    // This prevents zombies when the server exits for any reason
                    // (crash, error, etc.) while the parent is still alive.
                    // Also clears the PID from the atexit global so the atexit
                    // handler doesn't try to reap an already-reaped child.
                    if let Some(s) = _server.as_mut() {
                        match s.try_wait() {
                            Ok(Some(status)) => {
                                eprintln!("[CRASH_DBG] Server child exited with status: {}, reaped in timeout loop", status);
                                // Child has been reaped by try_wait; remove it
                                // so we don't try to wait again later.
                                #[cfg(unix)]
                                SERVER_PID.store(-1, Ordering::SeqCst);
                                _server = None;
                                // The server is gone, crash handling is no longer functional.
                                // We could break here, but let's continue the loop — the ping
                                // failures will be logged.
                            }
                            Ok(None) => {
                                // Server still running, this is normal
                            }
                            Err(e) => {
                                eprintln!("[CRASH_DBG] try_wait error in timeout loop: {e}");
                                #[cfg(unix)]
                                SERVER_PID.store(-1, Ordering::SeqCst);
                                _server = None;
                            }
                        }
                    }
                }
            }
        }

        // Thread is exiting — reap the server child process to prevent zombies.
        // This code is reached when the sender is dropped (Disconnected received).
        // This code is NOT reached on normal process exit (the OnceLock prevents Drop).
        eprintln!("[CRASH_DBG] Client thread exiting, reaping server child if any.");
        if let Some(mut server) = _server {
            eprintln!("[CRASH_DBG] Have server child handle (pid={}), attempting kill+wait...", server.id());
            match server.try_wait() {
                Ok(Some(status)) => {
                    eprintln!("[CRASH_DBG] Server child already exited with status: {}, no kill needed", status);
                }
                Ok(None) => {
                    eprintln!("[CRASH_DBG] Server child still running, sending SIGKILL...");
                    match server.kill() {
                        Ok(()) => eprintln!("[CRASH_DBG] SIGKILL sent successfully"),
                        Err(e) => eprintln!("[CRASH_DBG] Failed to send SIGKILL: {}", e),
                    }
                }
                Err(e) => {
                    eprintln!("[CRASH_DBG] try_wait error (child already reaped?): {}", e);
                }
            }
            match server.wait() {
                Ok(status) => eprintln!("[CRASH_DBG] Server child reaped successfully, exit status: {}", status),
                Err(e) => eprintln!("[CRASH_DBG] Failed to wait on server child: {}", e),
            }
            #[cfg(unix)]
            SERVER_PID.store(-1, Ordering::SeqCst);
        } else {
            eprintln!("[CRASH_DBG] No server child handle (external server), nothing to reap.");
        }
        eprintln!("[CRASH_DBG] Client thread done.");
    });

    CrashHandlerHandle {
        sender: sender,
        _handle: Some(handle),
    }
}