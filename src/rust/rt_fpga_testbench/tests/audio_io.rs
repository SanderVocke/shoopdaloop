#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn asynchronous_audio_bank_ownership_deadlines_reset_and_publication_cutoff() {
    for &c in rt_fpga_testbench::suites::audio_io::CONFIGS {
        rt_fpga_testbench::suites::audio_io::run(c).unwrap();
    }
}
#[shoop_test]
fn control_fences_backpressure_and_failures() {
    rt_fpga_testbench::suites::control::fences_and_failures().unwrap();
}
