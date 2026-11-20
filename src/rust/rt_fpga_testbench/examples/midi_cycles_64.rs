#[cfg(feature = "sim")]
#[allow(dead_code, unused_imports)]
mod oracle {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../rt_fpga_reference/tests/support/midi_session.rs"
    ));
}

#[cfg(feature = "sim")]
fn main() {
    use oracle::*;
    use rt_fpga_testbench::suites::midi::runtime::{Driver, compile_limits};

    let mut limits = SessionLimits::session();
    limits.loops = 64;
    limits.store.blocks = 2432;
    limits.store.views = 256;
    limits.store.snapshots = 384;
    limits.store.retirements = 128;
    limits.input_events_per_quantum = 16;
    limits.history_frames = 128;
    let compiled = compile_limits(limits).unwrap();
    let mut rtl = Driver::new(&compiled, 64).unwrap();
    let mut reference = Fixture::with_limits(32, limits);
    let mut rows = Vec::new();
    let mut measure = |label: &str, reference: &mut Fixture, events: &[InputEvent]| {
        let start = reference.pipeline.state().frame();
        assert_eq!(reference.run(events).error, None);
        let records: Vec<_> = (0..64)
            .map(|i| reference.pipeline.state().loop_state(id(i)).unwrap().render)
            .collect();
        let before = rtl.sim.cycles;
        assert_eq!(rtl.process(start, 32, &records, events), 0);
        let cycles = rtl.sim.cycles - before;
        let messages = rtl.messages();
        assert_eq!(messages, std::mem::take(&mut reference.emitted));
        rows.push(serde_json::json!({
            "operation": label, "cycles": cycles, "input_events": events.len(),
            "output_events": messages.len(), "microseconds_at_100mhz": cycles as f64 / 100.0,
            "microseconds_at_150mhz": cycles as f64 / 150.0
        }));
    };
    measure("64 stopped bindings, no input", &mut reference, &[]);
    for slot in 0..64 {
        reference.command(slot, Operation::Record);
    }
    measure("begin 64 recordings", &mut reference, &[]);
    for batch in 0..2 {
        let events: Vec<_> = (0..16)
            .map(|i| event(i, &[0xb0, 1, (batch * 16 + i + 1) as u8]))
            .collect();
        measure("record 16 CC messages", &mut reference, &events);
    }
    for slot in 0..64 {
        reference.command(slot, Operation::Stop);
    }
    measure("finish 64 recordings", &mut reference, &[]);
    for slot in 0..64 {
        reference.command(
            slot,
            Operation::Play {
                position: 0,
                retrigger: true,
            },
        );
    }
    measure("begin playback of 64 loops", &mut reference, &[]);
    measure("playback of 64 loops, first CC window", &mut reference, &[]);
    measure(
        "playback of 64 loops, second CC window",
        &mut reference,
        &[],
    );
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "scope": "MIDI service jobs only; SP, shared JP dispatch, audio and board DDR are excluded",
        "loops": 64, "destinations_available": 8, "destinations_bound": 4,
        "quantum_frames": 32, "sample_rate": 48000,
        "admitted_input_events_per_quantum": 16, "history_retention_frames": 128,
        "store": {"block_events": 256, "blocks": 2432, "max_events": 8192,
            "views": 256, "snapshots": 384, "retirements": 128},
        "memory": "deterministic randomized word acknowledgements and backpressure from the conformance driver",
        "measurements": rows
    })).unwrap());
}

#[cfg(not(feature = "sim"))]
fn main() {
    panic!("Enable sim to measure MIDI RTL service cycles");
}
