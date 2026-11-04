use super::*;
use shoop_wasm_test_support::shoop_test;

#[shoop_test]
fn session_files_preserve_ordinary_bus_shapes_order_and_an_empty_bus_set() {
    for empty in [false, true] {
        let mut bundle = SessionBundle::new(SessionDocument::empty(1000));
        if empty {
            bundle.document.buses.clear();
            bundle.document.bus_display_order.clear();
        } else {
            let mut bus = bundle.document.buses[0].clone();
            bus.id = 23;
            bus.name = "Cue".into();
            bus.gain_db = -6.;
            bus.muted = true;
            let template = bus.ports[0].clone();
            bus.ports = (0..8)
                .map(|index| {
                    let mut port = template.clone();
                    port.id = 1000 + index;
                    port.name = format!("cue_out_{index}");
                    port.external_connections = vec![format!("missing:playback_{index}")];
                    port
                })
                .collect();
            bus.channels = (0..8)
                .map(|index| BusChannelDocument {
                    id: 100 + index,
                    label: format!("Channel {}", index + 1),
                    output_port_id: 1000 + index,
                })
                .collect();
            bundle.document.buses.push(bus);
            bundle.document.bus_display_order = vec![23, 1];
        }
        let expected = bundle.document.buses.clone();
        let order = bundle.document.bus_display_order.clone();
        let image = crate::document::from_bundle(bundle).unwrap();
        let saved = crate::document::to_bundle(&image).unwrap();
        let bytes = encode_session(&saved, "bus-parity").unwrap();
        let reloaded = decode_session(&bytes).unwrap();
        assert_eq!(reloaded.document.buses, expected);
        assert_eq!(reloaded.document.bus_display_order, order);
        crate::document::from_bundle(reloaded).unwrap();
    }
}
fn channel(object: u32, frames: u64, offset: i64) -> image::Channel {
    image::Channel {
        object: Some(object),
        frames,
        gain: 1.,
        start_offset: offset,
        capture_alignment: 0,
        preplay: 0,
        recording_started_at: None,
    }
}
fn content() -> image::Loop {
    image::Loop {
        id: LoopId::from_raw(1),
        name: "User take/?".into(),
        sync: false,
        frames: 4,
        gain: 0.5,
        balance: 0.,
        audio: Vec::new(),
        midi: Vec::new(),
    }
}
#[shoop_test]
fn standard_midi_export_places_all_initial_state_before_frame_zero_events() {
    let initial = vec![vec![0xb0, 0, 1], vec![0xb0, 32, 2], vec![0xc0, 12]];
    let events = vec![
        MidiEventState {
            frame: 0,
            data: vec![0x90, 60, 100].into(),
        },
        MidiEventState {
            frame: 0,
            data: vec![0xb0, 7, 90].into(),
        },
        MidiEventState {
            frame: 2,
            data: vec![0x80, 60, 0].into(),
        },
    ];
    let mut content = content();
    content.midi.push(image::MidiChannel {
        channel: channel(1, 4, 0),
        initial: initial.clone(),
    });
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 1000,
            content,
        },
        payloads: vec![image::Payload::Midi(Arc::new(events.clone()))],
    });
    let Value::File { bytes, warning, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopMidi,
        wav: false,
        standard: true,
        channels: Vec::new(),
    })
    .run()
    .unwrap() else {
        panic!("expected MIDI file")
    };
    assert!(warning.contains("quantizes timing"));
    let exported = decode_standard_midi(&bytes, 1000).unwrap();
    let expected = initial
        .into_iter()
        .chain(events.iter().map(|e| e.data.to_vec()))
        .collect::<Vec<_>>();
    assert_eq!(
        exported
            .events
            .iter()
            .map(|e| e.data.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        exported.events.iter().map(|e| e.frame).collect::<Vec<_>>(),
        [0, 0, 0, 0, 0, 2]
    );
}

#[shoop_test]
fn conversion_budget_keeps_midi_bytes_and_rounds_each_audio_channel() {
    let target = |sample_rate| ConversionTarget {
        sample_rate,
        quantum_frames: 1,
    };
    let midi = ExactMidi {
        sample_rate: 24000,
        length_frames: 4,
        start_state: vec![vec![0xc0, 12]],
        events: vec![ExactMidiEvent {
            frame: 1,
            order: 0,
            data: vec![0x90, 60, 100],
        }],
    };
    let decoded = Decoded::Midi(midi.clone());
    let bytes = decoded.bytes().unwrap();
    decoded
        .check_conversion_budget(target(48000), false, bytes)
        .unwrap();
    assert!(decoded
        .check_conversion_budget(target(48000), false, bytes - 1)
        .is_err());
    let Value::Decoded(Decoded::Midi(converted)) = (Work::Convert {
        decoded,
        target: target(48000),
        update_length: true,
    })
    .run()
    .unwrap() else {
        panic!("expected converted MIDI")
    };
    assert_eq!(converted.start_state, midi.start_state);
    assert_eq!(converted.events[0].data, midi.events[0].data);
    assert_eq!(converted.events[0].frame, 2);
    assert_eq!(converted.length_frames, 8);

    let mut bundle = SessionBundle::new(SessionDocument::empty(24000));
    bundle.media.insert("midi".into(), MediaPayload::Midi(midi));
    bundle.media.insert(
        "audio".into(),
        MediaPayload::Audio(AudioPayload {
            samples: vec![0.; 3],
        }),
    );
    let mixed = Decoded::Session(bundle);
    mixed
        .check_conversion_budget(target(48000), false, bytes + 24)
        .unwrap();
    assert!(mixed
        .check_conversion_budget(target(48000), false, bytes + 23)
        .is_err());

    let audio = Decoded::Audio(LoopAudio {
        sample_rate: 48000,
        channels: (0..2)
            .map(|_| LoopAudioChannel {
                label: String::new(),
                role: "direct".into(),
                samples: vec![0.; 3],
            })
            .collect(),
    });
    audio
        .check_conversion_budget(target(24000), false, 16)
        .unwrap();
    assert!(audio
        .check_conversion_budget(target(24000), false, 15)
        .is_err());
}
#[shoop_test]
fn exact_audio_export_maps_the_logical_window_without_baking_in_gain() {
    let mut content = content();
    content.audio.push(channel(1, 2, -1));
    content.audio[0].gain = 3.;
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 48000,
            content,
        },
        payloads: vec![image::Payload::Audio(Arc::new(vec![-0.0, 0.25]))],
    });
    let Value::File { name, bytes, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopAudio,
        wav: false,
        standard: false,
        channels: vec![0],
    })
    .run()
    .unwrap() else {
        panic!()
    };
    assert_eq!(name, "User_take__.shoop-audio");
    let audio = decode_loop_audio(&bytes).unwrap();
    assert_eq!(audio.channels[0].samples, [0., -0.0, 0.25, 0.]);
    assert_eq!(audio.channels[0].samples[1].to_bits(), (-0.0f32).to_bits());
}
#[shoop_test]
fn exact_midi_export_folds_preroll_and_preserves_logical_event_order() {
    let mut content = content();
    content.frames = 3;
    content.midi.push(image::MidiChannel {
        channel: channel(1, 8, 2),
        initial: vec![vec![0xc0, 12]],
    });
    let packets = [
        (0, vec![0xb0, 7, 90]),
        (1, vec![0x90, 60, 100]),
        (2, vec![0x80, 60, 0]),
        (3, vec![0x90, 62, 99]),
        (6, vec![0x80, 62, 0]),
    ];
    let events = packets
        .into_iter()
        .map(|(frame, data)| MidiEventState {
            frame,
            data: data.into(),
        })
        .collect::<Vec<_>>();
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 48000,
            content,
        },
        payloads: vec![image::Payload::Midi(events.into())],
    });
    let Value::File { bytes, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopMidi,
        wav: false,
        standard: false,
        channels: Vec::new(),
    })
    .run()
    .unwrap() else {
        panic!()
    };
    let midi = decode_exact_midi(&bytes).unwrap();
    assert_eq!(midi.length_frames, 3);
    assert!(midi.start_state.contains(&vec![0xc0, 12]));
    assert!(midi.start_state.contains(&vec![0xb0, 7, 90]));
    assert!(midi.start_state.contains(&vec![0x90, 60, 100]));
    assert_eq!(
        midi.events,
        [
            ExactMidiEvent {
                frame: 0,
                order: 2,
                data: vec![0x80, 60, 0]
            },
            ExactMidiEvent {
                frame: 1,
                order: 3,
                data: vec![0x90, 62, 99]
            }
        ]
    );
}
#[shoop_test]
fn importing_unrestorable_midi_initial_state_refuses_before_staging() {
    let midi = ExactMidi {
        sample_rate: 1000,
        length_frames: 4,
        start_state: vec![vec![0xb0, 6, 40]],
        events: Vec::new(),
    };
    let error = (Work::Prepare {
        decoded: Decoded::Midi(midi),
        mapping: Vec::new(),
        update_length: true,
    })
    .run()
    .err()
    .unwrap();
    assert!(error.to_string().contains("parameter history"));
}
#[shoop_test]
fn a_final_system_reset_makes_prior_parameter_history_restorable() {
    let midi = ExactMidi {
        sample_rate: 1000,
        length_frames: 4,
        start_state: vec![vec![0xb0, 6, 40], vec![0xff]],
        events: Vec::new(),
    };
    assert!(matches!(
        (Work::Prepare {
            decoded: Decoded::Midi(midi),
            mapping: Vec::new(),
            update_length: true
        })
        .run(),
        Ok(Value::Image(_))
    ));
}

#[shoop_test]
fn preview_destination_survives_names_and_order_and_rejects_missing_identity() {
    use shoop_session::PreviewBusDocument;
    for designation in [
        PreviewBusDocument::Bus(1),
        PreviewBusDocument::Disabled,
        PreviewBusDocument::Automatic,
    ] {
        let mut bundle = SessionBundle::new(SessionDocument::empty(48000));
        bundle.document.preview_bus = designation;
        bundle.document.buses[0].name = "Main output".into();
        let image = crate::document::from_bundle(bundle).unwrap();
        let image::Manifest::Session(session) = &image.manifest else {
            panic!()
        };
        assert_eq!(
            session.preview_bus,
            (designation == PreviewBusDocument::Bus(1)).then_some(BusId::from_raw(1))
        );
        let saved = crate::document::to_bundle(&image).unwrap();
        let bytes = encode_session(&saved, "preview").unwrap();
        let restored = crate::document::from_bundle(decode_session(&bytes).unwrap()).unwrap();
        assert_eq!(image, restored);
    }
    let mut bundle = SessionBundle::new(SessionDocument::empty(48000));
    bundle.document.preview_bus = PreviewBusDocument::Bus(999);
    assert!(crate::document::from_bundle(bundle).is_err());
    let bundle = SessionBundle::new(SessionDocument::empty(48000));
    let image = crate::document::from_bundle(bundle).unwrap();
    let image::Manifest::Session(session) = &image.manifest else {
        panic!()
    };
    assert_eq!(session.preview_bus, Some(BusId::from_raw(1)));
}
