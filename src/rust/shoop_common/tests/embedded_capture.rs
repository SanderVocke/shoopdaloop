use std::path::{Path, PathBuf};

#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires the native Perfetto runtime and filesystem",
    no_trace = "manages the capture lifecycle directly"
)]
fn embedded_capture_publishes_a_trace() {
    let temporary_dir = std::env::var_os("SHOOP_PERFETTO_TEST_OUTPUT_DIR")
        .map(PathBuf::from)
        .map(TemporaryOutput::External)
        .unwrap_or_else(|| {
            TemporaryOutput::Owned(tempfile::tempdir().expect("create capture output directory"))
        });
    let output_dir = temporary_dir.path();
    std::fs::create_dir_all(output_dir).expect("create external capture output directory");

    let mut capture =
        shoop_common::tracing_capture::CaptureSession::configure(output_dir, "integration")
            .expect("configure Perfetto capture");
    capture
        .wait_until_capturing()
        .expect("start Perfetto capture");
    shoop_tracing::set_tracing_enabled(true);
    let span = shoop_tracing::realtime_span!("shoop.capture.integration.zone");
    assert!(span.entered_tracing());
    shoop_tracing::realtime_frame_mark!("shoop.capture.integration.event");
    drop(span);
    shoop_tracing::set_tracing_enabled(false);
    capture.finish().expect("finalize Perfetto capture");

    let metadata = capture.path().metadata().expect("read finalized capture");
    assert!(metadata.len() > 0);
    assert!(!capture.path().with_extension("pftrace.partial").exists());
    eprintln!("Perfetto capture test output: {}", capture.path().display());
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
