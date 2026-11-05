use super::*;

fn input_command(f: &mut Fixture, client: &mut Client, value: Command) {
    let outcome = command(&mut f.service, client, ConnectionId(1), value);
    assert!(matches!(outcome, Outcome::Confirmed { .. }), "{outcome:?}");
}
fn transport(loops: Vec<LoopId>, mode: LoopMode) -> Command {
    Command::Control(ControlOperation::Transition {
        loops,
        mode,
        cycles_delay: None,
        align_to_sync_at: None,
    })
}
fn loop_state(client: &Client, id: LoopId) -> &LoopState {
    client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|state| state.id == id)
        .unwrap()
}

#[shoop_test]
fn public_grab_selection_exact_lists_solo_replacing_and_reload_share_owned_service_outcomes() {
    let mut f = fixture();
    let mut client = attach(&mut f.service, 1, ConnectionId(1), true);
    for capability in [BackendCapability::Grab, BackendCapability::ReplacingMode] {
        assert!(client.snapshot().unwrap().capabilities.supports(capability));
    }
    input_command(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSync(false)),
    );
    let clock = client.snapshot().unwrap().tracks[0].loops[0].id;
    input_command(
        &mut f,
        &mut client,
        transport(vec![clock], LoopMode::Recording),
    );
    for _ in 0..32 {
        f.service.tick(STEP);
    }
    input_command(
        &mut f,
        &mut client,
        transport(vec![clock], LoopMode::Playing),
    );
    let period = loop_state(&client, clock).length_frames;
    assert!(period > 0);
    let (track, loops) = created(command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        new_track("Input edits", true),
    ));
    input_command(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSync(true)),
    );
    input_command(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetPlayAfterRecord(false)),
    );
    input_command(
        &mut f,
        &mut client,
        Command::Control(ControlOperation::SetLoopSelection {
            loops: loops[..2].to_vec(),
            selected: true,
            clear_others: true,
        }),
    );
    input_command(
        &mut f,
        &mut client,
        Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::GrabClicked,
        },
    );
    for id in &loops[..2] {
        assert_eq!(loop_state(&client, *id).length_frames, period);
        assert_eq!(loop_state(&client, *id).mode, LoopMode::Stopped);
        assert!(!loop_state(&client, *id).empty);
    }
    let peer = loop_state(&client, loops[1]).content_revision;
    input_command(
        &mut f,
        &mut client,
        Command::Control(ControlOperation::Grab {
            loops: vec![loops[0]],
        }),
    );
    assert_eq!(loop_state(&client, loops[1]).content_revision, peer);
    input_command(
        &mut f,
        &mut client,
        transport(vec![loops[1]], LoopMode::Playing),
    );
    input_command(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetSolo(true)),
    );
    let before = loop_state(&client, loops[0]).content_revision;
    let refused = command(
        &mut f.service,
        &mut client,
        ConnectionId(1),
        Command::Control(ControlOperation::Grab {
            loops: vec![loops[0], LoopId::INVALID],
        }),
    );
    assert!(matches!(refused, Outcome::Refused { .. }));
    assert_eq!(loop_state(&client, loops[0]).content_revision, before);
    assert_eq!(loop_state(&client, loops[1]).mode, LoopMode::Playing);
    input_command(
        &mut f,
        &mut client,
        Command::Control(ControlOperation::Grab {
            loops: vec![loops[0]],
        }),
    );
    assert_eq!(loop_state(&client, loops[1]).mode, LoopMode::Stopped);
    input_command(
        &mut f,
        &mut client,
        Command::Control(ControlOperation::Trigger {
            loops: vec![loops[0]],
            mode: LoopMode::Replacing,
        }),
    );
    assert_eq!(loop_state(&client, loops[0]).mode, LoopMode::Replacing);
    for _ in 0..80 {
        f.service.tick(STEP);
    }
    input_command(
        &mut f,
        &mut client,
        transport(vec![loops[0]], LoopMode::Stopped),
    );
    assert_eq!(loop_state(&client, loops[0]).length_frames, period);
    input_command(
        &mut f,
        &mut client,
        Command::Global(GlobalControlAction::SetDefaultRecordingAction(
            DefaultRecordingAction::Grab,
        )),
    );
    let image = capture_image(&mut f.service, &mut client, 1, true);
    assert!(!image.payloads.is_empty());
    let epoch = client.snapshot().unwrap().persistence_epoch;
    release_image(&mut f.service, &mut client, 1, epoch, true);
    let key = stage_image(&mut f.service, &mut client, 2, image, true);
    input_command(&mut f, &mut client, Command::CommitPersistence(key));
    let track = client
        .snapshot()
        .unwrap()
        .tracks
        .iter()
        .find(|track| track.name == "Input edits")
        .unwrap();
    let loaded = track.loops[0].id;
    assert_eq!(loop_state(&client, loaded).length_frames, period);
    assert_eq!(loop_state(&client, loaded).mode, LoopMode::Stopped);
    input_command(
        &mut f,
        &mut client,
        transport(vec![loaded], LoopMode::Replacing),
    );
    input_command(
        &mut f,
        &mut client,
        transport(vec![loaded], LoopMode::Stopped),
    );
    assert_eq!(f.waits.get(), f.initial_waits);
}

#[shoop_test]
fn bundled_keyboard_and_lua_use_negotiated_grab_and_replacing_through_the_normal_runtime() {
    use shoop_app_client::runtime::{Startup, StartupScript};
    for with_midi in [true, false] {
        let mut f = fixture();
        let (mut runtime, port) = runtime(
            &mut f.service,
            Startup::Fresh(vec![StartupScript {
                name: "keyboard.lua".into(),
                identity: Some("keyboard.lua".into()),
                source: include_str!("../../../../../resources/builtins/keyboard.lua").into(),
                source_path: None,
                kind: ScriptKind::Bundled,
                enabled: true,
            }]),
        );
        let sync = runtime.snapshot().tracks[0].clone();
        for intent in [
            AppIntent::SetLoopTimeline {
                loop_id: sync.loops[0].id,
                start_offset: None,
                preplay_samples: None,
                loop_length: Some(512),
            },
            AppIntent::Loop {
                track_id: sync.id,
                loop_id: sync.loops[0].id,
                action: LoopAction::PlayClicked,
            },
            AppIntent::AddTrack(DirectTrackSpec {
                name: "Keyboard and Lua".into(),
                audio_channels: 0,
                midi: with_midi,
            }),
        ] {
            runtime.dispatch(intent).unwrap();
            pump_runtime(&mut f.service, &mut runtime, &port, 20);
        }
        let track = runtime.snapshot().tracks[1].clone();
        for intent in [
            AppIntent::Global(GlobalControlAction::SetSync(true)),
            AppIntent::Global(GlobalControlAction::SetApplyNCycles(1)),
            AppIntent::Global(GlobalControlAction::SetPlayAfterRecord(true)),
            AppIntent::Global(GlobalControlAction::SetDefaultRecordingAction(
                DefaultRecordingAction::Grab,
            )),
            AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action: LoopAction::IconClicked(Default::default()),
            },
        ] {
            runtime.dispatch(intent).unwrap();
            pump_runtime(&mut f.service, &mut runtime, &port, 4);
        }
        runtime
            .dispatch(AppIntent::KeyEvent(KeyEvent {
                event_type: KeyEventType::Pressed,
                key: 32,
                modifiers: 0,
            }))
            .unwrap();
        pump_runtime(&mut f.service, &mut runtime, &port, 40);
        let grabbed = &runtime.snapshot().tracks[1].loops[0];
        assert_eq!(
            grabbed.mode,
            LoopMode::Playing,
            "midi={with_midi}, scripts={:?}, notifications={:?}",
            runtime.snapshot().scripting,
            runtime.snapshot().notifications
        );
        assert_eq!(grabbed.length_frames, 512);
        assert!(runtime.snapshot().tracks[1].loops[1].empty);
        for (name, source, expected) in [
        ("grab.lua", "shoop_announce_api_version(1,0); local c=require('shoop_control'); print('grab-once'); c.loop_trigger_grab({0,1})", LoopMode::Playing),
        ("replace.lua", "shoop_announce_api_version(1,0); local c=require('shoop_control'); print('replace-once'); c.loop_trigger({0,1}, c.constants.LoopMode_Replacing)", LoopMode::Replacing),
    ] {
        runtime.dispatch(AppIntent::AddScriptSource { name: name.into(), source: Arc::from(source), kind: ScriptKind::User, enabled: true }).unwrap();
        pump_runtime(&mut f.service, &mut runtime, &port, 80);
        let state = runtime.snapshot();
        for script in state.scripting.scripts.iter() { assert!(script.latest_error.is_none(), "{:?}", script.latest_error); }
        assert_eq!(state.tracks[1].loops[1].mode, expected);
        assert_eq!(state.tracks[1].loops[1].length_frames, 512);
    }
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[1].id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        pump_runtime(&mut f.service, &mut runtime, &port, 40);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[1].mode,
            LoopMode::Stopped
        );
        let old = runtime.snapshot().tracks[1].loops[0].content_revision;
        runtime
            .dispatch(AppIntent::KeyEvent(KeyEvent {
                event_type: KeyEventType::Pressed,
                key: 71,
                modifiers: 0,
            }))
            .unwrap();
        pump_runtime(&mut f.service, &mut runtime, &port, 40);
        assert!(runtime.snapshot().tracks[1].loops[0].content_revision > old);
        assert_eq!(f.waits.get(), f.initial_waits);
    }
}
