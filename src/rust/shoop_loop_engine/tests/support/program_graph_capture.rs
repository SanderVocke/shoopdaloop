use super::*;

#[shoop_test]
fn paired_program_capture_preflight_publishes_both_or_neither_and_records_real_fx_media() {
    for refused in [false, true] {
        for parts in [vec![13], vec![1; 13], vec![4, 3, 6]] {
            let mut h = Harness::options(
                true,
                1024,
                false,
                staging(),
                if refused { 2 } else { 8 },
                None,
            );
            h.engine.prepare_graph_staging(2).unwrap();
            let mut changed = description(false, false);
            changed.programs[0].sections[0].entries = (0..4)
                .map(|n| Entry {
                    target: id(1),
                    delay: 2 * n,
                    duration: Some(2),
                    repetitions: 1,
                    mode: Some(if n % 2 == 0 {
                        Mode::Recording
                    } else {
                        Mode::Playing
                    }),
                })
                .collect();
            let topology = h.ready(0, &compile(limits(1024), &changed).unwrap());
            let mut routes = routes(true, false, false);
            routes.connections.retain(|c| {
                !(c.destination.node.is_none() && c.destination.kind == PortType::Audio)
            });
            for channel in 0..2 {
                routes.connections.push(Connection {
                    source: Endpoint::node(node(2), PortType::Audio, channel),
                    destination: Endpoint::host(PortType::Audio, channel),
                });
            }
            let plan = h.control.compile_graph(&routes).unwrap();
            let graph = h.graph_plan(0, 1, plan);
            h.op(3, play(0));
            h.op(3, play(2));
            let change = h.op(4, paired(topology, graph));
            for (frame, parameter, value) in [
                (7, u32::MAX - 1, 6.),
                (7, u32::MAX, 2.),
                (15, u32::MAX - 1, 0.),
            ] {
                assert_no_heap(|| h.control.parameter(Frame(frame), node(4), parameter, value))
                    .unwrap_or_else(|e| panic!("{:?}", e.error));
            }
            let state = h.op(7, Operation::State { target: id(1) });
            let prefix = h.op(
                8,
                Operation::Export {
                    target: id(1),
                    selection: Selection::Recording,
                },
            );
            let first = h.op(
                9,
                Operation::Export {
                    target: id(1),
                    selection: if refused {
                        Selection::Playback
                    } else {
                        Selection::Latest
                    },
                },
            );
            let second = h.op(
                13,
                Operation::Export {
                    target: id(1),
                    selection: if refused {
                        Selection::Playback
                    } else {
                        Selection::Latest
                    },
                },
            );
            h.op(15, stop(0));
            h.op(15, stop(2));
            for frames in parts {
                h.step(frames);
            }
            h.drain();
            let Reply::ProgramGraphChanged {
                graph_epoch,
                topology_epoch,
                completion,
                ..
            } = h.reply(change)
            else {
                panic!("paired receipt")
            };
            assert_eq!(
                (graph_epoch, topology_epoch),
                (
                    Epoch(if refused { 1 } else { 2 }),
                    Epoch(u64::from(!refused))
                )
            );
            assert_eq!(
                completion.outcome,
                if refused {
                    Outcome::Failed {
                        frame: Frame(7),
                        reason: shoop_loop_engine::Error::Capacity,
                    }
                } else {
                    Outcome::Applied { frame: Frame(7) }
                }
            );
            let Reply::State(cut) = h.reply(state) else {
                panic!("state")
            };
            assert_eq!(
                (
                    cut.graph_epoch,
                    cut.topology_epoch,
                    cut.state.mode,
                    cut.state.capture_revision,
                    cut.state.capture_frames,
                    cut.capture_failure
                ),
                (
                    graph_epoch,
                    topology_epoch,
                    if refused {
                        Mode::Playing
                    } else {
                        Mode::Recording
                    },
                    Revision(u64::from(!refused)),
                    0,
                    refused.then_some(Error::Capacity)
                )
            );
            let values = if refused {
                vec![
                    0f32, 0., 0., 2., 4., 6., 8., 8., 10., 12., 14., 8., 10., 12., 14., 0.,
                ]
            } else {
                vec![
                    0f32, 0., 0., 2., 4., 6., 8., 0., 0., 6., 6., 0., 0., 6., 6., 0.,
                ]
            };
            assert_eq!(
                h.samples,
                values.into_iter().map(|v| [v, v * 10.]).collect::<Vec<_>>()
            );
            let mut packets = Vec::new();
            restore(3, &mut packets);
            packets.push((5, vec![0x80, 60, 45]));
            if refused {
                restore(7, &mut packets);
                packets.push((7, vec![0x92, 63, 55]));
                packets.push((9, vec![0x80, 60, 45]));
                restore(11, &mut packets);
                packets.push((13, vec![0x80, 60, 45]));
            } else {
                packets.push((7, vec![0x92, 63, 55]));
                restore(9, &mut packets);
                packets.pop();
                packets.push((9, vec![0x92, 63, 55]));
                packets.push((11, vec![0x82, 63, 0]));
                for channel in 0..16 {
                    for packet in [
                        [0xe0 | channel, 0, 64],
                        [0xb0 | channel, 64, 0],
                        [0xb0 | channel, 69, 0],
                        [0xb0 | channel, 66, 0],
                    ] {
                        packets.push((13, packet.to_vec()));
                    }
                    if channel == 2 {
                        packets.push((13, vec![0x92, 63, 55]));
                    }
                }
                packets.push((13, vec![0x82, 63, 0]));
            }
            assert_eq!(h.packets, packets);
            if refused {
                assert_eq!(
                    h.reply(prefix),
                    Reply::Failed(Failure::Runtime(Error::InvalidState))
                );
            }
            for (sequence, frames, revision, event) in [
                (prefix, 1, 1, [0x92, 63, 55]),
                (first, 2, 1, [0x92, 63, 55]),
                (second, 2, 2, [0x82, 63, 0]),
            ] {
                if refused && sequence == prefix {
                    continue;
                }
                let Reply::Export {
                    id: export,
                    cut,
                    media,
                } = h.reply(sequence)
                else {
                    panic!("export")
                };
                assert_eq!(
                    (cut.graph_epoch, cut.topology_epoch),
                    (graph_epoch, topology_epoch)
                );
                let values = if refused {
                    vec![4f32, 8., 12., 16.]
                } else {
                    vec![6f32; frames]
                };
                let bytes: Vec<_> = values
                    .into_iter()
                    .flat_map(|v| [v, v * 10.])
                    .flat_map(f32::to_le_bytes)
                    .collect();
                assert_eq!(read_export(&mut h, export, false), bytes);
                assert_eq!(
                    read_export(&mut h, export, true),
                    if refused {
                        encoded_midi()
                    } else {
                        encoded_packets(&[(0, event.to_vec())])
                    }
                );
                assert_eq!(
                    (
                        media.view.description.frames,
                        media.view.description.revision
                    ),
                    (
                        if refused { 4 } else { frames as u64 },
                        Revision(if refused { 0 } else { revision })
                    )
                );
                for channel in 0..16 {
                    let seq = h.op(
                        h.engine.next_frame().0,
                        Operation::MidiInitial { export, channel },
                    );
                    h.step(1);
                    h.drain();
                    let Reply::MidiInitial {
                        state,
                        system_reset_seen,
                        ..
                    } = h.reply(seq)
                    else {
                        panic!("initial")
                    };
                    assert!(!system_reset_seen);
                    for key in 0..128 {
                        let note = state.note(key).unwrap();
                        let held = !refused && revision == 2 && channel == 2 && key == 63;
                        assert_eq!(
                            (
                                note.velocity(),
                                note.held(),
                                note.sostenuto(),
                                note.pressure()
                            ),
                            (if held { 55 } else { 0 }, held, false, None)
                        );
                    }
                }
            }
            let at = h.engine.next_frame().0;
            let gs = h.op(at, Operation::GraphStatus { id: graph });
            let ts = h.op(at, Operation::TopologyStatus { id: topology });
            h.step(1);
            h.drain();
            assert_eq!(
                h.reply(gs),
                Reply::GraphStatus {
                    id: graph,
                    status: if refused {
                        GraphStageStatus::Ready {
                            validated_epoch: Epoch(1),
                        }
                    } else {
                        GraphStageStatus::Taken
                    }
                }
            );
            assert_eq!(
                h.reply(ts),
                Reply::TopologyStatus {
                    id: topology,
                    status: if refused {
                        Status::Ready
                    } else {
                        Status::Taken
                    }
                }
            );
            let log = h.finish().unwrap();
            let program: Vec<_> = log
                .events
                .iter()
                .filter(|e| e.kind == EventKind::Transition(Transition::Program))
                .copied()
                .collect();
            assert_eq!(
                program,
                if refused {
                    vec![]
                } else {
                    vec![Event {
                        frame: Frame(7),
                        target: id(2),
                        kind: EventKind::Transition(Transition::Program),
                    }]
                }
            );
        }
    }
}
