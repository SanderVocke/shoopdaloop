use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use perfetto_everywhere_core::TrackId;
use perfetto_everywhere_native::{
    CaptureConfig, CaptureSession as PerfettoCaptureSession, NativeError,
};
use thiserror::Error;

const BUFFER_SIZE_KB: u32 = 64 * 1024;
const FLUSH_TIMEOUT: Duration = Duration::from_secs(5);

static CAPTURE_ACTIVE: AtomicBool = AtomicBool::new(false);
static CAPTURE_CAPACITY_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("Perfetto capture failed: {0}")]
    Perfetto(#[from] NativeError),
    #[error("failed to {action} '{path}': {source}")]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("capture is not active")]
    Inactive,
    #[error("capture output '{0}' is missing or empty")]
    MissingOutput(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureDisposition {
    Save,
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptureStatus {
    pub active: bool,
    pub event_storage_bytes: u64,
}

impl CaptureStatus {
    pub fn current() -> Self {
        Self {
            active: CAPTURE_ACTIVE.load(Ordering::Acquire),
            event_storage_bytes: CAPTURE_CAPACITY_BYTES.load(Ordering::Relaxed),
        }
    }
}

pub struct CaptureSession {
    inner: Option<PerfettoCaptureSession>,
    path: PathBuf,
}

impl CaptureSession {
    pub fn configure(output_dir: &Path, label: &str) -> Result<Self, CaptureError> {
        let (inner, path) = start(output_dir, label)?;
        Ok(Self {
            inner: Some(inner),
            path,
        })
    }

    pub fn wait_until_capturing(&self) -> Result<(), CaptureError> {
        self.inner
            .as_ref()
            .map(|_| ())
            .ok_or(CaptureError::Inactive)
    }

    pub fn finish(&mut self) -> Result<(), CaptureError> {
        finish(&mut self.inner, &self.path, CaptureDisposition::Save)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct ReusableCaptureSession {
    inner: Option<PerfettoCaptureSession>,
    path: PathBuf,
}

impl ReusableCaptureSession {
    pub fn start(output_dir: &Path, label: &str) -> Result<Self, CaptureError> {
        let (inner, path) = start(output_dir, label)?;
        Ok(Self {
            inner: Some(inner),
            path,
        })
    }

    pub fn wait_until_capturing(&self) -> Result<(), CaptureError> {
        self.inner
            .as_ref()
            .map(|_| ())
            .ok_or(CaptureError::Inactive)
    }

    pub fn stop(&mut self, disposition: CaptureDisposition) -> Result<(), CaptureError> {
        finish(&mut self.inner, &self.path, disposition)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn shutdown_reusable_profiler() -> Result<(), CaptureError> {
    Ok(())
}

fn start(
    output_dir: &Path,
    label: &str,
) -> Result<(PerfettoCaptureSession, PathBuf), CaptureError> {
    std::fs::create_dir_all(output_dir)
        .map_err(|source| io_error("create capture output directory", output_dir, source))?;
    let output_dir = output_dir
        .canonicalize()
        .map_err(|source| io_error("canonicalize capture output directory", output_dir, source))?;
    let path = next_capture_path(&output_dir, label);
    let counter_tracks = crate::COUNTER_NAMES
        .iter()
        .map(|name| (*name, TrackId::CURRENT))
        .collect();
    let inner = PerfettoCaptureSession::start(CaptureConfig {
        buffer_size_kb: BUFFER_SIZE_KB,
        flush_timeout: FLUSH_TIMEOUT,
        counter_tracks,
        ..CaptureConfig::default()
    })?;
    CAPTURE_CAPACITY_BYTES.store(u64::from(BUFFER_SIZE_KB) * 1024, Ordering::Relaxed);
    CAPTURE_ACTIVE.store(true, Ordering::Release);
    Ok((inner, path))
}

fn finish(
    inner: &mut Option<PerfettoCaptureSession>,
    path: &Path,
    disposition: CaptureDisposition,
) -> Result<(), CaptureError> {
    let Some(session) = inner.take() else {
        return Ok(());
    };
    CAPTURE_ACTIVE.store(false, Ordering::Release);
    CAPTURE_CAPACITY_BYTES.store(0, Ordering::Relaxed);
    if disposition == CaptureDisposition::Discard {
        drop(session);
        return Ok(());
    }

    let report = session.finish()?;
    if report.bytes.is_empty() {
        return Err(CaptureError::MissingOutput(path.to_path_buf()));
    }
    let partial = path.with_extension("pftrace.partial");
    report.write_file(&partial)?;
    std::fs::rename(&partial, path).map_err(|source| io_error("publish capture", path, source))?;
    let metadata = path
        .metadata()
        .map_err(|_| CaptureError::MissingOutput(path.to_path_buf()))?;
    if metadata.len() == 0 {
        return Err(CaptureError::MissingOutput(path.to_path_buf()));
    }
    Ok(())
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
        let path = output_dir.join(format!("{sequence:04}-{label}.pftrace"));
        if !path.exists() && !path.with_extension("pftrace.partial").exists() {
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
