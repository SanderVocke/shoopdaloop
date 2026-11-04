use shoop_app_api::{BackendCapabilities, TrackId, TrackState};
use shoop_app_client::{Client, Error, State};
use shoop_app_protocol::*;
use shoop_transport::MAX_RECORD_BYTES;
use shoop_wasm_test_support::shoop_test;
use std::sync::Arc;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const AUTHORITY: Authority = Authority {
    lease: LeaseId(1),
    client: ClientId(2),
    generation: ConnectionGeneration(3),
};

fn snapshot(revision: u64) -> Arc<SessionSnapshot> {
    let mut profiling = shoop_app_api::ProfilingView {
        generation: revision,
        active: true,
        requested: true,
        available: true,
        ..Default::default()
    };
    profiling.live.generation = revision;
    profiling.live.rows.push(Default::default());
    let row = &mut profiling.live.rows[0];
    row.name = "Loop 1 MIDI playback".into();
    row.samples = 8;
    row.avg_ns = 1250.;
    row.worst_ns = 3000;
    row.stddev_ns = 500.;
    row.ready_avg_ns = 3250.;
    row.ready_worst_ns = 6000;
    row.ready_stddev_ns = 750.;
    profiling.topology = Some(Arc::new(shoop_app_api::profiling_data::Topology {
        generation: revision,
        sample_rate: 48000,
        quantum: 32,
        nodes: vec![shoop_app_api::profiling_data::GraphNode::default()],
        ..Default::default()
    }));
    Arc::new(SessionSnapshot {
        profiling,
        sync_sources: Vec::new(),
        preview: Default::default(),
        preview_bus: None,
        bus_limits: Default::default(),
        persistence_epoch: 1,
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
        host_graph: Arc::default(),
        connections: Default::default(),
        track_processors: Arc::from([]),
        bus_processors: Arc::from([]),
        pending: vec![],
        results: vec![],
    })
}

fn attaching() -> Client {
    let mut client = Client::new();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached {
            authority: AUTHORITY,
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

#[shoop_test]
fn persistence_replies_are_fenced_bounded_and_discarded_on_loss() {
    let key = persistence::Key {
        transfer: TransferId(8),
        epoch: 1,
    };
    let request = persistence::Request::Cancel { key };
    assert_eq!(
        attaching().persistence(request.clone()),
        Err(Error::NotReady)
    );
    for encoded in [false, true] {
        let mut client = ready();
        assert_eq!(
            client.persistence(request.clone()),
            Ok(ClientMessage::Persistence {
                authority: AUTHORITY,
                request: request.clone(),
            })
        );
        let reply = persistence::Reply::Closed { key };
        for _ in 0..MAX_MESSAGES_PER_TURN {
            if encoded {
                wire(
                    &mut client,
                    WireMessage::Persistence {
                        authority: AUTHORITY,
                        reply: reply.clone(),
                    },
                )
                .unwrap();
            } else {
                client
                    .receive(ServiceMessage::Persistence {
                        authority: AUTHORITY,
                        reply: reply.clone(),
                    })
                    .unwrap();
            }
        }
        assert_eq!(client.take_persistence_reply(), Some(reply.clone()));
        client
            .receive(ServiceMessage::Persistence {
                authority: AUTHORITY,
                reply: reply.clone(),
            })
            .unwrap();
        assert_eq!(
            client.receive(ServiceMessage::Persistence {
                authority: AUTHORITY,
                reply: reply.clone()
            }),
            Err(Error::Full)
        );
        assert_eq!(client.state(), State::Lost);
        assert_eq!(client.take_persistence_reply(), None);
        let mut client = ready();
        let stale = Authority {
            generation: ConnectionGeneration(99),
            ..AUTHORITY
        };
        assert!(client
            .receive(ServiceMessage::Persistence {
                authority: stale,
                reply
            })
            .is_err());
        assert_eq!(client.take_persistence_reply(), None);
    }
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

#[shoop_test]
fn media_streams_are_bounded_ordered_and_cancelled_without_serializing_local_data() {
    use shoop_app_api::LoopId;
    for encoded in [false, true] {
        let mut client = ready();
        let request = client.read_media(LoopId::from_raw(2)).unwrap();
        let ClientMessage::ReadMedia { transfer, .. } = request else {
            unreachable!()
        };
        let data = Arc::new(LoopMedia {
            loop_id: LoopId::from_raw(2),
            content_revision: 7,
            audio: vec![AudioMediaChannel {
                samples: vec![0.25; 32768].into(),
                start_offset: 0,
                capture_alignment_frames: 0,
                preplay: 0,
            }],
            midi: vec![MidiMediaChannel {
                events: Arc::from([]),
                content_revision: 7,
                length: 32768,
                capture_alignment_frames: 16,
                start_offset: -8,
                preplay: 4,
            }],
        });
        let message = ServiceMessage::Media {
            authority: AUTHORITY,
            transfer,
            data: Arc::clone(&data),
        };
        if encoded {
            let mut stream = stream::StreamEncoder::new(message, TransferId(100)).unwrap();
            while let Some(message) = stream.next_message() {
                wire(&mut client, message).unwrap();
            }
        } else {
            client.receive(message).unwrap();
            assert!(Arc::ptr_eq(
                client.media().unwrap().as_ref().unwrap(),
                &data
            ));
        }
        assert_eq!(
            client.media().unwrap().as_ref().unwrap().audio[0]
                .samples
                .len(),
            32768
        );
        let received = client.media().unwrap().as_ref().unwrap();
        let midi = &received.midi[0];
        assert_eq!(midi.length, 32768);
        assert_eq!(midi.capture_alignment_frames, 16);
        assert_eq!(midi.start_offset, -8);
        assert_eq!(midi.preplay, 4);
        client.cancel_media().unwrap();
        assert!(client.media().is_none());
        client
            .receive(ServiceMessage::Media {
                authority: AUTHORITY,
                transfer,
                data,
            })
            .unwrap();
        assert!(
            client.media().is_none(),
            "late cancelled delivery must not restore its buffers"
        );
        assert_eq!(client.state(), State::Ready);
    }
    for bad_index in [1, 2] {
        let mut client = ready();
        let ClientMessage::ReadMedia { transfer, .. } =
            client.read_media(LoopId::from_raw(2)).unwrap()
        else {
            unreachable!()
        };
        wire(
            &mut client,
            WireMessage::MediaBegin {
                authority: AUTHORITY,
                transfer,
                bytes: 100,
            },
        )
        .unwrap();
        assert_eq!(
            wire(
                &mut client,
                WireMessage::MediaChunk {
                    transfer,
                    index: bad_index,
                    bytes: vec![0]
                }
            ),
            Err(Error::Protocol)
        );
        assert!(client.media().is_none());
        assert_eq!(client.state(), State::Lost);
    }
    let mut client = ready();
    let ClientMessage::ReadMedia { transfer, .. } = client.read_media(LoopId::from_raw(2)).unwrap()
    else {
        unreachable!()
    };
    assert_eq!(
        client.receive_wire(WireMessage::MediaBegin {
            authority: AUTHORITY,
            transfer,
            bytes: MAX_MEDIA_BYTES as u32 + 1
        }),
        Err(Error::Protocol)
    );
    assert!(client.media().is_none());
}

#[shoop_test]
fn commands_carry_the_latest_session_epoch_and_images_keep_typed_ownership() {
    use shoop_app_protocol::persistence_image::{Image, Manifest};
    let mut client = ready();
    let mut state = (*snapshot(6)).clone();
    state.persistence_epoch = 9;
    client
        .receive(ServiceMessage::Snapshot {
            authority: AUTHORITY,
            state: Arc::new(state),
        })
        .unwrap();
    let ClientMessage::Submit { session_epoch, .. } =
        client.command(Command::ResetXruns, None).unwrap()
    else {
        panic!()
    };
    assert_eq!(session_epoch, 9);
    let image = Arc::new(Image {
        manifest: Manifest::Update {
            sample_rate: 48000,
            frames: Some(0),
            audio: vec![],
            midi: vec![],
        },
        payloads: vec![],
    });
    let key = persistence::Key {
        transfer: TransferId(8),
        epoch: 9,
    };
    client
        .receive(ServiceMessage::CapturedImage {
            authority: AUTHORITY,
            key,
            image: Arc::clone(&image),
        })
        .unwrap();
    let (_, received) = client.take_captured_image().unwrap();
    assert!(Arc::ptr_eq(&image, &received));
    client
        .receive(ServiceMessage::CapturedImage {
            authority: AUTHORITY,
            key,
            image,
        })
        .unwrap();
    client.disconnect();
    assert!(client.take_captured_image().is_none());
}
