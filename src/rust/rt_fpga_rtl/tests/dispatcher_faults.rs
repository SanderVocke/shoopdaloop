#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_rtl::{bits::Bits, host::registers::*, model::Design, sim::Sim};
use shoop_wasm_test_support::shoop_test;

#[shoop_test(no_wasm = "Native first-fault dispatcher simulation")]
fn dispatcher_retains_first_cause_context_and_simultaneous_sources_until_reset() {
    let root = rt_fpga_rtl::root();
    let rtl = root.join("fpga/host_bridge/rtl");
    let mut design = Design::new(
        "host_dispatch",
        vec![rtl.join("host_registers.sv"), rtl.join("host_dispatch.sv")],
    );
    design = design.parameters(&[
        ("STATE_DEPTH", 4),
        ("CONTROL_CYCLES", 16),
        ("WATCHDOG_CYCLES", 64),
    ]);
    let compiled = design.build(&root.join("work/dispatcher-faults")).unwrap();
    for expected in [
        FAULT_MEMORY,
        FAULT_AUDIO,
        FAULT_EVENT_OVERFLOW,
        FAULT_ABORT,
        FAULT_CONTROL_TIMEOUT,
        FAULT_WATCHDOG,
        FAULT_PIPELINE,
        FAULT_MEMORY | FAULT_AUDIO,
        FAULT_ABORT | FAULT_RECOVERY,
    ] {
        let mut sim = Sim::new(compiled.session(None).unwrap(), &[("clk", 10_000, 0)]);
        sim.model.set_all(&[
            ("reset", 1),
            ("epoch", 7),
            ("active_revision", 0x1234567887654321),
            ("sync_frame", 0x23456789abcdef01),
        ]);
        sim.idle(2);
        sim.model.set("reset", 0);
        sim.idle(1);
        let op = if expected == FAULT_CONTROL_TIMEOUT {
            OP_BEGIN_CONTROL
        } else {
            OP_RUN_QUANTUM
        };
        let mut words = [0; HOST_PAYLOAD_WORDS];
        if op == OP_BEGIN_CONTROL {
            words[0] = 0x87654321;
            words[1] = 0x12345678;
        }
        sim.model.set_bits("command_data", &Bits::from(words));
        sim.model.set_all(&[
            ("command_tag", 91),
            ("command_op", u64::from(op)),
            ("command_valid", 1),
        ]);
        assert_eq!(sim.model.get("command_ready"), 1);
        sim.idle(1);
        sim.model.set("command_valid", 0);
        if expected & FAULT_MEMORY != 0 {
            sim.model.set("memory_fault", 1);
        }
        if expected & FAULT_AUDIO != 0 {
            sim.model.set("audio_fault", 1);
        }
        if expected == FAULT_EVENT_OVERFLOW {
            sim.model.set("event_overflow", 1);
        }
        if expected & FAULT_ABORT != 0 {
            sim.model.set("host_abort", 1);
        }
        if expected & FAULT_RECOVERY != 0 {
            sim.model.set("recovery_quiesce", 1);
        }
        if expected == FAULT_PIPELINE {
            sim.model.set("run_ready", 1);
            sim.idle(1);
            sim.model
                .set_all(&[("done", 1), ("error_code", 77), ("error_stage", 2)]);
        }
        sim.idle(100);
        assert_eq!(sim.model.get("fault"), 1, "cause={expected}");
        assert_eq!(sim.model.get("fault_epoch"), 7);
        assert_eq!(sim.model.get("fault_cause"), u64::from(expected));
        assert_eq!(sim.model.get("fault_tag"), 91);
        assert_eq!(sim.model.get("fault_op"), u64::from(op));
        assert_eq!(sim.model.get("fault_revision"), 0x1234567887654321);
        assert_eq!(sim.model.get("fault_frame"), 0x23456789abcdef01);
        if expected == FAULT_CONTROL_TIMEOUT {
            assert_eq!(sim.model.get("fault_control_cycles"), 15);
        }
        if expected == FAULT_WATCHDOG {
            assert_eq!(sim.model.get("fault_cycles"), 64);
        }
        if expected == FAULT_PIPELINE {
            assert_eq!(sim.model.get("fault_detail") >> 8, 0x4d02);
        }
        sim.model.set_all(&[
            ("host_abort", 1),
            ("memory_fault", 1),
            ("active_revision", 0),
            ("sync_frame", 0),
            ("epoch", 8),
        ]);
        sim.idle(3);
        assert_eq!(sim.model.get("fault_cause"), u64::from(expected));
        assert_eq!(sim.model.get("fault_epoch"), 7);
        sim.model.set("reset", 1);
        sim.idle(1);
        assert_eq!(sim.model.get("fault_cause"), 0);
        assert_eq!(sim.model.get("fault_epoch"), 0);
    }
}
