#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn deployed_runner_measurement_boundaries_watchdog_drain_snapshot_stop_and_restart() {
    use rt_fpga_testbench::{memory::Mode, suites::zedboard};
    let workload = zedboard::prepare_example().unwrap();
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        zedboard::deployed_runner(&workload, 12, true, mode).unwrap();
    }
}
