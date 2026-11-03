use shoop_app_protocol::{persistence::*, *};
use shoop_transport::MAX_RECORD_BYTES;
use shoop_wasm_test_support::shoop_test;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const AUTHORITY: Authority = Authority {
    lease: LeaseId(1),
    client: ClientId(2),
    generation: ConnectionGeneration(3),
};
const KEY: Key = Key {
    transfer: TransferId(4),
    epoch: 5,
};

fn description(media_bytes: u64) -> Description {
    Description {
        key: KEY,
        target: Target::Session {
            revision: Revision(6),
        },
        objects: 2,
        metadata_bytes: 2,
        media_bytes,
    }
}
fn object(index: u32, kind: ObjectKind, bytes: u64) -> Request {
    Request::Object {
        key: KEY,
        object: ObjectDescription { index, kind, bytes },
    }
}
fn chunk(object: u32, offset: u64, bytes: &[u8]) -> Request {
    Request::Chunk {
        key: KEY,
        object,
        offset,
        bytes: bytes.to_vec(),
    }
}
fn metadata(receiver: &mut Receiver) {
    receiver
        .apply(AUTHORITY, object(0, ObjectKind::Metadata, 2))
        .unwrap();
    receiver.apply(AUTHORITY, chunk(0, 0, b"{}")).unwrap();
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 0,
            },
        )
        .unwrap();
}

#[shoop_test]
fn mixed_objects_require_exact_totals_order_and_finish() {
    let mut receiver = Receiver::new(AUTHORITY, description(4)).unwrap();
    assert_eq!(
        receiver.apply(AUTHORITY, object(1, ObjectKind::Audio, 4)),
        Err(Refusal::Invalid)
    );
    metadata(&mut receiver);
    assert_eq!(
        receiver.apply(AUTHORITY, object(1, ObjectKind::Metadata, 0)),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        receiver.apply(AUTHORITY, object(1, ObjectKind::Audio, 8)),
        Err(Refusal::Overloaded)
    );
    receiver
        .apply(AUTHORITY, object(1, ObjectKind::Audio, 4))
        .unwrap();
    assert_eq!(
        receiver.apply(AUTHORITY, Request::Finish { key: KEY }),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 1, &[1, 2])),
        Err(Refusal::Invalid)
    );
    receiver.apply(AUTHORITY, chunk(1, 0, &[1, 2])).unwrap();
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 1, &[2])),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 0, &[1, 9])),
        Err(Refusal::Invalid)
    );
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 0, &[1, 2])),
        Ok(Reply::Ack {
            key: KEY,
            object: 1,
            next_offset: 2
        })
    );
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 2, &[3, 4, 5])),
        Err(Refusal::Invalid)
    );
    receiver.apply(AUTHORITY, chunk(1, 2, &[3, 4])).unwrap();
    assert_eq!(
        receiver.apply(AUTHORITY, chunk(1, 0, &[1, 2])),
        Err(Refusal::Invalid)
    );
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 1,
            },
        )
        .unwrap();
    receiver
        .apply(AUTHORITY, Request::Finish { key: KEY })
        .unwrap();
    assert!(receiver.ready());
    assert_eq!(receiver.objects()[0].bytes, b"{}");
    assert_eq!(receiver.objects()[1].bytes, [1, 2, 3, 4]);
    assert_eq!(
        receiver.apply(AUTHORITY, object(2, ObjectKind::Midi, 0)),
        Err(Refusal::Busy)
    );
}

#[shoop_test]
fn stale_authority_epoch_transfer_and_overflow_never_consume_data() {
    let mut receiver = Receiver::new(AUTHORITY, description(4)).unwrap();
    metadata(&mut receiver);
    receiver
        .apply(AUTHORITY, object(1, ObjectKind::Audio, 4))
        .unwrap();
    let mut stale = AUTHORITY;
    stale.generation = ConnectionGeneration(7);
    assert_eq!(
        receiver.apply(stale, chunk(1, 0, &[1, 2, 3, 4])),
        Err(Refusal::StaleAuthority)
    );
    for key in [
        Key { epoch: 6, ..KEY },
        Key {
            transfer: TransferId(6),
            ..KEY
        },
    ] {
        assert_eq!(
            receiver.apply(
                AUTHORITY,
                Request::Chunk {
                    key,
                    object: 1,
                    offset: 0,
                    bytes: vec![1; 4]
                }
            ),
            Err(Refusal::Identity)
        );
    }
    for request in [
        chunk(1, u64::MAX, &[1]),
        chunk(1, 0, &[]),
        chunk(1, 0, &[1; CHUNK_BYTES + 1]),
    ] {
        assert_eq!(receiver.apply(AUTHORITY, request), Err(Refusal::Invalid));
    }
    receiver
        .apply(AUTHORITY, chunk(1, 0, &[4, 3, 2, 1]))
        .unwrap();
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 1,
            },
        )
        .unwrap();
    assert_eq!(receiver.objects()[1].bytes, [4, 3, 2, 1]);
}

#[shoop_test]
fn declarations_bound_aggregate_metadata_media_and_object_counts() {
    let good = description(4);
    for invalid in [
        Description { objects: 0, ..good },
        Description {
            objects: MAX_SESSION_OBJECTS + 1,
            ..good
        },
        Description {
            metadata_bytes: 0,
            ..good
        },
        Description {
            metadata_bytes: MAX_SESSION_METADATA_BYTES + 1,
            ..good
        },
        Description {
            media_bytes: MAX_SESSION_MEDIA_BYTES + 1,
            ..good
        },
        Description {
            key: Key { epoch: 0, ..KEY },
            ..good
        },
    ] {
        assert!(Receiver::new(AUTHORITY, invalid).is_err());
    }
    let mut receiver = Receiver::new(AUTHORITY, Description { objects: 3, ..good }).unwrap();
    metadata(&mut receiver);
    receiver
        .apply(AUTHORITY, object(1, ObjectKind::Midi, 3))
        .unwrap();
    receiver.apply(AUTHORITY, chunk(1, 0, &[1, 2, 3])).unwrap();
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 1,
            },
        )
        .unwrap();
    assert_eq!(
        receiver.apply(AUTHORITY, object(2, ObjectKind::Midi, 2)),
        Err(Refusal::Overloaded)
    );
    assert_eq!(
        receiver.apply(AUTHORITY, Request::Finish { key: KEY }),
        Err(Refusal::Invalid)
    );
    receiver
        .apply(AUTHORITY, object(2, ObjectKind::Midi, 1))
        .unwrap();
    receiver.apply(AUTHORITY, chunk(2, 0, &[4])).unwrap();
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 2,
            },
        )
        .unwrap();
    receiver
        .apply(AUTHORITY, Request::Finish { key: KEY })
        .unwrap();
}

#[shoop_test]
fn persistence_records_fit_carriers_and_match_typed_ingress() {
    let mut receiver = Receiver::new(AUTHORITY, description(CHUNK_BYTES as u64)).unwrap();
    let requests = [
        object(0, ObjectKind::Metadata, 2),
        chunk(0, 0, b"{}"),
        Request::FinishObject {
            key: KEY,
            object: 0,
        },
        object(1, ObjectKind::Audio, CHUNK_BYTES as u64),
        chunk(1, 0, &[42; CHUNK_BYTES]),
        Request::FinishObject {
            key: KEY,
            object: 1,
        },
        Request::Finish { key: KEY },
    ];
    for request in requests {
        let message = WireMessage::Client(ClientMessage::Persistence {
            authority: AUTHORITY,
            request: request.clone(),
        });
        let mut bytes = [0; MAX_RECORD_BYTES];
        let used = codec::encode(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode(&bytes[..used]), Ok(message));
        let reply = receiver.apply(AUTHORITY, request).unwrap();
        let message = WireMessage::Persistence {
            authority: AUTHORITY,
            reply,
        };
        let used = codec::encode(&message, &mut bytes).unwrap();
        assert_eq!(codec::decode(&bytes[..used]), Ok(message));
    }
    let reply = WireMessage::Persistence {
        authority: AUTHORITY,
        reply: Reply::Chunk {
            key: KEY,
            object: 1,
            offset: 0,
            bytes: vec![42; CHUNK_BYTES],
        },
    };
    let mut bytes = [0; MAX_RECORD_BYTES];
    let used = codec::encode(&reply, &mut bytes).unwrap();
    for length in 0..used {
        assert!(codec::decode(&bytes[..length]).is_err());
    }
    assert_eq!(codec::decode(&bytes[..used]), Ok(reply));
}

#[shoop_test]
fn transfers_can_exceed_media_view_limit_without_oversized_records() {
    let size = MAX_MEDIA_BYTES as u64 + CHUNK_BYTES as u64;
    let mut receiver = Receiver::new(AUTHORITY, description(size)).unwrap();
    metadata(&mut receiver);
    receiver
        .apply(AUTHORITY, object(1, ObjectKind::Audio, size))
        .unwrap();
    for offset in (0..size).step_by(CHUNK_BYTES) {
        receiver
            .apply(AUTHORITY, chunk(1, offset, &[37; CHUNK_BYTES]))
            .unwrap();
    }
    receiver
        .apply(
            AUTHORITY,
            Request::FinishObject {
                key: KEY,
                object: 1,
            },
        )
        .unwrap();
    receiver
        .apply(AUTHORITY, Request::Finish { key: KEY })
        .unwrap();
    assert!(receiver.ready());
    assert_eq!(receiver.objects()[1].bytes.len() as u64, size);
    assert!(receiver.objects()[1].bytes.iter().all(|&byte| byte == 37));
}
