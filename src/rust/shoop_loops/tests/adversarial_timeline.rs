#[path = "support/composite_reference.rs"]
mod reference;
#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;

use shoop_loops::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}
fn controls() {
    let (mut bytes, counts) =
        support::measure(|| Vec::<u8>::with_capacity(std::hint::black_box(1)));
    assert!(counts.allocations > 0);
    let (_, counts) = support::measure(|| bytes.reserve_exact(std::hint::black_box(129)));
    assert!(counts.reallocations > 0);
    std::hint::black_box(&bytes);
    let (_, counts) = support::measure(|| drop(bytes));
    assert!(counts.deallocations > 0);
}
fn play() -> Transition {
    Transition::Play {
        position: 0,
        retrigger: false,
    }
}
fn submit(
    resolver: &mut Resolver,
    target: u32,
    sequence: u64,
    at: u64,
    transition: Transition,
) -> RequestId {
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(target),
            sequence: Sequence(sequence),
            at: Frame(at),
            timing: Timing::At,
            transition,
        })
    })
    .unwrap()
}
fn limits(nodes: usize, pending: usize, journal: usize) -> Limits {
    Limits {
        loop_slots: nodes,
        relations: 32,
        program_entries: 32,
        program_segments: 128,
        pending_requests: pending,
        journal_events: journal,
    }
}
fn run(
    resolver: &mut Resolver,
    parts: &[u64],
    candidate: Option<&TopologyDescription>,
    limits: Limits,
) -> (Vec<Event>, Vec<Vec<State>>) {
    let mut events = Vec::new();
    let mut rows = Vec::new();
    let mut rejected = false;
    for &frames in parts {
        let at = resolver.frame();
        let end = Frame(at.0 + frames);
        while resolver.frame() < end {
            let interval = assert_no_heap(|| resolver.preview(end, &[])).unwrap();
            assert_eq!(assert_no_heap(|| resolver.commit()).unwrap(), interval);
            events.extend_from_slice(resolver.events());
            for offset in 0..interval.end.0 - interval.start.0 {
                rows.push(
                    resolver
                        .decisions()
                        .unwrap()
                        .iter()
                        .map(|decision| {
                            let mut state = decision.state;
                            if state.plays() {
                                state.position += offset;
                            }
                            if state.mode == Mode::Recording {
                                state.capture_frames += offset;
                            }
                            state
                        })
                        .collect(),
                );
            }
            assert_no_heap(|| resolver.finish(interval.end)).unwrap();
            if !rejected {
                if let Some(description) = candidate {
                    let owner = compile(limits, description).unwrap();
                    let address = owner.nodes().as_ptr();
                    let before: Vec<_> = resolver
                        .topology()
                        .nodes()
                        .iter()
                        .map(|node| resolver.state(node.id).unwrap())
                        .collect();
                    let journal = resolver.events().to_vec();
                    let epoch = resolver.epoch();
                    let retained = resolver.retained_requests();
                    let (error, returned) =
                        assert_no_heap(|| resolver.replace_topology(epoch, owner)).unwrap_err();
                    assert_eq!(error, Error::InvalidState);
                    assert_eq!(returned.nodes().as_ptr(), address);
                    assert_eq!(resolver.epoch(), epoch);
                    assert_eq!(resolver.retained_requests(), retained);
                    assert_eq!(resolver.events(), journal);
                    for state in before {
                        assert_eq!(resolver.state(state.id), Ok(state));
                    }
                    drop(returned);
                    rejected = true;
                }
            }
        }
        if frames == 0 {
            assert_eq!(resolver.frame(), at);
        }
    }
    assert_eq!(rejected, candidate.is_some());
    (events, rows)
}
fn release(resolver: &mut Resolver, starts: &[RequestId], waiting: RequestId) {
    let first = starts[0];
    assert_eq!(
        assert_no_heap(|| resolver.take_completion(first))
            .unwrap()
            .unwrap()
            .outcome,
        Outcome::Applied { frame: Frame(0) }
    );
    let reused = submit(resolver, 0, 9, 1000, Transition::Stop);
    for &request in &starts[1..] {
        assert_eq!(
            assert_no_heap(|| resolver.take_completion(request))
                .unwrap()
                .unwrap()
                .outcome,
            Outcome::Applied { frame: Frame(0) }
        );
    }
    assert_eq!(reused.slot(), first.slot());
    assert_ne!(reused.generation(), first.generation());
    assert_eq!(
        assert_no_heap(|| resolver.cancel(first)),
        Err(Error::InvalidId)
    );
    for request in [reused, waiting] {
        assert_eq!(
            assert_no_heap(|| resolver.cancel(request)),
            Ok(CancelResult::Cancelled)
        );
        assert_eq!(
            assert_no_heap(|| resolver.take_completion(request))
                .unwrap()
                .unwrap()
                .outcome,
            Outcome::Cancelled { frame: Frame(64) }
        );
    }
    assert_eq!(resolver.retained_requests(), 0);
}

#[shoop_test]
fn seeded_programs_preserve_scalar_timing_through_full_receipts_journal_loss_and_rejected_owners() {
    controls();
    for mut seed in [
        0x913c_8021_4b27_u64,
        0x359a_74b1_c983,
        0x7129_a310_e285,
        0xe26a_8917_3c51,
    ] {
        for case in 0..8 {
            let mut input = TopologyDescription {
                loops: (0..5)
                    .map(|slot| LoopSpec {
                        id: id(slot),
                        kind: if slot == 1 {
                            LoopKind::Composite
                        } else {
                            LoopKind::Primitive
                        },
                        length: if slot == 0 {
                            random(&mut seed) % 4 + 1
                        } else if slot == 1 {
                            0
                        } else {
                            random(&mut seed) % 7
                        },
                        sync_source: (slot == 1).then_some(id(0)),
                    })
                    .collect(),
                programs: vec![],
                triggers: vec![],
            };
            let explicit = case % 2 == 0;
            let sections = (0..2)
                .map(|section| Section {
                    entries: (0..2)
                        .map(|index| Entry {
                            target: id(if section == 0 && index == 0 {
                                2
                            } else {
                                (random(&mut seed) % 3 + 2) as u32
                            }),
                            delay: random(&mut seed) % 3,
                            duration: Some(random(&mut seed) % 3 + 2),
                            repetitions: (random(&mut seed) % 2 + 1) as u32,
                            mode: explicit.then(|| {
                                [Mode::Stopped, Mode::Playing, Mode::Recording]
                                    [(random(&mut seed) % 3) as usize]
                            }),
                        })
                        .collect(),
                })
                .collect();
            input.programs.push(ProgramDescription {
                source: id(1),
                nominal_unit_frames: random(&mut seed) % 4 + 1,
                sections,
            });
            let expected = reference::simulate(&input, 64);
            let mut bad = input.clone();
            bad.programs[0].nominal_unit_frames = 0;
            assert_eq!(
                compile(limits(5, 3, 512), &bad).unwrap_err(),
                Error::InvalidProgram
            );
            bad = input.clone();
            bad.programs[0].sections[0].entries[0].delay = u64::MAX;
            assert_eq!(
                compile(limits(5, 3, 512), &bad).unwrap_err(),
                Error::Exhausted
            );
            bad = input.clone();
            bad.triggers.push(Trigger {
                source: id(2),
                target: id(1),
                mode: Mode::Playing,
            });
            assert_eq!(
                compile(limits(5, 3, 512), &bad).unwrap_err(),
                Error::InstantaneousDependencyCycle
            );
            let mut changed = input.clone();
            changed.programs[0].nominal_unit_frames += 1;
            let mut baseline = None;
            for journal in [512, 0, 1] {
                for (order, parts) in [vec![64], vec![1; 64], vec![0, 3, 1, 0, 17, 43, 0]]
                    .iter()
                    .enumerate()
                {
                    let mut description = input.clone();
                    if order == 1 {
                        description.loops.reverse();
                    }
                    if order == 2 {
                        description.loops.rotate_left(2);
                    }
                    let limits = limits(5, 3, journal);
                    let mut resolver =
                        Resolver::prepare(compile(limits, &description).unwrap()).unwrap();
                    let starts = [
                        submit(&mut resolver, 0, 1, 0, play()),
                        submit(&mut resolver, 1, 2, 0, play()),
                    ];
                    let waiting = submit(&mut resolver, 0, 3, 1000, Transition::Stop);
                    assert_eq!(
                        assert_no_heap(|| resolver.submit(Request {
                            target: id(0),
                            sequence: Sequence(4),
                            at: Frame(1001),
                            timing: Timing::At,
                            transition: play()
                        })),
                        Err(Error::Capacity)
                    );
                    let (events, rows) = run(&mut resolver, parts, Some(&changed), limits);
                    assert_eq!(resolver.retained_requests(), 3);
                    assert!(resolver.completion(waiting).unwrap().is_none());
                    assert_eq!(
                        resolver.dropped_events(),
                        (expected.events.len() - events.len()) as u64
                    );
                    if journal == 512 {
                        assert_eq!(events, expected.events);
                    }
                    for (frame, row) in rows.iter().enumerate() {
                        let samples: Vec<_> = row
                            .iter()
                            .map(|state| {
                                (
                                    state.mode,
                                    if state.plays() { state.position } else { 0 },
                                    state.cycles,
                                    state.capture_frames,
                                    state.content_length,
                                )
                            })
                            .collect();
                        assert_eq!(samples, expected.samples[frame]);
                        let cursor = row[1].composite.unwrap();
                        assert_eq!(
                            (cursor.iteration, cursor.passes, cursor.entered_at.unwrap()),
                            expected.cursors[frame]
                        );
                    }
                    if let Some(before) = &baseline {
                        assert_eq!(&rows, before);
                    } else {
                        baseline = Some(rows);
                    }
                    release(&mut resolver, &starts, waiting);
                }
            }
        }
    }
}

fn event_key(event: &Event) -> (Frame, LoopId, u8) {
    (
        event.frame,
        event.target,
        match event.kind {
            EventKind::Transition(Transition::Play {
                position: 0,
                retrigger: false,
            }) => 0,
            EventKind::Wrap => 1,
            EventKind::Pulse => 2,
            _ => panic!("unexpected occurrence {event:?}"),
        },
    )
}

#[shoop_test]
fn seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals(
) {
    controls();
    for mut seed in [0x47e2_b913_u64, 0x913a_0297, 0xb82d_6517, 0xf123_14a9] {
        for _ in 0..8 {
            let count = (random(&mut seed) % 16 + 2) as u32;
            let period = random(&mut seed) % 7 + 1;
            let input = TopologyDescription {
                loops: (0..count)
                    .map(|slot| LoopSpec {
                        id: id(slot),
                        kind: LoopKind::Primitive,
                        length: if slot == 0 { period } else { 11 },
                        sync_source: Some(id(if slot == 0 { count - 1 } else { slot - 1 })),
                    })
                    .collect(),
                programs: vec![],
                triggers: vec![],
            };
            let mut expected = vec![Event {
                frame: Frame(0),
                target: id(0),
                kind: EventKind::Transition(play()),
            }];
            for frame in (0..64).filter(|frame| frame % period == 0) {
                if frame != 0 {
                    expected.push(Event {
                        frame: Frame(frame),
                        target: id(0),
                        kind: EventKind::Wrap,
                    });
                }
                for slot in 0..count {
                    expected.push(Event {
                        frame: Frame(frame),
                        target: id(slot),
                        kind: EventKind::Pulse,
                    });
                }
            }
            expected.sort_by_key(event_key);
            let mut baseline = None;
            let mut ordered = None;
            for journal in [512, 0, 1] {
                for (order, parts) in [vec![64], vec![1; 64], vec![0, 3, 1, 0, 17, 43, 0]]
                    .iter()
                    .enumerate()
                {
                    let mut description = input.clone();
                    if order == 1 {
                        description.loops.reverse();
                    }
                    if order == 2 {
                        description.loops.rotate_left(1);
                    }
                    let limits = limits(count as usize, 2, journal);
                    let mut resolver =
                        Resolver::prepare(compile(limits, &description).unwrap()).unwrap();
                    let start = submit(&mut resolver, 0, 1, 0, play());
                    let waiting = assert_no_heap(|| {
                        resolver.submit(Request {
                            target: id(0),
                            sequence: Sequence(2),
                            at: Frame(0),
                            timing: Timing::AfterTicks {
                                source: id(count - 1),
                                skip: u64::MAX,
                            },
                            transition: Transition::Stop,
                        })
                    })
                    .unwrap();
                    assert_eq!(
                        assert_no_heap(|| resolver.submit(Request {
                            target: id(0),
                            sequence: Sequence(3),
                            at: Frame(0),
                            timing: Timing::At,
                            transition: Transition::Stop
                        })),
                        Err(Error::Capacity)
                    );
                    let (events, rows) = run(&mut resolver, parts, None, limits);
                    assert_eq!(resolver.retained_requests(), 2);
                    assert!(resolver.completion(waiting).unwrap().is_none());
                    assert_eq!(
                        resolver.dropped_events(),
                        (expected.len() - events.len()) as u64
                    );
                    if journal == 512 {
                        if let Some(before) = &ordered {
                            assert_eq!(&events, before);
                        } else {
                            ordered = Some(events.clone());
                        }
                        let mut canonical = events;
                        canonical.sort_by_key(event_key);
                        assert_eq!(canonical, expected);
                    }
                    for (frame, row) in rows.iter().enumerate() {
                        for (slot, state) in row.iter().enumerate() {
                            assert_eq!(
                                state.mode,
                                if slot == 0 {
                                    Mode::Playing
                                } else {
                                    Mode::Stopped
                                }
                            );
                            assert_eq!(
                                state.position,
                                if slot == 0 { frame as u64 % period } else { 0 }
                            );
                            assert_eq!(
                                state.cycles,
                                if slot == 0 { frame as u64 / period } else { 0 }
                            );
                            assert_eq!(
                                state.last_pulse,
                                Some(Frame(frame as u64 / period * period))
                            );
                        }
                    }
                    if let Some(before) = &baseline {
                        assert_eq!(&rows, before);
                    } else {
                        baseline = Some(rows);
                    }
                    release(&mut resolver, &[start], waiting);
                }
            }
        }
    }
}
