//! Shared geometry and transaction helpers for the media ownership suites.
mod basic;
pub mod campaign;
pub mod deferred;
pub mod ownership;
use crate::{
    Result,
    drivers::store::{Geometry, Store},
    memory::Mode,
    model::{Design, Model},
    sim::Sim,
};
use serde_json::{Value, json};
pub const CONFIGS: &[(&str, [u64; 3])] = &[
    ("default", [32, 32, 32]),
    ("small", [16, 8, 4]),
    ("odd", [5, 5, 8]),
    ("wide", [16, 32, 4]),
    ("8x8", [64, 1024, 32]),
    ("retirement", [64, 45056, 32]),
];
pub(crate) fn remaining_cycles(model: &Model<'_>, reclaimer: &str) -> u64 {
    model
        .get(&format!("{reclaimer}.cycle_budget"))
        .saturating_sub(model.get(&format!("{reclaimer}.maintenance_charge")))
}

pub fn design(g: &Geometry, width: u64, mode: Mode) -> Design {
    let root = crate::root();
    let mut sources: Vec<_> = ["chunk_ram", "chunk_priority", "chunk_memory"]
        .iter()
        .map(|n| root.join(format!("fpga/chunk_memory/rtl/{n}.sv")))
        .collect();
    sources.extend("media_ram metadata_cache media_metadata media_reclaimer media_prefetch_resolver media_store".split_whitespace().map(|n| root.join(format!("fpga/media_store/rtl/{n}.sv"))));
    let mut parameters = g.parameters();
    let top = if mode == Mode::Legacy {
        "media_store"
    } else {
        sources.extend("axi_master axi_burst_mux shared_axi chunk_dma word_dma word_axi chunk_axi media_store_axi".split_whitespace().map(|n| root.join(format!("fpga/axi_memory/rtl/{n}.sv"))));
        parameters.extend([("AXI_DATA_WIDTH", width), ("PHYSICAL_BASE", 0x01000000)]);
        "media_store_axi"
    };
    let design = Design::new(top, sources).parameters(&parameters);
    if mode == Mode::Instant {
        design.memory(g.words(), 0x01000000)
    } else {
        design
    }
}
pub fn deferred_geometry() -> Geometry {
    let mut g = Geometry::new([16, 1024, 4], 32);
    g.retired_tables = 2;
    g.table_lines = 1;
    g.reference_lines = 1;
    g.stack_lines = 1;
    g
}
pub fn deferred_design(g: &Geometry, width: u64, mode: Mode) -> Design {
    use crate::model::Probe;
    assert_ne!(mode, Mode::Legacy);
    design(g, width, mode).probes(
        [
            "core.metadata.directory_reads",
            "core.metadata.table_reads",
            "core.metadata.high_water",
            "core.metadata.free_pages",
            "core.gc_lock_held",
            "core.reclaimer.phase",
            "core.reclaimer.maintenance_charge",
        ]
        .iter()
        .map(|n| Probe::observe(n))
        .chain(
            ["core.reclaimer.cycle_budget", "core.reclaimer.blocks_left"]
                .iter()
                .map(|n| Probe::inject(n)),
        )
        .collect(),
    )
}
pub fn run_deferred(width: u64, mode: Mode) -> Result<()> {
    let g = deferred_geometry();
    let compiled = deferred_design(&g, width, mode)
        .build(&crate::root().join("work/marlin-suite/deferred"))?;
    for &(name, test) in deferred::TESTS {
        println!("deferred/{width}/{mode:?}/{name}");
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Store::new(&mut s, g.clone(), mode, 20261112, "")?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    Ok(())
}
pub type Case = (&'static str, fn(&mut Store, &mut Sim<'_>));
pub fn run_cases(g: Geometry, width: u64, mode: Mode, cases: &[Case]) -> Result<()> {
    let compiled = deferred_design(&g, width, mode)
        .build(&crate::root().join("work/marlin-suite/media-ownership"))?;
    for &(name, test) in cases {
        println!("ownership/{width}/{mode:?}/{name}");
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Store::new(&mut s, g.clone(), mode, 20261112, "")?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    Ok(())
}
pub fn run_capacity(width: u64, mode: Mode) -> Result<()> {
    let mut g = Geometry::new([16, 1024, 4], 32);
    g.table_pages = 3;
    run_cases(
        g,
        width,
        mode,
        &[
            (
                "full_pools_and_pause_after_reference_release",
                ownership::full_pools_and_pause_after_reference_release,
            ),
            (
                "failed_detachment_preserves_ownership_until_reset",
                ownership::failed_detachment_preserves_ownership_until_reset,
            ),
            (
                "published_retirement_fault_requires_a_coordinated_reset",
                ownership::published_retirement_fault_requires_a_coordinated_reset,
            ),
        ],
    )
}
pub fn run_reuse(width: u64, mode: Mode) -> Result<()> {
    let mut g = Geometry::new([16, 1024, 4], 32);
    g.table_pages = 7;
    g.retired_tables = 2;
    g.reuse_views = true;
    run_cases(
        g,
        width,
        mode,
        &[
            (
                "removed_view_reuse_keeps_the_retired_root_independent",
                ownership::removed_view_reuse_keeps_the_retired_root_independent,
            ),
            (
                "full_view_space_slice_checks_all_pages_before_copying",
                ownership::full_view_space_slice_checks_all_pages_before_copying,
            ),
        ],
    )
}
pub fn run_statistics(width: u64, mode: Mode, reuse: bool) -> Result<()> {
    let mut g = Geometry::new([16, 8, 4], 1);
    g.table_pages = 2;
    g.reuse_views = reuse;
    run_cases(
        g,
        width,
        mode,
        &[
            ("storage_statistics", ownership::storage_statistics),
            (
                "statistics_count_retired_storage_without_draining",
                ownership::statistics_count_retired_storage_without_draining,
            ),
        ],
    )
}
pub fn run(name: &str, g: Geometry, width: u64, mode: Mode, random: u64) -> Result<()> {
    let compiled = design(&g, width, mode).build(&crate::root().join("work/marlin-suite/media"))?;
    for test in [basic::directed_media, basic::media_protocol] {
        let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
        let mut d = Store::new(&mut s, g.clone(), mode, 20261110, "")?;
        test(&mut d, &mut s);
        d.memory.borrow_mut().report(true);
    }
    let mut s = Sim::new(compiled.session(None)?, &[("clk", 10_000, 0)]);
    let mut d = Store::new(&mut s, g, mode, 20261110, "")?;
    basic::randomized_media(&mut d, &mut s, random);
    d.memory.borrow_mut().report(true);
    println!("media/{name}/{width}/{mode:?}: 3 groups, {random} seeded cases");
    Ok(())
}
pub fn empty(d: &Store) -> Value {
    json!({"block_chunks":d.geometry.block_chunks,"chunk_frames":d.geometry.chunk_frames,"chunk_limit":d.geometry.slots,"samples":[],"free_chunks":[],"views":[]})
}
pub fn completed(d: &mut Store, s: &mut Sim<'_>, command: Value) -> Value {
    let before = d.memory.borrow().sample_transactions;
    let result = d.command(s, &command, true, true, true);
    assert_eq!(
        result["status"],
        json!({"kind":"completed"}),
        "{command}: {result}"
    );
    if command["kind"] != "append" && command["kind"] != "read" {
        assert_eq!(
            d.memory.borrow().sample_transactions,
            before,
            "metadata operation touched sample storage"
        );
    }
    result
}
pub fn read_equals(
    d: &mut Store,
    s: &mut Sim<'_>,
    view: u64,
    values: &[i32],
    position: u64,
    count: Option<u64>,
) {
    assert!(!values.is_empty());
    let count = count.unwrap_or(values.len() as u64 * 2 + 3);
    let result = completed(
        d,
        s,
        json!({"kind":"read","view":view,"position":position,"length":values.len(),"frames":count}),
    );
    assert_eq!(
        result["value"],
        json!({"kind":"samples","value":(0..count).map(|i| values[((position+i)%values.len() as u64) as usize]).collect::<Vec<_>>()})
    );
}
