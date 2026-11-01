#![cfg(not(target_arch = "wasm32"))]
use shoop_rt_protocol::{codec::CodecError, MAX_RECORD_BYTES};
use shoop_rt_transport::{carrier::*, contracts::*};
use shoop_rt_transport_socket::{setup, Config, Port};
use shoop_wasm_test_support::shoop_test;
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[shoop_test(no_wasm = "native subprocess fixture")]
fn socket_peer() {
    let Ok(address) = std::env::var("SHOOP_SOCKET_TEST_ADDRESS") else {
        return;
    };
    let mut token = [0; 32];
    std::io::stdin().read_exact(&mut token).unwrap();
    let port = setup::connect(
        address.parse().unwrap(),
        &token,
        Config::default(),
        Duration::from_secs(5),
    )
    .unwrap();
    let mut server = RecordServer::new(ControlledEndpoint(TestEndpoint::default()), port, outbox());
    let deadline = Instant::now() + Duration::from_secs(30);
    while server.failure().is_none() && Instant::now() < deadline {
        server.pump(32);
        std::thread::yield_now();
    }
    assert!(server.failure().is_some());
}
#[shoop_test(no_wasm = "native sockets and process")]
fn subprocess_socket_uses_the_shared_lifecycle_credits_and_bulk_contract() {
    let listener = setup::Listener::bind().unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "socket_peer", "--nocapture"])
        .env(
            "SHOOP_SOCKET_TEST_ADDRESS",
            listener.address().unwrap().to_string(),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(listener.token())
        .unwrap();
    let port = listener
        .accept(Config::default(), Duration::from_secs(5))
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        lifecycle(RemoteCarrier::new(port))
    }));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("fixture did not exit")
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    if let Err(error) = result {
        std::panic::resume_unwind(error)
    }
}
fn streams() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let a = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let b = listener.accept().unwrap().0;
    (a, b)
}
#[shoop_test(no_wasm = "native socket authentication")]
fn authentication_uses_the_remaining_launch_deadline() {
    let listener = setup::Listener::bind().unwrap();
    let address = listener.address().unwrap();
    let token = *listener.token();
    let server =
        std::thread::spawn(move || listener.accept(Config::default(), Duration::from_secs(2)));
    let mut peer = TcpStream::connect(address).unwrap();
    std::thread::sleep(Duration::from_millis(250));
    peer.write_all(&token).unwrap();
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut ack = [0];
    let authenticated = peer.read_exact(&mut ack);
    let accepted = server.join().unwrap();
    assert!(accepted.is_ok(), "{authenticated:?}");
    assert!(authenticated.is_ok());
    assert_eq!(ack, [1]);
}
#[shoop_test(no_wasm = "native sockets")]
fn fragmented_and_coalesced_records_and_eof_are_distinct_from_idle() {
    let (mut sender, receiver) = streams();
    let mut port = Port::from_stream(receiver, Config::default()).unwrap();
    let mut buf = [0; MAX_RECORD_BYTES];
    assert_eq!(port.take(&mut buf), None);
    assert_eq!(port.failure(), None);
    sender.write_all(&[5]).unwrap();
    assert_eq!(port.take(&mut buf), None);
    sender.write_all(&[0, 1, 2, 3, 3, 0, 9]).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    for expected in [&[5, 0, 1, 2, 3][..], &[3, 0, 9][..]] {
        loop {
            if let Some(n) = port.take(&mut buf) {
                assert_eq!(&buf[..n], expected);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
    drop(sender);
    while port.failure().is_none() {
        port.take(&mut buf);
        assert!(Instant::now() < deadline);
    }
    assert_eq!(port.failure(), Some(CarrierError::Disconnected));
}
#[shoop_test(no_wasm = "native sockets")]
fn malformed_lengths_end_the_carrier() {
    for header in [[0, 0], [1, 0], (MAX_RECORD_BYTES as u16 + 1).to_le_bytes()] {
        let (mut sender, receiver) = streams();
        let mut port = Port::from_stream(receiver, Config::default()).unwrap();
        sender.write_all(&header).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while port.failure().is_none() {
            port.take(&mut [0; MAX_RECORD_BYTES]);
            assert!(Instant::now() < deadline);
        }
        assert_eq!(
            port.failure(),
            Some(CarrierError::Codec(CodecError::Invalid))
        );
    }
}
#[shoop_test(no_wasm = "native socket authentication")]
fn an_unauthenticated_peer_cannot_establish_the_connection() {
    let listener = setup::Listener::bind().unwrap();
    let address = listener.address().unwrap();
    let client = std::thread::spawn(move || {
        setup::connect(
            address,
            &[0; 32],
            Config::default(),
            Duration::from_millis(100),
        )
        .is_err()
    });
    assert!(listener
        .accept(Config::default(), Duration::from_millis(200))
        .is_err());
    assert!(client.join().unwrap());
}
#[shoop_test(no_wasm = "native socket backpressure")]
fn full_output_retries_preserve_whole_records_in_order() {
    let (a, b) = streams();
    let config = Config {
        reliable: 2,
        telemetry: 1,
        bulk: 1,
    };
    let mut sender = Port::from_stream(a, config).unwrap();
    let mut receiver = Port::from_stream(b, config).unwrap();
    let mut record = [0; MAX_RECORD_BYTES];
    record[..2].copy_from_slice(&(MAX_RECORD_BYTES as u16).to_le_bytes());
    let mut accepted = 0u64;
    loop {
        record[2..10].copy_from_slice(&accepted.to_le_bytes());
        match sender.post(&record) {
            Ok(()) => accepted += 1,
            Err(CarrierError::Full) => break,
            Err(e) => panic!("{e:?}"),
        }
        assert!(accepted < 100000);
    }
    assert!(sender.pending() <= 2);
    let retry_record = record;
    let mut received_record = [0; MAX_RECORD_BYTES];
    let mut received = 0u64;
    let mut retried = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while received < accepted || !retried {
        let _ = sender.flush();
        if !retried {
            if sender.post(&retry_record) == Ok(()) {
                accepted += 1;
                retried = true;
            }
        }
        if let Some(n) = receiver.take(&mut received_record) {
            assert_eq!(n, MAX_RECORD_BYTES);
            assert_eq!(
                u64::from_le_bytes(received_record[2..10].try_into().unwrap()),
                received
            );
            received += 1;
        }
        assert!(Instant::now() < deadline);
    }
}
