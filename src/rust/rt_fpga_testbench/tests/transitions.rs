use rt_fpga_reference::transitions::*;
use rt_fpga_testbench::transitions::*;
use rt_fpga_testbench::{Document, Payload};
use shoop_wasm_test_support::shoop_test;

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
fn step(commands: Vec<Command>) -> TransitionStep {
    TransitionStep {
        collect_completed: true,
        collect: vec![],
        cancel: vec![],
        commands,
    }
}
fn command(target: u32, sequence: u64, at: u64, operation: Operation) -> Command {
    Command {
        target: id(target),
        sequence,
        at,
        operation,
        timing: Timing::At,
    }
}
fn play() -> Operation {
    Operation::Play {
        position: 0,
        retrigger: true,
    }
}
fn case(quantum: u32, loops: Vec<LoopSpec>, steps: Vec<TransitionStep>) -> TransitionCase {
    TransitionCase {
        name: "primitive boundaries".into(),
        plan: TransitionPlanInput {
            description: PlanDescription { quantum, loops },
            limits: TransitionLimits {
                loop_slots: 12,
                pending_requests: 32,
                events: 56,
            },
        },
        steps,
    }
}
fn compare(case: TransitionCase) -> TransitionTrace {
    let model = run_transition_case(case.clone()).unwrap();
    let oracle = run_transition_oracle(case).unwrap();
    assert_eq!(model.results.len(), oracle.results.len());
    for (i, (actual, expected)) in model.results.iter().zip(&oracle.results).enumerate() {
        assert_eq!(actual, expected, "case {}, step {i}", model.case.name);
    }
    let document = Document::new(Payload::TransitionTrace(model.clone()));
    assert_eq!(
        Document::from_json(&document.to_json().unwrap()).unwrap(),
        document
    );
    model
}

#[shoop_test]
fn recording_repeat_wait_and_same_boundary_sync_chain() {
    let mut first = step(vec![
        command(0, 1, 0, play()),
        command(1, 2, 0, Operation::Record),
    ]);
    first.commands.push(Command {
        timing: Timing::AfterTicks {
            source: id(0),
            skip: 1,
        },
        ..command(1, 3, 0, play())
    });
    first.commands.push(Command {
        timing: Timing::AfterTicks {
            source: id(1),
            skip: 1,
        },
        ..command(2, 4, 0, play())
    });
    let trace = compare(case(
        16,
        vec![
            spec(0, 64, None, false),
            spec(1, 0, Some(0), true),
            spec(2, 16, Some(1), false),
        ],
        std::iter::once(first)
            .chain((0..17).map(|_| step(vec![])))
            .collect(),
    ));
    assert_eq!(trace.results[4].states[1].mode, Mode::Playing);
    assert_eq!(trace.results[4].states[1].content_length, 64);
    assert_eq!(trace.results[4].states[2].mode, Mode::Playing);
    assert_eq!(trace.results[4].states[2].last_pulse, Some(64));
}

#[shoop_test]
fn repeat_holds_endpoint_until_source_pulse() {
    let trace = compare(case(
        16,
        vec![spec(0, 64, None, false), spec(1, 16, Some(0), true)],
        std::iter::once(step(vec![
            command(0, 1, 0, play()),
            command(1, 2, 0, play()),
        ]))
        .chain((0..8).map(|_| step(vec![])))
        .collect(),
    ));
    assert_eq!(trace.results[1].states[1].position, 16);
    assert_eq!(trace.results[3].states[1].position, 16);
    assert_eq!(trace.results[4].states[1].position, 16);
    assert_eq!(trace.results[4].states[1].cycles, 1);
}

#[shoop_test]
fn cycles_self_sync_and_declaration_reordering_are_deterministic() {
    let loops = vec![
        spec(0, 16, Some(1), true),
        spec(1, 32, Some(0), true),
        spec(3, 16, Some(3), true),
        spec(7, 16, Some(1), false),
    ];
    let mut first = step(vec![
        command(0, 1, 0, play()),
        command(1, 2, 0, play()),
        command(3, 3, 0, play()),
    ]);
    first.commands.push(Command {
        timing: Timing::AfterTicks {
            source: id(1),
            skip: 0,
        },
        ..command(7, 4, 0, play())
    });
    let steps: Vec<_> = std::iter::once(first)
        .chain((0..16).map(|_| step(vec![])))
        .collect();
    let forward = compare(case(16, loops.clone(), steps.clone()));
    let reversed = compare(case(16, loops.into_iter().rev().collect(), steps));
    assert_eq!(forward.results, reversed.results);
    for result in forward.results {
        for state in result.states {
            assert!(
                result
                    .events
                    .iter()
                    .filter(|e| e.target == state.id && e.kind == EventKind::Pulse)
                    .count()
                    <= 1
            );
        }
    }
}

#[shoop_test]
fn wide_integers_roundtrip_and_timing_rounds_up() {
    let trace = compare(case(
        32,
        vec![spec(0, u64::MAX - 31, None, false)],
        vec![
            step(vec![command(
                0,
                u64::MAX,
                1,
                Operation::Play {
                    position: u64::MAX - 63,
                    retrigger: true,
                },
            )]),
            step(vec![]),
            step(vec![]),
        ],
    ));
    assert_eq!(trace.results[0].requests[0].unwrap().effective, 32);
    assert_eq!(trace.results[1].states[0].position, u64::MAX - 31);
    assert_eq!(trace.results[2].states[0].position, 32);
}

#[shoop_test]
fn sparse_cross_word_pulses_preserve_priority_when_lower_ids_become_ready_later() {
    let first = step(vec![
        command(129, 1, 0, play()),
        Command {
            timing: Timing::AfterTicks {
                source: id(129),
                skip: 0,
            },
            ..command(0, 2, 0, play())
        },
        Command {
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 0,
            },
            ..command(63, 3, 0, play())
        },
        Command {
            timing: Timing::AfterTicks {
                source: id(129),
                skip: 0,
            },
            ..command(64, 4, 0, Operation::Record)
        },
        command(64, 5, 64, play()),
    ]);
    let mut input = case(
        16,
        vec![
            spec(129, 16, Some(0), false),
            spec(64, 16, Some(129), false),
            spec(0, 32, Some(129), false),
            spec(63, 16, Some(0), true),
        ],
        std::iter::once(first)
            .chain((0..12).map(|_| step(vec![])))
            .collect(),
    );
    input.plan.limits = TransitionLimits {
        loop_slots: 130,
        pending_requests: 8,
        events: 268,
    };
    let trace = compare(input);
    let pulses: Vec<_> = trace.results[0]
        .events
        .iter()
        .filter(|e| e.kind == EventKind::Pulse)
        .map(|e| e.target)
        .collect();
    assert_eq!(pulses, vec![id(129), id(0), id(63), id(64)]);
}

struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 ^ (self.0 >> 29)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[shoop_test]
fn seeded_primitive_graphs_match_native_resolver() {
    for seed in 0..100 {
        let mut random = Random(seed + 1);
        let q = [1, 16, 32, 64, 128][seed as usize % 5];
        let n = 1 + random.below(12) as u32;
        let loops: Vec<_> = (0..n)
            .map(|i| {
                let source = (random.below(4) != 0).then(|| random.below(n.into()) as u32);
                spec(
                    i,
                    random.below(5) * u64::from(q),
                    source,
                    random.below(2) != 0,
                )
            })
            .collect();
        let mut sequence = 0;
        let mut steps = Vec::new();
        for quantum in 0..80 {
            let mut commands = Vec::new();
            for _ in 0..random.below(6) {
                let target = random.below(n.into()) as u32;
                sequence += 1;
                let operation = match random.below(7) {
                    0 => Operation::Stop,
                    1 | 2 => Operation::Record,
                    3 | 4 => Operation::Play {
                        position: random.below(6) * u64::from(q),
                        retrigger: random.below(2) != 0,
                    },
                    _ => Operation::Seek {
                        position: random.below(6) * u64::from(q),
                        retrigger: random.below(2) != 0,
                    },
                };
                let timing = if random.below(2) != 0 {
                    loops[target as usize]
                        .sync_source
                        .map(|source| Timing::AfterTicks {
                            source,
                            skip: random.below(3),
                        })
                        .unwrap_or(Timing::At)
                } else {
                    Timing::At
                };
                commands.push(Command {
                    timing,
                    ..command(
                        target,
                        sequence,
                        quantum * u64::from(q) + random.below(3 * u64::from(q)),
                        operation,
                    )
                });
            }
            let next = step(commands);
            steps.push(next);
        }
        let mut input = case(q, loops, steps);
        input.name = format!("seed {seed}");
        compare(input);
    }
}
