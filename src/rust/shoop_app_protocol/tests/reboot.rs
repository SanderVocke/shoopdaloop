use shoop_app_protocol::{codec, ManagementReply, ManagementRequest, WireMessage, VERSION};
use shoop_transport::MAX_RECORD_BYTES;
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn management_reboot_request_and_acknowledgment_roundtrip_with_bounded_records() {
    for message in [
        WireMessage::ManagementRequest(ManagementRequest::Reboot { version: VERSION }),
        WireMessage::ManagementReply(ManagementReply::Rebooting { version: VERSION }),
    ] {
        let mut bytes = [0; MAX_RECORD_BYTES];
        let size = codec::encode(&message, &mut bytes).unwrap();
        assert!(size < 32);
        assert_eq!(codec::decode(&bytes[..size]), Ok(message));
        for length in 0..size {
            assert!(codec::decode(&bytes[..length]).is_err());
        }
        let mut trailing = bytes[..size].to_vec();
        trailing.push(0);
        assert!(codec::decode(&trailing).is_err());
    }
}
