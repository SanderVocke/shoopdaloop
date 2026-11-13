#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn transition_processor_matches_production_and_preserves_transport_contracts() {
    for &(name, depths) in rt_fpga_testbench::suites::transition::CONFIGS {
        rt_fpga_testbench::suites::transition::run(name, depths, 100).unwrap();
    }
}
