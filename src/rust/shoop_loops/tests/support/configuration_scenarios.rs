use super::*;
#[path = "configuration_input.rs"]
mod input;
#[path = "configuration_rejections.rs"]
mod rejections;
#[path = "configuration_sources.rs"]
mod sources;
#[path = "configuration_teardown.rs"]
mod teardown;

fn prepared(media: bool, journal: usize, reverse: bool, packets: usize, spare: bool) -> Harness {
    let mut h = Harness::with_roles(
        media,
        journal,
        reverse,
        staging(),
        8,
        None,
        RoleSettings {
            candidates: 2,
            sources: 8,
            source_packets_per_callback: packets,
        },
        spare,
    );
    h.engine.prepare_graph_staging(2).unwrap();
    h
}
fn generation(slot: u32, number: u32) -> LoopId {
    LoopId::from_parts(slot, number).unwrap()
}
fn changed_description(number: u32, reverse: bool) -> TopologyDescription {
    let mut description = description(false, reverse);
    description
        .loops
        .iter_mut()
        .find(|n| n.id == id(1))
        .unwrap()
        .id = generation(1, number);
    description.programs[0].sections[0].entries[0].target = generation(1, number);
    description
}
fn begin(h: &mut Harness, slot: usize, count: usize) -> RoleStageId {
    let sequence = h.op(
        h.engine.next_frame().0,
        Operation::RoleBegin {
            slot,
            changes: count,
        },
    );
    h.step(1);
    h.drain();
    let Reply::RoleStatus { id, status } = h.reply(sequence) else {
        panic!("role begin")
    };
    assert_eq!(
        status,
        if count == 0 {
            RoleStatus::Ready
        } else {
            RoleStatus::Receiving {
                received: 0,
                expected: count,
            }
        }
    );
    id
}
fn source_pair(h: &mut Harness) -> [RoleSourceId; 2] {
    let at = h.engine.next_frame().0;
    let audio = h.op(
        at,
        Operation::RoleSnapshot {
            resource: h.imports[0],
        },
    );
    let midi = h.op(
        at,
        Operation::RoleSnapshot {
            resource: h.imports[1],
        },
    );
    h.step(1);
    h.drain();
    let Reply::RoleSource {
        id: audio,
        snapshot,
        status,
    } = h.reply(audio)
    else {
        panic!("audio source")
    };
    assert_eq!(
        (snapshot.description.frames, status),
        (4, RoleSourceStatus::Ready { max_packet: 0 })
    );
    let Reply::RoleSource {
        id: midi,
        snapshot,
        status,
    } = h.reply(midi)
    else {
        panic!("MIDI source")
    };
    assert_eq!(
        (snapshot.description.frames, status),
        (4, RoleSourceStatus::Validating { packets: 0 })
    );
    loop {
        let seq = h.op(
            h.engine.next_frame().0,
            Operation::RoleSourceValidate { id: midi },
        );
        h.step(1);
        h.drain();
        match h.reply(seq) {
            Reply::RoleSourceStatus {
                status: RoleSourceStatus::Ready { max_packet: 3 },
                ..
            } => break,
            Reply::RoleSourceStatus {
                status: RoleSourceStatus::Validating { .. },
                ..
            } => {}
            other => panic!("{other:?}"),
        }
    }
    [audio, midi]
}
fn transition(target: LoopId, transition: Transition) -> Operation {
    Operation::Transition {
        target,
        timing: Timing::At,
        transition,
    }
}
fn play_new(target: LoopId) -> Operation {
    transition(
        target,
        Transition::Play {
            position: 0,
            retrigger: true,
        },
    )
}
fn configuration(
    roles: RoleStageId,
    topology: StageId,
    graph: GraphStageId,
    graph_epoch: u64,
    topology_epoch: u64,
) -> Operation {
    Operation::ConfigurationActivate {
        roles,
        topology,
        graph,
        graph_epoch: Epoch(graph_epoch),
        topology_epoch: Epoch(topology_epoch),
    }
}
fn initial(target: LoopId, sources: [RoleSourceId; 2], preserve_input: bool) -> RoleInitial {
    RoleInitial {
        target,
        frames: 4,
        audio: Some(sources[0]),
        midi: Some(sources[1]),
        gain: 1.,
        preserve_input,
    }
}

#[shoop_test]
fn generation_graph_media_capture_and_old_exports_share_one_cut_and_independent_reference() {
    for (parts, journal, reverse) in [
        (vec![11], 1024, false),
        (vec![1; 11], 0, true),
        (vec![3, 2, 6], 1, false),
    ] {
        let mut h = prepared(true, journal, reverse, 64, false);
        let sources = source_pair(&mut h);
        let roles = begin(&mut h, 0, 1);
        let next = generation(1, 2);
        let appended = h.op(
            3,
            Operation::RoleAppend {
                id: roles,
                previous: Some(id(1)),
                initial: Some(initial(next, sources, true)),
            },
        );
        h.step(1);
        h.drain();
        assert_eq!(
            h.reply(appended),
            Reply::RoleStatus {
                id: roles,
                status: RoleStatus::Ready
            }
        );
        let topology = h.ready(
            0,
            &compile(limits(journal), &changed_description(2, reverse)).unwrap(),
        );
        let graph = h.graph(0, 1, true, reverse);
        assert_eq!(h.engine.next_frame(), Frame(7));
        let old_media = h
            .control
            .media()
            .iter()
            .find(|m| m.target == id(1))
            .unwrap()
            .media;
        h.op(7, play(0));
        h.op(
            7,
            transition(
                id(1),
                Transition::Record {
                    monitor_previous: true,
                },
            ),
        );
        let old_export = h.op(
            7,
            Operation::Export {
                target: id(1),
                selection: Selection::Playback,
            },
        );
        let refused = h.op(8, configuration(roles, topology, graph, 1, 0));
        h.op(9, stop(1));
        let pending = h.op(
            9,
            Operation::Select {
                target: id(1),
                media: old_media,
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 1000,
                },
                preserve_position: false,
            },
        );
        let old_take = h.op(
            9,
            Operation::Export {
                target: id(1),
                selection: Selection::Latest,
            },
        );
        let applied = h.op(10, configuration(roles, topology, graph, 1, 0));
        h.op(10, play_new(next));
        let current = h.op(10, Operation::State { target: next });
        let stale = h.op(10, Operation::State { target: id(1) });
        let stale_source = h.op(10, Operation::RoleSourceRelease { id: sources[0] });
        h.op(11, Operation::Retire { media: old_media });
        h.op(
            12,
            transition(
                next,
                Transition::Record {
                    monitor_previous: true,
                },
            ),
        );
        let prefix = h.op(
            13,
            Operation::Export {
                target: next,
                selection: Selection::Recording,
            },
        );
        h.op(14, play_new(next));
        let full = h.op(
            14,
            Operation::Export {
                target: next,
                selection: Selection::Latest,
            },
        );
        h.op(17, stop(0));
        h.op(17, transition(next, Transition::Stop));
        for n in parts {
            if h.engine.next_frame() == Frame(10) {
                h.step(0);
                assert_eq!(h.engine.diagnostics().epoch, Epoch(1));
            }
            h.step(n);
        }
        h.drain();
        assert_eq!(
            h.reply(refused),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
        );
        assert_eq!(
            h.reply(applied),
            Reply::ConfigurationActivated {
                roles,
                topology,
                graph,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(1)
            }
        );
        assert_eq!(
            h.reply(stale),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
        );
        assert_eq!(
            h.reply(stale_source),
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
        let Reply::Transition(completion) = h.reply(pending) else {
            panic!("pending result")
        };
        assert_eq!(
            completion.outcome,
            Outcome::Failed {
                frame: Frame(10),
                reason: shoop_loops::Error::InvalidId
            }
        );
        let Reply::State(cut) = h.reply(current) else {
            panic!("current cut")
        };
        assert_eq!(
            (
                cut.frame,
                cut.graph_epoch,
                cut.topology_epoch,
                cut.state.id,
                cut.state.content_length,
                cut.state.capture_revision
            ),
            (Frame(10), Epoch(2), Epoch(1), next, 4, Revision(0))
        );
        let new_initial = cut.playback.unwrap();
        assert_eq!(cut.latest, None);
        let expected: Vec<_> = [
            0f32, 0., 0., 0., 0., 0., 0., 2., 4., 0., 4., 8., 12., 16., 12., 16., 12., 0.,
        ]
        .into_iter()
        .map(|v| [v, v * 10.])
        .collect();
        assert_eq!(h.samples, expected);
        let mut midi = Vec::new();
        restore(7, &mut midi);
        midi.push((9, vec![0x80, 60, 0]));
        restore(10, &mut midi);
        midi.push((12, vec![0x80, 60, 45]));
        for f in [14, 16] {
            for channel in 0..16 {
                for packet in [
                    [0xe0 | channel, 0, 64],
                    [0xb0 | channel, 64, 0],
                    [0xb0 | channel, 69, 0],
                    [0xb0 | channel, 66, 0],
                ] {
                    midi.push((f, packet.to_vec()));
                }
                if channel == 0 {
                    midi.push((f, vec![0x90, 60, 100]));
                }
            }
            midi.push((f, vec![0x80, 60, 45]));
        }
        assert_eq!(h.packets, midi);
        let mut captured_old = Vec::new();
        restore(0, &mut captured_old);
        let expected_captured = encoded_packets(&[(0, vec![0x80, 60, 45])]);
        let mut exports = Vec::new();
        for (sequence, owner, revision, frames, cut_frame, values, packets) in [
            (
                old_export,
                id(1),
                0,
                4,
                7,
                vec![4f32, 8., 12., 16.],
                encoded_midi(),
            ),
            (
                old_take,
                id(1),
                1,
                2,
                9,
                vec![2., 4.],
                encoded_packets(&captured_old),
            ),
            (prefix, next, 1, 1, 13, vec![12.], expected_captured.clone()),
            (
                full,
                next,
                1,
                2,
                14,
                vec![12., 16.],
                expected_captured.clone(),
            ),
        ] {
            let Reply::Export {
                id: export,
                cut,
                media,
            } = h.reply(sequence)
            else {
                panic!("export")
            };
            exports.push(export);
            assert_eq!(
                (
                    cut.frame,
                    media.view.description.owner,
                    media.view.description.revision,
                    media.view.description.frames
                ),
                (Frame(cut_frame), owner, Revision(revision), frames)
            );
            assert_eq!(
                (cut.graph_epoch, cut.topology_epoch),
                if owner == id(1) {
                    (Epoch(1), Epoch(0))
                } else {
                    (Epoch(2), Epoch(1))
                }
            );
            let expected: Vec<_> = values
                .into_iter()
                .flat_map(|v| [v, v * 10.])
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(h.read_export(export, false, expected.len()), expected);
            assert_eq!(h.read_export(export, true, packets.len()), packets);
        }
        let at = h.engine.next_frame().0;
        for export in exports {
            h.op(at, Operation::Release { export });
        }
        h.op(at, Operation::Retire { media: new_initial });
        h.op(at, Operation::RoleRecycle { id: roles });
        h.op(at, Operation::TopologyRecycle { id: topology });
        assert_no_heap(|| h.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        h.step(1);
        h.drain();
        let log = h.finish().unwrap();
        for frame in 0usize..18 {
            for slot in 0..3 {
                let state = log.states[frame][slot].unwrap();
                let live = frame >= 7;
                let stopped = frame == 17;
                let (
                    who,
                    mode,
                    length,
                    revision,
                    position,
                    cycles,
                    capture,
                    capture_revision,
                    monitor,
                    last,
                    composite,
                ) = match slot {
                    0 => (
                        id(0),
                        if live && !stopped {
                            Mode::Playing
                        } else {
                            Mode::Stopped
                        },
                        1,
                        0,
                        0,
                        if live { (frame.min(16) - 7) as u64 } else { 0 },
                        0,
                        0,
                        false,
                        live.then_some(Frame(frame.min(16) as u64)),
                        None,
                    ),
                    2 => (
                        id(2),
                        Mode::Stopped,
                        4,
                        0,
                        0,
                        0,
                        0,
                        0,
                        false,
                        live.then_some(Frame(frame.min(16) as u64)),
                        Some(CompositeState::default()),
                    ),
                    _ => {
                        let mode = match frame {
                            7 | 8 | 12 | 13 => Mode::Recording,
                            10 | 11 | 14..=16 => Mode::Playing,
                            _ => Mode::Stopped,
                        };
                        let length = if frame == 9 || frame >= 14 { 2 } else { 4 };
                        let revision = u64::from(frame == 9 || frame >= 14);
                        let position = match frame {
                            8 | 11 | 15 => 1,
                            12 => 2,
                            13 => 3,
                            _ => 0,
                        };
                        let capture = match frame {
                            8 | 13 => 1,
                            9 | 14.. => 2,
                            _ => 0,
                        };
                        let cr = u64::from(matches!(frame,7..=9|12..));
                        let last = match frame {
                            0..=6 => None,
                            9 => Some(Frame(8)),
                            17 => Some(Frame(16)),
                            _ => Some(Frame(frame as u64)),
                        };
                        (
                            if frame < 10 { id(1) } else { next },
                            mode,
                            length,
                            revision,
                            position,
                            u64::from(frame >= 16),
                            capture,
                            cr,
                            matches!(frame, 7 | 8 | 12 | 13),
                            last,
                            None,
                        )
                    }
                };
                assert_eq!(
                    (
                        state.id,
                        state.mode,
                        state.content_length,
                        state.content_revision,
                        state.position,
                        state.cycles
                    ),
                    (who, mode, length, Revision(revision), position, cycles),
                    "frame {frame} slot {slot}"
                );
                assert_eq!(
                    (
                        state.capture_frames,
                        state.capture_revision,
                        state.capture_incomplete,
                        state.monitor_previous,
                        state.last_pulse,
                        state.composite
                    ),
                    (
                        capture,
                        Revision(capture_revision),
                        false,
                        monitor,
                        last,
                        composite
                    ),
                    "frame {frame} slot {slot}"
                );
            }
        }
        let mut events = Vec::new();
        let mut dropped = 0u64;
        for frame in 7..18 {
            use decision::EventKind::{Pulse, Transition as Changed, Wrap};
            let mut boundary = Vec::new();
            let mut push = |target, kind| {
                boundary.push(Event {
                    frame: Frame(frame),
                    target,
                    kind,
                })
            };
            if frame == 17 {
                push(id(0), Changed(Transition::Stop));
            } else {
                push(
                    id(0),
                    if frame == 7 {
                        Changed(Transition::Play {
                            position: 0,
                            retrigger: true,
                        })
                    } else {
                        Wrap
                    },
                );
                push(id(0), Pulse);
                push(id(2), Pulse);
            }
            let target = if frame < 10 { id(1) } else { next };
            match frame {
                7 | 12 => push(
                    target,
                    Changed(Transition::Record {
                        monitor_previous: true,
                    }),
                ),
                9 | 17 => push(target, Changed(Transition::Stop)),
                10 | 14 => push(
                    target,
                    Changed(Transition::Play {
                        position: 0,
                        retrigger: true,
                    }),
                ),
                16 => push(target, Wrap),
                _ => {}
            }
            if frame != 9 && frame != 17 {
                push(target, Pulse);
            }
            dropped += boundary.len().saturating_sub(journal) as u64;
            events.extend(boundary.into_iter().take(journal));
        }
        assert_eq!(log.events, events);
        assert_eq!(log.dropped, dropped);
    }
}

fn timing_initial(target: LoopId, frames: u64) -> RoleInitial {
    RoleInitial {
        target,
        frames,
        audio: None,
        midi: None,
        gain: 1.,
        preserve_input: false,
    }
}
fn spare_description(generation: Option<u32>, reverse: bool) -> TopologyDescription {
    let mut candidate = description(false, reverse);
    candidate.programs.clear();
    if let Some(number) = generation {
        let spec = candidate.loops.iter_mut().find(|n| n.id == id(2)).unwrap();
        spec.id = self::generation(2, number);
        spec.kind = LoopKind::Primitive;
        spec.length = 3;
    } else {
        candidate.loops.retain(|n| n.id != id(2));
    }
    candidate
}
fn spare_graph(h: &mut Harness, epoch: u64, reverse: bool) -> GraphStageId {
    let mut graph = routes(false, true, reverse);
    graph.nodes.extend([node(5), node(6)]);
    if reverse {
        graph.nodes.reverse();
    }
    let plan = h.control.compile_graph(&graph).unwrap();
    h.graph_plan(0, epoch, plan)
}
#[shoop_test]
fn inactive_prepared_consumers_support_kind_change_removal_and_readd_without_retargeting_old_requests(
) {
    for reverse in [false, true] {
        let mut h = prepared(false, 0, reverse, 4, true);
        let second = generation(2, 2);
        let third = generation(2, 3);
        let roles = begin(&mut h, 0, 1);
        let appended = h.op(
            1,
            Operation::RoleAppend {
                id: roles,
                previous: None,
                initial: Some(timing_initial(second, 3)),
            },
        );
        h.step(1);
        h.drain();
        assert_eq!(
            h.reply(appended),
            Reply::RoleStatus {
                id: roles,
                status: RoleStatus::Ready
            }
        );
        let topology = h.ready(
            0,
            &compile(limits(0), &spare_description(Some(2), reverse)).unwrap(),
        );
        let graph = spare_graph(&mut h, 1, reverse);
        assert_eq!(h.engine.next_frame(), Frame(5));
        let applied = h.op(5, configuration(roles, topology, graph, 1, 0));
        h.op(5, play(0));
        h.op(5, play_new(second));
        let first = h.op(5, Operation::State { target: second });
        let wrap = h.op(8, Operation::State { target: second });
        if reverse {
            for _ in 0..5 {
                h.step(1);
            }
        } else {
            h.step(5);
        }
        h.drain();
        assert_eq!(
            h.reply(applied),
            Reply::ConfigurationActivated {
                roles,
                topology,
                graph,
                graph_epoch: Epoch(2),
                topology_epoch: Epoch(1)
            }
        );
        let Reply::State(first) = h.reply(first) else {
            panic!("first")
        };
        let initial_media = first.playback.unwrap();
        assert_eq!(
            (
                first.state.id,
                first.state.position,
                first.state.content_length,
                first.state.mode,
                first.state.composite
            ),
            (second, 0, 3, Mode::Playing, None)
        );
        let Reply::State(wrap) = h.reply(wrap) else {
            panic!("wrap")
        };
        assert_eq!(
            (
                wrap.state.position,
                wrap.state.cycles,
                wrap.state.last_pulse
            ),
            (0, 1, Some(Frame(8)))
        );
        h.op(10, transition(second, Transition::Stop));
        let pending = h.op(
            10,
            Operation::Transition {
                target: second,
                timing: Timing::AfterTicks {
                    source: id(0),
                    skip: 1000,
                },
                transition: Transition::Stop,
            },
        );
        h.op(10, Operation::RoleRecycle { id: roles });
        h.op(10, Operation::TopologyRecycle { id: topology });
        assert_no_heap(|| h.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        h.step(1);
        h.drain();
        let removed = begin(&mut h, 0, 1);
        assert_eq!(removed.parts()[2], roles.parts()[2] + 1);
        h.op(
            12,
            Operation::RoleAppend {
                id: removed,
                previous: Some(second),
                initial: None,
            },
        );
        h.step(1);
        h.drain();
        let removed_topology = h.ready(
            0,
            &compile(limits(0), &spare_description(None, reverse)).unwrap(),
        );
        let removed_graph = h.graph(0, 2, true, reverse);
        assert_eq!(h.engine.next_frame(), Frame(16));
        let removal = h.op(
            16,
            configuration(removed, removed_topology, removed_graph, 2, 1),
        );
        let missing = h.op(16, Operation::State { target: second });
        let clock = h.op(16, Operation::State { target: id(0) });
        h.step(1);
        h.drain();
        assert_eq!(
            h.reply(removal),
            Reply::ConfigurationActivated {
                roles: removed,
                topology: removed_topology,
                graph: removed_graph,
                graph_epoch: Epoch(3),
                topology_epoch: Epoch(2)
            }
        );
        assert_eq!(
            h.reply(missing),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
        );
        let Reply::Transition(completion) = h.reply(pending) else {
            panic!("pending")
        };
        assert_eq!(
            completion.outcome,
            Outcome::Failed {
                frame: Frame(16),
                reason: shoop_loops::Error::InvalidId
            }
        );
        let Reply::State(clock) = h.reply(clock) else {
            panic!("clock")
        };
        assert_eq!(
            (clock.state.mode, clock.state.cycles, clock.state.last_pulse),
            (Mode::Playing, 11, Some(Frame(16)))
        );
        h.op(
            17,
            Operation::Retire {
                media: initial_media,
            },
        );
        h.op(17, Operation::RoleRecycle { id: removed });
        h.op(
            17,
            Operation::TopologyRecycle {
                id: removed_topology,
            },
        );
        assert_no_heap(|| h.control.release_graph(removed_graph))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        h.step(1);
        h.drain();
        let readded = begin(&mut h, 0, 1);
        h.op(
            19,
            Operation::RoleAppend {
                id: readded,
                previous: None,
                initial: Some(timing_initial(third, 3)),
            },
        );
        h.step(1);
        h.drain();
        let final_topology = h.ready(
            0,
            &compile(limits(0), &spare_description(Some(3), reverse)).unwrap(),
        );
        let final_graph = spare_graph(&mut h, 3, reverse);
        assert_eq!(h.engine.next_frame(), Frame(23));
        let applied = h.op(
            23,
            configuration(readded, final_topology, final_graph, 3, 2),
        );
        h.op(23, play_new(third));
        let old_bank = h.op(23, Operation::RoleStatus { id: roles });
        let old_state = h.op(24, Operation::State { target: second });
        let new_state = h.op(24, Operation::State { target: third });
        let final_wrap = h.op(26, Operation::State { target: third });
        h.op(27, transition(third, Transition::Stop));
        h.op(27, stop(0));
        if reverse {
            for _ in 0..5 {
                h.step(1);
            }
        } else {
            h.step(5);
        }
        h.drain();
        assert_eq!(
            h.reply(applied),
            Reply::ConfigurationActivated {
                roles: readded,
                topology: final_topology,
                graph: final_graph,
                graph_epoch: Epoch(4),
                topology_epoch: Epoch(3)
            }
        );
        assert_eq!(
            h.reply(old_bank),
            Reply::Failed(Failure::Runtime(Error::InvalidId))
        );
        assert_eq!(
            h.reply(old_state),
            Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
        );
        let Reply::State(new_state) = h.reply(new_state) else {
            panic!("new state")
        };
        assert_eq!(
            (
                new_state.state.id,
                new_state.state.mode,
                new_state.state.position,
                new_state.state.cycles,
                new_state.state.content_revision,
                new_state.state.capture_revision
            ),
            (third, Mode::Playing, 1, 0, Revision(0), Revision(0))
        );
        assert_ne!(new_state.playback, Some(initial_media));
        let Reply::State(final_wrap) = h.reply(final_wrap) else {
            panic!("final wrap")
        };
        assert_eq!(
            (
                final_wrap.state.position,
                final_wrap.state.cycles,
                final_wrap.state.last_pulse
            ),
            (0, 1, Some(Frame(26)))
        );
        assert_eq!(h.engine.diagnostics().epoch, Epoch(4));
        assert!(h.samples.iter().all(|s| *s == [0., 0.]));
        assert!(h.packets.is_empty());
        h.finish();
    }
}
