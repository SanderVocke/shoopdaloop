#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn capture_cuts_preserve_retained_content_and_exports_without_host_observation() {
    rt_fpga_testbench::suites::media_cuts::ownership().unwrap();
}

#[shoop_test]
fn device_observations_are_coherent_and_independent_of_held_host_replies() {
    rt_fpga_testbench::suites::transition::coherent_observations().unwrap();
}

#[shoop_test]
fn autonomous_capture_and_retake_cuts_preserve_real_stereo_samples_in_all_memory_models() {
    rt_fpga_testbench::suites::media_cuts::samples().unwrap();
}

#[shoop_test]
fn accepted_cut_records_are_retained_until_valid_ack_and_cannot_alias_across_reset() {
    rt_fpga_testbench::suites::media_cuts::retained_records().unwrap();
}
