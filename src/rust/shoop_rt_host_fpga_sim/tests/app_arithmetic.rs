#![cfg(all(feature = "sim", target_os = "linux"))]
mod support;
use rt_audio_math::{Q824, Sample};
use rt_fpga_adapter::{image::*, protocol};
use rt_fpga_sim::{Simulator, runtime, value_words};
use shoop_wasm_test_support::shoop_test;
fn base() -> Image {
    let case: rt_backend_testbench::spec::Scenario = serde_json::from_str(include_str!(
        "../../rt_fpga_sim/fixtures/instant-gains.json"
    ))
    .unwrap();
    rt_backend_testbench::rtl::compile(&case).unwrap()
}
#[shoop_test]
fn arithmetic_jobs_match_production_q824_gain_balance_and_complement() {
    use shoop_rt_engine::dsp::{Gains, Ramp as NativeRamp};
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let runtime = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&runtime, None, true).unwrap();
    let mut image = base();
    image.buffers = 9;
    image.input_ports = 1;
    image.output_ports = 2;
    image.initial_state.memory = vec![
        Value::Ramp(Ramp {
            start: 20_000_003,
            target: 7_654_321,
            duration: 97,
            elapsed: 0,
        }),
        Value::Ramp(Ramp {
            start: -8_000_001,
            target: 11_111_111,
            duration: 55,
            elapsed: 0,
        }),
        Value::Ramp(Ramp {
            start: Q824::ONE.0,
            target: 3_456_789,
            duration: 144,
            elapsed: 0,
        }),
    ];
    image.program.instructions = vec![
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessConstant {
            value: Q824::ONE.0,
            destination: 1,
        },
        Instruction::ProcessGain {
            ramp: 0,
            source: 1,
            destination: 2,
        },
        Instruction::ProcessGain {
            ramp: 1,
            source: 1,
            destination: 3,
        },
        Instruction::ProcessGain {
            ramp: 2,
            source: 1,
            destination: 4,
        },
        Instruction::ProcessArithmetic {
            left: 2,
            right: 4,
            destination: 5,
            mode: ArithmeticMode::Multiply,
        },
        Instruction::ProcessArithmetic {
            left: 5,
            right: 3,
            destination: 6,
            mode: ArithmeticMode::LeftBalance,
        },
        Instruction::ProcessArithmetic {
            left: 5,
            right: 3,
            destination: 7,
            mode: ArithmeticMode::RightBalance,
        },
        Instruction::ProcessArithmetic {
            left: 0,
            right: 6,
            destination: 8,
            mode: ArithmeticMode::Multiply,
        },
        Instruction::ProcessOutput { source: 8, port: 0 },
        Instruction::ProcessOutput { source: 7, port: 1 },
    ];
    image.program.jobs_per_quantum = image.program.instructions.len() as u32;
    sim.load(&image).unwrap();
    let mut gains = Gains::<Q824>::new(32);
    gains.gain = NativeRamp::at(Q824(20_000_003));
    gains.gain.set(Q824(7_654_321), 97);
    gains.balance = NativeRamp::at(Q824(-8_000_001));
    gains.balance.set(Q824(11_111_111), 55);
    let mut scale = NativeRamp::at(Q824::ONE);
    scale.set(Q824(3_456_789), 144);
    for tick in 0..8 {
        let input = (0..32)
            .map(|i| match (i + tick) % 4 {
                0 => i32::MIN,
                1 => i32::MAX,
                2 => -19_876_543,
                _ => 31,
            })
            .collect::<Vec<_>>();
        gains.fill(32, &mut scale);
        let output = sim.run(&image, &[input.clone()]).unwrap();
        assert_eq!(
            output[0],
            input
                .iter()
                .zip(gains.channel(0, true))
                .map(|(&s, &g)| (Q824(s) * g).0)
                .collect::<Vec<_>>(),
            "tick {tick}"
        );
        assert_eq!(
            output[1],
            gains
                .channel(1, true)
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>(),
            "tick {tick}"
        );
    }
    let mut complement = base();
    complement.buffers = 2;
    complement.program.instructions = vec![
        Instruction::ProcessInput {
            port: 0,
            destination: 0,
        },
        Instruction::ProcessArithmetic {
            left: 0,
            right: 0,
            destination: 1,
            mode: ArithmeticMode::OneMinus,
        },
        Instruction::ProcessOutput { source: 1, port: 0 },
    ];
    complement.program.jobs_per_quantum = 3;
    complement.initial_state.memory.clear();
    sim.load(&complement).unwrap();
    let input = (0..32).map(|i| i * 513_511).collect::<Vec<_>>();
    assert_eq!(
        sim.run(&complement, &[input.clone()]).unwrap()[0],
        input
            .iter()
            .map(|&s| (Q824::ONE - Q824(s)).0)
            .collect::<Vec<_>>()
    );
}
#[shoop_test]
fn smoothing_jobs_are_bit_exact_with_production_renderer_on_short_loops_and_restart_changes() {
    use rt_runtime::{Config, Frame, Quantum, Revision, resource::*};
    use shoop_loops::{
        LoopId, Request, Sequence, Timing, Transition,
        audio::{AudioRenderer, Playback, Smoothing},
    };
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let runtime = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&runtime, None, true).unwrap();
    for period in [32usize, 64, 192] {
        let mut image = base();
        image.buffers = 2;
        image.gains.clear();
        image.initial_state.memory = vec![Value::Word(1 | (144u64 << 32))];
        image.transition.description.loops = vec![LoopSpec {
            id: Identity {
                slot: 0,
                generation: 1,
            },
            kind: "primitive".into(),
            length: period as u64,
            sync_source: None,
            repeat_sync: false,
            timeline: None,
        }];
        image.transition.dependency_order = vec![Identity {
            slot: 0,
            generation: 1,
        }];
        image.program.instructions = vec![
            Instruction::ProcessInput {
                port: 0,
                destination: 0,
            },
            Instruction::ProcessSmoothing {
                loop_id: Identity {
                    slot: 0,
                    generation: 1,
                },
                control: 0,
                source: 0,
            },
            Instruction::ProcessOutput { source: 1, port: 0 },
        ];
        image.program.jobs_per_quantum = 3;
        sim.load(&image).unwrap();
        let config = Config {
            max_frames: 32,
            resource_pages: 32,
            page_bytes: 128,
            ..Default::default()
        }
        .prepare()
        .unwrap();
        let mut store =
            ResourceStore::<Q824>::new_numeric(config, Quantum::new(32).unwrap()).unwrap();
        let resource = store.create().unwrap();
        let samples = (0..period)
            .map(|i| {
                Q824(match i % 4 {
                    0 => i32::MIN,
                    1 => i32::MAX,
                    2 => -12_345_678,
                    _ => 19_876_543,
                })
            })
            .collect::<Vec<_>>();
        let bytes = samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<_>>();
        let upload = store
            .begin_upload(
                resource,
                Revision(0),
                Description {
                    format: Format::Audio { channels: 1 },
                    frames: period as u64,
                    bytes: bytes.len(),
                },
            )
            .unwrap();
        for (page, bytes) in bytes.chunks(128).enumerate() {
            store.begin_callback();
            store.write_upload(upload, page * 128, bytes).unwrap();
        }
        store.finish_upload(upload).unwrap();
        store.commit(upload).unwrap();
        let snapshot = store.snapshot(resource, Frame(0)).unwrap();
        let index = store.read_index(snapshot.id).unwrap();
        let mut renderer =
            AudioRenderer::prepare(1, 32, Smoothing::LinearRestart { frames: 144 }).unwrap();
        let mut position = 0;
        let mut playing = true;
        let mut restart = true;
        for tick in 0..16 {
            if tick == 0 || tick == 6 || tick == 10 {
                let transition = if tick == 6 {
                    playing = false;
                    Transition::Stop
                } else {
                    playing = true;
                    position = 0;
                    restart = true;
                    Transition::Play {
                        position: 0,
                        retrigger: false,
                    }
                };
                sim.command(
                    4,
                    0,
                    protocol::transition(Request {
                        target: LoopId::from_parts(0, 1).unwrap(),
                        sequence: Sequence(tick + 1),
                        at: Frame(tick * 32),
                        timing: Timing::At,
                        transition,
                    })
                    .unwrap(),
                )
                .unwrap();
            }
            if tick == 3 {
                renderer.set_smoothing(Smoothing::LinearRestart { frames: 3 });
                sim.state(0, Some(value_words(Value::Word(1 | (3u64 << 32)))), false)
                    .unwrap();
            }
            let input = samples[position..position + 32]
                .iter()
                .map(|s| s.0)
                .collect::<Vec<_>>();
            store.begin_callback();
            let mut expected = [Q824::ZERO; 32];
            renderer
                .render(
                    &mut store.render_io(),
                    Playback {
                        source: playing.then_some(index),
                        position: position as u64,
                        gain: Q824::ONE,
                        balance: Q824::ZERO,
                        discontinuity: restart,
                    },
                    &mut [&mut expected],
                )
                .unwrap();
            let output = sim.run(&image, &[input]).unwrap();
            assert_eq!(
                output[0],
                expected.map(|s| s.0),
                "period {period}, tick {tick}"
            );
            if playing {
                position += 32;
                restart = position == period;
                if restart {
                    position = 0;
                }
            } else {
                restart = false;
            }
        }
    }
}

#[shoop_test]
fn repeat_policy_updates_the_authoritative_tp_row_and_checks_generation() {
    use rt_fpga_adapter::compiler::transition_image;
    use rt_fpga_sim::mono;
    let cache = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../work/marlin-app");
    let runtime = runtime(&cache).unwrap();
    let mut sim = Simulator::new(&runtime, None, true).unwrap();
    let mut image = base();
    let topology = mono::topology().unwrap();
    image.transition = transition_image(&topology, 32, |_| false).unwrap();
    sim.load(&image).unwrap();
    let mut native =
        shoop_loops::Resolver::prepare(topology, rt_runtime::Quantum::new(32).unwrap()).unwrap();
    for enabled in [true, false, true] {
        native.set_repeat_sync(mono::loop_id(), enabled).unwrap();
        let mut data = [0; 20];
        data[0] = 1;
        data[1] = u32::from(enabled);
        sim.command(17, 0, data).unwrap();
        let observed = protocol::state(&sim.command(12, 0, [0; 20]).unwrap(), 0).unwrap();
        assert_eq!(
            observed.repeat_sync,
            native.state(mono::loop_id()).unwrap().repeat_sync
        );
    }
    let mut data = [0; 20];
    data[0] = 2;
    assert_eq!(sim.command_raw(17, 0, data).unwrap().0, 3);
    assert!(
        protocol::state(&sim.command(12, 0, [0; 20]).unwrap(), 0)
            .unwrap()
            .repeat_sync
    );
    sim.command(16, 0, [0; 20]).unwrap();
    data[0] = 1;
    assert_eq!(sim.command_raw(17, 0, data).unwrap().0, 5);
}
