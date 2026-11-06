use freertos_profiling::*;
use profiling_protocol::Name;
use shoop_wasm_test_support::shoop_test;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn sample(total: u64, runtime: u64, captured_us: u64) -> Snapshot {
    Snapshot {
        generation: 1,
        clock_domain: 7,
        captured_us,
        collection_us: 10,
        total_runtime: Some(total),
        counter_bits: 64,
        counter_hz: 1000000,
        complete: true,
        tasks: vec![Task {
            id: 1,
            name: Name::new("render").unwrap(),
            state: 2,
            priority: 6,
            base_priority: Some(6),
            runtime: Some(runtime),
            stack_free_bytes: Some(1024),
        }],
    }
}
#[shoop_test]
fn interval_usage_resets_on_missing_counters_boot_failure_and_task_reuse() {
    let first = sample(100, 25, 100);
    let next = sample(200, 75, 200);
    let mut view = View::default();
    view.receive(first.clone());
    assert_eq!(view.cpu_percent, [None]);
    view.receive(next.clone());
    assert_eq!(view.cpu_percent, [Some(50.)]);
    let mut changed = next.clone();
    changed.tasks[0].id = 2;
    assert_eq!(changed.cpu_percent(&first, &changed.tasks[0]), None);
    changed = next.clone();
    changed.clock_domain = 8;
    assert_eq!(changed.cpu_percent(&first, &changed.tasks[0]), None);
    changed = next.clone();
    changed.complete = false;
    assert_eq!(changed.cpu_percent(&first, &changed.tasks[0]), None);
    changed = next.clone();
    changed.total_runtime = None;
    assert_eq!(changed.cpu_percent(&first, &changed.tasks[0]), None);
    assert_eq!(first.cpu_percent(&next, &first.tasks[0]), None);
    let bytes = profiling_protocol::encode_report(&next).unwrap();
    assert_eq!(
        profiling_protocol::decode::<Snapshot>(&bytes).unwrap(),
        next
    );
}
#[shoop_test]
fn wrap_is_only_valid_with_a_bounded_interval_and_table_identities_are_unique() {
    let mut first = sample(u32::MAX as u64 - 99, u32::MAX as u64 - 49, 1000);
    let mut next = sample(100, 50, 1200);
    first.counter_bits = 32;
    next.counter_bits = 32;
    assert_eq!(next.cpu_percent(&first, &next.tasks[0]), Some(50.));
    next.captured_us = 1u64 << 33;
    assert_eq!(next.cpu_percent(&first, &next.tasks[0]), None);
    next.tasks.push(next.tasks[0].clone());
    assert!(!next.valid());
    next.tasks = vec![next.tasks[0].clone(); MAX_TASKS + 1];
    assert!(!next.valid());
}
