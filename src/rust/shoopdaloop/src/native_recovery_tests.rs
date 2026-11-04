use super::*;
use shoop_egui::{
    DirectTrackSpec, GlobalControlAction, LoopAction, LoopMode, SelectionModifiers, TrackAction,
};
use shoop_host::{config::HostConfig, remote::Target as HostEntry, serve::StartOptions};
use std::process::{Child, Command, Stdio};

struct Peer(Child);
impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn peer(name: &str, root: &Path) -> Peer {
    Peer(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                &format!("native_recovery_tests::{name}"),
                "--nocapture",
            ])
            .env("SHOOP_UI_RECOVERY_FIXTURE", root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    )
}
fn wait(runtime: &Runtime, done: impl Fn(&AppSnapshot) -> bool) -> Arc<AppSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let state = runtime.snapshot();
        if done(&state) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "UI recovery timed out: {:?}",
            state.notifications
        );
        std::thread::sleep(Duration::from_millis(4));
    }
}
fn ui(path: std::path::PathBuf) -> UnifiedApp {
    let args = AppArgs {
        ui_only: true,
        remote_profile: Some(path),
        ..Default::default()
    };
    let app = UnifiedApp::new(1.0, false, &args).unwrap();
    wait(&app.runtime, |state| !state.tracks.is_empty());
    app
}
fn loop_action(runtime: &mut Runtime, state: &AppSnapshot, action: LoopAction) {
    runtime
        .dispatch(AppIntent::Loop {
            track_id: state.tracks[1].id,
            loop_id: state.tracks[1].loops[0].id,
            action,
        })
        .unwrap();
}

#[shoop_wasm_test_support::shoop_test]
fn host_peer() {
    let Some(root) = std::env::var_os("SHOOP_UI_RECOVERY_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: shoop_egui::NewEnginePools {
                quantum: 1,
                buses: 8,
                bus_channels: 8,
                tracks: 4,
                loops: 32,
                channels: 2,
                loop_memory_mb: 16,
            },
            ..Default::default()
        },
        test_signal: true,
        ..Default::default()
    };
    let mut options = StartOptions::new().unwrap();

    options.export_profile = Some(root.join("profile.json"));
    if root.join("lan").exists() {
        options.lan = true;

        options.export_profile = Some(root.join("profile.json"));
    }
    shoop_host::serve::run(config, options).unwrap();
}

#[shoop_wasm_test_support::shoop_test]
fn ui_peer() {
    let Some(root) = std::env::var_os("SHOOP_UI_RECOVERY_FIXTURE") else {
        return;
    };
    let root = std::path::PathBuf::from(root);
    let path = std::path::PathBuf::from(std::fs::read_to_string(root.join("entry-path")).unwrap());
    let mut app = ui(path);
    app.runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    app.runtime
        .dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "UI crash take".into(),
            audio_channels: 1,
            midi: true,
        }))
        .unwrap();
    let state = wait(&app.runtime, |state| {
        state.tracks.len() == 2 && state.tracks[1].loops.len() == 8
    });
    loop_action(
        &mut app.runtime,
        &state,
        LoopAction::NameChanged("Surviving media".into()),
    );
    loop_action(&mut app.runtime, &state, LoopAction::RecordClicked);
    let state = wait(&app.runtime, |state| {
        state.tracks[1].loops[0].mode == LoopMode::Recording
    });
    std::fs::write(
        root.join("ready.tmp"),
        serde_json::to_vec(&(
            state.tracks[1].id.raw(),
            state.tracks[1].loops[0].id.raw(),
            state.status.processed_frames,
        ))
        .unwrap(),
    )
    .unwrap();
    std::fs::rename(root.join("ready.tmp"), root.join("ready")).unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[shoop_wasm_test_support::shoop_test]
fn real_ui_process_loss_repeated_attachment_media_and_both_exit_actions() {
    let root = tempfile::tempdir().unwrap();
    let mut host = peer("host_peer", root.path());
    let deadline = Instant::now() + Duration::from_secs(15);
    let path = root.path().join("profile.json");
    let entry = loop {
        if path.exists() {
            break HostEntry::read(&path).unwrap();
        }
        assert!(host.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    };
    std::fs::write(root.path().join("entry-path"), path.to_str().unwrap()).unwrap();
    let mut first_ui = peer("ui_peer", root.path());
    while !root.path().join("ready").exists() {
        assert!(
            first_ui.0.try_wait().unwrap().is_none(),
            "UI fixture failed"
        );
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let (track_id, loop_id, frame): (u64, u64, u64) =
        serde_json::from_slice(&std::fs::read(root.path().join("ready")).unwrap()).unwrap();
    first_ui.0.kill().unwrap();
    first_ui.0.wait().unwrap();
    let context = egui::Context::default();
    shoop_egui::initialize(&context);
    let mut frame = frame;
    for index in 0..4 {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let reply = shoop_host::connection::management(
                &entry,
                shoop_app_protocol::ManagementRequest::Status {
                    version: shoop_app_protocol::VERSION,
                },
                Duration::from_secs(2),
            )
            .unwrap();
            if matches!(
                reply,
                shoop_app_protocol::ManagementReply::Status { busy: false, .. }
            ) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(4));
        }
        let mut app = ui(path.clone());
        let state = wait(&app.runtime, |state| {
            state.tracks.len() == 2 && state.status.processed_frames > frame
        });
        assert_eq!(state.tracks[1].id.raw(), track_id);
        assert_eq!(state.tracks[1].loops[0].id.raw(), loop_id);
        assert_eq!(state.tracks[1].loops[0].name, "Surviving media");
        assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
        frame = state.status.processed_frames;
        assert!(state.scripting.scripts.iter().all(|script| !script.enabled));
        if index != 3 {
            let mut output = context.run_ui(Default::default(), |ui| {
                app.show(ui);
                app.handle_host_actions(ui.ctx(), vec![shoop_egui::HostAction::CloseUi]);
            });
            assert!(output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close));
            output.textures_delta.clear();
            drop(app);
            assert!(host.0.try_wait().unwrap().is_none());
            continue;
        }
        app.runtime
            .dispatch(AppIntent::Track {
                track_id: state.tracks[1].id,
                action: TrackAction::NameChanged("Recovered UI controls".into()),
            })
            .unwrap();
        loop_action(&mut app.runtime, &state, LoopAction::PlayClicked);
        wait(&app.runtime, |state| {
            state.tracks[1].name == "Recovered UI controls"
                && state.tracks[1].loops[0].mode == LoopMode::Playing
        });
        loop_action(
            &mut app.runtime,
            &state,
            LoopAction::IconClicked(SelectionModifiers::default()),
        );
        let state = wait(&app.runtime, |state| {
            state.details.as_ref().is_some_and(|details| {
                !details.channels.is_empty() && !details.channels[0].samples.is_empty()
            })
        });
        assert!(state.details.as_ref().unwrap().channels[0]
            .samples
            .iter()
            .any(|sample| *sample == 0.25));
        app.handle_host_actions(&context, vec![shoop_egui::HostAction::CloseUiAndHost]);
        assert!(app.exit.open);
        assert!(!app.exit.is_pending());
        assert!(host.0.try_wait().unwrap().is_none());
        app.exit
            .start(HostEntry::read(&path).unwrap(), app.runtime.handle.clone());
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let mut output = context.run_ui(Default::default(), |ui| app.show(ui));
            let closed = output.viewport_output[&egui::ViewportId::ROOT]
                .commands
                .contains(&egui::ViewportCommand::Close);
            output.textures_delta.clear();
            if closed {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "close-both was not confirmed: {:?}",
                app.exit
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(app);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(path.exists());
}

#[shoop_wasm_test_support::shoop_test]
fn remote_profile_ui_preserves_recording_across_close_and_reattach_and_confirms_shutdown() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lan"), b"1").unwrap();

    let mut host = peer("host_peer", root.path());
    let path = root.path().join("profile.json");
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(host.0.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let open = || {
        let args = AppArgs {
            ui_only: true,
            remote_profile: Some(path.clone()),
            ..Default::default()
        };
        let app = UnifiedApp::new(1., false, &args).unwrap();
        wait(&app.runtime, |state| !state.tracks.is_empty());
        app
    };
    let mut app = open();
    assert!(app.runtime.remote_entry.as_ref().unwrap().network.is_some());
    let context = egui::Context::default();
    shoop_egui::initialize(&context);
    app.handle_host_actions(&context, vec![shoop_egui::HostAction::OpenSettings]);
    assert!(app.host_panel.open);
    let deadline = Instant::now() + Duration::from_secs(15);
    while app.host_panel.report.is_none() {
        let mut output = context.run_ui(Default::default(), |ui| app.show(ui));
        output.textures_delta.clear();
        assert!(
            Instant::now() < deadline,
            "connected host panel did not receive settings"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        app.host_panel
            .report
            .as_ref()
            .unwrap()
            .effective
            .sample_rate,
        48000
    );
    let mut draft = shoop_settings::SettingsDraft::from_snapshot(&app.settings.active());
    draft.set(shoop_egui::DUMMY_SAMPLE_RATE, 32000);
    draft.set(shoop_egui::LOOP_EDGE_SMOOTHING_MS, 19);
    draft.set(shoop_egui::MASTER_AUTO_CONNECT, false);
    let revision = app.settings.active().revision();
    app.handle_settings_action(SettingsAction::RequestAudioDriverSwitch {
        config: shoop_egui::AudioDriverConfig::Dummy(shoop_egui::DummyAudioDriverConfig {
            sample_rate: 32000,
            buffer_size: 256,
        }),
        draft,
    });
    while app.settings.active().revision() == revision {
        let mut output = context.run_ui(Default::default(), |ui| app.show(ui));
        output.textures_delta.clear();
        assert!(
            Instant::now() < deadline,
            "local preference save did not finish"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        app.settings
            .active()
            .get(shoop_egui::DUMMY_SAMPLE_RATE)
            .unwrap(),
        32000
    );
    assert!(app.pending_audio_settings.is_none());
    let report = shoop_host::connection::host_report(
        app.runtime.remote_entry.as_ref().unwrap(),
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(report.effective.sample_rate, 48000);
    assert_eq!(report.loop_smoothing_ms, 3);
    assert!(report.master_auto_connect);
    app.host_panel.open = false;
    app.runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    app.runtime
        .dispatch(AppIntent::AddTrack(DirectTrackSpec {
            name: "LAN recording".into(),
            audio_channels: 1,
            midi: true,
        }))
        .unwrap();
    let state = wait(&app.runtime, |state| {
        state.tracks.len() == 2 && state.tracks[1].loops.len() == 8
    });
    loop_action(&mut app.runtime, &state, LoopAction::RecordClicked);
    let state = wait(&app.runtime, |state| {
        state.tracks[1].loops[0].mode == LoopMode::Recording
    });
    let before = state.status.processed_frames;
    let owner = state.tracks[1].id;
    let mut output = context.run_ui(Default::default(), |ui| {
        app.show(ui);
        app.exit.close_ui(ui.ctx());
    });
    output.textures_delta.clear();
    drop(app);
    std::thread::sleep(Duration::from_millis(100));
    assert!(host.0.try_wait().unwrap().is_none());
    let mut app = open();
    let state = wait(&app.runtime, |state| {
        state.tracks.len() == 2 && state.status.processed_frames > before
    });
    assert_eq!(state.tracks[1].id, owner);
    assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
    assert!(state.scripting.scripts.iter().all(|script| !script.enabled));
    loop_action(&mut app.runtime, &state, LoopAction::PlayClicked);
    wait(&app.runtime, |state| {
        state.tracks[1].loops[0].mode == LoopMode::Playing
    });
    app.exit.open = true;
    app.exit.start(
        app.runtime.remote_entry.clone().unwrap(),
        app.runtime.handle.clone(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut output = context.run_ui(Default::default(), |ui| app.show(ui));
        let closed = output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::Close);
        output.textures_delta.clear();
        if closed {
            break;
        }
        assert!(Instant::now() < deadline, "shutdown state: {:?}", app.exit);
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(app);
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(path.exists());
}

#[shoop_wasm_test_support::shoop_test]
fn a_new_connection_cancels_old_inputs_and_cannot_consume_old_file_callbacks() {
    let mut app = UnifiedApp::new(1., false, &AppArgs::default()).unwrap();
    let old_sender = app.pending_file_intent_tx.clone();
    old_sender
        .send(AppIntent::Global(GlobalControlAction::SetSolo(true)))
        .unwrap();
    let input = app
        .file_input
        .reserve(shoop_egui::TaskId::from_raw(1))
        .unwrap();
    let output = app.file_output.reserve().unwrap();
    let settings = app.settings.active();
    app.startup_session = Some(("previous.shoop".into(), false));
    app.session_url_task = Some(shoop_egui::TaskId::from_raw(1));
    let context = egui::Context::default();
    let field = egui::Id::unique("previous connection field");
    context.data_mut(|data| data.insert_temp(field, 42_u32));
    app.clear_connection_state(&context);
    assert!(context.data(|data| data.get_temp::<u32>(field)).is_none());
    assert!(app.pending_file_intent_rx.try_recv().is_err());
    assert!(old_sender
        .send(AppIntent::Global(GlobalControlAction::SetSolo(true)))
        .is_err());
    app.pending_file_intent_tx
        .send(AppIntent::Global(GlobalControlAction::SetSolo(false)))
        .unwrap();
    assert!(matches!(
        app.pending_file_intent_rx.try_recv(),
        Ok(AppIntent::Global(GlobalControlAction::SetSolo(false)))
    ));
    assert!(input.check().is_err());
    assert!(app
        .file_input
        .reserve(shoop_egui::TaskId::from_raw(2))
        .is_err());
    assert!(app.file_output.reserve().is_err());
    assert!(app.startup_session.is_none() && app.session_url_task.is_none());
    assert!(Arc::ptr_eq(&settings, &app.settings.active()));
    drop(input);
    drop(output);
    assert!(app
        .file_input
        .reserve(shoop_egui::TaskId::from_raw(2))
        .is_ok());
    assert!(app.file_output.reserve().is_ok());
}
