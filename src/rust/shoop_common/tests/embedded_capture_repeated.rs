use std::path::{Path, PathBuf};

use shoop_common::tracing_capture::{
    shutdown_reusable_profiler, CaptureDisposition, CaptureStatus, ReusableCaptureSession,
};

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires the native embedded Tracy runtime and filesystem",
    no_tracy = "manages the embedded capture lifecycle directly"
)]
fn embedded_capture_supports_save_then_discard() {
    let temporary_dir = std::env::var_os("SHOOP_TRACY_TEST_OUTPUT_DIR")
        .map(PathBuf::from)
        .map(TemporaryOutput::External)
        .unwrap_or_else(|| {
            TemporaryOutput::Owned(tempfile::tempdir().expect("create capture output directory"))
        });
    let output_dir = temporary_dir.path();
    std::fs::create_dir_all(output_dir).expect("create external capture output directory");

    let mut first = ReusableCaptureSession::start(output_dir, "repeated-save")
        .expect("start first reusable capture");
    let client = tracy_client::Client::start();
    first
        .wait_until_capturing()
        .expect("wait for first reusable capture");
    let first_status = CaptureStatus::current();
    assert!(first_status.active);
    assert!(first_status.event_storage_bytes > 0);
    client.message("shoop.embedded_capture.repeated.first", 0);
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
    client.message("shoop.embedded_capture.repeated.second", 0);
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
