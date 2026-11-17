#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn sample_pipeline_arithmetic_streaming_reset_and_production_numeric_contract() {
    for &(name, capacities) in rt_fpga_testbench::suites::audio::CONFIGS {
        rt_fpga_testbench::suites::audio::run(name, capacities, 150).unwrap();
    }
}
