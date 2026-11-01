use shoop_app_api::{BackendCapabilities, TrackId, TrackState};
use shoop_app_client::{Client, Error, State};
use shoop_app_protocol::*;
use shoop_transport::MAX_RECORD_BYTES;
use shoop_wasm_test_support::shoop_test;
use std::sync::Arc;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const AUTHORITY: Authority = Authority {
    session: SessionId(1),
    client: ClientId(2),
    generation: ConnectionGeneration(3),
};

fn snapshot(revision: u64) -> Arc<SessionSnapshot> {
    Arc::new(SessionSnapshot {
        revision: Revision(revision),
        tracks: vec![TrackState {
            id: TrackId::from_raw(1),
            name: "Persistent track".to_owned(),
            ..Default::default()
        }],
        buses: Arc::from([]),
        global: Default::default(),
        master_auto_connect: true,
        loop_smoothing_ms: 4,
        status: Default::default(),
        audio_drivers: Default::default(),
        capabilities: BackendCapabilities::NONE,
        connections: Default::default(),
        track_processors: Arc::from([]),
        bus_processors: Arc::from([]),
        pending: vec![],
        results: vec![],
    })
}

fn attaching() -> Client {
    let mut client = Client::new(AUTHORITY.client, HostId(4), AUTHORITY.session).unwrap();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached {
            authority: AUTHORITY,
            host: HostId(4),
        })
        .unwrap();
    client
}

fn ready() -> Client {
    let mut client = attaching();
    client
        .receive(ServiceMessage::Snapshot {
            authority: AUTHORITY,
            state: snapshot(5),
        })
        .unwrap();
    client.ready_sent().unwrap();
    client
}

fn wire(client: &mut Client, message: WireMessage) -> Result<(), Error> {
    let mut bytes = [0; MAX_RECORD_BYTES];
    let used = codec::encode(&message, &mut bytes).unwrap();
    client.receive_wire(codec::decode(&bytes[..used]).unwrap())
}

fn encoded_snapshot(client: &mut Client, state: &SessionSnapshot) {
    let bytes = codec::encode_snapshot(state).unwrap();
    wire(
        client,
        WireMessage::SnapshotBegin {
            authority: AUTHORITY,
            transfer: TransferId(1),
            revision: state.revision,
            bytes: bytes.len() as u32,
        },
    )
    .unwrap();
    for (index, bytes) in bytes.chunks(CHUNK_BYTES).enumerate() {
        wire(
            client,
            WireMessage::SnapshotChunk {
                transfer: TransferId(1),
                index: index as u32,
                bytes: bytes.to_vec(),
            },
        )
        .unwrap();
    }
    wire(
        client,
        WireMessage::SnapshotEnd {
            transfer: TransferId(1),
        },
    )
    .unwrap();
}

#[shoop_test]
fn typed_and_encoded_views_require_a_coherent_snapshot_before_commands() {
    for encoded in [false, true] {
        let mut client = attaching();
        assert_eq!(
            client.command(Command::PlaySelected, None),
            Err(Error::NotReady)
        );
        let state = snapshot(5);
        if encoded {
            encoded_snapshot(&mut client, &state);
        } else {
            client
                .receive(ServiceMessage::Snapshot {
                    authority: AUTHORITY,
                    state: Arc::clone(&state),
                })
                .unwrap();
            assert!(Arc::ptr_eq(client.snapshot().unwrap(), &state));
        }
        assert_eq!(client.state(), State::Synchronizing);
        assert_eq!(
            client.command(Command::PlaySelected, None),
            Err(Error::NotReady)
        );
        assert_eq!(
            client.ready_message(),
            Some(ClientMessage::Ready {
                authority: AUTHORITY,
                revision: Revision(5)
            })
        );
        client.ready_sent().unwrap();
        let ClientMessage::Submit { operation, .. } = client
            .command(Command::PlaySelected, Some(Revision(5)))
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            client.snapshot().unwrap().tracks[0].name,
            "Persistent track"
        );
        assert!(
            client.pending().is_empty(),
            "unsent/submitted work must not appear accepted"
        );
        let accepted = PendingOperation {
            id: operation,
            accepted_at: Revision(5),
        };
        let result = OperationResult {
            id: operation,
            outcome: Outcome::Confirmed {
                revision: Revision(6),
            },
        };
        if encoded {
            wire(
                &mut client,
                WireMessage::Accepted {
                    authority: AUTHORITY,
                    operation: accepted,
                },
            )
            .unwrap();
            wire(
                &mut client,
                WireMessage::Finished {
                    authority: AUTHORITY,
                    result: result.clone(),
                },
            )
            .unwrap();
        } else {
            client
                .receive(ServiceMessage::Accepted {
                    authority: AUTHORITY,
                    operation: accepted,
                })
                .unwrap();
            client
                .receive(ServiceMessage::Finished {
                    authority: AUTHORITY,
                    result: result.clone(),
                })
                .unwrap();
        }
        assert!(client.pending().is_empty());
        assert_eq!(client.results().get(&operation), Some(&result.outcome));
        assert_eq!(
            client.snapshot().unwrap().revision,
            Revision(5),
            "terminal result is distinct from structural publication"
        );
    }
}

#[shoop_test]
fn reattachment_reconciles_lost_replies_without_command_replay() {
    let mut client = ready();
    let ClientMessage::Submit { operation, .. } =
        client.command(Command::PlaySelected, None).unwrap()
    else {
        panic!()
    };
    client.disconnect();
    assert_eq!(
        client.command(Command::PlaySelected, None),
        Err(Error::NotReady)
    );
    client.attach().unwrap();
    let replacement = Authority {
        generation: ConnectionGeneration(4),
        ..AUTHORITY
    };
    client
        .receive(ServiceMessage::Attached {
            authority: replacement,
            host: HostId(4),
        })
        .unwrap();
    let mut state = (*snapshot(9)).clone();
    state.pending.push(PendingOperation {
        id: operation,
        accepted_at: Revision(6),
    });
    client
        .receive(ServiceMessage::Snapshot {
            authority: replacement,
            state: Arc::new(state),
        })
        .unwrap();
    assert!(client.pending().contains_key(&operation));
    client.ready_sent().unwrap();
    let ClientMessage::Submit {
        operation: next,
        authority,
        ..
    } = client.command(Command::ResetXruns, None).unwrap()
    else {
        panic!()
    };
    assert!(next.sequence > operation.sequence);
    assert_eq!(authority, replacement);
}

#[shoop_test]
fn interrupted_out_of_order_and_oversized_transfers_never_publish_partial_state() {
    for index in [1, 2] {
        let mut client = attaching();
        wire(
            &mut client,
            WireMessage::SnapshotBegin {
                authority: AUTHORITY,
                transfer: TransferId(1),
                revision: Revision(5),
                bytes: 4,
            },
        )
        .unwrap();
        assert_eq!(
            wire(
                &mut client,
                WireMessage::SnapshotChunk {
                    transfer: TransferId(1),
                    index,
                    bytes: vec![0]
                }
            ),
            Err(Error::Protocol)
        );
        assert_eq!(client.state(), State::Lost);
        assert!(client.snapshot().is_none());
    }
    let mut client = attaching();
    wire(
        &mut client,
        WireMessage::SnapshotBegin {
            authority: AUTHORITY,
            transfer: TransferId(1),
            revision: Revision(5),
            bytes: 1,
        },
    )
    .unwrap();
    assert_eq!(
        wire(
            &mut client,
            WireMessage::SnapshotChunk {
                transfer: TransferId(1),
                index: 0,
                bytes: vec![0, 0]
            }
        ),
        Err(Error::Protocol)
    );
    let mut client = attaching();
    wire(
        &mut client,
        WireMessage::SnapshotBegin {
            authority: AUTHORITY,
            transfer: TransferId(1),
            revision: Revision(5),
            bytes: 4,
        },
    )
    .unwrap();
    assert_eq!(
        wire(
            &mut client,
            WireMessage::SnapshotEnd {
                transfer: TransferId(1)
            }
        ),
        Err(Error::Protocol)
    );
    assert!(client.snapshot().is_none());
}

#[shoop_test]
fn mismatched_snapshot_revision_and_stale_authority_are_refused() {
    let mut client = attaching();
    let bytes = codec::encode_snapshot(&snapshot(6)).unwrap();
    wire(
        &mut client,
        WireMessage::SnapshotBegin {
            authority: AUTHORITY,
            transfer: TransferId(1),
            revision: Revision(5),
            bytes: bytes.len() as u32,
        },
    )
    .unwrap();
    for (index, bytes) in bytes.chunks(CHUNK_BYTES).enumerate() {
        wire(
            &mut client,
            WireMessage::SnapshotChunk {
                transfer: TransferId(1),
                index: index as u32,
                bytes: bytes.to_vec(),
            },
        )
        .unwrap();
    }
    assert_eq!(
        wire(
            &mut client,
            WireMessage::SnapshotEnd {
                transfer: TransferId(1)
            }
        ),
        Err(Error::Protocol)
    );
    let mut client = ready();
    let stale = Authority {
        generation: ConnectionGeneration(2),
        ..AUTHORITY
    };
    assert_eq!(
        client.receive(ServiceMessage::Snapshot {
            authority: stale,
            state: snapshot(9)
        }),
        Err(Error::Identity)
    );
    assert_eq!(client.snapshot().unwrap().revision, Revision(5));
}

#[shoop_test]
fn local_delivery_is_bounded_and_ready_ack_precedes_commands() {
    let (port, service) = local::channel();
    let mut client = attaching();
    for _ in 0..QUEUE_CAPACITY {
        port.commands
            .try_send(ClientMessage::Detach {
                authority: AUTHORITY,
            })
            .unwrap();
    }
    service
        .updates
        .try_send(ServiceMessage::Snapshot {
            authority: AUTHORITY,
            state: snapshot(5),
        })
        .unwrap();
    assert_eq!(client.pump_local(&port), Ok(1));
    assert_eq!(client.state(), State::Synchronizing);
    assert_eq!(
        client.submit_local(&port, Command::PlaySelected, None),
        Err(Error::NotReady)
    );
    for _ in 0..QUEUE_CAPACITY {
        service.commands.try_recv().unwrap();
    }
    client.pump_local(&port).unwrap();
    assert_eq!(client.state(), State::Ready);
    let operation = client
        .submit_local(&port, Command::PlaySelected, None)
        .unwrap();
    assert!(matches!(
        service.commands.try_recv().unwrap(),
        ClientMessage::Ready { .. }
    ));
    assert!(
        matches!(service.commands.try_recv().unwrap(), ClientMessage::Submit { operation: id, .. } if id == operation)
    );
    for _ in 0..QUEUE_CAPACITY {
        port.commands
            .try_send(ClientMessage::Detach {
                authority: AUTHORITY,
            })
            .unwrap();
    }
    assert_eq!(
        client.submit_local(&port, Command::ResetXruns, None),
        Err(Error::Full)
    );
    for _ in 0..QUEUE_CAPACITY {
        service.commands.try_recv().unwrap();
    }
    for _ in 0..MAX_PENDING_OPERATIONS - 1 {
        client.command(Command::ResetXruns, None).unwrap();
    }
    assert_eq!(client.command(Command::ResetXruns, None), Err(Error::Full));
    drop(service);
    assert_eq!(client.pump_local(&port), Err(Error::Disconnected));
}

#[shoop_test]
fn cooperative_pump_limits_each_turn_and_reclaims_transfer_on_loss() {
    let (port, service) = local::channel();
    let mut client = ready();
    for _ in 0..MAX_MESSAGES_PER_TURN + 1 {
        service
            .updates
            .try_send(ServiceMessage::Snapshot {
                authority: AUTHORITY,
                state: snapshot(6),
            })
            .unwrap();
    }
    assert_eq!(client.pump_local(&port), Ok(MAX_MESSAGES_PER_TURN));
    assert_eq!(client.pump_local(&port), Ok(1));
    client.disconnect();
    assert!(client.authority().is_none());
    assert!(client.ready_message().is_none());
}

#[shoop_test]
fn shutdown_results_are_correlated_and_refusal_allows_an_explicit_retry() {
    let mut client = ready();
    let ClientMessage::Shutdown {
        operation,
        authority,
    } = client.shutdown().unwrap()
    else {
        panic!()
    };
    let outcome = Outcome::Refused {
        reason: Refusal::ShutdownFailed,
        message: "device did not stop".to_owned(),
    };
    wire(
        &mut client,
        WireMessage::ShutdownComplete {
            authority,
            result: OperationResult {
                id: operation,
                outcome: outcome.clone(),
            },
        },
    )
    .unwrap();
    assert_eq!(client.results().get(&operation), Some(&outcome));
    assert_eq!(client.state(), State::Ready);
    let ClientMessage::Shutdown {
        operation: retry, ..
    } = client.shutdown().unwrap()
    else {
        panic!()
    };
    assert!(retry.sequence > operation.sequence);
}

#[shoop_test]
fn snapshot_storage_and_operation_inventory_are_bounded_in_both_delivery_modes() {
    let mut state = (*snapshot(5)).clone();
    state.tracks[0].name = "x".repeat(MAX_SNAPSHOT_BYTES);
    assert!(codec::encode_snapshot(&state).is_err());
    assert!(codec::decode_snapshot(&vec![0; MAX_SNAPSHOT_BYTES + 1]).is_err());
    state = (*snapshot(5)).clone();
    state.pending = vec![
        PendingOperation {
            id: OperationId {
                client: AUTHORITY.client,
                sequence: 1
            },
            accepted_at: Revision(5),
        };
        MAX_PENDING_OPERATIONS + 1
    ];
    assert!(codec::encode_snapshot(&state).is_err());
    assert_eq!(
        attaching().receive(ServiceMessage::Snapshot {
            authority: AUTHORITY,
            state: Arc::new(state)
        }),
        Err(Error::Protocol)
    );
}
