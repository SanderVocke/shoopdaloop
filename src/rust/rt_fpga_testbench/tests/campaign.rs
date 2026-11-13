#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn isolated_reference_release_timing() {
    use rt_fpga_testbench::{memory::Mode, suites::media::campaign};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        campaign::timing(64, mode).unwrap();
    }
}
#[shoop_test]
fn thirty_second_mono_and_stereo_ownership_tables_with_continued_recording() {
    use rt_fpga_testbench::suites::media::campaign;
    for (loops, channels) in [(32, 1), (16, 2)] {
        campaign::stress(loops, channels, 30).unwrap();
    }
}
