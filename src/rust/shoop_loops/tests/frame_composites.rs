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

fn play() -> Transition {
    Transition::Play {
        position: 0,
        retrigger: false,
    }
}

fn request(resolver: &mut Resolver, slot: u32, sequence: u64, frame: u64, action: Transition) {
    assert_no_heap(|| {
        resolver.submit(Request {
            target: id(slot),
            sequence: Sequence(sequence),
            at: Frame(frame),
            timing: Timing::At,
            transition: action,
        })
    })
    .unwrap();
}

fn description(q: u64, source_length: u64) -> TopologyDescription {
    TopologyDescription {
        loops: (0..5)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 1 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: match slot {
                    0 => source_length * q,
                    1 => 0,
                    4 => 6 * q,
                    _ => 12 * q,
                },
                sync_source: matches!(slot, 1 | 4).then_some(id(0)),
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(1),
            nominal_unit_frames: 3 * q,
            sections: vec![Section {
                entries: (0..2)
                    .map(|index| Entry {
                        target: id(2 + index),
                        delay: u64::from(index),
                        duration: Some(1),
                        repetitions: 1,
                        mode: None,
                    })
                    .collect(),
            }],
        }],
        triggers: vec![],
    }
}

fn arrangement(q: u64, source_length: u64) -> Resolver {
    Resolver::prepare(
        compile(Limits::default(), &description(q, source_length)).unwrap(),
        rt_runtime::Quantum::new(q as usize).unwrap(),
    )
    .unwrap()
}

fn tick(resolver: &mut Resolver) -> (Vec<State>, Vec<Event>) {
    assert_no_heap(|| resolver.preview_tick(&[])).unwrap();
    assert_no_heap(|| resolver.commit()).unwrap();
    let states = resolver
        .decisions()
        .unwrap()
        .iter()
        .map(|d| d.state)
        .collect();
    let events = resolver.events().to_vec();
    assert_no_heap(|| resolver.finish()).unwrap();
    (states, events)
}

#[shoop_test]
fn composite_seek_restores_the_authored_playback_role_after_an_independent_child_override() {
    for q in [1, 16, 32, 64, 128] {
        for authored in [Mode::Playing, Mode::PlayingDryThroughWet] {
            let mut input = description(q, 7);
            for entry in &mut input.programs[0].sections[0].entries {
                entry.mode = Some(authored);
            }
            let mut resolver = Resolver::prepare(
                compile(Limits::default(), &input).unwrap(),
                rt_runtime::Quantum::new(q as usize).unwrap(),
            )
            .unwrap();
            request(&mut resolver, 1, 1, 0, play());
            let override_mode = if authored == Mode::Playing {
                Transition::PlayDry {
                    position: 0,
                    retrigger: false,
                }
            } else {
                play()
            };
            request(&mut resolver, 2, 2, q, override_mode);
            request(
                &mut resolver,
                1,
                3,
                2 * q,
                Transition::SeekIteration {
                    iteration: 0,
                    retrigger: false,
                },
            );
            tick(&mut resolver);
            let (overridden, _) = tick(&mut resolver);
            assert_ne!(overridden[2].mode, authored);
            let (restored, _) = tick(&mut resolver);
            assert_eq!(restored[2].mode, authored);
            assert_eq!(restored[2].position, 0);
            assert_eq!(restored[1].position, 0);
        }
    }
}

#[shoop_test]
fn source_stop_resize_and_restart_preserve_every_prepared_frame() {
    for q in [1, 16, 32, 64, 128] {
        let mut resolver = arrangement(q, 7);
        request(&mut resolver, 0, 1, 0, play());
        request(&mut resolver, 1, 2, 0, play());
        request(&mut resolver, 0, 3, 2 * q, Transition::Stop);
        request(
            &mut resolver,
            0,
            4,
            3 * q,
            Transition::Replace {
                length: 5 * q,
                revision: Revision(1),
                preserve_position: false,
            },
        );
        request(&mut resolver, 0, 5, 4 * q, play());
        let mut pulses = vec![];
        for step in 0..19 {
            let (states, events) = tick(&mut resolver);
            let parent = states[1];
            let phase = step % 6;
            assert_eq!(parent.position, phase * q);
            assert_eq!(parent.composite.unwrap().passes, step / 6);
            assert_eq!(parent.composite.unwrap().iteration, phase / 3);
            assert_eq!(
                states[2].mode,
                if phase < 3 {
                    Mode::Playing
                } else {
                    Mode::Stopped
                }
            );
            assert_eq!(
                states[3].mode,
                if phase >= 3 {
                    Mode::Playing
                } else {
                    Mode::Stopped
                }
            );
            pulses.extend(
                events
                    .iter()
                    .filter(|e| e.target == id(1) && e.kind == EventKind::Pulse)
                    .map(|e| e.frame),
            );
        }
        assert_eq!(
            pulses,
            [Frame(0), Frame(6 * q), Frame(12 * q), Frame(18 * q)]
        );
    }
}

#[shoop_test]
fn primitives_and_composites_wait_only_at_their_own_end_and_resume_on_source_edges() {
    for q in [1, 16, 32, 64, 128] {
        let mut resolver = arrangement(q, 10);
        for slot in [1, 4] {
            assert_no_heap(|| resolver.set_repeat_sync(id(slot), true)).unwrap();
        }
        for slot in [0, 1, 4] {
            request(&mut resolver, slot, u64::from(slot) + 1, 0, play());
        }
        for step in 0..11 {
            let before = resolver.state(id(1)).unwrap();
            let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
            assert_eq!(resolver.state(id(1)).unwrap(), before);
            assert_eq!(probe.enters_zero(), step == 0 || step == 10);
            let (states, _) = tick(&mut resolver);
            let parent = states[1];
            let follower = states[4];
            if (6..10).contains(&step) {
                assert_eq!(parent.mode, Mode::Playing);
                assert_eq!(parent.position, 6 * q);
                assert_eq!(parent.composite.unwrap().passes, 1);
                assert!(parent.composite.unwrap().waiting_repeat);
                assert_eq!(follower.position, 6 * q);
                assert!(!follower.plays());
                assert_eq!(
                    (states[2].mode, states[3].mode),
                    (Mode::Stopped, Mode::Stopped)
                );
            } else {
                assert_eq!(parent.position, (step % 10) * q);
                assert!(!parent.composite.unwrap().waiting_repeat);
                assert_eq!(follower.position, (step % 10) * q);
            }
        }
    }
}

#[shoop_test]
fn stopping_a_source_releases_an_existing_wait_without_an_extra_pass_or_tick() {
    for q in [1, 32] {
        let mut resolver = arrangement(q, 10);
        for slot in [1, 4] {
            resolver.set_repeat_sync(id(slot), true).unwrap();
        }
        for slot in [0, 1, 4] {
            request(&mut resolver, slot, u64::from(slot) + 1, 0, play());
        }
        request(&mut resolver, 0, 6, 8 * q, Transition::Stop);
        for step in 0..15 {
            let (states, events) = tick(&mut resolver);
            if step == 7 {
                assert!(states[1].composite.unwrap().waiting_repeat);
            }
            if step == 8 {
                assert_eq!(states[1].position, 0);
                assert_eq!(states[1].composite.unwrap().passes, 1);
                assert_eq!(states[4].position, 0);
                assert_eq!(states[2].mode, Mode::Playing);
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| e.target == id(1) && e.kind == EventKind::Pulse)
                        .count(),
                    1
                );
            }
            if step == 14 {
                assert_eq!(states[1].position, 0);
                assert_eq!(states[1].composite.unwrap().passes, 2);
            }
        }
    }
}

#[shoop_test]
fn delayed_commands_still_follow_source_edges_without_owning_program_progression() {
    let mut resolver = arrangement(1, 7);
    request(&mut resolver, 0, 1, 0, play());
    request(&mut resolver, 1, 2, 0, play());
    resolver
        .submit(Request {
            target: id(1),
            sequence: Sequence(3),
            at: Frame(0),
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 1,
            },
            transition: Transition::Stop,
        })
        .unwrap();
    request(&mut resolver, 0, 4, 2, Transition::Stop);
    request(&mut resolver, 0, 5, 10, play());
    for step in 0..11 {
        let (states, _) = tick(&mut resolver);
        if step < 10 {
            assert_eq!(states[1].mode, Mode::Playing);
            assert_eq!(states[1].position, step % 6);
        } else {
            assert_eq!(states[1].mode, Mode::Stopped);
            assert_eq!(states[1].position, 0);
            assert_eq!(
                (states[2].mode, states[3].mode),
                (Mode::Stopped, Mode::Stopped)
            );
        }
    }
}

#[shoop_test]
fn explicit_recording_sections_finish_on_prepared_frames_after_source_loss() {
    for q in [1, 32] {
        let mut input = description(q, 7);
        for entry in &mut input.programs[0].sections[0].entries {
            entry.mode = Some(Mode::Recording);
        }
        let mut resolver = Resolver::prepare(
            compile(Limits::default(), &input).unwrap(),
            rt_runtime::Quantum::new(q as usize).unwrap(),
        )
        .unwrap();
        request(&mut resolver, 0, 1, 0, play());
        request(&mut resolver, 1, 2, 0, play());
        request(&mut resolver, 0, 3, q, Transition::Stop);
        for step in 0..8 {
            let (states, _) = tick(&mut resolver);
            assert_eq!(states[2].capture_frames, step.min(3) * q);
            assert_eq!(states[3].capture_frames, step.saturating_sub(3).min(3) * q);
            if step >= 6 {
                assert_eq!(states[1].mode, Mode::Stopped);
                assert_eq!(states[1].composite.unwrap().passes, 1);
                assert_eq!(
                    (states[2].content_length, states[3].content_length),
                    (3 * q, 3 * q)
                );
            }
        }
    }
}

#[shoop_test]
fn future_only_edits_adopt_before_wrap_but_the_next_unit_is_protected() {
    for q in [1, 32] {
        for safe in [false, true] {
            let mut old = description(q, 7);
            old.programs[0].sections[0].entries.truncate(1);
            old.programs[0].sections[0].entries[0].delay = 4;
            let mut new = old.clone();
            new.programs[0].sections[0].entries[0].delay = if safe { 3 } else { 1 };
            let mut resolver = Resolver::prepare(
                compile(Limits::default(), &old).unwrap(),
                rt_runtime::Quantum::new(q as usize).unwrap(),
            )
            .unwrap();
            request(&mut resolver, 1, 1, 0, play());
            tick(&mut resolver);
            let candidate = compile(Limits::default(), &new).unwrap();
            let change = assert_no_heap(|| {
                resolver.stage_program(Epoch(0), id(1), Sequence(2), Frame(q), candidate)
            })
            .unwrap();
            let applied = if safe { q } else { 15 * q };
            for step in 1..20 {
                let at = step * q;
                let (states, _) = tick(&mut resolver);
                assert_eq!(resolver.epoch(), Epoch(u64::from(at >= applied)));
                if at >= applied {
                    assert_eq!(
                        resolver.completion(change).unwrap().unwrap().outcome,
                        Outcome::Applied {
                            frame: Frame(applied)
                        }
                    );
                }
                if safe && step == 9 {
                    assert_eq!(states[2].mode, Mode::Playing);
                    assert_eq!(states[2].position, 0);
                }
                if !safe && step == 3 {
                    assert_eq!(states[2].mode, Mode::Stopped);
                }
            }
            let owner = assert_no_heap(|| resolver.take_program_change(change))
                .unwrap()
                .unwrap();
            assert_eq!(owner.activated_epoch, Some(Epoch(1)));
        }
    }
}

#[shoop_test]
fn editing_during_an_alignment_wait_emits_only_the_new_snapshot_at_actual_restart() {
    for q in [1, 32] {
        let mut resolver = arrangement(q, 10);
        resolver.set_repeat_sync(id(1), true).unwrap();
        request(&mut resolver, 0, 1, 0, play());
        request(&mut resolver, 1, 2, 0, play());
        for _ in 0..7 {
            tick(&mut resolver);
        }
        assert!(
            resolver
                .state(id(1))
                .unwrap()
                .composite
                .unwrap()
                .waiting_repeat
        );
        let mut new = description(q, 10);
        for entry in &mut new.programs[0].sections[0].entries {
            entry.delay = 1 - entry.delay;
        }
        let candidate = compile(Limits::default(), &new).unwrap();
        let change = assert_no_heap(|| {
            resolver.stage_program(Epoch(0), id(1), Sequence(3), Frame(7 * q), candidate)
        })
        .unwrap();
        for step in 7..11 {
            let before = resolver.state(id(1)).unwrap();
            for _ in 0..3 {
                let probe = assert_no_heap(|| resolver.probe_program_boundary(id(1), &[])).unwrap();
                assert_eq!(probe.enters_zero(), step == 10);
                assert_eq!(resolver.state(id(1)).unwrap(), before);
            }
            let (states, events) = tick(&mut resolver);
            if step < 10 {
                assert_eq!(resolver.epoch(), Epoch(0));
                assert!(events.is_empty());
            } else {
                assert_eq!(resolver.epoch(), Epoch(1));
                assert_eq!(states[1].composite.unwrap().passes, 1);
                assert_eq!(
                    (states[2].mode, states[3].mode),
                    (Mode::Stopped, Mode::Playing)
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| e.target == id(3)
                            && matches!(e.kind, EventKind::Transition(Transition::Play { .. })))
                        .count(),
                    1
                );
            }
        }
        let owner = assert_no_heap(|| resolver.take_program_change(change))
            .unwrap()
            .unwrap();
        assert_eq!(
            owner.completion.outcome,
            Outcome::Applied {
                frame: Frame(10 * q)
            }
        );
    }
}

#[shoop_test]
fn refused_batch_during_composite_playback_keeps_cursors_children_pulses_and_pending_clocks() {
    let make = || {
        Resolver::prepare(
            compile(Limits::default(), &description(32, 2)).unwrap(),
            rt_runtime::Quantum::new(32).unwrap(),
        )
        .unwrap()
    };
    let mut expected = make();
    let mut actual = make();
    let mut plans = [None; 2];
    for (index, resolver) in [&mut expected, &mut actual].into_iter().enumerate() {
        request(resolver, 0, 1, 0, play());
        request(resolver, 1, 2, 0, play());
        plans[index] = Some(
            resolver
                .submit(Request {
                    target: id(4),
                    sequence: Sequence(3),
                    at: Frame(0),
                    timing: Timing::AfterTicks {
                        source: id(0),
                        skip: 6,
                    },
                    transition: Transition::Record {
                        monitor_previous: false,
                    },
                })
                .unwrap(),
        );
    }
    assert_no_heap(|| {
        for tick in 0..12 {
            if tick == 4 {
                let mut receipts = [None; 2];
                actual
                    .submit_batch(
                        &[
                            Request {
                                target: id(0),
                                sequence: Sequence(4),
                                at: actual.frame(),
                                timing: Timing::At,
                                transition: Transition::Stop,
                            },
                            Request {
                                target: id(4),
                                sequence: Sequence(4),
                                at: actual.frame(),
                                timing: Timing::At,
                                transition: Transition::Play {
                                    position: 32 * 100,
                                    retrigger: true,
                                },
                            },
                        ],
                        &mut receipts,
                    )
                    .unwrap();
            }
            expected.preview_tick(&[]).unwrap();
            actual.preview_tick(&[]).unwrap();
            assert_eq!(actual.decisions().unwrap(), expected.decisions().unwrap());
            expected.commit().unwrap();
            actual.commit().unwrap();
            for slot in 0..5 {
                assert_eq!(actual.state(id(slot)), expected.state(id(slot)));
            }
            let delay = |resolver: &Resolver| {
                resolver
                    .pending_transitions()
                    .find(|pending| pending.request.sequence == Sequence(3))
                    .map(|pending| pending.remaining_ticks)
            };
            assert_eq!(delay(&actual), delay(&expected));
            expected.finish().unwrap();
            actual.finish().unwrap();
        }
        assert_eq!(
            actual.completion(plans[1].unwrap()),
            expected.completion(plans[0].unwrap())
        );
    });
}
