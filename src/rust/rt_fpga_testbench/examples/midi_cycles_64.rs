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

    let args: Vec<_> = std::env::args().collect();
    let option = |name: &str, default: usize| {
        args.windows(2)
            .find(|pair| pair[0] == name)
            .map_or(default, |pair| {
                pair[1]
                    .parse::<usize>()
                    .expect("numeric measurement option")
            })
    };
    let active_loops = option("--active-loops", 64);
    let input_events = option("--events-per-quantum", 1);
    assert!((1..=64).contains(&active_loops) && (1..=16).contains(&input_events));
    let idle_only = args.iter().any(|arg| arg == "--idle-only");
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
        eprintln!(
            "{label}: {cycles} clocks, {} output packets",
            messages.len()
        );
        rows.push(serde_json::json!({
            "operation": label, "cycles": cycles, "input_events": events.len(),
            "output_events": messages.len(), "microseconds_at_100mhz": cycles as f64 / 100.0,
            "microseconds_at_150mhz": cycles as f64 / 150.0
        }));
    };
    measure("64 stopped bindings, no input", &mut reference, &[]);
    if !idle_only {
        for slot in 0..active_loops {
            reference.command(slot, Operation::Record);
        }
        measure("begin recordings", &mut reference, &[]);
        for batch in 0..2u32 {
            let events: Vec<_> = (0..input_events as u32)
                .map(|i| event(i, &[0xb0, 1, (batch * input_events as u32 + i + 1) as u8]))
                .collect();
            measure("record CC messages", &mut reference, &events);
        }
        for slot in 0..active_loops {
            reference.command(slot, Operation::Stop);
        }
        measure("finish recordings", &mut reference, &[]);
        for slot in 0..active_loops {
            reference.command(
                slot,
                Operation::Play {
                    position: 0,
                    retrigger: true,
                },
            );
        }
        measure("begin playback", &mut reference, &[]);
        measure("playback, first CC window", &mut reference, &[]);
        measure("playback, second CC window", &mut reference, &[]);
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "scope": "MIDI service jobs only; SP, shared JP dispatch, audio and board DDR are excluded",
        "loops": 64, "active_loops": active_loops, "idle_only": idle_only, "destinations_available": 8, "destinations_bound": 4,
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
