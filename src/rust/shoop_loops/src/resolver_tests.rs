use super::*;
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[shoop_test]
fn program_probe_stops_before_old_child_actions_and_never_publishes_shadow_work() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let description = TopologyDescription {
        loops: (0..4)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 1 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: if slot == 0 { 1 } else { 8 },
                sync_source: (slot == 1).then(|| id(0)),
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(1),
            nominal_unit_frames: 1,
            sections: vec![Section {
                entries: vec![
                    Entry {
                        target: id(2),
                        delay: 0,
                        duration: Some(1),
                        repetitions: 1,
                        mode: Some(Mode::Recording),
                    },
                    Entry {
                        target: id(3),
                        delay: 1,
                        duration: Some(1),
                        repetitions: 1,
                        mode: Some(Mode::Playing),
                    },
                ],
            }],
        }],
        triggers: vec![Trigger {
            source: id(1),
            target: id(3),
            mode: Mode::Playing,
        }],
    };
    let mut resolver = Resolver::prepare(
        compile(Limits::default(), &description).unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
    )
    .unwrap();
    for slot in 0..2 {
        assert_no_heap(|| {
            resolver.submit(Request {
                target: id(slot),
                sequence: Sequence(slot as u64 + 1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
        })
        .unwrap();
    }
    for _ in 0..8 {
        let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
        assert!(probe.enters_zero());
        assert_eq!(probe.state.mode, Mode::Playing);
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Stopped);
        assert_eq!(resolver.scratch.states[2].unwrap().mode, Mode::Stopped);
        assert!(resolver.scratch.intents[2].is_none());
        assert!(resolver.scratch.intents[3].is_none());
        assert!(resolver
            .scratch
            .records
            .iter()
            .flatten()
            .all(|r| r.terminal.is_some()));
        assert!(resolver
            .records
            .iter()
            .flatten()
            .all(|r| r.terminal.is_none()));
        assert!(resolver.events().is_empty());
        assert_eq!(resolver.preview_program_change(), Err(Error::InvalidState));
        assert_eq!(
            assert_no_heap(|| resolver.commit()),
            Err(Error::InvalidState)
        );
    }
    assert_no_heap(|| resolver.preview_tick(&[])).unwrap();
    assert_eq!(resolver.preview_program_change(), Ok(None));
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_eq!(resolver.preview_program_change(), Err(Error::InvalidState));
    assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Recording);
    assert_no_heap(|| resolver.finish()).unwrap();
    let before = resolver.state(id(2)).unwrap();
    let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(probe.state.composite.unwrap().iteration, 1);
    assert!(!probe.enters_zero());
    assert_eq!(resolver.scratch.states[2], Some(before));
    assert!(resolver.scratch.intents[2].is_none());
    assert!(resolver.scratch.intents[3].is_none());
    assert_no_heap(|| resolver.preview_tick(&[])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Stopped);
    assert_eq!(resolver.state(id(3)).unwrap().mode, Mode::Playing);
    assert_no_heap(|| resolver.finish()).unwrap();
    let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
    assert_eq!(probe.state.mode, Mode::Stopped);
    assert_eq!(probe.state.composite.unwrap().passes, 1);
    assert_eq!(resolver.scratch.states[3].unwrap().mode, Mode::Playing);
    assert!(resolver.scratch.intents[3].is_none());
    assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Playing);
}

#[shoop_test]
fn replacing_preserves_playing_phase_wraps_sync_and_commits_one_content_revision_at_exit() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let description = TopologyDescription {
        loops: (0..2)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: LoopKind::Primitive,
                length: 64,
                sync_source: (slot == 1).then(|| id(0)),
            })
            .collect(),
        ..TopologyDescription::default()
    };
    let mut resolver = Resolver::prepare(
        compile(Limits::default(), &description).unwrap(),
        rt_runtime::Quantum::new(32).unwrap(),
    )
    .unwrap();
    assert_no_heap(|| {
        resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 32,
                    retrigger: false,
                },
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        resolver.finish().unwrap();
        let entry = resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(2),
                at: Frame(32),
                timing: Timing::At,
                transition: Transition::ReplaceInput { position: None },
            })
            .unwrap();
        let follower = resolver
            .submit(Request {
                target: id(1),
                sequence: Sequence(3),
                at: Frame(32),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 0,
                },
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        let state = resolver.state(id(0)).unwrap();
        assert_eq!(
            (state.mode, state.position, state.cycles),
            (Mode::Replacing, 0, 1)
        );
        assert!(!state.plays());
        assert!(state.advances());
        assert!(state.writes());
        resolver.finish().unwrap();
        assert_eq!(
            resolver.take_completion(entry).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(32) }
        );
        assert_eq!(
            resolver.take_completion(follower).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(32) }
        );
        let revision = state.capture_revision;
        resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(4),
                at: Frame(64),
                timing: Timing::At,
                transition: Transition::ReplaceInput { position: None },
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        assert_eq!(resolver.state(id(0)).unwrap().capture_revision, revision);
        resolver.finish().unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        resolver.finish().unwrap();
        resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(5),
                at: Frame(128),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        let playing = resolver.state(id(0)).unwrap();
        assert_eq!(
            (
                playing.mode,
                playing.position,
                playing.cycles,
                playing.content_revision
            ),
            (Mode::Playing, 32, 2, revision)
        );
        resolver.finish().unwrap();
        resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(6),
                at: Frame(160),
                timing: Timing::At,
                transition: Transition::Stop,
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        let state = resolver.state(id(0)).unwrap();
        assert_eq!(
            (
                state.mode,
                state.position,
                state.content_length,
                state.content_revision
            ),
            (Mode::Stopped, 0, 64, revision)
        );
        assert!(!state.capture_incomplete);
    });
}

#[shoop_test]
fn batches_settle_all_targets_or_refuse_all_with_no_state_or_pulse_leak() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let description = TopologyDescription {
        loops: (0..3)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: LoopKind::Primitive,
                length: 64,
                sync_source: None,
            })
            .collect(),
        programs: vec![],
        triggers: vec![Trigger {
            source: id(0),
            target: id(2),
            mode: Mode::Playing,
        }],
    };
    let mut resolver = Resolver::prepare(
        compile(Limits::default(), &description).unwrap(),
        rt_runtime::Quantum::new(32).unwrap(),
    )
    .unwrap();
    let request = |slot, sequence, at, transition| Request {
        target: id(slot),
        sequence: Sequence(sequence),
        at: Frame(at),
        timing: Timing::At,
        transition,
    };
    let play = Transition::Play {
        position: 0,
        retrigger: true,
    };
    assert_no_heap(|| {
        let mut receipts = [None; 2];
        assert_eq!(
            resolver.submit_batch(
                &[request(0, 1, 0, play), request(99, 1, 0, play),],
                &mut receipts
            ),
            Err(Error::InvalidId)
        );
        assert_eq!(receipts, [None; 2]);
        assert_eq!(resolver.retained_requests(), 0);
        for scenario in 0..3 {
            let frame = scenario * 32;
            resolver
                .submit_batch(
                    &[
                        request(0, 2 + scenario * 2, frame, play),
                        request(
                            1,
                            2 + scenario * 2,
                            frame,
                            if scenario == 0 {
                                Transition::Play {
                                    position: 128,
                                    retrigger: true,
                                }
                            } else {
                                play
                            },
                        ),
                    ],
                    &mut receipts,
                )
                .unwrap();
            let independent = if scenario == 2 {
                Some(
                    resolver
                        .submit(request(1, 3 + scenario * 2, frame, Transition::Stop))
                        .unwrap(),
                )
            } else {
                None
            };
            let refused = [Observation::Refused {
                request: receipts[0].unwrap(),
                reason: Error::Capacity,
            }];
            resolver
                .preview_tick(if scenario == 1 { &refused } else { &[] })
                .unwrap();
            resolver.commit().unwrap();
            for slot in 0..3 {
                assert_eq!(resolver.state(id(slot)).unwrap().mode, Mode::Stopped);
            }
            assert!(!resolver
                .events()
                .iter()
                .any(|event| event.kind == EventKind::Pulse));
            resolver.finish().unwrap();
            for receipt in receipts.into_iter().flatten() {
                assert!(matches!(
                    resolver.take_completion(receipt).unwrap().unwrap().outcome,
                    Outcome::Failed { .. }
                ));
            }
            if let Some(receipt) = independent {
                assert!(matches!(
                    resolver.take_completion(receipt).unwrap().unwrap().outcome,
                    Outcome::Applied { .. }
                ));
            }
        }
        resolver
            .submit_batch(
                &[
                    request(0, 9, 96, play),
                    request(
                        1,
                        9,
                        96,
                        Transition::Record {
                            monitor_previous: false,
                        },
                    ),
                ],
                &mut receipts,
            )
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Playing);
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Recording);
        assert_eq!(resolver.state(id(2)).unwrap().mode, Mode::Playing);
        resolver.finish().unwrap();
        for receipt in receipts.into_iter().flatten() {
            assert_eq!(
                resolver.take_completion(receipt).unwrap().unwrap().outcome,
                Outcome::Applied { frame: Frame(96) }
            );
        }
        resolver
            .submit_batch(
                &[
                    request(0, 10, 256, Transition::Stop),
                    request(1, 10, 256, Transition::Stop),
                ],
                &mut receipts,
            )
            .unwrap();
        resolver.cancel(receipts[1].unwrap()).unwrap();
        for receipt in receipts.into_iter().flatten() {
            assert_eq!(
                resolver.take_completion(receipt).unwrap().unwrap().outcome,
                Outcome::Cancelled { frame: Frame(128) }
            );
        }
        resolver
            .submit_batch(
                &[
                    request(0, 11, 256, Transition::Stop),
                    request(1, 11, 256, Transition::Stop),
                ],
                &mut receipts,
            )
            .unwrap();
        resolver.cancel(receipts[0].unwrap()).unwrap();
        let old = receipts[1].unwrap();
        resolver
            .take_completion(receipts[0].unwrap())
            .unwrap()
            .unwrap();
        resolver
            .submit_batch(
                &[request(0, 12, 128, play), request(1, 12, 128, play)],
                &mut receipts,
            )
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Playing);
        assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Playing);
        resolver.finish().unwrap();
        assert_eq!(
            resolver.take_completion(old).unwrap().unwrap().outcome,
            Outcome::Cancelled { frame: Frame(128) }
        );
        for receipt in receipts.into_iter().flatten() {
            assert_eq!(
                resolver.take_completion(receipt).unwrap().unwrap().outcome,
                Outcome::Applied { frame: Frame(128) }
            );
        }
        assert_eq!(resolver.retained_requests(), 0);
    });
}

#[shoop_test]
fn delayed_batches_apply_or_refuse_every_member_at_the_same_sync_cut() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let mut resolver = Resolver::prepare(
        compile(
            Limits::default(),
            &TopologyDescription {
                loops: (0..3)
                    .map(|slot| LoopSpec {
                        id: id(slot),
                        kind: LoopKind::Primitive,
                        length: 64,
                        sync_source: (slot > 0).then(|| id(0)),
                    })
                    .collect(),
                ..Default::default()
            },
        )
        .unwrap(),
        rt_runtime::Quantum::new(32).unwrap(),
    )
    .unwrap();
    assert_no_heap(|| {
        let clock = resolver
            .submit(Request {
                target: id(0),
                sequence: Sequence(1),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        resolver.finish().unwrap();
        resolver.take_completion(clock).unwrap().unwrap();
        let mut receipts = [None; 2];
        for (sequence, skip, effective, refuse) in [(2, 1, 128, true), (3, 0, 192, false)] {
            let at = resolver.frame();
            let requests = [1, 2].map(|slot| Request {
                target: id(slot),
                sequence: Sequence(sequence),
                at,
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip,
                },
                transition: Transition::Record {
                    monitor_previous: false,
                },
            });
            resolver.submit_batch(&requests, &mut receipts).unwrap();
            while resolver.frame() < Frame(effective) {
                resolver.preview_tick(&[]).unwrap();
                resolver.commit().unwrap();
                for slot in 1..3 {
                    assert_eq!(resolver.state(id(slot)).unwrap().mode, Mode::Stopped);
                }
                resolver.finish().unwrap();
                for receipt in receipts.into_iter().flatten() {
                    assert_eq!(resolver.completion(receipt).unwrap(), None);
                }
            }
            let refusal = [Observation::Refused {
                request: receipts[0].unwrap(),
                reason: Error::Capacity,
            }];
            resolver
                .preview_tick(if refuse { &refusal } else { &[] })
                .unwrap();
            resolver.commit().unwrap();
            assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Playing);
            for slot in 1..3 {
                assert_eq!(
                    resolver.state(id(slot)).unwrap().mode,
                    if refuse {
                        Mode::Stopped
                    } else {
                        Mode::Recording
                    }
                );
            }
            resolver.finish().unwrap();
            for receipt in receipts.into_iter().flatten() {
                assert_eq!(
                    resolver.take_completion(receipt).unwrap().unwrap().outcome,
                    if refuse {
                        Outcome::Failed {
                            frame: Frame(effective),
                            reason: Error::Capacity,
                        }
                    } else {
                        Outcome::Applied {
                            frame: Frame(effective),
                        }
                    }
                );
            }
        }
    });
}

#[shoop_test]
fn batches_preserve_existing_plans_until_every_member_can_apply() {
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let mut resolver = Resolver::prepare(
        compile(
            Limits::default(),
            &TopologyDescription {
                loops: (0..3)
                    .map(|slot| LoopSpec {
                        id: id(slot),
                        kind: LoopKind::Primitive,
                        length: 64,
                        sync_source: (slot > 0).then(|| id(0)),
                    })
                    .collect(),
                ..Default::default()
            },
        )
        .unwrap(),
        rt_runtime::Quantum::new(32).unwrap(),
    )
    .unwrap();
    assert_no_heap(|| {
        let play = Transition::Play {
            position: 0,
            retrigger: true,
        };
        let clock = resolver
            .submit(Request {
                target: id(0),
                at: Frame(0),
                sequence: Sequence(1),
                timing: Timing::At,
                transition: play,
            })
            .unwrap();
        resolver.preview_tick(&[]).unwrap();
        resolver.commit().unwrap();
        resolver.finish().unwrap();
        resolver.take_completion(clock).unwrap().unwrap();
        let plan = resolver
            .submit(Request {
                target: id(1),
                at: Frame(32),
                sequence: Sequence(2),
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 2,
                },
                transition: Transition::Record {
                    monitor_previous: false,
                },
            })
            .unwrap();
        for (sequence, position, succeeds) in [(3, 128, false), (4, 0, true)] {
            let frame = resolver.frame();
            let mut ids = [None; 2];
            resolver
                .submit_batch(
                    &[
                        Request {
                            target: id(1),
                            at: frame,
                            sequence: Sequence(sequence),
                            timing: Timing::At,
                            transition: play,
                        },
                        Request {
                            target: id(2),
                            at: frame,
                            sequence: Sequence(sequence),
                            timing: Timing::At,
                            transition: Transition::Play {
                                position,
                                retrigger: true,
                            },
                        },
                    ],
                    &mut ids,
                )
                .unwrap();
            resolver.preview_tick(&[]).unwrap();
            resolver.commit().unwrap();
            assert_eq!(
                resolver.state(id(1)).unwrap().mode,
                if succeeds {
                    Mode::Playing
                } else {
                    Mode::Stopped
                }
            );
            resolver.finish().unwrap();
            for receipt in ids.into_iter().flatten() {
                resolver.take_completion(receipt).unwrap().unwrap();
            }
            if succeeds {
                assert_eq!(
                    resolver.take_completion(plan).unwrap().unwrap().outcome,
                    Outcome::Cancelled { frame }
                );
            } else {
                assert_eq!(resolver.completion(plan).unwrap(), None);
            }
        }
    });
}
