use rt_fpga_reference::{
    jobs::JobQueue,
    state::{PlannedTransition, Program, StateProcessor},
    transitions::*,
};
#[path = "support/sync.rs"]
mod traces;
use shoop_wasm_test_support::shoop_test;
use traces::{Oracle, PlanInput, PlanWrite, Snapshot};

fn id(slot: u32) -> LoopId {
    LoopId {
        slot,
        generation: 3,
    }
}
fn spec(slot: u32, length: u64, source: Option<u32>, repeat: bool) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: source.map(id),
        repeat_sync: repeat,
        timeline: None,
    }
}
fn input(q: u32, loops: Vec<LoopSpec>) -> PlanInput {
    PlanInput {
        description: PlanDescription { quantum: q, loops },
        limits: TransitionLimits {
            loop_slots: 160,
            events: 640,
        },
    }
}
fn prepare(input: &PlanInput) -> (StateProcessor, Program, JobQueue, Oracle) {
    let plan = TransitionPlan::compile(input.description.clone(), input.limits).unwrap();
    let program = Program {
        sync_instructions: plan.sync_instructions(),
        instructions: vec![],
        quantum: input.description.quantum,
        chunk_frames: 1,
        jobs_per_quantum: 0,
    };
    (
        StateProcessor::with_loops(plan, vec![]),
        program,
        JobQueue::new(0),
        Oracle::prepare(input).unwrap(),
    )
}
fn write(
    target: u32,
    index: usize,
    sequence: u64,
    effective: u64,
    operation: Operation,
    on_pulse: bool,
    remaining_ticks: u64,
) -> PlanWrite {
    PlanWrite {
        target: id(target),
        index,
        plan: Some(PlannedTransition {
            sequence,
            effective,
            operation,
            on_pulse,
            remaining_ticks,
        }),
    }
}
fn play() -> Operation {
    Operation::Play {
        position: 0,
        retrigger: true,
    }
}
fn tick(
    sp: &mut StateProcessor,
    program: &Program,
    queue: &mut JobQueue,
    oracle: &mut Oracle,
) -> Snapshot {
    sp.run(program, queue).unwrap();
    let actual = Snapshot::from_sp(sp);
    assert_eq!(actual, oracle.tick());
    #[cfg(feature = "serde")]
    assert_eq!(
        serde_json::from_str::<Snapshot>(&serde_json::to_string(&actual).unwrap()).unwrap(),
        actual
    );
    actual
}

#[shoop_test]
fn recording_repeat_wait_and_same_boundary_sync_chain() {
    let input = input(
        16,
        vec![
            spec(0, 64, None, false),
            spec(1, 0, Some(0), true),
            spec(2, 16, Some(1), false),
        ],
    );
    let (mut sp, p, mut q, mut oracle) = prepare(&input);
    for write in [
        write(0, 0, 1, 0, play(), false, 0),
        write(1, 0, 2, 0, Operation::Record, false, 0),
        write(1, 1, 3, 0, play(), true, 1),
        write(2, 0, 4, 0, play(), true, 1),
    ] {
        sp.write_plans(write.target, write.values(&sp)).unwrap();
        oracle.write(&write);
    }
    for quantum in 0..18 {
        let result = tick(&mut sp, &p, &mut q, &mut oracle);
        if quantum == 4 {
            assert_eq!(result.states[1].mode, Mode::Playing);
            assert_eq!(result.states[1].content_length, 64);
            assert_eq!(result.states[2].last_pulse, Some(64));
        }
    }
}

#[shoop_test]
fn sparse_reordered_dependencies_and_self_sync_keep_deterministic_pulses() {
    let input = input(
        16,
        vec![
            spec(0, 16, Some(129), true),
            spec(3, 32, Some(3), true),
            spec(129, 64, None, false),
        ],
    );
    let (mut sp, p, mut q, mut oracle) = prepare(&input);
    for write in [
        write(129, 0, 1, 0, play(), false, 0),
        write(0, 0, 2, 0, play(), true, 0),
        write(3, 0, 3, 0, play(), false, 0),
        write(3, 1, 4, 0, Operation::Record, true, 1),
    ] {
        sp.write_plans(write.target, write.values(&sp)).unwrap();
        oracle.write(&write);
    }
    for _ in 0..18 {
        tick(&mut sp, &p, &mut q, &mut oracle);
    }
}

#[shoop_test]
fn wide_sequences_and_effective_frames_preserve_priority_without_request_ids() {
    let input = input(1, vec![spec(0, 4, None, false)]);
    let (mut sp, p, mut q, mut oracle) = prepare(&input);
    for write in [
        write(0, 0, u64::MAX - 1, 0, play(), false, 0),
        write(0, 1, 1, 1_u64 << 54, Operation::Record, false, 0),
    ] {
        sp.write_plans(write.target, write.values(&sp)).unwrap();
        oracle.write(&write);
    }
    for _ in 0..12 {
        tick(&mut sp, &p, &mut q, &mut oracle);
    }
}

struct Random(u64);
impl Random {
    fn below(&mut self, limit: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 16) % limit
    }
}
#[shoop_test]
fn seeded_direct_plan_graphs_match_native_resolver() {
    for seed in 0..100 {
        let mut random = Random(seed + 1);
        let quantum = [1, 16, 32, 64, 128][seed as usize % 5];
        let n = 1 + random.below(12) as u32;
        let mut labels: Vec<_> = (0..n).collect();
        for i in (1..labels.len()).rev() {
            labels.swap(i, random.below(i as u64 + 1) as usize);
        }
        let loops: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(i, &slot)| {
                let source =
                    (random.below(4) != 0).then(|| labels[random.below(i as u64 + 1) as usize]);
                spec(
                    slot,
                    random.below(5) * u64::from(quantum),
                    source,
                    random.below(2) != 0,
                )
            })
            .collect();
        let input = input(quantum, loops.clone());
        let (mut sp, p, mut q, mut oracle) = prepare(&input);
        let mut sequence = 0_u64;
        for boundary in 0..80 {
            for _ in 0..random.below(6) {
                let spec = &loops[random.below(n.into()) as usize];
                let plans = sp.loop_state(spec.id).unwrap().planned;
                let Some(index) = plans.iter().position(Option::is_none) else {
                    continue;
                };
                sequence += 1;
                let operation = match random.below(7) {
                    0 => Operation::Stop,
                    1 | 2 => Operation::Record,
                    3 | 4 => Operation::Play {
                        position: random.below(6) * u64::from(quantum),
                        retrigger: random.below(2) != 0,
                    },
                    _ => Operation::Seek {
                        position: random.below(6) * u64::from(quantum),
                        retrigger: random.below(2) != 0,
                    },
                };
                let on_pulse = random.below(2) != 0 && spec.sync_source.is_some();
                let write = write(
                    spec.id.slot,
                    index,
                    sequence.reverse_bits(),
                    (boundary + random.below(3)) * u64::from(quantum),
                    operation,
                    on_pulse,
                    if on_pulse { random.below(3) } else { 0 },
                );
                sp.write_plans(write.target, write.values(&sp)).unwrap();
                oracle.write(&write);
            }
            sp.run(&p, &mut q).unwrap();
            assert_eq!(
                Snapshot::from_sp(&sp),
                oracle.tick(),
                "seed {seed}, boundary {boundary}"
            );
        }
    }
}

#[shoop_test]
fn multi_loop_sync_cycles_are_rejected_by_both_compilers() {
    let input = input(
        16,
        vec![spec(0, 16, Some(1), false), spec(1, 16, Some(0), false)],
    );
    assert!(matches!(
        TransitionPlan::compile(input.description.clone(), input.limits),
        Err(TransitionError::InstantaneousDependencyCycle)
    ));
    assert!(matches!(
        Oracle::prepare(&input),
        Err(TransitionError::InstantaneousDependencyCycle)
    ));
}

#[path = "../../rt_fpga_compiler/tests/support/composite.rs"]
mod composites;

#[shoop_test]
fn composite_seek_resolves_shared_children_like_production() {
    use rt_fpga_compiler::{compile_composites, shoop_graph, CompileContext, StateProgramTarget};
    use rt_fpga_reference::target::RustTarget;
    use shoop_loops as native;

    let graph = composites::arbitration_graph();
    let mut logical = compile_composites(
        &graph,
        CompileContext {
            quantum: 16,
            limits: Default::default(),
        },
        16,
    )
    .unwrap();
    for (slot, length) in [(0, 32), (4, 48), (5, 48)] {
        logical.loops[slot].length = length;
    }
    let native_id = |slot| native::LoopId::from_parts(slot, 1).unwrap();
    let topology = native::TopologyDescription {
        loops: logical
            .loops
            .iter()
            .map(|l| native::LoopSpec {
                id: native_id(l.id.slot),
                kind: if graph.loops[l.id.slot as usize].composite.is_some() {
                    native::LoopKind::Composite
                } else {
                    native::LoopKind::Primitive
                },
                length: l.length,
                sync_source: l.sync_source.map(|s| native_id(s.slot)),
            })
            .collect(),
        programs: graph
            .loops
            .iter()
            .filter_map(|l| {
                let c = l.composite.as_ref()?;
                Some(native::ProgramDescription {
                    source: native_id(l.id.slot),
                    nominal_unit_frames: c.unit_frames,
                    sections: vec![native::Section {
                        entries: c
                            .spans
                            .iter()
                            .map(|span| native::Entry {
                                target: native_id(span.target.slot),
                                delay: span.start,
                                duration: Some(span.end - span.start),
                                repetitions: 1,
                                mode: match (c.kind, span.mode) {
                                    (shoop_graph::CompositeKind::Regular, _) => None,
                                    (_, shoop_graph::LoopMode::Playing) => {
                                        Some(native::Mode::Playing)
                                    }
                                    (_, shoop_graph::LoopMode::Stopped) => {
                                        Some(native::Mode::Stopped)
                                    }
                                    _ => unreachable!(),
                                },
                            })
                            .collect(),
                    }],
                })
            })
            .collect(),
        triggers: vec![],
    };
    let mut resolver = native::Resolver::prepare(
        native::compile(native::Limits::default(), &topology).unwrap(),
        rt_runtime::Quantum::new(16).unwrap(),
    )
    .unwrap();
    let compiled = RustTarget::new(&[]).build(&logical).unwrap();
    let mut sp = StateProcessor::with_loops(compiled.plan, compiled.memory);
    let mut queue = JobQueue::new(compiled.resources.queue_capacity as usize);
    for tick in 0..5 {
        let frame = tick * 16;
        let mut requests = vec![];
        for slot in 0..graph.loops.len() as u32 {
            let (operation, native_operation) = match (tick, slot) {
                (0, 0..=2 | 6) => (
                    play(),
                    native::Transition::Play {
                        position: 0,
                        retrigger: true,
                    },
                ),
                (2, 1) => (
                    Operation::Seek {
                        position: 32,
                        retrigger: false,
                    },
                    native::Transition::SeekIteration {
                        iteration: 2,
                        retrigger: false,
                    },
                ),
                (2, 4) => (Operation::Stop, native::Transition::Stop),
                _ => continue,
            };
            let sequence = tick * 8 + u64::from(slot) + 1;
            sp.write_plans(
                LoopId {
                    slot,
                    generation: 1,
                },
                [
                    Some(PlannedTransition {
                        sequence,
                        operation,
                        effective: frame,
                        on_pulse: false,
                        remaining_ticks: 0,
                    }),
                    None,
                ],
            )
            .unwrap();
            requests.push(
                resolver
                    .submit(native::Request {
                        target: native_id(slot),
                        sequence: native::Sequence(sequence),
                        at: native::Frame(frame),
                        transition: native_operation,
                        timing: native::Timing::At,
                    })
                    .unwrap(),
            );
        }
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        sp.run(&compiled.program, &mut queue).unwrap();
        for decision in resolver.decisions().unwrap() {
            let state = decision.state;
            let slot = sp
                .loop_state(LoopId {
                    slot: state.id.slot(),
                    generation: 1,
                })
                .unwrap();
            assert_eq!(
                slot.render.plays(),
                state.mode == native::Mode::Playing,
                "mode at frame {frame} for {}",
                state.id.slot()
            );
            assert_eq!(
                slot.render.position,
                state.position,
                "position at frame {frame} for {}",
                state.id.slot()
            );
            assert_eq!(
                slot.pulse_this_quantum,
                state.last_pulse == Some(native::Frame(frame)),
                "pulse at frame {frame} for {}",
                state.id.slot()
            );
        }
        resolver.finish().unwrap();
        for id in requests {
            assert_eq!(
                resolver.take_completion(id).unwrap().unwrap().outcome,
                native::Outcome::Applied {
                    frame: native::Frame(frame)
                }
            );
        }
        assert!(sp
            .resolutions()
            .iter()
            .all(|r| r.outcome == Outcome::Applied { frame }));
        while queue.pop().is_some() {}
    }
}
