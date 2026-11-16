use rt_fpga_reference::{
    jobs::{BufferId, Job, JobQueue},
    state::{
        Address, Instruction, PlannedTransition, Program, StateProcessor, SyncInstruction, Value,
    },
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
) -> (StateProcessor, Program, JobQueue) {
    let plan = TransitionPlan::compile(
        PlanDescription {
            quantum: 16,
            loops: specs,
        },
        TransitionLimits {
            loop_slots: 10,
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
    (
        StateProcessor::with_loops(plan, memory),
        program,
        JobQueue::new(8),
    )
}
fn plan(
    sequence: u64,
    effective: u64,
    operation: Operation,
    on_pulse: bool,
    remaining_ticks: u64,
) -> PlannedTransition {
    PlannedTransition {
        sequence,
        effective,
        operation,
        on_pulse,
        remaining_ticks,
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
    let (mut sp, program, mut queue) = fixture(
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
    sp.write_plans(id(9), [Some(plan(1, 0, play(), false, 0)), None])
        .unwrap();
    sp.write_plans(id(1), [Some(plan(2, 0, play(), true, 0)), None])
        .unwrap();
    assert_eq!(sp.loop_state(id(9)).unwrap().state.mode, Mode::Stopped);
    assert_eq!(sp.frame(), 0);
    assert!(sp.events().is_empty());
    for quantum in 0..6 {
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
    assert!(sp.resolutions().is_empty());
    assert_eq!(sp.frame(), 96);
}

#[shoop_test]
fn direct_plan_writes_reuse_resolved_slots_and_validate_atomically() {
    let (mut sp, program, mut queue) = fixture(vec![spec(0, 32, None, false)], vec![], vec![]);
    sp.write_plans(
        id(0),
        [
            Some(plan(1, 0, play(), false, 0)),
            Some(plan(2, 64, Operation::Stop, false, 0)),
        ],
    )
    .unwrap();
    sp.run(&program, &mut queue).unwrap();
    assert_eq!(sp.resolutions().len(), 1);
    assert_eq!(sp.resolutions()[0].plan_index, 0);
    assert_eq!(sp.resolutions()[0].outcome, Outcome::Applied { frame: 0 });
    let mut plans = sp.loop_state(id(0)).unwrap().planned;
    assert!(plans[0].is_none());
    plans[0] = Some(plan(3, 96, Operation::Record, false, 0));
    sp.write_plans(id(0), plans).unwrap();
    for (bad_id, bad_plans, error) in [
        (id(0), [plans[0], plans[0]], TransitionError::DuplicateId),
        (
            id(0),
            [Some(plan(4, 0, play(), true, 0)), None],
            TransitionError::MissingSync,
        ),
        (
            id(0),
            [
                Some(plan(
                    4,
                    0,
                    Operation::Seek {
                        position: 1,
                        retrigger: false,
                    },
                    false,
                    0,
                )),
                None,
            ],
            TransitionError::InvalidFrame,
        ),
        (id(9), plans, TransitionError::InvalidId),
    ] {
        assert_eq!(sp.write_plans(bad_id, bad_plans), Err(error));
        assert_eq!(sp.loop_state(id(0)).unwrap().planned, plans);
    }
    sp.run(&program, &mut queue).unwrap();
    assert!(sp.resolutions().is_empty());
    assert_eq!(sp.loop_state(id(0)).unwrap().planned, plans);
}

#[shoop_test]
fn capture_start_is_saved_before_length_advance_and_recomputed_on_retake() {
    let (mut sp, program, mut queue) = fixture(
        vec![spec(0, 0, None, false)],
        vec![Instruction::ProcessLogicalCapture {
            loop_id: id(0),
            view: Address(0),
            source: BufferId(0),
        }],
        vec![Value::Word(0)],
    );
    sp.write_plans(id(0), [Some(plan(1, 0, Operation::Record, false, 0)), None])
        .unwrap();
    for quantum in 0..3 {
        if quantum == 2 {
            sp.write_plans(id(0), [Some(plan(2, 0, Operation::Record, false, 0)), None])
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
    let (mut sp, mut program, mut queue) = fixture(
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
        sp.write_plans(id(9), [Some(plan(1, 0, play(), false, 0)), None]),
        Err(TransitionError::InvalidState)
    );
}
