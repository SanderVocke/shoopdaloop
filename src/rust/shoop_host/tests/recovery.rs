#![cfg(not(target_arch = "wasm32"))]
use shoop_app_api::*;
use shoop_app_client::{Client, State};
use shoop_app_protocol::*;
use shoop_host::{
    config::AppTransport,
    config::HostConfig,
    connection::{self, Connection},
    remote::Target as HostEntry,
};
use shoop_wasm_test_support::shoop_test;
use std::{
    path::{Path, PathBuf},
    process::{Child, Command as Process, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Host {
    child: Child,
    root: tempfile::TempDir,
    path: PathBuf,
    entry: HostEntry,
}
impl Drop for Host {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
fn host(transport: AppTransport) -> Host {
    host_with_loop_memory(transport, 16)
}
fn host_with_loop_memory(transport: AppTransport, loop_memory_mb: u32) -> Host {
    let root = tempfile::tempdir().unwrap();
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                tracks: 8,
                loops: 80,
                channels: 2,
                loop_memory_mb,
            },
            ..Default::default()
        },
        transport,
        test_signal: true,
        ..Default::default()
    };
    let config_path = root.path().join("config.json");
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    let key = root.path().join("host.key");
    shoop_host::remote::create_credential(&key).unwrap();
    let path = root.path().join("profile.json");

    let mut child = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .env("RUST_LOG", "info")
        .args(["--config"])
        .arg(&config_path)
        .arg("--credential-file")
        .arg(&key)
        .arg("--export-profile")
        .arg(&path)
        .arg("--log-file")
        .arg(root.path().join("host.log"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "host exited before readiness"
        );
        assert!(Instant::now() < deadline, "host startup timed out");
        thread::sleep(Duration::from_millis(5));
    }
    let entry = HostEntry::read(&path).unwrap();
    Host {
        child,
        root,
        path,
        entry,
    }
}
fn attach(entry: &HostEntry, _id: u128) -> (Client, Connection) {
    let mut connection = connection::connect(entry, Duration::from_secs(2)).unwrap();
    let mut client = Client::new();
    connection.send(client.attach().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.state() != State::Ready {
        connection.poll(&mut client).unwrap();
        if let Some(message) = client.ready_message() {
            connection.send(message).unwrap();
            client.ready_sent().unwrap();
        }
        assert!(Instant::now() < deadline, "client attach timed out");
        thread::sleep(Duration::from_millis(2));
    }
    (client, connection)
}
fn command(
    client: &mut Client,
    connection: &mut Connection,
    command: shoop_app_protocol::Command,
) -> Outcome {
    let message = client.command(command, None).unwrap();
    let ClientMessage::Submit { operation, .. } = message else {
        unreachable!()
    };
    connection.send(message).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        connection.poll(client).unwrap();
        if let Some(outcome) = client.results().get(&operation) {
            return outcome.clone();
        }
        assert!(Instant::now() < deadline, "operation timed out");
        thread::sleep(Duration::from_millis(2));
    }
}
fn shutdown(host: &mut Host, client: &mut Client) {
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = client.shutdown().unwrap()
    else {
        unreachable!()
    };
    let mut reply = connection::management(
        &host.entry,
        ManagementRequest::Shutdown {
            authority,
            operation,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match reply {
            ManagementReply::Finished { result, .. } => {
                assert!(
                    matches!(result.outcome, Outcome::Confirmed { .. }),
                    "{:?}",
                    result.outcome
                );
                break;
            }
            ManagementReply::Accepted { .. } => {}
            other => panic!("shutdown refused: {other:?}"),
        }
        assert!(Instant::now() < deadline, "shutdown timed out");
        thread::sleep(Duration::from_millis(10));
        reply = connection::management(
            &host.entry,
            ManagementRequest::Result {
                authority,
                operation,
            },
            Duration::from_secs(2),
        )
        .unwrap();
    }
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "host did not exit after cleanup");
        thread::sleep(Duration::from_millis(10));
    }
    assert!(host.path.exists());
    assert!(host.entry.credential.exists());
}

#[shoop_test]
fn ui_peer() {
    let Some(path) = std::env::var_os("SHOOP_TEST_ENTRY") else {
        return;
    };
    let entry = HostEntry::read(Path::new(&path)).unwrap();
    let (mut client, mut connection) = attach(&entry, 300);
    command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Global(GlobalControlAction::SetSync(false)),
    );
    let outcome = command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::AddTrack(TrackSpec {
            name: "Surviving take".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: true,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    );
    let Outcome::TrackCreated { track, loops, .. } = outcome else {
        panic!("{outcome:?}")
    };
    command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    );
    std::fs::write(
        std::env::var_os("SHOOP_TEST_READY").unwrap(),
        format!("{} {}", track.raw(), loops[0].raw()),
    )
    .unwrap();
    loop {
        if std::env::var_os("SHOOP_TEST_EXIT").is_some_and(|path| Path::new(&path).exists()) {
            return;
        }
        connection.poll(&mut client).unwrap();
        thread::sleep(Duration::from_millis(5));
    }
}

#[shoop_test]
fn killing_a_ui_preserves_recording_and_a_replacement_can_read_media_and_close_the_host() {
    let mut host = host(AppTransport::Socket);
    let ready = host.root.path().join("ui-ready");
    let mut peer = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "ui_peer", "--nocapture"])
        .env("SHOOP_TEST_ENTRY", &host.path)
        .env("SHOOP_TEST_READY", &ready)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready.exists() {
        assert!(peer.try_wait().unwrap().is_none(), "UI peer failed");
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    let ids: Vec<u64> = std::fs::read_to_string(&ready)
        .unwrap()
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    thread::sleep(Duration::from_millis(50));
    peer.kill().unwrap();
    peer.wait().unwrap();
    thread::sleep(Duration::from_millis(50));
    assert!(host.child.try_wait().unwrap().is_none());
    let (mut client, mut connection) = attach(&host.entry, 301);
    let state = client.snapshot().unwrap();
    assert_eq!(state.tracks.len(), 2);
    assert_eq!(state.tracks[1].id.raw(), ids[0]);
    assert_eq!(state.tracks[1].loops[0].id.raw(), ids[1]);
    assert_eq!(state.tracks[1].name, "Surviving take");
    assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
    assert!(state.status.processed_frames > 0);
    let track = TrackId::from_raw(ids[0]);
    let loop_id = LoopId::from_raw(ids[1]);
    command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Loop {
            track,
            loop_id,
            action: LoopAction::PlayClicked,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Playing {
        connection.poll(&mut client).unwrap();
        assert!(
            Instant::now() < deadline,
            "admitted playback did not execute"
        );
        thread::sleep(Duration::from_millis(2));
    }
    connection
        .send(client.read_media(loop_id).unwrap())
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.media().is_none() {
        connection.poll(&mut client).unwrap();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let media = client.media().unwrap().as_ref().unwrap();
    assert!(media.audio[0].samples.len() > 100);
    assert!(media.audio[0].samples.iter().any(|sample| *sample == 0.25));
    let authority = client.authority().unwrap();
    drop(connection);
    thread::sleep(Duration::from_millis(20));
    let (mut replacement, _connection) = attach(&host.entry, 302);
    let stale = connection::management(
        &host.entry,
        ManagementRequest::Shutdown {
            authority,
            operation: OperationId {
                client: authority.client,
                sequence: 1000,
            },
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(stale, ManagementReply::Refused(Refusal::StaleAuthority));
    shutdown(&mut host, &mut replacement);
}

#[shoop_test]
fn busy_and_identity_failures_do_not_evict_the_owner_and_shared_memory_starts_and_exits() {
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host(transport);
        let (mut owner, mut connection) = attach(&host.entry, 400);
        if transport == AppTransport::Socket {
            let mut second = connection::connect(&host.entry, Duration::from_secs(2)).unwrap();
            let mut client = Client::new();
            second.send(client.attach().unwrap()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(2);
            while client.state() != State::Lost {
                let _ = second.poll(&mut client);
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(2));
            }
        } else {
            assert!(connection::connect(&host.entry, Duration::from_secs(2)).is_err());
        }
        assert!(matches!(
            command(
                &mut owner,
                &mut connection,
                shoop_app_protocol::Command::Track {
                    track: TrackId::from_raw(1),
                    action: TrackAction::NameChanged("Owner still controls".into())
                }
            ),
            Outcome::Confirmed { .. }
        ));
        let wrong = connection::management(
            &host.entry,
            ManagementRequest::Status {
                version: VERSION + 1,
            },
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(wrong, ManagementReply::Refused(Refusal::Version));
        shutdown(&mut host, &mut owner);
    }
}

#[shoop_test]
fn launcher_peer() {
    let Some(root) = std::env::var_os("SHOOP_TEST_LAUNCH_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    let mut options = shoop_host::serve::StartOptions::new().unwrap();
    let key = root.join("host.key");
    shoop_host::remote::create_credential(&key).unwrap();
    options.credential = Some(key);
    options.export_profile = Some(root.join("profile.json"));
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                tracks: 8,
                loops: 80,
                channels: 2,
                loop_memory_mb: 16,
            },
            ..Default::default()
        },
        test_signal: true,
        ..Default::default()
    };
    let path = shoop_host::launch::pair(
        &config,
        options,
        Path::new(env!("CARGO_BIN_EXE_shoop_host")),
        |path| {
            let mut command = Process::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "ui_peer", "--nocapture"])
                .env("SHOOP_TEST_ENTRY", path)
                .env("SHOOP_TEST_READY", root.join("ui-ready"))
                .env("SHOOP_TEST_EXIT", root.join("ui-exit"));
            command
        },
    )
    .unwrap();
    std::fs::write(root.join("entry-path"), path.to_str().unwrap()).unwrap();
}

#[shoop_test]
fn siblings_survive_the_launcher_exit_and_later_control_needs_no_child_handle() {
    let root = tempfile::tempdir().unwrap();
    let mut launcher = Process::new(std::env::current_exe().unwrap())
        .args(["--exact", "launcher_peer", "--nocapture"])
        .env("SHOOP_TEST_LAUNCH_ROOT", root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(launcher.wait().unwrap().success());
    let path = PathBuf::from(std::fs::read_to_string(root.path().join("entry-path")).unwrap());
    let entry = HostEntry::read(&path).unwrap();
    struct Cleanup(HostEntry, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::write(self.1.join("ui-exit"), b"");
            thread::sleep(Duration::from_millis(100));
            let _ = connection::shutdown_ownerless(&self.0, Duration::from_secs(3));
        }
    }
    let _cleanup = Cleanup(entry.clone(), root.path().to_owned());
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.path().join("ui-ready").exists() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    std::fs::write(root.path().join("ui-exit"), b"").unwrap();
    thread::sleep(Duration::from_millis(50));
    let (mut client, _connection) = attach(&entry, 600);
    assert_eq!(client.snapshot().unwrap().tracks[1].name, "Surviving take");
    assert_eq!(
        client.snapshot().unwrap().tracks[1].loops[0].mode,
        LoopMode::Recording
    );
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = client.shutdown().unwrap()
    else {
        unreachable!()
    };
    let mut reply = connection::management(
        &entry,
        ManagementRequest::Shutdown {
            authority,
            operation,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    loop {
        match reply {
            ManagementReply::Finished { result, .. } => {
                assert!(matches!(result.outcome, Outcome::Confirmed { .. }));
                break;
            }
            ManagementReply::Accepted { .. } => {}
            other => panic!("{other:?}"),
        }
        thread::sleep(Duration::from_millis(10));
        assert!(Instant::now() < deadline);
        reply = connection::management(
            &entry,
            ManagementRequest::Result {
                authority,
                operation,
            },
            Duration::from_secs(2),
        )
        .unwrap();
    }
    while connection::management(
        &entry,
        ManagementRequest::Status { version: VERSION },
        Duration::from_millis(200),
    )
    .is_ok()
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists() && entry.credential.exists());
}

#[shoop_test]
fn bad_credentials_versions_and_early_disconnects_leave_the_session_healthy() {
    let mut host = host(AppTransport::Socket);
    assert!(shoop_transport_socket::setup::connect(
        host.entry.endpoint.parse().unwrap(),
        &[0; 32],
        Default::default(),
        Duration::from_millis(150)
    )
    .is_err());
    drop(
        shoop_transport_socket::setup::connect(
            host.entry.endpoint.parse().unwrap(),
            &host.entry.token().unwrap(),
            Default::default(),
            Duration::from_secs(2),
        )
        .unwrap(),
    );
    let reply = connection::management(
        &host.entry,
        ManagementRequest::Status {
            version: VERSION + 1,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(reply, ManagementReply::Refused(Refusal::Version));
    let (mut client, connection) = attach(&host.entry, 700);
    let authority = client.authority().unwrap();
    drop(connection);
    thread::sleep(Duration::from_millis(20));
    let reply = connection::management(
        &host.entry,
        ManagementRequest::Result {
            authority,
            operation: OperationId {
                client: authority.client,
                sequence: 1,
            },
        },
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(reply, ManagementReply::Refused(Refusal::Invalid));
    shutdown(&mut host, &mut client);
}

#[shoop_test]
fn authenticated_management_can_close_a_shared_host_after_its_only_data_connection_is_lost() {
    let mut host = host(AppTransport::SharedMemory);
    let (_client, connection) = attach(&host.entry, 900);
    assert!(connection::shutdown_ownerless(&host.entry, Duration::from_secs(2)).is_err());
    drop(connection);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let reply = connection::management(
            &host.entry,
            ManagementRequest::Status { version: VERSION },
            Duration::from_secs(2),
        )
        .unwrap();
        if matches!(reply, ManagementReply::Status { busy: false, .. }) {
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    assert!(connection::connect(&host.entry, Duration::from_secs(2)).is_err());
    connection::shutdown_ownerless(&host.entry, Duration::from_secs(5)).unwrap();
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline + Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    assert!(host.path.exists());
}

#[shoop_test]
fn bounded_recovery_stress_preserves_recording_and_reclaims_departed_connections() {
    let loop_memory_mb = NewEnginePools::default().loop_memory_mb;
    let mut host = host_with_loop_memory(AppTransport::Socket, loop_memory_mb);
    let (mut client, mut connection) = attach(&host.entry, 1000);
    command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Global(GlobalControlAction::SetSync(false)),
    );
    let Outcome::TrackCreated { track, loops, .. } = command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::AddTrack(TrackSpec {
            name: "Stress recording".into(),
            topology: TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    ) else {
        unreachable!()
    };
    command(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Loop {
            track,
            loop_id: loops[0],
            action: LoopAction::RecordClicked,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Recording {
        connection.poll(&mut client).unwrap();
        assert!(Instant::now() < deadline, "recording did not execute");
        thread::sleep(Duration::from_millis(2));
    }
    let mut frames = client.snapshot().unwrap().status.processed_frames;
    let descriptor_count = || {
        std::fs::read_dir(format!("/proc/{}/fd", host.child.id()))
            .ok()
            .map(|entries| entries.count())
    };
    let descriptors_before = descriptor_count();
    let mut attach_ms = Vec::new();
    let mut control_ms = Vec::new();
    for index in 0..32 {
        drop(connection);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let status = connection::management(
                &host.entry,
                ManagementRequest::Status { version: VERSION },
                Duration::from_secs(2),
            )
            .unwrap();
            if matches!(status, ManagementReply::Status { busy: false, .. }) {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        let started = Instant::now();
        (client, connection) = attach(&host.entry, 1001 + index);
        attach_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while client.snapshot().unwrap().status.processed_frames <= frames {
            connection.poll(&mut client).unwrap();
            assert!(Instant::now() < deadline, "audio frames stopped advancing");
            thread::sleep(Duration::from_millis(2));
        }
        let state = client.snapshot().unwrap();
        assert_eq!(state.tracks[1].id, track);
        assert_eq!(state.tracks[1].loops[0].id, loops[0]);
        assert_eq!(state.tracks[1].loops[0].mode, LoopMode::Recording);
        assert!(state.status.processed_frames > frames);
        frames = state.status.processed_frames;
        let started = Instant::now();
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::Track {
                track,
                action: TrackAction::OutputGainChanged(-6.0 + (index % 4) as f32),
            },
        );
        control_ms.push(started.elapsed().as_secs_f64() * 1000.0);
        assert!(client.snapshot().unwrap().pending.len() <= MAX_PENDING_OPERATIONS);
        assert!(client.snapshot().unwrap().results.len() <= MAX_RETAINED_RESULTS);
    }
    let descriptors_after = descriptor_count();
    if let (Some(before), Some(after)) = (descriptors_before, descriptors_after) {
        assert!(
            after <= before + 4,
            "departed connections leaked descriptors: {before} -> {after}"
        );
    }
    attach_ms.sort_by(f64::total_cmp);
    control_ms.sort_by(f64::total_cmp);
    eprintln!(
        "recovery stress: {}",
        serde_json::json!({"cycles":32, "loop_memory_mb":loop_memory_mb, "attach_ms_p50":attach_ms[16], "attach_ms_p95":attach_ms[30], "control_ms_p50":control_ms[16], "control_ms_p95":control_ms[30], "host_fds_before":descriptors_before, "host_fds_after":descriptors_after, "frames":frames})
    );
    shutdown(&mut host, &mut client);
}

#[shoop_test]
fn invalid_startup_is_rolled_back_and_a_ready_host_survives_ui_spawn_failure() {
    let root = tempfile::tempdir().unwrap();
    let config_path = root.path().join("invalid.json");
    std::fs::write(
        &config_path,
        br#"{"pools":{"tracks":0,"loops":1,"channels":2,"loop_memory_mb":1}}"#,
    )
    .unwrap();
    let key = root.path().join("host.key");
    shoop_host::remote::create_credential(&key).unwrap();
    let profile = root.path().join("profile.json");
    let status = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--config")
        .arg(&config_path)
        .arg("--credential-file")
        .arg(&key)
        .arg("--export-profile")
        .arg(&profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!profile.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    let mut options = shoop_host::serve::StartOptions::new().unwrap();
    options.credential = Some(key);
    options.export_profile = Some(profile.clone());
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                tracks: 8,
                loops: 80,
                channels: 2,
                loop_memory_mb: 16,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let result = shoop_host::launch::pair(
        &config,
        options,
        Path::new(env!("CARGO_BIN_EXE_shoop_host")),
        |_| Process::new(root.path().join("missing-ui")),
    );
    assert!(result.is_err());
    let path = profile;
    let entry = HostEntry::read(&path).unwrap();
    let (mut client, _connection) = attach(&entry, 800);
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = client.shutdown().unwrap()
    else {
        unreachable!()
    };
    let mut reply = connection::management(
        &entry,
        ManagementRequest::Shutdown {
            authority,
            operation,
        },
        Duration::from_secs(2),
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match reply {
            ManagementReply::Finished { result, .. } => {
                assert!(matches!(result.outcome, Outcome::Confirmed { .. }));
                break;
            }
            ManagementReply::Accepted { .. } => {}
            other => panic!("{other:?}"),
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
        reply = connection::management(
            &entry,
            ManagementRequest::Result {
                authority,
                operation,
            },
            Duration::from_secs(2),
        )
        .unwrap();
    }
    while connection::management(
        &entry,
        ManagementRequest::Status { version: VERSION },
        Duration::from_millis(200),
    )
    .is_ok()
    {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
    assert!(path.exists() && entry.credential.exists());
}

#[shoop_test]
fn management_monitoring_preserves_controller_and_logs_startup_failures() {
    let mut host = host(AppTransport::Socket);
    let (mut client, mut connection) = attach(&host.entry, 1);
    for _ in 0..3 {
        let report = connection::host_report(&host.entry, Duration::from_secs(2)).unwrap();
        assert!(report.ready && report.busy);
        assert!(connection::shutdown_ownerless(&host.entry, Duration::from_secs(2)).is_err());
        connection.poll(&mut client).unwrap();
        assert_eq!(client.state(), State::Ready);
    }
    let log_path = host.root.path().join("host.log");
    let log = shoop_host::logging::tail(&log_path).unwrap();
    assert!(log.contains("Host ready:"));
    shutdown(&mut host, &mut client);
    assert!(shoop_host::logging::tail(&log_path)
        .unwrap()
        .contains("Host cleanup confirmed"));

    let log_path = host.root.path().join("failed-start.log");
    let mut failed = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--config")
        .arg(host.root.path().join("missing.json"))
        .arg("--credential-file")
        .arg(&host.entry.credential)
        .arg("--log-file")
        .arg(&log_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(!failed.wait().unwrap().success());
    assert!(shoop_host::logging::tail(&log_path)
        .unwrap()
        .contains("session host:"));
}

#[shoop_test]
fn a_host_without_requested_outputs_writes_no_host_records_or_logs() {
    let root = tempfile::tempdir().unwrap();
    let key = root.path().join("provided.key");
    shoop_host::remote::create_credential(&key).unwrap();
    let mut options = shoop_host::serve::StartOptions::new().unwrap();
    options.credential = Some(key.clone());
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                tracks: 2,
                loops: 8,
                channels: 2,
                loop_memory_mb: 8,
            },
            ..Default::default()
        },
        ..Default::default()
    };
    let (mut child, target) = shoop_host::launch::spawn(
        &config,
        options,
        Path::new(env!("CARGO_BIN_EXE_shoop_host")),
    )
    .unwrap();
    assert!(target.log_file.is_none());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    connection::shutdown_ownerless(&target, Duration::from_secs(5)).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    assert!(key.exists());
    let status = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--registry")
        .arg(root.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
}
