#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn standalone_state_processor_preserves_instruction_fifo_and_protocol_contracts() {
    for &(name, depths) in rt_fpga_testbench::suites::state::CONFIGS {
        rt_fpga_testbench::suites::state::run(name, depths).unwrap();
    }
}
