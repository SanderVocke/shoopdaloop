//! The core workflow through the cooperative runtime on the new engine backend: the adapter
//! over the in-process carrier, the engine and the offline host.

use std::sync::Arc;
use std::time::Duration;

use shoop_app::CooperativeApplicationRuntime;
use shoop_app_api::{
    AppIntent, AppSnapshot, ApplicationPortOwner, BackendCapabilities, BackendCapability,
    BusAction, BusSpec, ConfirmedConnectionState, DirectTrackSpec, GlobalControlAction, HostPortId,
    IoTaskStatus, LoopAction, LoopMode, MidiNote, MixerRouteState, NotificationLevel, PianoAction,
    PortId, PortRole, SelectionModifiers, StructuralState, TrackAction, TrackId, TrackState,
};
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::testing::OfflineRender;
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const TICK: Duration = Duration::from_millis(4);
const CAPTURE: f32 = 0.5;

fn capture(port: usize, _frame: u64) -> f32 {
    if port == 0 {
        CAPTURE
    } else {
        0.0
    }
}

/// Renders the offline host as the application lets time pass.
struct OfflineClock(OfflineRender);

impl EngineClock for OfflineClock {
    fn advance(&mut self, elapsed: Duration) {
        self.0.run_for(elapsed, capture).expect("offline rendering");
    }

    fn wait(&mut self) -> bool {
        self.0.run(1, capture).expect("offline rendering");
        true
    }
}

fn start() -> CooperativeApplicationRuntime {
    let description = EngineDescription {
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 8,
            loops: 32,
        }],
        ramp_frames: 4,
        resource_pages: 2048,
        page_bytes: 256,
        midi_events: 64,
        midi_bytes: 512,
        pending_transitions: 64,
        telemetry_interval_frames: 0,
        ..EngineDescription::default()
    };
    let (offline, client) = OfflineRender::new(&description).expect("engine preparation");
    let backend = RtBackend::connect(client, OfflineClock(offline)).expect("connection");
    CooperativeApplicationRuntime::start(Box::new(backend)).expect("application start")
}

fn tick_until(
    runtime: &mut CooperativeApplicationRuntime,
    what: &str,
    done: impl Fn(&AppSnapshot) -> bool,
) -> Arc<AppSnapshot> {
    for _ in 0..1000 {
        runtime.tick(TICK);
        let snapshot = runtime.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
    }
    let snapshot = runtime.snapshot();
    let tracks = snapshot
        .tracks
        .iter()
        .map(|track| {
            let state = &track.loops[0];
            (
                track.name.as_str(),
                track.loops.len(),
                state.mode,
                state.next_mode,
                state.next_transition_delay,
                state.sync,
                state.length_frames,
            )
        })
        .collect::<Vec<_>>();
    panic!(
        "{what} did not happen; tracks: {tracks:?}; notifications: {:?}",
        snapshot.notifications
    );
}

fn track<'a>(snapshot: &'a AppSnapshot, id: TrackId) -> &'a TrackState {
    snapshot
        .tracks
        .iter()
        .find(|track| track.id == id)
        .expect("the track exists")
}

fn ports(snapshot: &AppSnapshot, track_id: TrackId, role: PortRole) -> Vec<PortId> {
    snapshot
        .connections
        .application_ports
        .iter()
        .filter(|port| {
            matches!(port.owner, ApplicationPortOwner::Track { track_id: owner, .. } if owner == track_id)
                && port.role == role
        })
        .map(|port| port.id)
        .collect()
}

fn loop_action(
    runtime: &mut CooperativeApplicationRuntime,
    track: &TrackState,
    action: LoopAction,
) {
    runtime
        .dispatch(AppIntent::Loop {
            track_id: track.id,
            loop_id: track.loops[0].id,
            action,
        })
        .unwrap();
}

fn track_action(
    runtime: &mut CooperativeApplicationRuntime,
    track_id: TrackId,
    action: TrackAction,
) {
    runtime
        .dispatch(AppIntent::Track { track_id, action })
        .unwrap();
}

fn first_loop(snapshot: &AppSnapshot, id: TrackId) -> &shoop_app_api::LoopState {
    &track(snapshot, id).loops[0]
}

#[shoop_test]
fn core_workflow_runs_on_the_new_engine_backend() {
    let mut runtime = start();
    let snapshot = tick_until(&mut runtime, "startup", |snapshot| {
        snapshot.tracks.len() == 1 && snapshot.connections.backend_available
    });
    assert_eq!(
        snapshot.capabilities,
        BackendCapabilities::NONE.with(BackendCapability::MidiInjection, true)
    );

    runtime.dispatch(AppIntent::RequestNewSession).unwrap();
    let snapshot = tick_until(&mut runtime, "a new session", |snapshot| {
        snapshot
            .io_task
            .as_ref()
            .is_some_and(|task| task.status == IoTaskStatus::Completed)
    });
    assert_eq!(snapshot.tracks.len(), 1);
    assert!(snapshot.tracks[0].is_sync);
    let sync = snapshot.tracks[0].clone();
    let master = snapshot.buses[0].clone();
    assert_eq!(master.name, "Master");
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)))
        .unwrap();

    for (name, audio_channels, midi) in [("Audio", 2, false), ("Keys", 0, true)] {
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: name.to_owned(),
                audio_channels,
                midi,
            }))
            .unwrap();
    }
    let snapshot = tick_until(&mut runtime, "two new tracks", |snapshot| {
        snapshot.tracks.len() == 3
            && snapshot.tracks.iter().all(|track| {
                track.structural_state == StructuralState::Confirmed
                    && track
                        .loops
                        .iter()
                        .all(|state| state.structural_state == StructuralState::Confirmed)
            })
    });
    let audio = snapshot.tracks[1].clone();
    let keys = snapshot.tracks[2].clone();

    let audio_inputs = ports(&snapshot, audio.id, PortRole::AudioInput);
    let audio_outputs = ports(&snapshot, audio.id, PortRole::AudioOutput);
    let keys_input = ports(&snapshot, keys.id, PortRole::MidiInput);
    assert_eq!((audio_inputs.len(), audio_outputs.len()), (2, 2));
    let mut links = Vec::new();
    for (port, host) in audio_inputs
        .iter()
        .map(|port| (*port, "audio_in_1"))
        .chain(keys_input.iter().map(|port| (*port, "midi_in_1")))
    {
        runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id: port,
                host_port_id: HostPortId::new(host),
                connected: true,
            })
            .unwrap();
        links.push(ConfirmedConnectionState {
            application_port_id: port,
            host_port_id: HostPortId::new(host),
        });
    }
    let routes = audio_outputs
        .iter()
        .zip(master.channels.iter())
        .map(|(port, channel)| MixerRouteState {
            source_port_id: *port,
            destination_channel_id: channel.id,
        })
        .collect::<Vec<_>>();
    for route in &routes {
        runtime
            .dispatch(AppIntent::SetMixerRouteConnected {
                source_port_id: route.source_port_id,
                destination_channel_id: route.destination_channel_id,
                connected: true,
            })
            .unwrap();
    }
    tick_until(&mut runtime, "connections and routes", |snapshot| {
        let connections = &snapshot.connections;
        links
            .iter()
            .all(|link| connections.confirmed_links.contains(link))
            && routes
                .iter()
                .all(|route| connections.mixer_links.contains(route))
    });

    loop_action(&mut runtime, &sync, LoopAction::RecordClicked);
    tick_until(&mut runtime, "sync recording", |snapshot| {
        first_loop(snapshot, sync.id).mode == LoopMode::Recording
    });
    for _ in 0..10 {
        runtime.tick(TICK);
    }
    loop_action(&mut runtime, &sync, LoopAction::PlayClicked);
    let snapshot = tick_until(&mut runtime, "sync playback", |snapshot| {
        first_loop(snapshot, sync.id).mode == LoopMode::Playing
    });
    let sync_length = first_loop(&snapshot, sync.id).length_frames;
    assert!(sync_length >= 40, "sync length {sync_length}");

    loop_action(&mut runtime, &audio, LoopAction::RecordClicked);
    tick_until(
        &mut runtime,
        "a planned synchronized recording",
        |snapshot| {
            let state = first_loop(snapshot, audio.id);
            state.mode == LoopMode::Stopped && state.next_mode == LoopMode::Recording
        },
    );
    tick_until(&mut runtime, "synchronized recording", |snapshot| {
        first_loop(snapshot, audio.id).mode == LoopMode::Recording
    });
    loop_action(&mut runtime, &audio, LoopAction::PlayClicked);
    let snapshot = tick_until(&mut runtime, "synchronized playback", |snapshot| {
        first_loop(snapshot, audio.id).mode == LoopMode::Playing
    });
    assert_eq!(first_loop(&snapshot, audio.id).length_frames, sync_length);

    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    track_action(
        &mut runtime,
        keys.id,
        TrackAction::InputMonitoringChanged {
            enabled: true,
            respect_auto_mute: false,
        },
    );
    loop_action(&mut runtime, &keys, LoopAction::RecordClicked);
    tick_until(&mut runtime, "immediate MIDI recording", |snapshot| {
        first_loop(snapshot, keys.id).mode == LoopMode::Recording
            && track(snapshot, keys.id).controls.input_monitoring
    });
    let note = MidiNote::new(60).unwrap();
    runtime
        .dispatch(AppIntent::Piano(PianoAction::Press(note)))
        .unwrap();
    for _ in 0..3 {
        runtime.tick(TICK);
    }
    runtime
        .dispatch(AppIntent::Piano(PianoAction::Release(note)))
        .unwrap();
    for _ in 0..3 {
        runtime.tick(TICK);
    }
    loop_action(&mut runtime, &keys, LoopAction::PlayClicked);
    tick_until(&mut runtime, "immediate MIDI playback", |snapshot| {
        let state = first_loop(snapshot, keys.id);
        state.mode == LoopMode::Playing && state.length_frames > 0
    });

    track_action(&mut runtime, audio.id, TrackAction::OutputGainChanged(-6.0));
    track_action(
        &mut runtime,
        audio.id,
        TrackAction::OutputBalanceChanged(0.5),
    );
    track_action(
        &mut runtime,
        audio.id,
        TrackAction::InputMonitoringChanged {
            enabled: true,
            respect_auto_mute: false,
        },
    );
    for action in [
        BusAction::GainChanged(-3.0),
        BusAction::BalanceChanged(-0.25),
    ] {
        runtime
            .dispatch(AppIntent::Bus {
                bus_id: master.id,
                action,
            })
            .unwrap();
    }
    let snapshot = tick_until(&mut runtime, "controls and meters", |snapshot| {
        let controls = &track(snapshot, audio.id).controls;
        let master = &snapshot.buses[0];
        controls.output_gain_db == -6.0
            && controls.output_balance == 0.5
            && controls.input_monitoring
            && controls.input_peak_left_db > -7.0
            && controls.output_peak_left_db > -100.0
            && master.gain_db == -3.0
            && master.balance == -0.25
            && master.output_peaks_db.iter().all(|peak| *peak > -100.0)
    });
    assert!(!snapshot.buses[0].control_pending);

    loop_action(
        &mut runtime,
        &audio,
        LoopAction::IconClicked(SelectionModifiers::default()),
    );
    let snapshot = tick_until(&mut runtime, "the waveform", |snapshot| {
        snapshot.details.as_ref().is_some_and(|details| {
            details.loop_id == audio.loops[0].id
                && details.channels.len() == 2
                && details
                    .channels
                    .iter()
                    .all(|channel| channel.samples.len() as u64 == sync_length)
        })
    });
    let details = snapshot.details.as_ref().unwrap();
    assert!(details.channels[0]
        .samples
        .iter()
        .all(|sample| (*sample - CAPTURE).abs() < 1e-6));

    loop_action(
        &mut runtime,
        &keys,
        LoopAction::IconClicked(SelectionModifiers::default()),
    );
    let snapshot = tick_until(&mut runtime, "the MIDI detail", |snapshot| {
        snapshot.details.as_ref().is_some_and(|details| {
            details.loop_id == keys.loops[0].id
                && details
                    .midi_channels
                    .first()
                    .is_some_and(|channel| channel.events.len() == 2)
        })
    });
    let events = &snapshot.details.as_ref().unwrap().midi_channels[0].events;
    assert!(events[0].frame < events[1].frame);

    loop_action(&mut runtime, &audio, LoopAction::StopClicked);
    tick_until(&mut runtime, "immediate stop", |snapshot| {
        first_loop(snapshot, audio.id).mode == LoopMode::Stopped
    });
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::ClearRecordings {
            include_sync: false,
        }))
        .unwrap();
    let snapshot = tick_until(&mut runtime, "cleared loops", |snapshot| {
        [audio.id, keys.id].iter().all(|id| {
            let state = first_loop(snapshot, *id);
            state.mode == LoopMode::Stopped && state.length_frames == 0
        })
    });
    assert_eq!(first_loop(&snapshot, sync.id).mode, LoopMode::Playing);
    let errors = snapshot
        .notifications
        .iter()
        .filter(|notification| notification.level == NotificationLevel::Error)
        .collect::<Vec<_>>();
    assert!(errors.is_empty(), "{errors:?}");
}

#[shoop_test]
fn unsupported_actions_are_refused_visibly_on_the_new_engine_backend() {
    let mut runtime = start();
    tick_until(&mut runtime, "startup", |snapshot| {
        snapshot.tracks.len() == 1
    });
    runtime
        .dispatch(AppIntent::AddBus(BusSpec {
            name: "Cue".to_owned(),
            channel_count: 2,
            fx: None,
            creation_request_id: None,
        }))
        .unwrap();
    let snapshot = tick_until(&mut runtime, "a refusal", |snapshot| {
        !snapshot.notifications.is_empty()
    });
    assert!(snapshot
        .notifications
        .iter()
        .any(|notification| notification
            .message
            .contains(BackendCapability::BusManagement.unavailable_reason())));
    assert_eq!(snapshot.buses.len(), 1);
}
