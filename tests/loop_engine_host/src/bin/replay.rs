fn main() {
    assert!(loop_engine_host_fixture::allocator_canaries());
    let report = loop_engine_host_fixture::run(&[13]);
    for partitions in [vec![1; 13], vec![3, 5, 2, 3], vec![0, 4, 0, 8, 0, 1, 0]] {
        assert_eq!(loop_engine_host_fixture::run(&partitions), report);
    }
    println!(
        "native loop foundation: {} stereo frames, {} MIDI packets, {} transitions; {} pages/{} versions/{} snapshots free, {} media references; four partitions agree",
        report.samples.len(), report.midi.len(), report.transitions.len(), report.free_pages,
        report.free_versions, report.free_snapshots, report.media_references,
    );
}
