#![forbid(unsafe_code)]
#[path = "../examples/one_track/scenario.rs"]
mod scenario;
use scenario::{run, Partition};
use shoop_wasm_test_support::{heap, shoop_test};
#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn public_scenario_matches_complete_oracles_across_all_partitions() {
    scenario::verify_stopped_input();
    for mode in [Partition::Whole, Partition::Single, Partition::Irregular] {
        let summary = run(mode, false, None);
        assert_eq!(summary.frames, 42);
        assert_eq!(summary.packets, 487);
        assert_eq!(summary.copied_bytes, 378);
        assert!(summary.callbacks >= 11);
    }
}
#[shoop_test]
fn alternate_valid_query_declaration_order_preserves_the_public_scenario() {
    for mode in [Partition::Whole, Partition::Single, Partition::Irregular] {
        let summary = run(mode, true, None);
        assert_eq!(summary.frames, 42);
        assert_eq!(summary.packets, 487);
        assert_eq!(summary.copied_bytes, 378);
        assert!(summary.callbacks >= 11);
    }
}
