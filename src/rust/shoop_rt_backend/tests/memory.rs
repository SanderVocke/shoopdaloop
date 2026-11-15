mod support;
use shoop_backend::Backend;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn memory_is_reported_without_timing_profiling_and_updates_at_five_hz() {
    let mut backend = stack(&description());
    let first = backend.poll().unwrap();
    let memory = first.status.memory.expect("initial memory telemetry");
    assert!(memory.valid());
    assert!(memory.accounted_bytes.unwrap() > 0);
    assert!(!first.profiling.active);
    backend.clock_mut().render(32);
    let unchanged = backend.poll().unwrap().status.memory.unwrap();
    assert_eq!(unchanged.frame, memory.frame);
    backend.clock_mut().render(208);
    let latest = backend.poll().unwrap().status.memory.unwrap();
    assert!(latest.frame > memory.frame);
    assert_eq!(latest.accounted_bytes, memory.accounted_bytes);
    assert!(latest.valid());
    backend.disconnect();
    assert!(backend.poll().unwrap().status.memory.is_none());
}
