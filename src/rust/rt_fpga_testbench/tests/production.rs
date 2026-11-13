#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn complete_production_sessions_match_the_shared_rust_backend_exactly() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::production};
    let scenarios = serde_json::from_str::<Vec<serde_json::Value>>(include_str!(
        "../fixtures/production-directed.json"
    ))
    .unwrap();
    production::run(
        &scenarios,
        production::DEFAULT,
        Geometry::new([32, 128, 32], 1),
        64,
        Mode::Instant,
    )
    .unwrap();
}
#[shoop_test]
fn fatal_processing_faults_and_cleanup_handoff_boundaries() {
    use rt_fpga_testbench::{memory::Mode, suites::production_faults};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        production_faults::run(mode, 1).unwrap();
    }
}
