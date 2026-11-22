#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn bounded_local_frames_preserve_samples_and_refuse_overflow_atomically() {
    use rt_fpga_testbench::{memory::Mode, suites::media::local_frames};
    for bits in [5, 21, 64] {
        for width in [32, 64] {
            for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
                local_frames::boundaries(bits, width, mode).unwrap();
            }
        }
        local_frames::boundaries(bits, 64, Mode::Legacy).unwrap();
    }
}
