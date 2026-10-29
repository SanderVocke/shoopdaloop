#[path = "support/reference.rs"]
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
fn node(slot: u32, length: u64, sync: Option<u32>) -> LoopSpec {
    LoopSpec {
        id: id(slot),
        kind: LoopKind::Primitive,
        length,
        sync_source: sync.map(id),
    }
}
fn topology(nodes: Vec<LoopSpec>, triggers: Vec<Trigger>) -> TopologyDescription {
    TopologyDescription {
        loops: nodes,
        triggers,
        programs: vec![],
    }
}
fn resolver(description: &TopologyDescription, limits: Limits) -> Resolver {
    Resolver::prepare(compile(limits, description).unwrap()).unwrap()
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
fn random(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[shoop_test]
fn seeded_primitive_execution_matches_scalar_reference_across_partitions() {
    let mut seed = 0x481c_49f2_87bd_f1e3;
    for case in 0..256 {
        let length = random(&mut seed) % 8;
        let mut requests = Vec::new();
        for sequence in 1..=32 {
            let position = random(&mut seed) % 9;
            let flag = random(&mut seed) % 2 == 0;
            let transition = match random(&mut seed) % 5 {
                0 => Transition::Play {
                    position,
                    retrigger: flag,
                },
                1 => Transition::Stop,
                2 => Transition::Record {
                    monitor_previous: flag,
                },
                3 => Transition::Seek {
                    position,
                    retrigger: flag,
                },
                _ => Transition::Replace {
                    length: position,
                    revision: Revision(sequence),
                    preserve_position: flag,
                },
            };
            requests.push(request(0, sequence, random(&mut seed) % 64, transition));
        }
        let expected = reference::primitive(length, &requests, 64);
        let input = topology(vec![node(0, length, None)], vec![]);
        let mut irregular = Vec::new();
        let mut remaining = 64;
        while remaining != 0 {
            let count = (random(&mut seed) % 13 + 1).min(remaining);
            irregular.push(count);
            remaining -= count;
        }
        for (order, partitions) in [vec![64], vec![1; 64], irregular].iter().enumerate() {
            let mut resolver = resolver(&input, Limits::default());
            let mut ids = Vec::new();
            for index in 0..requests.len() {
                let index = if order == 1 {
                    requests.len() - 1 - index
                } else {
                    index
                };
                ids.push((index, submit(&mut resolver, requests[index])));
            }
            let (events, samples) = run(&mut resolver, partitions);
            assert_eq!(events, expected.events, "case {case} partition {order}");
            assert_eq!(
                samples.into_iter().map(|row| row[0]).collect::<Vec<_>>(),
                expected.samples,
                "case {case} partition {order}"
            );
            for (index, id) in ids {
                assert_eq!(
                    resolver
                        .completion(id)
                        .unwrap()
                        .map(|completion| completion.outcome),
                    expected.outcomes[index],
                    "case {case} request {index}"
                );
            }
            let state = resolver.state(id(0)).unwrap();
            assert_eq!(
                (
                    state.mode,
                    state.content_length,
                    state.position,
                    state.cycles,
                    state.capture_frames,
                    state.content_revision,
                    state.capture_revision
                ),
                expected.final_state
            );
        }
    }
}

fn play() -> Transition {
    Transition::Play {
        position: 0,
        retrigger: false,
    }
}
fn submit(resolver: &mut Resolver, request: Request) -> RequestId {
    assert_no_heap(|| resolver.submit(request)).unwrap()
}
fn step(resolver: &mut Resolver, end: u64, observations: &[Observation]) -> Interval {
    let interval = assert_no_heap(|| resolver.preview(Frame(end), observations)).unwrap();
    assert_eq!(assert_no_heap(|| resolver.commit()).unwrap(), interval);
    interval
}
fn finish(resolver: &mut Resolver, interval: Interval) {
    assert_no_heap(|| resolver.finish(interval.end)).unwrap();
}
fn run(resolver: &mut Resolver, partitions: &[u64]) -> (Vec<Event>, Vec<Vec<(Mode, u64, u64)>>) {
    let mut events = Vec::new();
    let mut samples = Vec::new();
    for &frames in partitions {
        let end = resolver.frame().0 + frames;
        while resolver.frame().0 < end {
            let interval = step(resolver, end, &[]);
            events.extend_from_slice(resolver.events());
            for relative in 0..interval.end.0 - interval.start.0 {
                samples.push(
                    resolver
                        .decisions()
                        .unwrap()
                        .iter()
                        .map(|decision| {
                            let state = decision.state;
                            (
                                state.mode,
                                if state.plays() {
                                    state.position + relative
                                } else {
                                    0
                                },
                                if state.mode == Mode::Recording {
                                    state.capture_frames + relative
                                } else {
                                    state.capture_frames
                                },
                            )
                        })
                        .collect(),
                );
            }
            finish(resolver, interval);
        }
    }
    (events, samples)
}

#[shoop_test]
fn dense_wraps_and_callback_end_are_partition_invariant() {
    let input = topology(vec![node(0, 1, None)], vec![]);
    let mut a = resolver(&input, Limits::default());
    let mut b = resolver(&input, Limits::default());
    for resolver in [&mut a, &mut b] {
        submit(resolver, request(0, 1, 0, play()));
    }
    let expected = run(&mut a, &[128]);
    assert_eq!(run(&mut b, &[0, 17, 0, 19, 92]), expected);
    assert_eq!(
        expected
            .0
            .iter()
            .filter(|event| event.kind == EventKind::Wrap)
            .count(),
        127
    );
    assert!(expected.0.iter().all(|event| event.frame < Frame(128)));
    assert_eq!(
        (
            a.state(id(0)).unwrap().position,
            a.state(id(0)).unwrap().cycles
        ),
        (1, 127)
    );
    assert_eq!(
        run(&mut a, &[1])
            .0
            .iter()
            .filter(|event| event.kind == EventKind::Wrap)
            .count(),
        1
    );
}

#[shoop_test]
fn same_frame_cascade_and_full_journals_ignore_declaration_and_host_cuts() {
    let mut input = topology(
        vec![node(0, 4, None), node(1, 7, None), node(2, 9, None)],
        vec![
            Trigger {
                source: id(0),
                target: id(1),
                mode: Mode::Playing,
            },
            Trigger {
                source: id(1),
                target: id(2),
                mode: Mode::Playing,
            },
        ],
    );
    let mut a = resolver(&input, Limits::default());
    input.loops.reverse();
    input.triggers.reverse();
    let mut b = resolver(&input, Limits::default());
    for resolver in [&mut a, &mut b] {
        submit(resolver, request(0, 1, 0, play()));
    }
    let expected = run(&mut a, &[32]);
    assert_eq!(run(&mut b, &[1; 32]), expected);
    let starts: Vec<_> = expected
        .0
        .iter()
        .filter(|event| event.kind == EventKind::Transition(play()))
        .map(|event| (event.target, event.frame))
        .collect();
    assert_eq!(
        starts,
        vec![(id(0), Frame(0)), (id(1), Frame(0)), (id(2), Frame(0))]
    );
    assert!(expected.0.contains(&Event {
        frame: Frame(4),
        target: id(0),
        kind: EventKind::Wrap
    }));
}

#[shoop_test]
fn deep_cascade_executes_beyond_old_wave_limits_under_heap_guard() {
    let count = 256;
    let input = topology(
        (0..count).map(|slot| node(slot, 1, None)).collect(),
        (1..count)
            .map(|slot| Trigger {
                source: id(slot - 1),
                target: id(slot),
                mode: Mode::Playing,
            })
            .collect(),
    );
    let limits = Limits {
        loop_slots: count as usize,
        relations: count as usize,
        ..Limits::default()
    };
    let mut resolver = resolver(&input, limits);
    submit(&mut resolver, request(0, 1, 0, play()));
    let interval = step(&mut resolver, 1, &[]);
    assert_eq!(resolver.state(id(count - 1)).unwrap().mode, Mode::Playing);
    assert_eq!(
        resolver
            .events()
            .iter()
            .filter(|event| event.kind == EventKind::Pulse)
            .count(),
        count as usize
    );
    finish(&mut resolver, interval);
}

#[shoop_test]
fn pre_clock_stop_suppresses_wrap_and_losing_request_has_terminal_outcome() {
    let input = topology(vec![node(0, 4, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    submit(&mut resolver, request(0, 1, 0, play()));
    let losing = submit(
        &mut resolver,
        request(
            0,
            2,
            4,
            Transition::Play {
                position: 2,
                retrigger: true,
            },
        ),
    );
    let stop = submit(&mut resolver, request(0, 3, 4, Transition::Stop));
    let events = run(&mut resolver, &[4]).0;
    assert!(events.iter().all(|event| event.frame < Frame(4)));
    assert!(resolver.completion(stop).unwrap().is_none());
    let events = run(&mut resolver, &[1]).0;
    assert_eq!(
        events,
        vec![Event {
            frame: Frame(4),
            target: id(0),
            kind: EventKind::Transition(Transition::Stop)
        }]
    );
    assert_eq!(
        resolver.completion(losing).unwrap().unwrap().outcome,
        Outcome::Superseded { frame: Frame(4) }
    );
    assert_eq!(
        resolver.completion(stop).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(4) }
    );
}

#[shoop_test]
fn mutual_sync_needs_a_seed_and_delivers_each_clock_edge_once() {
    let input = topology(vec![node(0, 4, Some(1)), node(1, 7, Some(0))], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    let pending = submit(
        &mut resolver,
        Request {
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 1,
            },
            ..request(1, 2, 0, play())
        },
    );
    assert!(run(&mut resolver, &[7]).0.is_empty());
    assert!(resolver.completion(pending).unwrap().is_none());
    submit(&mut resolver, request(0, 1, 7, play()));
    let events = run(&mut resolver, &[5]).0;
    for frame in [7, 11] {
        assert_eq!(
            events
                .iter()
                .filter(|event| event.frame == Frame(frame) && event.kind == EventKind::Pulse)
                .count(),
            2
        );
    }
    assert_eq!(
        resolver.completion(pending).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(11) }
    );
    assert_eq!(resolver.state(id(1)).unwrap().cycles, 0);
}

#[shoop_test]
fn reactive_stop_does_not_retract_a_clock_that_activated_it() {
    let input = topology(
        vec![node(0, 4, Some(0)), node(1, 4, None)],
        vec![Trigger {
            source: id(0),
            target: id(1),
            mode: Mode::Playing,
        }],
    );
    let mut resolver = resolver(&input, Limits::default());
    submit(&mut resolver, request(0, 1, 0, play()));
    let stop = submit(
        &mut resolver,
        Request {
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 0,
            },
            ..request(0, 2, 0, Transition::Stop)
        },
    );
    let events = run(&mut resolver, &[1]).0;
    assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Stopped);
    assert_eq!(resolver.state(id(1)).unwrap().mode, Mode::Playing);
    assert_eq!(
        resolver.completion(stop).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(0) }
    );
    assert_eq!(events[1].kind, EventKind::Pulse);
    assert_eq!(events[2].kind, EventKind::Transition(Transition::Stop));
}

#[shoop_test]
fn higher_reactive_start_can_lift_a_stop_mask_but_lower_priority_cannot() {
    let input = topology(vec![node(0, 4, None), node(1, 4, Some(0))], vec![]);
    for later_sequence in [2, 4] {
        let mut resolver = resolver(&input, Limits::default());
        submit(&mut resolver, request(0, 1, 0, play()));
        submit(&mut resolver, request(1, 3, 0, Transition::Stop));
        let start = submit(
            &mut resolver,
            Request {
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 0,
                },
                ..request(1, later_sequence, 0, play())
            },
        );
        run(&mut resolver, &[1]);
        let outcome = resolver.completion(start).unwrap().unwrap().outcome;
        if later_sequence == 4 {
            assert_eq!(outcome, Outcome::Applied { frame: Frame(0) });
            assert_eq!(resolver.state(id(1)).unwrap().last_pulse, Some(Frame(0)));
        } else {
            assert_eq!(outcome, Outcome::Cancelled { frame: Frame(0) });
            assert_eq!(resolver.state(id(1)).unwrap().last_pulse, None);
        }
    }
}

#[shoop_test]
fn pending_receipts_survive_stalls_and_stale_ids_cannot_cancel_reused_slots() {
    let input = topology(vec![node(0, 1, None)], vec![]);
    let mut resolver = resolver(
        &input,
        Limits {
            pending_requests: 1,
            ..Limits::default()
        },
    );
    let first = submit(&mut resolver, request(0, 1, 0, play()));
    run(&mut resolver, &[256]);
    assert_eq!(
        assert_no_heap(|| resolver.submit(request(0, 2, 256, Transition::Stop))),
        Err(Error::Capacity)
    );
    assert_eq!(resolver.retained_requests(), 1);
    assert_eq!(resolver.state(id(0)).unwrap().cycles, 255);
    assert!(assert_no_heap(|| resolver.take_completion(first))
        .unwrap()
        .is_some());
    let second = submit(&mut resolver, request(0, 2, 300, Transition::Stop));
    assert_eq!(second.slot(), first.slot());
    assert_ne!(second.generation(), first.generation());
    assert_eq!(
        assert_no_heap(|| resolver.cancel(first)),
        Err(Error::InvalidId)
    );
    assert_eq!(
        assert_no_heap(|| resolver.cancel(second)),
        Ok(CancelResult::Cancelled)
    );
    assert_eq!(
        assert_no_heap(|| resolver.cancel(second)),
        Ok(CancelResult::AlreadyTerminal)
    );
}

#[shoop_test]
fn record_to_play_and_monitored_capture_failure_use_exact_common_boundaries() {
    let input = topology(vec![node(0, 3, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    submit(
        &mut resolver,
        request(
            0,
            1,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
    );
    submit(&mut resolver, request(0, 2, 5, play()));
    let (_, samples) = run(&mut resolver, &[7]);
    assert_eq!(samples[4][0], (Mode::Recording, 0, 4));
    assert_eq!(samples[5][0], (Mode::Playing, 0, 5));
    assert_eq!(resolver.state(id(0)).unwrap().content_length, 5);
    submit(
        &mut resolver,
        request(
            0,
            3,
            7,
            Transition::Record {
                monitor_previous: true,
            },
        ),
    );
    run(&mut resolver, &[2]);
    let before = resolver.state(id(0)).unwrap();
    assert_eq!((before.position, before.capture_frames), (4, 2));
    let interval = step(
        &mut resolver,
        10,
        &[Observation::CaptureExhausted { target: id(0) }],
    );
    let after = resolver.state(id(0)).unwrap();
    assert_eq!(
        (after.mode, after.position, after.capture_frames),
        (Mode::Playing, 4, 2)
    );
    assert!(after.capture_incomplete);
    assert_eq!(after.content_revision, before.content_revision);
    finish(&mut resolver, interval);
}

#[shoop_test]
fn discarded_preview_has_no_visible_pulses_results_or_state_changes() {
    let input = topology(vec![node(0, 4, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    let start = submit(&mut resolver, request(0, 1, 0, play()));
    assert_no_heap(|| resolver.preview(Frame(4), &[])).unwrap();
    assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Stopped);
    assert!(resolver.events().is_empty());
    assert!(resolver.completion(start).unwrap().is_none());
    let refusal = [Observation::Refused {
        request: start,
        reason: Error::Capacity,
    }];
    for _ in 0..3 {
        assert_no_heap(|| resolver.preview(Frame(4), &refusal)).unwrap();
    }
    let interval = assert_no_heap(|| resolver.commit()).unwrap();
    assert!(resolver.events().is_empty());
    assert_eq!(
        resolver.completion(start).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(0),
            reason: Error::Capacity
        }
    );
    assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Stopped);
    assert_eq!(
        assert_no_heap(|| resolver.commit()),
        Err(Error::InvalidState)
    );
    finish(&mut resolver, interval);
}

#[shoop_test]
fn optional_journal_exhaustion_does_not_change_musical_state() {
    let input = topology(vec![node(0, 1, None)], vec![]);
    let mut a = resolver(&input, Limits::default());
    let mut b = resolver(
        &input,
        Limits {
            journal_events: 0,
            ..Limits::default()
        },
    );
    for resolver in [&mut a, &mut b] {
        submit(resolver, request(0, 1, 0, play()));
    }
    let expected = run(&mut a, &[128]);
    let actual = run(&mut b, &[1; 128]);
    assert_eq!(expected.1, actual.1);
    assert_eq!(a.state(id(0)), b.state(id(0)));
    assert!(actual.0.is_empty());
    assert_eq!(b.dropped_events(), expected.0.len() as u64);
}

#[shoop_test]
fn duplicate_failure_observations_are_refused_without_losing_monitored_playback() {
    let input = topology(vec![node(0, 4, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    submit(
        &mut resolver,
        request(
            0,
            1,
            0,
            Transition::Record {
                monitor_previous: true,
            },
        ),
    );
    run(&mut resolver, &[1]);
    let before = resolver.state(id(0)).unwrap();
    let observation = Observation::CaptureExhausted { target: id(0) };
    assert_eq!(
        assert_no_heap(|| resolver.preview(Frame(2), &[observation, observation])),
        Err(Error::DuplicateId)
    );
    assert_eq!(resolver.state(id(0)).unwrap(), before);
    assert_eq!(resolver.decisions(), Err(Error::InvalidState));
    assert_eq!(
        assert_no_heap(|| resolver.commit()),
        Err(Error::InvalidState)
    );
    let interval = step(&mut resolver, 2, &[observation]);
    assert_eq!(resolver.state(id(0)).unwrap().mode, Mode::Playing);
    finish(&mut resolver, interval);
}

#[shoop_test]
fn maximum_countdown_and_empty_finish_do_not_overflow_or_invent_content() {
    let input = topology(vec![node(0, 1, Some(0))], vec![]);
    let mut running = resolver(&input, Limits::default());
    let waiting = submit(
        &mut running,
        Request {
            timing: Timing::AfterTicks {
                source: id(0),
                skip: u64::MAX,
            },
            ..request(0, 1, 0, Transition::Stop)
        },
    );
    submit(&mut running, request(0, 2, 0, play()));
    run(&mut running, &[4]);
    assert!(running.completion(waiting).unwrap().is_none());
    let input = topology(vec![node(0, 0, Some(0))], vec![]);
    let mut empty = resolver(&input, Limits::default());
    submit(
        &mut empty,
        request(
            0,
            1,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
    );
    let finish_request = submit(
        &mut empty,
        Request {
            timing: Timing::AfterTicks {
                source: id(0),
                skip: 0,
            },
            ..request(0, 2, 0, play())
        },
    );
    run(&mut empty, &[1]);
    assert_eq!(
        empty.completion(finish_request).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(0),
            reason: Error::EmptyContent
        }
    );
    assert_eq!(empty.state(id(0)).unwrap().mode, Mode::Recording);
    assert_eq!(empty.state(id(0)).unwrap().capture_frames, 1);
}

#[shoop_test]
fn invalid_seek_and_revision_exhaustion_leave_previous_state_usable() {
    let input = topology(vec![node(0, 4, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    let seek = submit(
        &mut resolver,
        request(
            0,
            1,
            0,
            Transition::Seek {
                position: 1,
                retrigger: false,
            },
        ),
    );
    run(&mut resolver, &[1]);
    assert_eq!(
        resolver.completion(seek).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(0),
            reason: Error::InvalidState
        }
    );
    submit(
        &mut resolver,
        request(
            0,
            2,
            1,
            Transition::Replace {
                length: 4,
                revision: Revision(u64::MAX),
                preserve_position: false,
            },
        ),
    );
    run(&mut resolver, &[1]);
    let record = submit(
        &mut resolver,
        request(
            0,
            3,
            2,
            Transition::Record {
                monitor_previous: false,
            },
        ),
    );
    run(&mut resolver, &[1]);
    assert_eq!(
        resolver.completion(record).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(2),
            reason: Error::Exhausted
        }
    );
    assert_eq!(resolver.state(id(0)).unwrap().content_length, 4);
}

#[shoop_test]
fn replacement_returns_owners_and_prevents_generation_resurrection() {
    let input = topology(vec![node(0, 4, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    let future = submit(&mut resolver, request(0, 1, 9, play()));
    let mut newer = input.clone();
    newer.loops[0].id = LoopId::from_parts(0, 2).unwrap();
    let candidate = compile(Limits::default(), &newer).unwrap();
    let retired = assert_no_heap(|| resolver.replace_topology(Epoch(0), candidate)).unwrap();
    assert_eq!(retired.nodes()[0].id, id(0));
    assert_eq!(
        resolver.completion(future).unwrap().unwrap().outcome,
        Outcome::Failed {
            frame: Frame(0),
            reason: Error::InvalidId
        }
    );
    let empty = compile(Limits::default(), &TopologyDescription::default()).unwrap();
    let retired_again = assert_no_heap(|| resolver.replace_topology(Epoch(1), empty)).unwrap();
    let stale = compile(Limits::default(), &newer).unwrap();
    let (error, rejected) =
        assert_no_heap(|| resolver.replace_topology(Epoch(2), stale)).unwrap_err();
    assert_eq!(error, Error::InvalidId);
    assert!(resolver.topology().nodes().is_empty());
    assert_eq!(resolver.epoch(), Epoch(2));
    drop((retired, retired_again, rejected));
}

#[shoop_test]
fn invalid_phase_late_delivery_and_absolute_frame_exhaustion_are_explicit() {
    let input = topology(vec![node(0, u64::MAX, None)], vec![]);
    let mut resolver = resolver(&input, Limits::default());
    assert_eq!(
        assert_no_heap(|| resolver.preview(Frame(0), &[])),
        Err(Error::InvalidFrame)
    );
    run(&mut resolver, &[3]);
    let late = submit(&mut resolver, request(0, 1, 1, play()));
    let interval = step(&mut resolver, u64::MAX, &[]);
    let completion = resolver.completion(late).unwrap().unwrap();
    assert_eq!(
        (completion.requested, completion.effective),
        (Frame(1), Frame(3))
    );
    assert_eq!(
        assert_no_heap(|| resolver.submit(request(0, 2, 3, Transition::Stop))),
        Err(Error::InvalidState)
    );
    assert_eq!(
        assert_no_heap(|| resolver.finish(Frame(4))),
        Err(Error::InvalidState)
    );
    finish(&mut resolver, interval);
    assert_eq!(resolver.frame(), Frame(u64::MAX));
    assert_eq!(
        assert_no_heap(|| resolver.preview(Frame(u64::MAX), &[])),
        Err(Error::Exhausted)
    );
}
