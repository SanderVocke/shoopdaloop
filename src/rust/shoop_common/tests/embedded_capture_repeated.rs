use std::path::{Path, PathBuf};

use shoop_common::tracing_capture::{
    shutdown_reusable_profiler, CaptureDisposition, CaptureStatus, ReusableCaptureSession,
};

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires the native Perfetto runtime and filesystem",
    no_trace = "manages the capture lifecycle directly"
)]
fn embedded_capture_supports_save_then_discard() {
    let temporary_dir = std::env::var_os("SHOOP_PERFETTO_TEST_OUTPUT_DIR")
        .map(PathBuf::from)
        .map(TemporaryOutput::External)
        .unwrap_or_else(|| {
            TemporaryOutput::Owned(tempfile::tempdir().expect("create capture output directory"))
        });
    let output_dir = temporary_dir.path();
    std::fs::create_dir_all(output_dir).expect("create external capture output directory");

    let mut first = ReusableCaptureSession::start(output_dir, "repeated-save")
        .expect("start first reusable capture");
    first
        .wait_until_capturing()
        .expect("wait for first reusable capture");
    let first_status = CaptureStatus::current();
    assert!(first_status.active);
    assert!(first_status.event_storage_bytes > 0);
    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::realtime_frame_mark!("shoop.capture.repeated.first");
    shoop_tracing::set_tracing_enabled(false);
    first
        .stop(CaptureDisposition::Save)
        .expect("save first reusable capture");
    let idle_status = CaptureStatus::current();
    assert!(!idle_status.active);
    assert_eq!(idle_status.event_storage_bytes, 0);
    assert!(first.path().metadata().unwrap().len() > 0);

    let mut second = ReusableCaptureSession::start(output_dir, "repeated-discard")
        .expect("start second reusable capture");
    second
        .wait_until_capturing()
        .expect("wait for second reusable capture");
    let second_status = CaptureStatus::current();
    assert!(second_status.active);
    assert!(second_status.event_storage_bytes > 0);
    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::realtime_frame_mark!("shoop.capture.repeated.second");
    shoop_tracing::set_tracing_enabled(false);
    second
        .stop(CaptureDisposition::Discard)
        .expect("discard second reusable capture");
    let idle_status = CaptureStatus::current();
    assert!(!idle_status.active);
    assert_eq!(idle_status.event_storage_bytes, 0);
    assert!(!second.path().exists());

    shutdown_reusable_profiler().expect("shut down reusable profiler");
}

enum TemporaryOutput {
    External(PathBuf),
    Owned(tempfile::TempDir),
}

impl TemporaryOutput {
    fn path(&self) -> &Path {
        match self {
            Self::External(path) => path,
            Self::Owned(directory) => directory.path(),
        }
    }
}
