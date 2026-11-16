use rt_fpga_reference::{
    jobs::{BufferId, Job, JobQueue},
    state::{Address, Instruction, Program, StateProcessor, SyncInstruction, Value},
    transitions::*,
    Error,
};
use shoop_wasm_test_support::shoop_test;

fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 7,
    }
}
fn spec(slot: u32, length: u64, source: Option<u32>, repeat_sync: bool) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: source.map(id),
        repeat_sync,
        timeline: None,
    }
}
fn fixture(
    specs: Vec<LoopSpec>,
    instructions: Vec<Instruction>,
    memory: Vec<Value>,
) -> (TransitionProcessor, StateProcessor, Program, JobQueue) {
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: 16,
            loops: specs,
        },
        TransitionLimits {
            loop_slots: 10,
            pending_requests: 8,
            events: 28,
        },
    )
    .unwrap();
    let program = Program {
        sync_instructions: plan.sync_instructions(),
        instructions,
        quantum: 16,
        chunk_frames: 1,
        jobs_per_quantum: 8,
    };
    let control = TransitionProcessor::prepare(&plan);
    (
        control,
        StateProcessor::with_loops(plan, memory),
        program,
        JobQueue::new(8),
    )
}
fn command(target: u32, sequence: u64, at: u64, operation: Operation, timing: Timing) -> Command {
    Command {
        target: id(target),
        sequence,
        at,
        operation,
        timing,
    }
}
fn play() -> Operation {
    Operation::Play {
        position: 0,
        retrigger: true,
    }
}

#[shoop_test]
fn sync_prefix_propagates_before_reverse_audio_order_and_keeps_render_cursors() {
    let (mut control, mut sp, program, mut queue) = fixture(
        vec![spec(1, 16, Some(9), true), spec(9, 32, None, false)],
        vec![
            Instruction::ProcessLogicalPlayback {
                loop_id: id(1),
                view: Address(0),
                destination: BufferId(0),
            },
            Instruction::ProcessLogicalPlayback {
                loop_id: id(9),
                view: Address(1),
                destination: BufferId(1),
            },
        ],
        vec![Value::Word(1), Value::Word(9)],
    );
    assert_eq!(
        program.sync_instructions,
        vec![
            SyncInstruction::ProcessLoop { loop_id: id(9) },
            SyncInstruction::ProcessLoop { loop_id: id(1) }
        ]
    );
    control
        .submit(&mut sp, command(9, 1, 0, play(), Timing::At))
        .unwrap();
    assert_eq!(sp.loop_state(id(9)).unwrap().state.mode, Mode::Stopped);
    assert_eq!(sp.frame(), 0);
    assert!(sp.events().is_empty());
    control
        .submit(
            &mut sp,
            command(
                1,
                2,
                0,
                play(),
                Timing::AfterTicks {
                    source: id(9),
                    skip: 0,
                },
            ),
        )
        .unwrap();
    for quantum in 0..6 {
        // No control-side execution or outcome harvesting is needed between quanta.
        sp.run(&program, &mut queue).unwrap();
        let source = sp.loop_state(id(9)).unwrap();
        let target = sp.loop_state(id(1)).unwrap();
        let wrap = quantum % 2 == 0;
        assert_eq!(source.render.position, if wrap { 0 } else { 16 });
        assert_eq!(source.state.position, if wrap { 16 } else { 32 });
        assert_eq!(source.pulse_this_quantum, wrap);
        assert_eq!(target.render.position, if wrap { 0 } else { 16 });
        assert_eq!(target.state.position, 16);
        assert_eq!(target.pulse_this_quantum, wrap);
        assert_eq!(
            queue.pop(),
            Some(if wrap {
                Job::ReadView {
                    view: rt_fpga_reference::storage::ViewId(1),
                    position: 0,
                    length: 16,
                    destination: BufferId(0),
                }
            } else {
                Job::Silence {
                    destination: BufferId(0),
                }
            })
        );
        assert_eq!(
            queue.pop(),
            Some(Job::ReadView {
                view: rt_fpga_reference::storage::ViewId(9),
                position: source.render.position,
                length: 32,
                destination: BufferId(1)
            })
        );
        assert!(queue.is_empty());
        if wrap {
            let pulses: Vec<_> = sp
                .events()
                .iter()
                .filter(|event| event.kind == EventKind::Pulse)
                .map(|event| event.target)
                .collect();
            assert_eq!(pulses, vec![id(9), id(1)]);
        }
    }
    control.harvest(&mut sp);
    assert_eq!(
        control
            .request_slots(&sp)
            .flatten()
            .filter(|request| matches!(request.outcome, Some(Outcome::Applied { frame: 0 })))
            .count(),
        2
    );
    assert_eq!(sp.frame(), 96);
}

#[shoop_test]
fn two_active_plans_are_bounded_and_resolved_slots_can_be_reused_before_collection() {
    let (mut control, mut sp, program, mut queue) =
        fixture(vec![spec(0, 32, None, false)], vec![], vec![]);
    let first = control
        .submit(&mut sp, command(0, 1, 0, play(), Timing::At))
        .unwrap();
    let second = control
        .submit(&mut sp, command(0, 2, 64, Operation::Stop, Timing::At))
        .unwrap();
    let before = sp.loop_state(id(0)).unwrap().clone();
    assert_eq!(
        control.submit(&mut sp, command(0, 3, 96, Operation::Record, Timing::At)),
        Err(TransitionError::Capacity)
    );
    assert_eq!(sp.loop_state(id(0)).unwrap(), &before);
    assert_eq!(sp.frame(), 0);
    sp.run(&program, &mut queue).unwrap();
    let third = control
        .submit(&mut sp, command(0, 3, 96, Operation::Record, Timing::At))
        .unwrap();
    assert_eq!(
        third,
        RequestId {
            slot: 2,
            generation: 1
        }
    );
    assert!(matches!(
        control.completion(first).unwrap(),
        Some(Completion {
            outcome: Outcome::Applied { frame: 0 },
            ..
        })
    ));
    assert_eq!(
        sp.loop_state(id(0))
            .unwrap()
            .planned
            .iter()
            .flatten()
            .count(),
        2
    );
    control.cancel(&mut sp, second).unwrap();
    let replacement = control
        .submit(&mut sp, command(0, 4, 32, Operation::Stop, Timing::At))
        .unwrap();
    assert_eq!(replacement.slot, 3);
    assert_eq!(
        sp.loop_state(id(0))
            .unwrap()
            .planned
            .iter()
            .flatten()
            .count(),
        2
    );
}

#[shoop_test]
fn capture_start_is_saved_before_length_advance_and_recomputed_on_retake() {
    let (mut control, mut sp, program, mut queue) = fixture(
        vec![spec(0, 0, None, false)],
        vec![Instruction::ProcessLogicalCapture {
            loop_id: id(0),
            view: Address(0),
            source: BufferId(0),
        }],
        vec![Value::Word(0)],
    );
    control
        .submit(&mut sp, command(0, 1, 0, Operation::Record, Timing::At))
        .unwrap();
    for quantum in 0..3 {
        if quantum == 2 {
            control
                .submit(&mut sp, command(0, 2, 0, Operation::Record, Timing::At))
                .unwrap();
        }
        sp.run(&program, &mut queue).unwrap();
        let start = quantum != 1;
        let slot = sp.loop_state(id(0)).unwrap();
        assert_eq!(slot.render.begin_capture, start);
        assert_eq!(slot.state.capture_frames, if start { 16 } else { 32 });
        assert_eq!(
            queue
                .iter()
                .filter(|job| matches!(job, Job::BeginCapture { .. }))
                .count(),
            usize::from(start)
        );
        while queue.pop().is_some() {}
    }
}

#[shoop_test]
fn malformed_sync_order_faults_without_emitting_audio_or_advancing_clocks() {
    let (mut control, mut sp, mut program, mut queue) = fixture(
        vec![spec(0, 32, Some(9), false), spec(9, 32, None, false)],
        vec![Instruction::ProcessSilence {
            destination: BufferId(0),
        }],
        vec![],
    );
    program.sync_instructions.reverse();
    assert_eq!(
        sp.run(&program, &mut queue),
        Err(Error::Transition(TransitionError::InvalidConfig))
    );
    assert_eq!(sp.frame(), 0);
    assert!(queue.is_empty());
    program.sync_instructions.reverse();
    assert_eq!(
        sp.run(&program, &mut queue),
        Err(Error::Transition(TransitionError::InvalidConfig))
    );
    assert_eq!(
        control.submit(&mut sp, command(9, 1, 0, play(), Timing::At)),
        Err(TransitionError::InvalidState)
    );
}
