#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn axi_endpoints_word_and_chunk_transfers_preserve_bounds_strobes_and_retirement() {
    for mode in [
        rt_fpga_testbench::memory::Mode::Instant,
        rt_fpga_testbench::memory::Mode::Fixed,
        rt_fpga_testbench::memory::Mode::Dramsim3,
    ] {
        for width in [32, 64] {
            rt_fpga_testbench::suites::axi::run(width, mode).unwrap();
        }
    }
}
