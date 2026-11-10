use rt_fpga_reference::{
    compiler::{compile, Binding, Graph, Node, NodeId},
    fixed::{Fixed, Ramp},
    jobs::{JobProcessor, JobQueue},
    state::{Address, StateProcessor, Value},
    storage::MediaStore,
    transitions::*,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 7,
    }
}
fn spec(slot: u32, length: u64, source: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: source.map(id),
        repeat_sync: false,
        timeline: None,
    }
}
fn limits(n: u32, p: u32) -> TransitionLimits {
    TransitionLimits {
        loop_slots: n,
        pending_requests: p,
        events: n * 2 + p,
    }
}
fn plan(loops: Vec<LoopSpec>, n: u32, p: u32) -> TransitionPlan {
    TransitionPlan::compile(PlanDescription { quantum: 16, loops }, limits(n, p)).unwrap()
}
fn command(target: u32, sequence: u64, operation: Operation) -> Command {
    Command {
        target: id(target),
        sequence,
        at: 0,
        timing: Timing::At,
        operation,
    }
}
fn play() -> Operation {
    Operation::Play {
        position: 0,
        retrigger: true,
    }
}
fn tick(processor: &mut TransitionProcessor) {
    let boundary = processor.preview(&[]).unwrap();
    processor.commit(boundary).unwrap();
    processor.finish_quantum(boundary).unwrap();
}

#[shoop_test]
fn preview_is_private_replaceable_and_generation_checked() {
    let mut processor = TransitionProcessor::prepare(plan(vec![spec(0, 32, None)], 1, 2));
    let request = processor.submit(command(0, 1, play())).unwrap();
    let first = processor.preview(&[]).unwrap();
    assert_eq!(processor.state(id(0)).unwrap().mode, Mode::Stopped);
    assert_eq!(processor.completion(request), Ok(None));
    assert!(processor.events().is_empty());
    assert_eq!(
        processor.decisions(first).unwrap()[0].state.mode,
        Mode::Playing
    );
    assert_eq!(
        processor.submit(command(0, 2, Operation::Stop)),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(
        processor.cancel(request),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(
        processor.take_completion(request),
        Err(TransitionError::InvalidState)
    );
    let second = processor
        .preview(&[Observation::Refused {
            request,
            reason: TransitionError::Capacity,
        }])
        .unwrap();
    assert_eq!(
        processor.decisions(first),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(processor.commit(first), Err(TransitionError::InvalidState));
    assert_eq!(
        processor.finish_quantum(second),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(
        processor.decisions(second).unwrap()[0].state.mode,
        Mode::Stopped
    );
    processor.commit(second).unwrap();
    assert_eq!(processor.commit(second), Err(TransitionError::InvalidState));
    assert_eq!(processor.preview(&[]), Err(TransitionError::InvalidState));
    assert_eq!(
        processor.completion(request).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: 0,
            reason: TransitionError::Capacity
        }
    );
    processor.finish_quantum(second).unwrap();
    assert_eq!(
        processor.finish_quantum(second),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(
        processor.decisions(second),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(processor.frame(), 16);
}

#[shoop_test]
fn failed_preview_invalidates_token_and_keeps_inputs_frozen_for_retry() {
    let mut processor = TransitionProcessor::prepare(plan(vec![spec(0, 32, None)], 1, 1));
    processor.submit(command(0, 1, play())).unwrap();
    let old = processor.preview(&[]).unwrap();
    assert_eq!(
        processor.preview(&[Observation::CaptureUnavailable {
            target: id(9),
            reason: TransitionError::Capacity
        }]),
        Err(TransitionError::InvalidId)
    );
    assert_eq!(processor.commit(old), Err(TransitionError::InvalidState));
    assert_eq!(
        processor.submit(command(0, 2, Operation::Stop)),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(processor.state(id(0)).unwrap().mode, Mode::Stopped);
    tick(&mut processor);
    assert_eq!(processor.state(id(0)).unwrap().position, 16);
}

#[shoop_test]
fn terminal_slots_stay_retained_until_collected_then_advance_generation() {
    let mut processor = TransitionProcessor::prepare(plan(vec![spec(0, 32, None)], 1, 1));
    let first = processor.submit(command(0, 1, play())).unwrap();
    assert_eq!(processor.take_completion(first), Ok(None));
    tick(&mut processor);
    assert_eq!(
        processor.submit(command(0, 2, Operation::Stop)),
        Err(TransitionError::Capacity)
    );
    assert_eq!(
        processor.submit(command(0, 1, Operation::Stop)),
        Err(TransitionError::DuplicateId)
    );
    assert!(!processor.cancel(first).unwrap());
    assert!(processor.take_completion(first).unwrap().is_some());
    assert_eq!(processor.completion(first), Err(TransitionError::InvalidId));
    let next = processor.submit(command(0, 2, Operation::Stop)).unwrap();
    assert_eq!(next.slot, first.slot);
    assert_eq!(next.generation, first.generation + 1);
    assert_eq!(processor.cancel(first), Err(TransitionError::InvalidId));
    assert!(processor.cancel(next).unwrap());
    assert_eq!(
        processor.completion(next).unwrap().unwrap().outcome,
        Outcome::Cancelled { frame: 16 }
    );
}

#[shoop_test]
fn prepared_execution_and_bindings_do_not_touch_heap() {
    let n = 130;
    let loops = (0..n).map(|i| spec(i, 32, Some((i + 1) % n))).collect();
    let mut processor = TransitionProcessor::prepare(plan(loops, n, n));
    let mut state = StateProcessor::new(vec![Value::Word(0); n as usize * 4]);
    let bindings = StateBindings::prepare(
        (0..n)
            .map(|i| LoopBinding {
                id: id(i),
                playback: Some(PlaybackBinding {
                    enabled: Address(i as usize * 4),
                    position: Address(i as usize * 4 + 1),
                    length: Address(i as usize * 4 + 2),
                }),
                capture_enabled: Some(Address(i as usize * 4 + 3)),
            })
            .collect(),
        &state,
    )
    .unwrap();
    assert_no_heap(|| {
        for i in 0..n {
            processor.submit(command(i, u64::from(i), play())).unwrap();
        }
        for frame in 0..12 {
            let first = processor.preview(&[]).unwrap();
            let retry = processor.preview(&[]).unwrap();
            assert_eq!(processor.commit(first), Err(TransitionError::InvalidState));
            processor.commit(retry).unwrap();
            bindings
                .export(processor.decisions(retry).unwrap(), &mut state)
                .unwrap();
            processor.finish_quantum(retry).unwrap();
            if frame == 0 {
                for i in 0..n {
                    processor
                        .take_completion(RequestId {
                            slot: i,
                            generation: 1,
                        })
                        .unwrap();
                }
            }
        }
        let request = processor
            .submit(command(0, 1000, Operation::Record))
            .unwrap();
        processor.cancel(request).unwrap();
        processor.take_completion(request).unwrap();
    });
}

#[shoop_test]
fn state_export_validates_all_operands_before_writing() {
    let mut state = StateProcessor::new(vec![Value::Word(0); 4]);
    let binding = LoopBinding {
        id: id(0),
        playback: Some(PlaybackBinding {
            enabled: Address(0),
            position: Address(1),
            length: Address(2),
        }),
        capture_enabled: Some(Address(3)),
    };
    let bindings = StateBindings::prepare(vec![binding], &state).unwrap();
    let mut processor = TransitionProcessor::prepare(plan(vec![spec(0, 32, None)], 1, 1));
    processor.submit(command(0, 1, play())).unwrap();
    let boundary = processor.preview(&[]).unwrap();
    processor.commit(boundary).unwrap();
    assert_eq!(
        bindings.export(&[], &mut state),
        Err(TransitionError::InvalidId)
    );
    state
        .write(
            Address(3),
            Value::Ramp(Ramp::new(Fixed::ZERO, Fixed::ZERO, 0)),
        )
        .unwrap();
    assert_eq!(
        bindings.export(processor.decisions(boundary).unwrap(), &mut state),
        Err(TransitionError::InvalidState)
    );
    assert_eq!(state.read(Address(0)), Ok(Value::Word(0)));
    assert_eq!(state.read(Address(2)), Ok(Value::Word(0)));
    assert!(matches!(
        StateBindings::prepare(vec![binding, binding], &state),
        Err(TransitionError::InvalidState)
    ));
    state.write(Address(3), Value::Word(0)).unwrap();
    assert!(matches!(
        StateBindings::prepare(vec![binding, binding], &state),
        Err(TransitionError::DuplicateId)
    ));
    let alias = LoopBinding {
        capture_enabled: Some(Address(0)),
        ..binding
    };
    assert!(matches!(
        StateBindings::prepare(vec![alias], &state),
        Err(TransitionError::DuplicateId)
    ));
}

#[shoop_test]
fn logical_record_play_exports_to_one_persistent_signal_program() {
    let mut media = MediaStore::new(7).unwrap();
    let view = media.create_recording().unwrap();
    // Playback precedes capture in signal order; both refer to one logical loop.
    let compiled = compile(&Graph {
        quantum: 16,
        nodes: vec![
            Node::Playback {
                view,
                length: 16,
                position: 0,
                playing: false,
            },
            Node::Output {
                input: NodeId(0),
                port: 0,
            },
            Node::Input { port: 0 },
            Node::Capture {
                input: NodeId(2),
                view,
                recording: false,
            },
        ],
    })
    .unwrap();
    let Binding::Playback {
        enabled,
        position,
        length,
    } = compiled.bindings[0]
    else {
        panic!("playback binding")
    };
    let Binding::Capture {
        enabled: capture_enabled,
    } = compiled.bindings[3]
    else {
        panic!("capture binding")
    };
    let mut state = StateProcessor::new(compiled.initial_state);
    let bindings = StateBindings::prepare(
        vec![LoopBinding {
            id: id(0),
            playback: Some(PlaybackBinding {
                enabled,
                position,
                length,
            }),
            capture_enabled: Some(capture_enabled),
        }],
        &state,
    )
    .unwrap();
    let mut transitions = TransitionProcessor::prepare(plan(vec![spec(0, 0, None)], 1, 2));
    let mut queue = JobQueue::new(compiled.program.jobs_per_quantum);
    let mut jobs = JobProcessor::new(16, 4);
    let input = [Fixed(12345); 16];
    let mut output = [Fixed::ZERO; 16];
    for quantum in 0..7 {
        if quantum == 0 {
            transitions
                .submit(command(0, 1, Operation::Record))
                .unwrap();
        }
        if quantum == 2 {
            transitions.submit(command(0, 2, play())).unwrap();
        }
        let boundary = transitions.preview(&[]).unwrap();
        transitions.commit(boundary).unwrap();
        bindings
            .export(transitions.decisions(boundary).unwrap(), &mut state)
            .unwrap();
        state.run(&compiled.program, &mut queue).unwrap();
        jobs.run(&mut queue, &mut media, &[&input], &mut [&mut output])
            .unwrap();
        transitions.finish_quantum(boundary).unwrap();
        assert_eq!(
            output,
            if quantum < 2 {
                [Fixed::ZERO; 16]
            } else {
                input
            }
        );
    }
    assert_eq!(media.frames(view), Ok(32));
    assert_eq!(transitions.state(id(0)).unwrap().content_length, 32);
    assert_eq!(transitions.state(id(0)).unwrap().content_revision, 1);
}

#[shoop_test]
fn compiler_rejects_unimplemented_features_bad_grids_ids_and_limits() {
    let base = PlanDescription {
        quantum: 16,
        loops: vec![spec(0, 32, None)],
    };
    let compile = |description, limits| TransitionPlan::compile(description, limits).err().unwrap();
    assert_eq!(
        compile(
            PlanDescription {
                quantum: 3,
                ..base.clone()
            },
            limits(1, 1)
        ),
        TransitionError::InvalidConfig
    );
    assert_eq!(
        compile(
            base.clone(),
            TransitionLimits {
                events: 2,
                ..limits(1, 1)
            }
        ),
        TransitionError::InvalidConfig
    );
    for changed in [
        LoopSpec {
            kind: LoopKind::Composite,
            ..base.loops[0]
        },
        LoopSpec {
            timeline: Some(0),
            ..base.loops[0]
        },
    ] {
        assert_eq!(
            compile(
                PlanDescription {
                    loops: vec![changed],
                    ..base.clone()
                },
                limits(1, 1)
            ),
            TransitionError::Unsupported
        );
    }
    assert_eq!(
        compile(
            PlanDescription {
                loops: vec![spec(0, 33, None)],
                ..base.clone()
            },
            limits(1, 1)
        ),
        TransitionError::InvalidFrame
    );
    assert_eq!(
        compile(
            PlanDescription {
                loops: vec![spec(0, 32, Some(1))],
                ..base.clone()
            },
            limits(1, 1)
        ),
        TransitionError::InvalidId
    );
    assert_eq!(
        compile(
            PlanDescription {
                loops: vec![base.loops[0], base.loops[0]],
                ..base.clone()
            },
            limits(2, 1)
        ),
        TransitionError::DuplicateId
    );
    let wrong_generation = LoopSpec {
        id: LoopId {
            generation: 0,
            ..id(0)
        },
        ..base.loops[0]
    };
    assert_eq!(
        compile(
            PlanDescription {
                loops: vec![wrong_generation],
                ..base
            },
            limits(1, 1)
        ),
        TransitionError::InvalidId
    );
}

#[shoop_test]
fn compiled_followers_are_canonical_sparse_and_generation_checked() {
    let loops = vec![
        spec(129, 32, Some(0)),
        spec(0, 32, Some(64)),
        spec(64, 32, Some(0)),
        spec(63, 32, Some(0)),
    ];
    let forward = plan(loops.clone(), 130, 4);
    let reverse = plan(loops.into_iter().rev().collect(), 130, 4);
    for id in [id(0), id(63), id(64), id(129)] {
        assert_eq!(forward.followers(id), reverse.followers(id));
    }
    assert_eq!(
        forward.followers(id(0)),
        Ok([id(63), id(64), id(129)].as_slice())
    );
    assert_eq!(forward.followers(id(63)), Ok([].as_slice()));
    assert_eq!(forward.followers(id(64)), Ok([id(0)].as_slice()));
    assert_eq!(forward.followers(id(1)), Err(TransitionError::InvalidId));
    assert_eq!(
        forward.followers(LoopId {
            slot: 0,
            generation: 8
        }),
        Err(TransitionError::InvalidId)
    );
}
