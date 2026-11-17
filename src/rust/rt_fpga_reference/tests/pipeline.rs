use rt_fpga_reference::{
    fixed::Fixed,
    jobs::BufferId,
    pipeline::PipelineError,
    state::{Address, Instruction, PlannedTransition, Program, Value},
    storage::{MediaStore, ViewId},
    transitions::*,
    Error,
};
#[path = "support/pipeline.rs"]
mod pipeline;
#[path = "support/sync.rs"]
mod traces;
use pipeline::*;
use shoop_wasm_test_support::shoop_test;
use traces::{PlanInput, PlanWrite};

fn fixture() -> PipelineCase {
    let loop_id = LoopId {
        slot: 0,
        generation: 9,
    };
    let mut media = MediaStore::new(1).unwrap();
    let view = media.create_recording().unwrap();
    let program = Program {
        sync_instructions: vec![],
        quantum: 16,
        chunk_frames: 1,
        jobs_per_quantum: 5,
        instructions: vec![
            Instruction::ProcessLogicalPlayback {
                loop_id,
                view: Address(0),
                destination: BufferId(0),
            },
            Instruction::ProcessOutput {
                source: BufferId(0),
                port: 0,
            },
            Instruction::ProcessInput {
                port: 0,
                destination: BufferId(2),
            },
            Instruction::ProcessLogicalCapture {
                loop_id,
                view: Address(1),
                source: BufferId(2),
            },
        ],
    };
    PipelineCase {
        name: "record-play-shared-records".into(),
        plan: PlanInput {
            description: PlanDescription {
                quantum: 16,
                loops: vec![LoopSpec {
                    id: loop_id,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                    repeat_sync: false,
                    timeline: None,
                }],
            },
            limits: TransitionLimits {
                loop_slots: 1,
                events: 4,
            },
        },
        program,
        initial_state: StateSnapshot {
            memory: vec![Value::Word(view.0 as u64); 2],
        },
        queue_capacity: 8,
        buffers: 4,
        output_ports: 1,
        media: Some(media.snapshot()),
        steps: (0..6)
            .map(|i| PipelineStep {
                writes: vec![],
                plans: if i == 0 || i == 2 {
                    vec![PlanWrite {
                        target: loop_id,
                        index: 0,
                        plan: Some(PlannedTransition {
                            sequence: i + 1,
                            effective: 0,
                            remaining_ticks: 0,
                            on_pulse: false,
                            operation: if i == 0 {
                                Operation::Record
                            } else {
                                Operation::Play {
                                    position: 0,
                                    retrigger: true,
                                }
                            },
                        }),
                    }]
                } else {
                    vec![]
                },
                work: PipelineWork::Audio {
                    inputs: vec![vec![Fixed(1000 + i as i32); 16]],
                },
            })
            .collect(),
    }
}

#[shoop_test]
fn combined_trace_runs_sp_and_jp_without_copied_loop_operands() {
    let case = fixture();
    let initial = case.initial_state.clone();
    let trace = run_pipeline_case(case).unwrap();
    compare_transitions(&trace);
    for (i, row) in trace.results.iter().enumerate() {
        assert_eq!(row.state, initial);
        assert_eq!(row.error, None);
        assert_eq!(row.sync.frame, 16 * (i as u64 + 1));
        let sample = if i < 2 { 0 } else { 1000 + (i as i32 - 2) % 2 };
        assert_eq!(
            row.audio.as_ref().unwrap().outputs,
            vec![vec![Fixed(sample); 16]]
        );
    }
    #[cfg(feature = "serde")]
    assert_eq!(
        serde_json::from_str::<PipelineTrace>(&serde_json::to_string(&trace).unwrap()).unwrap(),
        trace
    );
}

#[shoop_test]
fn combined_trace_latches_failed_audio_pass_after_computing_next_cursors() {
    let mut case = fixture();
    let rt_fpga_reference::state::Instruction::ProcessLogicalPlayback { loop_id, .. } =
        &mut case.program.instructions[0]
    else {
        panic!("logical playback")
    };
    loop_id.generation += 1;
    let trace = run_pipeline_case(case).unwrap();
    assert_eq!(trace.results.len(), 6);
    assert_eq!(
        trace.results[0].error,
        Some(PipelineError::State(Error::InvalidState))
    );
    assert_eq!(trace.results[0].sync.frame, 16);
}

#[shoop_test]
fn shared_loop_pipeline_plays_a_retained_range_after_source_clear() {
    let recorded = run_pipeline_case(fixture()).unwrap();
    let snapshot = recorded
        .results
        .last()
        .unwrap()
        .audio
        .as_ref()
        .unwrap()
        .media
        .clone()
        .unwrap();
    let mut media = MediaStore::from_snapshot(snapshot).unwrap();
    let slice = media.slice(ViewId(0), 3, 16).unwrap();
    media.clear(ViewId(0)).unwrap();

    let mut case = fixture();
    case.name = "shared-loop-playback-of-retained-range".into();
    case.media = Some(media.snapshot());
    case.plan.description.loops[0].length = 16;
    case.initial_state
        .memory
        .fill(rt_fpga_reference::state::Value::Word(slice.0 as u64));
    case.steps.truncate(2);
    case.steps[0].plans[0].plan.as_mut().unwrap().operation = Operation::Play {
        position: 0,
        retrigger: true,
    };
    let initial = case.initial_state.clone();
    let trace = run_pipeline_case(case).unwrap();
    let expected: Vec<_> = (0..16)
        .map(|i| Fixed(if i < 13 { 1000 } else { 1001 }))
        .collect();
    compare_transitions(&trace);
    for (i, row) in trace.results.iter().enumerate() {
        assert_eq!(row.error, None);
        assert_eq!(row.state, initial);
        assert_eq!(row.sync.frame, (i as u64 + 1) * 16);
        let audio = row.audio.as_ref().unwrap();
        assert_eq!(audio.outputs, vec![expected.clone()]);
        let snapshot = audio.media.as_ref().unwrap();
        assert_eq!(snapshot.views[0].as_ref().unwrap().frames, 0);
        assert_eq!(
            snapshot.views[slice.0]
                .as_ref()
                .unwrap()
                .range
                .unwrap()
                .offset,
            0
        );
    }
    #[cfg(feature = "serde")]
    assert_eq!(
        serde_json::from_str::<PipelineTrace>(&serde_json::to_string(&trace).unwrap()).unwrap(),
        trace
    );
}

fn compare_transitions(trace: &PipelineTrace) {
    let mut oracle = traces::Oracle::prepare(&trace.case.plan).unwrap();
    for (i, (step, result)) in trace.case.steps.iter().zip(&trace.results).enumerate() {
        for write in &step.plans {
            oracle.write(write);
        }
        assert_eq!(result.sync, oracle.tick(), "quantum {i}");
    }
}

#[shoop_test]
fn direct_plans_drive_gain_arithmetic_and_smoothing_jobs() {
    use rt_fpga_reference::{
        fixed::Ramp,
        jobs::{ArithmeticMode, Job},
    };
    let mut case = fixture();
    let loop_id = case.plan.description.loops[0].id;
    let one = 1_i64 << 24;
    case.queue_capacity = 16;
    case.buffers = 8;
    case.initial_state.memory.extend([
        Value::Ramp(Ramp::new(Fixed(one as i32), Fixed(-(one as i32) / 2), 47)),
        Value::Word((33 << 32) | (7 << 16) | 5),
    ]);
    case.program.jobs_per_quantum = 10;
    case.program.instructions = vec![
        Instruction::ProcessInput {
            port: 0,
            destination: BufferId(0),
        },
        Instruction::ProcessLogicalPlayback {
            loop_id,
            view: Address(0),
            destination: BufferId(1),
        },
        Instruction::ProcessGain {
            ramp: Address(2),
            source: BufferId(1),
            destination: BufferId(2),
        },
        Instruction::ProcessConstant {
            value: one as i32 / 2,
            destination: BufferId(3),
        },
        Instruction::ProcessArithmetic {
            left: BufferId(2),
            right: BufferId(3),
            destination: BufferId(4),
            mode: ArithmeticMode::Multiply,
        },
        Instruction::ProcessSmoothing {
            loop_id,
            control: Address(3),
            source: BufferId(4),
        },
        Instruction::ProcessMix {
            left: BufferId(5),
            right: BufferId(0),
            destination: BufferId(6),
        },
        Instruction::ProcessOutput {
            source: BufferId(6),
            port: 0,
        },
        Instruction::ProcessLogicalCapture {
            loop_id,
            view: Address(1),
            source: BufferId(0),
        },
    ];
    case.steps = (0..12)
        .map(|i| PipelineStep {
            writes: vec![],
            plans: match i {
                0 | 7 => Some(Operation::Record),
                2 | 9 => Some(Operation::Play {
                    position: 0,
                    retrigger: true,
                }),
                4 => Some(Operation::Seek {
                    position: 16,
                    retrigger: true,
                }),
                5 | 11 => Some(Operation::Stop),
                6 => Some(Operation::Play {
                    position: 0,
                    retrigger: false,
                }),
                _ => None,
            }
            .map(|operation| PlanWrite {
                target: loop_id,
                index: 0,
                plan: Some(PlannedTransition {
                    sequence: i + 1,
                    effective: 0,
                    on_pulse: false,
                    remaining_ticks: 0,
                    operation,
                }),
            })
            .into_iter()
            .collect(),
            work: PipelineWork::Audio {
                inputs: vec![(0..16)
                    .map(|n| Fixed((i as i32 + 1) * 100000 - n * 1000))
                    .collect()],
            },
        })
        .collect();
    let trace = run_pipeline_case(case).unwrap();
    compare_transitions(&trace);
    let mut last = 0_i64;
    let mut anchor = 0_i64;
    let mut elapsed = 33_u64;
    for (i, row) in trace.results.iter().enumerate() {
        assert_eq!(row.error, None);
        let playing = matches!(i, 2 | 3 | 4 | 6 | 9 | 10);
        if matches!(i, 2 | 6 | 9) {
            anchor = last;
            elapsed = 0;
        }
        // These source quanta follow the explicitly scheduled record/seek/retake sequence.
        let recorded = match i {
            2 | 6 => 0,
            3 | 4 => 1,
            9 => 7,
            10 => 8,
            _ => 0,
        };
        let expected: Vec<_> = (0..16)
            .map(|n| {
                let input = (i as i64 + 1) * 100000 - n * 1000;
                let raw = if playing {
                    (recorded + 1) * 100000 - n * 1000
                } else {
                    0
                };
                let gain = one + (-one / 2 - one) * (i as i64 * 16 + n).min(47) / 47;
                let gained = (raw * gain / one).clamp(i32::MIN as i64, i32::MAX as i64);
                let value = gained * (one / 2) / one;
                last = if !playing {
                    0
                } else if elapsed >= 33 {
                    value
                } else {
                    anchor + (value - anchor) * ((elapsed << 24) / 33) as i64 / one
                };
                if playing {
                    elapsed = (elapsed + 1).min(33);
                } else {
                    elapsed = 33;
                }
                Fixed((last + input).clamp(i32::MIN as i64, i32::MAX as i64) as i32)
            })
            .collect();
        let audio = row.audio.as_ref().unwrap();
        assert_eq!(audio.outputs, vec![expected], "quantum {i}");
        assert_eq!(audio.buffers[3], Some(vec![Fixed(one as i32 / 2); 16]));
        assert_eq!(
            row.jobs
                .iter()
                .filter(|j| matches!(j, Job::BeginCapture { .. }))
                .count(),
            usize::from(matches!(i, 0 | 7))
        );
        assert!(audio.remaining_jobs.is_empty());
    }
    assert_eq!(
        trace.results.last().unwrap().state.memory[2]
            .ramp()
            .unwrap()
            .elapsed,
        47
    );
}
