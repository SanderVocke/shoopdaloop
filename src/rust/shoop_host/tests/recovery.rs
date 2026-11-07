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
    stderr_path: PathBuf,
}
impl Drop for Host {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("host process state on failure: {:?}", self.child.try_wait());
            let stderr = std::fs::read_to_string(&self.stderr_path).unwrap_or_default();
            eprintln!(
                "host stderr: {}",
                stderr
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
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
    host_with_driver(transport, loop_memory_mb, None)
}
fn host_with_driver(
    transport: AppTransport,
    loop_memory_mb: u32,
    driver: Option<AudioDriverConfig>,
) -> Host {
    let root = tempfile::tempdir().unwrap();
    let real_device = driver.is_some();
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            processors: ProcessorSettings {
                builtin_fx: ProcessorPolicy::Off,
                oxisynth: ProcessorPolicy::Off,
                ..Default::default()
            },
            driver: driver.unwrap_or_default(),
            pools: NewEnginePools {
                quantum: 1,
                buses: 8,
                bus_channels: 8,
                tracks: 8,
                loops: 80,
                channels: 2,
                loop_memory_mb,
            },
            ..Default::default()
        },
        transport,
        test_signal: !real_device,
        ..Default::default()
    };
    let config_path = root.path().join("config.json");
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

    let path = root.path().join("profile.json");
    let log_path = if real_device || std::env::var_os("SHOOP_PREVIEW_LOG_DIR").is_some() {
        std::env::var_os("SHOOP_PREVIEW_LOG_DIR")
            .map(PathBuf::from)
            .map(|root| root.join(format!("host-{transport:?}.log")))
            .unwrap_or_else(|| root.path().join("host.log"))
    } else {
        root.path().join("host.log")
    };

    let stderr_path = log_path.with_extension("stderr.log");
    let mut child = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .env("RUST_LOG", "info")
        .args(["--config"])
        .arg(&config_path)
        .arg("--export-profile")
        .arg(&path)
        .arg("--log-file")
        .arg(&log_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(std::fs::File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(if real_device { 30 } else { 10 });
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "host exited before readiness: {}",
            std::fs::read_to_string(&log_path).unwrap_or_default()
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
        stderr_path,
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
    command_with_timeout(client, connection, command, Duration::from_secs(5))
}
fn command_with_timeout(
    client: &mut Client,
    connection: &mut Connection,
    command: shoop_app_protocol::Command,
    timeout: Duration,
) -> Outcome {
    let message = client.command(command, None).unwrap();
    let ClientMessage::Submit { operation, .. } = message else {
        unreachable!()
    };
    connection.send(message).unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        connection.poll(client).unwrap();
        if let Some(outcome) = client.results().get(&operation) {
            let projected = match outcome {
                Outcome::Confirmed { revision, .. }
                | Outcome::TrackCreated { revision, .. }
                | Outcome::LoopCreated { revision, .. } => client
                    .snapshot()
                    .is_some_and(|snapshot| snapshot.revision >= *revision),
                _ => true,
            };
            if projected {
                return outcome.clone();
            }
        }
        assert!(
            Instant::now() < deadline,
            "operation timed out: {:?}",
            client.snapshot().map(|s| (&s.audio_drivers, &s.pending))
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[shoop_test]
fn bus_lifecycle_uses_both_carriers_and_socket_recovery_does_not_recreate_master() {
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host(transport);
        let (mut client, mut connection) = attach(&host.entry, 801);
        for (name, channel_count) in [("Cue", 1), ("Wide", 8)] {
            assert!(matches!(
                command(
                    &mut client,
                    &mut connection,
                    shoop_app_protocol::Command::AddBus(BusSpec {
                        name: name.into(),
                        channel_count,
                        creation_request_id: None,
                        fx: None
                    })
                ),
                Outcome::Confirmed { .. }
            ));
        }
        let wide = client.snapshot().unwrap().buses[2].id;
        let master = client.snapshot().unwrap().buses[0].id;
        assert!(matches!(
            command(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::Bus {
                    bus: wide,
                    action: BusAction::MoveBefore(Some(master))
                }
            ),
            Outcome::Confirmed { .. }
        ));
        let state = client.snapshot().unwrap();
        let ids = state.buses.iter().map(|b| b.id).collect::<Vec<_>>();
        let message = client
            .command(
                shoop_app_protocol::Command::Bus {
                    bus: wide,
                    action: BusAction::GainChanged(-6.),
                },
                None,
            )
            .unwrap();
        let ClientMessage::Submit { operation, .. } = &message else {
            unreachable!()
        };
        let operation = *operation;
        connection.send(message).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !client.pending().contains_key(&operation)
            && !client.results().contains_key(&operation)
        {
            connection.poll(&mut client).unwrap();
            assert!(Instant::now() < deadline, "bus control was not admitted");
            thread::sleep(Duration::from_millis(2));
        }
        if transport == AppTransport::Socket {
            drop(connection);
            let deadline = Instant::now() + Duration::from_secs(5);
            while matches!(
                connection::management(
                    &host.entry,
                    ManagementRequest::Status { version: VERSION },
                    Duration::from_secs(1)
                )
                .unwrap(),
                ManagementReply::Status { busy: true, .. }
            ) {
                assert!(Instant::now() < deadline);
                thread::sleep(Duration::from_millis(10));
            }
            (client, connection) = attach(&host.entry, 802);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            connection.poll(&mut client).unwrap();
            let snapshot = client.snapshot().unwrap();
            if snapshot.results.iter().any(|result| {
                result.id == operation
                    && matches!(result.outcome, Outcome::Confirmed { revision } if snapshot.revision >= revision)
            }) {
                break;
            }
            assert!(Instant::now() < deadline, "bus control was not projected");
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(
            client
                .snapshot()
                .unwrap()
                .buses
                .iter()
                .map(|b| b.id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(client.snapshot().unwrap().buses[0].gain_db, -6.);
        for bus in ids {
            assert!(matches!(
                command(
                    &mut client,
                    &mut connection,
                    shoop_app_protocol::Command::Bus {
                        bus,
                        action: BusAction::Remove
                    }
                ),
                Outcome::Confirmed { .. }
            ));
        }
        assert!(client.snapshot().unwrap().buses.is_empty());
        assert!(matches!(
            command(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::NewSession
            ),
            Outcome::Confirmed { .. }
        ));
        assert_eq!(client.snapshot().unwrap().buses.len(), 1);
        assert_eq!(client.snapshot().unwrap().buses[0].name, "Master");
        drop(connection);
        shutdown(&mut host, &mut client);
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
}

#[shoop_test]
fn ui_peer() {
    let entry = if std::env::var_os("SHOOP_TEST_STDIN").is_some() {
        let address: String = shoop_host::launch::read_message(std::io::stdin().lock()).unwrap();
        connection::describe(&address, Duration::from_secs(2)).unwrap()
    } else if let Ok(address) = std::env::var("SHOOP_TEST_ADDRESS") {
        connection::describe(&address, Duration::from_secs(2)).unwrap()
    } else if let Some(path) = std::env::var_os("SHOOP_TEST_ENTRY") {
        HostEntry::read(Path::new(&path)).unwrap()
    } else {
        return;
    };
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
    let deadline = Instant::now() + Duration::from_secs(5);
    while client.snapshot().unwrap().tracks[1].loops[0].mode != LoopMode::Recording {
        connection.poll(&mut client).unwrap();
        assert!(
            Instant::now() < deadline,
            "recording did not start before UI readiness"
        );
        thread::sleep(Duration::from_millis(2));
    }
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
fn app_version_and_endpoint_role_are_checked_before_shared_attachment_admission() {
    use shoop_transport::{RecordPort, MAX_RECORD_BYTES};
    use std::io::Write;
    let mut host = host(AppTransport::SharedMemory);
    let data = connection::Endpoint::Data as u8;
    let management = connection::Endpoint::Management as u8;
    for (address, version, role) in [
        (&host.entry.endpoint, VERSION, management),
        (&host.entry.management, VERSION, data),
        (&host.entry.endpoint, VERSION + 1, data),
        (&host.entry.management, VERSION + 1, management),
    ] {
        let mut raw = shoop_transport_socket::setup::connect(
            address.parse().unwrap(),
            Default::default(),
            Duration::from_secs(2),
        )
        .unwrap();
        let version = version.to_le_bytes();
        for byte in [version[0], version[1], role] {
            raw.stream().write_all(&[byte]).unwrap();
            thread::sleep(Duration::from_millis(2));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        while raw.failure().is_none() {
            assert!(
                raw.take(&mut [0; MAX_RECORD_BYTES]).is_none(),
                "invalid admission reached the session"
            );
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        assert!(
            connection::describe(&host.entry.endpoint, Duration::from_secs(2))
                .unwrap()
                .can_attach
        );
    }
    let mut raw = shoop_transport_socket::setup::connect(
        host.entry.endpoint.parse().unwrap(),
        Default::default(),
        Duration::from_secs(2),
    )
    .unwrap();
    raw.stream().write_all(&VERSION.to_le_bytes()).unwrap();
    assert!(
        connection::describe(&host.entry.management, Duration::from_secs(2))
            .unwrap()
            .can_attach
    );
    raw.stream().write_all(&[data]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut bytes = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(count) = raw.take(&mut bytes) {
            assert!(matches!(
                codec::decode(&bytes[..count]).unwrap(),
                WireMessage::CarrierReady
            ));
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert!(
        !connection::describe(&host.entry.endpoint, Duration::from_secs(2))
            .unwrap()
            .can_attach
    );
    drop(raw);
    connection::shutdown_ownerless(&host.entry, Duration::from_secs(5)).unwrap();
    assert!(host.child.wait().unwrap().success());
}

#[shoop_test]
fn busy_and_identity_failures_do_not_evict_the_owner_and_shared_memory_starts_and_exits() {
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host(transport);
        let identity = shoop_host_processes::identity(host.child.id())
            .unwrap()
            .unwrap();
        let mut instance = None;
        for address in [&host.entry.endpoint, &host.entry.management] {
            for _ in 0..3 {
                let description = connection::describe(address, Duration::from_secs(2)).unwrap();
                assert_eq!(description.process.as_ref(), Some(&identity));
                assert!(description.can_attach);
                assert_eq!(
                    *instance.get_or_insert(description.instance),
                    description.instance
                );
                let profile = serde_json::to_value(description.profile()).unwrap();
                for field in [
                    "process",
                    "instance",
                    "credential",
                    "can_attach",
                    "shared_memory",
                ] {
                    assert!(profile.get(field).is_none());
                }
            }
        }
        let (mut owner, mut connection) = attach(&host.entry, 400);
        for _ in 0..3 {
            let description =
                connection::describe(&host.entry.endpoint, Duration::from_secs(2)).unwrap();
            assert_eq!(Some(description.instance), instance);
            assert_eq!(description.can_attach, transport == AppTransport::Socket);
            assert!(
                connection::host_report(&description, Duration::from_secs(2))
                    .unwrap()
                    .busy
            );
            connection.poll(&mut owner).unwrap();
            assert_eq!(owner.state(), State::Ready);
        }
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
    let options = shoop_host::serve::StartOptions::new().unwrap();
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                quantum: 1,
                buses: 8,
                bus_channels: 8,
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
    let target = shoop_host::launch::pair(
        &config,
        options,
        Path::new(env!("CARGO_BIN_EXE_shoop_host")),
        |_target| {
            let mut command = Process::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "ui_peer", "--nocapture"])
                .env("SHOOP_TEST_STDIN", "1")
                .env("SHOOP_TEST_READY", root.join("ui-ready"))
                .env("SHOOP_TEST_EXIT", root.join("ui-exit"));
            command
        },
    )
    .unwrap();
    std::fs::write(root.join("entry-address"), &target.management).unwrap();
    assert!(!root.join("profile.json").exists());
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
    let address = std::fs::read_to_string(root.path().join("entry-address")).unwrap();
    let entry = connection::describe(&address, Duration::from_secs(2)).unwrap();
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
    assert!(!root.path().join("profile.json").exists());
}

#[shoop_test]
fn incompatible_greetings_versions_and_early_disconnects_leave_the_session_healthy() {
    let mut host = host(AppTransport::Socket);
    {
        use std::io::Write;
        let mut incompatible = std::net::TcpStream::connect(&host.entry.endpoint).unwrap();
        incompatible.write_all(b"obsolete!").unwrap();
    }
    drop(
        shoop_transport_socket::setup::connect(
            host.entry.endpoint.parse().unwrap(),
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
fn management_can_close_a_shared_host_after_its_only_data_connection_is_lost() {
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

    let profile = root.path().join("profile.json");
    let status = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--config")
        .arg(&config_path)
        .arg("--export-profile")
        .arg(&profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
    assert!(!profile.exists());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
    let mut options = shoop_host::serve::StartOptions::new().unwrap();

    options.export_profile = Some(profile.clone());
    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                quantum: 1,
                buses: 8,
                bus_channels: 8,
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
    assert!(path.exists());
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

    let options = shoop_host::serve::StartOptions::new().unwrap();

    let config = HostConfig {
        settings: shoop_host::config::HostSettings {
            pools: NewEnginePools {
                tracks: 2,
                loops: 8,
                channels: 2,
                loop_memory_mb: 8,
                ..Default::default()
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
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    connection::shutdown_ownerless(&target, Duration::from_secs(5)).unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let status = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--registry")
        .arg(root.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success());
}

#[shoop_test]
fn master_preview_plays_in_the_independent_host_without_creating_loop_content() {
    verify_master_preview(None);
}

#[shoop_test]
#[ignore = "requires an isolated native driver and external signal capture"]
fn native_master_preview_device_acceptance() {
    let path =
        std::env::var("SHOOP_PREVIEW_DRIVER_CONFIG").expect("driver configuration JSON path");
    let driver = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    verify_master_preview(Some(driver));
}

fn verify_master_preview(driver: Option<AudioDriverConfig>) {
    #[cfg(feature = "jack")]
    let capture = matches!(&driver, Some(AudioDriverConfig::Jack(_))).then(JackCapture::new);
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host_with_driver(transport, 16, driver.clone());
        let (mut client, mut connection) = attach(&host.entry, 990);
        let state = client.snapshot().unwrap().clone();
        let master = state.preview_bus.unwrap();
        let rate = state.status.sample_rate;
        #[cfg(feature = "jack")]
        if capture.is_some() {
            let channels = state
                .buses
                .iter()
                .find(|bus| bus.id == master)
                .unwrap()
                .channels
                .clone();
            for (channel, target) in channels.iter().zip(["left", "right"]) {
                assert!(matches!(
                    command(
                        &mut client,
                        &mut connection,
                        Command::SetPortConnected {
                            port: channel.output_port_id,
                            host_port: shoop_app_api::HostPortId::new(format!(
                                "preview_capture:{target}"
                            )),
                            connected: true,
                        }
                    ),
                    Outcome::Confirmed { .. }
                ));
            }
        }
        let content: Vec<_> = state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .map(|entry| (entry.id, entry.length_frames, entry.content_revision))
            .collect();
        let mut submit = |preview| {
            assert!(matches!(
                command(&mut client, &mut connection, Command::Preview(preview)),
                Outcome::Confirmed { .. }
            ));
        };
        submit(PreviewCommand::Begin {
            request_id: 7,
            bus: master,
            sample_rate: rate,
            frames: 12_000,
        });
        for offset in (0..12_000).step_by(PREVIEW_CHUNK_FRAMES) {
            submit(PreviewCommand::Chunk {
                request_id: 7,
                offset: offset as u32,
                samples: vec![0.25; PREVIEW_CHUNK_FRAMES.min(12_000 - offset)],
            });
        }
        submit(PreviewCommand::Play { request_id: 7 });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            connection.poll(&mut client).unwrap();
            let state = client.snapshot().unwrap();
            if state.preview.preview_status == ClickTrackPreviewStatus::Playing
                && state
                    .buses
                    .iter()
                    .find(|bus| bus.id == master)
                    .unwrap()
                    .output_peaks_db
                    .iter()
                    .any(|peak| *peak > -13.)
            {
                break;
            }
            assert_ne!(
                state.preview.preview_status,
                ClickTrackPreviewStatus::Failed,
                "{:?}",
                state.preview
            );
            assert!(
                Instant::now() < deadline,
                "host preview produced no bus output: {:?}",
                state.preview
            );
            thread::sleep(Duration::from_millis(2));
        }
        if driver.is_some() {
            thread::sleep(Duration::from_millis(100));
            #[cfg(feature = "jack")]
            if let Some(capture) = &capture {
                let peaks = capture.take_peaks();
                assert!(
                    peaks.iter().all(|p| (*p - 0.25).abs() < 0.001),
                    "JACK port output: {peaks:?}"
                );
            }
        }
        assert!(matches!(
            command(
                &mut client,
                &mut connection,
                Command::Preview(PreviewCommand::Stop)
            ),
            Outcome::Confirmed { .. }
        ));
        let state = client.snapshot().unwrap();
        assert_eq!(
            state.preview.preview_status,
            ClickTrackPreviewStatus::Cancelled
        );
        assert_eq!(
            state
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .map(|entry| (entry.id, entry.length_frames, entry.content_revision))
                .collect::<Vec<_>>(),
            content
        );
        if driver.is_some() {
            thread::sleep(Duration::from_millis(500));
            connection.poll(&mut client).unwrap();
            assert_eq!(
                client.snapshot().unwrap().status.audio_driver,
                AudioDriverState::Running,
                "{:?}",
                client.snapshot().unwrap().status
            );
            #[cfg(feature = "jack")]
            if let Some(capture) = &capture {
                capture.take_peaks();
                thread::sleep(Duration::from_millis(250));
                let peaks = capture.take_peaks();
                assert!(
                    peaks.iter().all(|p| *p == 0.),
                    "JACK output persisted after Stop: {peaks:?}"
                );
            }
        }
        drop(connection);
        shutdown(&mut host, &mut client);
    }
}

#[cfg(feature = "jack")]
struct CaptureProcess {
    inputs: [jack::Port<jack::AudioIn>; 2],
    peaks: std::sync::Arc<[std::sync::atomic::AtomicU32; 2]>,
}
#[cfg(feature = "jack")]
impl jack::ProcessHandler for CaptureProcess {
    fn process(&mut self, _: &jack::Client, scope: &jack::ProcessScope) -> jack::Control {
        for (input, peak) in self.inputs.iter().zip(self.peaks.iter()) {
            let value = input
                .as_slice(scope)
                .iter()
                .fold(0.0_f32, |a, b| a.max(b.abs()));
            peak.fetch_max(value.to_bits(), std::sync::atomic::Ordering::Relaxed);
        }
        jack::Control::Continue
    }
}
#[cfg(feature = "jack")]
struct JackCapture {
    _client: jack::AsyncClient<(), CaptureProcess>,
    peaks: std::sync::Arc<[std::sync::atomic::AtomicU32; 2]>,
}
#[cfg(feature = "jack")]
impl JackCapture {
    fn new() -> Self {
        let (client, _) =
            jack::Client::new("preview_capture", jack::ClientOptions::NO_START_SERVER).unwrap();
        let inputs = ["left", "right"].map(|name| {
            client
                .register_port(name, jack::AudioIn::default())
                .unwrap()
        });
        let peaks = std::sync::Arc::new(core::array::from_fn(|_| {
            std::sync::atomic::AtomicU32::new(0)
        }));
        let handler = CaptureProcess {
            inputs,
            peaks: std::sync::Arc::clone(&peaks),
        };
        Self {
            _client: client.activate_async((), handler).unwrap(),
            peaks,
        }
    }
    fn take_peaks(&self) -> [f32; 2] {
        core::array::from_fn(|i| {
            f32::from_bits(self.peaks[i].swap(0, std::sync::atomic::Ordering::Relaxed))
        })
    }
}

fn driver_snapshot(
    client: &mut Client,
    connection: &mut Connection,
    predicate: impl Fn(&SessionSnapshot) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        connection.poll(client).unwrap();
        if client.snapshot().is_some_and(|s| predicate(s)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "driver state timed out: {:?}",
            client.snapshot().map(|s| &s.audio_drivers)
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[shoop_test]
fn audio_driver_switch_preserves_identities_and_persists_on_both_carriers() {
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host(transport);
        let (mut client, mut connection) = attach(&host.entry, 951);
        driver_snapshot(&mut client, &mut connection, |s| {
            s.audio_drivers.catalog.len() == 3
        });
        let audio_source = shoop_app_api::profiling_data::wire::SOURCE;
        let deadline = Instant::now() + Duration::from_secs(15);
        let previous_stream = loop {
            connection.poll(&mut client).unwrap();
            if let Some(source) = client
                .diagnostics()
                .sources
                .iter()
                .find(|source| source.descriptor.stream.source == audio_source)
            {
                break source.descriptor.stream;
            }
            assert!(
                Instant::now() < deadline,
                "audio profiling discovery timed out"
            );
            thread::sleep(Duration::from_millis(2));
        };
        let original = client.snapshot().unwrap().clone();
        let target = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 32_000,
            buffer_size: 128,
        });
        assert!(matches!(
            command(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Request(
                    target.clone()
                ))
            ),
            Outcome::Confirmed { .. }
        ));
        let state = client.snapshot().unwrap();
        assert_eq!(
            state.audio_drivers.switch.status,
            AudioDriverSwitchStatus::AwaitingConfirmation
        );
        assert_eq!(state.status.sample_rate, 48_000);
        let request_id = state.audio_drivers.switch.request_id;
        assert!(matches!(
            command(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Confirm {
                    request_id,
                    accept: true
                })
            ),
            Outcome::Confirmed { .. }
        ));
        driver_snapshot(&mut client, &mut connection, |s| {
            s.audio_drivers.switch.status == AudioDriverSwitchStatus::Completed
        });
        let switched = client.snapshot().unwrap();
        assert_eq!(switched.status.sample_rate, 32_000);
        assert_eq!(
            switched.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
            original.tracks.iter().map(|t| t.id).collect::<Vec<_>>()
        );
        assert_eq!(
            switched.buses.iter().map(|b| b.id).collect::<Vec<_>>(),
            original.buses.iter().map(|b| b.id).collect::<Vec<_>>()
        );
        assert!(switched
            .tracks
            .iter()
            .flat_map(|t| &t.loops)
            .all(|l| l.mode == LoopMode::Stopped));
        assert_eq!(
            shoop_host::audio_driver::read_preferences(
                &host.root.path().join("config.audio-driver.json")
            )
            .unwrap(),
            Some(target)
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        let replacement_stream = loop {
            connection.poll(&mut client).unwrap();
            if let Some(source) = client.diagnostics().sources.iter().find(|source| {
                source.descriptor.stream.source == audio_source
                    && source.descriptor.stream.epoch > previous_stream.epoch
            }) {
                break source.descriptor.stream;
            }
            assert!(
                Instant::now() < deadline,
                "replacement profiling epoch timed out"
            );
            thread::sleep(Duration::from_millis(2));
        };
        for (epoch, accepted) in [
            (previous_stream.epoch, false),
            (replacement_stream.epoch, true),
        ] {
            let outcome = command(
                &mut client,
                &mut connection,
                Command::Diagnostic(diagnostics_protocol::Request {
                    source: audio_source,
                    epoch,
                    action: diagnostics_protocol::Action::Snapshot,
                }),
            );
            assert_eq!(matches!(outcome, Outcome::Confirmed { .. }), accepted);
        }
        drop(connection);
        shutdown(&mut host, &mut client);
        let restarted_profile = host.root.path().join("restarted.json");
        host.child = Process::new(env!("CARGO_BIN_EXE_shoop_host"))
            .arg("--config")
            .arg(host.root.path().join("config.json"))
            .arg("--export-profile")
            .arg(&restarted_profile)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while !restarted_profile.exists() {
            assert!(host.child.try_wait().unwrap().is_none());
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        host.entry = HostEntry::read(&restarted_profile).unwrap();
        let (mut client, connection) = attach(&host.entry, 953);
        assert_eq!(client.snapshot().unwrap().status.sample_rate, 32_000);
        drop(connection);
        shutdown(&mut host, &mut client);
    }
}

#[shoop_test]
#[cfg(all(feature = "jack", feature = "cpal"))]
#[ignore = "requires an isolated JACK server with CPAL's JACK host available"]
fn native_driver_switching_device_acceptance_on_both_carriers() {
    assert!(std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_none());
    for transport in [AppTransport::Socket, AppTransport::SharedMemory] {
        let mut host = host(transport);
        let (mut client, mut connection) = attach(&host.entry, 952);
        driver_snapshot(&mut client, &mut connection, |s| {
            s.audio_drivers.catalog.len() == 3
        });
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::Global(GlobalControlAction::SetSync(false)),
        );
        let Outcome::TrackCreated { track, loops, .. } = command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::AddTrack(TrackSpec {
                name: "Driver survivor".into(),
                topology: TrackSpecTopology::Direct {
                    audio_channels: 1,
                    midi: true,
                },
                latency: Default::default(),
                initial_output_bus_names: vec!["Master".into()],
                creation_request_id: None,
            }),
        ) else {
            panic!("could not create survivor track")
        };
        let loop_id = loops[0];
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::Loop {
                track,
                loop_id,
                action: LoopAction::RecordClicked,
            },
        );
        driver_snapshot(&mut client, &mut connection, |s| {
            s.tracks[1].loops[0].mode == LoopMode::Recording
        });
        thread::sleep(Duration::from_millis(75));
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::Loop {
                track,
                loop_id,
                action: LoopAction::StopClicked,
            },
        );
        driver_snapshot(&mut client, &mut connection, |s| {
            s.tracks[1].loops[0].mode == LoopMode::Stopped && s.tracks[1].loops[0].length_frames > 0
        });
        let read = |client: &mut Client, connection: &mut Connection| {
            connection
                .send(client.read_media(loop_id).unwrap())
                .unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            while client.media().is_none() {
                connection.poll(client).unwrap();
                assert!(Instant::now() < deadline, "switched loop media timed out");
                thread::sleep(Duration::from_millis(2));
            }
            client.media().unwrap().as_ref().unwrap().clone()
        };
        let mut previous_media = read(&mut client, &mut connection);
        assert!(previous_media.audio[0].samples.iter().any(|s| *s == 0.25));
        let mut previous_rate = 48_000;
        let original = client.snapshot().unwrap().clone();
        let defaults = vec![
            AudioDriverConfig::Jack(JackAudioDriverConfig {
                client_name: "P14-direct".into(),
            }),
            AudioDriverConfig::Cpal(CpalAudioDriverConfig {
                host: "jack".into(),
                client_name: "P14-cpal".into(),
                sample_rate: 48_000,
                midi_inputs: Vec::new(),
                midi_outputs: Vec::new(),
                ..Default::default()
            }),
            AudioDriverConfig::Cpal(CpalAudioDriverConfig {
                host: "jack".into(),
                client_name: "P14-cpal-fixed".into(),
                sample_rate: 48_000,
                buffer_size: 256,
                midi_inputs: Vec::new(),
                midi_outputs: Vec::new(),
                ..Default::default()
            }),
            AudioDriverConfig::Jack(JackAudioDriverConfig {
                client_name: "P14-return".into(),
            }),
            AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 48_000,
                buffer_size: 128,
            }),
        ];
        let targets = std::env::var_os("SHOOP_P14_DRIVER_CONFIGS")
            .map(|path| {
                let bytes = std::fs::read(path).unwrap();
                assert!(bytes.len() <= 65_536);
                let targets: Vec<AudioDriverConfig> = serde_json::from_slice(&bytes).unwrap();
                assert!(
                    !targets.is_empty()
                        && targets.len() <= 16
                        && targets.iter().all(AudioDriverConfig::bounded)
                );
                targets
            })
            .unwrap_or(defaults);
        for target in targets {
            let outcome = command_with_timeout(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Request(
                    target.clone(),
                )),
                Duration::from_secs(30),
            );
            assert!(
                matches!(outcome, Outcome::Confirmed { .. }),
                "preflight {target:?}: {outcome:?}"
            );
            let negotiated = client
                .snapshot()
                .unwrap()
                .audio_drivers
                .switch
                .target
                .as_ref()
                .unwrap()
                .configured
                .clone();
            let request_id = client.snapshot().unwrap().audio_drivers.switch.request_id;
            let outcome = command_with_timeout(
                &mut client,
                &mut connection,
                shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Confirm {
                    request_id,
                    accept: true,
                }),
                Duration::from_secs(30),
            );
            assert!(
                matches!(outcome, Outcome::Confirmed { .. }),
                "switch {target:?}: {outcome:?}"
            );
            driver_snapshot(&mut client, &mut connection, |s| {
                s.audio_drivers.switch.status == AudioDriverSwitchStatus::Completed
            });
            let state = client.snapshot().unwrap();
            assert_eq!(
                state.audio_drivers.active.as_ref().unwrap().configured,
                negotiated
            );
            let rate = state.status.sample_rate;
            assert_eq!(
                state.tracks.iter().map(|t| t.id).collect::<Vec<_>>(),
                original.tracks.iter().map(|t| t.id).collect::<Vec<_>>()
            );
            assert_eq!(
                state.buses.iter().map(|b| b.id).collect::<Vec<_>>(),
                original.buses.iter().map(|b| b.id).collect::<Vec<_>>()
            );
            assert!(state
                .tracks
                .iter()
                .flat_map(|t| &t.loops)
                .all(|l| l.mode == LoopMode::Stopped));
            let media = read(&mut client, &mut connection);
            if rate == previous_rate {
                assert_eq!(media.audio[0].samples, previous_media.audio[0].samples);
                assert_eq!(media.midi[0].events, previous_media.midi[0].events);
            }
            assert!(media.audio[0]
                .samples
                .iter()
                .any(|s| (*s - 0.25).abs() < 1e-5));
            previous_media = media;
            previous_rate = rate;
        }
        drop(connection);
        shutdown(&mut host, &mut client);
    }
}

#[shoop_test]
#[cfg(feature = "cpal")]
fn startup_fallback_retains_the_effective_dummy_profile_for_later_switches() {
    let unavailable = AudioDriverConfig::Cpal(CpalAudioDriverConfig {
        output_device: "none".into(),
        midi_inputs: vec![],
        midi_outputs: vec![],
        ..Default::default()
    });
    let mut host = host_with_driver(AppTransport::Socket, 16, Some(unavailable.clone()));
    let (mut client, mut connection) = attach(&host.entry, 954);
    driver_snapshot(&mut client, &mut connection, |s| {
        s.audio_drivers.catalog.len() == 3
    });
    let effective = AudioDriverConfig::default();
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .configured,
        effective
    );
    assert!(matches!(
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Request(unavailable))
        ),
        Outcome::Refused { .. }
    ));
    assert_eq!(
        client
            .snapshot()
            .unwrap()
            .audio_drivers
            .active
            .as_ref()
            .unwrap()
            .configured,
        effective
    );
    assert!(!host.root.path().join("config.audio-driver.json").exists());
    assert!(matches!(
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Request(
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128
                })
            ))
        ),
        Outcome::Confirmed { .. }
    ));
    let state = client.snapshot().unwrap();
    assert_eq!(
        state
            .audio_drivers
            .switch
            .source
            .as_ref()
            .unwrap()
            .configured,
        effective
    );
    let request_id = state.audio_drivers.switch.request_id;
    assert!(matches!(
        command(
            &mut client,
            &mut connection,
            shoop_app_protocol::Command::AudioDriver(audio_driver::Command::Confirm {
                request_id,
                accept: true
            })
        ),
        Outcome::Confirmed { .. }
    ));
    assert_eq!(client.snapshot().unwrap().status.sample_rate, 32_000);
    drop(connection);
    shutdown(&mut host, &mut client);
}
