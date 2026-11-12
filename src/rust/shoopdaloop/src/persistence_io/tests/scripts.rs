use super::*;
use shoop_script_resources::ScriptResourceBundle;
use shoop_session::{ScriptDocument, SessionBundle, SessionDocument};

fn archive(source: &str, rate: u32) -> Arc<Vec<u8>> {
    let mut bundle = SessionBundle::new(SessionDocument::empty(rate));
    bundle.document.scripts.push(ScriptDocument {
        id: 77,
        name: "session.lua".into(),
        entrypoint: "main.lua".into(),
        enabled: true,
    });
    bundle.scripts.insert(
        77,
        Arc::new(ScriptResourceBundle::source_only("main.lua", source.as_bytes()).unwrap()),
    );
    Arc::new(shoop_session::encode_session(&bundle, "p16-test").unwrap())
}
fn done(snapshot: &AppSnapshot) -> bool {
    snapshot
        .io_task
        .as_ref()
        .is_some_and(|task| task.status == IoTaskStatus::Completed)
}
const SOURCE: &str = "shoop_announce_api_version(1, 0); require('shoop_control').set_solo(true)";

#[shoop_test]
async fn session_script_round_trip_is_client_owned_stopped_and_preserves_machine_scripts() {
    let mut f = Fixture::new();
    for (name, kind) in [
        ("machine.lua", ScriptKind::User),
        ("once.lua", ScriptKind::Ephemeral),
    ] {
        f.dispatch(AppIntent::AddScriptSource {
            name: name.into(),
            source: Arc::from("shoop_announce_api_version(1, 0); print('machine')"),
            kind,
            enabled: false,
        });
    }
    let machine = f.runtime.snapshot().scripting.scripts.clone();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "with-scripts.shoop".into(),
        bytes: archive(SOURCE, 1000),
    });
    let loaded = f.until(done).await;
    assert!(!loaded.global_controls.solo);
    for old in machine.iter() {
        assert!(loaded
            .scripting
            .scripts
            .iter()
            .any(|script| script.id == old.id && script.kind == old.kind));
    }
    let script = loaded
        .scripting
        .scripts
        .iter()
        .find(|script| script.kind == ScriptKind::Session)
        .unwrap();
    let id = script.id;
    assert!(script.enabled);
    assert_eq!(script.lifecycle, ScriptLifecycle::Inactive);
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    let saved = shoop_session::decode_session(&output.bytes).unwrap();
    assert_eq!(saved.document.scripts.len(), 1);
    assert_eq!(
        saved.scripts[&77].entrypoint_resource().bytes.as_ref(),
        SOURCE.as_bytes()
    );
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    f.dispatch(AppIntent::ExportScript { script_id: id });
    let exported = f.output().await;
    assert_eq!(exported.suggested_name, "session.lua");
    assert_eq!(exported.bytes.as_slice(), SOURCE.as_bytes());
    f.dispatch(AppIntent::RestartScript { script_id: id });
    f.until(|snapshot| snapshot.global_controls.solo).await;
    f.dispatch(AppIntent::RequestNewSession);
    let fresh = f.until(done).await;
    assert_eq!(fresh.scripting.scripts.len(), machine.len());
    assert!(fresh
        .scripting
        .scripts
        .iter()
        .all(|script| script.kind != ScriptKind::Session));
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    assert!(shoop_session::decode_session(&output.bytes)
        .unwrap()
        .scripts
        .is_empty());
}

#[shoop_test]
async fn invalid_or_declined_script_load_preserves_local_scripts_and_music() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::AddScriptSource {
        name: "old.lua".into(),
        source: Arc::from("shoop_announce_api_version(1, 0)"),
        kind: ScriptKind::Session,
        enabled: false,
    });
    let before = f.runtime.snapshot();
    let old_ids: Vec<_> = before.tracks.iter().map(|track| track.id).collect();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "invalid.shoop".into(),
        bytes: archive("function(", 1000),
    });
    let failed = f
        .until(|snapshot| {
            snapshot
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Failed)
        })
        .await;
    assert_eq!(failed.scripting.scripts, before.scripting.scripts);
    assert_eq!(
        failed
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        old_ids
    );
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "conversion.shoop".into(),
        bytes: archive(SOURCE, 2000),
    });
    let consent = f
        .until(|snapshot| {
            snapshot.io_task.as_ref().is_some_and(|task| {
                task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
            })
        })
        .await;
    f.dispatch(AppIntent::ConfirmFormatConversion {
        task_id: consent.io_task.as_ref().unwrap().id,
        accept: false,
    });
    let after = f.runtime.snapshot();
    assert_eq!(
        after.io_task.as_ref().unwrap().status,
        IoTaskStatus::Cancelled
    );
    assert_eq!(after.scripting.scripts, before.scripting.scripts);
    assert_eq!(
        after
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        old_ids
    );
    assert!(!after.global_controls.solo);
}

#[shoop_test]
async fn save_freezes_local_additions_before_later_removal() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::AddScriptSource {
        name: "frozen.lua".into(),
        source: Arc::from("shoop_announce_api_version(1, 0); print('frozen')"),
        kind: ScriptKind::Session,
        enabled: false,
    });
    let id = f.runtime.snapshot().scripting.scripts[0].id;
    f.dispatch(AppIntent::RequestSaveSession);
    f.dispatch(AppIntent::RemoveSessionScript { script_id: id });
    assert!(f.runtime.snapshot().scripting.scripts.is_empty());
    let output = f.output().await;
    let saved = shoop_session::decode_session(&output.bytes).unwrap();
    assert_eq!(saved.document.scripts[0].name, "frozen.lua");
    assert_eq!(saved.scripts.len(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test]
async fn filesystem_conversion_embeds_companions_and_survives_source_directory_removal() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(temp.path().join("help/images")).unwrap();
    let path = temp.path().join("portable.lua");
    let source = "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Portable Help', {d.markdown_file('help/readme.md')})";
    let markdown = "portable\n\n![icon](images/icon.png)";
    let image = &include_bytes!("../../../../../../resources/logo-small.png")[..];
    std::fs::write(&path, source).unwrap();
    std::fs::write(temp.path().join("help/readme.md"), markdown).unwrap();
    std::fs::write(temp.path().join("help/images/icon.png"), image).unwrap();
    let mut f = Fixture::new();
    f.dispatch(AppIntent::AddEphemeralScript {
        name: "portable.lua".into(),
        source: Arc::from(source),
        source_path: Some(path.to_string_lossy().into_owned()),
    });
    let id = f.runtime.snapshot().scripting.scripts[0].id;
    f.dispatch(AppIntent::ConvertScriptKind {
        script_id: id,
        kind: ScriptKind::Session,
    });
    f.until(|snapshot| snapshot.scripting.scripts[0].kind == ScriptKind::Session)
        .await;
    std::fs::remove_dir_all(temp.path()).unwrap();
    for kind in [ScriptKind::Ephemeral, ScriptKind::Session] {
        f.dispatch(AppIntent::ConvertScriptKind {
            script_id: id,
            kind,
        });
    }
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    let saved = shoop_session::decode_session(&output.bytes).unwrap();
    let resources = &saved.scripts[&saved.document.scripts[0].id];
    assert_eq!(
        resources.entrypoint_resource().bytes.as_ref(),
        source.as_bytes()
    );
    let help = shoop_script_resources::NormalizedRelativePath::parse("help/readme.md").unwrap();
    assert_eq!(
        resources.get(&help).unwrap().bytes.as_ref(),
        markdown.as_bytes()
    );
    assert_eq!(resources.resources.len(), 3);
    f.dispatch(AppIntent::CompleteIoOutput {
        task_id: output.task_id,
        outcome: FileOutputOutcome::Saved,
    });
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "portable.shoop".into(),
        bytes: output.bytes,
    });
    let loaded = f.until(done).await;
    assert!(loaded.scripting.dialogs.is_empty());
    let id = loaded.scripting.scripts[0].id;
    f.dispatch(AppIntent::RestartScript { script_id: id });
    let running = f.runtime.snapshot();
    assert!(
        running
            .scripting
            .dialogs
            .iter()
            .any(|dialog| dialog.name == "Portable Help"),
        "{:?}",
        running.scripting.scripts
    );
}

#[shoop_test]
async fn surviving_client_reconciles_accepted_load_after_reattachment_without_replay() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "pending.shoop".into(),
        bytes: archive(SOURCE, 1000),
    });
    f.until(|snapshot| {
        snapshot
            .io_task
            .as_ref()
            .is_some_and(|task| task.message == "Awaiting the host-owned commit result")
    })
    .await;
    f.service.pump_local(ConnectionId(1), &f.port, STEP);
    f.service.detach(ConnectionId(1));
    f.runtime.fail("simulated transport loss".into());
    let (delivery, port) = local::channel();
    f.runtime.reconnect(Box::new(delivery)).unwrap();
    f.port = port;
    let loaded = f.until(done).await;
    assert_eq!(loaded.scripting.scripts.len(), 1);
    assert_eq!(
        loaded.scripting.scripts[0].lifecycle,
        ScriptLifecycle::Inactive
    );
    assert!(!loaded.global_controls.solo);
    let ids: Vec<_> = loaded.tracks.iter().map(|track| track.id).collect();
    for _ in 0..8 {
        f.step();
    }
    assert_eq!(
        f.runtime
            .snapshot()
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        ids
    );
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    assert_eq!(
        shoop_session::decode_session(&output.bytes)
            .unwrap()
            .document
            .scripts[0]
            .id,
        77
    );
}

#[shoop_test]
async fn replacement_client_saves_music_with_only_its_own_scripts() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "first.shoop".into(),
        bytes: archive(SOURCE, 1000),
    });
    let loaded = f.until(done).await;
    let ids: Vec<_> = loaded.tracks.iter().map(|track| track.id).collect();
    f.service.detach(ConnectionId(1));
    let (delivery, port) = local::channel();
    f.runtime = Runtime::new(
        Client::new(),
        Box::new(delivery),
        Box::new(shoop_scripting::NullMidiService),
        Startup::Connected(Vec::new()),
    )
    .unwrap()
    .with_persistence_io(Box::new(Provider::new()));
    f.port = port;
    for _ in 0..8 {
        f.step();
    }
    assert_eq!(
        f.runtime
            .snapshot()
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>(),
        ids
    );
    assert!(f.runtime.snapshot().scripting.scripts.is_empty());
    f.dispatch(AppIntent::AddScriptSource {
        name: "second-client.lua".into(),
        source: Arc::from("shoop_announce_api_version(1, 0)"),
        kind: ScriptKind::Session,
        enabled: false,
    });
    f.dispatch(AppIntent::RequestSaveSession);
    let output = f.output().await;
    let saved = shoop_session::decode_session(&output.bytes).unwrap();
    assert_eq!(saved.document.scripts.len(), 1);
    assert_eq!(saved.document.scripts[0].name, "second-client.lua");
}

#[shoop_test]
fn evicted_commit_results_keep_local_candidates_inactive_until_explicit_discard() {
    let f = Fixture::new();
    let state = f.runtime.client().snapshot().unwrap().clone();
    let mut authority = f.runtime.client().authority().unwrap();
    let operation = OperationId {
        client: authority.client,
        sequence: 999,
    };
    authority.client = ClientId(authority.client.0 + 1);
    authority.generation = ConnectionGeneration(authority.generation.0 + 1);
    let mut client = Client::new();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached { authority })
        .unwrap();
    client
        .receive(ServiceMessage::Snapshot {
            authority,
            state: state.clone(),
        })
        .unwrap();
    client.ready_sent().unwrap();
    let mut scripts = shoop_scripting::ScriptManager::new();
    scripts
        .add(
            "old.lua",
            "shoop_announce_api_version(1, 0)",
            ScriptKind::Session,
            false,
        )
        .unwrap();
    let old = scripts.session_data();
    let mut provider = Provider::new();
    provider
        .start(&state, IoTaskKind::LoadSession, None)
        .unwrap();
    provider.prepared_scripts = Some(
        scripts
            .prepare_stopped_session_scripts(Default::default())
            .unwrap(),
    );
    provider.phase = Phase::Committed(operation);
    let (mut delivery, _port) = local::channel();
    provider
        .pump_scripts(&mut client, &mut delivery, &mut scripts)
        .unwrap();
    assert!(matches!(provider.phase, Phase::Unresolved));
    assert!(provider.prepared_scripts.is_some());
    assert!(provider.scripting_suspended());
    assert_eq!(scripts.session_data(), old);
    let id = provider.task().unwrap().id;
    provider
        .apply(
            AppIntent::CancelIoTask { task_id: id },
            &state,
            &mut scripts,
        )
        .unwrap();
    assert!(provider.prepared_scripts.is_none());
    assert!(provider
        .task()
        .unwrap()
        .message
        .contains("musical outcome remains unknown"));
    assert!(provider.messages.is_empty());
    assert_eq!(scripts.session_data(), old);
}

#[shoop_test]
fn ambiguous_commit_delivery_keeps_the_candidate_and_never_replays() {
    struct Broken {
        commits: usize,
    }
    impl Delivery for Broken {
        fn send(&mut self, message: ClientMessage) -> Result<(), Error> {
            assert!(matches!(
                message,
                ClientMessage::Submit {
                    command: Command::CommitPersistence(_),
                    ..
                }
            ));
            self.commits += 1;
            Err(Error::Disconnected)
        }
        fn poll(&mut self, _client: &mut Client) -> Result<usize, Error> {
            Ok(0)
        }
    }
    let f = Fixture::new();
    let state = f.runtime.client().snapshot().unwrap().clone();
    let authority = f.runtime.client().authority().unwrap();
    let mut client = Client::new();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached { authority })
        .unwrap();
    client
        .receive(ServiceMessage::Snapshot {
            authority,
            state: state.clone(),
        })
        .unwrap();
    client.ready_sent().unwrap();
    let mut scripts = shoop_scripting::ScriptManager::new();
    let mut provider = Provider::new();
    provider
        .start(&state, IoTaskKind::LoadSession, None)
        .unwrap();
    provider.prepared_scripts = Some(
        scripts
            .prepare_stopped_session_scripts(Default::default())
            .unwrap(),
    );
    provider.phase = Phase::Ready;
    let mut delivery = Broken { commits: 0 };
    provider.tick(&mut client, &mut delivery, &mut scripts);
    assert_eq!(client.state(), State::Lost);
    assert!(provider.prepared_scripts.is_some());
    assert!(matches!(provider.phase, Phase::Committed(_)));
    provider.tick(&mut client, &mut delivery, &mut scripts);
    assert_eq!(delivery.commits, 1);
}

#[shoop_test]
async fn incompatible_script_data_survives_accepted_format_conversion_and_export() {
    let source = "shoop_announce_api_version(1, 99); error('future API must not run')";
    let mut f = Fixture::new();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "future.shoop".into(),
        bytes: archive(source, 2000),
    });
    let consent = f
        .until(|snapshot| {
            snapshot.io_task.as_ref().is_some_and(|task| {
                task.status == IoTaskStatus::AwaitingFormatConversionConfirmation
            })
        })
        .await;
    f.dispatch(AppIntent::ConfirmFormatConversion {
        task_id: consent.io_task.as_ref().unwrap().id,
        accept: true,
    });
    let loaded = f.until(done).await;
    let id = loaded.scripting.scripts[0].id;
    assert_eq!(
        loaded.scripting.scripts[0].lifecycle,
        ScriptLifecycle::Inactive
    );
    f.dispatch(AppIntent::RestartScript { script_id: id });
    assert_eq!(
        f.runtime.snapshot().scripting.scripts[0].lifecycle,
        ScriptLifecycle::Incompatible
    );
    f.dispatch(AppIntent::ExportScript { script_id: id });
    assert_eq!(f.output().await.bytes.as_slice(), source.as_bytes());
    f.dispatch(AppIntent::RequestSaveSession);
    let saved = shoop_session::decode_session(&f.output().await.bytes).unwrap();
    assert_eq!(saved.document.sample_rate, 1000);
    assert_eq!(
        saved.scripts[&77].entrypoint_resource().bytes.as_ref(),
        source.as_bytes()
    );
}

#[shoop_test]
async fn reattachment_never_pairs_retained_scripts_with_another_clients_replacement() {
    let mut f = Fixture::new();
    f.dispatch(AppIntent::LoadSessionBytes {
        name: "original.shoop".into(),
        bytes: archive(SOURCE, 1000),
    });
    let original = f.until(done).await;
    let id = original.scripting.scripts[0].id;
    let epoch = f.runtime.client().snapshot().unwrap().persistence_epoch;
    f.service.detach(ConnectionId(1));
    f.runtime.fail("lost connection".into());
    let (delivery, port) = local::channel();
    let mut other = Runtime::new(
        Client::new(),
        Box::new(delivery),
        Box::new(shoop_scripting::NullMidiService),
        Startup::Connected(Vec::new()),
    )
    .unwrap()
    .with_persistence_io(Box::new(Provider::new()));
    for _ in 0..8 {
        f.service.pump_local(ConnectionId(2), &port, STEP);
        other.tick(STEP);
    }
    other.dispatch(AppIntent::RequestNewSession).unwrap();
    for _ in 0..10_000 {
        f.service.pump_local(ConnectionId(2), &port, STEP);
        other.tick(STEP);
        idle().await;
        if other
            .snapshot()
            .io_task
            .as_ref()
            .is_some_and(|task| task.status == IoTaskStatus::Completed)
        {
            break;
        }
    }
    let replacement_epoch = other.client().snapshot().unwrap().persistence_epoch;
    assert_eq!(replacement_epoch, epoch + 1);
    f.service.detach(ConnectionId(2));
    drop(other);
    let (delivery, port) = local::channel();
    f.runtime.reconnect(Box::new(delivery)).unwrap();
    f.port = port;
    for _ in 0..8 {
        f.step();
    }
    assert_eq!(
        f.runtime.client().snapshot().unwrap().persistence_epoch,
        replacement_epoch
    );
    assert_eq!(f.runtime.snapshot().scripting.scripts[0].id, id);
    assert_eq!(
        f.runtime.snapshot().scripting.scripts[0].lifecycle,
        ScriptLifecycle::Inactive
    );
    let previous_task = f.runtime.snapshot().io_task.as_ref().unwrap().id;
    f.dispatch(AppIntent::RequestSaveSession);
    assert!(f.runtime.take_file_output().is_none());
    assert_eq!(
        f.runtime.snapshot().io_task.as_ref().unwrap().id,
        previous_task
    );
    assert!(f
        .runtime
        .snapshot()
        .notifications
        .iter()
        .any(|notice| notice.message.contains("musical project was replaced")));
    f.dispatch(AppIntent::ExportScript { script_id: id });
    assert_eq!(f.output().await.bytes.as_slice(), SOURCE.as_bytes());
    f.dispatch(AppIntent::RequestNewSession);
    f.until(done).await;
    f.dispatch(AppIntent::RequestSaveSession);
    assert!(shoop_session::decode_session(&f.output().await.bytes)
        .unwrap()
        .scripts
        .is_empty());
}
