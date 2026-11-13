use super::*;
use shoop_app_api::*;
use shoop_wasm_test_support::shoop_test;

fn wait(runtime: &Runtime, done: impl Fn(&AppSnapshot) -> bool) -> std::sync::Arc<AppSnapshot> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let snapshot = runtime.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "RTL application timed out: {snapshot:?}"
        );
        std::thread::sleep(Duration::from_millis(4));
    }
}

#[shoop_test]
fn normal_application_runs_the_sync_workflow_with_the_selected_rtl_backend() {
    let mut builder = SettingsRegistryBuilder::default();
    register_settings(&mut builder).unwrap();
    register_audio_settings(&mut builder).unwrap();
    shoop_egui::register_audio_backend_settings(&mut builder).unwrap();
    register_script_settings(&mut builder).unwrap();
    let registry = builder.finish();
    let mut draft = shoop_settings::SettingsDraft::from_snapshot(&registry.defaults(1));
    draft.set(shoop_egui::AUDIO_BACKEND, "fpga_rtl_sim".into());
    let document = registry
        .document_from_draft(
            &shoop_settings::SettingsDocument::empty("test"),
            &draft,
            "test",
        )
        .unwrap();
    let settings = registry.resolve(&document, 2).snapshot;
    let args = AppArgs {
        audio: shoop_host::audio_args::AudioArgs {
            simulated_input: Some("synthetic".into()),
            ..Default::default()
        },
        ..Default::default()
    };
    assert_eq!(
        args.selected_audio_backend(&settings).unwrap(),
        AudioBackendKind::FpgaRtlSim
    );
    let mut runtime = Runtime::new(&settings, &args).unwrap();
    let snapshot = wait(&runtime, |s| {
        s.tracks.len() == 1 && s.connections.backend_available
    });
    let track_id = snapshot.tracks[0].id;
    let loop_id = snapshot.tracks[0].loops[0].id;
    let input = snapshot.connections.application_ports.iter().find(|p| p.data_type == PortDataType::Audio && p.direction == PortDirection::Input && matches!(p.owner, ApplicationPortOwner::Track { track_id: id, .. } if id == track_id)).unwrap().id;
    let host = snapshot
        .connections
        .host_ports
        .iter()
        .find(|p| {
            p.available
                && p.data_type == PortDataType::Audio
                && p.direction == PortDirection::Output
        })
        .unwrap()
        .id
        .clone();
    let output = snapshot.connections.application_ports.iter().find(|p| p.data_type == PortDataType::Audio && p.direction == PortDirection::Output && matches!(p.owner, ApplicationPortOwner::Track { track_id: id, .. } if id == track_id)).unwrap().id;
    runtime
        .dispatch(AppIntent::SetPortConnected {
            port_id: input,
            host_port_id: host,
            connected: true,
        })
        .unwrap();
    runtime
        .dispatch(AppIntent::SetMixerRouteConnected {
            source_port_id: output,
            destination_channel_id: snapshot.buses[0].channels[0].id,
            connected: true,
        })
        .unwrap();
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
        .unwrap();
    runtime
        .dispatch(AppIntent::Loop {
            track_id,
            loop_id,
            action: LoopAction::RecordClicked,
        })
        .unwrap();
    let snapshot = wait(&runtime, |s| {
        s.tracks[0].loops[0].mode == LoopMode::Recording
    });
    let first = snapshot.status.processed_frames;
    wait(&runtime, |s| s.status.processed_frames > first + 1024);
    runtime
        .dispatch(AppIntent::Loop {
            track_id,
            loop_id,
            action: LoopAction::PlayClicked,
        })
        .unwrap();
    let snapshot = wait(&runtime, |s| {
        s.tracks[0].loops[0].mode == LoopMode::Playing
            && s.status.output_peak > 0.001
            && s.status.memory.is_some()
    });
    assert_eq!(snapshot.status.audio_backend, AudioBackendKind::FpgaRtlSim);
    assert_eq!(
        snapshot
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .configured
            .kind(),
        AudioDriverKind::Simulated
    );
    assert!(snapshot.tracks[0].loops[0].length_frames > 0);
    runtime
        .dispatch(AppIntent::Loop {
            track_id,
            loop_id,
            action: LoopAction::StopClicked,
        })
        .unwrap();
    wait(&runtime, |s| s.tracks[0].loops[0].mode == LoopMode::Stopped);
    runtime
        .dispatch(AppIntent::Global(GlobalControlAction::ClearRecordings {
            include_sync: true,
        }))
        .unwrap();
    wait(&runtime, |s| s.tracks[0].loops[0].empty);
}
