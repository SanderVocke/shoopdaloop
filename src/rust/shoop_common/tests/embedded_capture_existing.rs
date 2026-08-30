#[shoop_wasm_test_support::shoop_test(
    no_wasm = "requires the native Perfetto runtime and filesystem",
    no_trace = "manages the capture lifecycle directly"
)]
fn capture_path_selection_preserves_existing_output() {
    let temporary_dir = tempfile::tempdir().expect("create capture output directory");
    let occupied = temporary_dir.path().join("0001-existing.pftrace");
    std::fs::write(&occupied, b"occupied").expect("occupy first capture output");

    let mut capture =
        shoop_common::tracing_capture::CaptureSession::configure(temporary_dir.path(), "existing")
            .expect("start capture after occupied path");
    assert_eq!(
        capture.path(),
        temporary_dir.path().join("0002-existing.pftrace")
    );
    shoop_tracing::set_tracing_enabled(true);
    shoop_tracing::realtime_frame_mark!("shoop.capture.existing.event");
    shoop_tracing::set_tracing_enabled(false);
    capture.finish().expect("finish capture");

    assert_eq!(std::fs::read(occupied).unwrap(), b"occupied");
    assert!(capture.path().metadata().unwrap().len() > 0);
}
