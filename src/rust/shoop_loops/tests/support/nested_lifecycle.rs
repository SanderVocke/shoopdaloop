use super::*;

fn iteration(value: u64) -> Transition {
    Transition::SeekIteration {
        iteration: value,
        retrigger: false,
    }
}
fn play_iteration(value: u64) -> Transition {
    Transition::PlayIteration {
        iteration: value,
        retrigger: true,
    }
}
fn reference_midi() -> [Vec<(u64, Vec<u8>)>; 4] {
    let mut out: [Vec<_>; 4] = std::array::from_fn(|_| Vec::new());
    out[0] = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
    for frame in 1..19 {
        if frame > 1 {
            out[1].push((frame, vec![0x80, 61, 0]));
        }
        restored(frame, &mut out[1]);
        out[1].push((frame, vec![0x90, 61, 80]));
    }
    out[1].push((19, vec![0x80, 61, 0]));
    for frame in (1..19).step_by(2) {
        restored(frame, &mut out[2]);
        out[2].push((frame, vec![0x90, 62, 90]));
        out[2].push((frame + 1, vec![0x80, 62, 32]));
    }
    restored(2, &mut out[3]);
    out[3].push((2, vec![0x90, 63, 70]));
    out[3].push((3, vec![0x80, 63, 0]));
    for frame in [6, 11, 12, 13] {
        for channel in 0..16 {
            for packet in [
                [0xe0 | channel, 0, 64],
                [0xb0 | channel, 64, 0],
                [0xb0 | channel, 69, 0],
                [0xb0 | channel, 66, 0],
            ] {
                out[3].push((frame, packet.to_vec()));
            }
            if channel == 0 && frame != 11 {
                out[3].push((frame, vec![0x90, 63, 70]));
            }
        }
        if frame != 11 {
            out[3].push((frame, vec![0x80, 63, 0]));
        }
    }
    out
}
fn reference_events() -> Vec<Event> {
    use EventKind::{Iteration, Pulse, Transition as Changed, Wrap};
    let mut events = vec![Event {
        frame: Frame(0),
        target: id(1),
        kind: Changed(Transition::Replace {
            length: 1,
            revision: Revision(1),
            preserve_position: false,
        }),
    }];
    let outer = [0, 1, 3, 4, 5, 1, 2, 3, 4, 5, 0, 0, 1, 2, 3, 4, 5, 0, 0];
    let inner = [0, 0, 2, 3, 4, 0, 1, 2, 3, 4, 0, 0, 0, 1, 2, 3, 4, 0, 0];
    for r in 0..19usize {
        let mut push = |target, kind| {
            events.push(Event {
                frame: Frame(r as u64 + 1),
                target: id(target),
                kind,
            })
        };
        if r == 18 {
            push(1, Changed(Transition::Stop));
            push(2, Changed(Transition::Stop));
        } else {
            push(
                1,
                if r == 0 {
                    Changed(Transition::Play {
                        position: 0,
                        retrigger: true,
                    })
                } else {
                    Wrap
                },
            );
            push(1, Pulse);
            if r == 0 {
                push(
                    2,
                    Changed(Transition::Play {
                        position: 0,
                        retrigger: false,
                    }),
                );
                push(2, Pulse);
            } else if r % 2 == 0 {
                push(2, Wrap);
                push(2, Pulse);
            }
        }
        let parent = match r {
            0 => Some(play_iteration(0)),
            2 | 7 => Some(iteration(3)),
            5 => Some(iteration(1)),
            10 | 18 => Some(Transition::Stop),
            12 => Some(play_iteration(1)),
            17 => Some(iteration(0)),
            _ => None,
        };
        if let Some(t) = parent {
            push(4, Changed(t));
        }
        if r != 10 && r != 18 {
            push(4, Pulse);
        }
        if !(10..=11).contains(&r) && r != 18 {
            push(
                4,
                Iteration {
                    iteration: outer[r],
                    passes: 0,
                },
            );
        }
        let child = match r {
            0 | 10 | 17 | 18 => Some(Transition::Stop),
            1 | 12 => Some(play_iteration(0)),
            2 | 7 => Some(iteration(2)),
            5 => Some(iteration(0)),
            _ => None,
        };
        if let Some(t) = child {
            push(5, Changed(t));
        }
        if ![0, 10, 17, 18].contains(&r) {
            push(5, Pulse);
        }
        if (1..=9).contains(&r) || (12..=16).contains(&r) {
            push(
                5,
                Iteration {
                    iteration: inner[r],
                    passes: 0,
                },
            );
        }
        let primitive = match r {
            1 | 5 | 12 => Some(Transition::Play {
                position: 0,
                retrigger: true,
            }),
            2 | 14 => Some(Transition::Record {
                monitor_previous: false,
            }),
            4 | 7 | 16 => Some(Transition::Stop),
            10 => Some(Transition::Play {
                position: 1,
                retrigger: true,
            }),
            _ => None,
        };
        if let Some(t) = primitive {
            push(3, Changed(t));
        }
        if r == 11 {
            push(3, Wrap);
        }
        if [1, 2, 5, 10, 11, 12, 14].contains(&r) {
            push(3, Pulse);
        }
    }
    events
}
fn assert_state(frame: usize, state: State) {
    let slot = state.id.slot();
    assert_eq!(state.id, id(slot));
    assert!(!state.capture_incomplete);
    assert!(!state.monitor_previous);
    let (mode, length, revision, position, cycles, capture, capture_revision, last, composite) =
        if frame == 0 {
            (
                Mode::Stopped,
                match slot {
                    1 | 3 => 1,
                    2 => 2,
                    4 => 6,
                    5 => 5,
                    _ => 0,
                },
                u64::from(slot == 1),
                0,
                0,
                0,
                0,
                None,
                (slot >= 4).then_some(shoop_loops::state::CompositeState::default()),
            )
        } else {
            let r = frame - 1;
            match slot {
                0 => (Mode::Stopped, 0, 0, 0, 0, 0, 0, None, None),
                1 => (
                    if r < 18 { Mode::Playing } else { Mode::Stopped },
                    1,
                    1,
                    0,
                    r.min(17) as u64,
                    0,
                    0,
                    Some(Frame((r.min(17) + 1) as u64)),
                    None,
                ),
                2 => (
                    if r < 18 { Mode::Playing } else { Mode::Stopped },
                    2,
                    0,
                    if r < 18 { (r % 2) as u64 } else { 0 },
                    (r.min(17) / 2) as u64,
                    0,
                    0,
                    Some(Frame((1 + 2 * (r.min(17) / 2)) as u64)),
                    None,
                ),
                3 => {
                    let mode = match r {
                        1 | 5 | 6 | 10..=13 => Mode::Playing,
                        2 | 3 | 14 | 15 => Mode::Recording,
                        _ => Mode::Stopped,
                    };
                    let position = match r {
                        2 | 3 | 6 | 10 | 13 => 1,
                        14 | 15 => 2,
                        _ => 0,
                    };
                    let capture = match r {
                        3 | 15 => 1,
                        4..=13 | 16.. => 2,
                        _ => 0,
                    };
                    let last = match r {
                        0 => None,
                        1 => Some(2),
                        2..=4 => Some(3),
                        5..=9 => Some(6),
                        10 => Some(11),
                        11 => Some(12),
                        12 | 13 => Some(13),
                        _ => Some(15),
                    };
                    (
                        mode,
                        if r < 4 { 1 } else { 2 },
                        if r < 4 {
                            0
                        } else if r < 16 {
                            1
                        } else {
                            2
                        },
                        position,
                        u64::from(r == 11),
                        capture,
                        if r < 2 {
                            0
                        } else if r < 14 {
                            1
                        } else {
                            2
                        },
                        last.map(Frame),
                        None,
                    )
                }
                4 | 5 => {
                    let outer = slot == 4;
                    let playing = if outer {
                        r < 10 || (12..=17).contains(&r)
                    } else {
                        (1..=9).contains(&r) || (12..=16).contains(&r)
                    };
                    let index = if outer {
                        [0, 1, 3, 4, 5, 1, 2, 3, 4, 5, 0, 0, 1, 2, 3, 4, 5, 0, 0][r]
                    } else {
                        [0, 0, 2, 3, 4, 0, 1, 2, 3, 4, 0, 0, 0, 1, 2, 3, 4, 0, 0][r]
                    };
                    let last = if !outer && r == 0 {
                        None
                    } else {
                        Some(Frame(match (outer, r) {
                            (_, 10) => 10,
                            (true, 18) => 18,
                            (false, 17 | 18) => 17,
                            _ => r as u64 + 1,
                        }))
                    };
                    (
                        if playing {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        },
                        if outer { 6 } else { 5 },
                        0,
                        0,
                        0,
                        0,
                        0,
                        last,
                        Some(shoop_loops::state::CompositeState {
                            iteration: index,
                            passes: 0,
                            entered_at: Some(Frame(if r == 11 { 11 } else { frame as u64 })),
                        }),
                    )
                }
                _ => unreachable!(),
            }
        };
    assert_eq!(
        (
            state.mode,
            state.content_length,
            state.content_revision,
            state.position,
            state.cycles
        ),
        (mode, length, Revision(revision), position, cycles),
        "frame {frame} slot {slot}"
    );
    assert_eq!(
        (
            state.capture_frames,
            state.capture_revision,
            state.last_pulse,
            state.composite
        ),
        (capture, Revision(capture_revision), last, composite),
        "frame {frame} slot {slot}"
    );
}
#[shoop_test]
fn nested_seek_stop_restart_and_direct_priority_preserve_media_cuts_and_pending_outcomes() {
    for (parts, journal, reversed) in [
        (vec![20], 1024, false),
        (vec![1; 20], 0, true),
        (vec![4, 5, 2, 9], 1, false),
        (vec![1; 20], 1024, true),
    ] {
        let (mut control, mut engine, handles) = setup_with_pages(journal, reversed, 256);
        let mut host = Host::new();
        let selected = control
            .media()
            .iter()
            .find(|m| m.target == id(1) && m.revision == Revision(1))
            .unwrap()
            .media;
        submit(
            &mut control,
            0,
            Operation::Select {
                target: id(1),
                timing: Timing::At,
                media: selected,
                preserve_position: false,
            },
        );
        transition(
            &mut control,
            1,
            1,
            Transition::Play {
                position: 0,
                retrigger: true,
            },
        );
        transition(&mut control, 1, 4, play_iteration(0));
        for (frame, value) in [(3, 3), (6, 1), (8, 3)] {
            transition(&mut control, frame, 4, iteration(value));
        }
        transition(&mut control, 8, 3, Transition::Stop);
        let pending = submit(
            &mut control,
            10,
            Operation::Transition {
                target: id(4),
                timing: Timing::AfterTicks {
                    source: id(1),
                    skip: 1,
                },
                transition: play_iteration(2),
            },
        );
        transition(&mut control, 11, 4, Transition::Stop);
        transition(
            &mut control,
            11,
            3,
            Transition::Play {
                position: 1,
                retrigger: true,
            },
        );
        transition(&mut control, 13, 4, play_iteration(1));
        transition(&mut control, 18, 4, iteration(0));
        for target in [1, 2, 4, 3] {
            transition(&mut control, 19, target, Transition::Stop);
        }
        let prefix = submit(
            &mut control,
            4,
            Operation::Export {
                target: id(3),
                selection: Selection::Recording,
            },
        );
        let first = submit(
            &mut control,
            5,
            Operation::Export {
                target: id(3),
                selection: Selection::Latest,
            },
        );
        let second = submit(
            &mut control,
            17,
            Operation::Export {
                target: id(3),
                selection: Selection::Latest,
            },
        );
        let won = submit(&mut control, 8, Operation::State { target: id(3) });
        let cut = submit(&mut control, 11, Operation::State { target: id(4) });
        let mut samples: [Vec<f32>; 4] = std::array::from_fn(|_| Vec::new());
        let mut midi: [Vec<(u64, Vec<u8>)>; 4] = std::array::from_fn(|_| Vec::new());
        for frames in parts {
            let at = engine.next_frame().0;
            if at == 4 {
                host.step(&mut engine, 0);
                assert_eq!(engine.next_frame(), Frame(4));
            }
            host.step(&mut engine, frames);
            for port in 0..4 {
                samples[port].extend_from_slice(&host.output[port][..frames]);
                for packet in 0..host.midi_out[port].len() {
                    let (offset, bytes) = host.midi_out[port].get(packet).unwrap();
                    midi[port].push((at + u64::from(offset), bytes.to_vec()));
                }
            }
        }
        let replies = drain(&mut control);
        let completion = replies.iter().find(|r| r.sequence == pending).unwrap();
        assert_eq!(
            (completion.delivered_frame, completion.actual_frame),
            (Some(Frame(10)), Frame(11))
        );
        let Some(Reply::Transition(completion)) = completion.operation_result else {
            panic!("countdown")
        };
        assert_eq!(completion.outcome, Outcome::Cancelled { frame: Frame(11) });
        let won = state_reply(&replies, won);
        assert_eq!(
            (
                won.state.mode,
                won.state.capture_revision,
                won.state.capture_frames
            ),
            (Mode::Stopped, Revision(1), 2)
        );
        let cut = state_reply(&replies, cut);
        assert_eq!(
            (
                cut.frame,
                cut.state.mode,
                cut.state.composite.unwrap().iteration
            ),
            (Frame(11), Mode::Stopped, 0)
        );
        let d = [
            1f32, 5., 1., 1., 1., 1.5, 1.5, 1., 1., 1., 1.5, 1.5, 1.5, 1.5, 1., 1., 1., 1., 1.,
        ];
        for f in 0..20 {
            let expected = [
                if f < 4 { (f + 1) as f32 } else { 0. },
                if (1..19).contains(&f) { 2. } else { 0. },
                if (1..19).contains(&f) {
                    if (f - 1) % 2 == 0 {
                        3.
                    } else {
                        6.
                    }
                } else {
                    0.
                },
                if f == 0 { 1. } else { d[f - 1] },
            ];
            for port in 0..4 {
                assert_eq!(samples[port][f], expected[port], "frame {f} port {port}");
            }
        }
        assert_eq!(midi, reference_midi());
        for (sequence, frames, revision) in [(prefix, 1, 1), (first, 2, 1), (second, 2, 2)] {
            let Some(Reply::Export {
                id: export,
                cut,
                media,
            }) = replies
                .iter()
                .find(|r| r.sequence == sequence)
                .unwrap()
                .operation_result
            else {
                panic!("export")
            };
            assert_eq!(
                (
                    cut.graph_epoch,
                    cut.topology_epoch,
                    media.view.description.owner,
                    media.view.description.frames,
                    media.view.description.revision
                ),
                (
                    rt_runtime::Epoch(1),
                    rt_runtime::Epoch(0),
                    id(3),
                    frames,
                    Revision(revision)
                )
            );
            assert_eq!(
                read(&mut control, &mut engine, &mut host, export, false),
                vec![1f32; frames as usize]
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                read(&mut control, &mut engine, &mut host, export, true),
                if revision == 1 {
                    encoded(&[(0, vec![0x80, 63, 0])])
                } else {
                    vec![]
                }
            );
            let seq = submit(
                &mut control,
                engine.next_frame().0,
                Operation::MidiInitial { export, channel: 0 },
            );
            host.step(&mut engine, 1);
            let queried = drain(&mut control);
            let Some(Reply::MidiInitial {
                state,
                system_reset_seen,
                ..
            }) = queried
                .iter()
                .find(|r| r.sequence == seq)
                .unwrap()
                .operation_result
            else {
                panic!("initial")
            };
            assert!(!system_reset_seen);
            assert_eq!(
                (
                    state.program(),
                    state.program_bank(),
                    state.pitch_bend(),
                    state.pressure(),
                    state.reset_controllers_seen(),
                    state.parameter_family(),
                    state.parameter_history_untracked()
                ),
                (None, [None, None], 8192, None, false, None, false)
            );
            for key in 0..128 {
                assert_eq!(
                    state.controller(key),
                    if [64, 66, 69].contains(&key) {
                        Some(0)
                    } else {
                        None
                    }
                );
                let note = state.note(key).unwrap();
                let held = revision == 1 && key == 63;
                assert_eq!(
                    (
                        note.velocity(),
                        note.held(),
                        note.sostenuto(),
                        note.pressure()
                    ),
                    (if held { 70 } else { 0 }, held, false, None)
                );
            }
        }
        drop(control);
        let mut stopped = engine.into_stopped();
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            for resource in &handles {
                stopped.resources().remove(*resource).unwrap();
            }
            for _ in 0..256 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.resource_usage().free_pages, 256);
            assert_eq!(stopped.resource_usage().free_versions, 64);
            assert_eq!(stopped.resource_usage().free_snapshots, 64);
            assert_eq!(stopped.media_usage().references, 0);
        });
        let log = stopped.observer().as_any().downcast_ref::<Log>().unwrap();
        let complete = reference_events();
        let mut expected = Vec::new();
        for frame in 0..20 {
            expected.extend(
                complete
                    .iter()
                    .filter(|e| e.frame == Frame(frame))
                    .take(journal)
                    .copied(),
            );
        }
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (complete.len() - expected.len()) as u64);
        for frame in 0..20 {
            for state in log.states[frame] {
                assert_state(frame, state.expect("every loop has a boundary decision"));
            }
        }
    }
}
