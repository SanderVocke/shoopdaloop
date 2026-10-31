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
fn play(retrigger: bool) -> Transition {
    Transition::Play {
        position: 0,
        retrigger,
    }
}
fn description(explicit: bool) -> TopologyDescription {
    TopologyDescription {
        loops: (0..5)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 1 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: [2, 0, 3, 5, 0][slot as usize],
                sync_source: match slot {
                    0 | 1 => Some(id(4)),
                    4 => Some(id(0)),
                    _ => None,
                },
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(1),
            nominal_unit_frames: 2,
            sections: vec![Section {
                entries: vec![
                    Entry {
                        target: id(2),
                        delay: 0,
                        duration: Some(1),
                        repetitions: 1,
                        mode: explicit.then_some(Mode::Playing),
                    },
                    Entry {
                        target: id(3),
                        delay: 1,
                        duration: Some(2),
                        repetitions: 1,
                        mode: explicit.then_some(Mode::Playing),
                    },
                ],
            }],
        }],
        triggers: vec![],
    }
}
fn prepare(input: &TopologyDescription, journal: usize, pending: usize) -> Resolver {
    Resolver::prepare(
        compile(
            Limits {
                loop_slots: 6,
                relations: 12,
                program_entries: 8,
                program_segments: 16,
                pending_requests: pending,
                journal_events: journal,
            },
            input,
        )
        .unwrap(),
    )
    .unwrap()
}
fn submit(
    resolver: &mut Resolver,
    target: u32,
    sequence: u64,
    at: u64,
    timing: Timing,
    transition: Transition,
) -> RequestId {
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(target),
            sequence: Sequence(sequence),
            at: Frame(at),
            timing,
            transition,
        })
    })
    .unwrap()
}
fn start(resolver: &mut Resolver) -> [RequestId; 2] {
    [
        submit(resolver, 0, 1, 0, Timing::At, play(false)),
        submit(resolver, 1, 2, 0, Timing::At, play(false)),
    ]
}
fn advance(resolver: &mut Resolver, end: u64) {
    while resolver.frame() < Frame(end) {
        let interval = assert_no_heap(|| resolver.preview(Frame(end), &[])).unwrap();
        assert_no_heap(|| resolver.commit()).unwrap();
        assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    }
}

#[shoop_test]
fn unpublished_program_cuts_match_scalar_clocks_across_partitions_orders_and_unread_results() {
    for explicit in [false, true] {
        for (parts, journal, reverse) in [
            (vec![128], 1024, false),
            (vec![1; 128], 0, true),
            (vec![3, 2, 7, 1, 19, 5, 31, 60], 1, true),
        ] {
            let mut input = description(explicit);
            if reverse {
                input.loops.reverse();
                input.programs.reverse();
            }
            let mut resolver = prepare(&input, journal, 2);
            let mut baseline = prepare(&input, journal, 2);
            let requests = start(&mut resolver);
            start(&mut baseline);
            for frames in parts {
                let end = Frame(resolver.frame().0 + frames);
                while resolver.frame() < end {
                    let f = resolver.frame().0;
                    let before = resolver.state(id(1)).unwrap();
                    let completions = requests.map(|r| resolver.completion(r).unwrap());
                    for _ in 0..3 {
                        let probe =
                            assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
                        let ended = explicit && f >= 6;
                        let (iteration, passes, entered) = if ended {
                            (0, 1, 6)
                        } else {
                            ((f / 2) % 3, f / 6, f / 2 * 2)
                        };
                        assert_eq!(probe.frame, Frame(f));
                        assert_eq!(probe.topology_epoch, Epoch(0));
                        assert_eq!(
                            probe.state.mode,
                            if ended { Mode::Stopped } else { Mode::Playing }
                        );
                        assert_eq!(
                            probe.state.composite.unwrap(),
                            CompositeState {
                                iteration,
                                passes,
                                entered_at: Some(Frame(entered))
                            }
                        );
                        assert_eq!(probe.state.last_pulse, Some(Frame(f / 2 * 2)));
                        assert_eq!(probe.enters_zero(), !ended && f % 6 == 0);
                        assert_eq!(resolver.state(id(1)).unwrap(), before);
                        assert_eq!(
                            requests.map(|r| resolver.completion(r).unwrap()),
                            completions
                        );
                        assert_eq!(resolver.decisions(), Err(Error::InvalidState));
                        assert_eq!(
                            assert_no_heap(|| resolver.commit()),
                            Err(Error::InvalidState)
                        );
                    }
                    let probe =
                        assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
                    let interval = assert_no_heap(|| resolver.preview(end, &[])).unwrap();
                    let expected = assert_no_heap(|| baseline.preview(end, &[])).unwrap();
                    assert_eq!(interval, expected);
                    assert_eq!(resolver.decisions().unwrap(), baseline.decisions().unwrap());
                    assert_eq!(
                        resolver
                            .decisions()
                            .unwrap()
                            .iter()
                            .find(|d| d.state.id == id(1))
                            .unwrap()
                            .state,
                        probe.state
                    );
                    assert_no_heap(|| resolver.commit()).unwrap();
                    assert_no_heap(|| baseline.commit()).unwrap();
                    assert_eq!(resolver.events(), baseline.events());
                    assert_eq!(resolver.dropped_events(), baseline.dropped_events());
                    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
                    assert_no_heap(|| baseline.finish(expected.end)).unwrap();
                    assert_eq!(resolver.retained_requests(), 2);
                }
            }
            assert_eq!(resolver.frame(), Frame(128));
            assert_eq!(
                resolver.completion(requests[0]).unwrap().unwrap().outcome,
                Outcome::Applied { frame: Frame(0) }
            );
        }
    }
}

#[shoop_test]
fn prospective_zero_cut_obeys_same_frame_control_priority_and_actual_source_pulses() {
    for case in 0..11 {
        let mut resolver = prepare(&description(false), 1024, 6);
        start(&mut resolver);
        advance(&mut resolver, 6);
        let transition = match case {
            1 | 6 | 8 | 9 | 10 => Transition::Stop,
            2 => Transition::SeekIteration {
                iteration: 1,
                retrigger: false,
            },
            3 | 7 => Transition::SeekIteration {
                iteration: 0,
                retrigger: false,
            },
            4 => play(true),
            _ => play(false),
        };
        let request = if case == 0 {
            None
        } else {
            Some(submit(
                &mut resolver,
                if case == 10 { 0 } else { 1 },
                3,
                6,
                if case == 9 {
                    Timing::AfterTicks {
                        source: id(4),
                        skip: 0,
                    }
                } else {
                    Timing::At
                },
                transition,
            ))
        };
        if case == 6 {
            assert_no_heap(|| resolver.cancel(request.unwrap())).unwrap();
        }
        if case == 8 {
            submit(&mut resolver, 1, 4, 6, Timing::At, play(true));
        }
        let observations = if case == 7 {
            vec![Observation::Refused {
                request: request.unwrap(),
                reason: Error::Capacity,
            }]
        } else {
            vec![]
        };
        let before = resolver.state(id(1)).unwrap();
        let probe =
            assert_no_heap(|| resolver.probe_program_boundary(id(1), &observations)).unwrap();
        let (mode, iteration, passes, entered, last, zero) = match case {
            1 => (Mode::Stopped, 0, 0, 6, 4, false),
            2 => (Mode::Playing, 1, 0, 6, 6, false),
            3 | 4 | 8 => (Mode::Playing, 0, 0, 6, 6, true),
            9 => (Mode::Stopped, 0, 0, 6, 6, false),
            10 => (Mode::Playing, 2, 0, 4, 4, false),
            _ => (Mode::Playing, 0, 1, 6, 6, true),
        };
        assert_eq!(probe.state.mode, mode, "case {case}");
        assert_eq!(
            probe.state.composite.unwrap(),
            CompositeState {
                iteration,
                passes,
                entered_at: Some(Frame(entered))
            },
            "case {case}"
        );
        assert_eq!(probe.state.last_pulse, Some(Frame(last)), "case {case}");
        assert_eq!(probe.enters_zero(), zero, "case {case}");
        assert_eq!(resolver.state(id(1)).unwrap(), before);
        if let Some(request) = request {
            assert_eq!(resolver.completion(request).unwrap().is_some(), case == 6);
        }
        let interval = assert_no_heap(|| resolver.preview(Frame(7), &observations)).unwrap();
        assert_no_heap(|| resolver.commit()).unwrap();
        assert_eq!(resolver.state(id(1)).unwrap(), probe.state);
        if let Some(request) = request {
            let expected = match case {
                6 => Outcome::Cancelled { frame: Frame(6) },
                7 => Outcome::Failed {
                    frame: Frame(6),
                    reason: Error::Capacity,
                },
                8 => Outcome::Superseded { frame: Frame(6) },
                _ => Outcome::Applied { frame: Frame(6) },
            };
            assert_eq!(
                resolver.completion(request).unwrap().unwrap().outcome,
                expected
            );
        }
        assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    }
}

#[shoop_test]
fn nested_request_refusal_invalid_inputs_and_committed_phase_keep_probe_state_unpublished() {
    let mut input = description(true);
    input.loops.push(LoopSpec {
        id: id(5),
        kind: LoopKind::Composite,
        length: 0,
        sync_source: Some(id(0)),
    });
    input.programs.push(ProgramDescription {
        source: id(5),
        nominal_unit_frames: 2,
        sections: vec![Section {
            entries: vec![Entry {
                target: id(1),
                delay: 0,
                duration: Some(3),
                repetitions: 1,
                mode: None,
            }],
        }],
    });
    let mut resolver = prepare(&input, 1, 4);
    submit(&mut resolver, 0, 1, 0, Timing::At, play(false));
    let parent = submit(&mut resolver, 5, 2, 0, Timing::At, play(false));
    let original = resolver.state(id(1)).unwrap();
    assert!(
        assert_no_heap(|| resolver.probe_program_boundary(id(1), &[]))
            .unwrap()
            .enters_zero()
    );
    let refusal = Observation::Refused {
        request: parent,
        reason: Error::Capacity,
    };
    let refused = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[refusal])).unwrap();
    assert_eq!(refused.state.mode, Mode::Stopped);
    assert_eq!(refused.state.composite, original.composite);
    assert!(!refused.enters_zero());
    assert_eq!(resolver.state(id(1)).unwrap(), original);
    assert_eq!(resolver.completion(parent).unwrap(), None);
    assert_eq!(
        assert_no_heap(|| resolver.probe_program_boundary(id(0), &[])),
        Err(Error::InvalidProgram)
    );
    assert_eq!(
        assert_no_heap(|| resolver.probe_program_boundary(LoopId::from_parts(1, 2).unwrap(), &[])),
        Err(Error::InvalidId)
    );
    assert_eq!(
        assert_no_heap(|| resolver.probe_program_boundary(id(1), &[refusal, refusal])),
        Err(Error::DuplicateId)
    );
    assert_eq!(
        assert_no_heap(|| resolver.commit()),
        Err(Error::InvalidState)
    );
    let interval = assert_no_heap(|| resolver.preview(Frame(1), &[refusal])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    let decisions = resolver.decisions().unwrap().to_vec();
    assert_eq!(
        assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])),
        Err(Error::InvalidState)
    );
    assert_eq!(resolver.decisions().unwrap(), decisions);
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
    let candidate = compile(resolver.topology().limits(), &input).unwrap();
    let retired = assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap();
    let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(probe.topology_epoch, Epoch(1));
    assert_eq!(probe.state.mode, Mode::Stopped);
    drop(retired);

    let mut resolver = prepare(&description(false), 0, 4);
    submit(&mut resolver, 1, 1, 0, Timing::At, play(false));
    advance(&mut resolver, 2);
    let start = submit(
        &mut resolver,
        0,
        2,
        2,
        Timing::At,
        Transition::Record {
            monitor_previous: false,
        },
    );
    let unavailable = Observation::CaptureUnavailable {
        target: id(0),
        reason: Error::Capacity,
    };
    let clocked = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(clocked.state.composite.unwrap().iteration, 1);
    let stalled =
        assert_no_heap(|| resolver.probe_program_boundary(id(1), &[unavailable])).unwrap();
    assert_eq!(stalled.state.composite.unwrap().iteration, 0);
    assert_eq!(stalled.state.composite.unwrap().entered_at, Some(Frame(0)));
    assert_eq!(stalled.state.last_pulse, Some(Frame(0)));
    assert!(!stalled.enters_zero());
    assert_eq!(resolver.completion(start).unwrap(), None);
    let interval = assert_no_heap(|| resolver.preview(Frame(3), &[unavailable])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_eq!(resolver.state(id(1)).unwrap(), stalled.state);
    assert_eq!(
        resolver.completion(start).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(2),
            reason: Error::Capacity
        }
    );
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
}
