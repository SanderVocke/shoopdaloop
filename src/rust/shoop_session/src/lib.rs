mod archive;
mod click_track;
mod document;
mod media;
mod resample;

pub use archive::{
    decode_session, decode_session_with_limits, encode_session, validate_bundle, DecodeLimits,
    SessionError,
};
pub use click_track::{
    click_sound_ids, generate_audio_click_track, generate_click_track_timing,
    generate_midi_click_track, AudioClickTrackSpec, ClickTrackError, ClickTrackTiming,
    ClickTrackTimingSpec, MidiClickTrackSpec, MAX_CLICK_TRACK_CLICKS, MAX_CLICK_TRACK_FRAMES,
    MAX_CLICK_TRACK_MIDI_EVENTS,
};
pub use document::*;
pub use media::{
    decode_exact_midi, decode_loop_audio, decode_standard_midi, decode_wav, encode_exact_midi,
    encode_float_wav, encode_loop_audio, encode_standard_midi, StandardMidiExport,
};
pub use resample::{
    resample_exact_midi, resample_loop_audio, resample_session, scale_duration, scale_nearest,
    scale_signed_nearest,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::io::{Cursor, Read, Write};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipArchive, ZipWriter};

    fn direct_bundle(channels: usize) -> SessionBundle {
        let sample_rate = 48_000;
        let mut media = BTreeMap::new();
        let mut loop_channels = Vec::new();
        let mut ports = Vec::new();
        for index in 0..channels {
            let media_id = format!("audio_{index}");
            let samples = vec![
                f32::from_bits(index as u32),
                0.25 + index as f32,
                -0.5 - index as f32,
            ];
            media.insert(
                media_id.clone(),
                MediaPayload::Audio(AudioPayload { samples }),
            );
            let input_id = 1000 + index as u64 * 2;
            let output_id = input_id + 1;
            ports.push(PortDocument {
                id: input_id,
                name: format!("input {index}"),
                data_type: DataTypeDocument::Audio,
                direction: PortDirectionDocument::Input,
                role: PortRoleDocument::AudioInput,
                input_connectability: vec![ConnectabilityDocument::External],
                output_connectability: vec![ConnectabilityDocument::Internal],
                gain: 1.0,
                muted: false,
                passthrough_muted: false,
                internal_connections: vec![output_id],
                external_connections: vec![format!("system:capture_{}", index + 1)],
                ringbuffer_frames: 96_000,
            });
            ports.push(PortDocument {
                id: output_id,
                name: format!("output {index}"),
                data_type: DataTypeDocument::Audio,
                direction: PortDirectionDocument::Output,
                role: PortRoleDocument::AudioOutput,
                input_connectability: vec![ConnectabilityDocument::Internal],
                output_connectability: vec![ConnectabilityDocument::External],
                gain: 1.0,
                muted: false,
                passthrough_muted: false,
                internal_connections: Vec::new(),
                external_connections: vec![format!("system:playback_{}", index + 1)],
                ringbuffer_frames: 0,
            });
            loop_channels.push(ChannelDocument {
                id: 100 + index as u64,
                mode: ChannelModeDocument::Direct,
                data_type: DataTypeDocument::Audio,
                data_length_frames: 3,
                start_offset_frames: -1,
                preplay_frames: 2,
                gain: 1.0,
                connected_port_ids: vec![input_id, output_id],
                media_id: Some(media_id),
                recording_started_at: None,
                recording_fx_state_id: Some(900),
            });
        }
        let midi_id = "midi_main".to_owned();
        let exact_midi = ExactMidi {
            sample_rate,
            length_frames: 301,
            start_state: vec![vec![0xB0, 7, 100]],
            events: vec![
                ExactMidiEvent {
                    frame: 101,
                    order: 0,
                    data: vec![0x90, 60, 100],
                },
                ExactMidiEvent {
                    frame: 201,
                    order: 1,
                    data: vec![0x80, 60, 0],
                },
            ],
        };
        media.insert(midi_id.clone(), MediaPayload::Midi(exact_midi));
        loop_channels.push(ChannelDocument {
            id: 999,
            mode: ChannelModeDocument::Direct,
            data_type: DataTypeDocument::Midi,
            data_length_frames: 301,
            start_offset_frames: 0,
            preplay_frames: 0,
            gain: 1.0,
            connected_port_ids: Vec::new(),
            media_id: Some(midi_id),
            recording_started_at: None,
            recording_fx_state_id: None,
        });
        let document = SessionDocument {
            sample_rate,
            connection_model_version: CONNECTION_MODEL_VERSION,
            global: GlobalControlsDocument {
                default_recording_action: RecordingActionDocument::Grab,
                play_after_record: true,
                sync: true,
                solo: true,
                apply_n_cycles: 4,
            },
            track_groups: vec![TrackGroupDocument {
                name: "main".to_owned(),
                tracks: vec![TrackDocument {
                    id: 1,
                    name: "Arbitrary".to_owned(),
                    port_name_base: "arbitrary".to_owned(),
                    is_sync: false,
                    width: Some(123.0),
                    topology: TrackTopologyDocument::Direct {
                        audio_channels: channels as u32,
                        midi: true,
                    },
                    controls: TrackControlsDocument {
                        output_gain_db: -3.0,
                        output_balance: 0.25,
                        output_muted: false,
                        input_gain_db: 2.0,
                        input_balance: -0.5,
                        input_monitoring: true,
                    },
                    loops: vec![LoopDocument {
                        id: 10,
                        name: "Loop".to_owned(),
                        length_frames: 301,
                        is_sync: false,
                        gain: 0.75,
                        balance: 0.1,
                        channels: loop_channels,
                        composite: None,
                    }],
                    ports,
                    fx_chain: Some(FxChainDocument {
                        id: 800,
                        title: "FX".to_owned(),
                        chain_type: FxChainTypeDocument::CarlaPatchbay16x,
                        ports: Vec::new(),
                        internal_state: "{\"opaque\":\"å\\u0000state\"}".to_owned(),
                    }),
                }],
            }],
            selected_loop_ids: vec![10],
            targeted_loop_id: Some(10),
            buses: Vec::new(),
            global_ports: Vec::new(),
            fx_states: vec![FxStateDocument {
                id: 900,
                chain_type: FxChainTypeDocument::CarlaPatchbay16x,
                internal_state: "carla\nstate\0bytes".to_owned(),
            }],
            scripts: vec![ScriptDocument {
                id: 700,
                name: "script".to_owned(),
                source: "return 1".to_owned(),
                enabled: true,
            }],
            midi_control: MidiControlDocument {
                bindings: vec![MidiBindingDocument {
                    id: 600,
                    message: vec![0x90, 1, 127],
                    action: "play".to_owned(),
                    target_id: Some(10),
                }],
            },
            settings: vec![SessionSettingDocument {
                key: "example".to_owned(),
                value: SettingValueDocument::String("value".to_owned()),
            }],
        };
        SessionBundle { document, media }
    }

    fn tiny_synth_fx_bundle() -> SessionBundle {
        let mut bundle = direct_bundle(0);
        let track = &mut bundle.document.track_groups[0].tracks[0];
        track.name = "Tiny Synth/FX".to_owned();
        track.port_name_base = "tiny".to_owned();
        track.topology = TrackTopologyDocument::TinySynthFx { audio_channels: 0 };
        track.fx_chain = Some(FxChainDocument {
            id: 800,
            title: "Tiny Synth/FX".to_owned(),
            chain_type: FxChainTypeDocument::TinySynthFx,
            ports: Vec::new(),
            internal_state: "shoop-tiny-synth-fx:1:c0c00000:VEFT".to_owned(),
        });
        let midi = &mut track.loops[0].channels[0];
        midi.mode = ChannelModeDocument::Dry;
        midi.recording_fx_state_id = Some(900);
        bundle.document.fx_states[0] = FxStateDocument {
            id: 900,
            chain_type: FxChainTypeDocument::TinySynthFx,
            internal_state: "shoop-tiny-synth-fx:1:c1000000:VEFT".to_owned(),
        };
        bundle
    }

    fn deferred_feature_bundle() -> SessionBundle {
        let mut bundle = direct_bundle(2);
        bundle.document.track_groups[0].tracks.extend([
            TrackDocument {
                id: 2,
                name: "Dry/wet".to_owned(),
                port_name_base: "dry_wet".to_owned(),
                is_sync: false,
                width: Some(144.0),
                topology: TrackTopologyDocument::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                controls: TrackControlsDocument::default(),
                loops: vec![LoopDocument {
                    id: 20,
                    name: "Deferred primitive".to_owned(),
                    length_frames: 960,
                    is_sync: false,
                    gain: 0.8,
                    balance: -0.25,
                    channels: vec![
                        ChannelDocument {
                            id: 200,
                            mode: ChannelModeDocument::Dry,
                            data_type: DataTypeDocument::Audio,
                            data_length_frames: 0,
                            start_offset_frames: -24,
                            preplay_frames: 48,
                            gain: 0.9,
                            connected_port_ids: Vec::new(),
                            media_id: None,
                            recording_started_at: Some("fixture-take".to_owned()),
                            recording_fx_state_id: None,
                        },
                        ChannelDocument {
                            id: 201,
                            mode: ChannelModeDocument::Wet,
                            data_type: DataTypeDocument::Audio,
                            data_length_frames: 0,
                            start_offset_frames: 12,
                            preplay_frames: 24,
                            gain: 0.7,
                            connected_port_ids: Vec::new(),
                            media_id: None,
                            recording_started_at: None,
                            recording_fx_state_id: None,
                        },
                        ChannelDocument {
                            id: 202,
                            mode: ChannelModeDocument::Dry,
                            data_type: DataTypeDocument::Midi,
                            data_length_frames: 0,
                            start_offset_frames: 0,
                            preplay_frames: 0,
                            gain: 1.0,
                            connected_port_ids: Vec::new(),
                            media_id: None,
                            recording_started_at: None,
                            recording_fx_state_id: None,
                        },
                    ],
                    composite: None,
                }],
                ports: Vec::new(),
                fx_chain: None,
            },
            TrackDocument {
                id: 3,
                name: "Composite".to_owned(),
                port_name_base: "composite".to_owned(),
                is_sync: false,
                width: None,
                topology: TrackTopologyDocument::Trigger,
                controls: TrackControlsDocument::default(),
                loops: vec![LoopDocument {
                    id: 30,
                    name: "Script composite".to_owned(),
                    length_frames: 1_920,
                    is_sync: false,
                    gain: 1.0,
                    balance: 0.0,
                    channels: Vec::new(),
                    composite: Some(CompositeDocument {
                        kind: CompositeKindDocument::Script,
                        playlists: vec![vec![vec![CompositeEventDocument {
                            delay_frames: 240,
                            loop_id: 10,
                            mode: Some("playing".to_owned()),
                            n_cycles: Some(2),
                        }]]],
                    }),
                }],
                ports: Vec::new(),
                fx_chain: None,
            },
            TrackDocument {
                id: 4,
                name: "Carla".to_owned(),
                port_name_base: "carla".to_owned(),
                is_sync: false,
                width: Some(180.0),
                topology: TrackTopologyDocument::Carla {
                    chain_type: FxChainTypeDocument::CarlaRack,
                    audio_channels: 16,
                    midi: true,
                    dry_audio_channels: None,
                    wet_audio_channels: None,
                },
                controls: TrackControlsDocument::default(),
                loops: Vec::new(),
                ports: Vec::new(),
                fx_chain: Some(FxChainDocument {
                    id: 801,
                    title: "Deferred Carla rack".to_owned(),
                    chain_type: FxChainTypeDocument::CarlaRack,
                    ports: Vec::new(),
                    internal_state: "opaque\0carla\nstate".to_owned(),
                }),
            },
        ]);
        bundle.document.selected_loop_ids = vec![10, 20, 30];
        bundle.document.targeted_loop_id = Some(30);
        bundle.document.buses = vec![BusDocument {
            id: 5_000,
            name: "Main bus".to_owned(),
            ports: Vec::new(),
            fx_chain: Some(FxChainDocument {
                id: 802,
                title: "Bus FX".to_owned(),
                chain_type: FxChainTypeDocument::Test,
                ports: Vec::new(),
                internal_state: "bus-state".to_owned(),
            }),
        }];
        bundle.document.global_ports = vec![PortDocument {
            id: 5_001,
            name: "global midi".to_owned(),
            data_type: DataTypeDocument::Midi,
            direction: PortDirectionDocument::Input,
            role: PortRoleDocument::MidiInput,
            input_connectability: vec![ConnectabilityDocument::External],
            output_connectability: vec![ConnectabilityDocument::Internal],
            gain: 1.0,
            muted: false,
            passthrough_muted: true,
            internal_connections: Vec::new(),
            external_connections: vec!["controller:out".to_owned()],
            ringbuffer_frames: 4_800,
        }];
        bundle.document.settings.extend([
            SessionSettingDocument {
                key: "bool".to_owned(),
                value: SettingValueDocument::Bool(true),
            },
            SessionSettingDocument {
                key: "integer".to_owned(),
                value: SettingValueDocument::Integer(-3),
            },
            SessionSettingDocument {
                key: "number".to_owned(),
                value: SettingValueDocument::Number(1.25),
            },
            SessionSettingDocument {
                key: "list".to_owned(),
                value: SettingValueDocument::StringList(vec!["a".to_owned(), "b".to_owned()]),
            },
        ]);
        bundle
    }

    fn rewrite_manifest_major(bytes: Vec<u8>, major: u16) -> Vec<u8> {
        let mut input = ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut output = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut payload = Vec::new();
            entry.read_to_end(&mut payload).unwrap();
            if name == "manifest.json" {
                let mut manifest: serde_json::Value = serde_json::from_slice(&payload).unwrap();
                manifest["format_version"]["major"] = serde_json::json!(major);
                payload = serde_json::to_vec(&manifest).unwrap();
            }
            output.start_file(name, options).unwrap();
            output.write_all(&payload).unwrap();
        }
        output.finish().unwrap().into_inner()
    }

    #[test]
    fn tiny_synth_fx_current_and_recorded_state_round_trip_and_validate_shape() {
        let bundle = tiny_synth_fx_bundle();
        let encoded = encode_session(&bundle, "tiny-test").unwrap();
        assert_eq!(decode_session(&encoded).unwrap(), bundle);

        let mut mismatched = bundle.clone();
        mismatched.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .chain_type = FxChainTypeDocument::CarlaRack;
        assert!(matches!(
            validate_bundle(&mismatched),
            Err(SessionError::Validation(message))
                if message.contains("chain type does not match")
        ));

        let mut missing_midi = bundle;
        missing_midi.document.track_groups[0].tracks[0].loops[0]
            .channels
            .clear();
        assert!(matches!(
            validate_bundle(&missing_midi),
            Err(SessionError::Validation(message))
                if message.contains("channel shape does not match")
        ));
    }

    #[test]
    fn minimal_session_fixture_round_trips() {
        let bundle = SessionBundle::new(SessionDocument::empty(48_000));
        let encoded = encode_session(&bundle, "minimal-fixture").unwrap();
        assert_eq!(decode_session(&encoded).unwrap(), bundle);
    }

    #[test]
    fn unsupported_older_and_future_major_archives_are_rejected() {
        let encoded = encode_session(
            &SessionBundle::new(SessionDocument::empty(48_000)),
            "version-fixture",
        )
        .unwrap();
        for major in [0, FORMAT_MAJOR + 1] {
            let archive = rewrite_manifest_major(encoded.clone(), major);
            assert!(matches!(
                decode_session(&archive),
                Err(SessionError::UnsupportedVersion {
                    major: actual,
                    ..
                }) if actual == major
            ));
        }
    }

    #[test]
    fn session_round_trip_is_exact_and_deterministic() {
        let bundle = direct_bundle(12);
        let first = encode_session(&bundle, "test").unwrap();
        let second = encode_session(&bundle, "test").unwrap();
        assert_eq!(first, second);
        let decoded = decode_session(&first).unwrap();
        assert_eq!(decoded, bundle);
        assert_eq!(
            decoded.document.fx_states[0].internal_state.as_bytes(),
            bundle.document.fx_states[0].internal_state.as_bytes()
        );
        for (id, payload) in &bundle.media {
            if let MediaPayload::Audio(expected) = payload {
                let MediaPayload::Audio(actual) = &decoded.media[id] else {
                    panic!("wrong payload type")
                };
                assert_eq!(
                    actual
                        .samples
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    expected
                        .samples
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn deferred_feature_fixture_round_trips_without_field_loss() {
        let bundle = deferred_feature_bundle();
        let encoded = encode_session(&bundle, "deferred-fixture").unwrap();
        let decoded = decode_session(&encoded).unwrap();
        assert_eq!(decoded, bundle);
        assert_eq!(
            decoded.document.track_groups[0].tracks[3]
                .fx_chain
                .as_ref()
                .unwrap()
                .internal_state
                .as_bytes(),
            b"opaque\0carla\nstate"
        );
        assert_eq!(
            decoded.document.track_groups[0].tracks[2].loops[0]
                .composite
                .as_ref()
                .unwrap()
                .playlists[0][0][0]
                .delay_frames,
            240
        );
        assert!(matches!(
            decoded.document.track_groups[0].tracks[3].topology,
            TrackTopologyDocument::Carla {
                chain_type: FxChainTypeDocument::CarlaRack,
                audio_channels: 16,
                midi: true,
                ..
            }
        ));
    }

    #[test]
    fn arbitrary_channel_loop_audio_round_trips() {
        let audio = LoopAudio {
            sample_rate: 96_000,
            channels: (0..300)
                .map(|index| LoopAudioChannel {
                    label: format!("channel {index}"),
                    role: "direct".to_owned(),
                    samples: vec![index as f32, -(index as f32)],
                })
                .collect(),
        };
        let encoded = encode_loop_audio(&audio).unwrap();
        assert_eq!(decode_loop_audio(&encoded).unwrap(), audio);
    }

    #[test]
    fn exact_and_standard_midi_preserve_order_and_bound_quantization() {
        let midi = ExactMidi {
            sample_rate: 48_000,
            length_frames: 1000,
            start_state: vec![vec![0xB0, 1, 2]],
            events: vec![
                ExactMidiEvent {
                    frame: 101,
                    order: 0,
                    data: vec![0x90, 60, 100],
                },
                ExactMidiEvent {
                    frame: 101,
                    order: 1,
                    data: vec![0x80, 60, 0],
                },
            ],
        };
        let exact = encode_exact_midi(&midi).unwrap();
        assert_eq!(decode_exact_midi(&exact).unwrap(), midi);
        let standard = encode_standard_midi(&midi).unwrap();
        assert!(standard.max_quantization_error_frames <= 48_000.0 / (2.0 * 7650.0));
        let imported = decode_standard_midi(&standard.bytes, 48_000).unwrap();
        assert_eq!(imported.events.len(), 3);
        assert!(imported
            .events
            .windows(2)
            .all(|events| events[0].frame <= events[1].frame));
    }

    #[test]
    fn standard_midi_import_honors_tempo_maps_stable_tracks_and_sysex() {
        use midly::num::{u15, u24, u28, u4, u7};
        use midly::{
            Format, Header, MetaMessage, MidiMessage, Smf, Timing, TrackEvent, TrackEventKind,
        };

        let tempo_track = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000))),
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(1_000_000))),
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let first_data_track = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOn {
                        key: u7::new(60),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(240),
                kind: TrackEventKind::SysEx(&[0x7d, 0x01, 0xf7]),
            },
            TrackEvent {
                delta: u28::new(720),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOff {
                        key: u7::new(60),
                        vel: u7::new(0),
                    },
                },
            },
        ];
        let second_data_track = vec![TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Midi {
                channel: u4::new(1),
                message: MidiMessage::NoteOn {
                    key: u7::new(62),
                    vel: u7::new(90),
                },
            },
        }];
        let smf = Smf {
            header: Header {
                format: Format::Parallel,
                timing: Timing::Metrical(u15::new(480)),
            },
            tracks: vec![tempo_track, first_data_track, second_data_track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        let imported = decode_standard_midi(&bytes, 48_000).unwrap();
        assert_eq!(imported.events.len(), 4);
        assert_eq!(imported.events[0].frame, 0);
        assert_eq!(imported.events[0].data[0], 0x90);
        assert_eq!(imported.events[1].frame, 0);
        assert_eq!(imported.events[1].data[0], 0x91);
        assert_eq!(imported.events[2].frame, 12_000);
        assert_eq!(imported.events[2].data, vec![0xf0, 0x7d, 0x01, 0xf7]);
        assert_eq!(imported.events[3].frame, 72_000);
        assert!(imported.length_frames >= 72_001);
    }

    #[test]
    fn float_wav_round_trip_is_exact() {
        let audio = LoopAudio {
            sample_rate: 44_100,
            channels: vec![
                LoopAudioChannel {
                    label: "left".to_owned(),
                    role: "direct".to_owned(),
                    samples: vec![0.0, 0.25, -0.5],
                },
                LoopAudioChannel {
                    label: "right".to_owned(),
                    role: "direct".to_owned(),
                    samples: vec![-0.0, 0.75, -1.0],
                },
            ],
        };
        let bytes = encode_float_wav(&audio).unwrap();
        let decoded = decode_wav(&bytes).unwrap();
        assert_eq!(decoded.sample_rate, audio.sample_rate);
        assert_eq!(decoded.channels.len(), 2);
        for (actual, expected) in decoded.channels.iter().zip(&audio.channels) {
            assert_eq!(
                actual
                    .samples
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>(),
                expected
                    .samples
                    .iter()
                    .map(|v| v.to_bits())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn resampling_converts_every_sample_domain_and_preserves_midi_order() {
        let bundle = direct_bundle(2);
        for rate in [44_100, 32_000, 96_000] {
            let converted = resample_session(&bundle, rate).unwrap();
            assert_eq!(converted.document.sample_rate, rate);
            assert_eq!(
                converted.document.track_groups[0].tracks[0].loops[0].channels[0]
                    .data_length_frames,
                scale_duration(3, 48_000, rate).unwrap()
            );
        }
        let converted = resample_session(&bundle, 32_000).unwrap();
        assert_eq!(converted.document.sample_rate, 32_000);
        let track = &converted.document.track_groups[0].tracks[0];
        assert_eq!(track.ports[0].ringbuffer_frames, 64_000);
        let loop_ = &track.loops[0];
        assert_eq!(loop_.length_frames, 201);
        assert_eq!(loop_.channels[0].data_length_frames, 2);
        assert_eq!(loop_.channels[0].start_offset_frames, -1);
        assert_eq!(loop_.channels[0].preplay_frames, 2);
        let MediaPayload::Midi(midi) = &converted.media["midi_main"] else {
            panic!("MIDI payload missing")
        };
        assert_eq!(midi.length_frames, 201);
        assert_eq!(midi.events[0].frame, 67);
        assert_eq!(midi.events[1].frame, 134);
        assert_eq!(midi.start_state, vec![vec![0xB0, 7, 100]]);
    }

    #[test]
    fn payload_hash_mismatch_is_rejected() {
        let encoded = encode_session(&direct_bundle(2), "hash-test").unwrap();
        let mut input = ZipArchive::new(Cursor::new(encoded)).unwrap();
        let mut output = ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut corrupted = false;
        for index in 0..input.len() {
            let mut entry = input.by_index(index).unwrap();
            let name = entry.name().to_owned();
            let mut payload = Vec::new();
            entry.read_to_end(&mut payload).unwrap();
            if !corrupted && name.starts_with("media/audio/") {
                payload[0] ^= 1;
                corrupted = true;
            }
            output.start_file(name, options).unwrap();
            output.write_all(&payload).unwrap();
        }
        assert!(corrupted);
        let bytes = output.finish().unwrap().into_inner();
        assert!(matches!(
            decode_session(&bytes),
            Err(SessionError::HashMismatch { .. })
        ));
    }

    #[test]
    fn old_non_zip_and_resource_limit_fail_without_decoding() {
        assert!(matches!(
            decode_session(b"old qml tar bytes"),
            Err(SessionError::UnsupportedFormat)
        ));
        let bytes = encode_session(&direct_bundle(1), "test").unwrap();
        assert!(matches!(
            decode_session_with_limits(
                &bytes,
                DecodeLimits {
                    max_entries: 1,
                    max_uncompressed_bytes: u64::MAX,
                }
            ),
            Err(SessionError::ResourceLimit(_))
        ));
    }

    #[test]
    fn topology_channel_shapes_and_fx_state_references_are_validated() {
        let mut wrong_mode = direct_bundle(1);
        wrong_mode.document.track_groups[0].tracks[0].loops[0].channels[0].mode =
            ChannelModeDocument::Wet;
        assert!(matches!(
            validate_bundle(&wrong_mode),
            Err(SessionError::Validation(message))
                if message.contains("channel shape")
        ));

        let mut missing_state = direct_bundle(1);
        missing_state.document.track_groups[0].tracks[0].loops[0].channels[0]
            .recording_fx_state_id = Some(123_456);
        assert!(matches!(
            validate_bundle(&missing_state),
            Err(SessionError::Validation(message))
                if message.contains("missing FX state")
        ));

        let mut wrong_state_type = direct_bundle(1);
        wrong_state_type.document.fx_states[0].chain_type = FxChainTypeDocument::CarlaRack;
        assert!(matches!(
            validate_bundle(&wrong_state_type),
            Err(SessionError::Validation(message))
                if message.contains("does not match")
        ));
    }

    #[test]
    fn stale_references_and_missing_media_are_rejected() {
        let mut bundle = direct_bundle(1);
        bundle.document.selected_loop_ids = vec![999_999];
        assert!(matches!(
            encode_session(&bundle, "test"),
            Err(SessionError::Validation(_))
        ));
        let mut bundle = direct_bundle(1);
        bundle.media.remove("audio_0");
        assert!(matches!(
            encode_session(&bundle, "test"),
            Err(SessionError::MissingMedia { .. })
        ));
    }
}
