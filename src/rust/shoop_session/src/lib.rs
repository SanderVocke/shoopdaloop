mod archive;
mod document;
mod media;
mod resample;

pub use archive::{
    decode_session, decode_session_with_limits, encode_session, validate_bundle, DecodeLimits,
    SessionError,
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
