#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn independent_smoothing_histories_extremes_and_irregular_job_boundaries() {
    rt_fpga_testbench::suites::smoothing::histories_and_extremes().unwrap();
}
