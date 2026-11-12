#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_fpga_sim::{Simulator, mono_trace, runtime};
use shoop_wasm_test_support::shoop_test;
#[shoop_test]
fn queued_loop_replays_cover_exports_partial_reads_eof_retirement_and_storage_reuse() {
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    for source in [
        include_str!("../fixtures/mono-loop.json"),
        include_str!("../fixtures/mono-empty.json"),
        include_str!("../fixtures/mono-pressure.json"),
        include_str!("../fixtures/mono-capacity.json"),
        include_str!("../fixtures/mono-media-roles.json"),
    ] {
        let trace: mono_trace::Trace = serde_json::from_str(source).unwrap();
        let sim = Simulator::new(&rt, None, true).unwrap();
        let result = mono_trace::run(&trace, sim).unwrap_or_else(|e| panic!("{}: {e}", trace.name));
        assert_eq!(result.compilations, 1);
        if trace.name.contains("rerecord") {
            assert!(result.read_bytes >= 640);
            assert_eq!(result.gain_writes, 3);
        }
    }
}

#[shoop_test]
fn seeded_queued_transitions_use_hardware_state_without_specializing_the_program() {
    use mono_trace::{Action, Command, Step, Trace};
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    for seed in [7u32, 0x12345678] {
        let mut random = seed;
        let mut steps = vec![];
        for quantum in 0..48 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            let action = match random % 5 {
                0 => Action::Record,
                1 => Action::Stop,
                2 => Action::Play,
                _ => Action::Gain {
                    target: ((random >> 8) as i32 % 25_165_825) - 12_582_912,
                },
            };
            let input = (0..32)
                .map(|sample| {
                    ((quantum * 32 + sample) * 17 + 3) * if sample % 2 == 0 { 1 } else { -1 }
                })
                .collect();
            steps.push(Step {
                input,
                commands: vec![Command {
                    name: format!("command-{quantum}"),
                    at: None,
                    recovery: false,
                    action,
                }],
                hold_results: false,
            });
        }
        let trace = Trace {
            format_version: 1,
            name: format!("seed-{seed}"),
            command_slots: 32,
            receipt_slots: 16,
            chunk_limit: 128,
            steps,
        };
        let sim = Simulator::new(&rt, None, true).unwrap();
        let result = mono_trace::run(&trace, sim).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        assert_eq!(result.compilations, 1);
    }
}
