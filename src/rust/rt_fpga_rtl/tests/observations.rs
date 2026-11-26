#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{
    bits::Bits,
    model::{Design, Probe},
    sim::Sim,
};
use shoop_wasm_test_support::shoop_test;

fn run(inject: bool, test: impl FnOnce(&mut Sim<'_>)) {
    let root = rt_fpga_rtl::root();
    let mut design = Design::new(
        "fifo_observations",
        vec![root.join("fpga/host_bridge/rtl/fifo_observations.sv")],
    );
    if inject {
        design = design.probes(vec![
            Probe::inject("failure_value"),
            Probe::inject("trace_lost"),
        ]);
    }
    let compiled = design.build(&root.join("work/fifo-observations")).unwrap();
    let mut s = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
    reset(&mut s);
    test(&mut s);
}
fn reset(s: &mut Sim<'_>) {
    s.model.set_all(&[
        ("reset", 1),
        ("epoch", 1),
        ("revision", 17),
        ("gain_valid", 0),
        ("event_valid", 0),
        ("cfg_valid", 0),
        ("read_valid", 0),
        ("trace_ready", 0),
    ]);
    s.idle(2);
    s.model.set("reset", 0);
    s.idle(1);
}
fn configure(s: &mut Sim<'_>, address: u64, value: Option<&Bits>, error: bool) -> Bits {
    s.model.set_all(&[
        ("cfg_valid", 1),
        ("cfg_write", u64::from(value.is_some())),
        ("cfg_address", address),
    ]);
    if let Some(v) = value {
        s.model.set_bits("cfg_write_data", &v.resized(129));
    }
    s.model.eval();
    assert_eq!(s.model.get("cfg_ready"), 1);
    s.idle(1);
    assert_eq!(s.model.get("cfg_response_valid"), 1);
    assert_eq!(s.model.get("cfg_response_error"), u64::from(error));
    let data = s.model.bits("cfg_read_data");
    s.model.set("cfg_valid", 0);
    s.idle(1);
    data
}
fn initialize(s: &mut Sim<'_>, slot: u64, gain: u32) {
    for word in 0..3 {
        configure(
            s,
            slot * 3 + word,
            Some(&Bits::from_u64(if word == 0 { u64::from(gain) } else { 0 })),
            false,
        );
    }
}
fn read(s: &mut Sim<'_>, slot: u64, error: bool) -> Bits {
    s.model.set_all(&[("read_valid", 1), ("read_slot", slot)]);
    s.model.eval();
    assert_eq!(s.model.get("read_ready"), 1);
    s.idle(1);
    assert_eq!(s.model.get("response_valid"), 1);
    assert_eq!(s.model.get("response_error"), u64::from(error));
    let data = s.model.bits("read_data");
    s.model.set("read_valid", 0);
    s.idle(1);
    data
}
fn event(s: &mut Sim<'_>, kind: u64, outcome: u64, slot: u64, frame: u64) {
    let mut bits = Bits::zero(256);
    bits.set(0, 8, kind)
        .set(16, 8, outcome)
        .set(32, 32, slot)
        .set(192, 64, frame);
    s.model.set_bits("event_data", &bits);
    s.model.set("event_valid", 1);
}

#[shoop_test]
fn all_128_rows_update_with_trace_full_and_execution_wins_over_configuration_and_reads() {
    run(false, |s| {
        for slot in 0..128 {
            initialize(s, slot, 1.0f32.to_bits());
        }
        s.model.set_all(&[
            ("cfg_valid", 1),
            ("cfg_write", 1),
            ("cfg_address", 0),
            ("read_valid", 1),
            ("read_slot", 0),
        ]);
        s.model.set_bits(
            "cfg_write_data",
            &Bits::from_u64(u64::from(2.0f32.to_bits())),
        );
        let base = 0.3f32.to_bits();
        for slot in 0..128 {
            event(s, 2, 0, slot, 0xfedc_ba98_0000_0000 + slot);
            s.model.set_all(&[
                ("gain_valid", 1),
                ("gain_slot", 127 - slot),
                ("gain_bits", u64::from(base) + slot),
            ]);
            s.model.eval();
            assert_eq!(s.model.get("cfg_ready"), 0);
            assert_eq!(s.model.get("read_ready"), 0);
            s.idle(1);
            assert_eq!(s.model.get("cfg_response_valid"), 0);
            assert_eq!(s.model.get("response_valid"), 0);
        }
        s.model
            .set_all(&[("cfg_valid", 0), ("read_valid", 0), ("gain_valid", 0)]);
        for _ in 0..3 {
            for slot in 0..128 {
                event(s, 3, 3, slot, 99);
                s.idle(1);
            }
        }
        s.model.set("event_valid", 0);
        assert_eq!(s.model.get("trace_valid"), 1);
        assert_eq!(s.model.get("trace_lost"), 511);
        let trace = s.model.bits("trace_data");
        s.model.set_all(&[("epoch", 2), ("revision", 18)]);
        s.model.eval();
        assert_eq!(s.model.get("trace_epoch"), 1);
        assert_eq!(s.model.get("trace_revision"), 17);
        assert_eq!(trace.get(0, 8), 2);
        assert_eq!(trace.get(32, 32), 0);
        for slot in 0..128 {
            let data = read(s, slot, false);
            assert_eq!(data.get(0, 32), u64::from(base) + 127 - slot);
            assert_eq!(data.get(32, 32), 3);
            assert_eq!(data.get(64, 64), 0xfedc_ba98_0000_0000 + slot);
            assert_eq!(data.get(128, 1), 1);
            assert_eq!(s.model.bits("trace_data"), trace);
            assert_eq!(s.model.get("trace_epoch"), 1);
            assert_eq!(s.model.get("trace_revision"), 17);
        }
        s.model.set("trace_ready", 1);
        event(s, 2, 0, 5, 777);
        s.idle(1);
        s.model.set_all(&[("event_valid", 0), ("trace_ready", 0)]);
        assert_eq!(s.model.get("trace_valid"), 1);
        assert_eq!(s.model.get("trace_lost"), 511);
        assert_eq!(s.model.bits("trace_data").get(192, 64), 777);
        assert_eq!(s.model.get("trace_epoch"), 2);
        assert_eq!(s.model.get("trace_revision"), 18);
        assert_eq!(read(s, 5, false).get(64, 64), 777);
        assert_eq!(s.model.get("faulted"), 0);
    });
}

#[shoop_test]
fn failure_and_optional_loss_counters_saturate_at_their_actual_32_and_64_bit_limits() {
    run(true, |s| {
        initialize(s, 0, 1.0f32.to_bits());
        s.model.set_all(&[
            ("probe_failure_value_force", 1),
            ("probe_failure_value_value", u64::from(u32::MAX) - 1),
        ]);
        event(s, 3, 3, 0, 0);
        s.idle(1);
        s.model.set("probe_failure_value_force", 0);
        s.idle(1);
        s.model.set("event_valid", 0);
        assert_eq!(read(s, 0, false).get(32, 32), u64::from(u32::MAX));
        s.model.set_all(&[
            ("probe_trace_lost_force", 1),
            ("probe_trace_lost_value", u64::MAX - 1),
        ]);
        event(s, 0, 0, u64::from(u32::MAX), 0);
        s.idle(1);
        s.model.set("probe_trace_lost_force", 0);
        s.idle(1);
        s.model.set("event_valid", 0);
        assert_eq!(s.model.get("trace_lost"), u64::MAX);
        assert_eq!(s.model.get("faulted"), 0);
        assert_eq!(read(s, 0, false).get(32, 32), u64::from(u32::MAX));
    });
}

#[shoop_test]
fn new_lease_validation_bounds_reset_and_required_source_faults_never_alias_other_rows() {
    run(false, |s| {
        initialize(s, 0, 1.0f32.to_bits());
        initialize(s, 127, 0.5f32.to_bits());
        let original = read(s, 0, false);
        for (address, value) in [
            (0, u64::from(f32::INFINITY.to_bits())),
            (0, u64::from(f32::NAN.to_bits())),
            (0, 1 << 32),
            (1, 77),
            (2, 1),
            (384, 0),
            (512, 0),
        ] {
            configure(s, address, Some(&Bits::from_u64(value)), true);
            assert_eq!(read(s, 0, false), original);
        }
        let mut upper = Bits::zero(129);
        upper.set(128, 1, 1);
        configure(s, 0, Some(&upper), true);
        assert_eq!(read(s, 0, false), original);
        assert_eq!(read(s, 128, true), Bits::zero(129));
        assert_eq!(read(s, 127, false).get(0, 32), u64::from(0.5f32.to_bits()));
        event(s, 3, 2, u64::from(u32::MAX), 0);
        s.idle(1);
        s.model.set("event_valid", 0);
        assert_eq!(
            s.model.get("faulted"),
            0,
            "nonfailure outcomes are optional trace only"
        );
        event(s, 2, 0, 128, 0);
        s.model.eval();
        assert_eq!(s.model.get("faulted"), 1);
        assert_eq!(s.model.get("fault_latched"), 0);
        s.idle(1);
        assert_eq!(s.model.get("fault_latched"), 1);
        s.model.set("event_valid", 0);
        s.idle(1);
        assert_eq!(s.model.get("faulted"), 1);
        reset(s);
        assert_eq!(s.model.get("trace_valid"), 0);
        assert_eq!(s.model.get("trace_lost"), 0);
        assert_eq!(
            read(s, 0, false),
            original,
            "reset invalidates streams, not the distributed RAM"
        );
        s.model.set_all(&[
            ("gain_valid", 1),
            ("gain_slot", 0),
            ("gain_bits", u64::from(f32::NEG_INFINITY.to_bits())),
        ]);
        s.model.eval();
        assert_eq!(s.model.get("faulted"), 1);
        s.idle(1);
        reset(s);
        assert_eq!(read(s, 0, false), original);
    });
}
