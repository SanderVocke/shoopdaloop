use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::SystemTime;

use common::logging::macros::*;

shoop_log_unit!("TracingCapture");

/// Configuration for the tracing capture process, stored so it can be reused
/// when restarting for a new trace file.
#[derive(Clone, Debug)]
struct CaptureConfig {
    tool: String,
    args: Option<String>,
}

static CAPTURE_PROCESS: Mutex<Option<Child>> = Mutex::new(None);
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

/// Spawn a capture child process with the given config and output filename.
fn spawn_capture_process(config: &CaptureConfig, output_filename: &str) -> Result<Child, String> {
    let args_str = build_args_str(config, output_filename);

    info!(
        "Starting tracing capture: {} {}",
        config.tool,
        args_str
    );

    let child = Command::new(&config.tool)
        .args(shlex::split(&args_str).unwrap_or_else(|| vec![args_str.clone()]))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("Failed to start tracing capture tool '{}': {}", config.tool, e))?;

    info!("Tracing capture process started (PID: {:?})", child.id());
    Ok(child)
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
    let child = spawn_capture_process(&config, &output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(child);

    Ok(())
}

/// Stop the capture child process gracefully.
///
/// Sends SIGTERM and waits for the process to exit. If the process doesn't exit
/// within a reasonable time, it will be killed.
pub fn stop_tracing_capture() {
    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    if let Some(mut child) = guard.take() {
        info!("Stopping tracing capture process (PID: {})...", child.id());

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
                info!("Tracing capture process exited with status: {}", status);
            }
            Err(e) => {
                warn!("Error waiting for tracing capture process: {}", e);
                // Force kill if wait fails
                let _ = child.kill();
            }
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

    info!("Restarting tracing capture to a new trace file");

    // Stop the current capture process (this finalizes the current trace file)
    stop_tracing_capture();

    // Start a new capture process with a fresh filename
    let output_filename = generate_trace_filename();
    let child = spawn_capture_process(&config, &output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(child);

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

    info!("Restarting tracing capture to '{}'", output_filename);

    // Stop the current capture process (this finalizes the current trace file)
    stop_tracing_capture();

    // Start a new capture process with the specified filename
    let child = spawn_capture_process(&config, output_filename)?;

    let mut guard = CAPTURE_PROCESS.lock().unwrap();
    *guard = Some(child);

    Ok(())
}
