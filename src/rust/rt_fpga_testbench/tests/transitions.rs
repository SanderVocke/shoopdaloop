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
        previews: vec![vec![]],
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
    first.previews = vec![vec![], vec![], vec![]];
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
    assert_eq!(trace.results[4].states[1].position, 0);
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
fn retries_refusals_arbitration_cancellation_and_slot_reuse() {
    let mut first = step(vec![
        command(0, 1, 0, Operation::Record),
        command(0, 2, 0, play()),
        command(1, 3, 97, play()),
    ]);
    first.collect_completed = false;
    first.previews = vec![
        vec![Observation::Refused {
            request: RequestId {
                slot: 1,
                generation: 1,
            },
            reason: TransitionError::Capacity,
        }],
        vec![],
    ];
    let mut second = step(vec![command(0, 4, 0, Operation::Record)]);
    second.cancel = vec![RequestId {
        slot: 2,
        generation: 1,
    }];
    second.previews = vec![
        vec![],
        vec![Observation::CaptureUnavailable {
            target: id(0),
            reason: TransitionError::Capacity,
        }],
        vec![],
    ];
    let third = step(vec![
        command(0, 5, 0, play()),
        command(0, 6, 80, Operation::Record),
        command(0, 7, 64, Operation::Stop),
    ]);
    let trace = compare(case(
        16,
        vec![spec(0, 32, None, false), spec(1, 32, Some(0), false)],
        vec![
            first,
            second,
            third,
            step(vec![]),
            step(vec![]),
            step(vec![]),
            step(vec![]),
        ],
    ));
    assert_eq!(
        trace.results[1].submitted[0],
        Response::Accepted(RequestId {
            slot: 0,
            generation: 2
        })
    );
    assert_eq!(trace.results[2].states[0].content_length, 16);
    assert_eq!(trace.results[4].states[0].mode, Mode::Stopped);
}

#[shoop_test]
fn malformed_commands_and_observations_leave_committed_state_intact() {
    let mut first = step(vec![command(
        0,
        1,
        0,
        Operation::Seek {
            position: 0,
            retrigger: false,
        },
    )]);
    first.commands.extend([
        command(0, 1, 0, play()),
        command(
            0,
            2,
            0,
            Operation::Play {
                position: 1,
                retrigger: false,
            },
        ),
        command(0, 3, u64::MAX, play()),
        command(9, 4, 0, play()),
        Command {
            timing: Timing::AfterTicks {
                source: id(1),
                skip: 0,
            },
            ..command(0, 5, 0, play())
        },
    ]);
    let duplicate = Observation::CaptureUnavailable {
        target: id(0),
        reason: TransitionError::Capacity,
    };
    first.previews = vec![
        vec![duplicate, duplicate],
        vec![],
        vec![Observation::CaptureUnavailable {
            target: id(9),
            reason: TransitionError::Capacity,
        }],
    ];
    let trace = compare(case(
        16,
        vec![spec(0, 32, None, false), spec(1, 32, None, false)],
        vec![first, step(vec![])],
    ));
    assert!(!trace.results[0].committed);
    assert_eq!(trace.results[0].next_frame, 0);
    assert_eq!(
        trace.results[1].requests[0].unwrap().outcome,
        Some(Outcome::Failed {
            frame: 0,
            reason: TransitionError::InvalidState
        })
    );
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
    assert_eq!(trace.results[1].after_finish[0].position, u64::MAX - 31);
    assert_eq!(trace.results[2].states[0].position, 0);
}

#[shoop_test]
fn oracle_adapter_enforces_the_hardware_observation_limit() {
    let mut first = step(vec![command(0, 1, 0, play())]);
    first.previews = vec![
        vec![
            Observation::CaptureUnavailable {
                target: id(0),
                reason: TransitionError::Capacity,
            };
            45
        ],
        vec![],
    ];
    let trace = compare(case(16, vec![spec(0, 32, None, false)], vec![first]));
    assert_eq!(
        trace.results[0].previews[0],
        Response::Rejected(TransitionError::Capacity)
    );
    assert_eq!(trace.results[0].states[0].mode, Mode::Playing);
}

#[shoop_test]
fn sparse_cross_word_pulses_preserve_priority_when_lower_ids_become_ready_later() {
    let mut first = step(vec![
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
    first.previews = vec![
        vec![Observation::CaptureUnavailable {
            target: id(64),
            reason: TransitionError::Capacity,
        }],
        vec![],
        vec![],
    ];
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

#[shoop_test]
fn pending_indexes_survive_head_removal_slot_reuse_arbitration_and_retry() {
    let sync = |target, sequence, operation, skip| Command {
        timing: Timing::AfterTicks {
            source: id(0),
            skip,
        },
        ..command(target, sequence, 0, operation)
    };
    let mut first = step(vec![
        command(0, 1, 0, play()),
        sync(1, 2, play(), 3),
        sync(2, 3, play(), 4),
        sync(1, 4, Operation::Record, 1),
        command(1, 5, 48, Operation::Stop),
        sync(
            1,
            6,
            Operation::Seek {
                position: 0,
                retrigger: true,
            },
            1,
        ),
    ]);
    first.previews = vec![vec![], vec![], vec![]];
    let mut second = step(vec![sync(2, 7, play(), 0), sync(1, 8, play(), 2)]);
    second.cancel = vec![RequestId {
        slot: 1,
        generation: 1,
    }];
    let mut third = step(vec![
        sync(1, 9, play(), 0),
        command(1, 10, 32, Operation::Record),
        command(2, 12, 32, Operation::Stop),
    ]);
    third.previews = vec![
        vec![Observation::Refused {
            request: RequestId {
                slot: 1,
                generation: 2,
            },
            reason: TransitionError::Capacity,
        }],
        vec![],
    ];
    let trace = compare(case(
        16,
        vec![
            spec(0, 16, None, false),
            spec(1, 16, Some(0), false),
            spec(2, 16, Some(0), false),
        ],
        vec![
            first,
            second,
            third,
            step(vec![]),
            step(vec![]),
            step(vec![]),
        ],
    ));
    assert_eq!(
        trace.results[1].submitted[0],
        Response::Accepted(RequestId {
            slot: 0,
            generation: 2
        })
    );
    assert_eq!(
        trace.results[2].requests[2].unwrap().outcome,
        Some(Outcome::Cancelled { frame: 32 })
    );
    assert_eq!(trace.results[3].states[1].mode, Mode::Playing);
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
            let mut next = step(commands);
            if random.below(3) == 0 {
                let observation = Observation::CaptureUnavailable {
                    target: id(random.below(n.into()) as u32),
                    reason: TransitionError::Capacity,
                };
                next.previews.insert(0, vec![observation]);
            }
            steps.push(next);
        }
        let mut input = case(q, loops, steps);
        input.name = format!("seed {seed}");
        compare(input);
    }
}
