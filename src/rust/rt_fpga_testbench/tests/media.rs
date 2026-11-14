#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn media_store_retained_slices_atomic_refusals_protocols_and_reset() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::media};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3, Mode::Legacy] {
        for &(name, c) in &media::CONFIGS[..3] {
            media::run(name, Geometry::new(c, 1), 64, mode, 40).unwrap();
        }
    }
}

#[shoop_test]
fn capture_views_seal_in_place_with_bounded_work_and_retained_ownership() {
    use rt_fpga_testbench::{memory::Mode, suites::media};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3, Mode::Legacy] {
        media::run_seal(mode).unwrap();
    }
}

#[shoop_test]
fn reusable_view_usage_counts_sparse_descriptors_and_reset_for_single_and_odd_capacities() {
    rt_fpga_testbench::suites::media::reusable_view_usage().unwrap();
}
