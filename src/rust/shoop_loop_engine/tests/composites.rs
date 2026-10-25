#[path = "support/composite_reference.rs"]
mod reference;
#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;

use shoop_loop_engine::*;
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn primitive(slot: u32, length: u64, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: sync.map(id),
    }
}
fn composite(slot: u32, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        kind: LoopKind::Composite,
        ..primitive(slot, 0, sync)
    }
}
fn entry(target: u32, delay: u64, duration: u64, mode: Option<Mode>) -> Entry {
    Entry {
        target: id(target),
        delay,
        duration: Some(duration),
        repetitions: 1,
        mode,
    }
}
fn program(source: u32, sections: Vec<Vec<Entry>>) -> ProgramDescription {
    ProgramDescription {
        source: id(source),
        nominal_unit_frames: 2,
        sections: sections
            .into_iter()
            .map(|entries| Section { entries })
            .collect(),
    }
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
    frame: u64,
    transition: Transition,
) -> RequestId {
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(target),
            sequence: Sequence(sequence),
            at: Frame(frame),
            timing: Timing::At,
            transition,
        })
    })
    .unwrap()
}
fn prepare(description: &TopologyDescription) -> Resolver {
    Resolver::prepare(compile(Limits::default(), description).unwrap()).unwrap()
}
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[shoop_test]
fn generated_regular_and_explicit_programs_match_dense_scalar_execution() {
    let mut seed = 0x85ad_3180_e5c7_492b;
    for case in 0..256 {
        let explicit = case % 2 == 0;
        let mut input = TopologyDescription {
            loops: vec![
                primitive(0, random(&mut seed) % 5 + 1, None),
                composite(1, Some(0)),
                primitive(2, random(&mut seed) % 7, None),
                primitive(3, random(&mut seed) % 7, None),
                primitive(4, random(&mut seed) % 7, None),
            ],
            programs: vec![],
            triggers: vec![],
        };
        let mut sections = Vec::new();
        for _ in 0..random(&mut seed) % 3 + 1 {
            let mut entries = Vec::new();
            for _ in 0..random(&mut seed) % 4 + 1 {
                entries.push(Entry {
                    target: id((random(&mut seed) % 3 + 2) as u32),
                    delay: random(&mut seed) % 4,
                    duration: if random(&mut seed) % 2 == 0 {
                        None
                    } else {
                        Some(random(&mut seed) % 3 + 1)
                    },
                    repetitions: (random(&mut seed) % 3 + 1) as u32,
                    mode: if explicit {
                        Some(
                            [Mode::Stopped, Mode::Playing, Mode::Recording]
                                [(random(&mut seed) % 3) as usize],
                        )
                    } else {
                        None
                    },
                });
            }
            sections.push(Section { entries });
        }
        input.programs.push(ProgramDescription {
            source: id(1),
            nominal_unit_frames: random(&mut seed) % 4 + 1,
            sections,
        });
        let expected = reference::simulate(&input, 64);
        let mut irregular = Vec::new();
        let mut remaining = 64;
        while remaining != 0 {
            let count = (random(&mut seed) % 11 + 1).min(remaining);
            irregular.push(count);
            remaining -= count;
        }
        for (order, partitions) in [vec![64], vec![1; 64], irregular].iter().enumerate() {
            let mut description = input.clone();
            if order == 1 {
                description.loops.reverse();
            }
            let mut resolver = prepare(&description);
            start(&mut resolver);
            let (events, rows) = run(&mut resolver, partitions);
            assert_eq!(events, expected.events, "case {case}, partition {order}");
            for (frame, row) in rows.iter().enumerate() {
                let actual: Vec<_> = row
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
                assert_eq!(
                    actual, expected.samples[frame],
                    "case {case}, frame {frame}, partition {order}"
                );
                let cursor = row[1].composite.unwrap();
                assert_eq!(
                    (cursor.iteration, cursor.passes, cursor.entered_at.unwrap()),
                    expected.cursors[frame]
                );
            }
        }
    }
}

fn base() -> TopologyDescription {
    TopologyDescription {
        loops: vec![
            primitive(0, 2, None),
            composite(1, Some(0)),
            primitive(2, 3, None),
            primitive(3, 5, None),
        ],
        programs: vec![program(
            1,
            vec![vec![entry(2, 0, 1, None), entry(3, 1, 2, None)]],
        )],
        triggers: vec![],
    }
}
fn start(resolver: &mut Resolver) {
    submit(resolver, 0, 1, 0, play());
    submit(resolver, 1, 2, 0, play());
}
fn run(resolver: &mut Resolver, parts: &[u64]) -> (Vec<Event>, Vec<Vec<State>>) {
    let mut events = Vec::new();
    let mut rows = Vec::new();
    for &frames in parts {
        let end = Frame(resolver.frame().0 + frames);
        while resolver.frame() < end {
            let interval = assert_no_heap(|| resolver.preview(end, &[])).unwrap();
            assert_no_heap(|| resolver.commit()).unwrap();
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
        }
    }
    (events, rows)
}

#[shoop_test]
fn regular_program_uses_clock_ticks_and_preserves_contiguous_playback() {
    let mut input = base();
    let mut whole = prepare(&input);
    input.loops.reverse();
    let mut split = prepare(&input);
    start(&mut whole);
    start(&mut split);
    let expected = run(&mut whole, &[12]);
    assert_eq!(run(&mut split, &[1; 12]), expected);
    for (frame, states) in expected.1.iter().enumerate() {
        let cursor = states[1].composite.unwrap();
        assert_eq!(
            (cursor.iteration, cursor.passes),
            ((frame / 2 % 3) as u64, (frame / 6) as u64)
        );
        assert_eq!(states[1].position, 0);
        if frame % 6 < 2 {
            assert_eq!(
                (states[2].mode, states[2].position),
                (Mode::Playing, (frame % 6) as u64)
            );
            assert_eq!(states[3].mode, Mode::Stopped);
        } else {
            assert_eq!(states[2].mode, Mode::Stopped);
            assert_eq!(
                (states[3].mode, states[3].position),
                (Mode::Playing, (frame % 6 - 2) as u64)
            );
        }
    }
}

#[shoop_test]
fn nominal_units_do_not_create_ticks_and_an_idle_source_stalls() {
    let mut input = base();
    input.loops[0].length = 7;
    input.programs[0].nominal_unit_frames = 100;
    let mut resolver = prepare(&input);
    start(&mut resolver);
    run(&mut resolver, &[10]);
    let state = resolver.state(id(1)).unwrap();
    assert_eq!(state.composite.unwrap().iteration, 1);
    assert_eq!(state.composite.unwrap().entered_at, Some(Frame(7)));
    assert_eq!(state.content_length, 300);
    input.loops[1].sync_source = None;
    let mut unsynchronized = prepare(&input);
    start(&mut unsynchronized);
    run(&mut unsynchronized, &[20]);
    assert_eq!(
        unsynchronized
            .state(id(1))
            .unwrap()
            .composite
            .unwrap()
            .iteration,
        0
    );
    let mut stopped_source = prepare(&base());
    start(&mut stopped_source);
    submit(&mut stopped_source, 0, 3, 1, Transition::Stop);
    run(&mut stopped_source, &[20]);
    assert_eq!(
        stopped_source
            .state(id(1))
            .unwrap()
            .composite
            .unwrap()
            .iteration,
        0
    );
}

#[shoop_test]
fn explicit_program_records_then_plays_and_finishes_one_pass() {
    let mut input = base();
    input.programs[0] = program(
        1,
        vec![
            vec![entry(2, 0, 2, Some(Mode::Recording))],
            vec![entry(2, 0, 1, Some(Mode::Playing))],
        ],
    );
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let (events, states) = run(&mut resolver, &[10]);
    assert_eq!(
        (states[3][2].mode, states[3][2].capture_frames),
        (Mode::Recording, 3)
    );
    assert_eq!(
        (
            states[4][2].mode,
            states[4][2].position,
            states[4][2].content_length
        ),
        (Mode::Playing, 0, 4)
    );
    assert_eq!(states[6][2].mode, Mode::Stopped);
    assert_eq!(states[6][1].mode, Mode::Stopped);
    assert_eq!(states[6][1].composite.unwrap().passes, 1);
    assert_eq!(
        events
            .iter()
            .filter(|event| event.kind == EventKind::ProgramEnd)
            .map(|event| event.frame)
            .collect::<Vec<_>>(),
        vec![Frame(6)]
    );
}

fn nested() -> TopologyDescription {
    TopologyDescription {
        loops: vec![
            primitive(0, 2, None),
            composite(1, Some(0)),
            composite(2, Some(1)),
            primitive(3, 3, None),
        ],
        programs: vec![
            program(1, vec![vec![entry(2, 0, 3, None)]]),
            program(
                2,
                vec![vec![entry(3, 0, 1, None)], vec![entry(3, 1, 1, None)]],
            ),
        ],
        triggers: vec![],
    }
}

#[shoop_test]
fn nested_start_seek_and_stop_are_authoritative_at_the_same_sample() {
    let input = nested();
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let seek = submit(
        &mut resolver,
        1,
        3,
        1,
        Transition::SeekIteration {
            iteration: 2,
            retrigger: false,
        },
    );
    submit(&mut resolver, 1, 4, 3, Transition::Stop);
    let (events, states) = run(&mut resolver, &[5]);
    assert_eq!(states[0][3].mode, Mode::Playing);
    assert_eq!(states[1][1].composite.unwrap().iteration, 2);
    assert_eq!(states[1][2].composite.unwrap().iteration, 2);
    assert_eq!(states[1][3].position, 0);
    assert!(!events
        .iter()
        .any(|event| event.frame == Frame(1) && event.kind == EventKind::Pulse));
    assert_eq!(states[3][1].mode, Mode::Stopped);
    assert_eq!(states[3][2].mode, Mode::Stopped);
    assert_eq!(states[3][3].mode, Mode::Stopped);
    assert_eq!(
        resolver.completion(seek).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(1) }
    );
}

#[shoop_test]
fn nested_program_journals_match_across_partitions_and_declarations() {
    let input = nested();
    let mut whole = prepare(&input);
    start(&mut whole);
    let expected = run(&mut whole, &[128]);
    let mut shuffled = input;
    shuffled.loops.reverse();
    shuffled.programs.reverse();
    for parts in [vec![1; 128], vec![17, 3, 51, 57]] {
        let mut split = prepare(&shuffled);
        start(&mut split);
        assert_eq!(run(&mut split, &parts), expected);
    }
    let limits = Limits {
        journal_events: 1,
        ..Limits::default()
    };
    let mut limited = Resolver::prepare(compile(limits, &shuffled).unwrap()).unwrap();
    start(&mut limited);
    assert_eq!(run(&mut limited, &[128]).1, expected.1);
    assert!(limited.dropped_events() > 0);
}

#[shoop_test]
fn composite_primitive_composite_chain_has_no_hop_delay() {
    let mut input = base();
    input.loops[3] = composite(3, Some(2));
    input.loops.push(primitive(4, 5, None));
    input.programs[0] = program(1, vec![vec![entry(2, 0, 2, None)]]);
    input
        .programs
        .push(program(3, vec![vec![entry(4, 0, 2, None)]]));
    input.triggers.push(Trigger {
        source: id(2),
        target: id(3),
        mode: Mode::Playing,
    });
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let (_, states) = run(&mut resolver, &[1]);
    for target in 1..5 {
        assert_eq!(states[0][target].mode, Mode::Playing);
    }
    assert_eq!(states[0][4].position, 0);
}

#[shoop_test]
fn pure_sync_component_may_contain_a_composite_but_not_action_feedback() {
    let mut input = base();
    input.loops[0].sync_source = Some(id(1));
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let (events, rows) = run(&mut resolver, &[8]);
    for frame in [0, 2, 4, 6] {
        for target in [0, 1] {
            assert_eq!(
                events
                    .iter()
                    .filter(|event| event.target == id(target)
                        && event.frame == Frame(frame)
                        && event.kind == EventKind::Pulse)
                    .count(),
                1
            );
        }
    }
    assert_eq!(rows[4][1].composite.unwrap().iteration, 2);
    input.programs[0].sections[0]
        .entries
        .push(entry(0, 0, 1, None));
    assert_eq!(
        compile(Limits::default(), &input).unwrap_err(),
        Error::InstantaneousDependencyCycle
    );
}

#[shoop_test]
fn explicit_actions_outrank_inherited_actions_then_source_identity_wins() {
    for explicit_first in [false, true] {
        let input = TopologyDescription {
            loops: vec![
                primitive(0, 2, None),
                composite(1, Some(0)),
                composite(2, Some(0)),
                primitive(3, 4, None),
            ],
            programs: vec![
                program(
                    1,
                    vec![vec![entry(
                        3,
                        0,
                        2,
                        if explicit_first {
                            Some(Mode::Playing)
                        } else {
                            None
                        },
                    )]],
                ),
                program(2, vec![vec![entry(3, 0, 2, Some(Mode::Stopped))]]),
            ],
            triggers: vec![],
        };
        let mut resolver = prepare(&input);
        start(&mut resolver);
        submit(&mut resolver, 2, 3, 0, play());
        run(&mut resolver, &[1]);
        assert_eq!(
            resolver.state(id(3)).unwrap().mode,
            if explicit_first {
                Mode::Playing
            } else {
                Mode::Stopped
            }
        );
    }
}

#[shoop_test]
fn autonomous_actions_do_not_claim_an_unrelated_request_as_transaction_origin() {
    let mut input = base();
    input.triggers.push(Trigger {
        source: id(0),
        target: id(3),
        mode: Mode::Playing,
    });
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let interval = assert_no_heap(|| resolver.preview(Frame(1), &[])).unwrap();
    let follower = resolver
        .decisions()
        .unwrap()
        .iter()
        .find(|decision| decision.state.id == id(3))
        .unwrap();
    assert_eq!(follower.state.mode, Mode::Playing);
    assert_eq!(follower.origin, None);
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    run(&mut resolver, &[1]);
    submit(&mut resolver, 1, 3, 2, play());
    assert_no_heap(|| resolver.preview(Frame(3), &[])).unwrap();
    let natural = resolver
        .decisions()
        .unwrap()
        .iter()
        .find(|decision| decision.state.id == id(2))
        .unwrap();
    assert_eq!(natural.state.mode, Mode::Stopped);
    assert_eq!(natural.origin, None);
}

#[shoop_test]
fn trigger_ordinals_are_disjoint_from_program_ordinals() {
    let mut input = base();
    input.programs[0] = program(1, vec![vec![entry(2, 0, 2, Some(Mode::Playing))]]);
    input.triggers.push(Trigger {
        source: id(1),
        target: id(2),
        mode: Mode::Stopped,
    });
    let mut resolver = prepare(&input);
    start(&mut resolver);
    run(&mut resolver, &[1]);
    assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Stopped);
}

#[shoop_test]
fn empty_children_reserve_time_and_empty_program_is_a_stopped_success() {
    let mut input = base();
    input.loops[2].length = 0;
    let mut resolver = prepare(&input);
    start(&mut resolver);
    let (_, states) = run(&mut resolver, &[4]);
    assert_eq!(states[0][2].mode, Mode::Stopped);
    assert_eq!(states[2][3].mode, Mode::Playing);
    input.programs[0].sections.clear();
    let mut empty = prepare(&input);
    let request = submit(&mut empty, 1, 1, 0, play());
    let (events, _) = run(&mut empty, &[1]);
    assert_eq!(empty.state(id(1)).unwrap().mode, Mode::Stopped);
    assert_eq!(
        empty.completion(request).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(0) }
    );
    assert!(!events.iter().any(|event| event.kind == EventKind::Pulse));
}

#[shoop_test]
fn refused_authoritative_preview_preserves_every_descendant() {
    let mut input = nested();
    input.programs[1] = program(
        2,
        vec![
            vec![entry(3, 0, 1, Some(Mode::Playing))],
            vec![entry(3, 0, 2, Some(Mode::Recording))],
        ],
    );
    let mut resolver = prepare(&input);
    start(&mut resolver);
    run(&mut resolver, &[1]);
    let before: Vec<_> = (1..4)
        .map(|slot| resolver.state(id(slot)).unwrap())
        .collect();
    let seek = submit(
        &mut resolver,
        1,
        3,
        1,
        Transition::SeekIteration {
            iteration: 2,
            retrigger: false,
        },
    );
    assert_no_heap(|| resolver.preview(Frame(2), &[])).unwrap();
    let child = resolver
        .decisions()
        .unwrap()
        .iter()
        .find(|decision| decision.state.id == id(3))
        .unwrap();
    assert_eq!(child.state.mode, Mode::Recording);
    assert_eq!(child.origin, Some(seek));
    assert_no_heap(|| {
        resolver.preview(
            Frame(2),
            &[Observation::Refused {
                request: seek,
                reason: Error::Capacity,
            }],
        )
    })
    .unwrap();
    let interval = assert_no_heap(|| resolver.commit()).unwrap();
    for slot in 1..4 {
        assert_eq!(
            resolver.state(id(slot)).unwrap(),
            before[(slot - 1) as usize]
        );
    }
    assert_eq!(
        resolver.completion(seek).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(1),
            reason: Error::Capacity
        }
    );
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
}

#[shoop_test]
fn deep_nested_programs_execute_all_dense_ticks_without_wave_or_subrange_caps() {
    let depth = 65;
    let mut input = TopologyDescription {
        loops: vec![primitive(0, 1, None)],
        programs: vec![],
        triggers: vec![],
    };
    for source in 1..=depth {
        input.loops.push(composite(source, Some(source - 1)));
        input
            .programs
            .push(program(source, vec![vec![entry(source + 1, 0, 1, None)]]));
    }
    input.loops.push(primitive(depth + 1, 1, None));
    let limits = Limits {
        loop_slots: (depth + 2) as usize,
        ..Limits::default()
    };
    let mut whole = Resolver::prepare(compile(limits, &input).unwrap()).unwrap();
    start(&mut whole);
    let expected = run(&mut whole, &[128]);
    input.loops.reverse();
    input.programs.reverse();
    let mut split = Resolver::prepare(compile(limits, &input).unwrap()).unwrap();
    start(&mut split);
    assert_eq!(run(&mut split, &[1; 128]), expected);
    for (frame, row) in expected.1.iter().enumerate() {
        assert_eq!(row[depth as usize].composite.unwrap().passes, frame as u64);
        assert_eq!(row[(depth + 1) as usize].mode, Mode::Playing);
        assert_eq!(row[(depth + 1) as usize].cycles, frame as u64);
    }
}

#[shoop_test]
fn maximum_sparse_iteration_seek_is_indexed_and_wraps_without_overflow() {
    let mut input = base();
    input.loops[0].length = 1;
    input.loops[2].length = 11;
    input.programs[0] = program(1, vec![vec![entry(2, 0, u64::MAX, None)]]);
    input.programs[0].nominal_unit_frames = 1;
    let mut resolver = prepare(&input);
    submit(&mut resolver, 0, 1, 0, play());
    let request = submit(
        &mut resolver,
        1,
        2,
        0,
        Transition::PlayIteration {
            iteration: u64::MAX - 1,
            retrigger: true,
        },
    );
    let (_, rows) = run(&mut resolver, &[2]);
    assert_eq!(rows[0][1].composite.unwrap().iteration, u64::MAX - 1);
    assert_eq!(rows[0][2].position, (u64::MAX - 1) % 11);
    assert_eq!(
        (
            rows[1][1].composite.unwrap().iteration,
            rows[1][1].composite.unwrap().passes
        ),
        (0, 1)
    );
    assert_eq!(
        resolver.completion(request).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(0) }
    );
}

#[shoop_test]
fn stopped_relay_can_clock_a_program_without_starting_its_own_media() {
    let input = TopologyDescription {
        loops: vec![
            primitive(0, 2, None),
            primitive(1, 7, Some(0)),
            composite(2, Some(1)),
            primitive(3, 4, None),
        ],
        programs: vec![program(2, vec![vec![entry(3, 0, 3, None)]])],
        triggers: vec![],
    };
    let mut resolver = prepare(&input);
    submit(&mut resolver, 0, 1, 0, play());
    submit(&mut resolver, 2, 2, 0, play());
    let (_, rows) = run(&mut resolver, &[3]);
    assert_eq!(rows[2][1].mode, Mode::Stopped);
    assert_eq!(rows[2][1].cycles, 0);
    assert_eq!(rows[2][2].composite.unwrap().iteration, 1);
}

#[shoop_test]
fn running_program_replacement_is_refused_without_dropping_owners() {
    let input = base();
    let mut resolver = prepare(&input);
    start(&mut resolver);
    run(&mut resolver, &[1]);
    let mut changed = input;
    changed.programs[0].sections[0].entries[0].duration = Some(5);
    let candidate = compile(Limits::default(), &changed).unwrap();
    let before = resolver.state(id(1)).unwrap();
    let (error, candidate) =
        assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap_err();
    assert_eq!(error, Error::InvalidState);
    assert_eq!(resolver.state(id(1)).unwrap(), before);
    submit(&mut resolver, 1, 3, 1, Transition::Stop);
    run(&mut resolver, &[1]);
    let retired = assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap();
    assert_eq!(resolver.state(id(1)).unwrap().content_length, 10);
    assert_eq!(
        resolver.state(id(1)).unwrap().composite.unwrap(),
        CompositeState::default()
    );
    drop(retired);
}
