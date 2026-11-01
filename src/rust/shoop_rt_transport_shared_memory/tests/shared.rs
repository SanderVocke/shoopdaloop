#![cfg(not(target_arch = "wasm32"))]
use shoop_rt_protocol::{Lane, MAX_RECORD_BYTES};
use shoop_rt_transport::{carrier::*, contracts::*};
use shoop_rt_transport_shared_memory::{Descriptor, Port, Side, DESCRIPTOR_BYTES};
use shoop_rt_transport_socket::{setup, Config};
use shoop_wasm_test_support::shoop_test;
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
fn frame(seq: u64) -> [u8; MAX_RECORD_BYTES] {
    let mut bytes = [seq as u8; MAX_RECORD_BYTES];
    bytes[..2].copy_from_slice(&(MAX_RECORD_BYTES as u16).to_le_bytes());
    bytes[2..10].copy_from_slice(&seq.to_le_bytes());
    bytes
}
fn receive(port: &mut Port) -> [u8; MAX_RECORD_BYTES] {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(len) = port.take(&mut bytes) {
            assert_eq!(len, MAX_RECORD_BYTES);
            return bytes;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
struct Guard(Child);
impl Guard {
    fn wait(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert!(status.success());
                return;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
#[shoop_test(no_wasm = "shared-memory subprocess fixture")]
fn shared_peer() {
    let Ok(address) = std::env::var("SHOOP_SHM_TEST_ADDRESS") else {
        return;
    };
    let mut input = std::io::stdin().lock();
    let mut token = [0; 32];
    let mut descriptor = [0; DESCRIPTOR_BYTES];
    input.read_exact(&mut token).unwrap();
    input.read_exact(&mut descriptor).unwrap();
    let port = Port::open(Descriptor::decode(&descriptor).unwrap(), Side::Server).unwrap();
    let mut control = setup::connect(
        address.parse().unwrap(),
        &token,
        Config::default(),
        Duration::from_secs(5),
    )
    .unwrap();
    let mut port = port
        .with_liveness(control.stream().try_clone().unwrap())
        .unwrap();
    control.post(&[3, 0, 0]).unwrap();
    if std::env::var("SHOOP_SHM_TEST_MODE").as_deref() == Ok("flood") {
        let deadline = Instant::now() + Duration::from_secs(20);
        for seq in 0..10000 {
            loop {
                match port.post(&frame(seq)) {
                    Ok(()) => break,
                    Err(CarrierError::Full) => {}
                    Err(e) => panic!("{e:?}"),
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        input.read_exact(&mut [0; 1]).unwrap();
    } else {
        let mut server =
            RecordServer::new(ControlledEndpoint(TestEndpoint::default()), port, outbox());
        let deadline = Instant::now() + Duration::from_secs(30);
        while server.failure().is_none() && Instant::now() < deadline {
            server.pump(32);
            std::thread::yield_now();
        }
        assert!(server.failure().is_some());
    }
}
fn peer(mode: &str) -> (Guard, shoop_rt_transport_socket::Port, Port) {
    let descriptor = Descriptor::new([32, 16, 4]).unwrap();
    let port = Port::create(descriptor, Side::Client).unwrap();
    let listener = setup::Listener::bind().unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "shared_peer", "--nocapture"])
        .env(
            "SHOOP_SHM_TEST_ADDRESS",
            listener.address().unwrap().to_string(),
        )
        .env("SHOOP_SHM_TEST_MODE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = Guard(child);
    let input = child.0.stdin.as_mut().unwrap();
    input.write_all(listener.token()).unwrap();
    input.write_all(&descriptor.encode()).unwrap();
    let mut control = listener
        .accept(Config::default(), Duration::from_secs(5))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut buf = [0; MAX_RECORD_BYTES];
    loop {
        if let Some(len) = control.take(&mut buf) {
            assert_eq!(&buf[..len], &[3, 0, 0]);
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let port = port
        .with_liveness(control.stream().try_clone().unwrap())
        .unwrap();
    (child, control, port)
}
#[shoop_test(no_wasm = "actual shared mappings")]
fn descriptor_layouts_are_validated_before_mapping_access() {
    let descriptor = Descriptor::new([2, 2, 2]).unwrap();
    let encoded = descriptor.encode();
    assert_eq!(Descriptor::decode(&encoded).unwrap(), descriptor);
    for index in [0, 4, 6, 36] {
        let mut bad = encoded;
        bad[index] ^= 1;
        assert!(Descriptor::decode(&bad).is_err());
    }
    assert!(Descriptor::decode(&encoded[..DESCRIPTOR_BYTES - 1]).is_err());
    assert!(Descriptor::new([0, 2, 2]).is_err());
    let _owner = Port::create(descriptor, Side::Client).unwrap();
    let mut bad = encoded;
    bad[8..12].copy_from_slice(&1u32.to_le_bytes());
    assert!(Port::open(Descriptor::decode(&bad).unwrap(), Side::Server).is_err());
    fn require_send<T: Send>() {}
    require_send::<Port>();
}
#[shoop_test(no_wasm = "actual shared mappings")]
fn bounded_queue_recycling_matches_fifo_and_latest_value_models() {
    let descriptor = Descriptor::new([2, 2, 2]).unwrap();
    let mut a = Port::create(descriptor, Side::Client).unwrap();
    let mut b = Port::open(descriptor, Side::Server).unwrap();
    for cycle in 0..1000u64 {
        assert_eq!(a.post(&frame(cycle * 3)), Ok(()));
        assert_eq!(a.post(&frame(cycle * 3 + 1)), Ok(()));
        assert_eq!(a.post(&frame(cycle * 3 + 2)), Err(CarrierError::Full));
        assert_eq!(receive(&mut b), frame(cycle * 3));
        assert_eq!(receive(&mut b), frame(cycle * 3 + 1));
        assert_eq!(a.post(&frame(cycle * 3 + 2)), Ok(()));
        assert_eq!(receive(&mut b), frame(cycle * 3 + 2));
    }
    for seq in 10..13 {
        assert_eq!(a.post_on_lane(&frame(seq), Lane::Telemetry), Ok(()));
    }
    assert_eq!(receive(&mut b), frame(11));
    assert_eq!(receive(&mut b), frame(12));
}
#[shoop_test(no_wasm = "subprocess shared memory")]
fn subprocess_shared_memory_uses_the_same_contract_without_socket_record_traffic() {
    let (mut child, mut control, port) = peer("contract");
    lifecycle(RemoteCarrier::new(port));
    assert_eq!(control.take(&mut [0; MAX_RECORD_BYTES]), None);
    drop(control);
    child.wait();
}
#[shoop_test(no_wasm = "concurrent process publication")]
fn concurrent_publication_keeps_every_payload_coherent_and_recycles_samples() {
    let (mut child, control, mut port) = peer("flood");
    for seq in 0..10000 {
        assert_eq!(receive(&mut port), frame(seq));
    }
    child.0.stdin.as_mut().unwrap().write_all(&[1]).unwrap();
    child.wait();
    drop(port);
    drop(control);
}
#[shoop_test(no_wasm = "process kill and mapping cleanup")]
fn peer_death_during_publication_is_terminal_and_dead_resources_are_reclaimed() {
    let (mut child, control, mut port) = peer("flood");
    let _ = receive(&mut port);
    child.0.kill().unwrap();
    let _ = child.0.wait();
    let deadline = Instant::now() + Duration::from_secs(5);
    while port.failure().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(port.failure(), Some(CarrierError::Disconnected));
    assert!(port.cleanup_dead_peer().unwrap() > 0);
    drop(control);
}
