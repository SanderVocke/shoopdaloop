use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;

use common::logging::macros::*;

shoop_log_unit!("TracyCapture");

static CAPTURE_PROCESS: Mutex<Option<Child>> = Mutex::new(None);

/// Start a tracy-capture child process.
///
/// # Arguments
/// * `tool` - Path to the capture tool (e.g., "tracy-capture"). If None, defaults to "tracy-capture".
/// * `args` - Arguments string for the capture tool. If None, defaults to `-o trace-{timestamp}.tracy`.
///
/// # Returns
/// `Ok(())` if the process was started successfully, `Err` otherwise.
pub fn start_tracy_capture(
    tool: Option<&str>,
    args: Option<&str>,
) -> Result<(), String> {
    let tool = tool.unwrap_or("tracy-capture");

    let args_str = match args {
        Some(a) => a.to_string(),
        None => {
            // Generate a timestamp-based filename
            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            format!("-o trace-{}.tracy", timestamp)
        }
    };

    info!(
        "Starting Tracy capture: {} {}",
        tool,
        args_str
    );

    let child = Command::new(tool)
        .args(shlex::split(&args_str).unwrap_or_else(|| vec![args_str.clone()]))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start Tracy capture tool '{}': {}", tool, e))?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(child);

    info!("Tracy capture process started (PID: {:?})", child.id());
    Ok(())
}

/// Stop the tracy-capture child process gracefully.
///
/// Sends SIGTERM and waits for the process to exit. If the process doesn't exit
/// within a reasonable time, it will be killed.
pub fn stop_tracy_capture() {
    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    if let Some(mut child) = guard.take() {
        info!("Stopping Tracy capture process (PID: {})...", child.id());

        // Try to terminate gracefully
        #[cfg(unix)]
        {
            // Send SIGTERM
            let _ = unsafe {
                let pid = child.id();
                libc::kill(pid as i32, libc::SIGTERM)
            };
        }

        // Wait for the process to exit
        match child.wait() {
            Ok(status) => {
                info!("Tracy capture process exited with status: {}", status);
            }
            Err(e) => {
                warn!("Error waiting for Tracy capture process: {}", e);
                // Force kill if wait fails
                let _ = child.kill();
            }
        }
    }
}
