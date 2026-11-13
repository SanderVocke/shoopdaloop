use rt_fpga_adapter::memory::decode;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn pressure_tracks_independent_pools_and_counts_whole_allocation_blocks() {
    let report = decode(64, [1, 8, 19, 20, 2], [65536, 0, 32, 32, 8]).unwrap();
    assert_eq!(report.frame, 64);
    assert_eq!(report.accounted_bytes, None);
    let samples = report.resources[0].unwrap();
    assert_eq!(samples.used, 4096);
    assert_eq!(samples.capacity, Some(32768));
    let limiting = report.limiting_resource().unwrap();
    assert_eq!(limiting.name.as_str(), "Metadata pages");
    assert_eq!(limiting.pressure(), Some(0.95));
}

#[shoop_test]
fn invalid_counter_samples_are_refused_instead_of_reporting_false_headroom() {
    let geometry = [65536, 0, 32, 32, 8];
    for usage in [
        [9, 8, 1, 20, 1],
        [1, 8, 21, 20, 1],
        [1, 8, 1, 20, 9],
        [0, 0, 0, 20, 0],
    ] {
        assert!(decode(0, usage, geometry).is_err());
    }
    for geometry in [
        [1, 0, 32, 32, 8],
        [65536, 0, 0, 32, 8],
        [65536, 0, 32, 3, 8],
        [65536, 0, 32, 32, 0],
    ] {
        assert!(decode(0, [1, 8, 1, 20, 1], geometry).is_err());
    }
}
