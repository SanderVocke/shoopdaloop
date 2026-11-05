#![cfg(not(target_arch = "wasm32"))]
use shoop_host::deployment::{Bundle, Fastboot, MAX_BUNDLE_BYTES};
use shoop_wasm_test_support::shoop_test;
use std::{
    net::UdpSocket,
    thread,
    time::{Duration, Instant},
};

#[shoop_test]
fn fastboot_retries_identical_packets_and_wraps_sequence() {
    let server = UdpSocket::bind("127.0.0.1:0").unwrap();
    server
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let address = server.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let steps: Vec<(u8, u16, &[u8], &[u8])> = vec![
            (1, 0, b"", &[255, 253]),
            (2, 65533, b"", &[0, 1, 4, 0]),
            (3, 65534, b"download:00000003", b""),
            (3, 65535, b"", b"DATA00000003"),
            (3, 0, b"abc", b""),
            (3, 1, b"", b"OKAY"),
            (3, 2, b"oem board:install", b""),
            (3, 3, b"", b"OKAY"),
        ];
        let mut bytes = [0; 1025];
        for (index, (id, sequence, request, reply)) in steps.into_iter().enumerate() {
            let (n, peer) = server.recv_from(&mut bytes).unwrap();
            assert_eq!(&bytes[..4], &[id, 0, (sequence >> 8) as u8, sequence as u8]);
            assert_eq!(&bytes[4..n], request);
            if index == 3 {
                // The server has committed this command; only its reply was lost.
                let previous = bytes[..n].to_vec();
                let (retry_n, retry_peer) = server.recv_from(&mut bytes).unwrap();
                assert_eq!(retry_peer, peer);
                assert_eq!(&bytes[..retry_n], previous);
            }
            let mut response = vec![id, 0, (sequence >> 8) as u8, sequence as u8];
            response.extend_from_slice(reply);
            // Unrelated sequence numbers must never advance the client.
            let mut unrelated = response.clone();
            unrelated[3] = unrelated[3].wrapping_sub(2);
            server.send_to(&unrelated, peer).unwrap();
            server.send_to(&response, peer).unwrap();
        }
    });
    let mut client = Fastboot::connect(address, Duration::from_secs(5)).unwrap();
    client
        .download(&Bundle {
            bytes: b"abc".to_vec(),
            hash: [0; 32],
        })
        .unwrap();
    client.install().unwrap();
    worker.join().unwrap();
}

#[shoop_test]
fn fastboot_deadline_and_bundle_bound_are_enforced() {
    let blackhole = UdpSocket::bind("127.0.0.1:0").unwrap();
    let start = Instant::now();
    assert!(Fastboot::connect(blackhole.local_addr().unwrap(), Duration::from_millis(40)).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    let file = tempfile::NamedTempFile::new().unwrap();
    assert!(Bundle::read(file.path()).is_err());
    file.as_file()
        .set_len((MAX_BUNDLE_BYTES + 1) as u64)
        .unwrap();
    assert!(Bundle::read(file.path()).is_err());
}
