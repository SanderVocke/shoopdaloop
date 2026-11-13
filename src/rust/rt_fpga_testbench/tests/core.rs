#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_testbench::{
    bits::Bits,
    model::{Design, Probe},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn exported_scopes_are_destroyed_in_their_own_model_context() {
    use rt_fpga_testbench::{drivers::endpoint, memory::Mode};
    let cache = rt_fpga_testbench::root().join("work/marlin-suite/core-contexts");
    let first = endpoint::design("axi_master", 32, Mode::Instant)
        .build(&cache)
        .unwrap();
    let second = endpoint::design("word_axi", 32, Mode::Instant)
        .build(&cache)
        .unwrap();
    let mut a = Sim::new(first.session(None).unwrap(), &[("clk", 10_000, 0)]);
    let mut b = Sim::new(second.session(None).unwrap(), &[("clk", 10_000, 0)]);
    a.model.set("reset", 1);
    a.idle(2);
    b.model.set("reset", 1);
    b.idle(2);
    drop(a);
    drop(first);
    b.idle(1);
    drop(b);
    drop(second);
}

#[shoop_test]
fn elaborated_widths_wide_inputs_probes_and_independent_models() {
    let root = rt_fpga_testbench::root();
    let cache = root.join("work/marlin-suite/core");
    for depth in [1, 7] {
        let sources = "sp_ram sp_job_fifo sp_engine state_processor"
            .split_whitespace()
            .map(|n| root.join(format!("fpga/state_processor/rtl/{n}.sv")))
            .collect();
        let design = Design::new("state_processor", sources)
            .parameters(&[
                ("PROGRAM_DEPTH", depth),
                ("STATE_DEPTH", depth),
                ("JOB_DEPTH", depth),
            ])
            .probes(vec![Probe::observe("engine.phase")])
            .build(&cache)
            .unwrap();
        let trace = cache.join(format!("depth-{depth}.vcd"));
        let mut sim = Sim::new(design.session(Some(&trace)).unwrap(), &[("clk", 10_000, 0)]);
        let mut independent = Sim::new(design.session(None).unwrap(), &[("clk", 10_000, 0)]);
        for model in [&mut sim.model, &mut independent.model] {
            model.set("reset", 1);
        }
        sim.idle(2);
        independent.idle(2);
        sim.model.set("reset", 0);
        independent.model.set("reset", 0);
        sim.idle(1);
        independent.idle(1);
        assert_eq!(sim.model.width("cfg_write_data"), 129);
        let mut value = Bits::zero(129);
        value
            .set(128, 1, 1)
            .set(96, 32, u64::from(u32::MAX))
            .set(0, 64, 0x8123456789abcdef);
        sim.model.set_all(&[
            ("cfg_valid", 1),
            ("cfg_write", 1),
            ("cfg_state", 1),
            ("cfg_address", 0),
        ]);
        sim.model.set_bits("cfg_write_data", &value);
        sim.idle(1);
        sim.model.set("cfg_valid", 0);
        sim.idle(1);
        sim.model.set_all(&[("cfg_valid", 1), ("cfg_write", 0)]);
        sim.idle(1);
        assert_eq!(
            sim.model.bits("cfg_read_data").array::<5>(),
            value.array::<5>()
        );
        assert_eq!(sim.model.get("engine.phase"), 0);
        independent
            .model
            .set_all(&[("cfg_valid", 1), ("cfg_write", 0), ("cfg_state", 1)]);
        independent.idle(1);
        assert_eq!(independent.model.bits("cfg_read_data").get(0, 64), 0);
        drop(sim);
        drop(independent);
        assert!(
            std::fs::read_to_string(&trace)
                .unwrap()
                .contains("$timescale 1ps")
        );
        std::fs::remove_file(trace).unwrap();
    }
}
