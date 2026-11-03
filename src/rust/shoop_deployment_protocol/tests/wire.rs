use shoop_deployment_protocol::*;
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn frozen_wire_and_strict_validation() {
    let mut bytes = [0; MAX_BYTES];
    let n = encode_request(Request::Status, &mut bytes).unwrap();
    assert_eq!(&bytes[..n], b"\x0e\0SHOOPDP\x01\0\0\0\0");
    let request = Request::Enter {
        boot_nonce: 3,
        operation: Operation {
            client: 1,
            sequence: 2,
        },
    };
    let n = encode_request(request, &mut bytes).unwrap();
    assert_eq!(n, 46);
    let mut golden = b"\x2e\0SHOOPDP\x01\x01\0\0\0".to_vec();
    golden.extend_from_slice(&3u64.to_le_bytes());
    golden.extend_from_slice(&1u128.to_le_bytes());
    golden.extend_from_slice(&2u64.to_le_bytes());
    assert_eq!(&bytes[..n], golden);
    assert_eq!(decode_request(&golden), Ok(request));
    golden.push(0);
    assert_eq!(decode_request(&golden), Err(Error::Invalid));
    bytes[9] = 2;
    assert_eq!(decode_request(&bytes[..n]), Err(Error::Version));
    assert!(is_record(&bytes[..n]));
    assert_eq!(
        encode_request(
            Request::Enter {
                boot_nonce: 0,
                operation: Operation {
                    client: 1,
                    sequence: 1
                }
            },
            &mut bytes
        ),
        Err(Error::Invalid)
    );
    assert_eq!(
        encode_reply(&Reply::Refused(Refusal::StaleBoot), &mut bytes),
        Ok(18)
    );
    assert_eq!(&bytes[..18], b"\x12\0SHOOPDP\x01\x03\0\0\0\x04\0\0\0");
}

#[shoop_test]
fn bounded_status_and_truncation() {
    let reply = Reply::Info(Info {
        profile: "shoop-zynq7000-1g-v1".into(),
        build: "firmware-a".into(),
        hash: [7; 32],
        slot: Some(0),
        confirmed: true,
        trial: false,
        boot_nonce: 15,
        last_operation: None,
        session_version: 65535,
        busy: false,
        ready: true,
    });
    let mut bytes = [0; MAX_BYTES];
    let n = encode_reply(&reply, &mut bytes).unwrap();
    assert_eq!(decode_reply(&bytes[..n]), Ok(reply));
    for end in 0..n {
        assert!(decode_reply(&bytes[..end]).is_err());
    }
    let mut huge = b"\x0e\0SHOOPDP\x01\0\0\0\0".to_vec();
    huge.extend_from_slice(&u64::MAX.to_le_bytes());
    assert!(decode_reply(&huge).is_err());
}
