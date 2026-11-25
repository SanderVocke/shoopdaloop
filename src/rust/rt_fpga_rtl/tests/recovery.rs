#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

fn build() -> rt_fpga_rtl::model::Compiled {
    let root = rt_fpga_rtl::root();
    Design::new(
        "host_recovery",
        vec![root.join("fpga/host_bridge/rtl/host_recovery.sv")],
    )
    .parameters(&[("TIMEOUT_CYCLES", 32), ("RESET_CYCLES", 4)])
    .build(&root.join("work/host-mailbox"))
    .unwrap()
}

#[shoop_test(no_wasm = "Native recovery RTL simulator")]
fn reset_waits_for_both_axi_directions_and_advances_epoch_once() {
    let compiled = build();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    sim.model.set_all(&[("reset", 1), ("seed_epoch", 7)]);
    sim.idle(2);
    sim.model.set("reset", 0);
    sim.model.set_all(&[("write_start", 1), ("read_start", 1)]);
    sim.idle(1);
    sim.model.set_all(&[
        ("write_start", 0),
        ("read_start", 0),
        ("quiescent", 1),
        ("request", 1),
    ]);
    sim.idle(1);
    sim.model.set("request", 0);
    sim.idle(3);
    assert_eq!(sim.model.get("quiesce"), 1);
    assert_eq!(sim.model.get("datapath_reset"), 0);
    sim.model.set("write_done", 1);
    sim.idle(1);
    sim.model.set("write_done", 0);
    sim.idle(3);
    assert_eq!(sim.model.get("datapath_reset"), 0);
    sim.model.set("read_done", 1);
    sim.idle(1);
    sim.model.set("read_done", 0);
    sim.idle(3);
    assert_eq!(sim.model.get("datapath_reset"), 1);
    assert_eq!(sim.model.get("epoch"), 8);
    sim.idle(8);
    assert_eq!(sim.model.get("state"), 3);
    assert_eq!(sim.model.get("quiesce"), 0);
    assert_eq!(sim.model.get("datapath_reset"), 0);
    assert_eq!(sim.model.get("epoch"), 8);
    sim.model.set("request", 1);
    sim.idle(1);
    sim.model.set("request", 0);
    sim.idle(10);
    assert_eq!(sim.model.get("state"), 3);
    assert_eq!(sim.model.get("epoch"), 9);
}

#[shoop_test(no_wasm = "Native recovery RTL simulator")]
fn a_transaction_during_reset_fails_closed_instead_of_reporting_a_safe_fence() {
    let compiled = build();
    let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
    sim.model.set_all(&[("reset", 1), ("seed_epoch", 1)]);
    sim.idle(2);
    sim.model
        .set_all(&[("reset", 0), ("quiescent", 1), ("request", 1)]);
    sim.idle(1);
    sim.model.set("request", 0);
    sim.idle(3);
    assert_eq!(sim.model.get("datapath_reset"), 1);
    sim.model.set("write_start", 1);
    sim.idle(1);
    sim.model.set("write_start", 0);
    sim.idle(10);
    assert_eq!(sim.model.get("state"), 4);
    assert_eq!(sim.model.get("quiesce"), 1);
}

#[shoop_test(no_wasm = "Native recovery RTL simulator")]
fn stuck_drain_and_bad_bus_accounting_never_reset_or_reuse_an_epoch() {
    let compiled = build();
    for mode in 0..4 {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10000, 0)]);
        sim.model.set_all(&[
            ("reset", 1),
            ("seed_epoch", if mode == 3 { u32::MAX.into() } else { 1 }),
        ]);
        sim.idle(2);
        sim.model.set("reset", 0);
        if mode == 1 {
            sim.model.set("read_start", 1);
            sim.idle(1);
            sim.model.set("read_start", 0);
        }
        if mode == 2 {
            sim.model.set("write_done", 1);
            sim.idle(1);
            sim.model.set("write_done", 0);
        }
        sim.model
            .set_all(&[("request", 1), ("quiescent", u64::from(mode != 0))]);
        sim.idle(1);
        sim.model.set("request", 0);
        for _ in 0..40 {
            sim.idle(1);
            assert_eq!(sim.model.get("datapath_reset"), 0);
        }
        assert_eq!(sim.model.get("state"), 4);
        assert_eq!(sim.model.get("quiesce"), 1);
        assert_eq!(
            sim.model.get("epoch"),
            if mode == 3 { u32::MAX.into() } else { 1 }
        );
    }
}
