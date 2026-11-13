use super::*;
use crate::application::{ApplicationRuntime, ApplicationStartOptions};
use shoop_egui::*;
use shoop_session::*;

pub fn run(transport: EngineTransportKind) -> Result<()> {
    let (root, mut host, entry) = launch_with_settings(
        transport,
        HostSettings {
            backend: Default::default(),
            driver: AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 8000,
                buffer_size: 128,
            }),
            pools: NewEnginePools {
                quantum: 16,
                buses: 2,
                bus_channels: 2,
                tracks: 3,
                loops: 24,
                channels: 2,
                loop_memory_mb: 32,
            },
            processors: ProcessorSettings {
                builtin_fx: ProcessorPolicy::On,
                oxisynth: ProcessorPolicy::Off,
                fx_instances: 1,
                synth_instances: 0,
                memory_mb: 64,
            },
            master_auto_connect: true,
            loop_smoothing_ms: 0,
        },
    )?;
    let app = ApplicationRuntime::attach(
        entry.clone(),
        ApplicationStartOptions {
            master_auto_connect: true,
            loop_smoothing_ms: 0,
            ..Default::default()
        },
    )?;
    let handle = app.handle();
    file_wait(&handle, |s| {
        !s.tracks.is_empty() && s.capabilities.supports(BackendCapability::BusFx)
    })?;
    handle.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
    handle.dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
        name: "Bus FX source".into(),
        topology: TrackSpecTopology::Direct {
            audio_channels: 2,
            midi: false,
        },
        latency: Default::default(),
        initial_output_bus_names: vec!["Master".into()],
        creation_request_id: None,
    }))?;
    let state = file_wait(&handle, |s| {
        s.tracks
            .iter()
            .any(|t| t.name == "Bus FX source" && !t.loops.is_empty())
    })?;
    let track = state
        .tracks
        .iter()
        .find(|t| t.name == "Bus FX source")
        .unwrap();
    let (track_id, loop_id, bus_id) = (track.id, track.loops[0].id, state.buses[0].id);
    handle.dispatch(AppIntent::ImportLoopAudioBytes {
        loop_id,
        name: "bus.wav".into(),
        bytes: encode_float_wav(&LoopAudio {
            sample_rate: 8000,
            channels: (0..2)
                .map(|c| LoopAudioChannel {
                    label: format!("Audio {c}"),
                    role: "direct".into(),
                    samples: vec![0.25; 4096],
                })
                .collect(),
        })?
        .into(),
        update_loop_length: true,
    })?;
    let state = file_wait(&handle, |s| {
        s.io_task.as_ref().is_some_and(|t| {
            matches!(
                t.status,
                IoTaskStatus::AwaitingChannelMapping
                    | IoTaskStatus::Completed
                    | IoTaskStatus::Failed
            )
        })
    })?;
    if state.io_task.as_ref().unwrap().status == IoTaskStatus::AwaitingChannelMapping {
        handle.dispatch(AppIntent::ConfirmAudioChannelMapping {
            task_id: state.io_task.as_ref().unwrap().id,
            source_for_destination: vec![0, 1],
        })?;
    }
    file_done(&handle, IoTaskKind::ImportLoopAudio)?;
    handle.dispatch(AppIntent::Loop {
        track_id,
        loop_id,
        action: LoopAction::PlayClicked,
    })?;
    file_wait(&handle, |s| {
        s.buses[0]
            .output_peaks_db
            .iter()
            .any(|v| (*v + 12.0412).abs() < 0.1)
    })?;
    let bus = |action| AppIntent::Bus { bus_id, action };
    handle.dispatch(bus(BusAction::FxProcessorChanged(Some(
        TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
    ))))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().is_some_and(|fx| fx.active)
    })?;
    handle.dispatch(bus(BusAction::BuiltInFx(
        BuiltInFxControl::SetReverbEnabled(false),
    )))?;
    handle.dispatch(bus(BusAction::BuiltInFx(
        BuiltInFxControl::SetStageEnabled(BuiltInFxStage::Drive, true),
    )))?;
    handle.dispatch(bus(BusAction::BuiltInFx(BuiltInFxControl::SetParameter(
        BuiltInFxParameter::DriveMix,
        0.,
    ))))?;
    handle.dispatch(bus(BusAction::BuiltInFx(BuiltInFxControl::SetParameter(
        BuiltInFxParameter::DriveOutput,
        -18.,
    ))))?;
    handle.dispatch(bus(BusAction::FxVisibilityChanged(true)))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().is_some_and(|fx| fx.visible)
            && s.buses[0]
                .output_peaks_db
                .iter()
                .all(|v| (*v + 30.0412).abs() < 0.1)
    })?;
    handle.dispatch(bus(BusAction::FxActiveChanged(false)))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().is_some_and(|fx| !fx.active)
            && s.buses[0]
                .output_peaks_db
                .iter()
                .all(|v| (*v + 12.0412).abs() < 0.1)
    })?;
    handle.dispatch(bus(BusAction::FxActiveChanged(true)))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().is_some_and(|fx| fx.active)
    })?;
    let configured = handle.snapshot().buses[0]
        .fx
        .as_ref()
        .unwrap()
        .editor
        .as_ref()
        .unwrap()
        .canonical_state()?;
    handle.dispatch(bus(BusAction::FxProcessorChanged(Some(
        TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
    ))))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().and_then(|fx| fx.editor.as_ref()).is_some_and(
            |editor| matches!(editor, TrackProcessorEditorState::BuiltInFx(fx) if fx.reverb_enabled && !fx.drive_enabled)
        )
    })?;
    handle.dispatch(bus(BusAction::FxRestoreState(configured.clone())))?;
    file_wait(&handle, |s| {
        s.buses[0]
            .output_peaks_db
            .iter()
            .all(|v| (*v + 30.0412).abs() < 0.1)
    })?;
    handle.dispatch(bus(BusAction::FxProcessorChanged(None)))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.is_none()
            && s.buses[0]
                .output_peaks_db
                .iter()
                .all(|v| (*v + 12.0412).abs() < 0.1)
    })?;
    handle.dispatch(bus(BusAction::FxProcessorChanged(Some(
        TrackProcessorTypeId::new(TrackProcessorTypeId::BUILTIN_FX),
    ))))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().is_some_and(|fx| fx.active)
    })?;
    handle.dispatch(bus(BusAction::FxRestoreState(configured)))?;
    file_wait(&handle, |s| {
        s.buses[0]
            .output_peaks_db
            .iter()
            .all(|v| (*v + 30.0412).abs() < 0.1)
    })?;
    handle.dispatch(AppIntent::AddEphemeralScript { name: "Bus FX fader".into(), source: "shoop_announce_api_version(1, 6); local c = require('shoop_control'); c.bus_set_gain(0, 0.5)".into(), source_path: None })?;
    file_wait(&handle, |s| {
        (s.buses[0].gain_db + 6.0206).abs() < 0.1
            && s.buses[0]
                .output_peaks_db
                .iter()
                .all(|v| (*v + 36.0618).abs() < 0.1)
    })?;
    handle.dispatch(bus(BusAction::BuiltInFx(BuiltInFxControl::AssignMidiCc(
        BuiltInFxMidiCcAssignment {
            parameter: BuiltInFxParameter::DriveOutput,
            channel: 2,
            controller: 17,
        },
    ))))?;
    file_wait(&handle, |s| {
        s.buses[0].fx.as_ref().and_then(|fx| fx.editor.as_ref()).is_some_and(|editor| matches!(editor, TrackProcessorEditorState::BuiltInFx(fx) if fx.midi_cc_assignments.len() == 1))
    })?;
    let saved = root.path().join("busfx.shoop");
    let before = file_save(&handle, &saved)?;
    ensure!(
        before.document.buses[0].fx_chain.is_some(),
        "saved bus processor missing"
    );
    handle.dispatch(AppIntent::LoadSessionBytes {
        name: "busfx.shoop".into(),
        bytes: std::fs::read(&saved)?.into(),
    })?;
    file_done(&handle, IoTaskKind::LoadSession)?;
    let after = file_save(&handle, &root.path().join("busfx-again.shoop"))?;
    let old_fx = before.document.buses[0].fx_chain.as_ref().unwrap();
    let new_fx = after.document.buses[0].fx_chain.as_ref().unwrap();
    ensure!(
        old_fx.internal_state == new_fx.internal_state
            && old_fx.builtin_fx_midi_cc_assignments == new_fx.builtin_fx_midi_cc_assignments,
        "bus processor file state changed"
    );
    ensure!(
        handle.snapshot().buses[0].id != bus_id,
        "load reused live bus identity"
    );
    let ticket = if transport == EngineTransportKind::Socket {
        drop(handle);
        drop(app);
        std::thread::sleep(Duration::from_millis(50));
        let (mut client, connection) = attach(&entry)?;
        ensure!(
            client.snapshot().unwrap().buses[0].fx.is_some(),
            "reattachment lost bus FX"
        );
        let ticket = client.shutdown().map_err(|e| anyhow!("{e:?}"))?;
        drop(connection);
        ticket
    } else {
        let ticket = handle.prepare_shutdown()?;
        drop(app);
        ticket
    };
    crate::recovery::shutdown(&entry, ticket)
        .0
        .map_err(anyhow::Error::msg)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.0.try_wait()?.is_none() {
        ensure!(Instant::now() < deadline, "bus FX host shutdown timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    println!(
        "Bus FX {}: real audio, bypass, UI/Lua, files and recovery passed",
        transport.id()
    );
    Ok(())
}
