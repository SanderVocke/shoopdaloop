use super::*;
use rt_runtime::{
    buffer::MidiBuffer,
    midi_state::MidiState,
    resource::{self, ResourceStore, Snapshot},
    Config, Frame, Revision,
};
use shoop_wasm_test_support::shoop_test;

fn events(resources: &mut ResourceStore, snapshot: Snapshot) -> Vec<MidiEventState> {
    let mut bytes = vec![0; snapshot.description.bytes];
    let mut cursor = resources.cursor(snapshot.id).unwrap();
    let mut copied = 0;
    while copied < bytes.len() {
        resources.begin_callback();
        let end = (copied + 7).min(bytes.len());
        copied += resources
            .read(&mut cursor, &mut bytes[copied..end])
            .unwrap();
    }
    let mut at = 0;
    let mut events = Vec::new();
    while at < bytes.len() {
        let frame = u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let len = u32::from_le_bytes(bytes[at + 8..at + 12].try_into().unwrap()) as usize;
        events.push(MidiEventState {
            frame: frame as u32,
            data: bytes[at + 12..at + 12 + len].to_vec().into(),
        });
        at += 12 + len;
    }
    events
}
fn initial(resources: &mut ResourceStore, snapshot: Snapshot) -> (MidiState, Vec<Vec<u8>>) {
    let index = resources.read_index(snapshot.id).unwrap();
    let state = resources.render_io().midi_seek(index, 0).unwrap().state;
    let mut packets = Vec::new();
    let report = shoop_loops::midi_restore::restore(&state, |p| {
        packets.push(p.to_vec());
        Ok(())
    });
    assert_eq!(report.first_error, None);
    assert!(!report.parameter_history_untracked);
    (state, packets)
}
fn drain(resources: &mut ResourceStore) {
    for _ in 0..4096 {
        resources.begin_callback();
        resources.reclaim();
    }
}
fn reload_and_edit(
    resources: &mut ResourceStore,
    store: resource::MidiStoreId,
    events: &[MidiEventState],
    packets: &[Vec<u8>],
    expected: &MidiState,
) {
    let mut initial = MidiState::default();
    for packet in packets {
        initial.apply_packet(packet, 3).unwrap();
    }
    assert_eq!(&initial, expected);
    let data: Vec<u8> = events
        .iter()
        .flat_map(|e| {
            let mut data = vec![0; 12 + e.data.len()];
            resource::encode_midi_record(u64::from(e.frame), &e.data, &mut data).unwrap();
            data
        })
        .collect();
    let loaded = resources.create().unwrap();
    let upload = resources
        .begin_indexed_midi_upload(
            loaded,
            Revision(0),
            resource::Description {
                format: resource::Format::Midi,
                frames: 8,
                bytes: data.len(),
            },
            &initial,
        )
        .unwrap();
    for (page, bytes) in data.chunks(16).enumerate() {
        resources.begin_callback();
        resources.write_upload(upload, page * 16, bytes).unwrap();
    }
    resources.begin_callback();
    resources.finish_upload(upload).unwrap();
    resources.commit(upload).unwrap();
    let original = resources.snapshot(loaded, Frame(0)).unwrap();
    let import = resources.prepare_midi_import(store, original.id).unwrap();
    while resources.render_io().progress_midi_import(import, 3)
        == Ok(resource::shared_midi::EditStatus::Building)
    {}
    let editable = resources.finish_midi_import(import).unwrap();
    let mut input = MidiBuffer::new(1, 3).unwrap();
    input.push(0, 1, &[0xbf, 1, 42]).unwrap();
    let edit = resources
        .render_io()
        .prepare_midi_replace(editable, Revision(1), 7, 1, &input, &initial)
        .unwrap();
    while resources.render_io().progress_midi_edit(edit, 3)
        == Ok(resource::shared_midi::EditStatus::Building)
    {}
    resources.render_io().commit_midi_edit(edit).unwrap();
    let edited = resources.snapshot(editable, Frame(0)).unwrap();
    let mut expected_events = events.to_vec();
    expected_events.retain(|e| e.frame != 7);
    expected_events.push(MidiEventState {
        frame: 7,
        data: vec![0xbf, 1, 42].into(),
    });
    assert_eq!(self::events(resources, edited), expected_events);
    assert_eq!(self::events(resources, original), events);
    assert_eq!(self::initial(resources, edited).0, initial);
    resources.remove(loaded).unwrap();
    resources.remove(editable).unwrap();
    resources.release_snapshot(original.id).unwrap();
    resources.release_snapshot(edited.id).unwrap();
    drain(resources);
}

#[shoop_test]
fn shared_midi_session_exact_and_standard_exports_round_trip_then_reload_and_edit() {
    let mut resources = ResourceStore::with_midi_indexing(
        Config {
            sample_rate: 1000,
            max_frames: 16,
            resource_slots: 24,
            resource_pages: 4096,
            page_bytes: 16,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
        resource::MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 32,
            replay_events: 4,
        },
    )
    .unwrap();
    let store = resources
        .prepare_midi_store(
            resource::shared_midi::Config {
                history_frames: 8,
                chunks: 8,
                views: 16,
                pages: 4000,
                max_events: 32,
                max_bytes: 256,
                max_packet: 48,
                transactions: 2,
                edit_steps: 3,
            },
            Frame(0),
            &MidiState::default(),
        )
        .unwrap();
    let mut sysex = vec![1; 35];
    let audio = resources
        .prepare_audio_store(
            resource::shared_audio::Config {
                channels: 1,
                history_frames: 8,
                views: 4,
                pages: 32,
            },
            Frame(0),
        )
        .unwrap();
    resources
        .render_io()
        .append_audio_history(audio, Frame(0), &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10.])
        .unwrap();
    let audio_resource = resources.grab_audio(audio, Frame(2)..Frame(10)).unwrap();
    let audio_snapshot = resources.snapshot(audio_resource, Frame(10)).unwrap();
    resources.remove(audio_resource).unwrap();
    sysex[0] = 0xf0;
    sysex[34] = 0xf7;
    let mut input = MidiBuffer::new(32, 256).unwrap();
    for (at, packet) in [
        (0, &[0x90, 60, 80][..]),
        (1, &[0xb0, 0, 2]),
        (1, &[0xc0, 5]),
        (2, &[0xb0, 7, 99]),
        (3, &sysex),
        (5, &[0x80, 60, 0]),
    ] {
        input.push(at, 10, packet).unwrap();
    }
    resources
        .render_io()
        .append_midi_history(store, Frame(0), 10, &input)
        .unwrap();
    let mut initial_state = MidiState::default();
    for packet in [&[0x90, 60, 80][..], &[0xb0, 0, 2], &[0xc0, 5]] {
        initial_state.apply_packet(packet, 3).unwrap();
    }
    let original_events = vec![
        MidiEventState {
            frame: 0,
            data: vec![0xb0, 7, 99].into(),
        },
        MidiEventState {
            frame: 1,
            data: sysex.clone().into(),
        },
        MidiEventState {
            frame: 3,
            data: vec![0x80, 60, 0].into(),
        },
    ];
    let editable = resources.grab_midi(store, Frame(2)..Frame(10)).unwrap();
    let old = resources.snapshot(editable, Frame(10)).unwrap();
    assert_eq!(old.cut_frame, audio_snapshot.cut_frame);
    assert_eq!(events(&mut resources, old), original_events);
    assert_eq!(initial(&mut resources, old).0, initial_state);
    input.clear();
    resources
        .render_io()
        .append_midi_history(store, Frame(10), 10, &input)
        .unwrap();
    resources
        .render_io()
        .append_audio_history(
            audio,
            Frame(10),
            &[11., 12., 13., 14., 15., 16., 17., 18., 19., 20.],
        )
        .unwrap();
    input.clear();
    input.push(0, 3, &[0xb0, 1, 88]).unwrap();
    let edit = resources
        .render_io()
        .prepare_midi_replace(editable, Revision(1), 2, 3, &input, &initial_state)
        .unwrap();
    while resources.render_io().progress_midi_edit(edit, 3)
        == Ok(resource::shared_midi::EditStatus::Building)
    {}
    resources.render_io().commit_midi_edit(edit).unwrap();
    let middle = resources.snapshot(editable, Frame(20)).unwrap();
    drain(&mut resources);
    input.clear();
    input.push(0, 4, &[0xff]).unwrap();
    input.push(2, 4, &[0x9f, 70, 88]).unwrap();
    input.push(3, 4, &sysex).unwrap();
    let edit = resources
        .render_io()
        .prepare_midi_replace(editable, Revision(2), 6, 4, &input, &initial_state)
        .unwrap();
    while resources.render_io().progress_midi_edit(edit, 3)
        == Ok(resource::shared_midi::EditStatus::Building)
    {}
    resources.render_io().commit_midi_edit(edit).unwrap();
    let final_snapshot = resources.snapshot(editable, Frame(20)).unwrap();
    resources.remove(editable).unwrap();
    drain(&mut resources);
    for snapshot in [old, middle, final_snapshot] {
        let originals = events(&mut resources, snapshot);
        let (state, start_state) = initial(&mut resources, snapshot);
        let mut content = tests::content();
        content.frames = 8;
        let mut cursor = resources.cursor(audio_snapshot.id).unwrap();
        let mut bytes = [0; 32];
        let mut copied = 0;
        while copied < bytes.len() {
            resources.begin_callback();
            let end = (copied + 7).min(bytes.len());
            copied += resources
                .read(&mut cursor, &mut bytes[copied..end])
                .unwrap();
        }
        let samples: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(samples, [3., 4., 5., 6., 7., 8., 9., 10.]);
        content.audio = vec![tests::channel(1, 8, 0)];
        content.midi = vec![image::MidiChannel {
            channel: tests::channel(2, 8, 0),
            initial: start_state.clone(),
        }];
        let midi_payload = image::Payload::Midi(Arc::new(originals.clone()));
        let payloads = vec![
            image::Payload::Audio(Arc::new(samples)),
            midi_payload.clone(),
        ];
        let mut session = tests::session(content.clone(), 1, true);
        session.sample_rate = 1000;
        let Value::File { bytes, .. } = (Work::Encode {
            image: Arc::new(image::Image {
                manifest: image::Manifest::Session(session),
                payloads: payloads.clone(),
            }),
            kind: IoTaskKind::SaveSession,
            wav: false,
            standard: false,
            channels: vec![],
        })
        .run()
        .unwrap() else {
            panic!("session file")
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: 1000,
        })
        .run()
        .unwrap() else {
            panic!("decoded session")
        };
        let Value::Image(restored) = (Work::Prepare {
            decoded,
            mapping: vec![],
            update_length: false,
        })
        .run()
        .unwrap() else {
            panic!("session image")
        };
        assert_eq!(restored.payloads, payloads);
        let image::Manifest::Session(session) = &restored.manifest else {
            panic!("session")
        };
        assert_eq!(session.tracks[0].loops[0].frames, 8);
        let initial = &session.tracks[0].loops[0].midi[0].initial;
        assert_eq!(initial, &start_state);
        reload_and_edit(&mut resources, store, &originals, initial, &state);
        for standard in [false, true] {
            let Value::File { bytes, warning, .. } = (Work::Encode {
                image: Arc::new(image::Image {
                    manifest: image::Manifest::Loop {
                        sample_rate: 1000,
                        content: content.clone(),
                    },
                    payloads: payloads.clone(),
                }),
                kind: IoTaskKind::ExportLoopMidi,
                wav: false,
                standard,
                channels: vec![],
            })
            .run()
            .unwrap() else {
                panic!("MIDI file")
            };
            if standard {
                assert!(warning.contains("quantizes timing"));
                let decoded = decode_standard_midi(&bytes, 1000).unwrap();
                assert_eq!(decoded.length_frames, 8);
                assert_eq!(
                    decoded
                        .events
                        .iter()
                        .map(|e| e.data.clone())
                        .collect::<Vec<_>>(),
                    start_state
                        .iter()
                        .cloned()
                        .chain(originals.iter().map(|e| e.data.to_vec()))
                        // The existing SMF conversion drops realtime reset.
                        .filter(|packet| packet.as_slice() != [0xff])
                        .collect::<Vec<_>>()
                );
                continue;
            }
            let decoded = decode_exact_midi(&bytes).unwrap();
            assert_eq!(decoded.length_frames, 8);
            assert_eq!(decoded.start_state, start_state);
            assert_eq!(
                decoded.events,
                originals
                    .iter()
                    .enumerate()
                    .map(|(order, e)| ExactMidiEvent {
                        frame: u64::from(e.frame),
                        order: order as u32,
                        data: e.data.to_vec()
                    })
                    .collect::<Vec<_>>()
            );
            let Value::Decoded(decoded) = (Work::Decode {
                kind: IoTaskKind::ImportLoopMidi,
                bytes,
                rate: 1000,
            })
            .run()
            .unwrap() else {
                panic!("MIDI decode")
            };
            let Value::Image(restored) = (Work::Prepare {
                decoded,
                mapping: vec![],
                update_length: true,
            })
            .run()
            .unwrap() else {
                panic!("MIDI image")
            };
            assert_eq!(restored.payloads, vec![midi_payload.clone()]);
            let image::Manifest::Update { frames, midi, .. } = &restored.manifest else {
                panic!("MIDI update")
            };
            assert_eq!(*frames, Some(8));
            reload_and_edit(
                &mut resources,
                store,
                &originals,
                &midi[0].content.initial,
                &state,
            );
        }
        resources.release_snapshot(snapshot.id).unwrap();
    }
    drain(&mut resources);
    resources.release_snapshot(audio_snapshot.id).unwrap();
    drain(&mut resources);
    resources.cleanup_audio_store(audio).unwrap();
    resources.cleanup_midi_store(store).unwrap();
    drain(&mut resources);
    assert_eq!(resources.usage().free_pages, 4096);
    assert_eq!(resources.usage().reserved_pages, 0);
    assert_eq!(resources.midi_index_usage().free_versions, 8);
}
