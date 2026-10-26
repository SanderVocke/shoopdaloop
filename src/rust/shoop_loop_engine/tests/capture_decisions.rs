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
fn play() -> Transition {
    Transition::Play {
        position: 0,
        retrigger: false,
    }
}
fn request(target: u32, sequence: u64, at: u64, transition: Transition) -> Request {
    Request {
        target: id(target),
        sequence: Sequence(sequence),
        at: Frame(at),
        timing: Timing::At,
        transition,
    }
}
fn decision(resolver: &Resolver, target: u32) -> Decision {
    *resolver
        .decisions()
        .unwrap()
        .iter()
        .find(|d| d.state.id == id(target))
        .unwrap()
}
fn unavailable(target: u32) -> Observation {
    Observation::CaptureUnavailable {
        target: id(target),
        reason: Error::Capacity,
    }
}
fn run_trigger_case(period: u64, partitions: &[usize], journal: usize) -> Vec<Event> {
    let mut resolver = Resolver::prepare(
        compile(
            Limits {
                loop_slots: 4,
                pending_requests: 4,
                journal_events: journal,
                ..Limits::default()
            },
            &TopologyDescription {
                loops: vec![
                    primitive(0, period, None),
                    primitive(1, 1000, None),
                    primitive(2, 1000, None),
                    primitive(3, 1000, None),
                ],
                triggers: vec![
                    Trigger {
                        source: id(0),
                        target: id(1),
                        mode: Mode::Recording,
                    },
                    Trigger {
                        source: id(1),
                        target: id(2),
                        mode: Mode::Playing,
                    },
                ],
                programs: vec![],
            },
        )
        .unwrap(),
    )
    .unwrap();
    let unrelated = assert_no_heap(|| {
        resolver.submit(request(0, 1, 0, play())).unwrap();
        resolver.submit(request(1, 2, 0, play())).unwrap();
        resolver.submit(request(2, 3, 1, Transition::Stop)).unwrap();
        resolver.submit(request(3, 4, 2, play())).unwrap()
    });
    let mut events = Vec::new();
    let mut end = 0;
    let mut refused = 0;
    for &frames in partitions {
        end += frames as u64;
        while resolver.frame().0 < end {
            let interval = assert_no_heap(|| {
                resolver.preview(Frame(end), &[]).unwrap();
                let candidate = decision(&resolver, 1);
                let interval = if candidate.state.mode == Mode::Recording {
                    assert_eq!(candidate.origin, None);
                    refused += 1;
                    // Repeated previews must not retain a failed capture pulse or revision.
                    let first = resolver.preview(Frame(end), &[unavailable(1)]).unwrap();
                    let state = decision(&resolver, 1);
                    assert_eq!(
                        resolver.preview(Frame(end), &[unavailable(1)]).unwrap(),
                        first
                    );
                    assert_eq!(decision(&resolver, 1), state);
                    first
                } else {
                    resolver.preview(Frame(end), &[unavailable(1)]).unwrap()
                };
                let retained = decision(&resolver, 1);
                assert_eq!(retained.state.mode, Mode::Playing);
                assert_eq!(retained.state.position, interval.start.0);
                assert_eq!(retained.state.capture_revision, Revision(0));
                assert_eq!(retained.state.capture_frames, 0);
                assert_eq!(retained.playback_discontinuity, interval.start == Frame(0));
                if interval.start.0 >= 1 {
                    assert_eq!(decision(&resolver, 2).state.mode, Mode::Stopped);
                }
                resolver.commit().unwrap();
                interval
            });
            events.extend_from_slice(resolver.events());
            assert_no_heap(|| {
                resolver.finish(interval.end).unwrap();
                assert_eq!(resolver.retained_requests(), 4);
            });
        }
    }
    assert_eq!(refused, (end - 1) / period);
    assert_eq!(resolver.state(id(1)).unwrap().position, end);
    assert_eq!(resolver.state(id(0)).unwrap().cycles, (end - 1) / period);
    assert_eq!(
        resolver.completion(unrelated).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(2) }
    );
    if journal == 1024 {
        let actual: Vec<_> = events
            .iter()
            .filter(|e| e.target == id(1) && e.kind == EventKind::Refused(Error::Capacity))
            .map(|e| e.frame.0)
            .collect();
        let expected: Vec<_> = (1..end).filter(|frame| frame % period == 0).collect();
        assert_eq!(actual, expected);
        assert!(!events
            .iter()
            .any(|e| e.target == id(1) && e.frame.0 > 0 && e.kind == EventKind::Pulse));
        assert!(!events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Transition(Transition::Record { .. }))));
    }
    events
}
#[shoop_test]
fn autonomous_capture_refusal_preserves_playback_without_pulsing_descendants_or_claiming_requests()
{
    let mut baseline = None;
    for partition in [vec![8], vec![1; 8], vec![3, 5]] {
        for journal in [0, 1, 1024] {
            let events = run_trigger_case(2, &partition, journal);
            if journal == 1024 {
                if let Some(expected) = &baseline {
                    assert_eq!(&events, expected);
                } else {
                    baseline = Some(events);
                }
            }
        }
    }
}
#[shoop_test]
fn dense_autonomous_refusals_are_not_limited_by_external_receipts_or_diagnostic_journals() {
    let mut baseline = None;
    for partition in [vec![128], vec![1; 128], vec![7, 53, 19, 49]] {
        for journal in [0, 1, 1024] {
            let events = run_trigger_case(1, &partition, journal);
            if journal == 1024 {
                if let Some(expected) = &baseline {
                    assert_eq!(&events, expected);
                } else {
                    baseline = Some(events);
                }
            }
        }
    }
}
#[shoop_test]
fn capture_restart_refusal_and_running_take_exhaustion_have_distinct_effects() {
    for exhausted in [false, true] {
        let mut resolver = Resolver::prepare(
            compile(
                Limits {
                    loop_slots: 1,
                    pending_requests: 2,
                    journal_events: 0,
                    ..Limits::default()
                },
                &TopologyDescription {
                    loops: vec![primitive(0, 100, None)],
                    ..TopologyDescription::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_no_heap(|| {
            let first = resolver
                .submit(request(
                    0,
                    1,
                    0,
                    Transition::Record {
                        monitor_previous: true,
                    },
                ))
                .unwrap();
            resolver.preview(Frame(2), &[]).unwrap();
            resolver.commit().unwrap();
            resolver.finish(Frame(2)).unwrap();
            resolver.take_completion(first).unwrap().unwrap();
            let restart = resolver
                .submit(request(
                    0,
                    2,
                    2,
                    Transition::Record {
                        monitor_previous: false,
                    },
                ))
                .unwrap();
            resolver.preview(Frame(3), &[]).unwrap();
            assert_eq!(decision(&resolver, 0).state.capture_revision, Revision(2));
            let observations = [
                unavailable(0),
                Observation::CaptureExhausted { target: id(0) },
            ];
            resolver
                .preview(Frame(3), &observations[..if exhausted { 2 } else { 1 }])
                .unwrap();
            let value = decision(&resolver, 0);
            assert_eq!(
                value.state.mode,
                if exhausted {
                    Mode::Playing
                } else {
                    Mode::Recording
                }
            );
            assert_eq!(value.state.capture_revision, Revision(1));
            assert_eq!(value.state.capture_frames, 2);
            assert_eq!(value.state.capture_incomplete, exhausted);
            assert_eq!(value.state.last_pulse, Some(Frame(0)));
            assert!(!value.playback_discontinuity);
            resolver.commit().unwrap();
            assert_eq!(
                resolver.completion(restart).unwrap().unwrap().outcome,
                Outcome::Failed {
                    frame: Frame(2),
                    reason: Error::Capacity
                }
            );
            resolver.finish(Frame(3)).unwrap();
            resolver.take_completion(restart).unwrap().unwrap();
            let stop = resolver.submit(request(0, 3, 3, Transition::Stop)).unwrap();
            resolver.preview(Frame(4), &[unavailable(0)]).unwrap();
            resolver.commit().unwrap();
            assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Stopped);
            assert_eq!(
                resolver.completion(stop).unwrap().unwrap().outcome,
                Outcome::Applied { frame: Frame(3) }
            );
            resolver.finish(Frame(4)).unwrap();
            resolver.take_completion(stop).unwrap().unwrap();
            resolver
                .submit(request(
                    0,
                    4,
                    4,
                    Transition::Record {
                        monitor_previous: false,
                    },
                ))
                .unwrap();
            resolver.preview(Frame(5), &[]).unwrap();
            assert_eq!(decision(&resolver, 0).state.capture_revision, Revision(2));
        });
    }
    for reversed in [false, true] {
        let mut resolver = Resolver::prepare(
            compile(
                Limits {
                    loop_slots: 1,
                    pending_requests: 1,
                    journal_events: 0,
                    ..Limits::default()
                },
                &TopologyDescription {
                    loops: vec![primitive(0, 8, None)],
                    ..TopologyDescription::default()
                },
            )
            .unwrap(),
        )
        .unwrap();
        assert_no_heap(|| {
            let token = resolver
                .submit(request(
                    0,
                    1,
                    0,
                    Transition::Record {
                        monitor_previous: false,
                    },
                ))
                .unwrap();
            let mut observations = [
                unavailable(0),
                Observation::Refused {
                    request: token,
                    reason: Error::Exhausted,
                },
            ];
            if reversed {
                observations.reverse();
            }
            resolver.preview(Frame(1), &observations).unwrap();
            resolver.commit().unwrap();
            assert_eq!(
                resolver.completion(token).unwrap().unwrap().outcome,
                Outcome::Failed {
                    frame: Frame(0),
                    reason: Error::Exhausted
                }
            );
            assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Stopped);
        });
    }
    // Both independently latched failure classes fit even with one external slot.
    let mut resolver = Resolver::prepare(
        compile(
            Limits {
                loop_slots: 2,
                pending_requests: 1,
                ..Limits::default()
            },
            &TopologyDescription {
                loops: vec![primitive(0, 8, None), primitive(1, 8, None)],
                ..TopologyDescription::default()
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_no_heap(|| {
        for target in 0..2 {
            let token = resolver
                .submit(request(
                    target,
                    u64::from(target) + 1,
                    u64::from(target),
                    Transition::Record {
                        monitor_previous: false,
                    },
                ))
                .unwrap();
            let end = Frame(u64::from(target) + 1);
            resolver.preview(end, &[]).unwrap();
            resolver.commit().unwrap();
            resolver.finish(end).unwrap();
            resolver.take_completion(token).unwrap().unwrap();
        }
        resolver
            .preview(
                Frame(3),
                &[
                    unavailable(0),
                    Observation::CaptureExhausted { target: id(0) },
                    unavailable(1),
                    Observation::CaptureExhausted { target: id(1) },
                ],
            )
            .unwrap();
        for target in 0..2 {
            assert_eq!(decision(&resolver, target).state.mode, Mode::Stopped);
            assert!(decision(&resolver, target).state.capture_incomplete);
        }
    });
}
#[shoop_test]
fn request_family_refusal_remains_separate_from_targeted_capture_admission() {
    let mut resolver = Resolver::prepare(
        compile(
            Limits {
                loop_slots: 4,
                ..Limits::default()
            },
            &TopologyDescription {
                loops: vec![
                    primitive(0, 2, None),
                    primitive(1, 4, Some(0)),
                    LoopSpec {
                        id: id(2),
                        kind: LoopKind::Composite,
                        length: 0,
                        sync_source: Some(id(0)),
                    },
                    primitive(3, 4, Some(0)),
                ],
                triggers: vec![],
                programs: vec![ProgramDescription {
                    source: id(2),
                    nominal_unit_frames: 2,
                    sections: vec![Section {
                        entries: vec![
                            Entry {
                                target: id(1),
                                delay: 0,
                                duration: Some(2),
                                repetitions: 1,
                                mode: Some(Mode::Recording),
                            },
                            Entry {
                                target: id(3),
                                delay: 0,
                                duration: Some(2),
                                repetitions: 1,
                                mode: Some(Mode::Playing),
                            },
                        ],
                    }],
                }],
            },
        )
        .unwrap(),
    )
    .unwrap();
    assert_no_heap(|| {
        assert_eq!(
            resolver.preview(Frame(1), &[unavailable(2)]),
            Err(Error::InvalidState)
        );
        assert_eq!(
            resolver.preview(
                Frame(1),
                &[Observation::CaptureUnavailable {
                    target: LoopId::from_parts(1, 2).unwrap(),
                    reason: Error::Capacity
                }]
            ),
            Err(Error::InvalidId)
        );
        assert_eq!(
            resolver.preview(Frame(1), &[unavailable(1), unavailable(1)]),
            Err(Error::DuplicateId)
        );
        resolver.submit(request(0, 1, 0, play())).unwrap();
        let parent = resolver.submit(request(2, 2, 0, play())).unwrap();
        resolver.preview(Frame(1), &[]).unwrap();
        assert_eq!(decision(&resolver, 1).state.mode, Mode::Recording);
        assert_eq!(decision(&resolver, 1).origin, Some(parent));
        resolver
            .preview(
                Frame(1),
                &[Observation::Refused {
                    request: parent,
                    reason: Error::Capacity,
                }],
            )
            .unwrap();
        resolver.commit().unwrap();
        for target in [1, 2, 3] {
            assert_eq!(resolver.state(id(target)).unwrap().mode, Mode::Stopped);
        }
        assert_eq!(
            resolver.completion(parent).unwrap().unwrap().outcome,
            Outcome::Failed {
                frame: Frame(0),
                reason: Error::Capacity
            }
        );
        resolver.finish(Frame(1)).unwrap();
        let next = resolver.submit(request(2, 3, 1, play())).unwrap();
        resolver.preview(Frame(2), &[unavailable(1)]).unwrap();
        resolver.commit().unwrap();
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Stopped);
        for target in [2, 3] {
            assert_eq!(resolver.state(id(target)).unwrap().mode, Mode::Playing);
        }
        assert_eq!(
            resolver.completion(next).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(1) }
        );
        assert!(resolver
            .events()
            .iter()
            .any(|e| e.target == id(1) && e.kind == EventKind::Refused(Error::Capacity)));
        resolver.finish(Frame(2)).unwrap();
    });
}
