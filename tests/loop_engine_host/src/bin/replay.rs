#[cfg(not(target_arch = "wasm32"))]
fn main() {
    assert!(loop_engine_host_fixture::allocator_canaries());
    let mode = std::env::args().nth(1).unwrap_or_else(|| "--direct".into());
    let driver = match mode.as_str() {
        "--direct" => loop_engine_host_fixture::run,
        "--wire" => loop_engine_host_fixture::run_wire,
        "--two-thread" => loop_engine_host_fixture::run_two_thread,
        _ => panic!("expected --direct, --wire or --two-thread"),
    };
    let report = loop_engine_host_fixture::run(&[13]);
    for partitions in [
        vec![13],
        vec![1; 13],
        vec![3, 5, 2, 3],
        vec![0, 4, 0, 8, 0, 1, 0],
    ] {
        assert_eq!(driver(&partitions), report);
    }
    println!(
        "native loop {mode}: {} stereo frames, {} MIDI packets, {} transitions; {} pages/{} versions/{} snapshots free, {} media references; four partitions agree",
        report.samples.len(), report.midi.len(), report.transitions.len(), report.free_pages,
        report.free_versions, report.free_snapshots, report.media_references,
    );
}

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("replay is a native binary; use the fixture library for Wasm");
}
