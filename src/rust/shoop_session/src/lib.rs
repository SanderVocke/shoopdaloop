#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

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
                capture_alignment_frames: 0,
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
            capture_alignment_frames: 0,
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
                auto_mute_other_track_inputs: true,
                auto_arm_track_inputs: false,
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
                    default_playback_mode: DefaultPlaybackModeDocument::Regular,
                    controls: TrackControlsDocument {
                        output_gain_db: -3.0,
                        output_balance: 0.25,
                        output_muted: false,
                        input_gain_db: 2.0,
                        input_balance: -0.5,
                        input_monitoring: true,
                    },
                    latency: TrackLatencyDocument::default(),
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
                        midi_cc_assignments: Vec::new(),
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
                entrypoint: "main.lua".to_owned(),
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
        let scripts = BTreeMap::from([(
            700,
            std::sync::Arc::new(
                shoop_script_resources::ScriptResourceBundle::source_only(
                    "main.lua",
                    std::sync::Arc::<[u8]>::from(&b"return 1"[..]),
                )
                .unwrap(),
            ),
        )]);
        SessionBundle {
            document,
            media,
            scripts,
        }
    }

    fn oxisynth_bundle() -> SessionBundle {
        let mut bundle = direct_bundle(2);
        let track = &mut bundle.document.track_groups[0].tracks[0];
        track.name = "OxiSynth".to_owned();
        track.port_name_base = "oxisynth".to_owned();
        track.topology = TrackTopologyDocument::OxiSynth;
        track.fx_chain = Some(FxChainDocument {
            id: 800,
            title: "OxiSynth".to_owned(),
            chain_type: FxChainTypeDocument::OxiSynth,
            ports: Vec::new(),
            internal_state: "shoop-oxisynth:2:timgm6mb:0:40:00000000:00000000".to_owned(),
            midi_cc_assignments: Vec::new(),
        });
        let channels = &mut track.loops[0].channels;
        for channel in channels.iter_mut() {
            channel.mode = ChannelModeDocument::Dry;
            channel.recording_fx_state_id = None;
        }
        let wet = channels[..2]
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, mut channel)| {
                channel.id = 200 + index as u64;
                channel.mode = ChannelModeDocument::Wet;
                channel.connected_port_ids.clear();
                channel
            })
            .collect::<Vec<_>>();
        channels.splice(2..2, wet);
        bundle.document.fx_states.clear();
        bundle
    }

    fn builtin_fx_bundle() -> SessionBundle {
        let mut bundle = oxisynth_bundle();
        let track = &mut bundle.document.track_groups[0].tracks[0];
        track.name = "Built-in FX".to_owned();
        track.port_name_base = "builtin_fx".to_owned();
        track.topology = TrackTopologyDocument::BuiltInFx;
        let chain = track.fx_chain.as_mut().unwrap();
        chain.title = "Built-in FX".to_owned();
        chain.chain_type = FxChainTypeDocument::BuiltInFx;
        chain.internal_state = "shoop-builtin-fx:1:0".to_owned();
        track.loops[0]
            .channels
            .retain(|channel| channel.data_type == DataTypeDocument::Audio);
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
                default_playback_mode: DefaultPlaybackModeDocument::DryThroughWet,
                controls: TrackControlsDocument::default(),
                latency: TrackLatencyDocument::default(),
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
                            capture_alignment_frames: 0,
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
                            capture_alignment_frames: 0,
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
                            capture_alignment_frames: 0,
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
                default_playback_mode: DefaultPlaybackModeDocument::Regular,
                controls: TrackControlsDocument::default(),
                latency: TrackLatencyDocument::default(),
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
                        instances: vec![CompositeLoopInstanceDocument {
                            instance_id: 1,
                            start_cycle: 2,
                            loop_id: 10,
                            mode: Some("playing".to_owned()),
                            n_cycles: Some(2),
                        }],
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
                default_playback_mode: DefaultPlaybackModeDocument::Regular,
                controls: TrackControlsDocument::default(),
                latency: TrackLatencyDocument::default(),
                loops: Vec::new(),
                ports: Vec::new(),
                fx_chain: Some(FxChainDocument {
                    id: 801,
                    title: "Deferred Carla rack".to_owned(),
                    chain_type: FxChainTypeDocument::CarlaRack,
                    ports: Vec::new(),
                    internal_state: "opaque\0carla\nstate".to_owned(),
                    midi_cc_assignments: Vec::new(),
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
                midi_cc_assignments: Vec::new(),
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

    fn rewrite_manifest(bytes: Vec<u8>, rewrite: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut rewrite = Some(rewrite);
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
                rewrite.take().unwrap()(&mut manifest);
                payload = serde_json::to_vec(&manifest).unwrap();
            }
            output.start_file(name, options).unwrap();
            output.write_all(&payload).unwrap();
        }
        output.finish().unwrap().into_inner()
    }

    fn rewrite_manifest_major(bytes: Vec<u8>, major: u16) -> Vec<u8> {
        rewrite_manifest(bytes, |manifest| {
            manifest["format_version"]["major"] = serde_json::json!(major);
        })
    }

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_state_and_assignments_round_trip_with_exact_version() {
        let mut bundle = oxisynth_bundle();
        bundle.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .midi_cc_assignments
            .push(OxiSynthMidiCcAssignmentDocument {
                parameter: OxiSynthParameterDocument::ReverbSend,
                channel: 0,
                controller: 91,
            });
        let encoded = encode_session(&bundle, "oxisynth-test").unwrap();
        assert_eq!(decode_session(&encoded).unwrap(), bundle);

        for unsupported in [5, 10] {
            let invalid = rewrite_manifest(encoded.clone(), |manifest| {
                manifest["document_version"] = serde_json::json!(unsupported);
            });
            assert!(matches!(
                decode_session(&invalid),
                Err(SessionError::UnsupportedVersion { .. })
            ));
        }
        let legacy = rewrite_manifest(encoded.clone(), |manifest| {
            manifest["document_version"] = serde_json::json!(6);
        });
        assert_eq!(decode_session(&legacy).unwrap(), bundle);

        let removed_tiny = rewrite_manifest(encoded, |manifest| {
            manifest["document"]["track_groups"][0]["tracks"][0]["topology"] =
                serde_json::json!({"kind": "tiny_synth_fx", "audio_channels": 2});
        });
        assert!(matches!(
            decode_session(&removed_tiny),
            Err(SessionError::Manifest(_))
        ));

        let mut mismatched = bundle.clone();
        mismatched.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .chain_type = FxChainTypeDocument::CarlaRack;
        assert!(validate_bundle(&mismatched).is_err());

        let mut empty = bundle.clone();
        empty.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .internal_state
            .clear();
        assert!(validate_bundle(&empty).is_err());

        let mut duplicate = bundle;
        duplicate.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .midi_cc_assignments
            .push(OxiSynthMidiCcAssignmentDocument {
                parameter: OxiSynthParameterDocument::ChorusSend,
                channel: 0,
                controller: 91,
            });
        assert!(validate_bundle(&duplicate).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn builtin_fx_state_shape_and_previous_version_are_validated() {
        let bundle = builtin_fx_bundle();
        let encoded = encode_session(&bundle, "builtin-fx-test").unwrap();
        assert_eq!(decode_session(&encoded).unwrap(), bundle);

        let previous_bundle = SessionBundle::new(SessionDocument::empty(48_000));
        let previous = rewrite_manifest(
            encode_session(&previous_bundle, "previous-version").unwrap(),
            |manifest| {
                manifest["document_version"] = serde_json::json!(8);
            },
        );
        assert_eq!(decode_session(&previous).unwrap(), previous_bundle);

        let mut with_recorded_state = bundle.clone();
        with_recorded_state
            .document
            .fx_states
            .push(FxStateDocument {
                id: 99,
                chain_type: FxChainTypeDocument::BuiltInFx,
                internal_state: "shoop-builtin-fx:1:1".to_owned(),
            });
        validate_bundle(&with_recorded_state).unwrap();

        for invalid_state in [
            "",
            "shoop-builtin-fx:1:false",
            "shoop-builtin-fx:1:2",
            "shoop-builtin-fx:2:1",
            "shoop-builtin-fx:1:1:extra",
        ] {
            let mut invalid = bundle.clone();
            invalid.document.track_groups[0].tracks[0]
                .fx_chain
                .as_mut()
                .unwrap()
                .internal_state = invalid_state.to_owned();
            assert!(validate_bundle(&invalid).is_err(), "{invalid_state}");

            let mut invalid_recorded_state = with_recorded_state.clone();
            invalid_recorded_state.document.fx_states[0].internal_state = invalid_state.to_owned();
            assert!(
                validate_bundle(&invalid_recorded_state).is_err(),
                "recorded {invalid_state}"
            );
        }

        let mut mismatched = bundle.clone();
        mismatched.document.track_groups[0].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .chain_type = FxChainTypeDocument::OxiSynth;
        assert!(validate_bundle(&mismatched).is_err());

        let mut midi = bundle.clone();
        midi.document.track_groups[0].tracks[0].loops[0]
            .channels
            .push(ChannelDocument {
                id: 9999,
                mode: ChannelModeDocument::Dry,
                data_type: DataTypeDocument::Midi,
                data_length_frames: 0,
                start_offset_frames: 0,
                capture_alignment_frames: 0,
                preplay_frames: 0,
                gain: 1.0,
                connected_port_ids: Vec::new(),
                media_id: None,
                recording_started_at: None,
                recording_fx_state_id: None,
            });
        assert!(validate_bundle(&midi).is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn legacy_fx_chain_without_midi_assignments_defaults_to_empty() {
        let chain = oxisynth_bundle().document.track_groups[0].tracks[0]
            .fx_chain
            .clone()
            .unwrap();
        let mut value = serde_json::to_value(chain).unwrap();
        value.as_object_mut().unwrap().remove("midi_cc_assignments");
        let decoded: FxChainDocument = serde_json::from_value(value).unwrap();
        assert!(decoded.midi_cc_assignments.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn minimal_session_fixture_round_trips() {
        let bundle = SessionBundle::new(SessionDocument::empty(48_000));
        let encoded = encode_session(&bundle, "minimal-fixture").unwrap();
        assert_eq!(decode_session(&encoded).unwrap(), bundle);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn missing_auto_mute_other_track_inputs_defaults_off() {
        let mut bundle = direct_bundle(1);
        bundle.document.global.auto_mute_other_track_inputs = false;
        let encoded = encode_session(&bundle, "legacy-global-fixture").unwrap();
        let without_field = rewrite_manifest(encoded, |manifest| {
            manifest["document"]["global"]
                .as_object_mut()
                .unwrap()
                .remove("auto_mute_other_track_inputs");
        });
        assert_eq!(decode_session(&without_field).unwrap(), bundle);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn missing_auto_arm_track_inputs_defaults_on() {
        let mut bundle = direct_bundle(1);
        bundle.document.global.auto_arm_track_inputs = true;
        let encoded = encode_session(&bundle, "legacy-auto-arm-fixture").unwrap();
        let without_field = rewrite_manifest(encoded, |manifest| {
            manifest["document"]["global"]
                .as_object_mut()
                .unwrap()
                .remove("auto_arm_track_inputs");
        });
        assert_eq!(decode_session(&without_field).unwrap(), bundle);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn independent_script_bundles_round_trip_exact_resources_and_paths() {
        use shoop_script_resources::{
            NormalizedRelativePath, ResourceKind, ResourceLimits, ScriptResource,
            ScriptResourceBundle,
        };
        use std::sync::Arc;

        let mut bundle = direct_bundle(1);
        let resources = |source: &'static [u8], markdown: &'static [u8]| {
            Arc::new(
                ScriptResourceBundle::new(
                    NormalizedRelativePath::parse("main.lua").unwrap(),
                    BTreeMap::from([
                        (
                            NormalizedRelativePath::parse("main.lua").unwrap(),
                            ScriptResource::new(ResourceKind::Lua, Arc::<[u8]>::from(source)),
                        ),
                        (
                            NormalizedRelativePath::parse("help/readme.md").unwrap(),
                            ScriptResource::new(
                                ResourceKind::Markdown,
                                Arc::<[u8]>::from(markdown),
                            ),
                        ),
                        (
                            NormalizedRelativePath::parse("help/image.png").unwrap(),
                            ScriptResource::new(
                                ResourceKind::Image,
                                Arc::<[u8]>::from(&b"\0PNG\xff"[..]),
                            ),
                        ),
                    ]),
                    ResourceLimits::default(),
                )
                .unwrap(),
            )
        };
        bundle.scripts.insert(700, resources(b"return 1", b"first"));
        bundle.document.scripts.push(ScriptDocument {
            id: 701,
            name: "second".to_owned(),
            entrypoint: "main.lua".to_owned(),
            enabled: false,
        });
        bundle
            .scripts
            .insert(701, resources(b"return 2", b"second"));

        let encoded = encode_session(&bundle, "script-bundles").unwrap();
        assert_eq!(encoded, encode_session(&bundle, "script-bundles").unwrap());
        let decoded = decode_session(&encoded).unwrap();
        assert_eq!(decoded, bundle);
        assert_eq!(
            decoded.scripts[&700]
                .get(&NormalizedRelativePath::parse("help/readme.md").unwrap())
                .unwrap()
                .bytes
                .as_ref(),
            b"first"
        );
        assert_eq!(
            decoded.scripts[&701]
                .get(&NormalizedRelativePath::parse("help/readme.md").unwrap())
                .unwrap()
                .bytes
                .as_ref(),
            b"second"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn undeclared_and_cross_owner_script_resources_are_rejected() {
        let encoded = encode_session(&direct_bundle(1), "script-adversarial").unwrap();
        let undeclared = rewrite_manifest(encoded.clone(), |manifest| {
            manifest["scripts"].as_array_mut().unwrap().clear();
        });
        assert!(decode_session(&undeclared).is_err());
        let wrong_owner = rewrite_manifest(encoded, |manifest| {
            manifest["scripts"][0]["owner_script_id"] = serde_json::json!(999);
        });
        assert!(matches!(
            decode_session(&wrong_owner),
            Err(SessionError::Validation(_))
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn deferred_feature_fixture_round_trips_without_field_loss() {
        let bundle = deferred_feature_bundle();
        let encoded = encode_session(&bundle, "deferred-fixture").unwrap();
        let decoded = decode_session(&encoded).unwrap();
        assert_eq!(decoded, bundle);
        let version_eight = rewrite_manifest(encoded, |manifest| {
            manifest["document_version"] = serde_json::json!(8);
            for track in manifest["document"]["track_groups"][0]["tracks"]
                .as_array_mut()
                .unwrap()
            {
                track
                    .as_object_mut()
                    .unwrap()
                    .remove("default_playback_mode");
            }
        });
        let migrated = decode_session(&version_eight).unwrap();
        assert!(migrated.document.track_groups[0]
            .tracks
            .iter()
            .all(|track| { track.default_playback_mode == DefaultPlaybackModeDocument::Regular }));
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
                .instances[0]
                .start_cycle,
            2
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

    #[shoop_wasm_test_support::shoop_test]
    fn positioned_composite_instances_validate_identity_length_mode_and_range() {
        let mut duplicate = deferred_feature_bundle();
        let composite = duplicate.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        composite.instances.push(composite.instances[0].clone());
        assert!(matches!(
            validate_bundle(&duplicate),
            Err(SessionError::Validation(message)) if message.contains("duplicate instance ID")
        ));

        let mut invalid = deferred_feature_bundle();
        let composite = invalid.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        composite.instances[0].n_cycles = Some(0);
        assert!(matches!(
            validate_bundle(&invalid),
            Err(SessionError::Validation(message)) if message.contains("zero-length instance")
        ));

        let mut invalid = deferred_feature_bundle();
        let composite = invalid.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        composite.kind = CompositeKindDocument::Regular;
        assert!(matches!(
            validate_bundle(&invalid),
            Err(SessionError::Validation(message)) if message.contains("explicit instance mode")
        ));

        let mut invalid = deferred_feature_bundle();
        let mut nested_regular = invalid.document.track_groups[0].tracks[2].loops[0].clone();
        nested_regular.id = 31;
        nested_regular.name = "Nested regular".to_owned();
        let nested_document = nested_regular.composite.as_mut().unwrap();
        nested_document.kind = CompositeKindDocument::Regular;
        for instance in &mut nested_document.instances {
            instance.mode = None;
        }
        invalid.document.track_groups[0].tracks[2]
            .loops
            .push(nested_regular);
        let script = invalid.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        script.instances[0].loop_id = 31;
        script.instances[0].mode = Some("recording".to_owned());
        assert!(matches!(
            validate_bundle(&invalid),
            Err(SessionError::Validation(message))
                if message.contains("unsupported mode from nested regular composite")
        ));

        let mut invalid = deferred_feature_bundle();
        let composite = invalid.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        composite.instances[0].start_cycle = u64::from(u32::MAX) + 1;
        assert!(matches!(
            validate_bundle(&invalid),
            Err(SessionError::Validation(message)) if message.contains("out-of-range start cycle")
        ));

        let mut invalid = deferred_feature_bundle();
        let composite = invalid.document.track_groups[0].tracks[2].loops[0]
            .composite
            .as_mut()
            .unwrap();
        composite.instances[0].start_cycle = u64::from(u32::MAX) - 1;
        composite.instances[0].n_cycles = Some(2);
        assert!(matches!(
            validate_bundle(&invalid),
            Err(SessionError::Validation(message)) if message.contains("out-of-range instance end cycle")
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn latency_settings_and_take_alignment_round_trip_without_provider_metadata() {
        let mut bundle = direct_bundle(1);
        let track = &mut bundle.document.track_groups[0].tracks[0];
        track.latency = TrackLatencyDocument {
            adjustment: RecordingOffsetAdjustmentDocument::ManualOverride,
            manual_frames: -13,
            processor_adjustment: ProcessorLatencyAdjustmentDocument::AutomaticPlusTrim,
            processor_manual_frames: 17,
            legacy_processor_advance_frames: None,
        };
        track.loops[0].length_frames = 3;
        track.loops[0].channels[0].start_offset_frames = 13;
        track.loops[0].channels[0].capture_alignment_frames = -13;
        let encoded = encode_session(&bundle, "latency-roundtrip").unwrap();
        let decoded = decode_session(&encoded).unwrap();
        assert_eq!(decoded, bundle);

        let version_seven = rewrite_manifest(encoded.clone(), |manifest| {
            manifest["document_version"] = serde_json::json!(7);
            let latency = &mut manifest["document"]["track_groups"][0]["tracks"][0]["latency"];
            latency
                .as_object_mut()
                .unwrap()
                .remove("processor_adjustment");
            latency
                .as_object_mut()
                .unwrap()
                .remove("processor_manual_frames");
            latency["processor_advance_frames"] = serde_json::json!(17);
        });
        let migrated = decode_session(&version_seven).unwrap();
        assert_eq!(
            migrated.document.track_groups[0].tracks[0].latency,
            TrackLatencyDocument {
                adjustment: RecordingOffsetAdjustmentDocument::ManualOverride,
                manual_frames: -13,
                processor_adjustment: ProcessorLatencyAdjustmentDocument::ManualOverride,
                processor_manual_frames: 17,
                legacy_processor_advance_frames: None,
            }
        );

        let invalid_archive = rewrite_manifest(encoded.clone(), |manifest| {
            let channel = &mut manifest["document"]["track_groups"][0]["tracks"][0]["loops"][0]
                ["channels"][0];
            channel["start_offset_frames"] = serde_json::json!(0);
            channel["capture_alignment_frames"] = serde_json::json!(1);
        });
        assert!(matches!(
            decode_session(&invalid_archive),
            Err(SessionError::Validation(message)) if message.contains("retained media window")
        ));

        let legacy = rewrite_manifest(encoded, |manifest| {
            manifest["document_version"] = serde_json::json!(6);
        });
        let legacy = decode_session(&legacy).unwrap();
        assert_eq!(
            legacy.document.track_groups[0].tracks[0].loops[0].channels[0].capture_alignment_frames,
            0
        );

        {
            let channel = &mut bundle.document.track_groups[0].tracks[0].loops[0].channels[0];
            channel.start_offset_frames = 0;
            channel.capture_alignment_frames = 1;
        }
        assert!(matches!(
            encode_session(&bundle, "invalid-retained-window"),
            Err(SessionError::Validation(message)) if message.contains("retained media window")
        ));
        bundle.document.track_groups[0].tracks[0].loops[0].channels[0].capture_alignment_frames =
            -1;
        assert!(matches!(
            encode_session(&bundle, "invalid-negative-window"),
            Err(SessionError::Validation(message)) if message.contains("retained media window")
        ));

        bundle.document.track_groups[0].tracks[0].loops[0].channels[0].capture_alignment_frames =
            i64::from(shoop_latency::MAX_COMPENSATION_FRAMES) + 1;
        assert!(matches!(
            encode_session(&bundle, "invalid-latency"),
            Err(SessionError::Validation(message)) if message.contains("capture alignment")
        ));

        let mut processed = oxisynth_bundle();
        processed.document.track_groups[0].tracks[0].latency = TrackLatencyDocument {
            adjustment: RecordingOffsetAdjustmentDocument::ManualOverride,
            manual_frames: i64::from(shoop_latency::MAX_COMPENSATION_FRAMES),
            processor_adjustment: ProcessorLatencyAdjustmentDocument::ManualOverride,
            processor_manual_frames: 1,
            legacy_processor_advance_frames: None,
        };
        assert!(matches!(
            encode_session(&processed, "invalid-derived-wet-alignment"),
            Err(SessionError::Validation(message)) if message.contains("latency")
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn resampling_converts_every_sample_domain_and_preserves_midi_order() {
        let mut bundle = direct_bundle(2);
        let MediaPayload::Audio(audio) = bundle.media.get_mut("audio_0").unwrap() else {
            panic!("audio payload missing")
        };
        audio.samples.resize(309, 0.25);
        let source_track = &mut bundle.document.track_groups[0].tracks[0];
        source_track.latency = TrackLatencyDocument {
            adjustment: RecordingOffsetAdjustmentDocument::AutomaticPlusTrim,
            manual_frames: -9,
            processor_adjustment: ProcessorLatencyAdjustmentDocument::ManualOverride,
            processor_manual_frames: 12,
            legacy_processor_advance_frames: None,
        };
        source_track.loops[0].channels[0].data_length_frames = 309;
        source_track.loops[0].channels[0].capture_alignment_frames = 9;
        for rate in [44_100, 32_000, 96_000] {
            let converted = resample_session(&bundle, rate).unwrap();
            assert_eq!(converted.document.sample_rate, rate);
            assert_eq!(
                converted.document.track_groups[0].tracks[0].loops[0].channels[0]
                    .data_length_frames,
                scale_duration(309, 48_000, rate).unwrap()
            );
        }
        let converted = resample_session(&bundle, 32_000).unwrap();
        assert_eq!(converted.document.sample_rate, 32_000);
        assert_eq!(converted.scripts, bundle.scripts);
        let track = &converted.document.track_groups[0].tracks[0];
        assert_eq!(track.ports[0].ringbuffer_frames, 64_000);
        assert_eq!(track.latency.manual_frames, -6);
        assert_eq!(track.latency.processor_manual_frames, 8);
        let loop_ = &track.loops[0];
        assert_eq!(loop_.length_frames, 201);
        assert_eq!(loop_.channels[0].data_length_frames, 206);
        assert_eq!(loop_.channels[0].start_offset_frames, -1);
        assert_eq!(loop_.channels[0].capture_alignment_frames, 6);
        assert_eq!(loop_.channels[0].preplay_frames, 2);
        let MediaPayload::Midi(midi) = &converted.media["midi_main"] else {
            panic!("MIDI payload missing")
        };
        assert_eq!(midi.length_frames, 201);
        assert_eq!(midi.events[0].frame, 67);
        assert_eq!(midi.events[1].frame, 134);
        assert_eq!(midi.start_state, vec![vec![0xB0, 7, 100]]);

        let deferred = resample_session(&deferred_feature_bundle(), 32_000).unwrap();
        assert_eq!(
            deferred.document.track_groups[0].tracks[2].loops[0]
                .composite
                .as_ref()
                .unwrap()
                .instances[0]
                .start_cycle,
            2
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn resampling_pads_rounding_gap_at_compensated_window_end_with_silence() {
        let mut bundle = direct_bundle(1);
        let loop_ = &mut bundle.document.track_groups[0].tracks[0].loops[0];
        loop_.length_frames = 101;
        let channel = &mut loop_.channels[0];
        channel.data_length_frames = 102;
        channel.start_offset_frames = 0;
        channel.capture_alignment_frames = 1;
        let midi_channel = loop_.channels.last_mut().unwrap();
        midi_channel.data_length_frames = 102;
        midi_channel.capture_alignment_frames = 1;
        let MediaPayload::Audio(audio) = bundle.media.get_mut("audio_0").unwrap() else {
            panic!("audio payload missing")
        };
        audio.samples.resize(102, 0.5);
        let MediaPayload::Midi(midi) = bundle.media.get_mut("midi_main").unwrap() else {
            panic!("MIDI payload missing")
        };
        midi.length_frames = 102;
        midi.events.clear();

        let converted = resample_session(&bundle, 24_000).unwrap();
        let loop_ = &converted.document.track_groups[0].tracks[0].loops[0];
        let channel = &loop_.channels[0];
        assert_eq!(loop_.length_frames, 51);
        assert_eq!(channel.capture_alignment_frames, 1);
        assert_eq!(channel.data_length_frames, 52);
        let MediaPayload::Audio(audio) = &converted.media["audio_0"] else {
            panic!("audio payload missing")
        };
        assert_eq!(audio.samples.len(), 52);
        assert_eq!(audio.samples.last(), Some(&0.0));
        let midi_channel = loop_.channels.last().unwrap();
        assert_eq!(midi_channel.data_length_frames, 52);
        let MediaPayload::Midi(midi) = &converted.media["midi_main"] else {
            panic!("MIDI payload missing")
        };
        assert_eq!(midi.length_frames, 52);
        let raw_end = channel.start_offset_frames
            + channel.capture_alignment_frames
            + loop_.length_frames as i64;
        assert_eq!(raw_end, channel.data_length_frames as i64);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn old_non_zip_and_resource_limit_fail_without_decoding() {
        assert!(matches!(
            decode_session(b"unsupported predecessor archive"),
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn dry_through_wet_default_requires_dry_wet_topology() {
        let mut invalid = deferred_feature_bundle();
        invalid.document.track_groups[0].tracks[2].default_playback_mode =
            DefaultPlaybackModeDocument::DryThroughWet;
        assert!(matches!(
            encode_session(&invalid, "invalid-default"),
            Err(SessionError::Validation(message))
                if message.contains("dry-through-wet default playback")
        ));

        let mut invalid_sync = deferred_feature_bundle();
        invalid_sync.document.track_groups[0].tracks[3].is_sync = true;
        invalid_sync.document.track_groups[0].tracks[3].default_playback_mode =
            DefaultPlaybackModeDocument::DryThroughWet;
        assert!(matches!(
            encode_session(&invalid_sync, "invalid-sync-default"),
            Err(SessionError::Validation(message))
                if message.contains("dry-through-wet default playback")
        ));

        let mut invalid_no_wet = deferred_feature_bundle();
        let TrackTopologyDocument::DryWetExternal {
            wet_audio_channels, ..
        } = &mut invalid_no_wet.document.track_groups[0].tracks[1].topology
        else {
            panic!("fixture dry/wet track changed topology");
        };
        *wet_audio_channels = 0;
        assert!(matches!(
            encode_session(&invalid_no_wet, "invalid-no-wet-default"),
            Err(SessionError::Validation(message))
                if message.contains("dry-through-wet default playback")
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
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
