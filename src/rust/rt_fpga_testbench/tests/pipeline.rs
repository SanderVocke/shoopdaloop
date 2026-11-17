#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn ordered_media_audio_publication_protocols_faults_reset_and_retained_blocks() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::pipeline};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3, Mode::Legacy] {
        pipeline::run(
            "default",
            [32, 16, 32, 32, 16, 2, 2],
            Geometry::new([32, 32, 32], 1),
            64,
            mode,
            1,
            20,
        )
        .unwrap();
        pipeline::run(
            "small",
            [16, 8, 16, 4, 8, 2, 2],
            Geometry::new([16, 8, 4], 1),
            64,
            mode,
            5,
            20,
        )
        .unwrap();
        pipeline::run(
            "blocked",
            [32, 16, 32, 32, 16, 2, 2],
            Geometry::new([16, 32 * 32, 4], 32),
            64,
            mode,
            1,
            0,
        )
        .unwrap();
    }
}
