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
fn control(client: &mut Client, connection: &mut Connection, command: shoop_app_protocol::Command) {
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
        Outcome::Confirmed { .. }
    ));
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
    connection.poll(&mut client).unwrap();
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
