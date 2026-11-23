#[path = "support/mono.rs"]
mod mono;
use rt_fpga_compiler::{compile, CompileContext};
use rt_fpga_compiler::{CompileError, Identity, ProgramLimits, Resource};
use shoop_wasm_test_support::shoop_test;

fn row(slot: u32, source: Option<u32>) -> mono::MonoLoop {
    mono::MonoLoop {
        id: Identity {
            slot,
            generation: 3,
        },
        source,
        input: true,
        output: true,
        gain: 0.5,
        repeat_sync: false,
    }
}

#[shoop_test]
fn graph_orders_sync_independently_of_audio_without_a_profile_loop_limit() {
    for quantum in [1, 16, 32, 64, 128] {
        let description = mono::session(
            &((0..64)
                .rev()
                .map(|i| row(i, (i > 0).then(|| i - 1)))
                .collect::<Vec<_>>()),
        );
        let program = compile(
            &description,
            CompileContext {
                quantum,
                limits: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(
            program.sync_pass.iter().map(|i| i.slot).collect::<Vec<_>>(),
            (0..64).collect::<Vec<_>>()
        );
        assert_eq!(program.loops[0].id.slot, 0);
    }
    compile(
        &mono::session(&(vec![row(0, Some(0))])),
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
    )
    .unwrap();
}

#[shoop_test]
fn invalid_graphs_fail_before_target_preparation() {
    for loops in [
        vec![row(0, Some(1))],
        vec![row(0, Some(1)), row(1, Some(0))],
        vec![row(0, None), row(0, None)],
        vec![mono::MonoLoop {
            gain: f32::NAN,
            ..row(0, None)
        }],
        vec![mono::MonoLoop {
            id: Identity {
                slot: 0,
                generation: 0,
            },
            ..row(0, None)
        }],
    ] {
        assert_eq!(
            compile(
                &mono::session(&(loops)),
                CompileContext {
                    quantum: 16,
                    limits: Default::default()
                }
            ),
            Err(CompileError::InvalidProgram)
        );
    }
    assert_eq!(
        compile(
            &mono::session(&(vec![row(0, None)])),
            CompileContext {
                quantum: 3,
                limits: Default::default()
            }
        ),
        Err(CompileError::InvalidProgram)
    );
}

#[shoop_test]
fn compiler_checks_each_target_resource_and_accepts_the_exact_fit() {
    let graph = mono::session(&((0..40).map(|i| row(i + 1000, None)).collect::<Vec<_>>()));
    let program = compile(
        &graph,
        CompileContext {
            quantum: 32,
            limits: Default::default(),
        },
    )
    .unwrap();
    let exact = ProgramLimits {
        instructions: (program.sync_pass.len() + program.work_pass.len()) as u32,
        state_values: program.initial_state.len() as u32,
        loop_slots: 40,
        jobs: program.resources.queue_capacity,
        buffers: program.resources.buffers,
        history_slots: 0,
        input_ports: 1,
        output_ports: 2,
        quantum: 32,
    };
    compile(
        &graph,
        CompileContext {
            quantum: 32,
            limits: exact,
        },
    )
    .unwrap();
    for (resource, limits) in [
        (
            Resource::Instructions,
            ProgramLimits {
                instructions: exact.instructions - 1,
                ..exact
            },
        ),
        (
            Resource::State,
            ProgramLimits {
                state_values: exact.state_values - 1,
                ..exact
            },
        ),
        (
            Resource::Loops,
            ProgramLimits {
                loop_slots: 39,
                ..exact
            },
        ),
        (
            Resource::Jobs,
            ProgramLimits {
                jobs: exact.jobs - 1,
                ..exact
            },
        ),
        (
            Resource::Buffers,
            ProgramLimits {
                buffers: exact.buffers - 1,
                ..exact
            },
        ),
        (
            Resource::Inputs,
            ProgramLimits {
                input_ports: 0,
                ..exact
            },
        ),
        (
            Resource::Outputs,
            ProgramLimits {
                output_ports: 1,
                ..exact
            },
        ),
        (
            Resource::Quantum,
            ProgramLimits {
                quantum: 16,
                ..exact
            },
        ),
    ] {
        assert_eq!(
            compile(
                &graph,
                CompileContext {
                    quantum: 32,
                    limits: limits
                }
            ),
            Err(CompileError::Capacity(resource))
        );
    }
    assert!(program.resources.queue_capacity > 0);
    assert!(program.resources.buffers < 4 * graph.loops.len() as u32);
    assert_eq!(program.resources.history_slots, 0);
}

#[shoop_test]
fn full_graph_views_report_unsupported_features_without_flattening_them_away() {
    use rt_fpga_compiler::Feature;
    use shoop_graph::*;
    let fixture = mono::session(&(vec![row(123, None)]));
    let initial = fixture.clone();
    let context = CompileContext {
        quantum: 16,
        limits: ProgramLimits::default(),
    };
    assert_eq!(
        compile(&initial, context).unwrap(),
        compile(
            &fixture,
            CompileContext {
                quantum: 16,
                limits: Default::default()
            }
        )
        .unwrap()
    );
    let owner = Owner::Strip(initial.strips[0].id);
    let cases = [Feature::Midi, Feature::Processor, Feature::Composite];
    for feature in cases {
        let mut graph = initial.clone();
        let object = match feature {
            Feature::Midi => {
                graph.strips[0].layout = ChannelLayout::Direct {
                    audio: 1,
                    midi: true,
                };
                owner
            }
            Feature::Processor => {
                graph.strips[0].processor = Some(ProcessorView {
                    type_id: "any processor".into(),
                    parameters: vec![0.4].into(),
                });
                owner
            }
            Feature::Composite => {
                graph.loops[0].composite = Some(CompositeView {
                    kind: CompositeKind::Regular,
                    version: 1,
                    unit_frames: 16,
                    iterations: 1,
                    sample_rate: 48000,
                    quantum: 16,
                    spans: vec![].into(),
                });
                Owner::Loop(graph.loops[0].id)
            }
            _ => unreachable!(),
        };
        assert_eq!(
            compile(&graph, context),
            Err(CompileError::Unsupported { object, feature })
        );
        assert_eq!(
            Graph::decode(&Graph::capture(&graph).encode().unwrap()).unwrap(),
            graph
        );
    }
    let mut invalid = initial.clone();
    invalid.connections[0].source.index = 900;
    assert_eq!(
        compile(&invalid, context),
        Err(CompileError::InvalidProgram)
    );
    let mut midi = initial.clone();
    let source = PortId {
        owner: Owner::Host,
        signal: SignalType::Midi,
        role: PortRole::Input,
        index: 0,
    };
    let destination = PortId {
        owner: Owner::Global,
        signal: SignalType::Midi,
        role: PortRole::Control,
        index: 0,
    };
    for id in [source, destination] {
        midi.ports.push(PortView {
            id,
            channel: 0,
            name: "MIDI".into(),
        });
    }
    midi.connections.push(Connection {
        source,
        destination,
    });
    assert_eq!(
        compile(&midi, context),
        Err(CompileError::Unsupported {
            object: Owner::Global,
            feature: Feature::Midi
        })
    );
}
