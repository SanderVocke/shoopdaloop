#[cfg(not(target_arch = "wasm32"))]
fn main() {
    assert!(loop_engine_host_fixture::allocator_canaries());
    let mode = std::env::args().nth(1).unwrap_or_else(|| "--direct".into());
    if mode == "--planner-json" {
        println!("{}", loop_engine_host_fixture::planner_report());
        return;
    }
    if mode == "--media-json" || mode == "--media-two-thread-json" {
        println!(
            "{}",
            loop_engine_host_fixture::run_media(mode == "--media-two-thread-json", &[57]).json()
        );
        return;
    }
    if mode == "--stall-json" || mode == "--stall-two-thread-json" {
        println!(
            "{}",
            loop_engine_host_fixture::run_stalls(mode == "--stall-two-thread-json", &[128]).json()
        );
        return;
    }
    if mode == "--dense-json" || mode == "--dense-two-thread-json" {
        println!(
            "{}",
            loop_engine_host_fixture::run_dense(mode == "--dense-two-thread-json", &[128]).json()
        );
        return;
    }
    if mode == "--staging-json" || mode == "--staging-two-thread-json" {
        println!(
            "{}",
            loop_engine_host_fixture::run_staging(mode == "--staging-two-thread-json").json()
        );
        return;
    }
    if mode == "--export-json" || mode == "--export-two-thread-json" {
        println!(
            "{}",
            loop_engine_host_fixture::run_exports(mode == "--export-two-thread-json", &[128])
                .json()
        );
        return;
    }
    if mode == "--graph-json" || mode == "--graph-two-thread-json" {
        let run = if mode == "--graph-json" {
            loop_engine_host_fixture::run_graph
        } else {
            loop_engine_host_fixture::run_graph_two_thread
        };
        println!("{}", run(&[128]).json());
        return;
    }
    let driver = match mode.as_str() {
        "--direct" => loop_engine_host_fixture::run,
        "--wire" => loop_engine_host_fixture::run_wire,
        "--two-thread" => loop_engine_host_fixture::run_two_thread,
        "--json" => loop_engine_host_fixture::run_wire,
        _ => panic!("expected --direct, --wire, --two-thread, --json, --graph-json, --graph-two-thread-json, --export-json, --export-two-thread-json, --staging-json or --staging-two-thread-json"),
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
    if mode == "--json" {
        let midi = report
            .midi
            .iter()
            .map(|(at, packet)| format!("[{at},{packet:?}]"))
            .collect::<Vec<_>>()
            .join(",");
        let transitions = report
            .transitions
            .iter()
            .map(|(requested, delivered, terminal)| format!("[{requested},{delivered},{terminal}]"))
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"samples\":{:?},\"midi\":[{}],\"transitions\":[{}],\"semanticBytes\":{:?},\"recovered\":[{},{},{},{}]}}",
            report.samples,
            midi,
            transitions,
            report.semantic,
            report.free_pages,
            report.free_versions,
            report.free_snapshots,
            report.media_references
        );
        return;
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
