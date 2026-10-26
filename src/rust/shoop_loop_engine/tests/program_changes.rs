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
fn play(retrigger: bool) -> Transition {
    Transition::Play {
        position: 0,
        retrigger,
    }
}
fn limits(journal: usize) -> Limits {
    Limits {
        loop_slots: 6,
        relations: 12,
        program_entries: 8,
        program_segments: 16,
        pending_requests: 8,
        journal_events: journal,
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
fn description(new: bool, record: bool) -> TopologyDescription {
    TopologyDescription {
        loops: (0..6)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot == 5 {
                    LoopKind::Composite
                } else {
                    LoopKind::Primitive
                },
                length: [2, 5, 7, 3, 4, 0][slot as usize],
                sync_source: (slot == 5).then(|| id(0)),
            })
            .collect(),
        programs: vec![ProgramDescription {
            source: id(5),
            nominal_unit_frames: 2,
            sections: vec![Section {
                entries: if new {
                    vec![
                        entry(1, 0, 2, record.then_some(Mode::Playing)),
                        entry(3, 0, 2, record.then_some(Mode::Recording)),
                    ]
                } else {
                    vec![entry(1, 0, 3, None), entry(2, 1, 2, None)]
                },
            }],
        }],
        triggers: vec![],
    }
}
fn prepared(input: &TopologyDescription, journal: usize) -> PreparedTopology {
    compile(limits(journal), input).unwrap()
}
fn submit(r: &mut Resolver, target: u32, seq: u64, at: u64, transition: Transition) -> RequestId {
    assert_no_heap(|| {
        r.submit(Request {
            target: id(target),
            sequence: Sequence(seq),
            at: Frame(at),
            timing: Timing::At,
            transition,
        })
    })
    .unwrap()
}
fn start(r: &mut Resolver) {
    submit(r, 0, 1, 0, play(false));
    submit(r, 5, 2, 0, play(false));
}
fn advance(r: &mut Resolver, end: u64) {
    while r.frame() < Frame(end) {
        let interval = assert_no_heap(|| r.preview(Frame(end), &[])).unwrap();
        assert_no_heap(|| r.commit()).unwrap();
        assert_no_heap(|| r.finish(interval.end)).unwrap();
    }
}
fn event(out: &mut Vec<Event>, f: u64, target: u32, kind: EventKind) {
    out.push(Event {
        frame: Frame(f),
        target: id(target),
        kind,
    });
}
fn pulse(out: &mut Vec<Event>, f: u64, target: u32) {
    event(out, f, target, EventKind::Pulse);
}
fn expected_events(f: u64) -> Vec<Event> {
    let mut out = vec![];
    if f == 6 {
        event(&mut out, f, 5, EventKind::Transition(Transition::Program));
    }
    if f == 0 {
        event(&mut out, f, 0, EventKind::Transition(play(false)));
        pulse(&mut out, f, 0);
    } else if f % 2 == 0 {
        event(&mut out, f, 0, EventKind::Wrap);
        pulse(&mut out, f, 0);
    }
    if f == 6 {
        event(&mut out, f, 2, EventKind::Transition(Transition::Stop));
    }
    if f == 0 {
        event(&mut out, f, 5, EventKind::Transition(play(false)));
    }
    if f % 2 == 0 {
        pulse(&mut out, f, 5);
        event(
            &mut out,
            f,
            5,
            EventKind::Iteration {
                iteration: if f < 6 {
                    (f / 2) % 3
                } else {
                    ((f - 6) / 2) % 2
                },
                passes: if f < 6 { 0 } else { 1 + (f - 6) / 4 },
            },
        );
    }
    if f == 0 {
        event(&mut out, f, 1, EventKind::Transition(play(true)));
        pulse(&mut out, f, 1);
    } else if f % 5 == 0 {
        event(&mut out, f, 1, EventKind::Wrap);
        pulse(&mut out, f, 1);
    }
    if f == 2 {
        event(&mut out, f, 2, EventKind::Transition(play(true)));
        pulse(&mut out, f, 2);
    }
    if f == 6 {
        event(&mut out, f, 3, EventKind::Transition(play(true)));
        pulse(&mut out, f, 3);
    } else if f > 6 && (f - 6) % 3 == 0 {
        event(&mut out, f, 3, EventKind::Wrap);
        pulse(&mut out, f, 3);
    }
    out
}

#[shoop_test]
fn running_change_activates_at_zero_with_continuity_removed_target_cleanup_and_exact_journals() {
    for (parts, journal, reverse) in [
        (vec![20], 1024, false),
        (vec![1; 20], 0, true),
        (vec![3, 2, 7, 1, 7], 1, true),
    ] {
        let mut old = description(false, false);
        let mut new = description(true, false);
        if reverse {
            old.loops.reverse();
            new.loops.reverse();
        }
        let mut r = Resolver::prepare(prepared(&old, journal)).unwrap();
        start(&mut r);
        let candidate = prepared(&new, journal);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
                .unwrap();
        let mut dropped = 0;
        for frames in parts {
            let end = Frame(r.frame().0 + frames);
            while r.frame() < end {
                let f = r.frame().0;
                let before = r.state(id(5)).unwrap();
                let epoch = r.epoch();
                let interval = assert_no_heap(|| r.preview(end, &[])).unwrap();
                assert_eq!(r.state(id(5)).unwrap(), before);
                assert_eq!(r.epoch(), epoch);
                if f <= 6 {
                    assert_eq!(r.completion(change).unwrap(), None);
                }
                for offset in 0..interval.end.0 - f {
                    let frame = f + offset;
                    for d in r.decisions().unwrap() {
                        let s = d.state;
                        let expected = match s.id.slot() {
                            0 => (Mode::Playing, frame % 2, frame / 2),
                            1 => (Mode::Playing, frame % 5, frame / 5),
                            2 if (2..6).contains(&frame) => (Mode::Playing, frame - 2, 0),
                            3 if frame >= 6 => (Mode::Playing, (frame - 6) % 3, (frame - 6) / 3),
                            5 => (Mode::Playing, 0, 0),
                            _ => (Mode::Stopped, 0, 0),
                        };
                        assert_eq!(
                            (
                                s.mode,
                                if s.plays() { s.position + offset } else { 0 },
                                s.cycles
                            ),
                            expected,
                            "frame{frame},id{:?}",
                            s.id
                        );
                        if s.id == id(5) {
                            assert_eq!(
                                s.composite.unwrap(),
                                CompositeState {
                                    iteration: if frame < 6 {
                                        (frame / 2) % 3
                                    } else {
                                        ((frame - 6) / 2) % 2
                                    },
                                    passes: if frame < 6 { 0 } else { 1 + (frame - 6) / 4 },
                                    entered_at: Some(Frame(frame / 2 * 2))
                                }
                            );
                        }
                    }
                }
                assert_no_heap(|| r.commit()).unwrap();
                assert_eq!(r.epoch(), Epoch(if f >= 6 { 1 } else { 0 }));
                assert_eq!(
                    r.state(id(5)).unwrap().content_length,
                    if f >= 6 { 4 } else { 6 }
                );
                let expected = expected_events(f);
                assert_eq!(
                    r.events(),
                    &expected[..expected.len().min(journal)],
                    "frame {f}"
                );
                dropped += expected.len().saturating_sub(journal) as u64;
                assert_eq!(r.dropped_events(), dropped);
                assert_no_heap(|| r.finish(interval.end)).unwrap();
            }
        }
        assert_eq!(
            r.completion(change).unwrap().unwrap(),
            Completion {
                id: change,
                sequence: Sequence(3),
                requested: Frame(1),
                effective: Frame(1),
                outcome: Outcome::Applied { frame: Frame(6) }
            }
        );
        assert_eq!(
            assert_no_heap(|| r.take_completion(change)),
            Err(Error::InvalidState)
        );
        let returned = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        assert_eq!(returned.activated_epoch, Some(Epoch(1)));
        assert_eq!(returned.topology.program(id(5)).unwrap().iterations, 3);
        assert_eq!(r.topology().program(id(5)).unwrap().iterations, 2);
        assert_eq!(r.retained_requests(), 2);
        drop(returned);
    }
}

#[shoop_test]
fn candidate_capture_preflight_failure_preserves_old_configuration_and_retries_without_revision_gaps(
) {
    let mut r = Resolver::prepare(prepared(&description(false, false), 1024)).unwrap();
    start(&mut r);
    let candidate = prepared(&description(true, true), 1024);
    let change =
        assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
            .unwrap();
    advance(&mut r, 6);
    let previous = r.state(id(3)).unwrap();
    assert_no_heap(|| r.preview(Frame(7), &[])).unwrap();
    let decision = r
        .decisions()
        .unwrap()
        .iter()
        .find(|d| d.state.id == id(3))
        .unwrap();
    assert_eq!(decision.state.mode, Mode::Recording);
    assert_eq!(decision.origin, Some(change));
    assert_eq!(decision.state.capture_revision, Revision(1));
    assert_eq!(r.state(id(3)).unwrap(), previous);
    assert_eq!(r.epoch(), Epoch(0));
    let interval = assert_no_heap(|| {
        r.preview(
            Frame(7),
            &[Observation::Refused {
                request: change,
                reason: Error::Capacity,
            }],
        )
    })
    .unwrap();
    assert_no_heap(|| r.commit()).unwrap();
    assert_eq!(r.state(id(3)).unwrap(), previous);
    assert_eq!(r.epoch(), Epoch(0));
    assert!(!r
        .events()
        .iter()
        .any(|e| e.kind == EventKind::Transition(Transition::Program)));
    assert_no_heap(|| r.finish(interval.end)).unwrap();
    let rejected = assert_no_heap(|| r.take_program_change(change))
        .unwrap()
        .unwrap();
    assert_eq!(
        rejected.completion.outcome,
        Outcome::Failed {
            frame: Frame(6),
            reason: Error::Capacity
        }
    );
    assert_eq!(rejected.activated_epoch, None);
    let retry = assert_no_heap(|| {
        r.stage_program(Epoch(0), id(5), Sequence(4), Frame(7), rejected.topology)
    })
    .unwrap();
    assert_ne!(change, retry);
    assert_eq!(assert_no_heap(|| r.cancel(change)), Err(Error::InvalidId));
    advance(&mut r, 13);
    assert_eq!(
        r.completion(retry).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(12) }
    );
    assert_eq!(r.state(id(3)).unwrap().capture_revision, Revision(1));
    assert_eq!(r.state(id(3)).unwrap().capture_frames, 1);
    let returned = assert_no_heap(|| r.take_program_change(retry))
        .unwrap()
        .unwrap();
    drop(returned);
}

#[shoop_test]
fn idle_clock_cancellation_staleness_and_owner_backpressure_do_not_publish_early_or_block_music() {
    for case in 0..4 {
        let mut r = Resolver::prepare(prepared(&description(false, false), 1)).unwrap();
        start(&mut r);
        let candidate = prepared(&description(true, false), 1);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
                .unwrap();
        let extra = prepared(&description(true, false), 1);
        let (error, extra) =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(99), Frame(1), extra))
                .unwrap_err();
        assert_eq!(error, Error::Capacity);
        drop(extra);
        match case {
            0 => {
                submit(&mut r, 0, 4, 4, Transition::Stop);
                submit(&mut r, 0, 5, 10, play(false));
                advance(&mut r, 10);
                assert_eq!(r.completion(change).unwrap(), None);
                advance(&mut r, 13);
                assert_eq!(
                    r.completion(change).unwrap().unwrap().outcome,
                    Outcome::Applied { frame: Frame(12) }
                );
            }
            1 => {
                advance(&mut r, 2);
                assert_no_heap(|| r.cancel(change)).unwrap();
                advance(&mut r, 13);
                assert_eq!(
                    r.completion(change).unwrap().unwrap().outcome,
                    Outcome::Cancelled { frame: Frame(2) }
                );
            }
            2 => {
                submit(&mut r, 5, 4, 2, Transition::Stop);
                advance(&mut r, 13);
                assert_eq!(
                    r.completion(change).unwrap().unwrap().outcome,
                    Outcome::Cancelled { frame: Frame(2) }
                );
            }
            _ => {
                advance(&mut r, 2);
                let candidate = prepared(&description(false, false), 1);
                let old = assert_no_heap(|| r.replace_topology(Epoch(0), candidate)).unwrap();
                drop(old);
                advance(&mut r, 13);
                assert_eq!(
                    r.completion(change).unwrap().unwrap().outcome,
                    Outcome::Failed {
                        frame: Frame(2),
                        reason: Error::InvalidState
                    }
                );
            }
        }
        assert!(r.state(id(0)).unwrap().cycles > 0);
        let result = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        assert_eq!(
            result.activated_epoch,
            if case == 0 { Some(Epoch(1)) } else { None }
        );
        drop(result);
    }
}

#[shoop_test]
fn same_frame_stop_seek_restart_and_cancelled_preview_keep_one_configuration_cut() {
    for case in 0..8 {
        let mut r = Resolver::prepare(prepared(&description(false, false), 1024)).unwrap();
        start(&mut r);
        let candidate = prepared(&description(true, false), 1024);
        let change = assert_no_heap(|| {
            r.stage_program(
                Epoch(0),
                id(5),
                Sequence(if case == 4 { 4 } else { 3 }),
                Frame(1),
                candidate,
            )
        })
        .unwrap();
        let transition = match case {
            1 => play(true),
            2 => Transition::SeekIteration {
                iteration: 1,
                retrigger: false,
            },
            3 | 4 => Transition::Stop,
            5 => play(false),
            _ => Transition::SeekIteration {
                iteration: 0,
                retrigger: false,
            },
        };
        let control = submit(&mut r, 5, if case == 4 { 3 } else { 4 }, 4, transition);
        advance(&mut r, 4);
        let observations = if case == 6 {
            vec![Observation::Refused {
                request: control,
                reason: Error::Capacity,
            }]
        } else {
            vec![]
        };
        let mut interval = assert_no_heap(|| r.preview(Frame(5), &observations)).unwrap();
        if case == 7 {
            assert_no_heap(|| r.cancel(change)).unwrap();
            assert_eq!(assert_no_heap(|| r.commit()), Err(Error::InvalidState));
            interval = assert_no_heap(|| r.preview(Frame(5), &observations)).unwrap();
        }
        assert_no_heap(|| r.commit()).unwrap();
        assert_no_heap(|| r.finish(interval.end)).unwrap();
        advance(&mut r, 10);
        let expected = match case {
            3 | 7 => Outcome::Cancelled { frame: Frame(4) },
            2 => Outcome::Applied { frame: Frame(8) },
            5 | 6 => Outcome::Applied { frame: Frame(6) },
            _ => Outcome::Applied { frame: Frame(4) },
        };
        assert_eq!(
            r.completion(change).unwrap().unwrap().outcome,
            expected,
            "case {case}"
        );
        assert_eq!(r.epoch(), Epoch(if case == 3 || case == 7 { 0 } else { 1 }));
        assert_eq!(
            r.state(id(5)).unwrap().mode,
            if case == 3 || case == 4 {
                Mode::Stopped
            } else {
                Mode::Playing
            }
        );
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
}

#[shoop_test]
fn stopped_empty_and_explicit_end_changes_do_not_manufacture_playback_or_clock_edges() {
    for play_request in [false, true] {
        let mut old = description(false, false);
        old.loops[5].sync_source = None;
        let mut new = old.clone();
        new.programs[0].sections.clear();
        new.loops[5].length = 42;
        let mut r = Resolver::prepare(prepared(&old, 1024)).unwrap();
        if play_request {
            submit(&mut r, 5, 1, 0, play(false));
        }
        let candidate = prepared(&new, 1024);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(2), Frame(0), candidate))
                .unwrap();
        advance(&mut r, 4);
        assert_eq!(r.epoch(), Epoch(1));
        assert_eq!(r.state(id(5)).unwrap().mode, Mode::Stopped);
        assert_eq!(r.state(id(5)).unwrap().last_pulse, None);
        assert_eq!(r.state(id(5)).unwrap().content_length, 0);
        for slot in 0..5 {
            assert_eq!(r.state(id(slot)).unwrap().mode, Mode::Stopped);
        }
        assert_eq!(
            r.completion(change).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(0) }
        );
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
    for play_request in [false, true] {
        let mut r = Resolver::prepare(prepared(&description(false, false), 0)).unwrap();
        if play_request {
            submit(&mut r, 5, 1, 0, play(false));
        }
        let candidate = prepared(&description(true, false), 0);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(2), Frame(0), candidate))
                .unwrap();
        advance(&mut r, 1);
        assert_eq!(
            r.state(id(3)).unwrap().mode,
            if play_request {
                Mode::Playing
            } else {
                Mode::Stopped
            }
        );
        assert_eq!(r.state(id(5)).unwrap().content_length, 4);
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
    let mut old = description(false, false);
    for entry in &mut old.programs[0].sections[0].entries {
        entry.mode = Some(Mode::Playing);
    }
    let mut r = Resolver::prepare(prepared(&old, 1024)).unwrap();
    start(&mut r);
    let candidate = prepared(&description(true, false), 1024);
    let change =
        assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
            .unwrap();
    advance(&mut r, 7);
    assert_eq!(
        r.completion(change).unwrap().unwrap().outcome,
        Outcome::Applied { frame: Frame(6) }
    );
    for slot in [1, 2, 3, 5] {
        assert_eq!(r.state(id(slot)).unwrap().mode, Mode::Stopped);
    }
    assert!(r
        .events()
        .iter()
        .any(|event| event.target == id(5) && event.kind == EventKind::ProgramEnd));
    submit(&mut r, 5, 4, 8, play(false));
    advance(&mut r, 9);
    for slot in [1, 3, 5] {
        assert_eq!(r.state(id(slot)).unwrap().mode, Mode::Playing);
    }
    let owner = assert_no_heap(|| r.take_program_change(change))
        .unwrap()
        .unwrap();
    drop(owner);
    let mut r = Resolver::prepare(prepared(&old, 1024)).unwrap();
    start(&mut r);
    let candidate = prepared(&description(true, false), 1024);
    let change =
        assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
            .unwrap();
    submit(&mut r, 5, 4, 6, play(true));
    advance(&mut r, 7);
    assert_eq!(r.state(id(5)).unwrap().mode, Mode::Playing);
    assert_eq!(r.state(id(5)).unwrap().composite.unwrap().passes, 0);
    assert_eq!(r.state(id(3)).unwrap().mode, Mode::Playing);
    assert!(!r
        .events()
        .iter()
        .any(|event| event.kind == EventKind::ProgramEnd));
    let owner = assert_no_heap(|| r.take_program_change(change))
        .unwrap()
        .unwrap();
    drop(owner);
}

#[shoop_test]
fn removed_nested_program_stops_descendants_before_new_graph_order_unless_direct_control_wins() {
    for override_stop in [false, true] {
        let mut old = description(false, false);
        old.loops[4].kind = LoopKind::Composite;
        old.loops[4].length = 0;
        old.loops[4].sync_source = Some(id(0));
        old.programs[0].sections[0].entries = vec![entry(4, 0, 3, None)];
        old.programs.push(ProgramDescription {
            source: id(4),
            nominal_unit_frames: 2,
            sections: vec![Section {
                entries: vec![entry(2, 0, 2, None)],
            }],
        });
        let mut new = old.clone();
        new.programs[0].sections[0].entries = vec![entry(3, 0, 2, None)];
        let mut r = Resolver::prepare(prepared(&old, 1024)).unwrap();
        start(&mut r);
        let candidate = prepared(&new, 1024);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
                .unwrap();
        if override_stop {
            submit(&mut r, 4, 4, 6, play(true));
        }
        advance(&mut r, 6);
        assert_eq!(r.state(id(4)).unwrap().mode, Mode::Playing);
        assert_eq!(r.state(id(2)).unwrap().mode, Mode::Playing);
        let interval = assert_no_heap(|| r.preview(Frame(7), &[])).unwrap();
        let expected = if override_stop {
            Mode::Playing
        } else {
            Mode::Stopped
        };
        for slot in [2, 4] {
            assert_eq!(
                r.decisions()
                    .unwrap()
                    .iter()
                    .find(|d| d.state.id == id(slot))
                    .unwrap()
                    .state
                    .mode,
                expected
            );
        }
        assert_no_heap(|| r.commit()).unwrap();
        assert_eq!(r.epoch(), Epoch(1));
        assert_eq!(r.state(id(3)).unwrap().mode, Mode::Playing);
        assert_eq!(
            r.state(id(4)).unwrap().last_pulse,
            Some(Frame(if override_stop { 6 } else { 4 }))
        );
        assert_no_heap(|| r.finish(interval.end)).unwrap();
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
}

#[shoop_test]
fn invalid_candidate_shapes_epochs_and_raw_program_tags_return_owners_without_mutation() {
    for case in 0..6 {
        let old = description(false, false);
        let mut new = description(true, false);
        let mut candidate_limits = limits(0);
        match case {
            0 => new.loops[4].id = LoopId::from_parts(4, 2).unwrap(),
            1 => new.loops[5].sync_source = None,
            2 => new.triggers.push(Trigger {
                source: id(0),
                target: id(4),
                mode: Mode::Playing,
            }),
            3 => new.loops[4].length = 9,
            4 => candidate_limits.pending_requests += 1,
            _ => {}
        }
        let mut r = Resolver::prepare(prepared(&old, 0)).unwrap();
        start(&mut r);
        advance(&mut r, 2);
        let before = r.state(id(5)).unwrap();
        let candidate = compile(candidate_limits, &new).unwrap();
        let records = candidate.encoded_records();
        let bytes = candidate.storage_bytes();
        let (error, owner) = assert_no_heap(|| {
            r.stage_program(
                Epoch(if case == 5 { 99 } else { 0 }),
                id(5),
                Sequence(3),
                Frame(2),
                candidate,
            )
        })
        .unwrap_err();
        assert_eq!(
            error,
            if case == 5 {
                Error::InvalidState
            } else {
                Error::InvalidProgram
            }
        );
        assert_eq!(owner.encoded_records(), records);
        assert_eq!(owner.storage_bytes(), bytes);
        assert_eq!(r.state(id(5)).unwrap(), before);
        assert_eq!(r.epoch(), Epoch(0));
        assert_eq!(r.retained_requests(), 2);
        drop(owner);
        assert_eq!(
            assert_no_heap(|| r.submit(Request {
                target: id(5),
                sequence: Sequence(3),
                at: Frame(2),
                timing: Timing::At,
                transition: Transition::Program
            })),
            Err(Error::InvalidProgram)
        );
    }
}

#[shoop_test]
fn retained_change_and_full_request_slots_allow_all_128_one_sample_boundaries() {
    for journal in [0, 1, 1024] {
        let mut old = description(false, false);
        let mut new = description(true, false);
        old.loops[0].length = 1;
        new.loops[0].length = 1;
        let mut bounds = limits(journal);
        bounds.pending_requests = 3;
        let mut r = Resolver::prepare(compile(bounds, &old).unwrap()).unwrap();
        start(&mut r);
        let candidate = compile(bounds, &new).unwrap();
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
                .unwrap();
        for f in 0..128 {
            let interval = assert_no_heap(|| r.preview(Frame(128), &[])).unwrap();
            assert_eq!(
                interval,
                Interval {
                    start: Frame(f),
                    end: Frame(f + 1)
                }
            );
            assert_no_heap(|| r.commit()).unwrap();
            assert_eq!(r.epoch(), Epoch(if f < 3 { 0 } else { 1 }));
            assert_eq!(r.state(id(0)).unwrap().cycles, f);
            assert_eq!(r.state(id(1)).unwrap().position, f % 5);
            assert_eq!(r.state(id(1)).unwrap().cycles, f / 5);
            assert_eq!(
                r.state(id(5)).unwrap().composite.unwrap(),
                CompositeState {
                    iteration: if f < 3 { f } else { (f - 3) % 2 },
                    passes: if f < 3 { 0 } else { 1 + (f - 3) / 2 },
                    entered_at: Some(Frame(f))
                }
            );
            if f >= 3 {
                assert_eq!(r.state(id(2)).unwrap().mode, Mode::Stopped);
                assert_eq!(r.state(id(3)).unwrap().position, (f - 3) % 3);
            }
            assert_eq!(r.retained_requests(), 3);
            assert_no_heap(|| r.finish(interval.end)).unwrap();
        }
        assert_eq!(
            r.completion(change).unwrap().unwrap().outcome,
            Outcome::Applied { frame: Frame(3) }
        );
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
}

#[shoop_test]
fn old_zero_actions_never_start_a_capture_in_the_candidate_preview_or_double_apply_on_refusal() {
    for refuse in [false, true] {
        let mut old = description(false, false);
        old.loops[0].length = 1;
        old.programs[0].sections[0].entries = vec![entry(1, 0, 1, None), entry(4, 1, 2, None)];
        old.triggers.push(Trigger {
            source: id(1),
            target: id(2),
            mode: Mode::Recording,
        });
        let mut new = old.clone();
        new.programs[0].sections[0].entries = vec![entry(3, 0, 2, Some(Mode::Recording))];
        let mut r = Resolver::prepare(prepared(&old, 1024)).unwrap();
        start(&mut r);
        let candidate = prepared(&new, 1024);
        let change =
            assert_no_heap(|| r.stage_program(Epoch(0), id(5), Sequence(3), Frame(1), candidate))
                .unwrap();
        advance(&mut r, 3);
        assert_eq!(r.state(id(1)).unwrap().mode, Mode::Stopped);
        assert_eq!(r.state(id(2)).unwrap().capture_revision, Revision(1));
        assert_eq!(r.state(id(2)).unwrap().capture_frames, 3);
        let mut interval = assert_no_heap(|| r.preview(Frame(4), &[])).unwrap();
        let decisions = r.decisions().unwrap();
        let old_target = decisions.iter().find(|d| d.state.id == id(2)).unwrap();
        assert_eq!(old_target.state.capture_revision, Revision(1));
        assert_eq!(old_target.state.capture_frames, 3);
        let new_target = decisions.iter().find(|d| d.state.id == id(3)).unwrap();
        assert_eq!(new_target.state.capture_revision, Revision(1));
        assert_eq!(new_target.origin, Some(change));
        if refuse {
            interval = assert_no_heap(|| {
                r.preview(
                    Frame(4),
                    &[Observation::Refused {
                        request: change,
                        reason: Error::Capacity,
                    }],
                )
            })
            .unwrap();
        }
        assert_no_heap(|| r.commit()).unwrap();
        assert_eq!(r.epoch(), Epoch(if refuse { 0 } else { 1 }));
        assert_eq!(
            r.state(id(2)).unwrap().capture_revision,
            Revision(if refuse { 2 } else { 1 })
        );
        assert_eq!(
            r.state(id(3)).unwrap().mode,
            if refuse {
                Mode::Stopped
            } else {
                Mode::Recording
            }
        );
        assert_eq!(
            r.events()
                .iter()
                .any(|e| e.target == id(1) && e.kind == EventKind::Pulse),
            refuse
        );
        assert_no_heap(|| r.finish(interval.end)).unwrap();
        assert_eq!(
            r.state(id(2)).unwrap().capture_frames,
            if refuse { 1 } else { 4 }
        );
        let owner = assert_no_heap(|| r.take_program_change(change))
            .unwrap()
            .unwrap();
        drop(owner);
    }
}
