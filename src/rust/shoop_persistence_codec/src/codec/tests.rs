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
pub(super) fn channel(object: u32, frames: u64, offset: i64) -> image::Channel {
    image::Channel {
        mode: image::ChannelMode::Direct,
        object: Some(object),
        frames,
        gain: 1.,
        start_offset: offset,
        capture_alignment: 0,
        preplay: 0,
        recording_started_at: None,
    }
}
pub(super) fn content() -> image::Loop {
    image::Loop {
        recorded_processor_state: None,
        sync_source: None,
        repeat_sync: false,
        composite: None,
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

fn composite_session_image() -> image::Image {
    use shoop_app_protocol::composite as c;
    let mut child = content();
    child.frames = 0;
    let mut session = session(child, 0, false);
    for channel in &mut session.buses[0].channels {
        channel.port.state.connection_policy = ConnectionPolicy::UserManaged;
    }
    session.global_ports.push(image::Port {
        state: ApplicationPortState {
            publication: None,
            id: PortId::from_raw(3),
            owner: ApplicationPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".into(),
            data_type: PortDataType::Midi,
            direction: PortDirection::Input,
            role: PortRole::MidiInput,
            connection_policy: ConnectionPolicy::UserManaged,
        },
        external_connections: vec![],
    });
    session.global.sync = false;
    let target = LoopId::from_raw(1);
    let mut parent = content();
    parent.id = LoopId::from_raw(2);
    parent.frames = 480 * 1010;
    parent.sync_source = Some(LoopId::from_raw(4));
    parent.repeat_sync = true;
    parent.composite = Some(c::Content {
        document: c::Document {
            kind: CompositeKind::Regular,
            basis: c::TimingBasis {
                sample_rate: 48000,
                unit_frames: 480,
            },
            instances: [(17, 0, 3), (19, 1003, 7)]
                .into_iter()
                .map(|(instance_id, start_cycle, n_cycles)| c::Instance {
                    instance_id,
                    loop_id: target,
                    start_cycle,
                    n_cycles: Some(n_cycles),
                    mode: None,
                })
                .collect(),
        },
        prepared: c::Prepared {
            sample_rate: 48000,
            quantum: 32,
            unit_frames: 480,
            iterations: 1010,
            instances: vec![
                c::EffectiveInstance {
                    instance_id: 17,
                    start_frame: 0,
                    end_frame: 1440,
                },
                c::EffectiveInstance {
                    instance_id: 19,
                    start_frame: 481440,
                    end_frame: 484800,
                },
            ],
            spans: vec![
                c::Span {
                    target,
                    start: 0,
                    end: 3,
                    occurrence_start: 0,
                    ordinal: 0,
                    mode: LoopMode::Playing,
                },
                c::Span {
                    target,
                    start: 1003,
                    end: 1010,
                    occurrence_start: 1003,
                    ordinal: 1,
                    mode: LoopMode::Playing,
                },
            ],
        },
    });
    let mut outer = parent.clone();
    outer.id = LoopId::from_raw(3);
    outer.sync_source = None;
    outer.repeat_sync = false;
    let c = outer.composite.as_mut().unwrap();
    c.document.instances = vec![c::Instance {
        instance_id: 23,
        loop_id: parent.id,
        start_cycle: 0,
        n_cycles: Some(1010),
        mode: None,
    }];
    c.prepared.instances = vec![c::EffectiveInstance {
        instance_id: 23,
        start_frame: 0,
        end_frame: 484800,
    }];
    c.prepared.spans = vec![c::Span {
        target: parent.id,
        start: 0,
        end: 1010,
        occurrence_start: 0,
        ordinal: 0,
        mode: LoopMode::Playing,
    }];
    let mut source = content();
    source.id = LoopId::from_raw(4);
    source.frames = 96;
    session.tracks[0].loops.extend([parent, outer, source]);
    image::Image {
        manifest: image::Manifest::Session(session),
        payloads: vec![],
    }
}

#[shoop_test]
fn nested_composite_files_preserve_original_basis_frozen_spans_and_repeat_after_source_resize() {
    let original = composite_session_image();
    let bundle = document::to_bundle(&original).unwrap();
    let bytes = encode_session(&bundle, "composite-parity").unwrap();
    let decoded = decode_session(&bytes).unwrap();
    assert_eq!(decoded, bundle);
    let restored = document::from_bundle(decoded).unwrap();
    let image::Manifest::Session(expected) = &original.manifest else {
        panic!()
    };
    let image::Manifest::Session(actual) = &restored.manifest else {
        panic!()
    };
    assert_eq!(actual.tracks, expected.tracks);
    assert_eq!(restored.payloads, original.payloads);
    let saved = document::to_bundle(&restored).unwrap();
    assert_eq!(saved, bundle);
}

#[shoop_test]
fn composite_format_conversion_rounds_each_frozen_endpoint_and_keeps_the_authored_clock() {
    let original = composite_session_image();
    let bundle = document::to_bundle(&original).unwrap();
    let target = ConversionTarget {
        sample_rate: 44100,
        quantum_frames: 32,
    };
    let plan = SessionFormatPlan::new(&bundle, target, MAX_SESSION_MEDIA_BYTES).unwrap();
    assert!(plan.timing.requires_confirmation);
    let converted = plan.convert(&bundle, &mut || false).unwrap();
    let image = document::from_bundle(converted.clone()).unwrap();
    let image::Manifest::Session(session) = &image.manifest else {
        panic!()
    };
    let c = session.tracks[0].loops[1].composite.as_ref().unwrap();
    assert_eq!(
        c.document.basis,
        shoop_app_protocol::composite::TimingBasis {
            sample_rate: 48000,
            unit_frames: 480
        }
    );
    assert_eq!(
        (
            c.prepared.instances[0].start_frame,
            c.prepared.instances[0].end_frame
        ),
        (0, 1312)
    );
    assert_eq!(
        (
            c.prepared.instances[1].start_frame,
            c.prepared.instances[1].end_frame
        ),
        (442336, 445408)
    );
    assert_eq!(session.tracks[0].loops[1].frames, 445408);
    let second = SessionFormatPlan::new(&converted, target, MAX_SESSION_MEDIA_BYTES).unwrap();
    assert!(!second.timing.requires_confirmation);
    assert_eq!(
        second.convert(&converted, &mut || false).unwrap(),
        converted
    );
}

#[shoop_test]
fn legacy_composite_migration_uses_saved_lengths_and_unsupported_modes_refuse_whole_documents() {
    let mut bundle = document::to_bundle(&composite_session_image()).unwrap();
    for loop_ in &mut bundle.document.track_groups[1].tracks[0].loops {
        loop_.execution = None;
        if let Some(composite) = &mut loop_.composite {
            composite.timing = None;
        }
    }
    let once = document::from_bundle(bundle.clone()).unwrap();
    let twice = document::from_bundle(bundle.clone()).unwrap();
    assert_eq!(once, twice);
    let image::Manifest::Session(session) = &once.manifest else {
        panic!()
    };
    assert_eq!(
        session.tracks[0].loops[1]
            .composite
            .as_ref()
            .unwrap()
            .document
            .basis
            .unit_frames,
        480
    );
    bundle.document.track_groups[1].tracks[0].loops[1]
        .composite
        .as_mut()
        .unwrap()
        .kind = CompositeKindDocument::Script;
    for (mode, diagnostic) in [
        ("replacing", "replacing composite events"),
        ("playing_dry_through_wet", "dry/wet composite events"),
        ("recording_dry_into_wet", "dry/wet composite events"),
    ] {
        for event in &mut bundle.document.track_groups[1].tracks[0].loops[1]
            .composite
            .as_mut()
            .unwrap()
            .instances
        {
            event.mode = Some(mode.into());
        }
        let error = document::from_bundle(bundle.clone())
            .unwrap_err()
            .to_string();
        assert!(error.contains(diagnostic), "{mode}: {error}");
    }
}

#[shoop_test]
fn compatible_legacy_composite_metadata_adopts_the_engine_quantum_without_a_retiming_prompt() {
    let original = composite_session_image();
    let mut bundle = document::to_bundle(&original).unwrap();
    for loop_ in &mut bundle.document.track_groups[1].tracks[0].loops {
        if let Some(composite) = &mut loop_.composite {
            composite.timing = None;
        }
    }
    let target = ConversionTarget {
        sample_rate: 48000,
        quantum_frames: 32,
    };
    let Value::Inspected {
        decoded,
        conversion_required,
    } = (Work::Inspect {
        decoded: Decoded::Session(bundle),
        target,
        mapping: vec![],
        update_length: false,
    })
    .run()
    .unwrap()
    else {
        panic!()
    };
    assert!(!conversion_required);
    let Value::Image(image) = (Work::Prepare {
        decoded,
        mapping: vec![],
        update_length: false,
    })
    .run()
    .unwrap() else {
        panic!()
    };
    let image::Manifest::Session(restored) = &image.manifest else {
        panic!()
    };
    let image::Manifest::Session(original) = &original.manifest else {
        panic!()
    };
    assert_eq!(restored.tracks, original.tracks);
    for content in &restored.tracks[0].loops {
        if let Some(composite) = &content.composite {
            assert_eq!(composite.prepared.quantum, 32);
        }
    }
}

pub(super) fn session(content: image::Loop, channels: u32, midi: bool) -> image::Session {
    image::Session {
        sample_rate: 48000,
        global: GlobalControlState::default(),
        tracks: vec![image::Track {
            topology: shoop_app_api::TrackTopology::Direct,
            default_playback_mode: Default::default(),
            processor_state: None,
            builtin_fx_midi_cc_assignments: Vec::new(),
            oxisynth_midi_cc_assignments: Vec::new(),
            id: TrackId::from_raw(1),
            name: "Track".into(),
            port_name_base: "track".into(),
            sync: false,
            audio_channels: channels,
            midi,
            ports: vec![],
            loops: vec![content.clone()],
            controls: image::Controls {
                input_gain_db: 0.,
                input_balance: 0.,
                monitoring: false,
                output_gain_db: 0.,
                output_balance: 0.,
                muted: false,
            },
        }],
        buses: vec![image::Bus {
            fx: None,
            id: BusId::from_raw(1),
            name: "Master".into(),
            gain_db: 0.,
            balance: 0.,
            muted: false,
            channels: ["Left", "Right"]
                .into_iter()
                .enumerate()
                .map(|(index, label)| image::BusChannel {
                    id: BusChannelId::from_raw(index as u64 + 1),
                    label: label.into(),
                    port: image::Port {
                        state: ApplicationPortState {
                            publication: None,
                            id: PortId::from_raw(index as u64 + 1),
                            owner: ApplicationPortOwner::Bus {
                                bus_id: BusId::from_raw(1),
                            },
                            name: label.into(),
                            data_type: PortDataType::Audio,
                            direction: PortDirection::Output,
                            role: PortRole::AudioOutput,
                            connection_policy: ConnectionPolicy::OwnerManaged,
                        },
                        external_connections: vec![],
                    },
                })
                .collect(),
        }],
        bus_display_order: vec![BusId::from_raw(1)],
        preview_bus: None,
        global_ports: vec![],
        routes: vec![],
        selected: vec![],
        target: None,
    }
}

#[shoop_test]
fn session_and_audio_files_round_trip_pinned_shared_audio_then_reload_into_editable_pcm() {
    use rt_runtime::{
        resource::{shared_audio, ResourceStore},
        Config, Frame, Revision,
    };
    let mut resources = ResourceStore::new(
        Config {
            max_frames: 8,
            resource_pages: 160,
            resource_slots: 24,
            page_bytes: 24,
            ..Config::default()
        }
        .prepare()
        .unwrap(),
        rt_runtime::Quantum::new(1).unwrap(),
    )
    .unwrap();
    let audio = resources
        .prepare_audio_store(
            shared_audio::Config {
                channels: 2,
                history_frames: 4,
                views: 16,
                pages: 140,
            },
            Frame(0),
        )
        .unwrap();
    resources.begin_callback();
    resources
        .render_io()
        .append_audio_history(audio, Frame(0), &[1., -1., 2., -2., 3., -3., 4., -4.])
        .unwrap();
    let editable = resources.grab_audio(audio, Frame(1)..Frame(4)).unwrap();
    let old = resources.snapshot(editable, Frame(4)).unwrap();
    resources
        .render_io()
        .replace_audio(editable, Revision(1), 2, &[9., -9., 8., -8.])
        .unwrap();
    let changed = resources.snapshot(editable, Frame(4)).unwrap();
    resources.remove(editable).unwrap();

    for (snapshot, expected) in [
        (old, [2., -2., 3., -3., 4., -4.]),
        (changed, [8., -8., 3., -3., 9., -9.]),
    ] {
        let mut cursor = resources.cursor(snapshot.id).unwrap();
        let mut bytes = [0; 24];
        let mut filled = 0;
        while filled < bytes.len() {
            resources.begin_callback();
            let count = resources
                .read(&mut cursor, &mut bytes[filled..(filled + 7).min(24)])
                .unwrap();
            filled += count;
        }
        let samples: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert_eq!(samples, expected);
        let mut content = content();
        content.frames = 3;
        content.audio = vec![channel(1, 3, 0), channel(2, 3, 0)];
        let payloads = (0..2)
            .map(|c| {
                image::Payload::Audio(Arc::new(
                    samples.iter().skip(c).step_by(2).copied().collect(),
                ))
            })
            .collect::<Vec<_>>();
        let session = session(content.clone(), 2, false);
        let captured = Arc::new(image::Image {
            manifest: image::Manifest::Session(session),
            payloads: payloads.clone(),
        });
        let Value::File { bytes, .. } = (Work::Encode {
            image: captured,
            kind: IoTaskKind::SaveSession,
            wav: false,
            standard: false,
            channels: vec![],
        })
        .run()
        .unwrap() else {
            panic!("session file");
        };
        let Value::Decoded(decoded) = (Work::Decode {
            kind: IoTaskKind::LoadSession,
            bytes,
            rate: 48000,
        })
        .run()
        .unwrap() else {
            panic!("decoded session");
        };
        let Value::Image(restored) = (Work::Prepare {
            decoded,
            mapping: vec![],
            update_length: false,
        })
        .run()
        .unwrap() else {
            panic!("prepared session");
        };
        assert_eq!(restored.payloads, payloads);
        let image::Manifest::Session(session) = &restored.manifest else {
            panic!("session");
        };
        assert_eq!(session.tracks[0].loops[0].gain, content.gain);

        for wav in [false, true] {
            let Value::File { bytes, .. } = (Work::Encode {
                image: Arc::new(image::Image {
                    manifest: image::Manifest::Loop {
                        sample_rate: 48000,
                        content: content.clone(),
                    },
                    payloads: payloads.clone(),
                }),
                kind: IoTaskKind::ExportLoopAudio,
                wav,
                standard: false,
                channels: vec![0, 1],
            })
            .run()
            .unwrap() else {
                panic!("audio file");
            };
            let Value::Decoded(decoded) = (Work::Decode {
                kind: IoTaskKind::ImportLoopAudio,
                bytes,
                rate: 48000,
            })
            .run()
            .unwrap() else {
                panic!("decoded audio");
            };
            let Value::Image(loaded) = (Work::Prepare {
                decoded,
                mapping: vec![0, 1],
                update_length: true,
            })
            .run()
            .unwrap() else {
                panic!("prepared audio");
            };
            assert_eq!(loaded.payloads, payloads);
        }

        let loaded = resources.create().unwrap();
        let data: Vec<_> = samples.iter().copied().flat_map(f32::to_le_bytes).collect();
        let upload = resources
            .begin_upload(loaded, Revision(0), snapshot.description)
            .unwrap();
        resources.begin_callback();
        resources.write_upload(upload, 0, &data).unwrap();
        resources.finish_upload(upload).unwrap();
        resources.commit(upload).unwrap();
        let loaded_snapshot = resources.snapshot(loaded, Frame(0)).unwrap();
        let cow = resources.fork_audio(audio, loaded_snapshot.id).unwrap();
        resources
            .render_io()
            .replace_audio(cow, Revision(1), 0, &[7., -7.])
            .unwrap();
        let mut cursor = resources.cursor(loaded_snapshot.id).unwrap();
        resources.begin_callback();
        let mut original = [0; 24];
        assert_eq!(resources.read(&mut cursor, &mut original).unwrap(), 24);
        assert_eq!(original.as_slice(), data);
        resources.release_snapshot(loaded_snapshot.id).unwrap();
        resources.release_snapshot(snapshot.id).unwrap();
        resources.remove(cow).unwrap();
        resources.remove(loaded).unwrap();
    }
    for _ in 0..1024 {
        resources.begin_callback();
        resources.reclaim();
    }
    resources.cleanup_audio_store(audio).unwrap();
    for _ in 0..1024 {
        resources.begin_callback();
        resources.reclaim();
    }
    assert_eq!(resources.usage().free_pages, 160);
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
fn role_exports_preserve_selected_order_labels_and_map_all_sixteen_destinations() {
    let mut content = content();
    content.audio = (0..16)
        .map(|index| {
            let mut channel = channel(index + 1, 2, -1);
            channel.mode = if index < 8 {
                image::ChannelMode::Dry
            } else {
                image::ChannelMode::Wet
            };
            channel
        })
        .collect();
    let image = Arc::new(image::Image {
        manifest: image::Manifest::Loop {
            sample_rate: 48000,
            content,
        },
        payloads: (0..16)
            .map(|index| image::Payload::Audio(Arc::new(vec![index as f32 + 0.25; 2])))
            .collect(),
    });
    let Value::File { bytes, .. } = (Work::Encode {
        image,
        kind: IoTaskKind::ExportLoopAudio,
        wav: false,
        standard: false,
        channels: vec![15, 0, 9],
    })
    .run()
    .unwrap() else {
        panic!()
    };
    let audio = decode_loop_audio(&bytes).unwrap();
    assert_eq!(
        audio
            .channels
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        ["Wet 8", "Dry 1", "Wet 2"]
    );
    assert_eq!(
        audio
            .channels
            .iter()
            .map(|c| c.role.as_str())
            .collect::<Vec<_>>(),
        ["wet", "dry", "wet"]
    );
    assert_eq!(audio.channels[0].samples, [0., 15.25, 15.25, 0.]);
    let selected = Decoded::Audio(audio)
        .select_audio(&(0..16).map(|i| i % 3).collect::<Vec<_>>())
        .unwrap();
    let Decoded::Audio(selected) = selected else {
        panic!()
    };
    assert_eq!(selected.channels.len(), 16);
    assert_eq!(selected.channels[15].samples, selected.channels[0].samples);
    assert!(Decoded::Audio(selected).select_audio(&[0; 17]).is_err());
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

#[shoop_test]
fn default_grab_policy_survives_the_file_and_application_image_round_trip() {
    let mut bundle = SessionBundle::new(SessionDocument::empty(1000));
    bundle.document.global.default_recording_action = shoop_session::RecordingActionDocument::Grab;
    let bytes = encode_session(&bundle, "grab-policy").unwrap();
    let image = crate::document::from_bundle(decode_session(&bytes).unwrap()).unwrap();
    let image::Manifest::Session(session) = &image.manifest else {
        panic!("session")
    };
    assert_eq!(
        session.global.default_recording_action,
        DefaultRecordingAction::Grab
    );
    let restored = crate::document::to_bundle(&image).unwrap();
    assert_eq!(
        restored.document.global.default_recording_action,
        shoop_session::RecordingActionDocument::Grab
    );
}
