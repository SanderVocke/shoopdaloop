#![cfg(all(feature = "sim", not(target_arch = "wasm32")))]
use rt_audio_math::Q824;
use rt_backend_testbench::{
    production, rtl,
    spec::{Action, Samples, Scenario},
};
use rt_fpga_sim::{Simulator, runtime, value_words};
use shoop_wasm_test_support::shoop_test;
use std::path::Path;

#[shoop_test]
fn integrated_gain_and_silence_with_stalls_and_independent_models() {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    std::fs::create_dir_all(&cache).unwrap();
    let rt = runtime(&cache).unwrap();
    let wave = cache.join("smoke.vcd");
    let mut sim = Simulator::new(&rt, Some(&wave), true).unwrap();
    let mut independent = Simulator::new(&rt, None, false).unwrap();
    for source in [
        include_str!("../fixtures/instant-gains.json"),
        include_str!("../../../../fpga/verification/scenarios/silence.json"),
    ] {
        let case: Scenario = serde_json::from_str(source).unwrap();
        let expected = production::run::<Q824>(&case).unwrap();
        let image = rtl::compile(&case).unwrap();
        sim.load(&image).unwrap();
        for (i, step) in case.steps.iter().enumerate() {
            for action in &step.actions {
                let Action::Gain { node, target, .. } = action else {
                    panic!("gain fixture")
                };
                for &address in &image.gains.iter().find(|b| b.id == *node).unwrap().ramps {
                    sim.state(
                        address,
                        Some(value_words(rtl::Value::Ramp(rtl::Ramp {
                            start: *target,
                            target: *target,
                            duration: 0,
                            elapsed: 0,
                        }))),
                        false,
                    )
                    .unwrap();
                }
            }
            assert_eq!(
                Samples::Q824(sim.run(&image, &step.inputs).unwrap()),
                expected.quanta[i].outputs
            );
            assert_eq!(sim.frame(), expected.quanta[i].next_frame);
            assert_eq!(independent.frame(), 0);
        }
    }
    assert!(sim.sample(3, 0, None).is_err());
    independent.timeout = 0;
    assert!(independent.command(0, 0, [0; 20]).is_err());
    sim.reset();
    assert_eq!(sim.frame(), 0);
    drop(sim);
    drop(independent);
    let vcd = std::fs::read_to_string(&wave).unwrap();
    std::fs::remove_file(wave).unwrap();
    let clock = vcd
        .lines()
        .find(|l| l.contains(" clk "))
        .unwrap()
        .split_whitespace()
        .nth(3)
        .unwrap();
    assert!(vcd.contains(&format!("0{clock}\n")));
    assert!(vcd.contains(&format!("1{clock}\n")));
}

#[shoop_test]
fn repeated_runtime_creation_uses_owned_trace_contexts_without_opening_files() {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    for _ in 0..3 {
        let rt = runtime(&cache).unwrap();
        let sim = Simulator::new(&rt, None, false).unwrap();
        assert_eq!(sim.frame(), 0);
    }
}

mod fifo {
    use marlin::{
        verilator::{VerilatedModelConfig, VerilatorRuntime, VerilatorRuntimeOptions, WideIn},
        verilog::prelude::*,
    };
    use shoop_wasm_test_support::shoop_test;
    use std::path::Path;
    #[verilog(src = "rtl/fifo_probe.sv", name = "probe_top")]
    struct Probe;
    fn tick(m: &mut Probe<'_>) {
        m.clk = 0;
        m.eval();
        m.clk = 1;
        m.eval();
        m.clk = 0;
        m.eval();
    }
    #[shoop_test]
    fn odd_depth_fifo_wrap_wide_words_and_backpressure() {
        let here = Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = here.join("../../..");
        let config = rt_fpga_sim::RuntimeConfig::development(&root.join("work/marlin-fifo"));
        rt_fpga_sim::check_verilator(&config.verilator).unwrap();
        let rt = VerilatorRuntime::new2(
            &config.artifacts,
            &[
                here.join("rtl/fifo_probe.sv"),
                root.join("fpga/audio_io/rtl/audio_command_fifo.sv"),
                root.join("fpga/state_processor/rtl/sp_ram.sv"),
            ],
            &[] as &[&Path],
            [],
            VerilatorRuntimeOptions {
                verilator_executable: config.verilator,
                build_jobs: config.jobs,
                ..Default::default()
            },
        )
        .unwrap();
        let mut m = rt
            .create_model::<Probe>(&VerilatedModelConfig::default())
            .unwrap();
        m.reset = 1;
        tick(&mut m);
        tick(&mut m);
        m.reset = 0;
        tick(&mut m);
        for ordinal in 0..20u32 {
            let mut words = core::array::from_fn::<_, 24, _>(|i| {
                ordinal.wrapping_mul(12345) ^ (i as u32).wrapping_mul(0x9e3779b9)
            });
            words[23] &= 0x3ff;
            assert_eq!(m.push_ready, 1);
            m.push_data = WideIn::new(words);
            m.push_valid = 1;
            tick(&mut m);
            m.push_valid = 0;
            for _ in 0..8 {
                if m.pop_valid != 0 {
                    break;
                }
                tick(&mut m);
            }
            assert_eq!(m.pop_valid, 1);
            assert_eq!(*m.pop_data.value(), words);
            for _ in 0..ordinal % 3 {
                tick(&mut m);
                assert_eq!(*m.pop_data.value(), words);
            }
            m.pop_ready = 1;
            tick(&mut m);
            m.pop_ready = 0;
            assert_eq!(m.count, 0);
        }
    }
}

#[shoop_test]
fn queued_chain_and_parallel_match_native_samples_completions_and_receipts() {
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    for source in [
        include_str!("../fixtures/chain.json"),
        include_str!("../fixtures/parallel.json"),
    ] {
        let scenario = serde_json::from_str(source).unwrap();
        let sim = Simulator::new(&rt, None, true).unwrap();
        let report = rt_fpga_sim::differential::run(&scenario, sim).unwrap();
        assert_eq!(report.samples, 320);
        assert_eq!(report.state, 2);
        assert!(report.gain_writes >= 8);
    }
}

#[shoop_test]
fn original_record_stop_play_case_preserves_audio_state_and_captured_media() {
    use rt_backend_testbench::spec::{Timing, Transition};
    use rt_fpga_sim::{get, put};
    let case: Scenario = serde_json::from_str(include_str!(
        "../../../../fpga/verification/scenarios/mono-record.json"
    ))
    .unwrap();
    let expected = production::run::<Q824>(&case).unwrap();
    let image = rtl::compile(&case).unwrap();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&rt, None, true).unwrap();
    sim.load(&image).unwrap();
    let mut requests =
        rt_fpga_adapter::transition_service::TransitionService::new(&image.transition).unwrap();
    let mut sequence = 0;
    for (i, step) in case.steps.iter().enumerate() {
        for action in &step.actions {
            let Action::Transition {
                target,
                at,
                timing: Timing::At,
                operation,
                ..
            } = action
            else {
                panic!("record fixture")
            };
            let mut words = [0; 20];
            let code = match operation {
                Transition::Stop => 0,
                Transition::Play { .. } => 1,
                Transition::Record => 2,
                Transition::Seek { .. } => 3,
            };
            put(&mut words, 34, 2, code);
            put(
                &mut words,
                64,
                64,
                u64::from(target.slot) | (u64::from(target.generation) << 32),
            );
            sequence += 1;
            put(&mut words, 128, 64, sequence);
            put(&mut words, 192, 64, *at);
            if let Transition::Play {
                position,
                retrigger,
            }
            | Transition::Seek {
                position,
                retrigger,
            } = operation
            {
                put(&mut words, 384, 64, *position);
                put(&mut words, 36, 1, u64::from(*retrigger));
            }
            use rt_fpga_adapter::looping::LoopDevice;
            let reply = requests
                .exchange(4, 0, words, |transfer| {
                    sim.tp(transfer.opcode, transfer.index, transfer.words)
                })
                .unwrap();
            assert_eq!(reply.error, 0);
        }
        assert_eq!(
            Samples::Q824(sim.run(&image, &step.inputs).unwrap()),
            expected.quanta[i].outputs,
            "quantum {i}"
        );
        assert_eq!(sim.frame(), expected.quanta[i].next_frame);
        for state in &expected.quanta[i].states {
            let raw = sim.command(12, state.id.slot, [0; 20]).unwrap();
            assert_eq!(get(&raw, 0, 1), 1);
            assert_eq!(get(&raw, 1, 32), u64::from(state.id.generation));
            assert_eq!(
                ["stopped", "playing", "recording"][get(&raw, 33, 2) as usize],
                state.mode
            );
            for (offset, value) in [
                (64, state.content_length),
                (128, state.position),
                (192, state.capture_frames),
                (256, state.cycles),
                (320, state.content_revision),
                (384, state.capture_revision),
            ] {
                assert_eq!(
                    get(&raw, offset, 64),
                    value,
                    "state field {offset} quantum {i}"
                );
            }
            assert_eq!(get(&raw, 35, 1) != 0, state.repeat_sync);
            assert_eq!(
                if get(&raw, 36, 1) != 0 {
                    Some(get(&raw, 512, 64))
                } else {
                    None
                },
                state.last_pulse
            );
        }
    }
    let frames = expected.media[0].frames as u32;
    let capture_view = image
        .program
        .instructions
        .iter()
        .find_map(|i| {
            if let rtl::Instruction::ProcessLogicalCapture { view, .. } = i {
                Some(*view)
            } else {
                None
            }
        })
        .unwrap();
    let words = sim.state(capture_view, None, false).unwrap();
    let view = u64::from(words[0]) | (u64::from(words[1]) << 32);
    assert_eq!(
        Samples::Q824(vec![sim.read_view(view, frames).unwrap()]),
        expected.media[0].samples
    );
}

#[shoop_test]
fn rtl_fault_is_latched_and_future_controls_receive_terminal_cancellation() {
    use rt_fpga_adapter::adapter::{Adapter, Node, Operation, Processor, Request};
    use rt_runtime::{
        Epoch, Error, Frame, NodeId,
        graph::{self, Catalog, GraphDescription},
        schedule::Action,
        transport::{Lane, Outcome},
    };
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-tests");
    let rt = runtime(&cache).unwrap();
    let sim = Simulator::new(&rt, None, true).unwrap();
    let id = NodeId::from_parts(0, 1).unwrap();
    let nodes = vec![Node::new(id, Processor::new(1, 32, Q824(16777216)))];
    let config = rt_fpga_sim::differential::config();
    let plan = graph::compile(
        &Catalog::new(config, &[nodes[0].spec()]).unwrap(),
        &GraphDescription {
            nodes: vec![id],
            connections: vec![],
        },
    )
    .unwrap();
    let (mut c, mut a) = Adapter::prepare(config, nodes, sim).unwrap();
    c.submit(Lane::Normal, Request::commit(Epoch(0), plan))
        .map_err(|e| e.error)
        .unwrap();
    a.service().unwrap();
    assert_eq!(c.try_completion().unwrap().outcome, Outcome::Applied);
    c.submit(
        Lane::Normal,
        Request::Schedule {
            at: Frame(64),
            action: Action::Processor {
                node: id,
                operation: Operation {
                    target: Q824(1),
                    duration: 0,
                },
            },
            payload: Box::new([]),
        },
    )
    .map_err(|e| e.error)
    .unwrap();
    a.device_mut()
        .state(0, Some([255, 0, 0, 0, 0]), true)
        .unwrap();
    assert_eq!(a.process(&[vec![1; 32]]), Err(Error::InvalidState));
    assert_eq!(a.fault(), Some(Error::InvalidState));
    assert_eq!(a.frame(), Frame(0));
    assert!(
        a.device()
            .last_error
            .as_ref()
            .unwrap()
            .contains("pipeline fault stage")
    );
    assert_eq!(c.try_execution().unwrap().result, Err(Error::Cancelled));
}

#[shoop_test]
fn changing_the_rtl_root_cannot_reuse_a_cached_model_with_older_source_timestamps() {
    use rt_fpga_sim::{RuntimeConfig, runtime_with_config};
    use std::fs::{File, FileTimes};
    use std::time::SystemTime;
    let root = tempfile::tempdir().unwrap();
    let mut config = RuntimeConfig::development(&root.path().join("cache"));
    {
        let runtime = runtime_with_config(&config).unwrap();
        let _model = Simulator::new(&runtime, None, false).unwrap();
    }
    fn copy_rtl(source: &Path, destination: &Path) {
        std::fs::create_dir_all(destination).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let to = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_rtl(&entry.path(), &to);
            } else if matches!(
                entry.path().extension().and_then(|s| s.to_str()),
                Some("sv" | "svh" | "vh")
            ) {
                std::fs::copy(entry.path(), &to).unwrap();
                File::open(to)
                    .unwrap()
                    .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
                    .unwrap();
            }
        }
    }
    let alternate = root.path().join("alternate-rtl");
    copy_rtl(&config.rtl_directory, &alternate);
    let broken = alternate.join("state_processor/rtl/sp_engine.sv");
    std::fs::write(&broken, "this is deliberately invalid RTL").unwrap();
    File::open(broken)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
        .unwrap();
    config.rtl_directory = alternate;
    let runtime = runtime_with_config(&config).unwrap();
    let error = Simulator::new(&runtime, None, false)
        .err()
        .expect("the selected invalid RTL must be compiled instead of reusing the previous model")
        .to_string();
    assert!(
        error.contains("verilator") || error.contains("Verilator"),
        "{error}"
    );
}
