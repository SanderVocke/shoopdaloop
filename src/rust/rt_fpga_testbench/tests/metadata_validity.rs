#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn metadata_validity_survives_repeated_invalidation_and_reset_with_stale_ram() {
    for width in [32, 64] {
        rt_fpga_testbench::suites::metadata::validity::workloads(width).unwrap();
    }
}
