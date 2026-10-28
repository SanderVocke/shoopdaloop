use super::*;
use shoop_loop_engine::{Mode, Trigger};

pub(crate) const RECORDS: usize = 4096;
pub(crate) const MIDI_EVENTS: usize = 16384;
pub(crate) const MIDI_BYTES: usize = 65536;

pub(crate) fn prepare(witness: Arc<AtomicU32>) -> (Controller<Mix>, Engine<Mix>, LoopId) {
    let config = Config {
        max_frames: 128,
        max_nodes: 7,
        max_ports: 24,
        max_edges: 16,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: MIDI_EVENTS,
        midi_bytes_per_port: MIDI_BYTES,
        resource_slots: 16,
        resource_pages: PAGES,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
        budgets: Budgets {
            commands: 8,
            events: 8,
            subranges: 0,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 8,
        },
        ..Config::default()
    }
    .prepare()
    .expect("dense configuration");
    let resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 4,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 4096,
        },
    )
    .expect("dense capture storage");
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let node = |slot| NodeId::from_parts(slot, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 5,
            pending_requests: 5,
            journal_events: 1024,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: (0..5)
                .map(|slot| LoopSpec {
                    id: id(slot),
                    kind: if slot < 3 {
                        LoopKind::Primitive
                    } else {
                        LoopKind::Composite
                    },
                    length: if slot == 1 || slot == 2 { 1 } else { 0 },
                    sync_source: (slot >= 3).then(|| id(1)),
                })
                .collect(),
            triggers: vec![
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
                Trigger {
                    source: id(1),
                    target: id(3),
                    mode: Mode::Playing,
                },
            ],
            programs: vec![
                ProgramDescription {
                    source: id(3),
                    nominal_unit_frames: 1,
                    sections: vec![Section {
                        entries: vec![Entry {
                            target: id(4),
                            delay: 0,
                            duration: Some(4),
                            repetitions: 1,
                            mode: None,
                        }],
                    }],
                },
                ProgramDescription {
                    source: id(4),
                    nominal_unit_frames: 1,
                    sections: [Mode::Playing, Mode::Stopped]
                        .into_iter()
                        .map(|mode| Section {
                            entries: vec![Entry {
                                target: id(2),
                                delay: 0,
                                duration: Some(2),
                                repetitions: 1,
                                mode: Some(mode),
                            }],
                        })
                        .collect(),
                },
            ],
            ..TopologyDescription::default()
        },
    )
    .expect("dense acyclic start and clock relationships");
    let mut graph = routes(false);
    graph.nodes.extend((3..7).map(node));
    let mut bindings = vec![Binding {
        target: id(0),
        playback: node(0),
        capture: node(1),
        channels: 2,
        midi: true,
        gain: 1.,
        smoothing: Smoothing::None,
        midi_events: 128,
        midi_bytes: 512,
        packet_bytes: 512,
    }];
    for slot in 1..3 {
        bindings.push(Binding {
            target: id(slot),
            playback: node(slot * 2 + 1),
            capture: node(slot * 2 + 2),
            channels: 0,
            midi: false,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 0,
            midi_bytes: 0,
            packet_bytes: 0,
        });
    }
    let (control, engine) = Engine::prepare_with_observer(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 6,
            export_slots: 2,
        },
        bindings,
        vec![],
        vec![(node(2), Mix { fault: 0, witness })],
        graph,
        Box::new(crate::journal::Journal::new(RECORDS)),
    )
    .expect("dense prepared facade");
    (control, engine, id(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dense_capture_cascades_and_clocked_composites_keep_music_with_unread_results() {
        let (mut control, mut engine, target) = prepare(Arc::new(AtomicU32::new(0)));
        for (at, transition) in [
            (
                0,
                Transition::Record {
                    monitor_previous: false,
                },
            ),
            (
                1,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            ),
            (
                64,
                Transition::Seek {
                    position: 0,
                    retrigger: false,
                },
            ),
            (
                130,
                Transition::Seek {
                    position: 0,
                    retrigger: false,
                },
            ),
        ] {
            control
                .submit(
                    Lane::Normal,
                    Frame(at),
                    Operation::Transition {
                        target,
                        timing: Timing::At,
                        transition,
                    },
                )
                .unwrap_or_else(|error| panic!("{:?}", error.error));
        }
        let mut midi_in = [MidiBuffer::new(1, 3).unwrap()];
        let mut midi_out = [MidiBuffer::new(MIDI_EVENTS, MIDI_BYTES).unwrap()];
        let mut packets = Vec::new();
        let mut samples = Vec::new();
        for (frames, recovery) in [(128, false), (1, false), (2, true)] {
            let at = engine.next_frame().0;
            if recovery {
                control
                    .submit(
                        Lane::Recovery,
                        Frame(at),
                        Operation::Transition {
                            target,
                            timing: Timing::At,
                            transition: Transition::Stop,
                        },
                    )
                    .unwrap_or_else(|error| panic!("{:?}", error.error));
            }
            let mut left = [0.; 128];
            let mut right = [0.; 128];
            if at == 0 {
                left[0] = 2.;
                right[0] = 20.;
            }
            midi_in[0].clear();
            if at == 0 {
                midi_in[0].push(0, frames, &[0x90, 60, 100]).unwrap();
            }
            let mut output_left = [0.; 128];
            let mut output_right = [0.; 128];
            midi_out[0].clear();
            let counts = crate::allocation_guard::measure(|| {
                engine.process(
                    AudioBlock {
                        frame: Frame(at),
                        frames,
                        inputs: &[&left[..frames], &right[..frames]],
                        outputs: &mut [&mut output_left[..frames], &mut output_right[..frames]],
                    },
                    MidiBlock {
                        inputs: &midi_in,
                        outputs: &mut midi_out,
                    },
                )
            });
            assert_eq!(counts.0, Ok(()));
            assert_eq!(counts.1, crate::allocation_guard::Counts::default());
            samples.extend((0..frames).map(|i| [output_left[i], output_right[i]]));
            for i in 0..midi_out[0].len() {
                let (offset, packet) = midi_out[0].get(i).unwrap();
                packets.push((at + u64::from(offset), packet.to_vec()));
            }
        }
        assert_eq!(
            samples,
            (0..131)
                .map(|frame| if frame == 0 {
                    [1., 10.]
                } else if frame < 129 {
                    [0.5, 5.]
                } else {
                    [0., 0.]
                })
                .collect::<Vec<_>>()
        );
        let mut expected = vec![(0, vec![0x90, 60, 100])];
        for frame in 1..129 {
            if frame > 1 {
                expected.push((frame, vec![0x80, 60, 0]));
            }
            for channel in 0..16 {
                expected.push((frame, vec![0xe0 | channel, 0, 64]));
                for cc in [64, 69, 66] {
                    expected.push((frame, vec![0xb0 | channel, cc, 0]));
                }
            }
            expected.push((frame, vec![0x90, 60, 100]));
        }
        expected.push((129, vec![0x80, 60, 0]));
        assert_eq!(packets, expected);
        let mut completions = 0;
        while control.try_completion().is_some() {
            completions += 1;
        }
        let mut executions = 0;
        while control.try_execution().is_some() {
            executions += 1;
        }
        assert_eq!((completions, executions), (5, 5));
        drop(control);
        let mut stopped = engine.into_stopped();
        let journal = stopped
            .observer()
            .as_any()
            .downcast_ref::<crate::journal::Journal>()
            .unwrap()
            .bytes();
        assert_eq!(&journal[16..40], &[0; 24]);
        let events = journal[40..]
            .chunks_exact(160)
            .filter(|r| r[0] == 3)
            .map(|r| {
                (
                    crate::export_wire::get64(r, 8),
                    r[1],
                    crate::export_wire::get32(r, 16),
                )
            })
            .collect::<Vec<_>>();
        for target in 0..5 {
            assert!(
                events.contains(&(0, 10, target)),
                "start cascade target {target}"
            );
        }
        for frame in 1..131 {
            assert!(
                events.contains(&(frame, 10, 1)),
                "one-frame source clock {frame}"
            );
        }
        for frame in 1..129 {
            assert_eq!(
                events.contains(&(frame, 9, 0)),
                frame != 1 && frame != 64,
                "media wrap {frame}"
            );
        }
        assert!(!events.contains(&(129, 10, 0)));
        let counts = crate::allocation_guard::measure(|| {
            stopped.cleanup().unwrap();
            for _ in 0..512 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
        });
        assert_eq!(counts.1, crate::allocation_guard::Counts::default());
        assert_eq!(stopped.resource_usage().free_pages, PAGES);
        assert_eq!(stopped.resource_usage().free_versions, 16);
        assert_eq!(stopped.resource_usage().free_snapshots, 16);
        assert_eq!(stopped.media_usage().references, 0);
    }
}
