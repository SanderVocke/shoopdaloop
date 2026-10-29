use super::*;
use crate::assembly::{LoopControl, LoopRuntime};
use rt_runtime::{
    midi_state::MidiState,
    resource::{self, Format},
    ResourceId, Revision,
};
use shoop_loops::media;
#[cfg(test)]
use shoop_loops::Mode;

pub(crate) const PAGES: usize = 42;
fn upload(
    store: &mut ResourceStore,
    format: Format,
    frames: u64,
    bytes: &[u8],
    initial: Option<&MidiState>,
) -> media::Source {
    let resource = store.create().unwrap();
    let description = resource::Description {
        format,
        frames,
        bytes: bytes.len(),
    };
    let token = if let Some(initial) = initial {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, initial)
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    for (page, chunk) in bytes.chunks(64).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 64, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
pub(crate) fn prepare(
    witness: Arc<AtomicU32>,
) -> (LoopControl<Mix>, LoopRuntime<Mix>, LoopId, [ResourceId; 4]) {
    let config = Config {
        max_frames: 128,
        max_nodes: 5,
        max_ports: 24,
        max_edges: 16,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 1024,
        midi_bytes_per_port: 4096,
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
    .unwrap();
    let mut store = ResourceStore::with_capture_groups(
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
    .unwrap();
    let id = |slot| LoopId::from_parts(slot, 1).unwrap();
    let node = |slot| NodeId::from_parts(slot, 1).unwrap();
    let mut inputs = Vec::new();
    let mut resources = Vec::new();
    for (revision, values, held, note, velocity) in [
        (0, vec![4f32, 8., 12., 16.], (55, 31), 61, 100),
        (7, vec![20f32, 40., 60.], (70, 50), 62, 90),
    ] {
        let frames = values.len() as u64;
        let bytes: Vec<_> = values
            .into_iter()
            .flat_map(|v| [v, v * 10.])
            .flat_map(f32::to_le_bytes)
            .collect();
        let audio = upload(
            &mut store,
            Format::Audio { channels: 2 },
            frames,
            &bytes,
            None,
        );
        let mut initial = MidiState::default();
        initial.apply_packet(&[0x90, held.0, held.1], 3).unwrap();
        let mut bytes = Vec::new();
        for (frame, packet) in [(0u64, [0x90, note, velocity]), (2, [0x80, note, 45])] {
            bytes.extend_from_slice(&frame.to_le_bytes());
            bytes.extend_from_slice(&3u32.to_le_bytes());
            bytes.extend_from_slice(&packet);
        }
        let midi = upload(&mut store, Format::Midi, frames, &bytes, Some(&initial));
        resources.extend([audio.snapshot.resource, midi.snapshot.resource]);
        inputs.push(media::Input {
            description: media::Description {
                owner: id(0),
                revision: Revision(revision),
                frames,
                channels: 2,
                midi: true,
            },
            audio: Some(audio),
            midi: Some(midi),
        });
    }
    let topology = compile(
        Limits {
            loop_slots: 2,
            pending_requests: 7,
            journal_events: 1024,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![
                LoopSpec {
                    id: id(0),
                    kind: LoopKind::Primitive,
                    length: 4,
                    sync_source: Some(id(1)),
                },
                LoopSpec {
                    id: id(1),
                    kind: LoopKind::Primitive,
                    length: 1,
                    sync_source: None,
                },
            ],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let mut graph = routes(false);
    graph.nodes.extend([node(3), node(4)]);
    let bindings = vec![
        Binding {
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
        },
        Binding {
            target: id(1),
            playback: node(3),
            capture: node(4),
            channels: 0,
            midi: false,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 0,
            midi_bytes: 0,
            packet_bytes: 0,
        },
    ];
    let (control, engine) = LoopRuntime::prepare_with_observer(
        config,
        topology,
        store,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 6,
            media_slots: 8,
            export_slots: 2,
        },
        bindings,
        inputs,
        vec![(node(2), Mix { fault: 0, witness })],
        graph,
        Box::new(crate::journal::Journal::new(4096)),
    )
    .unwrap();
    (control, engine, id(0), resources.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_countdown_and_capture_exhaustion_keep_exact_prefix_and_live_audio() {
        let (mut control, mut engine, target, imports) = prepare(Arc::new(AtomicU32::new(0)));
        let selected = control
            .media()
            .iter()
            .find(|m| m.revision == Revision(7))
            .unwrap()
            .media;
        for (at, operation) in [
            (
                0,
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            ),
            (
                6,
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition: Transition::Seek {
                        position: 1,
                        retrigger: false,
                    },
                },
            ),
            (
                4,
                Operation::Select {
                    target,
                    timing: Timing::At,
                    media: selected,
                    preserve_position: false,
                },
            ),
            (
                7,
                Operation::Transition {
                    target,
                    timing: Timing::AfterTicks {
                        source: LoopId::from_parts(1, 1).unwrap(),
                        skip: 2,
                    },
                    transition: Transition::Record {
                        monitor_previous: false,
                    },
                },
            ),
            (
                0,
                Operation::Transition {
                    target: LoopId::from_parts(1, 1).unwrap(),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            ),
        ] {
            control
                .submit(Lane::Normal, Frame(at), operation)
                .unwrap_or_else(|e| panic!("{:?}", e.error));
        }
        let mut input = [MidiBuffer::new(2, 6).unwrap()];
        let mut output = [MidiBuffer::new(1024, 4096).unwrap()];
        let mut samples = Vec::new();
        let mut midi = Vec::new();
        let mut step = |engine: &mut LoopRuntime<Mix>, frames: usize| {
            let start = engine.next_frame().0;
            let mut left = [0.; 128];
            let mut right = [0.; 128];
            let mut out_left = [0.; 128];
            let mut out_right = [0.; 128];
            input[0].clear();
            output[0].clear();
            for i in 0..frames {
                let frame = start + i as u64;
                left[i] = if frame < 4 {
                    2. * (frame + 1) as f32
                } else {
                    2.
                };
                right[i] = left[i] * 10.;
            }
            for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
                if at >= start && at < start + frames as u64 {
                    input[0].push((at - start) as u32, frames, &packet).unwrap();
                }
            }
            let (result, counts) = crate::allocation_guard::measure(|| {
                engine.process(
                    AudioBlock {
                        frame: Frame(start),
                        frames,
                        inputs: &[&left[..frames], &right[..frames]],
                        outputs: &mut [&mut out_left[..frames], &mut out_right[..frames]],
                    },
                    MidiBlock {
                        inputs: &input,
                        outputs: &mut output,
                    },
                )
            });
            assert_eq!(result, Ok(()));
            assert_eq!(counts, crate::allocation_guard::Counts::default());
            samples.extend((0..frames).map(|i| [out_left[i], out_right[i]]));
            for i in 0..output[0].len() {
                let (offset, packet) = output[0].get(i).unwrap();
                midi.push((start + u64::from(offset), packet.to_vec()));
            }
        };
        step(&mut engine, 64);
        let mut completions = 0;
        while control.try_completion().is_some() {
            completions += 1;
        }
        assert_eq!(completions, 5);
        let mut replies = Vec::new();
        while let Some(reply) = control.try_execution() {
            replies.push(reply);
        }
        assert_eq!(replies.len(), 5);
        let countdown = replies
            .iter()
            .find(|r| r.requested_frame == Frame(7))
            .unwrap();
        assert_eq!(countdown.delivered_frame, Some(Frame(7)));
        assert_eq!(countdown.actual_frame, Frame(9));
        for operation in [
            Operation::State { target },
            Operation::Export {
                target,
                selection: shoop_loops::runtime::Selection::Latest,
            },
        ] {
            control
                .submit(Lane::Normal, Frame(64), operation)
                .unwrap_or_else(|e| panic!("{:?}", e.error));
        }
        step(&mut engine, 1);
        drop(step);
        for (frame, sample) in samples.iter().enumerate() {
            let value = match frame {
                0 => 3.,
                1 => 6.,
                2 => 9.,
                3 => 12.,
                4 => 11.,
                5 | 6 => 21.,
                7 => 31.,
                8 => 11.,
                _ => 1.,
            };
            assert_eq!(*sample, [value, value * 10.], "sample {frame}");
        }
        let mut expected = vec![(0, vec![0x90, 60, 100])];
        fn restore(frame: u64, held: &[(u8, u8)], out: &mut Vec<(u64, Vec<u8>)>) {
            for channel in 0..16 {
                out.push((frame, vec![0xe0 | channel, 0, 64]));
                for cc in [64, 69, 66] {
                    out.push((frame, vec![0xb0 | channel, cc, 0]));
                }
                if channel == 0 {
                    for &(key, velocity) in held {
                        out.push((frame, vec![0x90, key, velocity]));
                    }
                }
            }
        }
        restore(0, &[(55, 31)], &mut expected);
        expected.extend([
            (0, vec![0x90, 61, 100]),
            (2, vec![0x80, 60, 45]),
            (2, vec![0x80, 61, 45]),
            (4, vec![0x80, 55, 0]),
        ]);
        restore(4, &[(70, 50)], &mut expected);
        expected.extend([
            (4, vec![0x90, 62, 90]),
            (6, vec![0x80, 62, 0]),
            (6, vec![0x80, 70, 0]),
        ]);
        restore(6, &[(62, 90), (70, 50)], &mut expected);
        expected.extend([(7, vec![0x80, 62, 45]), (8, vec![0x80, 70, 0])]);
        restore(8, &[(70, 50)], &mut expected);
        expected.extend([
            (8, vec![0x90, 62, 90]),
            (9, vec![0x80, 62, 0]),
            (9, vec![0x80, 70, 0]),
        ]);
        assert_eq!(midi, expected);
        while control.try_completion().is_some() {}
        let mut cut = None;
        let mut exported = None;
        while let Some(reply) = control.try_execution() {
            match reply.operation_result.unwrap() {
                Reply::State(value) => cut = Some(value),
                Reply::Export { id, media, .. } => exported = Some((id, media)),
                other => panic!("{other:?}"),
            }
        }
        let cut = cut.unwrap();
        assert_eq!(cut.state.content_length, 3);
        assert_eq!(cut.state.content_revision, Revision(7));
        assert_eq!(cut.state.capture_revision, Revision(8));
        assert_eq!(cut.state.capture_frames, 40);
        assert!(cut.state.capture_incomplete);
        assert_eq!(cut.capture_failure, Some(Error::Capacity));
        assert_eq!(cut.state.mode, Mode::Stopped);
        let (_, exported) = exported.unwrap();
        assert_eq!(exported.view.description.revision, Revision(8));
        assert_eq!(exported.view.description.frames, 40);
        drop(control);
        let mut stopped = engine.into_stopped();
        for source in [exported.view.audio, exported.view.midi]
            .into_iter()
            .flatten()
        {
            assert_eq!(source.snapshot.description.frames, 40);
            assert_eq!(source.snapshot.cut_frame, Frame(49));
            let capture = source.snapshot.capture.unwrap();
            assert_eq!(capture.incomplete, Some(Error::Capacity));
            let mut cursor = stopped.resources().cursor(source.snapshot.id).unwrap();
            let mut bytes = Vec::new();
            while cursor.position() < source.snapshot.description.bytes {
                let mut buffer = [0; 64];
                stopped.resources().begin_callback();
                let read = stopped.resources().read(&mut cursor, &mut buffer).unwrap();
                assert!(read > 0);
                bytes.extend_from_slice(&buffer[..read]);
            }
            if matches!(source.snapshot.description.format, Format::Audio { .. }) {
                assert_eq!(
                    bytes,
                    [1f32, 10.]
                        .into_iter()
                        .cycle()
                        .take(80)
                        .flat_map(f32::to_le_bytes)
                        .collect::<Vec<_>>()
                );
            } else {
                let mut expected = Vec::new();
                for key in [62, 70] {
                    expected.extend_from_slice(&0u64.to_le_bytes());
                    expected.extend_from_slice(&3u32.to_le_bytes());
                    expected.extend_from_slice(&[0x80, key, 0]);
                }
                assert_eq!(bytes, expected);
            }
        }
        let journal = stopped
            .observer()
            .as_any()
            .downcast_ref::<crate::journal::Journal>()
            .unwrap()
            .bytes();
        assert_eq!(&journal[16..40], &[0; 24]);
        assert!(journal[40..]
            .chunks_exact(160)
            .any(|r| r[0] == 3 && r[1] == 11 && crate::export_wire::get64(r, 8) == 49));
        crate::allocation_guard::assert_no_heap(|| {
            stopped.cleanup().unwrap();
            for resource in imports {
                stopped.resources().remove(resource).unwrap();
            }
            for _ in 0..512 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
        });
        assert_eq!(stopped.resource_usage().free_pages, PAGES);
        assert_eq!(stopped.resource_usage().free_versions, 16);
        assert_eq!(stopped.resource_usage().free_snapshots, 16);
        assert_eq!(stopped.media_usage().references, 0);
    }
}
