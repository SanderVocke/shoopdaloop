#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn chunk_cache_visibility_protocols_and_block_retirement() {
    use rt_fpga_testbench::{memory::Mode, suites::chunk};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3, Mode::Legacy] {
        for width in if mode == Mode::Legacy {
            &[32][..]
        } else {
            &[32, 64][..]
        } {
            for &(name, c) in chunk::CONFIGS {
                chunk::run(name, c, *width, mode, 4).unwrap();
            }
            chunk::retirement("small", [8, 4, 2, 2], *width, mode, 32).unwrap();
        }
    }
}
