use shoop_app_protocol::*;
use shoop_transport::MAX_RECORD_BYTES;
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[shoop_test]
fn restart_management_roundtrips_with_bounded_records_and_no_session_lease() {
    let operation = OperationId {
        client: ClientId(789),
        sequence: 123,
    };
    for message in [
        WireMessage::ManagementRequest(ManagementRequest::RestartBackend {
            version: VERSION,
            operation,
            expected_generation: 7,
        }),
        WireMessage::ManagementRequest(ManagementRequest::BackendRestartStatus {
            version: VERSION,
        }),
        WireMessage::ManagementReply(ManagementReply::BackendRestart(BackendRestartStatus {
            generation: 8,
            operation: Some(operation),
            phase: BackendRestartPhase::Failed,
            failure: Some(BackendRestartFailure::WorkerDidNotStop),
        })),
    ] {
        let mut bytes = [0; MAX_RECORD_BYTES];
        let size = codec::encode(&message, &mut bytes).unwrap();
        assert!(size < 128);
        assert_eq!(codec::decode(&bytes[..size]), Ok(message));
        for end in 0..size {
            assert!(codec::decode(&bytes[..end]).is_err());
        }
    }
    assert!(!ManagementRequest::RestartBackend {
        version: VERSION,
        operation,
        expected_generation: 0
    }
    .valid());
    assert!(!ManagementRequest::RestartBackend {
        version: VERSION,
        operation: OperationId {
            client: ClientId(0),
            sequence: 1
        },
        expected_generation: 1
    }
    .valid());
}
