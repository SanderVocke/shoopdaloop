use super::*;
#[path = "program_graph_capture.rs"]
mod capture;
#[path = "program_graph_lifecycle.rs"]
mod lifecycle;

fn paired(id: StageId, graph: GraphStageId) -> Operation {
    Operation::ProgramGraphActivate {
        id,
        target: self::id(2),
        graph,
        graph_epoch: Epoch(1),
        topology_epoch: Epoch(0),
    }
}
#[shoop_test]
fn running_program_and_routes_publish_at_one_zero_cut_with_full_independent_reference() {
    for (parts, journal, reverse) in [
        (vec![12], 1024, false),
        (vec![1; 12], 0, true),
        (vec![4, 2, 6], 1, false),
        (vec![4, 8], 1024, true),
    ] {
        let mut h = live(true, journal, reverse);
        let topology = h.ready(
            0,
            &compile(limits(journal), &description(true, reverse)).unwrap(),
        );
        let graph = h.graph(0, 1, true, reverse);
        h.op(3, play(0));
        h.op(3, play(2));
        let state = h.op(7, Operation::State { target: id(2) });
        let export = h.op(
            7,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        let change = h.op(4, paired(topology, graph));
        h.op(14, stop(0));
        h.op(14, stop(2));
        for frames in parts {
            if h.engine.next_frame() == Frame(7) {
                h.step(0);
                assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
                h.drain();
                assert!(!h.replies.iter().any(|r| r.sequence == change));
            }
            h.step(frames);
        }
        h.drain();
        let Reply::ProgramGraphChanged {
            id: actual,
            graph: actual_graph,
            graph_epoch,
            topology_epoch,
            completion,
        } = h.reply(change)
        else {
            panic!("paired result")
        };
        assert_eq!(
            (actual, actual_graph, graph_epoch, topology_epoch),
            (topology, graph, Epoch(2), Epoch(1))
        );
        assert_eq!(
            (
                completion.sequence,
                completion.requested,
                completion.effective,
                completion.outcome
            ),
            (
                change,
                Frame(4),
                Frame(4),
                Outcome::Applied { frame: Frame(7) }
            )
        );
        let execution = h.replies.iter().find(|r| r.sequence == change).unwrap();
        assert_eq!(
            (execution.delivered_frame, execution.actual_frame),
            (Some(Frame(4)), Frame(7))
        );
        let Reply::State(cut) = h.reply(state) else {
            panic!("state")
        };
        assert_eq!(
            (
                cut.frame,
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.content_length,
                cut.state.composite.unwrap().iteration,
                cut.state.composite.unwrap().passes
            ),
            (Frame(7), Epoch(2), Epoch(1), 3, 0, 1)
        );
        let expected: Vec<_> = [0f32, 0., 0., 2., 4., 6., 8., 0., 4., 8., 0., 4., 8., 0., 0.]
            .into_iter()
            .map(|v| [v, v * 10.])
            .collect();
        assert_eq!(h.samples, expected);
        let mut packets = Vec::new();
        restore(3, &mut packets);
        packets.push((5, vec![0x80, 60, 45]));
        restore(8, &mut packets);
        packets.push((10, vec![0x80, 60, 0]));
        restore(11, &mut packets);
        packets.push((13, vec![0x80, 60, 0]));
        assert_eq!(h.packets, packets);
        let Reply::Export {
            id: export,
            cut,
            media,
        } = h.reply(export)
        else {
            panic!("export")
        };
        assert_eq!(
            (
                cut.graph_epoch,
                cut.topology_epoch,
                media.view.description.frames,
                media.view.description.revision
            ),
            (Epoch(2), Epoch(1), 4, Revision(0))
        );
        let bytes: Vec<_> = [4f32, 8., 12., 16.]
            .into_iter()
            .flat_map(|v| [v, v * 10.])
            .flat_map(f32::to_le_bytes)
            .collect();
        assert_eq!(read_export(&mut h, export, false), bytes);
        assert_eq!(read_export(&mut h, export, true), encoded_midi());
        let at = h.engine.next_frame().0;
        h.op(at, Operation::Release { export });
        h.op(at, Operation::TopologyRecycle { id: topology });
        assert_no_heap(|| h.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        h.step(1);
        h.drain();
        let log = h.finish().unwrap();
        let mut expected_events = Vec::new();
        let mut dropped = 0;
        for frame in 3..15 {
            use EventKind::{Iteration, Pulse, Transition as Changed, Wrap};
            let mut events = Vec::new();
            let mut push = |target, kind| {
                events.push(Event {
                    frame: Frame(frame),
                    target: id(target),
                    kind,
                })
            };
            if frame == 7 {
                push(2, Changed(Transition::Program));
            }
            if frame == 14 {
                push(0, Changed(Transition::Stop));
                push(2, Changed(Transition::Stop));
            } else {
                push(
                    0,
                    if frame == 3 {
                        Changed(Transition::Play {
                            position: 0,
                            retrigger: true,
                        })
                    } else {
                        Wrap
                    },
                );
                push(0, Pulse);
                if frame == 3 {
                    push(
                        2,
                        Changed(Transition::Play {
                            position: 0,
                            retrigger: true,
                        }),
                    );
                }
                push(2, Pulse);
                push(
                    2,
                    Iteration {
                        iteration: if frame < 7 {
                            frame - 3
                        } else {
                            (frame - 7) % 3
                        },
                        passes: if frame < 7 { 0 } else { 1 + (frame - 7) / 3 },
                    },
                );
            }
            if [3, 8, 11].contains(&frame) {
                push(
                    1,
                    Changed(Transition::Play {
                        position: 0,
                        retrigger: true,
                    }),
                );
            }
            if [7, 10, 13].contains(&frame) {
                push(1, Changed(Transition::Stop));
            }
            if ![7, 10, 13, 14].contains(&frame) {
                push(1, Pulse);
            }
            dropped += events.len().saturating_sub(journal) as u64;
            expected_events.extend(events.into_iter().take(journal));
        }
        assert_eq!(log.events, expected_events);
        assert_eq!(log.dropped, dropped);
        for f in 0usize..15 {
            for slot in 0..3 {
                let state = log.states[f][slot].unwrap();
                assert_eq!(state.id, id(slot as u32));
                assert_eq!(
                    (
                        state.content_revision,
                        state.capture_revision,
                        state.capture_frames,
                        state.capture_incomplete,
                        state.monitor_previous
                    ),
                    (Revision(0), Revision(0), 0, false, false)
                );
                let (mode, length, position, cycles, last, composite) = match slot {
                    0 => (
                        if (3..14).contains(&f) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        },
                        1,
                        0,
                        f.saturating_sub(3).min(10) as u64,
                        (f >= 3).then_some(Frame(f.min(13) as u64)),
                        None,
                    ),
                    1 => {
                        let start = if (3..7).contains(&f) {
                            Some(3)
                        } else if (8..10).contains(&f) {
                            Some(8)
                        } else if (11..13).contains(&f) {
                            Some(11)
                        } else {
                            None
                        };
                        let last = match f {
                            0..=2 => None,
                            7 => Some(Frame(6)),
                            10 => Some(Frame(9)),
                            13 | 14 => Some(Frame(12)),
                            _ => Some(Frame(f as u64)),
                        };
                        (
                            if start.is_some() {
                                Mode::Playing
                            } else {
                                Mode::Stopped
                            },
                            4,
                            start.map_or(0, |s| (f - s) as u64),
                            0,
                            last,
                            None,
                        )
                    }
                    _ => (
                        if (3..14).contains(&f) {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        },
                        if f < 7 { 4 } else { 3 },
                        0,
                        0,
                        (f >= 3).then_some(Frame(f.min(13) as u64)),
                        Some(state::CompositeState {
                            iteration: if f < 3 || f == 14 {
                                0
                            } else if f < 7 {
                                (f - 3) as u64
                            } else {
                                ((f - 7) % 3) as u64
                            },
                            passes: if f < 7 {
                                0
                            } else {
                                (1 + (f.min(13) - 7) / 3) as u64
                            },
                            entered_at: (f >= 3).then_some(Frame(f as u64)),
                        }),
                    ),
                };
                assert_eq!(
                    (
                        state.mode,
                        state.content_length,
                        state.position,
                        state.cycles,
                        state.last_pulse,
                        state.composite
                    ),
                    (mode, length, position, cycles, last, composite),
                    "frame {f} slot {slot}"
                );
            }
        }
    }
}
