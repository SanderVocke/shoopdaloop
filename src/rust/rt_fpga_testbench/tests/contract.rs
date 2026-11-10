use rt_fpga_reference::{
    compiler::{compile, Binding, Graph, Node, NodeId},
    fixed::{Fixed, Ramp},
    jobs::{BufferId, Job},
    state::{Address, Instruction, Program, Value},
    storage::{ChunkId, ViewId},
    transitions::OnWrap,
    Error,
};
use rt_fpga_testbench::{
    run_audio_case, run_case, schema, AudioCase, AudioStep, Document, ExecutionStage,
    ExecutionStatus, Payload, StateCase, StateSnapshot, StateStep, StateWrite,
};
use shoop_wasm_test_support::shoop_test;

fn ramp() -> Ramp {
    Ramp {
        start: Fixed(i32::MIN),
        target: Fixed(i32::MAX),
        duration: u32::MAX,
        elapsed: u32::MAX - 16,
    }
}

fn case() -> StateCase {
    StateCase {
        name: "capture/play and a saturating ramp".into(),
        program: Program {
            quantum: 32,
            jobs_per_quantum: 6,
            instructions: vec![
                Instruction::ProcessInput {
                    port: 0,
                    destination: BufferId(0),
                },
                Instruction::ProcessGain {
                    ramp: Address(0),
                    source: BufferId(0),
                    destination: BufferId(1),
                },
                Instruction::ProcessLoopCapture {
                    view: Address(1),
                    enabled: Address(2),
                    source: BufferId(1),
                },
                Instruction::ProcessLoopPlayback {
                    view: Address(1),
                    position: Address(3),
                    length: Address(4),
                    enabled: Address(5),
                    destination: BufferId(2),
                },
                Instruction::ProcessMix {
                    left: BufferId(1),
                    right: BufferId(2),
                    destination: BufferId(3),
                },
                Instruction::ProcessOutput {
                    source: BufferId(3),
                    port: 0,
                },
            ],
        },
        initial_state: StateSnapshot {
            memory: vec![
                Value::Ramp(ramp()),
                Value::Word(17),
                Value::Word(1),
                Value::Word(0),
                Value::Word(45),
                Value::Word(0),
            ],
        },
        queue_capacity: 6,
        steps: vec![
            StateStep { writes: vec![] },
            StateStep {
                writes: vec![
                    StateWrite {
                        address: Address(2),
                        value: Value::Word(0),
                    },
                    StateWrite {
                        address: Address(3),
                        value: Value::Word(40),
                    },
                    StateWrite {
                        address: Address(5),
                        value: Value::Word(1),
                    },
                ],
            },
        ],
    }
}

fn audio_case() -> AudioCase {
    AudioCase {
        name: "sample processing and persistent state".into(),
        program: Program {
            quantum: 3,
            jobs_per_quantum: 5,
            instructions: vec![
                Instruction::ProcessInput {
                    port: 0,
                    destination: BufferId(0),
                },
                Instruction::ProcessGain {
                    ramp: Address(0),
                    source: BufferId(0),
                    destination: BufferId(1),
                },
                Instruction::ProcessInput {
                    port: 1,
                    destination: BufferId(2),
                },
                Instruction::ProcessMix {
                    left: BufferId(1),
                    right: BufferId(2),
                    destination: BufferId(3),
                },
                Instruction::ProcessOutput {
                    source: BufferId(3),
                    port: 0,
                },
            ],
        },
        initial_state: StateSnapshot {
            memory: vec![Value::Ramp(Ramp::constant(Fixed(1 << 23)))],
        },
        queue_capacity: 8,
        buffers: 4,
        output_ports: 2,
        media: None,
        steps: vec![
            AudioStep {
                writes: vec![],
                inputs: vec![vec![Fixed(1), Fixed(-3), Fixed(i32::MAX)]; 2],
                media_actions: None,
            };
            2
        ],
    }
}

#[shoop_test]
fn all_subsystem_values_roundtrip_with_exact_integer_bits() {
    let graph = Graph {
        quantum: 32,
        nodes: vec![
            Node::Input { port: 0 },
            Node::Gain {
                input: NodeId(0),
                gain: Fixed(i32::MIN),
            },
            Node::Mix {
                left: NodeId(0),
                right: NodeId(1),
            },
            Node::Capture {
                input: NodeId(2),
                view: ViewId(7),
                recording: true,
            },
            Node::Playback {
                view: ViewId(7),
                length: u64::MAX,
                position: u64::MAX - 1,
                playing: false,
            },
            Node::Output {
                input: NodeId(4),
                port: u32::MAX as usize,
            },
        ],
    };
    let compiled = compile(&graph).unwrap();
    let max = u32::MAX as usize;
    let jobs = vec![
        Job::Input {
            port: max,
            destination: BufferId(max),
        },
        Job::Silence {
            destination: BufferId(max),
        },
        Job::ReadView {
            view: ViewId(max),
            position: u64::MAX - 1,
            length: u64::MAX,
            destination: BufferId(max),
        },
        Job::Gain {
            source: BufferId(max),
            destination: BufferId(max),
            ramp: ramp(),
        },
        Job::Mix {
            left: BufferId(max),
            right: BufferId(max),
            destination: BufferId(max),
        },
        Job::Capture {
            source: BufferId(max),
            view: ViewId(max),
        },
        Job::Output {
            source: BufferId(max),
            port: max,
        },
    ];
    let mut payloads = vec![
        Payload::Graph(graph),
        Payload::Compiled(compiled.clone()),
        Payload::Program(compiled.program.clone()),
        Payload::Instructions(compiled.program.instructions),
        Payload::Bindings(compiled.bindings),
        Payload::State(StateSnapshot {
            memory: vec![Value::Word(u64::MAX), Value::Ramp(ramp())],
        }),
        Payload::Jobs(jobs),
        Payload::StateWrites(vec![StateWrite {
            address: Address(max),
            value: Value::Word(u64::MAX),
        }]),
        Payload::OnWrap(OnWrap {
            source: NodeId(max),
            target: NodeId(max),
            enabled: true,
        }),
        Payload::Status(ExecutionStatus::Completed),
        Payload::Case(case()),
        Payload::Trace(run_case(case())),
        Payload::AudioCase(audio_case()),
        Payload::AudioTrace(run_audio_case(audio_case()).unwrap()),
    ];
    for error in [
        Error::InvalidGraph,
        Error::InvalidState,
        Error::InvalidBuffer,
        Error::InvalidView,
        Error::InvalidShape,
        Error::QueueFull,
        Error::MemoryFull,
    ] {
        payloads.push(Payload::Status(ExecutionStatus::Failed { error }));
    }
    for payload in payloads {
        let document = Document::new(payload);
        let json = document.to_json().unwrap();
        assert_eq!(Document::from_json(&json).unwrap(), document);
    }
    assert_eq!(
        serde_json::to_string(&ChunkId(max)).unwrap(),
        max.to_string()
    );
    assert_eq!(
        serde_json::from_str::<ChunkId>(&max.to_string()).unwrap(),
        ChunkId(max)
    );
    let encoded = serde_json::to_value(Value::Ramp(ramp())).unwrap();
    assert_eq!(encoded["value"]["start"], i32::MIN);
    assert_eq!(encoded["value"]["target"], i32::MAX);
    assert_eq!(
        serde_json::to_value(Binding::None).unwrap(),
        serde_json::json!({"kind":"none"})
    );
}

#[shoop_test]
fn audio_traces_run_real_jobs_with_rounding_saturation_and_buffer_reuse() {
    let trace = run_audio_case(audio_case()).unwrap();
    assert_eq!(trace.results.len(), 2);
    for result in &trace.results {
        assert_eq!(result.stage, ExecutionStage::Jobs);
        assert_eq!(result.status, ExecutionStatus::Completed);
        assert_eq!(
            result.outputs[0],
            vec![Fixed(1), Fixed(-4), Fixed(i32::MAX)]
        );
        assert_eq!(result.outputs[1], vec![Fixed::ZERO; 3]);
        assert!(result.buffers.iter().all(Option::is_some));
        assert_eq!(
            result.buffers[1],
            Some(vec![Fixed(0), Fixed(-1), Fixed(1073741823)])
        );
        assert_eq!(result.jobs.len(), 5);
        assert!(result.remaining_jobs.is_empty());
    }
}

#[shoop_test]
fn audio_failure_preserves_state_and_remaining_jobs_and_shape_checks_run_first() {
    let mut input = audio_case();
    input.initial_state.memory[0] = Value::Ramp(Ramp::new(Fixed::ZERO, Fixed::ONE, 11));
    input.program.instructions[1] = Instruction::ProcessGain {
        ramp: Address(0),
        source: BufferId(0),
        destination: BufferId(0),
    };
    let trace = run_audio_case(input).unwrap();
    assert_eq!(trace.results.len(), 1);
    let result = &trace.results[0];
    assert_eq!(
        result.status,
        ExecutionStatus::Failed {
            error: Error::InvalidBuffer
        }
    );
    assert_eq!(result.state.memory[0].ramp().unwrap().elapsed, 3);
    assert_eq!(result.remaining_jobs.len(), 3);
    assert!(result.buffers[0].is_some());
    assert!(result.buffers[1..].iter().all(Option::is_none));
    assert_eq!(result.outputs, vec![vec![Fixed::ZERO; 3]; 2]);

    let mut input = audio_case();
    input.steps[0].inputs[0].pop();
    let trace = run_audio_case(input).unwrap();
    let result = &trace.results[0];
    assert_eq!(
        result.status,
        ExecutionStatus::Failed {
            error: Error::InvalidShape
        }
    );
    assert_eq!(result.remaining_jobs.len(), 5);
    assert!(result.buffers.iter().all(Option::is_none));
}

#[shoop_test]
fn traces_preserve_job_order_and_persistent_state_without_advancing_loop_clocks() {
    let trace = run_case(case());
    assert_eq!(trace.results.len(), 2);
    assert_eq!(
        trace.results[0].jobs,
        vec![
            Job::Input {
                port: 0,
                destination: BufferId(0)
            },
            Job::Gain {
                source: BufferId(0),
                destination: BufferId(1),
                ramp: ramp()
            },
            Job::Capture {
                source: BufferId(1),
                view: ViewId(17)
            },
            Job::Silence {
                destination: BufferId(2)
            },
            Job::Mix {
                left: BufferId(1),
                right: BufferId(2),
                destination: BufferId(3)
            },
            Job::Output {
                source: BufferId(3),
                port: 0
            },
        ]
    );
    assert_eq!(
        trace.results[0].state.memory[0].ramp().unwrap().elapsed,
        u32::MAX
    );
    assert_eq!(trace.results[0].state.memory[3], Value::Word(0));
    assert!(matches!(
        trace.results[1].jobs[2],
        Job::ReadView {
            position: 40,
            length: 45,
            ..
        }
    ));
    assert_eq!(trace.results[1].jobs.len(), 5);
    assert_eq!(trace.results[1].state.memory[3], Value::Word(40));
    assert!(trace
        .results
        .iter()
        .all(|result| result.status == ExecutionStatus::Completed));
}

#[shoop_test]
fn failed_steps_retain_partial_results_and_stop_before_the_next_quantum() {
    let mut input = case();
    input.steps[0].writes = vec![
        StateWrite {
            address: Address(5),
            value: Value::Word(1),
        },
        StateWrite {
            address: Address(3),
            value: Value::Word(45),
        },
    ];
    let trace = run_case(input);
    assert_eq!(trace.results.len(), 1);
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::InvalidState
        }
    );
    assert_eq!(trace.results[0].jobs.len(), 3);
    assert_eq!(
        trace.results[0].state.memory[0].ramp().unwrap().elapsed,
        u32::MAX
    );
    assert_eq!(trace.results[0].state.memory[3], Value::Word(45));

    let mut input = case();
    input.queue_capacity = 5;
    let initial = input.initial_state.clone();
    let trace = run_case(input);
    assert_eq!(trace.results.len(), 1);
    assert_eq!(
        trace.results[0].status,
        ExecutionStatus::Failed {
            error: Error::QueueFull
        }
    );
    assert!(trace.results[0].jobs.is_empty());
    assert_eq!(trace.results[0].state, initial);
}

#[shoop_test]
fn the_published_schema_is_generated_from_the_rust_boundary_types() {
    let published: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../scripts/fpga_testbench/schema.json"
    )))
    .unwrap();
    assert_eq!(published, serde_json::to_value(schema()).unwrap());
}

#[shoop_test]
fn versions_unknown_fields_and_unrepresentable_indices_are_rejected() {
    for json in [
        r#"{"format_version":2,"payload":{"kind":"status","value":{"kind":"completed"}}}"#,
        r#"{"format_version":1,"payload":{"kind":"status","value":{"kind":"completed","extra":1}}}"#,
        r#"{"format_version":1,"payload":{"kind":"bindings","value":[{"kind":"none","extra":1}]}}"#,
        r#"{"format_version":1,"payload":{"kind":"status","value":{"kind":"completed","value":null}}}"#,
        r#"{"format_version":1,"payload":{"kind":"state_writes","value":[{"address":4294967296,"value":{"kind":"word","value":0}}]}}"#,
    ] {
        assert!(Document::from_json(json).is_err(), "{json}");
    }
    if usize::BITS > 32 {
        let too_large = Address((u64::from(u32::MAX) + 1) as usize);
        assert!(serde_json::to_string(&too_large).is_err());
    }
}
