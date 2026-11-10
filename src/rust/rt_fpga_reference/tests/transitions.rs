use rt_fpga_reference::{
    fixed::Fixed,
    jobs::{BufferId, Job, JobProcessor, JobQueue},
    pipeline::{Pipeline, PipelineError},
    state::{Address, Instruction, Program, StateProcessor, Value},
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
fn prepared_shared_loop_execution_does_not_touch_heap() {
    let n = 130;
    let loops = (0..n).map(|i| spec(i, 32, Some((i + 1) % n))).collect();
    let mut processor = TransitionProcessor::prepare(plan(loops, n, n));
    let mut state = StateProcessor::new(vec![Value::Word(0); n as usize]);
    let program = Program {
        quantum: 16,
        jobs_per_quantum: n as usize,
        instructions: (0..n)
            .map(|i| Instruction::ProcessLogicalPlayback {
                loop_id: id(i),
                view: Address(i as usize),
                destination: BufferId(i as usize),
            })
            .collect(),
    };
    let mut queue = JobQueue::new(n as usize);
    assert_no_heap(|| {
        for i in 0..n {
            processor.submit(command(i, u64::from(i), play())).unwrap();
        }
        for frame in 0..12 {
            let first = processor.preview(&[]).unwrap();
            let retry = processor.preview(&[]).unwrap();
            assert_eq!(processor.commit(first), Err(TransitionError::InvalidState));
            processor.commit(retry).unwrap();
            state
                .run_with_loops(&program, &mut queue, processor.committed(retry).unwrap())
                .unwrap();
            while queue.pop().is_some() {}
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
    });
    assert!(state.values().iter().all(|v| *v == Value::Word(0)));
}

#[shoop_test]
fn shared_reads_require_committed_token_and_match_generation() {
    let mut processor = TransitionProcessor::prepare(plan(vec![spec(0, 32, None)], 1, 1));
    processor.submit(command(0, 1, play())).unwrap();
    let boundary = processor.preview(&[]).unwrap();
    assert_eq!(
        processor.committed(boundary).err(),
        Some(TransitionError::InvalidState)
    );
    processor.commit(boundary).unwrap();
    let loops = processor.committed(boundary).unwrap();
    assert_eq!(loops.read(id(0)).unwrap().position, 0);
    assert_eq!(
        loops.read(LoopId {
            slot: 0,
            generation: 8
        }),
        Err(TransitionError::InvalidId)
    );
    let mut state = StateProcessor::new(vec![Value::Word(0)]);
    let mut program = Program {
        quantum: 16,
        jobs_per_quantum: 1,
        instructions: vec![Instruction::ProcessLogicalPlayback {
            loop_id: id(0),
            view: Address(0),
            destination: BufferId(0),
        }],
    };
    let mut queue = JobQueue::new(1);
    assert_eq!(
        state.run(&program, &mut queue),
        Err(rt_fpga_reference::Error::InvalidState)
    );
    state.run_with_loops(&program, &mut queue, loops).unwrap();
    assert!(matches!(
        queue.pop(),
        Some(Job::ReadView {
            position: 0,
            length: 32,
            ..
        })
    ));
    program.instructions[0] = Instruction::ProcessLogicalPlayback {
        loop_id: LoopId {
            slot: 0,
            generation: 8,
        },
        view: Address(0),
        destination: BufferId(0),
    };
    assert_eq!(
        state.run_with_loops(&program, &mut queue, loops),
        Err(rt_fpga_reference::Error::InvalidState)
    );
    processor.finish_quantum(boundary).unwrap();
    assert_eq!(
        processor.committed(boundary).err(),
        Some(TransitionError::InvalidState)
    );
}

fn loop_program() -> Program {
    Program {
        quantum: 16,
        jobs_per_quantum: 4,
        instructions: vec![
            Instruction::ProcessLogicalPlayback {
                loop_id: id(0),
                view: Address(0),
                destination: BufferId(0),
            },
            Instruction::ProcessOutput {
                source: BufferId(0),
                port: 0,
            },
            Instruction::ProcessInput {
                port: 0,
                destination: BufferId(1),
            },
            Instruction::ProcessLogicalCapture {
                loop_id: id(0),
                view: Address(0),
                source: BufferId(1),
            },
        ],
    }
}

#[shoop_test]
fn logical_record_play_uses_shared_records_in_one_pipeline() {
    let mut media = MediaStore::new(7).unwrap();
    let view = media.create_recording().unwrap();
    let mut pipeline = Pipeline::prepare(
        plan(vec![spec(0, 0, None)], 1, 2),
        loop_program(),
        vec![Value::Word(view.0 as u64)],
        4,
    )
    .unwrap();
    let saved_program = pipeline.program().clone();
    let mut jobs = JobProcessor::new(16, 2);
    let mut output = [Fixed::ZERO; 16];
    for quantum in 0..7 {
        if quantum == 0 {
            pipeline.submit(command(0, 1, Operation::Record)).unwrap();
        }
        if quantum == 2 {
            pipeline.submit(command(0, 2, play())).unwrap();
        }
        let input = [Fixed(12345 + quantum); 16];
        pipeline
            .run(&[], &mut jobs, &mut media, &[&input], &mut [&mut output])
            .unwrap();
        assert_eq!(
            output,
            if quantum < 2 {
                [Fixed::ZERO; 16]
            } else {
                [Fixed(12345 + (quantum - 2) % 2); 16]
            }
        );
        assert_eq!(pipeline.state().values(), &[Value::Word(view.0 as u64)]);
        assert_eq!(pipeline.program(), &saved_program);
    }
    assert_eq!(media.frames(view), Ok(32));
    assert_eq!(
        pipeline.transitions().state(id(0)).unwrap().content_length,
        32
    );
    assert_eq!(pipeline.transitions().frame(), 112);
}

#[shoop_test]
fn pipeline_retries_and_job_completion_guard_clock_advancement() {
    let mut p = Pipeline::prepare(
        plan(vec![spec(0, 32, None)], 1, 2),
        loop_program(),
        vec![Value::Word(0)],
        4,
    )
    .unwrap();
    p.submit(command(0, 1, play())).unwrap();
    let first = p.preview(&[]).unwrap();
    let boundary = p.preview(&[]).unwrap();
    assert_eq!(
        p.run_state(first),
        Err(PipelineError::Transition(TransitionError::InvalidState))
    );
    assert_eq!(p.transitions().state(id(0)).unwrap().mode, Mode::Stopped);
    p.run_state(boundary).unwrap();
    assert!(p.finish_jobs(boundary, Ok(())).is_err());
    assert_eq!(p.transitions().frame(), 0);
    assert!(p.preview(&[]).is_err());
    while p.pop_job().is_some() {}
    p.finish_jobs(boundary, Ok(())).unwrap();
    assert_eq!(p.transitions().frame(), 16);
    assert!(p.finish_jobs(boundary, Ok(())).is_err());
    let boundary = p.preview(&[]).unwrap();
    p.run_state(boundary).unwrap();
    while p.pop_job().is_some() {}
    assert_eq!(
        p.finish_jobs(boundary, Err(rt_fpga_reference::Error::InvalidView)),
        Err(PipelineError::Jobs(rt_fpga_reference::Error::InvalidView))
    );
    assert_eq!(p.transitions().frame(), 16);
    assert!(p.preview(&[]).is_err());
}

#[shoop_test]
fn pipeline_rejects_a_job_processor_with_a_different_quantum() {
    let mut pipeline = Pipeline::prepare(
        plan(vec![spec(0, 32, None)], 1, 2),
        loop_program(),
        vec![Value::Word(0)],
        4,
    )
    .unwrap();
    let boundary = pipeline.preview(&[]).unwrap();
    pipeline.run_state(boundary).unwrap();
    let queued: Vec<_> = pipeline.queued_jobs().copied().collect();
    let mut jobs = JobProcessor::new(32, 2);
    let mut media = MediaStore::new(7).unwrap();
    let mut output = [Fixed(99); 32];
    assert_eq!(
        pipeline.run_jobs(
            boundary,
            &mut jobs,
            &mut media,
            &[&[Fixed(1); 32]],
            &mut [&mut output]
        ),
        Err(PipelineError::Jobs(rt_fpga_reference::Error::InvalidShape))
    );
    assert_eq!(pipeline.transitions().frame(), 0);
    assert_eq!(output, [Fixed(99); 32]);
    assert_eq!(pipeline.queued_jobs().copied().collect::<Vec<_>>(), queued);
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
