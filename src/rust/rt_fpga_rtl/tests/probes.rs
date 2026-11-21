#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{
    model::{Design, Probe},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test(no_wasm = "native RTL observation")]
fn indexed_array_probes_match_static_probes_without_advancing_clocks() {
    let root = rt_fpga_rtl::root();
    let design = Design::new(
        "probe_array",
        vec![root.join("src/rust/rt_fpga_rtl/tests/support/probe_array.sv")],
    )
    .probes(vec![
        Probe::array("words"),
        Probe::observe("words[0]"),
        Probe::observe("words[3]"),
    ]);
    let compiled = design.build(&root.join("work/rt-indexed-probes")).unwrap();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    sim.model.set("reset", 1);
    sim.idle(2);
    sim.model.set("reset", 0);
    let values = [0x1234_5678_9abc_def0, 1, u64::MAX, 0xfedc_ba98_7654_3210];
    for (index, value) in values.into_iter().enumerate() {
        sim.model.set_all(&[
            ("write_enable", 1),
            ("write_index", index as u64),
            ("write_data", value),
        ]);
        sim.idle(1);
    }
    sim.model.set("write_enable", 0);
    let before = (sim.cycles, sim.ps(), sim.model.get("ticks"));
    for _ in 0..3 {
        for (index, value) in values.into_iter().enumerate() {
            assert_eq!(sim.model.read_indexed("words", index as u64), value);
            assert_eq!(sim.model.get("words[0]"), values[0]);
            assert_eq!(sim.model.get("words[3]"), values[3]);
        }
    }
    assert_eq!(sim.model.read_indexed("words", 4), 0);
    assert_eq!((sim.cycles, sim.ps(), sim.model.get("ticks")), before);
}
