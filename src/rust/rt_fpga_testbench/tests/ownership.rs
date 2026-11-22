#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn full_pools_transactional_view_reuse_block_sharing_and_allocator_statistics() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::media};
    for mode in [Mode::Instant, Mode::Fixed, Mode::Dramsim3] {
        media::run_capacity(64, mode).unwrap();
        media::run_reuse(64, mode).unwrap();
        for reuse in [false, true] {
            media::run_statistics(64, mode, reuse).unwrap();
        }
        for bc in [2, 32] {
            media::run_cases(
                Geometry::new([16, 8 * bc, 4], bc),
                64,
                mode,
                &[("block_ownership", media::ownership::block_ownership)],
            )
            .unwrap();
        }
    }
}

#[shoop_test]
fn descriptor_statistics_count_distinct_views_and_discard_stale_ram_after_reset() {
    use rt_fpga_testbench::{drivers::store::Geometry, memory::Mode, suites::media};
    for views in [1, 7, 512, 513] {
        for reuse in [false, true] {
            let mut geometry = Geometry::new([views, 8, 4], 1);
            geometry.table_pages = 2;
            geometry.reuse_views = reuse;
            media::run_cases(
                geometry,
                64,
                Mode::Fixed,
                &[(
                    "descriptor_statistics_reset_restore",
                    media::ownership::descriptor_statistics_reset_restore,
                )],
            )
            .unwrap();
        }
    }
}
