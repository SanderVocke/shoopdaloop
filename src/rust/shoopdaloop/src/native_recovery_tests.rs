use super::*;
use shoop_egui::{
    DirectTrackSpec, GlobalControlAction, LoopAction, LoopMode, SelectionModifiers, TrackAction,
};
use shoop_host::{
    config::HostConfig,
    registry::{HostEntry, Registry},
    serve::StartOptions,
};
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
        host_file: Some(path),
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
        pools: shoop_egui::NewEnginePools {
            tracks: 4,
            loops: 32,
            channels: 2,
            loop_memory_mb: 16,
        },
        test_signal: true,
        ..Default::default()
    };
    let mut options = StartOptions::new(root.join("registry")).unwrap();
    if root.join("lan").exists() {
        options.lan = true;
        options.credential = Some(root.join("host.key"));
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
        root.join("ready"),
        serde_json::to_vec(&(
            state.tracks[1].id.raw(),
            state.tracks[1].loops[0].id.raw(),
            state.status.processed_frames,
        ))
        .unwrap(),
    )
    .unwrap();
    loop {
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[shoop_wasm_test_support::shoop_test]
fn real_ui_process_loss_repeated_attachment_media_and_both_exit_actions() {
    let root = tempfile::tempdir().unwrap();
    let mut host = peer("host_peer", root.path());
    let deadline = Instant::now() + Duration::from_secs(15);
    let registry_path = root.path().join("registry");
    let (path, entry) = loop {
        if registry_path.exists() {
            if let Some(entry) = Registry::open(&registry_path)
                .unwrap()
                .list()
                .unwrap()
                .into_iter()
                .next()
            {
                break entry;
            }
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
                    host: entry.host,
                    session: entry.session,
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
                app.exit.close_ui(ui.ctx());
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
        app.exit.open = true;
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
            assert!(Instant::now() < deadline, "close-both was not confirmed");
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(app);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(!path.exists() && !entry.credential.exists());
}

#[shoop_wasm_test_support::shoop_test]
fn remote_profile_ui_preserves_recording_across_close_and_reattach_and_confirms_shutdown() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("lan"), b"1").unwrap();
    shoop_host::remote::create_credential(&root.path().join("host.key")).unwrap();
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
    let context = egui::Context::default();
    shoop_egui::initialize(&context);
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
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(app);
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.0.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(path.exists() && root.path().join("host.key").exists());
}
