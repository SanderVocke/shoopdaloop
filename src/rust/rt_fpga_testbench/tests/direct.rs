#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn functional_axi_memory_preserves_byte_lanes_error_tokens_and_reset_contents() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::direct::run(width).unwrap();
    }
}
