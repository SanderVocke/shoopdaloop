use super::*;
use shoop_app_protocol::*;
use shoop_wasm_test_support::shoop_test;
use std::{collections::VecDeque, sync::Arc};

const AUTHORITY: Authority = Authority {
    lease: LeaseId(1),
    client: ClientId(2),
    generation: ConnectionGeneration(1),
};

fn snapshot(revision: u64, epoch: u64) -> SessionSnapshot {
    SessionSnapshot {
        composites: Vec::new(),
        profiling: Default::default(),
        preview: Default::default(),
        preview_bus: None,
        bus_limits: Default::default(),
        persistence_epoch: epoch,
        sync_sources: Vec::new(),
        revision: Revision(revision),
        tracks: Vec::new(),
        buses: Arc::from([]),
        global: Default::default(),
        master_auto_connect: true,
        loop_smoothing_ms: 0,
        status: Default::default(),
        audio_drivers: Default::default(),
        capabilities: shoop_app_api::BackendCapabilities::NONE,
        connections: Arc::default(),
        host_graph: Arc::default(),
        track_processors: Arc::from([]),
        bus_processors: Arc::from([]),
        pending: Vec::new(),
        results: Vec::new(),
    }
}

fn ready_client() -> (Client, OperationId) {
    let mut client = Client::new();
    client.attach().unwrap();
    client
        .receive(ServiceMessage::Attached {
            authority: AUTHORITY,
        })
        .unwrap();
    client
        .receive(ServiceMessage::Snapshot {
            authority: AUTHORITY,
            state: Arc::new(snapshot(5, 1)),
        })
        .unwrap();
    client.ready_sent().unwrap();
    let ClientMessage::Submit { operation, .. } = client
        .command(
            Command::CommitPersistence(persistence::Key {
                transfer: TransferId(11),
                epoch: 1,
            }),
            None,
        )
        .unwrap()
    else {
        panic!("command expected")
    };
    (client, operation)
}

#[shoop_test(no_wasm = "the Zynq probe uses native transports")]
fn a_finished_commit_waits_for_the_complete_snapshot_before_reusing_the_session_epoch() {
    for outcome in [
        Outcome::Confirmed {
            revision: Revision(6),
        },
        Outcome::AncestorsStopped {
            revision: Revision(6),
            loops: vec![shoop_app_api::LoopId::from_raw(1)],
        },
    ] {
        for snapshot_first in [false, true] {
            let (mut client, operation) = ready_client();
            let finished = WireMessage::Finished {
                authority: AUTHORITY,
                result: OperationResult {
                    id: operation,
                    outcome: outcome.clone(),
                },
            };
            let bytes = codec::encode_snapshot(&snapshot(6, 2)).unwrap();
            let mut messages = VecDeque::from([WireMessage::SnapshotBegin {
                authority: AUTHORITY,
                transfer: TransferId(1),
                revision: Revision(6),
                bytes: bytes.len() as u32,
            }]);
            for (chunk, bytes) in bytes.chunks(CHUNK_BYTES).enumerate() {
                messages.push_back(WireMessage::SnapshotChunk {
                    transfer: TransferId(1),
                    index: chunk as u32,
                    bytes: bytes.to_vec(),
                });
            }
            messages.push_back(WireMessage::SnapshotEnd {
                transfer: TransferId(1),
            });
            if snapshot_first {
                messages.push_back(finished);
            } else {
                messages.push_front(finished);
            }
            assert_eq!(
                wait_for_result(&mut client, operation, true, |client| {
                    client
                        .receive_wire(messages.pop_front().expect("publication did not complete"))
                        .unwrap();
                }),
                outcome
            );
            assert!(
                messages.is_empty(),
                "command returned before structural publication"
            );
            assert_eq!(client.snapshot().unwrap().persistence_epoch, 2);
        }
    }
}

#[shoop_test(no_wasm = "the Zynq probe uses native transports")]
fn refusals_and_shutdown_complete_without_a_new_snapshot() {
    for (wait_for_snapshot, outcome) in [
        (
            true,
            Outcome::Refused {
                reason: Refusal::Unsupported,
                message: "unsupported".into(),
            },
        ),
        (
            false,
            Outcome::Confirmed {
                revision: Revision(6),
            },
        ),
    ] {
        let (mut client, operation) = ready_client();
        let mut progressed = false;
        assert_eq!(
            wait_for_result(&mut client, operation, wait_for_snapshot, |client| {
                assert!(!progressed, "unnecessary snapshot wait");
                progressed = true;
                client
                    .receive(ServiceMessage::Finished {
                        authority: AUTHORITY,
                        result: OperationResult {
                            id: operation,
                            outcome: outcome.clone(),
                        },
                    })
                    .unwrap();
            }),
            outcome
        );
        assert_eq!(client.snapshot().unwrap().persistence_epoch, 1);
    }
}
