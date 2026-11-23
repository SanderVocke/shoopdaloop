#![cfg(all(not(target_arch = "wasm32"), feature = "carla-worker"))]

use rt_buffered::{AsyncBatch, Backend, BatchTag, BufferLimits, MidiLimits, Timing};
use rt_runtime::{buffer::MidiBuffer, Frame, Quantum};
use shoop_carla::{
    carla_shared_memory::SharedBlockTransport,
    processor::CarlaProcessor,
    service::ChainService,
    state::{decode_state, encode_state},
    subprocess::{new_nonce, nonce_hex, SupervisedCarlaProcessor},
};
use shoop_plugin_protocol::{
    read_frame, write_frame, BlockSequence, CarlaChainType, CarlaProcessorLifecycle, ChainId,
    ControlRequest, ControlRequestKind, ControlResponse, ControlResponseKind, MidiEvent,
    ParentToWorker, ProcessGeneration, ProtocolErrorCode, PrototypeBlock, RequestId,
    WorkerToParent,
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};
use std::{
    net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(5);
const CHAIN: ChainId = ChainId(41);
const GENERATION: ProcessGeneration = ProcessGeneration(73);

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn runtime_available() -> bool {
    match shoop_carla::native::carla_runtime_availability() {
        Ok(()) => true,
        Err(error) => {
            assert!(
                std::env::var_os("SHOOP_REQUIRE_CARLA_TESTS").is_none(),
                "{error}"
            );
            eprintln!("skipping Carla child test: {error}");
            false
        }
    }
}

#[shoop_test]
fn companion_installs_real_carla_track_and_bus_banks() {
    use shoop_backend::{
        Backend, BackendAsyncResult, BackendAudioContent, BackendBusFxControl, BackendBusFxRequest,
        BackendBusRequest, BackendChannelMode, BackendLoopContent, BackendLoopMode,
        BackendPortRole, BackendSessionMixerRoute, BackendTrackFxControl, BackendTrackTopology,
        TrackRequest,
    };
    use shoop_rt_engine::EngineDescription;
    use shoop_rt_host_native::{DriverConfig, DummyConfig, NativeConfig};
    use shoop_rt_host_process::{Pending, Started, Transport};
    use shoop_rt_protocol::{
        processors::{CarlaHosting, CarlaSettings},
        ProcessorSettings,
    };

    if !runtime_available() {
        return;
    }
    for (hosting, transport) in [
        (CarlaHosting::InProcess, Transport::Socket),
        (CarlaHosting::Subprocess, Transport::SharedMemory),
    ] {
        let companion = std::path::Path::new(env!("CARGO_BIN_EXE_shoop_rt_host_process"));
        let Started {
            client, supervisor, ..
        } = Pending::spawn(companion, transport)
            .unwrap()
            .start(&NativeConfig {
                driver: DriverConfig::Dummy(DummyConfig::default()),
                engine: EngineDescription {
                    processors: ProcessorSettings {
                        memory_mb: 256,
                        carla: CarlaSettings {
                            hosting,
                            rack_instances: 1,
                            patchbay_instances: 1,
                            patchbay16x_instances: 0,
                        },
                        ..ProcessorSettings::disabled()
                    },
                    ..EngineDescription::default()
                },
            })
            .unwrap();
        let mut backend =
            shoop_rt_backend::RtBackend::connect(client, shoop_rt_backend::Sleep::default())
                .unwrap();
        let limits = backend.processor_limits().unwrap();
        assert!(limits.dynamic_carla, "{limits:?}");
        assert_eq!(limits.effective.carla.instances(), 0, "{limits:?}");
        assert_eq!(limits.reserved_bytes, 0, "{limits:?}");
        assert_eq!(limits.carla_latency_frames, 768);
        let descriptors = backend.track_processor_catalog().unwrap();
        let rack = descriptors
            .iter()
            .find(|entry| entry.id.as_str() == "carla_rack")
            .unwrap();
        assert!(rack.available);
        assert_eq!(rack.features.logs, hosting == CarlaHosting::Subprocess);
        assert_eq!(rack.resources.as_ref().unwrap().buffering_frames, 768);

        let track = backend
            .create_track(TrackRequest {
                port_name_base: "Carla rack".into(),
                topology: BackendTrackTopology::DryWetProcessor {
                    dry_audio_channels: 1,
                    wet_audio_channels: 2,
                    dry_midi: false,
                    processor_type: rack.id.to_string(),
                    remote_fx: None,
                },
                initial_loops: 1,
            })
            .unwrap();
        let bus = backend
            .create_bus(BackendBusRequest {
                name: "Carla patchbay".into(),
                channel_count: 2,
                fx: Some(BackendBusFxRequest {
                    processor_type: "carla_patchbay".into(),
                    audio_channels: 2,
                }),
            })
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let state = backend.poll().unwrap();
            assert!(
                state.mutation_failures.is_empty(),
                "{:?}",
                state.mutation_failures
            );
            let fx = state
                .tracks
                .get(&track.track_id)
                .and_then(|track| track.fx.as_ref());
            let bus_fx = state
                .mixer
                .buses
                .get(&bus.bus_id)
                .and_then(|bus| bus.fx.as_ref());
            if let (Some(fx), Some(bus_fx)) = (fx, bus_fx.filter(|fx| fx.active)) {
                assert_eq!(fx.processor_type.as_str(), "carla_rack");
                assert_eq!(bus_fx.processor_type.as_str(), "carla_patchbay");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "track FX={fx:?} bus FX={bus_fx:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
        if hosting == CarlaHosting::Subprocess {
            backend
                .set_track_fx_control(track.track_id, BackendTrackFxControl::ClearLogs)
                .unwrap();
            backend
                .set_bus_fx_control(bus.bus_id, BackendBusFxControl::ClearLogs)
                .unwrap();
            let through = backend.mutation_cursor();
            let deadline = Instant::now() + TIMEOUT;
            while backend.mutations_pending_through(through) {
                let state = backend.poll().unwrap();
                assert!(
                    state.mutation_failures.is_empty(),
                    "{:?}",
                    state.mutation_failures
                );
                assert!(
                    Instant::now() < deadline,
                    "Carla log controls did not finish"
                );
                thread::sleep(Duration::from_millis(2));
            }
        }
        assert!(matches!(
            backend.capture_session_async().unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        backend.cancel_media_reads();
        let capture = backend.capture_session().unwrap();
        let track_state = capture
            .tracks
            .iter()
            .find(|value| value.source_id == track.track_id.raw())
            .unwrap();
        let bus_state = capture
            .buses
            .iter()
            .find(|value| value.source_id == bus.bus_id.raw())
            .unwrap();
        for (state, kind) in [
            (
                track_state.processor_state.as_deref(),
                shoop_plugin_protocol::CarlaChainType::Rack,
            ),
            (
                bus_state.processor_state.as_deref(),
                shoop_plugin_protocol::CarlaChainType::Patchbay,
            ),
        ] {
            let checkpoint =
                shoop_plugin_protocol::state::decode_state(state.unwrap(), kind).unwrap();
            assert!(checkpoint
                .windows(b"<CARLA-PROJECT".len())
                .any(|window| window == b"<CARLA-PROJECT"));
        }
        let loop_id = track.loops[0];
        let audio = |mode, sample| BackendAudioContent {
            mode,
            samples: std::sync::Arc::new(vec![sample; 1024]),
            gain: 1.,
            start_offset: 0,
            capture_alignment_frames: 0,
            preplay: 0,
        };
        let content = BackendLoopContent {
            recorded_processor_state: None,
            source_id: 0,
            length: 1024,
            gain: 1.,
            balance: 0.,
            audio: vec![
                audio(BackendChannelMode::Dry, 0.25),
                audio(BackendChannelMode::Wet, 0.),
                audio(BackendChannelMode::Wet, 0.),
            ],
            midi: vec![],
        };
        let deadline = Instant::now() + TIMEOUT;
        while !matches!(
            backend.load_loop_content_async(loop_id, &content).unwrap(),
            BackendAsyncResult::Ready(())
        ) {
            assert!(Instant::now() < deadline, "Carla track upload stalled");
            thread::sleep(Duration::from_millis(2));
        }
        backend
            .transition_loop(loop_id, BackendLoopMode::PlayingDryThroughWet, None)
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let state = backend.poll().unwrap();
            assert!(
                state.mutation_failures.is_empty(),
                "{:?}",
                state.mutation_failures
            );
            let track = &state.tracks[&track.track_id];
            if track
                .output_peaks
                .first()
                .is_some_and(|peak| (*peak - 20.0 * 0.25_f32.log10()).abs() < 1e-4)
            {
                assert!(track.output_peaks[1] < -150.);
                assert!(track.fx.as_ref().unwrap().active);
                break;
            }
            assert!(Instant::now() < deadline, "{track:?}");
            thread::sleep(Duration::from_millis(2));
        }
        let old_revision = backend.poll().unwrap().loops[&loop_id].content_revision;
        backend
            .transition_loop(loop_id, BackendLoopMode::RecordingDryIntoWet, None)
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        let mut started_at = None;
        loop {
            let state = backend.poll().unwrap();
            assert!(
                state.mutation_failures.is_empty(),
                "{:?}",
                state.mutation_failures
            );
            let view = &state.loops[&loop_id];
            assert!(!view.capture_incomplete, "{view:?}");
            if view.mode == BackendLoopMode::RecordingDryIntoWet {
                let frame = state.status.processed_frames;
                let start = *started_at.get_or_insert(frame);
                if frame.saturating_sub(start) >= 2 * u64::from(view.length) {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "Carla rerecord did not wrap: {view:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, None)
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        loop {
            let state = backend.poll().unwrap();
            let view = &state.loops[&loop_id];
            assert!(!view.capture_incomplete, "{view:?}");
            if view.mode == BackendLoopMode::Stopped && view.content_revision > old_revision {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Carla wet tail did not settle: {view:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
        let captured = loop {
            if let BackendAsyncResult::Ready(captured) =
                backend.capture_loop_content_async(loop_id).unwrap()
            {
                break captured;
            }
            assert!(
                Instant::now() < deadline,
                "Carla loop media capture stalled"
            );
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(captured.length, 1024);
        assert_eq!(captured.audio[0].mode, BackendChannelMode::Dry);
        assert_eq!(captured.audio[0].samples.as_slice(), &[0.25; 1024]);
        assert_eq!(captured.audio[1].mode, BackendChannelMode::Wet);
        assert_eq!(captured.audio[1].samples.as_slice(), &[0.25; 1024]);
        assert_eq!(captured.audio[2].mode, BackendChannelMode::Wet);
        assert_eq!(captured.audio[2].samples.as_slice(), &[0.; 1024]);

        let rack_fixture =
            include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_state.json");
        let bus_fixture =
            include_str!("../../shoop_engine/test_data/carla_legacy_patchbay_loaded_state.json");
        let rack_project =
            String::from_utf8(decode_state(rack_fixture, CarlaChainType::Rack).unwrap()).unwrap();
        let mut project = rack_project
            .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1)
            .into_bytes();
        if hosting == CarlaHosting::Subprocess {
            project.extend_from_slice(b"\n<!--");
            project.resize(shoop_plugin_protocol::MAX_STATE_BYTES - 3, b'x');
            project.extend_from_slice(b"-->");
        }
        let rack_state = encode_state(CarlaChainType::Rack, &project).unwrap();
        let mut replacement = backend.capture_session().unwrap();
        let candidate_track = replacement
            .tracks
            .iter_mut()
            .find(|value| value.source_id == track.track_id.raw())
            .unwrap();
        candidate_track.processor_state = Some(rack_state);
        candidate_track.loops[0].recorded_processor_state = Some(rack_fixture.into());
        let candidate_bus = replacement
            .buses
            .iter_mut()
            .find(|value| value.source_id == bus.bus_id.raw())
            .unwrap();
        candidate_bus.processor_state = Some(bus_fixture.into());
        replacement.mixer_routes.push(BackendSessionMixerRoute {
            source_port_id: candidate_track
                .ports
                .iter()
                .find(|port| port.descriptor.role == BackendPortRole::AudioOutput)
                .unwrap()
                .source_id,
            destination_channel_id: candidate_bus.channels[0].source_id,
        });
        let mapping = backend.replace_session(&replacement).unwrap();
        let new_track = mapping.tracks[&track.track_id.raw()].track_id;
        let new_bus = mapping.buses[&bus.bus_id.raw()];
        let new_loop = mapping.loops[&loop_id.raw()];
        backend
            .transition_loop(new_loop, BackendLoopMode::PlayingDryThroughWet, None)
            .unwrap();

        let wait_for_routed_audio = |backend: &mut dyn Backend| {
            let deadline = Instant::now() + TIMEOUT;
            let expected = 20.0 * 0.125_f32.log10();
            loop {
                let state = backend.poll().unwrap();
                assert!(
                    state.mutation_failures.is_empty(),
                    "{:?}",
                    state.mutation_failures
                );
                let track = &state.tracks[&new_track];
                let bus = &state.mixer.buses[&new_bus];
                if track
                    .output_peaks
                    .first()
                    .is_some_and(|peak| (*peak - expected).abs() < 0.01)
                    && bus
                        .output_peaks_db
                        .first()
                        .is_some_and(|peak| (*peak - expected).abs() < 0.01)
                {
                    assert!(track.fx.as_ref().unwrap().active);
                    assert!(bus.fx.as_ref().unwrap().active);
                    break;
                }
                assert!(Instant::now() < deadline, "track {track:?}, bus {bus:?}");
                thread::sleep(Duration::from_millis(2));
            }
        };
        wait_for_routed_audio(&mut backend);
        let saved = backend.capture_session().unwrap();
        let saved_track = saved
            .tracks
            .iter()
            .find(|value| value.source_id == new_track.raw())
            .unwrap();
        assert_eq!(
            saved_track.loops[0].recorded_processor_state.as_deref(),
            Some(rack_fixture)
        );
        assert_eq!(
            saved_track.loops[0].audio[0].samples,
            captured.audio[0].samples
        );
        let saved_bus = saved
            .buses
            .iter()
            .find(|value| value.source_id == new_bus.raw())
            .unwrap();
        let state = decode_state(
            saved_bus.processor_state.as_deref().unwrap(),
            CarlaChainType::Patchbay,
        )
        .unwrap();
        assert!(String::from_utf8(state)
            .unwrap()
            .contains("<Label>audiogain_s</Label>"));

        let mut invalid = saved;
        let invalid_track = invalid
            .tracks
            .iter_mut()
            .find(|value| value.source_id == new_track.raw())
            .unwrap();
        invalid_track.processor_state = Some(
            encode_state(
                CarlaChainType::Rack,
                rack_project
                    .replace(
                        "<Label>audiogain_s</Label>",
                        "<Label>shoop_missing_carla_plugin</Label>",
                    )
                    .as_bytes(),
            )
            .unwrap(),
        );
        let refused = backend.replace_session(&invalid).unwrap_err();
        assert!(refused.to_string().contains("InvalidValue"), "{refused:#}");
        wait_for_routed_audio(&mut backend);

        let old_revision = backend.poll().unwrap().loops[&new_loop].content_revision;
        backend
            .transition_loop(new_loop, BackendLoopMode::Replacing, None)
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        let mut started = None;
        loop {
            let state = backend.poll().unwrap();
            let take = &state.loops[&new_loop];
            assert!(
                state.mutation_failures.is_empty(),
                "{:?}",
                state.mutation_failures
            );
            assert!(!take.capture_incomplete, "{take:?}");
            if take.mode == BackendLoopMode::Replacing {
                let start = *started.get_or_insert(state.status.processed_frames);
                assert_eq!(take.content_revision, old_revision);
                if state.status.processed_frames - start >= 2 * u64::from(take.length) {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "replacing did not start: {take:?}"
            );
            thread::sleep(Duration::from_millis(2));
        }
        backend
            .transition_loop(new_loop, BackendLoopMode::Stopped, None)
            .unwrap();
        let deadline = Instant::now() + TIMEOUT;
        let replaced = loop {
            let state = backend.poll().unwrap();
            let take = &state.loops[&new_loop];
            assert!(!take.capture_incomplete, "{take:?}");
            if take.mode == BackendLoopMode::Stopped && take.content_revision > old_revision {
                if let BackendAsyncResult::Ready(take) =
                    backend.capture_loop_content_async(new_loop).unwrap()
                {
                    break take;
                }
            }
            assert!(
                Instant::now() < deadline,
                "replacement tail did not settle: {take:?}"
            );
            thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(replaced.length, 1024);
        assert_eq!(replaced.audio.len(), 3);
        assert!(replaced
            .audio
            .iter()
            .all(|channel| channel.samples.as_slice() == &[0.; 1024]));
        drop(backend);
        let report = supervisor.stop();
        assert!(
            report.success && report.cleaned && report.midi_reset,
            "{report:?}"
        );
    }
}

#[shoop_test]
fn companion_negotiates_independent_patchbay_16x_capacity() {
    use shoop_backend::{
        Backend, BackendAudioContent, BackendBusFxRequest, BackendBusRequest, BackendChannelMode,
        BackendLoopMode, BackendMidiContent, BackendPortDirection, BackendPortRole,
        BackendSessionMixerRoute, BackendTrackControl, BackendTrackTopology, TrackRequest,
    };
    use shoop_rt_engine::EngineDescription;
    use shoop_rt_host_native::{DriverConfig, DummyConfig, NativeConfig};
    use shoop_rt_host_process::{Pending, Started, Transport};
    use shoop_rt_protocol::{
        processors::{CarlaHosting, CarlaSettings},
        ProcessorSettings,
    };

    if !runtime_available() {
        return;
    }
    let companion = std::path::Path::new(env!("CARGO_BIN_EXE_shoop_rt_host_process"));
    for (hosting, transport, channels, memory_mb) in [
        (CarlaHosting::InProcess, Transport::Socket, 16, 256),
        (CarlaHosting::Subprocess, Transport::SharedMemory, 16, 256),
        (CarlaHosting::InProcess, Transport::Socket, 8, 256),
        (CarlaHosting::Subprocess, Transport::SharedMemory, 16, 1),
    ] {
        let Started {
            client, supervisor, ..
        } = Pending::spawn(companion, transport)
            .unwrap()
            .start(&NativeConfig {
                driver: DriverConfig::Dummy(DummyConfig::default()),
                engine: EngineDescription {
                    strip_channels: channels,
                    bus_channels: channels,
                    processors: ProcessorSettings {
                        memory_mb,
                        carla: CarlaSettings {
                            hosting,
                            rack_instances: 1,
                            patchbay_instances: 0,
                            patchbay16x_instances: 2,
                        },
                        ..ProcessorSettings::disabled()
                    },
                    ..EngineDescription::default()
                },
            })
            .unwrap_or_else(|error| panic!("starting {hosting:?} {transport:?} channels={channels} budget={memory_mb} MiB: {error:#}"));
        let mut backend =
            shoop_rt_backend::RtBackend::connect(client, shoop_rt_backend::Sleep::default())
                .unwrap();
        let limits = backend.processor_limits().unwrap();
        let width = usize::from(channels);
        let wide = "carla_patchbay_16x";
        let catalog = backend.track_processor_catalog().unwrap();
        let descriptor = catalog
            .iter()
            .find(|entry| entry.id.as_str() == wide)
            .unwrap();
        {
            assert!(limits.dynamic_carla, "{limits:?}");
            assert_eq!(limits.effective.carla.instances(), 0, "{limits:?}");
            assert_eq!(limits.reserved_bytes, 0, "{limits:?}");
            assert_eq!(limits.effective.memory_bytes, u64::from(memory_mb) << 20);
            assert!(descriptor.available);
            assert_eq!(
                descriptor.constraints.max_dry_audio_channels,
                Some(u32::from(channels))
            );
            assert_eq!(
                descriptor.constraints.max_wet_audio_channels,
                Some(u32::from(channels))
            );
            assert!(descriptor.features.logs == (hosting == CarlaHosting::Subprocess));
            assert!(backend
                .bus_processor_catalog()
                .unwrap()
                .iter()
                .any(|entry| entry.id.as_str() == wide && entry.available));
            assert!(backend
                .create_track(TrackRequest {
                    port_name_base: "Overwide Carla track".into(),
                    topology: BackendTrackTopology::DryWetProcessor {
                        dry_audio_channels: u32::from(channels) + 1,
                        wet_audio_channels: u32::from(channels) + 1,
                        dry_midi: true,
                        processor_type: wide.into(),
                        remote_fx: None,
                    },
                    initial_loops: 1,
                })
                .is_err());
            let track = backend
                .create_track(TrackRequest {
                    port_name_base: "Wide Carla track".into(),
                    topology: BackendTrackTopology::DryWetProcessor {
                        dry_audio_channels: u32::from(channels),
                        wet_audio_channels: u32::from(channels),
                        dry_midi: true,
                        processor_type: wide.into(),
                        remote_fx: None,
                    },
                    initial_loops: 1,
                })
                .unwrap();
            let bus = backend
                .create_bus(BackendBusRequest {
                    name: "Wide Carla bus".into(),
                    channel_count: u32::from(channels),
                    fx: Some(BackendBusFxRequest {
                        processor_type: wide.into(),
                        audio_channels: u32::from(channels),
                    }),
                })
                .unwrap();
            backend
                .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
                .unwrap();
            let through = backend.mutation_cursor();
            let deadline = Instant::now() + TIMEOUT;
            loop {
                let state = backend.poll().unwrap();
                assert!(
                    state.mutation_failures.is_empty(),
                    "{:?}",
                    state.mutation_failures
                );
                let track_fx = state
                    .tracks
                    .get(&track.track_id)
                    .and_then(|track| track.fx.as_ref());
                let bus_fx = state
                    .mixer
                    .buses
                    .get(&bus.bus_id)
                    .and_then(|bus| bus.fx.as_ref());
                if let (false, Some(track_fx), Some(bus_fx)) = (
                    backend.mutations_pending_through(through),
                    track_fx.filter(|fx| fx.active),
                    bus_fx.filter(|fx| fx.active),
                ) {
                    assert_eq!(track_fx.processor_type.as_str(), wide);
                    assert_eq!(bus_fx.processor_type.as_str(), wide);
                    assert_eq!(state.tracks[&track.track_id].output_peaks.len(), width);
                    assert_eq!(state.mixer.buses[&bus.bus_id].channels.len(), width);
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "track={track_fx:?} bus={bus_fx:?}"
                );
                thread::sleep(Duration::from_millis(2));
            }
            assert_eq!(
                track
                    .ports
                    .iter()
                    .filter(|port| port.direction == BackendPortDirection::Output)
                    .count(),
                width
            );
            let chain = CarlaChainType::Patchbay16x;
            let project =
                wide_loaded_project().replacen("<Value>1</Value>", "<Value>0.5</Value>", 1);
            let checkpoint = encode_state(chain, project.as_bytes()).unwrap();
            let mut session = backend.capture_session().unwrap();
            let target_bus = session
                .buses
                .iter_mut()
                .find(|entry| entry.source_id == bus.bus_id.raw())
                .unwrap();
            target_bus.processor_state = Some(checkpoint.clone());
            let target = session
                .tracks
                .iter_mut()
                .find(|entry| entry.source_id == track.track_id.raw())
                .unwrap();
            target.processor_state = Some(checkpoint);
            let lanes = [(0, 0.25_f32), (7, 0.375), (8, 0.5), (15, 0.625)];
            let take = &mut target.loops[0];
            take.length = 1024;
            let take_id = take.source_id;
            take.audio = [BackendChannelMode::Dry, BackendChannelMode::Wet]
                .into_iter()
                .flat_map(|mode| {
                    (0..width).map(move |channel| {
                        let sample = lanes
                            .iter()
                            .find(|(lane, _)| *lane == channel && mode == BackendChannelMode::Dry)
                            .map_or(0., |(_, sample)| *sample);
                        BackendAudioContent {
                            mode,
                            samples: std::sync::Arc::new(vec![sample; 1024]),
                            gain: 1.,
                            start_offset: 0,
                            capture_alignment_frames: 0,
                            preplay: 0,
                        }
                    })
                })
                .collect();
            take.midi = vec![BackendMidiContent {
                mode: BackendChannelMode::Dry,
                length: 1024,
                start_state: vec![],
                events: std::sync::Arc::new(vec![
                    shoop_backend::BackendMidiEvent {
                        frame: 59,
                        data: vec![0x90, 61, 100].into(),
                    },
                    shoop_backend::BackendMidiEvent {
                        frame: 184,
                        data: vec![0x80, 61, 0].into(),
                    },
                ]),
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            }];
            for (channel, port) in target
                .ports
                .iter()
                .filter(|port| port.descriptor.role == BackendPortRole::AudioOutput)
                .enumerate()
            {
                session.mixer_routes.push(BackendSessionMixerRoute {
                    source_port_id: port.source_id,
                    destination_channel_id: target_bus.channels[channel].source_id,
                });
            }
            let mut track_id = track.track_id.raw();
            let mut bus_id = bus.bus_id.raw();
            let mut take_id = take_id;
            for _ in 0..2 {
                let mapping = backend.replace_session(&session).unwrap();
                let restored_track = mapping.tracks[&track_id].track_id;
                let restored_bus = mapping.buses[&bus_id];
                let restored_take = mapping.loops[&take_id];
                backend
                    .transition_loop(restored_take, BackendLoopMode::PlayingDryThroughWet, None)
                    .unwrap();
                let deadline = Instant::now() + TIMEOUT;
                loop {
                    let state = backend.poll().unwrap();
                    assert!(
                        state.mutation_failures.is_empty(),
                        "{:?}",
                        state.mutation_failures
                    );
                    let track = &state.tracks[&restored_track];
                    let bus = &state.mixer.buses[&restored_bus];
                    let close =
                        |value: f32, sample: f32| (value - 20. * sample.log10()).abs() < 0.015;
                    if lanes
                        .iter()
                        .filter(|(lane, _)| *lane < width)
                        .all(|&(lane, amplitude)| {
                            let gain = if lane == 0 || lane == 15 { 0.5 } else { 1. };
                            close(track.output_peaks[lane], amplitude * gain)
                                && close(bus.output_peaks_db[lane], amplitude * gain * gain)
                        })
                    {
                        assert!((0..width)
                            .filter(|index| !lanes.iter().any(|&(lane, _)| lane == *index))
                            .all(|index| track.output_peaks[index] < -150.
                                && bus.output_peaks_db[index] < -150.));
                        break;
                    }
                    assert!(
                        Instant::now() < deadline,
                        "track {:?}, bus {:?}",
                        track.output_peaks,
                        bus.output_peaks_db
                    );
                    thread::sleep(Duration::from_millis(2));
                }
                track_id = restored_track.raw();
                bus_id = restored_bus.raw();
                take_id = restored_take.raw();
                session = backend.capture_session().unwrap();
                let saved_track = session
                    .tracks
                    .iter()
                    .find(|entry| entry.source_id == track_id)
                    .unwrap();
                let saved_take = &saved_track.loops[0];
                assert_eq!(saved_take.audio.len(), 2 * width);
                assert_eq!(saved_take.midi[0].events[0].frame, 59);
                assert_eq!(saved_take.midi[0].events[1].frame, 184);
                assert_eq!(
                    session
                        .mixer_routes
                        .iter()
                        .filter(|route| session
                            .buses
                            .iter()
                            .find(|entry| entry.source_id == bus_id)
                            .unwrap()
                            .channels
                            .iter()
                            .any(|channel| channel.source_id == route.destination_channel_id))
                        .count(),
                    width
                );
            }
        }
        drop(backend);
        let report = supervisor.stop();
        assert!(
            report.success && report.cleaned && report.midi_reset,
            "{report:?}"
        );
    }
}

fn wide_loaded_project() -> String {
    include_str!("../../shoop_engine/test_data/carla_legacy_patchbay16_loaded_project.xml")
        .replace("Audio Input:Capture 2<", "Audio Input:Capture 16<")
        .replace("Audio Output:Playback 2<", "Audio Output:Playback 16<")
        .replace(" </Patchbay>", concat!(
            "  <Connection><Source>Audio Input:Capture 8</Source><Target>Audio Output:Playback 8</Target></Connection>\n",
            "  <Connection><Source>Audio Input:Capture 9</Source><Target>Audio Output:Playback 9</Target></Connection>\n",
            " </Patchbay>"))
}

#[shoop_test]
fn patchbay_16x_services_process_loaded_high_audio_lanes_and_capture_live_state() {
    if !runtime_available() {
        return;
    }
    let timing = Timing {
        batch_quanta: 1,
        ..Timing::default()
    }
    .prepare(Quantum::new(64).unwrap())
    .unwrap();
    let midi = MidiLimits {
        events_per_quantum: 32,
        bytes_per_quantum: 128,
    };
    let limits = BufferLimits {
        midi_input: midi,
        midi_output: midi,
        parameters_per_quantum: 0,
    };
    let chain = CarlaChainType::Patchbay16x;
    let project = wide_loaded_project();
    for isolated in [false, true] {
        let (mut endpoint, service) = if isolated {
            ChainService::start_isolated(
                env!("CARGO_BIN_EXE_shoop_rt_host_process").into(),
                ChainId(104),
                chain,
                48_000,
                timing,
                limits,
            )
        } else {
            ChainService::start_in_process(chain, 48_000, timing, limits, None)
        }
        .unwrap();
        service
            .restore_state(encode_state(chain, project.as_bytes()).unwrap())
            .unwrap();
        let monitor = service.monitor();
        let control = service.control();
        let (owner, health) = monitor.owned_health();
        let transport = endpoint.async_transport().unwrap();
        let mut inputs = [0.; 64 * 16];
        for (frame, channel) in [0, 7, 8, 15].into_iter().enumerate() {
            inputs[channel * 64 + 20 + frame] = 0.125 * (frame + 1) as f32;
        }
        let mut midi_input = [MidiBuffer::new(4, 16).unwrap()];
        midi_input[0].push(34, 64, &[0x90, 64, 100]).unwrap();
        let until = Instant::now() + TIMEOUT;
        let mut sequence = 0;
        loop {
            let tag = BatchTag {
                generation: 7,
                sequence,
                source: Frame(sequence * 64),
            };
            assert_no_heap(|| {
                transport.try_submit(AsyncBatch {
                    tag,
                    frames: 64,
                    audio: &inputs,
                    midi: &midi_input,
                    parameters: &[],
                })
            })
            .unwrap();
            let matched = loop {
                let matched = assert_no_heap(|| {
                    let completion = transport.try_completion()?;
                    assert_eq!(completion.tag, tag);
                    Some(completion.output.map(|output| {
                        let impulse = [20, 7 * 64 + 21, 8 * 64 + 22, 15 * 64 + 23]
                            .map(|index| output.audio[index]);
                        let midi = &output.midi[0];
                        let midi = (0..midi.len())
                            .filter(|&index| midi.get(index) == Some((34, &[0x90, 64, 100][..])))
                            .count()
                            == 1;
                        let audio = output.audio.iter().zip(&inputs).enumerate().all(
                            |(index, (&actual, &expected))| {
                                // Carla's stereo gain has a smoothed, nominal unity setting.
                                if index == 20 || index == 15 * 64 + 23 {
                                    (actual - expected).abs() <= 1.0e-5
                                } else {
                                    actual == expected
                                }
                            },
                        );
                        (audio && midi, impulse, midi)
                    }))
                });
                if let Some(matched) = matched {
                    break matched;
                }
                assert!(
                    Instant::now() < until,
                    "waiting for Patchbay 16x output in isolated={isolated}, sequence={sequence}, health={:?}", monitor.owned_health()
                );
                thread::sleep(Duration::from_millis(2));
            };
            if matches!(matched, Ok((true, ..))) {
                break;
            }
            assert!(
                Instant::now() < until,
                "Patchbay 16x audio/MIDI did not match in isolated={isolated}: {matched:?}"
            );
            sequence += 1;
            thread::sleep(Duration::from_millis(20));
        }
        let state = control.capture_state(owner, health.generation).unwrap();
        let stale = control.capture_state(owner + 1, health.generation).unwrap();
        let deadline = Instant::now() + TIMEOUT;
        for (ticket, accepted) in [(state, true), (stale, false)] {
            loop {
                if let Some(result) = ticket.poll() {
                    assert_eq!(result.is_ok(), accepted, "{result:?}");
                    if let Ok(state) = result {
                        let xml = String::from_utf8(decode_state(&state, chain).unwrap()).unwrap();
                        assert!(xml.contains("Audio Output:Playback 16"));
                    }
                    break;
                }
                assert!(Instant::now() < deadline, "waiting for Patchbay 16x state");
                thread::sleep(Duration::from_millis(2));
            }
        }
        service.stop(TIMEOUT).unwrap();
    }
}

#[shoop_test]
fn native_companion_serves_and_releases_a_confirmed_carla_checkpoint() {
    use shoop_rt_engine::EngineDescription;
    use shoop_rt_host_native::{DriverConfig, DummyConfig, NativeConfig};
    use shoop_rt_host_process::{Client, Pending, Started, Transport};
    use shoop_rt_protocol::{
        processors::{CarlaHosting, CarlaSettings},
        BusId, Capabilities, EngineMessage, Outcome, ProcessorCheckpointCommand as Command,
        ProcessorKind, ProcessorSettings, ProcessorTarget, Refusal, Request, SlotId, StateRecord,
        BULK_CHUNK_BYTES,
    };

    if !runtime_available() {
        return;
    }
    fn receive<T>(client: &mut Client, mut inspect: impl FnMut(EngineMessage) -> Option<T>) -> T {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(value) = client.poll().and_then(&mut inspect) {
                return value;
            }
            assert!(
                Instant::now() < deadline,
                "waiting for checkpoint control response"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn result(client: &mut Client, request: Request) -> Outcome {
        let correlation = client.request(request).unwrap();
        receive(client, |message| match message {
            EngineMessage::Result {
                correlation: id,
                outcome,
            } if id == correlation => Some(outcome),
            _ => None,
        })
    }

    let companion = std::path::Path::new(env!("CARGO_BIN_EXE_shoop_rt_host_process"));
    for (hosting, transport, generation) in [
        (CarlaHosting::InProcess, Transport::Socket, 0),
        (CarlaHosting::Subprocess, Transport::SharedMemory, 1),
    ] {
        let Started {
            mut client,
            supervisor,
            ..
        } = Pending::spawn(companion, transport)
            .unwrap()
            .start(&NativeConfig {
                driver: DriverConfig::Dummy(DummyConfig::default()),
                engine: EngineDescription {
                    processors: ProcessorSettings {
                        memory_mb: 256,
                        carla: CarlaSettings {
                            hosting,
                            patchbay_instances: 1,
                            ..CarlaSettings::default()
                        },
                        ..ProcessorSettings::disabled()
                    },
                    ..EngineDescription::default()
                },
            })
            .unwrap();
        client.connect().unwrap();
        let capabilities = receive(&mut client, |message| match message {
            EngineMessage::Welcome { capabilities, .. } => Some(capabilities),
            _ => None,
        });
        assert!(capabilities.contains(Capabilities::PROCESSOR_CHECKPOINTS));
        let bus = BusId(SlotId::new(1, 1));
        assert_eq!(
            result(
                &mut client,
                Request::CreateProcessedBus {
                    bus,
                    channels: 2,
                    kind: ProcessorKind::CarlaPatchbay
                }
            ),
            Outcome::Done
        );
        client.resync().unwrap();
        let slot = receive(&mut client, |message| match message {
            EngineMessage::State(StateRecord::BusProcessor {
                bus: observed,
                slot,
                ..
            }) if observed == bus => slot,
            _ => None,
        });
        let checkpoint = |command| Request::ProcessorCheckpoint {
            target: ProcessorTarget::Bus(bus),
            slot,
            worker_generation: generation,
            command,
        };
        let Outcome::ProcessorCheckpoint { snapshot, bytes } =
            result(&mut client, checkpoint(Command::Open))
        else {
            panic!("expected a real Carla checkpoint");
        };
        assert_eq!(
            result(&mut client, checkpoint(Command::Open)),
            Outcome::Refused(Refusal::Busy)
        );
        assert_eq!(
            result(
                &mut client,
                checkpoint(Command::Read {
                    snapshot,
                    offset: bytes
                })
            ),
            Outcome::Refused(Refusal::InvalidValue)
        );
        let mut state = Vec::with_capacity(bytes as usize);
        while state.len() < bytes as usize {
            let offset = state.len() as u32;
            let correlation = client
                .request(checkpoint(Command::Read { snapshot, offset }))
                .unwrap();
            let page = receive(&mut client, |message| match message {
                EngineMessage::ProcessorCheckpointPage {
                    correlation: id,
                    page,
                } if id == correlation => Some(page),
                EngineMessage::Result {
                    correlation: id,
                    outcome,
                } if id == correlation => panic!("checkpoint page refused: {outcome:?}"),
                _ => None,
            });
            assert_eq!(
                (page.snapshot, page.offset, page.total),
                (snapshot, offset, bytes)
            );
            assert_eq!(
                usize::from(page.len),
                (bytes as usize - state.len()).min(BULK_CHUNK_BYTES)
            );
            state.extend_from_slice(&page.bytes[..usize::from(page.len)]);
        }
        let xml = decode_state(
            std::str::from_utf8(&state).unwrap(),
            CarlaChainType::Patchbay,
        )
        .unwrap();
        assert!(std::str::from_utf8(&xml)
            .unwrap()
            .contains("<CARLA-PROJECT"));
        let requests = (0..8)
            .map(|offset| {
                let correlation = client
                    .request(checkpoint(Command::Read { snapshot, offset }))
                    .unwrap();
                (offset, correlation)
            })
            .collect::<Vec<_>>();
        for (offset, expected) in requests {
            let (correlation, page) = receive(&mut client, |message| match message {
                EngineMessage::ProcessorCheckpointPage { correlation, page } => {
                    Some((correlation, page))
                }
                EngineMessage::Result {
                    correlation,
                    outcome,
                } => panic!("checkpoint burst request {correlation}: {outcome:?}"),
                _ => None,
            });
            assert_eq!(correlation, expected);
            assert_eq!(page.offset, offset);
            let offset = offset as usize;
            let count = usize::from(page.len);
            assert_eq!(&page.bytes[..count], &state[offset..offset + count]);
        }
        assert_eq!(
            result(&mut client, checkpoint(Command::Close { snapshot })),
            Outcome::Done
        );
        assert_eq!(
            result(
                &mut client,
                checkpoint(Command::Read {
                    snapshot,
                    offset: 0
                })
            ),
            Outcome::Refused(Refusal::InvalidId)
        );
        drop(client);
        let report = supervisor.stop();
        assert!(
            report.success && report.cleaned && report.midi_reset,
            "{report:?}"
        );
    }
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn send(
    stream: &mut TcpStream,
    id: u64,
    generation: ProcessGeneration,
    kind: ControlRequestKind,
) -> ControlResponseKind {
    write_frame(
        stream,
        &ParentToWorker::Control(ControlRequest {
            request_id: RequestId(id),
            chain_id: CHAIN,
            generation,
            kind,
        }),
    )
    .unwrap();
    let WorkerToParent::Control(response) = read_frame(stream).unwrap() else {
        panic!("worker sent an unexpected audio result");
    };
    response.validate().unwrap();
    assert_eq!(response.request_id, RequestId(id));
    assert_eq!(response.chain_id, CHAIN);
    assert_eq!(response.generation, GENERATION);
    response.kind
}

#[shoop_test]
fn companion_carla_child_processes_socket_and_shared_audio_and_midi() {
    if !runtime_available() {
        return;
    }
    const FRAMES: usize = 64;
    let nonce = new_nonce().unwrap();
    let mut transport = SharedBlockTransport::create(GENERATION, &nonce).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut worker = Worker(
        Command::new(env!("CARGO_BIN_EXE_shoop_rt_host_process"))
            .arg("--carla-worker")
            .arg("--carla-worker-address")
            .arg(listener.local_addr().unwrap().to_string())
            .arg("--carla-worker-nonce")
            .arg(nonce_hex(&nonce))
            .arg("--carla-worker-chain-id")
            .arg(CHAIN.0.to_string())
            .arg("--carla-worker-generation")
            .arg(GENERATION.0.to_string())
            .arg("--carla-worker-shared-memory")
            .arg(transport.path())
            .spawn()
            .unwrap(),
    );
    let start = Instant::now();
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, peer)) => {
                assert!(peer.ip().is_loopback());
                break stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(
                    worker.0.try_wait().unwrap().is_none(),
                    "Carla child exited before startup"
                );
                assert!(
                    start.elapsed() < TIMEOUT,
                    "Carla child connection timed out"
                );
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("Carla connection failed: {error}"),
        }
    };
    stream.set_read_timeout(Some(TIMEOUT)).unwrap();
    stream.set_write_timeout(Some(TIMEOUT)).unwrap();
    let WorkerToParent::Control(ControlResponse {
        request_id: RequestId(1),
        chain_id: CHAIN,
        generation: GENERATION,
        kind: ControlResponseKind::Handshake(hello),
    }) = read_frame(&mut stream).unwrap()
    else {
        panic!("invalid Carla handshake");
    };
    hello.validate(&nonce, GENERATION).unwrap();
    let stale = send(
        &mut stream,
        2,
        ProcessGeneration(GENERATION.0 - 1),
        ControlRequestKind::Ping,
    );
    assert!(
        matches!(stale, ControlResponseKind::Error(error) if error.code == ProtocolErrorCode::StaleGeneration)
    );
    assert_eq!(
        send(
            &mut stream,
            3,
            GENERATION,
            ControlRequestKind::Instantiate {
                chain_type: CarlaChainType::Rack,
                sample_rate: 48_000,
                nominal_buffer_size: FRAMES as u32,
            }
        ),
        ControlResponseKind::Ack
    );
    let ControlResponseKind::State(state) =
        send(&mut stream, 4, GENERATION, ControlRequestKind::SaveState)
    else {
        panic!("missing Carla Native state");
    };
    assert!(state.starts_with("shoop-carla-native-state:2:rack:"));
    let mut xml = shoop_carla::state::decode_state(&state, CarlaChainType::Rack).unwrap();
    xml.extend_from_slice(b"\n<!--");
    xml.resize(shoop_plugin_protocol::MAX_STATE_BYTES - 3, b'x');
    xml.extend_from_slice(b"-->");
    let state = shoop_carla::state::encode_state(CarlaChainType::Rack, &xml).unwrap();
    drop(xml);
    assert_eq!(
        send(
            &mut stream,
            5,
            GENERATION,
            ControlRequestKind::RestoreState(state)
        ),
        ControlResponseKind::Ack
    );
    assert_eq!(
        send(
            &mut stream,
            6,
            GENERATION,
            ControlRequestKind::SetActive(true)
        ),
        ControlResponseKind::Ack
    );

    let audio_inputs = [vec![0.125; FRAMES], vec![-0.75; FRAMES]];
    let midi = MidiEvent {
        frame_offset: 17,
        data: vec![0x90, 60, 100],
    };
    write_frame(
        &mut stream,
        &ParentToWorker::Process(PrototypeBlock {
            sequence: BlockSequence(1),
            generation: GENERATION,
            frames: FRAMES as u32,
            audio_inputs: audio_inputs.to_vec(),
            midi_inputs: vec![midi.clone()],
        }),
    )
    .unwrap();
    let WorkerToParent::Process(result) = read_frame(&mut stream).unwrap() else {
        panic!("missing Carla DSP result");
    };
    result.validate().unwrap();
    assert_eq!(
        (result.sequence, result.generation),
        (BlockSequence(1), GENERATION)
    );
    assert_eq!(result.audio_outputs, audio_inputs);
    assert_eq!(result.midi_outputs, [midi.clone()]);

    let token = transport
        .submit(
            BlockSequence(2),
            FRAMES,
            &audio_inputs,
            2,
            &[vec![(midi.frame_offset, midi.data.clone())]],
            &[1],
        )
        .unwrap();
    let notification = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    notification
        .send_to(&nonce[..16], (Ipv4Addr::LOCALHOST, hello.notification_port))
        .unwrap();
    let mut outputs = [vec![0.; FRAMES], vec![0.; FRAMES]];
    let mut events = Vec::new();
    transport
        .wait_and_copy(token, Instant::now() + TIMEOUT, &mut outputs, &mut events)
        .unwrap();
    assert_eq!(outputs, audio_inputs);
    assert_eq!(events, [midi]);

    let ControlResponseKind::Status(status) =
        send(&mut stream, 7, GENERATION, ControlRequestKind::Status)
    else {
        panic!("missing Carla status");
    };
    assert!(status.active && status.ready);
    assert_eq!(status.processed_blocks, 2);
    assert_eq!(
        send(&mut stream, 8, GENERATION, ControlRequestKind::Shutdown),
        ControlResponseKind::Ack
    );
    let start = Instant::now();
    loop {
        if let Some(exit) = worker.0.try_wait().unwrap() {
            assert!(exit.success(), "Carla child exited with {exit}");
            break;
        }
        assert!(start.elapsed() < TIMEOUT, "Carla child shutdown timed out");
        thread::sleep(Duration::from_millis(1));
    }
}

#[shoop_test]
fn a_lost_carla_child_recovers_its_checkpoint_while_another_chain_keeps_running() {
    if !runtime_available() {
        return;
    }
    const FRAMES: usize = 256;
    let launch = |id| {
        let mut chain = SupervisedCarlaProcessor::launch(
            env!("CARGO_BIN_EXE_shoop_rt_host_process"),
            CarlaChainType::Rack,
            48_000,
            FRAMES as u32,
            ChainId(id),
        )
        .unwrap();
        assert!(chain.is_ready(), "{:?}", chain.crash_summary());
        chain.set_active(true);
        assert!(chain.is_active());
        chain
    };
    let mut recover = launch(100);
    let mut unaffected = launch(101);
    let project = include_str!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml")
        .replacen("<Value>1</Value>", "<Value>0.5</Value>", 1);
    let checkpoint = encode_state(CarlaChainType::Rack, project.as_bytes()).unwrap();
    recover.restore_state(&checkpoint).unwrap();
    let confirm_audio = |chain: &mut SupervisedCarlaProcessor, expected: f32| {
        for _ in 0..12 {
            chain.audio_input_mut(0).unwrap()[..FRAMES].fill(0.25);
            chain.audio_input_mut(1).unwrap()[..FRAMES].fill(-0.25);
            chain.process(FRAMES).unwrap();
            let left = &chain.audio_output(0).unwrap()[..FRAMES];
            let right = &chain.audio_output(1).unwrap()[..FRAMES];
            if left
                .iter()
                .all(|sample| (*sample - expected).abs() < 0.0001)
                && right
                    .iter()
                    .all(|sample| (*sample + expected).abs() < 0.0001)
            {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!(
            "worker audio was {}, expected {expected}",
            chain.audio_output(0).unwrap()[FRAMES - 1]
        );
    };
    confirm_audio(&mut recover, 0.125);
    confirm_audio(&mut unaffected, 0.25);
    let generation = recover.generation();
    let unaffected_generation = unaffected.generation();
    recover.terminate_worker_for_test().unwrap();
    assert!(!recover.is_ready());
    assert_eq!(recover.lifecycle(), CarlaProcessorLifecycle::Crashed);
    confirm_audio(&mut unaffected, 0.25);
    recover.recover(false).unwrap();
    assert_eq!(recover.generation(), generation + 1);
    assert_eq!(unaffected.generation(), unaffected_generation);
    assert!(recover.is_active());
    confirm_audio(&mut recover, 0.125);
    let logs = recover.generation_logs();
    assert_eq!(
        logs.iter().map(|log| log.generation).collect::<Vec<_>>(),
        [generation, generation + 1]
    );
    recover.clear_logs();
    let logs = recover.generation_logs();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].generation, generation + 1);
    assert!(logs[0].stdout.is_empty() && logs[0].stderr.is_empty());

    // Simulate a checkpoint whose plugin was unavailable on the replacement worker.
    let missing = project.replacen(
        "<Label>audiogain_s</Label>",
        "<Label>shoop-missing-plugin-fixture</Label>",
        1,
    );
    assert_ne!(missing, project);
    let missing = encode_state(CarlaChainType::Rack, missing.as_bytes()).unwrap();
    recover.terminate_worker_for_test().unwrap();
    assert!(!recover.is_ready());
    recover.restore_state(&missing).unwrap();
    assert!(recover
        .recover(false)
        .unwrap_err()
        .to_string()
        .contains("different plugin inventory"));
    assert_eq!(recover.lifecycle(), CarlaProcessorLifecycle::Unavailable);
    assert!(!recover.is_active());
    assert!(recover.worker_id().is_none());
    confirm_audio(&mut unaffected, 0.25);

    let files = tempfile::tempdir().unwrap();
    let source = files.path().join("checkpoint.mid");
    std::fs::write(
        &source,
        b"MThd\0\0\0\x06\0\0\0\x01\0\x60MTrk\0\0\0\x0c\0\x90\x3c\x64\x60\x80\x3c\0\0\xff\x2f\0",
    )
    .unwrap();
    let path = source
        .to_str()
        .unwrap()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let project = format!(
        "<CARLA-PROJECT VERSION='2.5'><Plugin><Info><Type>INTERNAL</Type>\
         <Label>midifile</Label></Info><Data><Active>Yes</Active><CustomData>\
         <Type>http://kxstudio.sf.net/ns/carla/string</Type><Key>file</Key>\
         <Value>{path}</Value></CustomData></Data></Plugin></CARLA-PROJECT>"
    );
    let checkpoint = encode_state(CarlaChainType::Rack, project.as_bytes()).unwrap();
    recover.restore_state(&checkpoint).unwrap();
    recover.recover(false).unwrap();
    assert!(recover.is_ready());
    recover.terminate_worker_for_test().unwrap();
    assert!(!recover.is_ready());
    std::fs::remove_file(&source).unwrap();
    let refusal = recover.recover(false).unwrap_err();
    assert!(
        refusal
            .to_string()
            .contains("Carla source file is unavailable"),
        "{refusal:#}"
    );
    assert_eq!(recover.lifecycle(), CarlaProcessorLifecycle::Unavailable);
    assert!(!recover.is_active());
    assert!(recover.worker_id().is_none());
    confirm_audio(&mut unaffected, 0.25);
}

#[shoop_test]
fn isolated_service_uses_the_prepared_render_endpoint() {
    if !runtime_available() {
        return;
    }
    const FRAMES: usize = 256;
    let prepared = Timing::default()
        .prepare(Quantum::new(128).unwrap())
        .unwrap();
    assert_eq!(prepared.batch_frames(), FRAMES);
    let midi = MidiLimits {
        events_per_quantum: 32,
        bytes_per_quantum: 128,
    };
    let limits = BufferLimits {
        midi_input: midi,
        midi_output: midi,
        parameters_per_quantum: 0,
    };
    let (mut endpoint, service) = ChainService::start_isolated(
        env!("CARGO_BIN_EXE_shoop_rt_host_process").into(),
        ChainId(102),
        CarlaChainType::Rack,
        48_000,
        prepared,
        limits,
    )
    .unwrap();
    let checkpoint = service.save_state().unwrap();
    service.restore_state(checkpoint).unwrap();
    service.set_active(true).unwrap();

    let mut inputs = [0.25; FRAMES * 2];
    inputs[FRAMES..].fill(-0.625);
    let mut midi_input = [MidiBuffer::new(4, 16).unwrap()];
    midi_input[0].push(63, FRAMES, &[0x90, 64, 100]).unwrap();
    let transport = endpoint.async_transport().unwrap();
    let mut received = false;
    for sequence in 0..8 {
        let tag = BatchTag {
            generation: 7,
            sequence,
            source: Frame(sequence * FRAMES as u64),
        };
        assert_no_heap(|| {
            transport.try_submit(AsyncBatch {
                tag,
                frames: FRAMES,
                audio: &inputs,
                midi: &midi_input,
                parameters: &[],
            })
        })
        .unwrap();
        let start = Instant::now();
        loop {
            let result = assert_no_heap(|| {
                let completion = transport.try_completion()?;
                assert_eq!(completion.tag, tag);
                Some(match completion.output {
                    Ok(output) => {
                        assert_eq!(output.audio, inputs);
                        let midi = &output.midi[0];
                        assert_eq!(midi.get(midi.len() - 1), Some((63, &[0x90, 64, 100][..])));
                        true
                    }
                    Err(error) => {
                        assert_eq!(error, rt_runtime::Error::Incomplete);
                        false
                    }
                })
            });
            if let Some(result) = result {
                received = result;
                break;
            }
            assert!(start.elapsed() < TIMEOUT, "isolated Carla audio timed out");
            thread::sleep(Duration::from_micros(200));
        }
        if received {
            break;
        }
    }
    assert!(received);
    let project =
        include_bytes!("../../shoop_engine/test_data/carla_legacy_rack_loaded_project.xml");
    service
        .restore_state(encode_state(CarlaChainType::Rack, project).unwrap())
        .unwrap();
    let has_gain = |state: &str| {
        let xml = decode_state(state, CarlaChainType::Rack).unwrap();
        String::from_utf8(xml).unwrap().contains("audiogain_s")
    };
    assert!(has_gain(&service.save_state().unwrap()));
    let monitor = service.monitor();
    let control = service.control();
    let (owner, health) = monitor.owned_health();
    assert!(!assert_no_heap(|| endpoint.reset_step(0)).unwrap());
    let deadline = Instant::now() + TIMEOUT;
    let mut step = 1;
    while !assert_no_heap(|| endpoint.reset_step(step)).unwrap() {
        assert!(
            Instant::now() < deadline,
            "Carla owner retirement timed out"
        );
        thread::sleep(Duration::from_millis(1));
        step += 1;
    }
    assert!(!has_gain(&service.save_state().unwrap()));
    let (new_owner, new_health) = monitor.owned_health();
    assert_ne!(new_owner, owner);
    let stale = control.clear_logs(owner, health.generation).unwrap();
    let current = control
        .clear_logs(new_owner, new_health.generation)
        .unwrap();
    let deadline = Instant::now() + TIMEOUT;
    for (ticket, accepted) in [(stale, false), (current, true)] {
        loop {
            if let Some(outcome) = ticket.poll() {
                assert_eq!(outcome.is_ok(), accepted, "{outcome:?}");
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Carla log control was not processed"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    let stale = control.read_logs(owner, health.generation).unwrap();
    let current = control.read_logs(new_owner, new_health.generation).unwrap();
    let deadline = Instant::now() + TIMEOUT;
    for (ticket, accepted) in [(stale, false), (current, true)] {
        loop {
            if let Some(outcome) = ticket.poll() {
                assert_eq!(outcome.is_ok(), accepted, "{outcome:?}");
                if let Ok(logs) = outcome {
                    assert_eq!(logs.len(), 1);
                    assert_eq!(logs[0].generation, new_health.generation);
                }
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Carla log read was not processed"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    service.stop(TIMEOUT).unwrap();
}
