#![cfg(not(target_arch = "wasm32"))]
use shoop_app_client::{Client, State};
use shoop_app_protocol::*;
use shoop_host::{
    connection::{self, Connection},
    remote::{create_credential, RemoteProfile},
};
use shoop_wasm_test_support::shoop_test;
use std::{
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Host {
    child: Child,
    _root: tempfile::TempDir,
    profile: RemoteProfile,
    path: PathBuf,
}
impl Drop for Host {
    fn drop(&mut self) {
        if self.child.try_wait().unwrap().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
fn host(ip: IpAddr) -> Host {
    let root = tempfile::tempdir().unwrap();
    let config = root.path().join("config.json");
    std::fs::write(&config, b"{}").unwrap();
    let key = root.path().join("host.key");
    create_credential(&key).unwrap();
    let path = root.path().join("profile.json");
    let mut child = Command::new(env!("CARGO_BIN_EXE_shoop_host"))
        .arg("--config")
        .arg(config)
        .arg("--registry")
        .arg(root.path().join("registry"))
        .arg("--lan")
        .arg("--endpoint")
        .arg(SocketAddr::new(ip, 0).to_string())
        .arg("--management-endpoint")
        .arg(SocketAddr::new(ip, 0).to_string())
        .arg("--credential-file")
        .arg(key)
        .arg("--export-profile")
        .arg(&path)
        .args(["--host-id", "100", "--session-id", "200"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !path.exists() {
        assert!(
            child.try_wait().unwrap().is_none(),
            "LAN host failed before readiness"
        );
        assert!(
            Instant::now() < deadline,
            "LAN host did not publish its profile"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let profile = RemoteProfile::read(&path).unwrap();
    Host {
        child,
        _root: root,
        profile,
        path,
    }
}
fn attach(profile: &RemoteProfile, id: u128) -> (Client, Connection) {
    let mut connection = connection::connect(profile, Duration::from_secs(5)).unwrap();
    let mut client = Client::new(ClientId(id), profile.host, profile.session).unwrap();
    connection.send(client.attach().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while client.state() != State::Ready {
        connection.poll(&mut client).unwrap();
        if let Some(message) = client.ready_message() {
            connection.send(message).unwrap();
            client.ready_sent().unwrap();
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    (client, connection)
}
fn control(
    client: &mut Client,
    connection: &mut Connection,
    command: shoop_app_protocol::Command,
) -> Outcome {
    let message = client.command(command, None).unwrap();
    let ClientMessage::Submit { operation, .. } = message else {
        panic!()
    };
    connection.send(message).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !client.results().contains_key(&operation) {
        connection.poll(client).unwrap();
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    assert!(matches!(
        client.results()[&operation],
        Outcome::Confirmed { .. } | Outcome::TrackCreated { .. }
    ));
    client.results()[&operation].clone()
}
fn status(profile: &RemoteProfile) -> ManagementReply {
    connection::management(
        profile,
        ManagementRequest::Status {
            version: VERSION,
            host: profile.host,
            session: profile.session,
        },
        Duration::from_secs(5),
    )
    .unwrap()
}

#[shoop_test]
fn lan_profile_dns_identity_refusal_and_replacement_use_both_endpoints() {
    let host = host(Ipv4Addr::LOCALHOST.into());
    let mut profile = host.profile.clone();
    profile.endpoint = format!(
        "localhost:{}",
        profile.endpoint.parse::<SocketAddr>().unwrap().port()
    );
    profile.management = format!(
        "localhost:{}",
        profile.management.parse::<SocketAddr>().unwrap().port()
    );
    let serialized = std::fs::read_to_string(&host.path).unwrap();
    assert!(!serialized.contains("process"));
    assert!(!serialized.contains("registry"));
    let (mut client, mut connection) = attach(&profile, 1);
    let old = client.authority().unwrap();
    control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::SetLoopSmoothingMs(9),
    );
    use std::sync::atomic::{AtomicBool, Ordering};
    struct StopPump<'a>(&'a AtomicBool);
    impl Drop for StopPump<'_> {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    let stop = AtomicBool::new(false);
    thread::scope(|scope| {
        let _stop = StopPump(&stop);
        scope.spawn(|| {
            while !stop.load(Ordering::Acquire) {
                connection.poll(&mut client).unwrap();
                thread::sleep(Duration::from_millis(2));
            }
        });
        let mut wrong = profile.clone();
        wrong.host = HostId(101);
        assert!(connection::connect(&wrong, Duration::from_secs(5)).is_err());
        assert!(connection::management(
            &wrong,
            ManagementRequest::Status {
                version: VERSION,
                host: wrong.host,
                session: wrong.session
            },
            Duration::from_secs(5)
        )
        .is_err());
        let mut wrong = profile.clone();
        wrong.version = VERSION + 1;
        assert!(connection::connect(&wrong, Duration::from_secs(5)).is_err());
        let bad_key = host._root.path().join("wrong.key");
        create_credential(&bad_key).unwrap();
        let mut wrong = profile.clone();
        wrong.credential = bad_key;
        assert!(connection::connect(&wrong, Duration::from_secs(2)).is_err());
        assert!(matches!(
            status(&profile),
            ManagementReply::Status { busy: true, .. }
        ));
        let mut busy = connection::connect(&profile, Duration::from_secs(5)).unwrap();
        let mut other = Client::new(ClientId(2), profile.host, profile.session).unwrap();
        busy.send(other.attach().unwrap()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while other.state() != State::Lost {
            let _ = busy.poll(&mut other);
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(other.refusal(), Some(Refusal::Busy));
    });
    drop(connection);
    thread::sleep(Duration::from_millis(100));
    let (mut replacement, mut connection) = attach(&profile, 3);
    assert!(replacement.authority().unwrap().generation > old.generation);
    assert!(matches!(
        connection::management(
            &profile,
            ManagementRequest::Shutdown {
                authority: old,
                operation: OperationId {
                    client: old.client,
                    sequence: 2
                }
            },
            Duration::from_secs(5)
        )
        .unwrap(),
        ManagementReply::Refused(Refusal::StaleAuthority)
    ));
    control(
        &mut replacement,
        &mut connection,
        shoop_app_protocol::Command::SetLoopSmoothingMs(4),
    );
}

#[shoop_test]
fn ipv6_profiles_connect_and_ownerless_shutdown_is_confirmed() {
    let mut host = host(Ipv6Addr::LOCALHOST.into());
    let (mut client, mut connection) = attach(&host.profile, 1);
    control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::SetLoopSmoothingMs(8),
    );
    drop(connection);
    thread::sleep(Duration::from_millis(100));
    connection::shutdown_ownerless(&host.profile, Duration::from_secs(10)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
#[shoop_test]
fn non_loopback_interface_is_supported_only_by_explicit_lan_policy() {
    let addresses = Command::new("hostname").arg("-I").output().unwrap();
    let ip = String::from_utf8(addresses.stdout)
        .unwrap()
        .split_whitespace()
        .filter_map(|s| s.parse::<Ipv4Addr>().ok())
        .find(|ip| !ip.is_loopback())
        .expect("Linux LAN acceptance requires a non-loopback IPv4 interface");
    assert!(shoop_transport_socket::setup::Listener::bind_with(
        SocketAddr::new(ip.into(), 0),
        [1; 32]
    )
    .is_err());
    let host = host(ip.into());
    let (mut client, mut connection) = attach(&host.profile, 1);
    control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::SetLoopSmoothingMs(5),
    );
    assert!(!host
        .profile
        .endpoint
        .parse::<SocketAddr>()
        .unwrap()
        .ip()
        .is_loopback());
}

#[shoop_test]
fn wildcard_profiles_and_remote_shared_memory_are_refused() {
    let host = host(Ipv4Addr::LOCALHOST.into());
    let mut profile = host.profile.clone();
    profile.endpoint = "0.0.0.0:1234".into();
    assert!(profile.validate().is_err());
    let mut value = serde_json::to_value(&host.profile).unwrap();
    value["transport"] = serde_json::json!("shared_memory");
    assert!(serde_json::from_value::<RemoteProfile>(value).is_err());
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = occupied.local_addr().unwrap();
    assert!(shoop_transport_socket::setup::Listener::bind_lan(address, [1; 32]).is_err());
}

struct Blackhole {
    address: SocketAddr,
    blocked: std::sync::Arc<std::sync::atomic::AtomicBool>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    sockets: std::sync::Arc<std::sync::Mutex<Vec<std::net::TcpStream>>>,
    join: Option<std::thread::JoinHandle<()>>,
}
impl Blackhole {
    fn new(target: SocketAddr) -> Self {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc, Mutex,
        };
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let blocked = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let (b, s, held) = (blocked.clone(), stop.clone(), sockets.clone());
        let join = thread::spawn(move || {
            let incoming = loop {
                if s.load(Ordering::Acquire) {
                    return;
                }
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => return,
                }
            };
            let outgoing =
                std::net::TcpStream::connect_timeout(&target, Duration::from_secs(5)).unwrap();
            held.lock()
                .unwrap()
                .extend([incoming.try_clone().unwrap(), outgoing.try_clone().unwrap()]);
            let forward = |mut reader: std::net::TcpStream,
                           mut writer: std::net::TcpStream,
                           b: Arc<AtomicBool>,
                           s: Arc<AtomicBool>| {
                thread::spawn(move || {
                    use std::io::{Read, Write};
                    reader
                        .set_read_timeout(Some(Duration::from_millis(50)))
                        .unwrap();
                    writer
                        .set_write_timeout(Some(Duration::from_millis(100)))
                        .unwrap();
                    let mut bytes = [0; 4096];
                    while !s.load(Ordering::Acquire) {
                        match reader.read(&mut bytes) {
                            Ok(0) => break,
                            Ok(n) => {
                                if !b.load(Ordering::Acquire)
                                    && writer.write_all(&bytes[..n]).is_err()
                                {
                                    break;
                                }
                            }
                            Err(error)
                                if matches!(
                                    error.kind(),
                                    std::io::ErrorKind::WouldBlock
                                        | std::io::ErrorKind::TimedOut
                                        | std::io::ErrorKind::Interrupted
                                ) => {}
                            Err(_) => break,
                        }
                    }
                })
            };
            let a = forward(
                incoming.try_clone().unwrap(),
                outgoing.try_clone().unwrap(),
                b.clone(),
                s.clone(),
            );
            let c = forward(outgoing, incoming, b, s);
            a.join().unwrap();
            c.join().unwrap();
        });
        Self {
            address,
            blocked,
            stop,
            sockets,
            join: Some(join),
        }
    }
}
impl Drop for Blackhole {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        self.stop.store(true, Ordering::Release);
        for socket in self.sockets.lock().unwrap().iter() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        self.join.take().unwrap().join().unwrap();
    }
}

#[shoop_test]
fn silent_network_loss_releases_the_controller_while_recording_continues() {
    let host = host(Ipv4Addr::LOCALHOST.into());
    let relay = Blackhole::new(host.profile.endpoint.parse().unwrap());
    let mut profile = host.profile.clone();
    profile.endpoint = relay.address.to_string();
    let (mut client, mut connection) = attach(&profile, 9);
    control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Global(shoop_app_api::GlobalControlAction::SetSync(false)),
    );
    let outcome = control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::AddTrack(shoop_app_api::TrackSpec {
            name: "Blackhole recording".into(),
            topology: shoop_app_api::TrackSpecTopology::Direct {
                audio_channels: 1,
                midi: false,
            },
            latency: Default::default(),
            initial_output_bus_names: vec!["Master".into()],
            creation_request_id: None,
        }),
    );
    let Outcome::TrackCreated { track, loops, .. } = outcome else {
        panic!()
    };
    let loop_id = loops[0];
    control(
        &mut client,
        &mut connection,
        shoop_app_protocol::Command::Loop {
            track,
            loop_id,
            action: shoop_app_api::LoopAction::RecordClicked,
        },
    );
    let owner = client.authority().unwrap();
    relay
        .blocked
        .store(true, std::sync::atomic::Ordering::Release);
    thread::sleep(Duration::from_secs(7));
    let (mut replacement, mut direct) = attach(&host.profile, 10);
    let state = replacement.snapshot().unwrap();
    let preserved = state
        .tracks
        .iter()
        .find(|candidate| candidate.id == track)
        .unwrap();
    assert_eq!(preserved.loops[0].mode, shoop_app_api::LoopMode::Recording);
    assert!(preserved.loops[0].length_frames > 48000 * 5);
    assert!(replacement.authority().unwrap().generation > owner.generation);
    assert!(connection.poll(&mut client).is_err());
    assert_eq!(client.state(), State::Lost);
    assert!(matches!(
        connection::management(
            &host.profile,
            ManagementRequest::Shutdown {
                authority: owner,
                operation: OperationId {
                    client: owner.client,
                    sequence: 100
                }
            },
            Duration::from_secs(5)
        )
        .unwrap(),
        ManagementReply::Refused(Refusal::StaleAuthority)
    ));
    control(
        &mut replacement,
        &mut direct,
        shoop_app_protocol::Command::SetLoopSmoothingMs(7),
    );
}

#[shoop_test]
fn losing_a_management_reply_reconciles_the_same_shutdown_operation() {
    let mut host = host(Ipv4Addr::LOCALHOST.into());
    let (mut client, connection) = attach(&host.profile, 11);
    let ticket = client.shutdown().unwrap();
    let ClientMessage::Shutdown {
        authority,
        operation,
    } = ticket
    else {
        panic!()
    };
    let target: shoop_host::remote::Target = host.profile.clone().into();
    let token = target.token().unwrap();
    let mut raw = shoop_transport_socket::setup::connect_lan(
        host.profile.management.parse().unwrap(),
        &token,
        Default::default(),
        Duration::from_secs(5),
    )
    .unwrap();
    use shoop_transport::RecordPort;
    let mut bytes = [0; 1280];
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(n) = raw.take(&mut bytes) {
            assert_eq!(
                codec::decode(&bytes[..n]).unwrap(),
                WireMessage::CarrierReady {
                    host: host.profile.host,
                    session: host.profile.session
                }
            );
            break;
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(2));
    }
    let n = codec::encode(
        &WireMessage::ManagementRequest(ManagementRequest::Shutdown {
            authority,
            operation,
        }),
        &mut bytes,
    )
    .unwrap();
    raw.post(&bytes[..n]).unwrap();
    while !raw.outgoing_empty() {
        raw.flush().unwrap();
        assert!(Instant::now() < deadline);
    }
    drop(raw);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match connection::management(
            &host.profile,
            ManagementRequest::Result {
                authority,
                operation,
            },
            Duration::from_secs(2),
        )
        .unwrap()
        {
            ManagementReply::Finished { result, .. } => {
                assert_eq!(result.id, operation);
                assert!(matches!(result.outcome, Outcome::Confirmed { .. }));
                break;
            }
            ManagementReply::Accepted {
                operation: pending, ..
            } => assert_eq!(pending.id, operation),
            reply => panic!("unexpected reconciliation: {reply:?}"),
        }
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
    drop(connection);
    while host.child.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        thread::sleep(Duration::from_millis(5));
    }
}

#[shoop_test]
fn exported_profiles_become_visible_complete_and_never_overwrite_an_existing_profile() {
    let host = host(Ipv4Addr::LOCALHOST.into());
    for attempt in 0..16 {
        let path = host._root.path().join(format!("published-{attempt}.json"));
        let profile = host.profile.clone();
        let destination = path.clone();
        let writer = thread::spawn(move || profile.write(&destination).unwrap());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let profile: RemoteProfile = serde_json::from_slice(&bytes)
                        .expect("published profile must already contain complete JSON");
                    profile.validate().unwrap();
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("profile publication failed: {error}"),
            }
            assert!(Instant::now() < deadline);
            thread::yield_now();
        }
        writer.join().unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(host.profile.write(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let mut invalid = host.profile.clone();
    invalid.warnings = vec!["x".repeat(1024); 65];
    assert!(invalid.validate().is_err());
}
