use shoop_app_api::{LoopAction, LoopId, MidiNote, PianoAction, TrackAction, TrackId};
use shoop_app_protocol::*;
use shoop_transport::{CodecError, MAX_RECORD_BYTES};
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const AUTHORITY: Authority = Authority {
    lease: LeaseId(1),
    client: ClientId(2),
    generation: ConnectionGeneration(3),
};

fn roundtrip(message: WireMessage) {
    let mut buffer = [0; MAX_RECORD_BYTES];
    let size = codec::encode(&message, &mut buffer).unwrap();
    assert_eq!(codec::decode(&buffer[..size]), Ok(message));
    for length in 0..size {
        assert!(
            codec::decode(&buffer[..length]).is_err(),
            "truncation {length}"
        );
    }
    let mut trailing = buffer[..size].to_vec();
    trailing.push(0);
    assert_eq!(codec::decode(&trailing), Err(CodecError::Trailing));
}

#[shoop_test]
fn lifecycle_commands_and_results_roundtrip_and_reject_every_truncation() {
    let operation = OperationId {
        client: AUTHORITY.client,
        sequence: 4,
    };
    let outcome = Outcome::Confirmed {
        revision: Revision(9),
    };
    for message in [
        WireMessage::Client(ClientMessage::Attach { version: VERSION }),
        WireMessage::Attached {
            authority: AUTHORITY,
        },
        WireMessage::Client(ClientMessage::Ready {
            authority: AUTHORITY,
            revision: Revision(8),
        }),
        WireMessage::Client(ClientMessage::Shutdown {
            authority: AUTHORITY,
            operation,
        }),
        WireMessage::Client(ClientMessage::Detach {
            authority: AUTHORITY,
        }),
        WireMessage::SnapshotBegin {
            authority: AUTHORITY,
            transfer: TransferId(4),
            revision: Revision(8),
            bytes: 1025,
        },
        WireMessage::SnapshotChunk {
            transfer: TransferId(4),
            index: 0,
            bytes: vec![42; CHUNK_BYTES],
        },
        WireMessage::SnapshotEnd {
            transfer: TransferId(4),
        },
        WireMessage::Accepted {
            authority: AUTHORITY,
            operation: PendingOperation {
                id: operation,
                accepted_at: Revision(8),
            },
        },
        WireMessage::Finished {
            authority: AUTHORITY,
            result: OperationResult {
                id: operation,
                outcome: outcome.clone(),
            },
        },
        WireMessage::ShutdownComplete {
            authority: AUTHORITY,
            result: OperationResult {
                id: operation,
                outcome,
            },
        },
        WireMessage::Detached {
            authority: AUTHORITY,
        },
        WireMessage::Rejected(Refusal::Busy),
    ] {
        roundtrip(message);
    }
    for command in [
        Command::SetHostGraphConnected(shoop_app_api::HostGraphRequest {
            epoch: 4,
            local: shoop_app_api::HostGraphPortId(1),
            peer: shoop_app_api::HostGraphPortId(2),
            connected: true,
        }),
        Command::Track {
            track: TrackId::from_raw(7),
            action: TrackAction::NameChanged("Live track".to_owned()),
        },
        Command::Loop {
            track: TrackId::from_raw(7),
            loop_id: LoopId::from_raw(9),
            action: LoopAction::PlayClicked,
        },
        Command::SelectLoops {
            loops: vec![LoopId::from_raw(9), LoopId::from_raw(10)],
        },
        Command::PlaySelected,
        Command::SetProfiling(true),
        Command::InspectProfiling(shoop_app_api::profiling_data::CaptureRequest {
            generation: 3,
            capture: 7,
            callback: 4,
            tick: 2,
            offset: 256,
        }),
        Command::Piano(PianoAction::Press(MidiNote::new(127).unwrap())),
    ] {
        roundtrip(WireMessage::Client(ClientMessage::Submit {
            session_epoch: 1,
            authority: AUTHORITY,
            operation,
            expected_revision: Some(Revision(8)),
            command,
        }));
    }
}

#[shoop_test]
fn versions_protocol_discriminators_lengths_and_identity_are_checked() {
    let message = WireMessage::Rejected(Refusal::Version);
    let mut buffer = [0; MAX_RECORD_BYTES];
    let length = codec::encode(&message, &mut buffer).unwrap();
    for index in 2..8 {
        let mut corrupt = buffer[..length].to_vec();
        corrupt[index] ^= 1;
        assert_eq!(codec::decode(&corrupt), Err(CodecError::Invalid));
    }
    let mut previous = buffer[..length].to_vec();
    previous[6..8].copy_from_slice(&19u16.to_le_bytes());
    assert_eq!(codec::decode(&previous), Err(CodecError::Invalid));
    assert_eq!(
        codec::encode(&message, &mut [0; 7]),
        Err(CodecError::Overflow)
    );
    assert_eq!(
        codec::decode(&[255; MAX_RECORD_BYTES + 1]),
        Err(CodecError::Invalid)
    );
    let mut authority = AUTHORITY;
    authority.generation = ConnectionGeneration(0);
    assert_eq!(
        codec::encode(&WireMessage::Attached { authority }, &mut buffer),
        Err(CodecError::Invalid)
    );
    let operation = OperationId {
        client: ClientId(99),
        sequence: 1,
    };
    assert_eq!(
        codec::encode(
            &WireMessage::Client(ClientMessage::Shutdown {
                authority: AUTHORITY,
                operation
            }),
            &mut buffer
        ),
        Err(CodecError::Invalid)
    );
}

#[shoop_test]
fn bulk_and_command_storage_are_bounded_before_encoding() {
    let mut buffer = [0; MAX_RECORD_BYTES];
    for bytes in [vec![], vec![0; CHUNK_BYTES + 1]] {
        assert_eq!(
            codec::encode(
                &WireMessage::SnapshotChunk {
                    transfer: TransferId(1),
                    index: 0,
                    bytes
                },
                &mut buffer
            ),
            Err(CodecError::Invalid)
        );
    }
    let command = Command::Track {
        track: TrackId::from_raw(1),
        action: TrackAction::NameChanged("x".repeat(MAX_RECORD_BYTES)),
    };
    assert_eq!(
        codec::encode(
            &WireMessage::Client(ClientMessage::Submit {
                session_epoch: 1,
                authority: AUTHORITY,
                operation: OperationId {
                    client: AUTHORITY.client,
                    sequence: 1
                },
                expected_revision: None,
                command
            }),
            &mut buffer
        ),
        Err(CodecError::Invalid)
    );
    assert_eq!(
        codec::encode(
            &WireMessage::SnapshotBegin {
                authority: AUTHORITY,
                transfer: TransferId(1),
                revision: Revision(0),
                bytes: MAX_SNAPSHOT_BYTES as u32 + 1
            },
            &mut buffer
        ),
        Err(CodecError::Invalid)
    );
}

#[shoop_test]
fn invalid_midi_values_do_not_bypass_domain_invariants_during_decode() {
    let mut buffer = [0; MAX_RECORD_BYTES];
    let message = WireMessage::Client(ClientMessage::Submit {
        session_epoch: 1,
        authority: AUTHORITY,
        operation: OperationId {
            client: AUTHORITY.client,
            sequence: 1,
        },
        expected_revision: None,
        command: Command::Piano(PianoAction::Press(MidiNote::new(127).unwrap())),
    });
    let length = codec::encode(&message, &mut buffer).unwrap();
    assert_eq!(buffer[length - 1], 127);
    buffer[length - 1] = 128;
    assert_eq!(codec::decode(&buffer[..length]), Err(CodecError::Invalid));
}

#[shoop_test]
fn hostile_container_lengths_and_midi_message_lengths_are_refused() {
    let mut record = vec![0, 0, b'S', b'A', b'P', b'P', 1, 0, 4, 1, 0, 253];
    record[6..8].copy_from_slice(&VERSION.to_le_bytes());
    record.extend_from_slice(&u64::MAX.to_le_bytes());
    let length = record.len() as u16;
    record[..2].copy_from_slice(&length.to_le_bytes());
    assert!(codec::decode(&record).is_err());
    for len in [0, 5, 255] {
        assert!(
            bincode::serde::decode_from_slice::<shoop_app_api::LatestMidiMessage, _>(
                &[0x90, 60, 100, 0, len],
                bincode::config::standard(),
            )
            .is_err()
        );
    }
    let (message, _) = bincode::serde::decode_from_slice::<shoop_app_api::LatestMidiMessage, _>(
        &[0x90, 60, 100, 0, 3],
        bincode::config::standard(),
    )
    .unwrap();
    assert_eq!(message.data(), [0x90, 60, 100]);
}

#[shoop_test]
fn heartbeat_records_have_a_canonical_cross_platform_representation() {
    let mut bytes = [0; 1280];
    let n = codec::encode(&WireMessage::Ping(42), &mut bytes).unwrap();
    assert_eq!(&bytes[..n], &[10, 0, b'S', b'A', b'P', b'P', 22, 0, 0, 42]);
    let n = codec::encode(&WireMessage::Pong(0x1122334455667788), &mut bytes).unwrap();
    assert_eq!(
        &bytes[..n],
        &[
            18, 0, b'S', b'A', b'P', b'P', 22, 0, 1, 253, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22,
            0x11
        ]
    );
}

#[shoop_test]
fn attachment_and_greeting_carry_no_host_or_session_identifiers() {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let n = codec::encode(
        &WireMessage::Client(ClientMessage::Attach { version: VERSION }),
        &mut bytes,
    )
    .unwrap();
    assert_eq!(
        &bytes[..n],
        &[11, 0, b'S', b'A', b'P', b'P', 22, 0, 3, 1, 22]
    );
    let n = codec::encode(&WireMessage::CarrierReady, &mut bytes).unwrap();
    assert_eq!(&bytes[..n], &[9, 0, b'S', b'A', b'P', b'P', 22, 0, 19]);
}
