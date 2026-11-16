mod support;
use shoop_backend::*;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn capture(backend: &mut Stack, id: BackendLoopId) -> BackendLoopContent {
    for _ in 0..4000 {
        match backend.capture_loop_content_async(id).unwrap() {
            BackendAsyncResult::Ready(content) => return content,
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(16),
        }
    }
    panic!("capture did not settle")
}

fn load(
    backend: &mut Stack,
    id: BackendLoopId,
    content: &BackendLoopContent,
) -> anyhow::Result<()> {
    for _ in 0..4000 {
        match backend.load_loop_content_async(id, content)? {
            BackendAsyncResult::Ready(()) => return Ok(()),
            BackendAsyncResult::Pending(_) => backend.clock_mut().render(16),
        }
    }
    panic!("content loading did not settle")
}

#[shoop_test]
fn complete_payload_replaces_all_roles_and_empty_media_with_no_spare_loop_slot() {
    use shoop_rt_protocol::{ProcessorPolicy, ProcessorSettings};
    for topology in [
        BackendTrackTopology::Direct {
            audio_channels: 2,
            midi: true,
        },
        BackendTrackTopology::Direct {
            audio_channels: 0,
            midi: true,
        },
        BackendTrackTopology::Direct {
            audio_channels: 1,
            midi: false,
        },
        BackendTrackTopology::DryWetExternal {
            dry_audio_channels: 1,
            wet_audio_channels: 2,
            dry_midi: true,
        },
        BackendTrackTopology::DryWetProcessor {
            dry_audio_channels: 2,
            wet_audio_channels: 2,
            dry_midi: true,
            processor_type: "builtin_fx".into(),
        },
        BackendTrackTopology::DryWetProcessor {
            dry_audio_channels: 2,
            wet_audio_channels: 2,
            dry_midi: true,
            processor_type: "oxisynth".into(),
        },
    ] {
        let mut d = description();
        d.sample_rate = 8000;
        d.quantum = 16;
        d.segments[0].loops = 3;
        d.external_dry_wet = true;
        let processor = match &topology {
            BackendTrackTopology::DryWetProcessor { processor_type, .. } => {
                Some(processor_type.as_str())
            }
            _ => None,
        };
        d.processors = ProcessorSettings {
            builtin_fx: if processor == Some("builtin_fx") {
                ProcessorPolicy::On
            } else {
                ProcessorPolicy::Off
            },
            oxisynth: if processor == Some("oxisynth") {
                ProcessorPolicy::On
            } else {
                ProcessorPolicy::Off
            },
            fx_instances: 1,
            synth_instances: 1,
            memory_mb: 128,
        };
        let (offline, client) = shoop_rt_engine::testing::OfflineRender::new_with_processors(
            &d,
            shoop_host_processors::prepare(&d).unwrap(),
        )
        .unwrap();
        let mut backend = shoop_rt_backend::RtBackend::connect(
            client,
            OfflineClock {
                offline,
                capture: |_| 0.,
                block: 16,
            },
        )
        .unwrap();
        let track = backend
            .create_track(TrackRequest {
                port_name_base: "copy".into(),
                topology: topology.clone(),
                initial_loops: 3,
            })
            .unwrap();
        backend.wait_idle();
        let [source, target, unrelated] = track.loops[..] else {
            panic!()
        };
        let empty = capture(&mut backend, source);
        let mut full = empty.clone();
        full.length = 32;
        for (i, channel) in full.audio.iter_mut().enumerate() {
            channel.samples = vec![(i + 1) as f32 / 8.; 64].into();
            channel.gain = 0.75;
            channel.start_offset = 2;
            channel.capture_alignment_frames = 1;
            channel.preplay = 2;
        }
        for channel in &mut full.midi {
            channel.length = 64;
            channel.start_state = vec![vec![0xb0, 7, 99]];
            channel.events = vec![
                BackendMidiEvent {
                    frame: 1,
                    data: vec![0x90, 60, 100].into(),
                },
                BackendMidiEvent {
                    frame: 55,
                    data: vec![0xb0, 11, 23].into(),
                },
            ]
            .into();
            channel.start_offset = 2;
            channel.capture_alignment_frames = 1;
            channel.preplay = 2;
        }
        if let Some(processor) = processor {
            let kind = if processor == "builtin_fx" {
                shoop_processor_protocol::Kind::BuiltInFx
            } else {
                shoop_processor_protocol::Kind::OxiSynth
            };
            let mut state = shoop_processor_protocol::state::State::new(kind).unwrap();
            if processor == "builtin_fx" {
                state.values[55] = 2.;
            }
            full.recorded_processor_state = Some(state.encode());
        }
        load(&mut backend, source, &full).unwrap();
        let retained = capture(&mut backend, source);
        backend
            .transition_loop(unrelated, BackendLoopMode::Recording, None)
            .unwrap();
        backend.wait_idle();
        for variant in 0..5 {
            load(&mut backend, target, &retained).unwrap();
            let mut expected = empty.clone();
            if variant > 0 {
                expected.length = 32;
            }
            if variant == 2 || variant == 4 {
                expected.audio = retained.audio.clone();
                expected.recorded_processor_state = retained.recorded_processor_state.clone();
            }
            if variant == 3 || variant == 4 {
                expected.midi = retained.midi.clone();
            }
            load(&mut backend, target, &expected).unwrap();
            let actual = capture(&mut backend, target);
            assert_eq!(actual.audio, expected.audio);
            assert_eq!(actual.midi, expected.midi);
            assert_eq!(actual.length, expected.length);
            assert_eq!(
                actual.recorded_processor_state,
                expected.recorded_processor_state
            );
            let snapshot = backend.poll().unwrap();
            assert_eq!(snapshot.loops.len(), 3);
            assert_eq!(snapshot.loops[&target].mode, BackendLoopMode::Stopped);
            assert_eq!(snapshot.loops[&target].position, 0);
            assert_eq!(snapshot.loops[&unrelated].mode, BackendLoopMode::Recording);
        }
        backend.clear_loop(source).unwrap();
        backend.wait_idle();
        assert_eq!(capture(&mut backend, target).audio, retained.audio);
        assert!(retained
            .audio
            .iter()
            .all(|c| c.samples.iter().all(|v| v.is_finite())));
    }
}

#[shoop_test]
fn invalid_payload_and_budget_exhaustion_preserve_the_occupied_destination() {
    let mut backend = stack(&description())
        .with_resource_limits(shoop_app_api::HostResourceLimits {
            media_bytes: 256,
            ..Default::default()
        })
        .unwrap();
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "copy".into(),
            audio_channels: 1,
            midi: true,
            initial_loops: 1,
        })
        .unwrap();
    backend.wait_idle();
    let target = track.loops[0];
    let mut content = capture(&mut backend, target);
    content.length = 16;
    content.audio[0].samples = vec![0.25; 16].into();
    load(&mut backend, target, &content).unwrap();
    let before = capture(&mut backend, target);
    let mut invalid = content.clone();
    invalid.audio.clear();
    assert!(load(&mut backend, target, &invalid).is_err());
    invalid = content.clone();
    invalid.audio[0].samples = vec![f32::NAN; 16].into();
    assert!(load(&mut backend, target, &invalid).is_err());
    invalid = content.clone();
    invalid.audio[0].samples = vec![0.5; 1024].into();
    assert!(load(&mut backend, target, &invalid).is_err());
    assert_eq!(capture(&mut backend, target), before);
}
