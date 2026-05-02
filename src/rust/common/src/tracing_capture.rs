use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use crate::logging::macros::*;

#[allow(unused_imports)]
shoop_log_unit!("Tracing.Capture");

/// Maximum time to wait for the capture child process and reader threads
/// to shut down gracefully before force-killing them.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

/// Configuration for the tracing capture process, stored so it can be reused
/// when restarting for a new trace file.
#[derive(Clone, Debug)]
struct CaptureConfig {
    tool: String,
    args: Option<String>,
}

struct CaptureState {
    child: Option<Child>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_thread: Option<JoinHandle<()>>,
}

static CAPTURE_PROCESS: Mutex<Option<CaptureState>> = Mutex::new(None);
static CAPTURE_CONFIG: Mutex<Option<CaptureConfig>> = Mutex::new(None);

/// Generate a timestamp-based output filename.
fn generate_trace_filename() -> String {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("trace-{}.tracy", timestamp)
}

/// Build the arguments string for the capture process.
fn build_args_str(config: &CaptureConfig, output_filename: &str) -> String {
    match &config.args {
        Some(args) => {
            // Replace {output} placeholder with the actual filename if present,
            // otherwise append the output argument.
            if args.contains("{output}") {
                args.replace("{output}", output_filename)
            } else {
                format!("{} -o {}", args, output_filename)
            }
        }
        None => format!("-o {}", output_filename),
    }
}

/// Spawn reader threads that forward child process output to the log.
fn spawn_output_readers(
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
) -> (JoinHandle<()>, JoinHandle<()>) {
    let stdout_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            match line {
                Ok(msg) if !msg.is_empty() => {
                    println!("{}", msg);
                }
                Ok(_) => {}
                Err(e) => {
                    println!("read error on stdout: {}", e);
                    break;
                }
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(msg) if !msg.is_empty() => {
                    println!("{}", msg);
                }
                Ok(_) => {}
                Err(e) => {
                    println!("read error on stderr: {}", e);
                    break;
                }
            }
        }
    });

    (stdout_thread, stderr_thread)
}

/// Spawn a capture child process with the given config and output filename.
fn spawn_capture_process(
    config: &CaptureConfig,
    output_filename: &str,
) -> Result<(Child, JoinHandle<()>, JoinHandle<()>), String> {
    let args_str = build_args_str(config, output_filename);

    println!(
        "Starting tracing capture: {} {}",
        config.tool,
        args_str
    );

    let mut child = Command::new(&config.tool)
        .args(shlex::split(&args_str).unwrap_or_else(|| vec![args_str.clone()]))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start tracing capture tool '{}': {}", config.tool, e))?;

    let stdout = child
        .stdout
        .take()
        .expect("stdout was not piped");
    let stderr = child
        .stderr
        .take()
        .expect("stderr was not piped");

    let (stdout_thread, stderr_thread) = spawn_output_readers(stdout, stderr);

    println!("Tracing capture process started (PID: {:?})", child.id());
    Ok((child, stdout_thread, stderr_thread))
}

/// Wait for a child process to exit, with a timeout.
///
/// If the process does not exit within the timeout, it is killed.
fn wait_child_with_timeout(child: &mut Child, timeout: Duration) {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                println!("Tracing capture process exited with status: {}", status);
                return;
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    println!(
                        "Tracing capture process (PID: {}) did not exit within {:?}, killing...",
                        child.id(),
                        timeout
                    );
                    let _ = child.kill();
                    // Best-effort final wait after kill
                    let _ = child.wait();
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                println!("Error waiting for tracing capture process: {}", e);
                return;
            }
        }
    }
}

/// Wait for a thread to finish, with a timeout.
///
/// If the thread does not finish within the timeout, this function returns
/// without joining. The thread continues running in the background.
fn join_thread_with_timeout(handle: JoinHandle<()>, timeout: Duration) {
    // We can't directly timeout a JoinHandle, so we poll via a helper thread.
    let join_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let join_done_clone = join_done.clone();

    let monitor = std::thread::spawn(move || {
        let _ = handle.join();
        join_done_clone.store(true, std::sync::atomic::Ordering::Relaxed);
    });

    let start = Instant::now();
    while !join_done.load(std::sync::atomic::Ordering::Relaxed) {
        if start.elapsed() >= timeout {
            println!("Reader thread did not finish within {:?}, detaching", timeout);
            // Forget the monitor thread handle - it will finish on its own
            std::mem::forget(monitor);
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Start a capture child process.
///
/// The provided configuration is stored internally so it can be reused when
/// calling [`restart_tracing_capture`].
///
/// # Arguments
/// * `tool` - Path to the capture tool (e.g., "tracy-capture"). If None, defaults to "tracy-capture".
/// * `args` - Arguments string for the capture tool. If None, defaults to `-o trace-{timestamp}.tracy`.
///            The string may contain `{output}` as a placeholder for the output filename.
///
/// # Returns
/// `Ok(())` if the process was started successfully, `Err` otherwise.
pub fn start_tracing_capture(
    tool: Option<&str>,
    args: Option<&str>,
) -> Result<(), String> {
    let config = CaptureConfig {
        tool: tool.unwrap_or("tracy-capture").to_string(),
        args: args.map(str::to_string),
    };

    // Store config for later restarts
    {
        let mut guard = CAPTURE_CONFIG.lock().unwrap();
        *guard = Some(config.clone());
    }

    let output_filename = generate_trace_filename();
    let (child, stdout_thread, stderr_thread) = spawn_capture_process(&config, &output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(CaptureState {
        child: Some(child),
        stdout_thread: Some(stdout_thread),
        stderr_thread: Some(stderr_thread),
    });

    Ok(())
}

/// Stop the capture child process gracefully.
///
/// Sends SIGINT and waits for the process to exit (up to 5 seconds).
/// If the process doesn't exit within the timeout, it is killed.
/// Reader threads are also given a timeout to finish; if they don't,
/// they are detached to prevent blocking.
pub fn stop_tracing_capture() {
    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    if let Some(state) = guard.take() {
        let pid = state.child.as_ref().map(|c| c.id()).unwrap_or(0);
        println!("Stopping tracing capture process (PID: {})...", pid);

        // Wait for the child to exit with timeout (reader threads will finish when pipes close)
        if let Some(mut child) = state.child {
            // Try to terminate gracefully
            #[cfg(unix)]
            {
                let _ = unsafe {
                    libc::kill(child.id() as i32, libc::SIGINT)
                };
            }

            wait_child_with_timeout(&mut child, STOP_TIMEOUT);
        }

        // Join the output reader threads with timeout so they finish draining
        // any buffered output. If they don't finish in time, they are detached.
        let thread_timeout = STOP_TIMEOUT;
        if let Some(handle) = state.stdout_thread {
            join_thread_with_timeout(handle, thread_timeout);
        }
        if let Some(handle) = state.stderr_thread {
            join_thread_with_timeout(handle, thread_timeout);
        }
    }
}

/// Restart the tracing capture process to write to a new trace file.
///
/// This stops the current capture process (ensuring the current trace file is
/// finalized) and starts a new one with a fresh timestamp-based filename.
///
/// Uses the configuration from the most recent [`start_tracing_capture`] call.
///
/// # Returns
/// `Ok(())` if the restart was successful, `Err` if no configuration is available
/// or if the new process failed to start.
pub fn restart_tracing_capture() -> Result<(), String> {
    // Get the stored config
    let config = {
        let guard = CAPTURE_CONFIG.lock().unwrap();
        guard
            .clone()
            .ok_or_else(|| "Tracing capture not configured. Call start_tracing_capture() first.".to_string())?
    };

    println!("Restarting tracing capture to a new trace file");

    // Stop the current capture process (this finalizes the current trace file)
    stop_tracing_capture();

    // Start a new capture process with a fresh filename
    let output_filename = generate_trace_filename();
    let (child, stdout_thread, stderr_thread) = spawn_capture_process(&config, &output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(CaptureState {
        child: Some(child),
        stdout_thread: Some(stdout_thread),
        stderr_thread: Some(stderr_thread),
    });

    Ok(())
}

/// Restart the tracing capture process to a trace file with a specific name.
///
/// This stops the current capture process and starts a new one writing to
/// the specified output filename.
///
/// Uses the configuration from the most recent [`start_tracing_capture`] call.
///
/// # Arguments
/// * `output_filename` - The filename for the new trace file (e.g., "testcase_foo.tracy").
///
/// # Returns
/// `Ok(())` if the restart was successful, `Err` if no configuration is available
/// or if the new process failed to start.
pub fn restart_tracing_capture_to(output_filename: &str) -> Result<(), String> {
    // Get the stored config
    let config = {
        let guard = CAPTURE_CONFIG.lock().unwrap();
        guard
            .clone()
            .ok_or_else(|| "Tracing capture not configured. Call start_tracing_capture() first.".to_string())?
    };

    println!("Restarting tracing capture to '{}'", output_filename);

    // Stop the current capture process (this finalizes the current trace file)
    stop_tracing_capture();

    // Start a new capture process with the specified filename
    let (child, stdout_thread, stderr_thread) = spawn_capture_process(&config, output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(CaptureState {
        child: Some(child),
        stdout_thread: Some(stdout_thread),
        stderr_thread: Some(stderr_thread),
    });

    Ok(())
}
