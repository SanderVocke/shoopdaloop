//! Grab and Replacing through the packaged companion, application client and file codec.
use super::*;
use crate::application::{
    ApplicationHandle, ApplicationRuntime, ApplicationStartOptions, StartupScript,
};
use shoop_egui::*;

#[track_caller]
fn ready(
    handle: &ApplicationHandle,
    test: impl Fn(&AppSnapshot) -> bool,
) -> Result<std::sync::Arc<AppSnapshot>> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let state = handle.snapshot();
        if test(&state) {
            return Ok(state);
        }
        ensure!(
            Instant::now() < deadline,
            "input state timed out at {}: loops={:?}; scripts={:?}; notifications={:?}",
            std::panic::Location::caller(),
            state
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .map(|l| (
                    l.name.clone(),
                    l.mode,
                    l.length_frames,
                    l.selected,
                    l.next_transition_delay
                ))
                .collect::<Vec<_>>(),
            state.scripting.scripts,
            state.notifications
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn action(
    handle: &ApplicationHandle,
    track: TrackId,
    loop_id: LoopId,
    action: LoopAction,
) -> Result<()> {
    handle.dispatch(AppIntent::Loop {
        track_id: track,
        loop_id,
        action,
    })?;
    Ok(())
}
fn playback(handle: &ApplicationHandle, track: TrackId, loop_id: LoopId) -> Result<()> {
    action(handle, track, loop_id, LoopAction::PlayClicked)?;
    ready(handle, |s| {
        s.tracks
            .iter()
            .flat_map(|t| &t.loops)
            .any(|l| l.id == loop_id && l.mode == LoopMode::Playing)
    })?;
    action(handle, track, loop_id, LoopAction::StopClicked)?;
    ready(handle, |s| {
        s.tracks
            .iter()
            .flat_map(|t| &t.loops)
            .any(|l| l.id == loop_id && l.mode == LoopMode::Stopped)
    })?;
    Ok(())
}
fn script(handle: &ApplicationHandle, name: &str, source: &str) -> Result<()> {
    handle.dispatch(AppIntent::AddScriptSource {
        name: name.into(),
        source: source.into(),
        kind: ScriptKind::User,
        enabled: true,
    })?;
    Ok(())
}
pub fn run(transport: EngineTransportKind) -> Result<()> {
    let _span =
        tracing::info_span!("frontend.input.companion_probe", transport = transport.id()).entered();
    let (root, mut host, entry) = if transport == EngineTransportKind::Socket {
        let driver = if std::env::var_os("SHOOP_INPUT_PROBE_JACK").is_some() {
            AudioDriverConfig::Jack(Default::default())
        } else {
            AudioDriverConfig::Dummy(DummyAudioDriverConfig::default())
        };
        launch_with_settings(
            transport,
            shoop_host::config::HostSettings {
                driver,
                ..Default::default()
            },
        )?
    } else {
        launch(transport, 16)?
    };
    let builtins_root = shoop_egui::default_builtins_location();
    let catalog = shoop_script_resources::scan_builtin_directory(
        std::path::Path::new(&builtins_root),
        1,
        shoop_script_resources::ResourceLimits::default(),
    )?;
    ensure!(
        catalog.diagnostics.is_empty(),
        "built-in script diagnostics: {:?}",
        catalog.diagnostics
    );
    let keyboard = catalog
        .entries
        .into_iter()
        .find(|entry| entry.identity.as_str() == "keyboard.lua")
        .ok_or_else(|| anyhow!("bundled keyboard.lua missing from {builtins_root}"))?;
    let options = ApplicationStartOptions {
        master_auto_connect: false,
        loop_smoothing_ms: 0,
        startup_scripts: vec![StartupScript {
            name: "keyboard.lua".into(),
            identity: Some("keyboard.lua".into()),
            source: keyboard.source.to_string(),
            source_path: Some(
                std::path::Path::new(&builtins_root)
                    .join("keyboard.lua")
                    .to_string_lossy()
                    .into_owned(),
            ),
            kind: ScriptKind::Bundled,
            enabled: true,
        }],
        ..Default::default()
    };
    let mut app = ApplicationRuntime::attach(entry.clone(), options.clone())?;
    let mut handle = app.handle();
    let initial = ready(&handle, |s| !s.tracks.is_empty())?;
    ensure!(
        initial.capabilities.supports(BackendCapability::Grab)
            && initial
                .capabilities
                .supports(BackendCapability::ReplacingMode),
        "input capabilities missing"
    );
    let keyboard = ready(&handle, |s| {
        s.scripting
            .scripts
            .iter()
            .any(|script| script.name == "keyboard.lua")
    })?;
    let keyboard = keyboard
        .scripting
        .scripts
        .iter()
        .find(|script| script.name == "keyboard.lua")
        .unwrap()
        .id;
    handle.dispatch(AppIntent::RestartScript {
        script_id: keyboard,
    })?;
    println!("Input controls: normal startup and explicit keyboard activation");
    let sync = initial.tracks[0].clone();
    let period = 32768;
    handle.dispatch(AppIntent::SetLoopTimeline {
        loop_id: sync.loops[0].id,
        start_offset: None,
        preplay_samples: None,
        loop_length: Some(period),
    })?;
    action(&handle, sync.id, sync.loops[0].id, LoopAction::PlayClicked)?;
    for (name, channels) in [("Stereo input", 2), ("MIDI input", 0)] {
        handle.dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: name.into(),
            audio_channels: channels,
            midi: true,
        }))?;
        ready(&handle, |s| {
            s.tracks
                .iter()
                .any(|t| t.name == name && t.loops.len() == 8)
        })?;
    }
    // Allow asynchronous native port publication to settle before the first input cut.
    std::thread::sleep(Duration::from_millis(1500));
    // Instrumented hosts can render more slowly than wall time. Grab needs a
    // complete sync period of actual input history, including a partial first cycle.
    let history_start = handle.snapshot().status.processed_frames;
    ready(&handle, |s| {
        s.status.processed_frames.saturating_sub(history_start) >= 2 * period
    })?;
    let state = handle.snapshot();
    let audio = state.tracks[1].clone();
    let keys = state.tracks[2].clone();
    for setting in [
        GlobalControlAction::SetSync(true),
        GlobalControlAction::SetApplyNCycles(1),
        GlobalControlAction::SetPlayAfterRecord(false),
        GlobalControlAction::SetDefaultRecordingAction(DefaultRecordingAction::Grab),
    ] {
        handle.dispatch(AppIntent::Global(setting))?;
    }
    action(
        &handle,
        audio.id,
        audio.loops[0].id,
        LoopAction::IconClicked(Default::default()),
    )?;
    action(
        &handle,
        audio.id,
        audio.loops[1].id,
        LoopAction::IconClicked(SelectionModifiers { additive: true }),
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[..2].iter().all(|l| l.selected)
    })?;
    action(
        &handle,
        audio.id,
        audio.loops[0].id,
        LoopAction::GrabClicked,
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[..2]
            .iter()
            .all(|l| !l.empty && l.length_frames == period && l.mode == LoopMode::Stopped)
    })?;
    playback(&handle, audio.id, audio.loops[0].id)?;
    script(&handle, "explicit-grab.lua", "shoop_announce_api_version(1,0); local c=require('shoop_control'); c.loop_trigger_grab({1,0})")?;
    ready(&handle, |s| {
        !s.tracks[2].loops[0].empty && s.tracks[2].loops[0].length_frames == period
    })?;
    playback(&handle, keys.id, keys.loops[0].id)?;
    println!("Input controls: selected and explicit Grab confirmed");
    // The bundled Space binding uses the default recording action on an empty selected loop.
    action(
        &handle,
        audio.id,
        audio.loops[2].id,
        LoopAction::IconClicked(Default::default()),
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[2].selected && !s.tracks[1].loops[0].selected
    })?;
    handle.dispatch(AppIntent::KeyEvent(KeyEvent {
        event_type: KeyEventType::Pressed,
        key: 32,
        modifiers: 0,
    }))?;
    handle.dispatch(AppIntent::KeyEvent(KeyEvent {
        event_type: KeyEventType::Released,
        key: 32,
        modifiers: 0,
    }))?;
    ready(&handle, |s| {
        !s.tracks[1].loops[2].empty && s.tracks[1].loops[2].length_frames == period
    })?;
    playback(&handle, audio.id, audio.loops[2].id)?;
    println!("Input controls: bundled Space default Grab confirmed");
    // The finish survives loss of the UI and its Lua VM.
    for setting in [
        GlobalControlAction::SetSync(false),
        GlobalControlAction::SetApplyNCycles(2),
        GlobalControlAction::SetPlayAfterRecord(true),
    ] {
        handle.dispatch(AppIntent::Global(setting))?;
    }
    action(
        &handle,
        audio.id,
        audio.loops[3].id,
        LoopAction::GrabClicked,
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[3].mode == LoopMode::Recording
            && s.tracks[1].loops[3].next_transition_delay.is_some()
    })?;
    if transport == EngineTransportKind::Socket {
        drop(handle);
        drop(app);
        composites::wait_for_controller_release(&entry)?;
        app = ApplicationRuntime::attach(entry.clone(), options)?;
        handle = app.handle();
    }
    ready(&handle, |s| {
        s.tracks
            .get(1)
            .and_then(|t| t.loops.get(3))
            .is_some_and(|l| l.mode == LoopMode::Playing && l.length_frames == 2 * period)
    })?;
    println!(
        "Input controls: owned automatic finish confirmed on {}",
        transport.id()
    );
    handle.dispatch(AppIntent::Global(GlobalControlAction::DeselectAll))?;
    ready(&handle, |s| {
        s.tracks.iter().flat_map(|t| &t.loops).all(|l| !l.selected)
    })?;
    action(
        &handle,
        audio.id,
        audio.loops[0].id,
        LoopAction::ReplaceClicked,
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[0].mode == LoopMode::Replacing
    })?;
    action(
        &handle,
        audio.id,
        audio.loops[0].id,
        LoopAction::StopClicked,
    )?;
    ready(&handle, |s| s.tracks[1].loops[0].mode == LoopMode::Stopped)?;
    script(&handle, "continuous-midi.lua", "shoop_announce_api_version(1,0); local c=require('shoop_control'); c.loop_trigger({{0,0},{1,0}}, c.constants.LoopMode_Replacing)")?;
    ready(&handle, |s| {
        s.tracks[1].loops[0].mode == LoopMode::Replacing
            && s.tracks[2].loops[0].mode == LoopMode::Replacing
    })?;
    std::thread::sleep(Duration::from_millis(2200));
    action(
        &handle,
        keys.id,
        keys.loops[0].id,
        LoopAction::IconClicked(Default::default()),
    )?;
    ready(&handle, |s| s.tracks[2].loops[0].selected)?;
    handle.dispatch(AppIntent::Track {
        track_id: keys.id,
        action: TrackAction::InputMonitoringChanged {
            enabled: true,
            respect_auto_mute: false,
        },
    })?;
    ready(&handle, |s| s.tracks[2].controls.input_monitoring)?;
    handle.dispatch(AppIntent::Piano(PianoAction::Press(
        MidiNote::new(60).unwrap(),
    )))?;
    std::thread::sleep(Duration::from_millis(60));
    handle.dispatch(AppIntent::Piano(PianoAction::Release(
        MidiNote::new(60).unwrap(),
    )))?;
    std::thread::sleep(Duration::from_millis(30));
    handle.dispatch(AppIntent::Global(GlobalControlAction::StopAll))?;
    ready(&handle, |s| {
        s.tracks
            .iter()
            .flat_map(|t| &t.loops)
            .all(|l| l.mode == LoopMode::Stopped)
    })?;
    let before = file_save(&handle, &root.path().join("input.shoop"))?;
    ensure!(before.media.values().any(|m| matches!(m, shoop_session::MediaPayload::Midi(m) if m.events.iter().any(|e| e.data == [0x90,60,100]) || m.events.iter().any(|e| e.data.first().is_some_and(|s| s & 0xf0 == 0x90)))), "replacement did not retain piano MIDI");
    ensure!(
        before
            .media
            .values()
            .filter(|m| matches!(m, shoop_session::MediaPayload::Audio(_)))
            .count()
            >= 2,
        "stereo capture missing"
    );
    let old = audio.loops[0].id;
    handle.dispatch(AppIntent::LoadSessionBytes {
        name: "input.shoop".into(),
        bytes: std::fs::read(root.path().join("input.shoop"))?.into(),
    })?;
    let loaded = file_done(&handle, IoTaskKind::LoadSession)?;
    ensure!(
        loaded.global_controls.default_recording_action == DefaultRecordingAction::Grab
            && loaded.tracks[1].loops[0].id != old
            && loaded
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .all(|l| l.mode == LoopMode::Stopped),
        "reload did not restore Grab and fresh stopped owners"
    );
    let after = file_save(&handle, &root.path().join("input-resaved.shoop"))?;
    ensure!(
        before.media.values().collect::<Vec<_>>() == after.media.values().collect::<Vec<_>>(),
        "input save/load changed PCM or MIDI"
    );
    handle.dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
    action(
        &handle,
        loaded.tracks[1].id,
        loaded.tracks[1].loops[0].id,
        LoopAction::ReplaceClicked,
    )?;
    ready(&handle, |s| {
        s.tracks[1].loops[0].mode == LoopMode::Replacing
    })?;
    action(
        &handle,
        loaded.tracks[1].id,
        loaded.tracks[1].loops[0].id,
        LoopAction::StopClicked,
    )?;
    let finished = ready(&handle, |s| s.tracks[1].loops[0].mode == LoopMode::Stopped)?;
    ensure!(
        finished.tracks[1].loops[0].length_frames == period,
        "reloaded replacement changed period"
    );
    ensure!(
        finished
            .scripting
            .scripts
            .iter()
            .all(|s| s.latest_error.is_none()),
        "input Lua failed"
    );
    let ticket = handle.prepare_shutdown()?;
    drop(app);
    crate::recovery::shutdown(&entry, ticket)
        .0
        .map_err(anyhow::Error::msg)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = host.0.try_wait()? {
            ensure!(status.success(), "input companion failed on exit");
            break;
        }
        ensure!(Instant::now() < deadline, "input companion did not exit");
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("Input {}: selected Grab, bundled keyboard, exact-list Lua, owned prefix completion, stereo/MIDI replacement across wraps, exact file reload/re-edit and shutdown passed", transport.id());
    Ok(())
}
