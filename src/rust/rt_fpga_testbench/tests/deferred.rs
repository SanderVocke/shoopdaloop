#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn detached_ownership_bounded_cleanup_faults_and_preemption() {
    use rt_fpga_testbench::{memory::Mode, suites::media};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        media::run_deferred(64, mode).unwrap();
    }
}
