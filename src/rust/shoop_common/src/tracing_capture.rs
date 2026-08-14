use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::logging::macros::*;
shoop_log_unit!("Tracing.Capture");

const CHANNEL_CAPACITY: usize = 256 * 1024;
const WORKER_MEMORY_LIMIT: i64 = 256 * 1024 * 1024;
const START_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("embedded Tracy capture ABI mismatch: expected {expected}, found {actual}")]
    AbiMismatch { expected: u32, actual: u32 },
    #[error("capture output path is not valid UTF-8: {0}")]
    NonUtf8Path(PathBuf),
    #[error("failed to {action} '{path}': {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("embedded Tracy capture failed with status {status}: {diagnostic}")]
    Embedded { status: i32, diagnostic: String },
    #[error("embedded Tracy capture did not start within {timeout:?}; state={state}")]
    StartTimeout { timeout: Duration, state: i32 },
    #[error("capture output '{0}' is missing or empty")]
    MissingOutput(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureDisposition {
    Save,
    Discard,
}

impl CaptureDisposition {
    const fn as_ffi(self) -> i32 {
        match self {
            Self::Save => tracy_client_sys::TRACY_EMBEDDED_CAPTURE_SAVE,
            Self::Discard => tracy_client_sys::TRACY_EMBEDDED_CAPTURE_DISCARD,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureStatus {
    pub active: bool,
    pub event_storage_bytes: u64,
}

impl CaptureStatus {
    pub fn current() -> Self {
        let state = unsafe { tracy_client_sys::___tracy_embedded_capture_get_state() };
        let event_storage_bytes =
            unsafe { tracy_client_sys::___tracy_embedded_capture_get_event_storage_bytes() };
        Self {
            active: state == tracy_client_sys::TRACY_EMBEDDED_CAPTURE_CAPTURING,
            event_storage_bytes: event_storage_bytes.max(0) as u64,
        }
    }
}

pub struct CaptureSession {
    path: PathBuf,
    finished: bool,
}

impl CaptureSession {
    pub fn configure(output_dir: &Path, label: &str) -> Result<Self, CaptureError> {
        verify_abi()?;
        std::fs::create_dir_all(output_dir)
            .map_err(|source| io_error("create capture output directory", output_dir, source))?;
        let output_dir = output_dir.canonicalize().map_err(|source| {
            io_error("canonicalize capture output directory", output_dir, source)
        })?;
        let path = next_capture_path(&output_dir, label);
        let path_str = path
            .to_str()
            .ok_or_else(|| CaptureError::NonUtf8Path(path.clone()))?;
        let status = unsafe {
            tracy_client_sys::___tracy_embedded_capture_configure(
                path_str.as_ptr().cast(),
                path_str.len(),
                CHANNEL_CAPACITY,
                WORKER_MEMORY_LIMIT,
            )
        };
        check_status(status)?;
        info!("Configured embedded Tracy capture {}", path.display());
        Ok(Self {
            path,
            finished: false,
        })
    }

    pub fn wait_until_capturing(&self) -> Result<(), CaptureError> {
        wait_until_capturing(&self.path)
    }

    pub fn finish(&mut self) -> Result<(), CaptureError> {
        if self.finished {
            return Ok(());
        }
        crate::tracing_helpers::set_tracing_output_enabled(false);
        let status = unsafe {
            tracy_client_sys::___tracy_embedded_capture_finish_with_disposition(
                tracy_client_sys::TRACY_EMBEDDED_CAPTURE_SAVE,
            )
        };
        check_status(status)?;
        self.finished = true;

        let metadata = self
            .path
            .metadata()
            .map_err(|_| CaptureError::MissingOutput(self.path.clone()))?;
        if metadata.len() == 0 {
            return Err(CaptureError::MissingOutput(self.path.clone()));
        }

        let mut statistics = tracy_client_sys::tracy_embedded_capture_statistics::default();
        let statistics_status =
            unsafe { tracy_client_sys::___tracy_embedded_capture_get_statistics(&mut statistics) };
        check_status(statistics_status)?;
        info!(
            "Finalized embedded Tracy capture {} ({} bytes, transport c2s={}, s2c={})",
            self.path.display(),
            metadata.len(),
            statistics.client_to_server_bytes,
            statistics.server_to_client_bytes,
        );
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct ReusableCaptureSession {
    path: PathBuf,
    stopped: bool,
}

impl ReusableCaptureSession {
    pub fn start(output_dir: &Path, label: &str) -> Result<Self, CaptureError> {
        verify_abi()?;
        std::fs::create_dir_all(output_dir)
            .map_err(|source| io_error("create capture output directory", output_dir, source))?;
        let output_dir = output_dir.canonicalize().map_err(|source| {
            io_error("canonicalize capture output directory", output_dir, source)
        })?;
        let path = next_capture_path(&output_dir, label);
        let path_str = path
            .to_str()
            .ok_or_else(|| CaptureError::NonUtf8Path(path.clone()))?;
        let status = unsafe {
            tracy_client_sys::___tracy_embedded_capture_start(
                path_str.as_ptr().cast(),
                path_str.len(),
                CHANNEL_CAPACITY,
                WORKER_MEMORY_LIMIT,
            )
        };
        check_status(status)?;
        info!(
            "Starting reusable embedded Tracy capture {}",
            path.display()
        );
        Ok(Self {
            path,
            stopped: false,
        })
    }

    pub fn wait_until_capturing(&self) -> Result<(), CaptureError> {
        wait_until_capturing(&self.path)
    }

    pub fn stop(&mut self, disposition: CaptureDisposition) -> Result<(), CaptureError> {
        if self.stopped {
            return Ok(());
        }
        let status = unsafe {
            tracy_client_sys::___tracy_embedded_capture_stop_with_disposition(disposition.as_ffi())
        };
        check_status(status)?;
        self.stopped = true;

        if disposition == CaptureDisposition::Save {
            let metadata = self
                .path
                .metadata()
                .map_err(|_| CaptureError::MissingOutput(self.path.clone()))?;
            if metadata.len() == 0 {
                return Err(CaptureError::MissingOutput(self.path.clone()));
            }
            info!(
                "Finalized reusable embedded Tracy capture {} ({} bytes)",
                self.path.display(),
                metadata.len()
            );
        } else {
            info!("Discarded reusable embedded Tracy capture");
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn shutdown_reusable_profiler() -> Result<(), CaptureError> {
    let status = unsafe { tracy_client_sys::___tracy_embedded_capture_shutdown() };
    check_status(status)
}

fn verify_abi() -> Result<(), CaptureError> {
    let actual = unsafe { tracy_client_sys::___tracy_embedded_capture_abi_version() };
    let expected = tracy_client_sys::TRACY_EMBEDDED_CAPTURE_ABI_VERSION;
    if actual == expected {
        Ok(())
    } else {
        Err(CaptureError::AbiMismatch { expected, actual })
    }
}

fn wait_until_capturing(path: &Path) -> Result<(), CaptureError> {
    let deadline = Instant::now() + START_TIMEOUT;
    loop {
        let state = unsafe { tracy_client_sys::___tracy_embedded_capture_get_state() };
        if state == tracy_client_sys::TRACY_EMBEDDED_CAPTURE_CAPTURING {
            info!("Embedded Tracy capture started: {}", path.display());
            return Ok(());
        }
        if state == tracy_client_sys::TRACY_EMBEDDED_CAPTURE_FAILED {
            return Err(embedded_error(state));
        }
        if Instant::now() >= deadline {
            return Err(CaptureError::StartTimeout {
                timeout: START_TIMEOUT,
                state,
            });
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

fn check_status(status: i32) -> Result<(), CaptureError> {
    if status == tracy_client_sys::TRACY_EMBEDDED_CAPTURE_OK {
        Ok(())
    } else {
        Err(embedded_error(status))
    }
}

fn embedded_error(status: i32) -> CaptureError {
    let length =
        unsafe { tracy_client_sys::___tracy_embedded_capture_get_error(std::ptr::null_mut(), 0) };
    let mut bytes = vec![0_u8; length + 1];
    unsafe {
        tracy_client_sys::___tracy_embedded_capture_get_error(
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        );
    }
    CaptureError::Embedded {
        status,
        diagnostic: String::from_utf8_lossy(&bytes[..length]).into_owned(),
    }
}

fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> CaptureError {
    CaptureError::Io {
        action,
        path: path.to_path_buf(),
        source,
    }
}

fn next_capture_path(output_dir: &Path, label: &str) -> PathBuf {
    let label = sanitize_label(label);
    for sequence in 1_u64.. {
        let path = output_dir.join(format!("{sequence:04}-{label}.tracy"));
        if !path.exists() && !path.with_extension("tracy.partial").exists() {
            return path;
        }
    }
    unreachable!("capture sequence is finite")
}

fn sanitize_label(label: &str) -> String {
    let sanitized: String = label
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
        "capture".to_owned()
    } else {
        sanitized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tracy_nextest_capture::tracy_capture_test]
    fn sanitizes_capture_labels() {
        assert_eq!(sanitize_label("application"), "application");
        assert_eq!(sanitize_label("../../unsafe name"), ".._.._unsafe_name");
        assert_eq!(sanitize_label("💥"), "_");
        assert_eq!(sanitize_label(".."), "capture");
        assert_eq!(sanitize_label(""), "capture");
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn capture_paths_are_unique_and_confined_to_output_directory() {
        let temporary_dir = tempfile::tempdir().expect("create temporary directory");
        let first = next_capture_path(temporary_dir.path(), "../unsafe name");
        assert_eq!(
            first,
            temporary_dir.path().join("0001-.._unsafe_name.tracy")
        );
        std::fs::write(&first, b"existing trace").expect("reserve first trace name");
        let second = next_capture_path(temporary_dir.path(), "../unsafe name");
        assert_eq!(
            second,
            temporary_dir.path().join("0002-.._unsafe_name.tracy")
        );
        assert_eq!(second.parent(), Some(temporary_dir.path()));
    }
}
