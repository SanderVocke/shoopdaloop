use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lazy_static::lazy_static;
use thiserror::Error;

use crate::logging::macros::*;
shoop_log_unit!("Tracing.Capture");

const POLL_INTERVAL: Duration = Duration::from_millis(25);
const TRACING_QUIESCE_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Debug)]
pub struct CaptureConfig {
    pub tool: PathBuf,
    pub output_dir: PathBuf,
    pub connect_timeout: Duration,
    pub stop_timeout: Duration,
}

impl CaptureConfig {
    pub fn new(tool: PathBuf, output_dir: PathBuf) -> Self {
        Self {
            tool,
            output_dir,
            connect_timeout: Duration::from_secs(5),
            stop_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug)]
pub struct CapturedTrace {
    pub source_label: String,
    pub path: PathBuf,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("tracing capture is not configured")]
    NotConfigured,
    #[error("tracing capture state lock is poisoned")]
    LockPoisoned,
    #[error("capture tool '{0}' was not found or is not executable")]
    ToolNotExecutable(PathBuf),
    #[error("unable to find tracing capture tool '{0}' on PATH")]
    ToolNotFound(String),
    #[error("failed to {action} '{path}': {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to start tracing capture tool '{tool}': {source}")]
    Spawn {
        tool: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("tracing capture process exited before connecting: {0}")]
    ExitedBeforeConnection(ExitStatus),
    #[error("tracing capture did not connect within {0:?}")]
    ConnectTimeout(Duration),
    #[error("Tracy client remained connected after capture stopped for {0:?}")]
    DisconnectTimeout(Duration),
    #[error("failed to signal tracing capture process {pid}: {source}")]
    Signal {
        pid: u32,
        #[source]
        source: std::io::Error,
    },
    #[error("tracing capture process {pid} did not stop within {timeout:?} and was killed")]
    StopTimeout { pid: u32, timeout: Duration },
    #[error("tracing capture process exited unsuccessfully: {0}")]
    UnsuccessfulExit(ExitStatus),
    #[error("capture output '{0}' is missing or empty")]
    MissingOutput(PathBuf),
    #[error("graceful Tracy capture shutdown is unsupported on this platform")]
    UnsupportedPlatform,
}

struct ActiveCapture {
    child: Child,
    source_label: String,
    path: PathBuf,
    started_at: SystemTime,
}

#[derive(Default)]
struct CaptureController {
    config: Option<CaptureConfig>,
    active: Option<ActiveCapture>,
    sequence: u64,
}

lazy_static! {
    static ref CAPTURE_CONTROLLER: Mutex<CaptureController> =
        Mutex::new(CaptureController::default());
}

fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> CaptureError {
    CaptureError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn is_executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn executable_names(name: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let candidate = Path::new(name);
        if candidate.extension().is_some() {
            return vec![name.to_string()];
        }
        let extensions = env::var_os("PATHEXT")
            .unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into())
            .to_string_lossy()
            .split(';')
            .filter(|extension| !extension.is_empty())
            .map(|extension| format!("{name}{extension}"))
            .collect::<Vec<_>>();
        return extensions;
    }

    #[cfg(not(windows))]
    {
        vec![name.to_string()]
    }
}

pub fn resolve_capture_tool(tool: Option<&Path>) -> Result<PathBuf, CaptureError> {
    let candidate = tool
        .map(Path::to_path_buf)
        .or_else(|| env::var_os("TRACY_CAPTURE_TOOL").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("tracy-capture"));

    if candidate.is_absolute() || candidate.components().count() > 1 {
        if is_executable(&candidate) {
            return candidate
                .canonicalize()
                .map_err(|error| io_error("canonicalize capture tool", &candidate, error));
        }
        return Err(CaptureError::ToolNotExecutable(candidate));
    }

    let name = candidate.to_string_lossy().to_string();
    if let Some(paths) = env::var_os("PATH") {
        for directory in env::split_paths(&paths) {
            for executable_name in executable_names(&name) {
                let path = directory.join(executable_name);
                if is_executable(&path) {
                    return path
                        .canonicalize()
                        .map_err(|error| io_error("canonicalize capture tool", &path, error));
                }
            }
        }
    }

    Err(CaptureError::ToolNotFound(name))
}

pub fn configure(mut config: CaptureConfig) -> Result<(), CaptureError> {
    config.tool = resolve_capture_tool(Some(&config.tool))?;
    std::fs::create_dir_all(&config.output_dir)
        .map_err(|error| io_error("create capture output directory", &config.output_dir, error))?;
    config.output_dir = config.output_dir.canonicalize().map_err(|error| {
        io_error(
            "canonicalize capture output directory",
            &config.output_dir,
            error,
        )
    })?;

    let mut controller = CAPTURE_CONTROLLER
        .lock()
        .map_err(|_| CaptureError::LockPoisoned)?;
    controller.stop_active(None)?;
    controller.config = Some(config.clone());
    controller.sequence = 0;
    initialize_output_files(&config.output_dir)?;
    info!(
        "Configured Tracy capture tool {} with output directory {}",
        config.tool.display(),
        config.output_dir.display()
    );
    Ok(())
}

pub fn is_configured() -> bool {
    CAPTURE_CONTROLLER
        .lock()
        .map(|controller| controller.config.is_some())
        .unwrap_or(false)
}

pub fn start_named_capture(label: &str) -> Result<PathBuf, CaptureError> {
    let mut controller = CAPTURE_CONTROLLER
        .lock()
        .map_err(|_| CaptureError::LockPoisoned)?;
    controller.start(label)
}

pub fn start_default_capture() -> Result<PathBuf, CaptureError> {
    start_named_capture("application")
}

pub fn stop_capture() -> Result<Option<CapturedTrace>, CaptureError> {
    stop_capture_with_outcome(None)
}

pub fn stop_capture_with_outcome(
    test_outcome: Option<&str>,
) -> Result<Option<CapturedTrace>, CaptureError> {
    CAPTURE_CONTROLLER
        .lock()
        .map_err(|_| CaptureError::LockPoisoned)?
        .stop_active(test_outcome)
}

pub fn shutdown() -> Result<(), CaptureError> {
    let mut controller = CAPTURE_CONTROLLER
        .lock()
        .map_err(|_| CaptureError::LockPoisoned)?;
    let result = controller.stop_active(None).map(|_| ());
    controller.config = None;
    crate::tracing_helpers::set_tracing_output_enabled(true);
    result
}

impl CaptureController {
    fn next_capture_path(&mut self, output_dir: &Path, label: &str) -> PathBuf {
        let sanitized = sanitize_label(label);
        loop {
            self.sequence += 1;
            let path = output_dir.join(format!("{:04}-{sanitized}.tracy", self.sequence));
            if !path.exists() {
                return path;
            }
        }
    }

    fn start(&mut self, label: &str) -> Result<PathBuf, CaptureError> {
        self.stop_active(None)?;
        let config = self.config.clone().ok_or(CaptureError::NotConfigured)?;
        crate::tracing_helpers::set_tracing_output_enabled(false);
        wait_for_connection_state(false, config.connect_timeout)?;

        let path = self.next_capture_path(&config.output_dir, label);

        let log_path = config.output_dir.join("tracy-capture.log");
        let stdout = open_capture_log(&log_path)?;
        let stderr = stdout
            .try_clone()
            .map_err(|error| io_error("clone capture log handle", &log_path, error))?;

        info!(
            "Starting Tracy capture for {label:?}: {} -o {}",
            config.tool.display(),
            path.display()
        );
        let mut command = Command::new(&config.tool);
        command
            .arg("-o")
            .arg(&path)
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr));
        #[cfg(windows)]
        {
            // Give tracy-capture a private console so a helper can send Ctrl+C without
            // interrupting ShoopDaLoop or the CI shell sharing its original console.
            use std::os::windows::process::CommandExt;
            const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
            command.creation_flags(CREATE_NEW_CONSOLE);
        }
        let child = command.spawn().map_err(|source| CaptureError::Spawn {
            tool: config.tool.clone(),
            source,
        })?;

        self.active = Some(ActiveCapture {
            child,
            source_label: label.to_string(),
            path: path.clone(),
            started_at: SystemTime::now(),
        });

        let start = Instant::now();
        loop {
            if tracy_client::Client::is_connected() {
                crate::tracing_helpers::set_tracing_output_enabled(true);
                info!("Tracy capture connected for {label:?}");
                return Ok(path);
            }

            let status = match self
                .active
                .as_mut()
                .expect("active capture was just assigned")
                .child
                .try_wait()
            {
                Ok(status) => status,
                Err(error) => {
                    if let Some(mut active) = self.active.take() {
                        force_kill_and_reap(&mut active.child);
                    }
                    return Err(io_error("query capture child status", &path, error));
                }
            };
            if let Some(status) = status {
                self.active = None;
                return Err(CaptureError::ExitedBeforeConnection(status));
            }

            if start.elapsed() >= config.connect_timeout {
                let _ = self.stop_active(None);
                return Err(CaptureError::ConnectTimeout(config.connect_timeout));
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn stop_active(
        &mut self,
        test_outcome: Option<&str>,
    ) -> Result<Option<CapturedTrace>, CaptureError> {
        let Some(mut active) = self.active.take() else {
            return Ok(None);
        };
        let config = self.config.clone().ok_or(CaptureError::NotConfigured)?;
        let pid = active.child.id();
        info!("Stopping Tracy capture process {pid}");
        // Prevent zones from spanning a profiler disconnect/reconnect. Tracy
        // rejects the next capture if a zone opened for the old connection is
        // closed after the new capture connects.
        crate::tracing_helpers::set_tracing_output_enabled(false);
        thread::sleep(TRACING_QUIESCE_INTERVAL);

        let mut status = match active.child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                force_kill_and_reap(&mut active.child);
                return Err(io_error("query capture child status", &active.path, error));
            }
        };
        if status.is_none() {
            if let Err(error) = signal_capture_process(&mut active.child) {
                force_kill_and_reap(&mut active.child);
                return Err(error);
            }
            let start = Instant::now();
            while start.elapsed() < config.stop_timeout {
                status = match active.child.try_wait() {
                    Ok(status) => status,
                    Err(error) => {
                        force_kill_and_reap(&mut active.child);
                        return Err(io_error("wait for capture child", &active.path, error));
                    }
                };
                if status.is_some() {
                    break;
                }
                thread::sleep(POLL_INTERVAL);
            }
        }

        if status.is_none() {
            force_kill_and_reap(&mut active.child);
            return Err(CaptureError::StopTimeout {
                pid,
                timeout: config.stop_timeout,
            });
        }

        wait_for_connection_state(false, config.stop_timeout)?;
        let status = status.expect("capture status checked above");
        if !status.success() {
            return Err(CaptureError::UnsuccessfulExit(status));
        }

        let metadata = active
            .path
            .metadata()
            .map_err(|_| CaptureError::MissingOutput(active.path.clone()))?;
        if metadata.len() == 0 {
            return Err(CaptureError::MissingOutput(active.path.clone()));
        }

        append_manifest(
            &config.output_dir,
            self.sequence,
            &active.source_label,
            &active.path,
            active.started_at,
            SystemTime::now(),
            status,
            test_outcome,
        )?;
        info!("Finalized Tracy capture {}", active.path.display());
        Ok(Some(CapturedTrace {
            source_label: active.source_label,
            path: active.path,
        }))
    }
}

fn initialize_output_files(output_dir: &Path) -> Result<(), CaptureError> {
    let manifest_path = output_dir.join("manifest.tsv");
    if !manifest_path.exists() {
        let mut manifest = File::create(&manifest_path)
            .map_err(|error| io_error("create capture manifest", &manifest_path, error))?;
        writeln!(
            manifest,
            "sequence\tsource_qml\tcapture_file\tstarted_unix_ms\tended_unix_ms\tstatus\ttest_outcome"
        )
        .map_err(|error| io_error("write capture manifest", &manifest_path, error))?;
    }
    let log_path = output_dir.join("tracy-capture.log");
    open_capture_log(&log_path).map(|_| ())
}

fn open_capture_log(path: &Path) -> Result<File, CaptureError> {
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| io_error("open capture log", path, error))
}

fn append_manifest(
    output_dir: &Path,
    sequence: u64,
    source_label: &str,
    capture_path: &Path,
    started_at: SystemTime,
    ended_at: SystemTime,
    status: ExitStatus,
    test_outcome: Option<&str>,
) -> Result<(), CaptureError> {
    let manifest_path = output_dir.join("manifest.tsv");
    let mut manifest = OpenOptions::new()
        .append(true)
        .open(&manifest_path)
        .map_err(|error| io_error("open capture manifest", &manifest_path, error))?;
    let capture_file = capture_path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    writeln!(
        manifest,
        "{}\t{}\t{}\t{}\t{}\t{}\t{}",
        sequence,
        escape_tsv(source_label),
        escape_tsv(&capture_file),
        unix_millis(started_at),
        unix_millis(ended_at),
        escape_tsv(&status.to_string()),
        escape_tsv(test_outcome.unwrap_or("not_applicable"))
    )
    .map_err(|error| io_error("append capture manifest", &manifest_path, error))
}

fn unix_millis(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn escape_tsv(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '\t' | '\n' | '\r' => ' ',
            other => other,
        })
        .collect()
}

fn sanitize_label(label: &str) -> String {
    let stem = Path::new(label)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    let sanitized: String = stem
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() || matches!(sanitized.as_str(), "." | "..") {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn wait_for_connection_state(connected: bool, timeout: Duration) -> Result<(), CaptureError> {
    let start = Instant::now();
    while tracy_client::Client::is_connected() != connected {
        if start.elapsed() >= timeout {
            return if connected {
                Err(CaptureError::ConnectTimeout(timeout))
            } else {
                Err(CaptureError::DisconnectTimeout(timeout))
            };
        }
        thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

fn force_kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn signal_capture_process(child: &mut Child) -> Result<(), CaptureError> {
    let pid = child.id();
    let result = unsafe { libc::kill(pid as i32, libc::SIGINT) };
    if result == 0 {
        Ok(())
    } else {
        Err(CaptureError::Signal {
            pid,
            source: std::io::Error::last_os_error(),
        })
    }
}

#[cfg(windows)]
fn signal_capture_process(child: &mut Child) -> Result<(), CaptureError> {
    let pid = child.id();
    // tracy-capture installs a SIGINT handler on Windows and saves after Ctrl+C.
    // A short-lived PowerShell helper attaches to the private child console, ignores
    // the event itself, and emits Ctrl+C only within that console. Keeping this in a
    // helper avoids detaching the GUI/test process from the CI runner's console.
    let script = format!(
        r#"
$native = @'
using System;
using System.Runtime.InteropServices;
public static class ShoopConsoleSignal {{
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool FreeConsole();
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool AttachConsole(uint processId);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool SetConsoleCtrlHandler(IntPtr handler, bool add);
    [DllImport("kernel32.dll", SetLastError = true)]
    public static extern bool GenerateConsoleCtrlEvent(uint ctrlEvent, uint processGroupId);
}}
'@
Add-Type -TypeDefinition $native
[ShoopConsoleSignal]::FreeConsole() | Out-Null
if (-not [ShoopConsoleSignal]::AttachConsole({pid})) {{ exit 10 }}
if (-not [ShoopConsoleSignal]::SetConsoleCtrlHandler([IntPtr]::Zero, $true)) {{ exit 11 }}
if (-not [ShoopConsoleSignal]::GenerateConsoleCtrlEvent(0, 0)) {{ exit 12 }}
Start-Sleep -Milliseconds 100
"#
    );
    let status = Command::new("powershell.exe")
        .args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command"])
        .arg(script)
        .status()
        .map_err(|source| CaptureError::Signal { pid, source })?;
    if status.success() {
        Ok(())
    } else {
        Err(CaptureError::Signal {
            pid,
            source: std::io::Error::other(format!("Windows Ctrl+C helper exited with {status}")),
        })
    }
}

#[cfg(all(not(unix), not(windows)))]
fn signal_capture_process(_child: &mut Child) -> Result<(), CaptureError> {
    Err(CaptureError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_capture_labels() {
        assert_eq!(sanitize_label("/tmp/tst_Two Loops.qml"), "tst_Two_Loops");
        assert_eq!(sanitize_label("../../evil.qml"), "evil");
        assert_eq!(sanitize_label("💥.qml"), "_");
        assert_eq!(sanitize_label(".."), "unknown");
        assert_eq!(sanitize_label(""), "unknown");
    }

    #[test]
    fn escapes_manifest_fields() {
        assert_eq!(escape_tsv("a\tb\nc\r"), "a b c ");
    }

    #[test]
    fn missing_tool_is_rejected() {
        let result = resolve_capture_tool(Some(Path::new("/definitely/not/a/real/tracy-capture")));
        assert!(matches!(result, Err(CaptureError::ToolNotExecutable(_))));
    }

    #[test]
    fn starting_without_configuration_is_rejected() {
        let mut controller = CaptureController::default();
        assert!(matches!(
            controller.start("test.qml"),
            Err(CaptureError::NotConfigured)
        ));
    }

    #[test]
    fn capture_paths_are_unique_and_confined_to_output_directory() {
        let temporary_dir = tempfile::tempdir().expect("create temporary directory");
        let mut controller = CaptureController::default();
        let first = controller.next_capture_path(temporary_dir.path(), "../../unsafe name.qml");
        assert_eq!(first, temporary_dir.path().join("0001-unsafe_name.tracy"));
        std::fs::write(&first, b"existing trace").expect("reserve first trace name");
        let second = controller.next_capture_path(temporary_dir.path(), "../../unsafe name.qml");
        assert_eq!(second, temporary_dir.path().join("0002-unsafe_name.tracy"));
        assert_eq!(second.parent(), Some(temporary_dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn connection_timeout_kills_and_reaps_capture_process() {
        use std::os::unix::fs::PermissionsExt;

        let temporary_dir = tempfile::tempdir().expect("create temporary directory");
        let tool = temporary_dir.path().join("fake capture tool");
        std::fs::write(&tool, "#!/bin/sh\ntrap '' INT\nwhile true; do :; done\n")
            .expect("write fake capture tool");
        let mut permissions = tool.metadata().expect("read permissions").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&tool, permissions).expect("make fake tool executable");

        let mut config = CaptureConfig::new(tool, temporary_dir.path().join("output with spaces"));
        config.connect_timeout = Duration::from_millis(50);
        config.stop_timeout = Duration::from_millis(50);
        configure(config).expect("configure fake capture");
        let result = start_named_capture("../../unsafe name.qml");
        assert!(matches!(result, Err(CaptureError::ConnectTimeout(_))));
        assert!(stop_capture()
            .expect("capture process was reaped")
            .is_none());
        shutdown().expect("reset capture controller");
    }
}
