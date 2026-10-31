#![forbid(unsafe_code)]
#[cfg(not(target_arch = "wasm32"))]
#[path = "../examples/latency_probe/current.rs"]
mod current;
#[path = "../examples/latency_probe/scenario.rs"]
mod scenario;
use scenario::{run, Capture, Partition, Signal};
use shoop_wasm_test_support::{heap, shoop_test};
#[global_allocator]
static ALLOCATOR: heap::TrackingAllocator = heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
#[shoop_test]
fn physical_capture_markers_and_missing_windows_agree_across_partitions() {
    for signal in [Signal::Ramp, Signal::Impulses] {
        for capture in [Capture::Raw, Capture::Tail, Capture::Margins] {
            let expected = run(signal, capture, Partition::Whole);
            for partition in [Partition::Single, Partition::Irregular] {
                assert_eq!(run(signal, capture, partition), expected);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test]
fn current_engine_retains_history_postroll_and_independent_logical_windows() {
    for signal in [Signal::Ramp, Signal::Impulses] {
        let physical = run(signal, Capture::Raw, Partition::Whole);
        current::verify(&physical, false);
    }
}
