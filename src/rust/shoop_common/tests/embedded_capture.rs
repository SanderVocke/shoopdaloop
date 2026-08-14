use std::path::{Path, PathBuf};

#[test]
fn embedded_capture_publishes_a_trace() {
    let temporary_dir = std::env::var_os("SHOOP_TRACY_TEST_OUTPUT_DIR")
        .map(PathBuf::from)
        .map(TemporaryOutput::External)
        .unwrap_or_else(|| {
            TemporaryOutput::Owned(tempfile::tempdir().expect("create capture output directory"))
        });
    let output_dir = temporary_dir.path();
    std::fs::create_dir_all(output_dir).expect("create external capture output directory");

    shoop_tracing::set_tracing_enabled(true);
    let mut capture =
        shoop_common::tracing_capture::CaptureSession::configure(output_dir, "integration")
            .expect("configure embedded capture");
    let client = tracy_client::Client::start();
    capture
        .wait_until_capturing()
        .expect("start embedded capture");
    client.message("shoop.embedded_capture.integration", 0);
    let span = client.span(
        tracy_client::span_location!("shoop.embedded_capture.integration.zone"),
        0,
    );
    drop(span);
    capture.finish().expect("finalize embedded capture");

    let metadata = capture.path().metadata().expect("read finalized capture");
    assert!(metadata.len() > 0);
    assert!(!capture.path().with_extension("tracy.partial").exists());
    eprintln!("embedded capture test output: {}", capture.path().display());
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
