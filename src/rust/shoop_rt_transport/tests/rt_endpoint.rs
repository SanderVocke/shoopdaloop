#![cfg(feature = "rt-interface")]
use shoop_rt_protocol::{
    codec,
    immediate::{Batch, Code, Command, CommandRecord},
    snapshot::{self, Header, Record, SnapshotDelivery},
    *,
};
use shoop_rt_transport::{
    carrier::{Carrier, CarrierError, Endpoint as CarrierEndpoint, InProcess, Outbox},
    client::{ClientError, ProtocolClient},
    rt::{self, Endpoint, Execution, Settings},
};
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn prepare(records: usize) -> (Endpoint, Execution) {
    let (control, execution) = rt::channel(records, 2, 8192).unwrap();
    let settings = Settings {
        graph: 7,
        topology: 9,
        limits: Limits {
            composite_entries: 0,
            composite_segments: 0,
            timing_relations: 0,
            content_chunk_bytes: 0,
            strips: 0,
            loops: 200,
            strip_audio_channels: 1,
            master_channels: 1,
            buses: 0,
            bus_channels: 0,
            host_audio_inputs: 1,
            host_audio_outputs: 1,
            host_midi_inputs: 0,
            host_midi_outputs: 0,
            max_frames: 32,
            quantum: 16,
            processing_latency_frames: 0,
            smoothing_limit_frames: 0,
            reliable_credits: 2,
            bulk_credits: 1,
        },
        driver: DriverStatus {
            kind: DriverKind::Simulated,
            state: DriverRunState::Running,
            sample_rate: 1000,
            buffer_frames: 16,
        },
    };
    (Endpoint::new(control, settings).unwrap(), execution)
}
fn batch(gain: f32) -> Batch {
    Batch::new(7, 9, &[Command::LoopGain(0, gain)]).unwrap()
}
fn deliver(endpoint: &mut Endpoint, out: &mut Outbox, message: ClientMessage) {
    endpoint.try_deliver(message, out).unwrap();
}
fn connect(endpoint: &mut Endpoint, out: &mut Outbox) {
    deliver(
        endpoint,
        out,
        ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        },
    );
    let Some(EngineMessage::Welcome {
        capabilities,
        limits,
        ..
    }) = out.pop()
    else {
        panic!("welcome");
    };
    assert_eq!(capabilities, Capabilities::default());
    assert_eq!((limits.reliable_credits, limits.bulk_credits), (0, 0));
}
fn complete(execution: &mut Execution, frame: u64) {
    execution.commands.begin_callback();
    while let Some(group) = execution.commands.pop() {
        let header = group.header.complete(frame, Code::Applied);
        group.respond(header, |record| record.complete(Code::Applied));
    }
}
fn publication(frame: u64, gain: f32) -> Vec<u8> {
    let mut bytes = vec![0; 8192];
    let mut pos = snapshot::HEADER;
    for index in 0..200 {
        pos += codec::encode_snapshot(
            &Record::LoopLevels {
                id: LoopSlotId(SlotId::new(index, 1)),
                gain,
                balance: 0.,
                repeat_sync: false,
            },
            &mut bytes[pos..],
        )
        .unwrap();
    }
    Header {
        frame,
        graph: 7,
        topology: 9,
        records: 200,
        loops: 200,
        strips: 0,
        buses: 0,
    }
    .write(&mut bytes);
    bytes.truncate(pos);
    assert!(bytes.len() > 2 * snapshot::CHUNK_BYTES);
    bytes
}

#[shoop_test]
fn carrier_client_uses_queues_and_ignores_legacy_messages_without_running_execution() {
    let (mut endpoint, mut execution) = prepare(4);
    let mut out = Outbox::new(1, 1, 0);
    assert_eq!(
        endpoint.try_deliver(ClientMessage::Immediate(batch(0.25)), &mut out),
        Err(CarrierError::Disconnected)
    );
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::Hello {
            version: PROTOCOL_VERSION + 1,
        },
    );
    assert_eq!(
        out.pop(),
        Some(EngineMessage::Rejected {
            version: PROTOCOL_VERSION
        })
    );
    let mut client = ProtocolClient::new(InProcess::new(endpoint, out));
    client.connect().unwrap();
    assert!(matches!(client.poll(), Some(EngineMessage::Welcome { .. })));
    assert_eq!(
        client.request(Request::RemoveLoop {
            loop_id: LoopSlotId(SlotId::new(0, 1))
        }),
        Err(ClientError::Busy)
    );
    for message in [
        ClientMessage::Request {
            correlation: 1,
            request: Request::RemoveLoop {
                loop_id: LoopSlotId(SlotId::new(0, 1)),
            },
        },
        ClientMessage::Resync,
        ClientMessage::Bulk(BulkRead {
            id: 1,
            loop_id: LoopSlotId(SlotId::new(0, 1)),
            selector: BulkSelector::AudioFrames,
            offset: 0,
        }),
    ] {
        client.carrier_mut().send(message).unwrap();
    }
    client.immediate(batch(0.25)).unwrap();
    for _ in 0..100 {
        assert_eq!(client.poll(), None);
    }
    execution.commands.begin_callback();
    let group = execution
        .commands
        .pop()
        .expect("only the processing owner consumes commands");
    assert_eq!(
        group.commands().copied().collect::<Vec<_>>(),
        vec![CommandRecord::new(Command::LoopGain(0, 0.25))]
    );
    let header = group.header.complete(32, Code::Applied);
    group.respond(header, |record| record.complete(Code::Applied));
    let Some(EngineMessage::ImmediateResponse(response)) = client.poll() else {
        panic!("response");
    };
    assert_eq!(response.header.frame(), 32);
    assert_eq!(response.records()[0].code(), Code::Applied);
    assert_eq!(client.poll(), None);
}

#[shoop_test]
fn reliable_outbox_backpressure_preserves_responses_and_fifo_credits() {
    let (mut endpoint, mut execution) = prepare(4);
    let mut out = Outbox::new(1, 1, 0);
    connect(&mut endpoint, &mut out);
    for gain in [0.25, 0.5] {
        assert_no_heap(|| {
            deliver(
                &mut endpoint,
                &mut out,
                ClientMessage::Immediate(batch(gain)),
            )
        });
    }
    complete(&mut execution, 16);
    out.push(EngineMessage::SnapshotReady { sequence: 123 })
        .unwrap();
    assert_no_heap(|| endpoint.poll(&mut out));
    assert_eq!(
        endpoint.try_deliver(ClientMessage::Immediate(batch(0.75)), &mut out),
        Err(CarrierError::Full)
    );
    assert_eq!(
        out.pop(),
        Some(EngineMessage::SnapshotReady { sequence: 123 })
    );
    for gain in [0.25, 0.5] {
        assert_no_heap(|| endpoint.poll(&mut out));
        let Some(EngineMessage::ImmediateResponse(response)) = out.pop() else {
            panic!("response");
        };
        assert_eq!(response.records()[0].command(), Command::LoopGain(0, gain));
    }
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::Immediate(batch(0.75)),
    );
    complete(&mut execution, 32);
    endpoint.poll(&mut out);
    let Some(EngineMessage::ImmediateResponse(response)) = out.pop() else {
        panic!("response");
    };
    assert_eq!(response.records()[0].command(), Command::LoopGain(0, 0.75));
}

#[shoop_test]
fn pull_snapshot_chunks_remain_coherent_across_overwrites_and_reliable_stalls() {
    let (mut endpoint, mut execution) = prepare(4);
    let mut out = Outbox::new(1, 1, 0);
    connect(&mut endpoint, &mut out);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::ObserveSnapshots(true),
    );
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::ConfigureSnapshots(SnapshotDelivery::Pull),
    );
    let original = publication(0, 0.25);
    let newer = publication(16, 0.5);
    execution.snapshots.publish(&original).unwrap();
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::RequestSnapshot { after: 0 },
    );
    endpoint.poll(&mut out);
    assert_eq!(
        out.pop(),
        Some(EngineMessage::SnapshotConfiguration {
            graph: 7,
            topology: 9
        })
    );
    for _ in 0..20 {
        execution.snapshots.publish(&newer).unwrap();
    }
    let mut received = Vec::new();
    loop {
        assert_no_heap(|| endpoint.poll(&mut out));
        match out.pop().unwrap() {
            EngineMessage::SnapshotReply(chunk) => {
                assert_eq!(chunk.sequence, 2);
                assert_eq!(chunk.offset as usize, received.len());
                received.extend_from_slice(&chunk.bytes[..chunk.len as usize]);
            }
            EngineMessage::SnapshotReady { sequence } => {
                assert_eq!(sequence, 2);
                break;
            }
            message => panic!("{message:?}"),
        }
    }
    assert_eq!(received, original);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::RequestSnapshot { after: 2 },
    );
    received.clear();
    loop {
        endpoint.poll(&mut out);
        match out.pop().unwrap() {
            EngineMessage::SnapshotReply(chunk) => {
                assert_eq!(chunk.sequence, 42);
                received.extend_from_slice(&chunk.bytes[..chunk.len as usize]);
            }
            EngineMessage::SnapshotReady { sequence } => {
                assert_eq!(sequence, 42);
                break;
            }
            message => panic!("{message:?}"),
        }
    }
    assert_eq!(received, newer);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::RequestSnapshot { after: 42 },
    );
    endpoint.poll(&mut out);
    assert_eq!(
        out.pop(),
        Some(EngineMessage::SnapshotReady { sequence: 42 })
    );
    let mut stale = publication(32, 1.);
    stale[12..20].copy_from_slice(&8u64.to_le_bytes());
    execution.snapshots.publish(&stale).unwrap();
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::RequestSnapshot { after: 0 },
    );
    endpoint.poll(&mut out);
    assert_eq!(
        out.pop(),
        Some(EngineMessage::SnapshotReady { sequence: 0 })
    );
}

#[shoop_test]
fn push_snapshot_rate_uses_observed_frames_without_advancing_processing() {
    let (mut endpoint, mut execution) = prepare(4);
    let mut out = Outbox::new(1, 1, 0);
    connect(&mut endpoint, &mut out);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::ObserveSnapshots(true),
    );
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::ConfigureSnapshots(SnapshotDelivery::push(10).unwrap()),
    );
    for (frame, gain, sent) in [(0, 0.25, true), (16, 0.5, false), (128, 0.75, true)] {
        let bytes = publication(frame, gain);
        execution.snapshots.publish(&bytes).unwrap();
        let mut received = Vec::new();
        for _ in 0..20 {
            endpoint.poll(&mut out);
            while let Some(message) = out.pop() {
                match message {
                    EngineMessage::Snapshot(chunk) => {
                        received.extend_from_slice(&chunk.bytes[..chunk.len as usize])
                    }
                    EngineMessage::SnapshotConfiguration {
                        graph: 7,
                        topology: 9,
                    } => {}
                    message => panic!("{message:?}"),
                }
            }
        }
        assert_eq!(received, if sent { bytes } else { vec![] });
    }
}

#[shoop_test]
fn reconnect_discards_pending_old_responses_without_cancelling_execution() {
    let (mut endpoint, mut execution) = prepare(8);
    let mut out = Outbox::new(1, 1, 0);
    connect(&mut endpoint, &mut out);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::Immediate(batch(0.25)),
    );
    deliver(&mut endpoint, &mut out, ClientMessage::Goodbye);
    assert_eq!(
        out.pop(),
        Some(EngineMessage::Disconnected(DisconnectReason::Shutdown))
    );
    connect(&mut endpoint, &mut out);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::Immediate(batch(0.75)),
    );
    execution.commands.begin_callback();
    let mut executed = Vec::new();
    while let Some(group) = execution.commands.pop() {
        executed.extend(group.commands().map(|r| r.command()));
        let header = group.header.complete(16, Code::Applied);
        group.respond(header, |record| record.complete(Code::Applied));
    }
    assert_eq!(
        executed,
        vec![Command::LoopGain(0, 0.25), Command::LoopGain(0, 0.75)]
    );
    endpoint.poll(&mut out);
    let Some(EngineMessage::ImmediateResponse(response)) = out.pop() else {
        panic!("response");
    };
    assert_eq!(response.records()[0].command(), Command::LoopGain(0, 0.75));
    endpoint.poll(&mut out);
    assert_eq!(out.pop(), None);
}

#[shoop_test]
fn publisher_failure_disconnects_without_audio_and_survives_full_reply_space() {
    let (mut endpoint, execution) = prepare(8);
    let mut out = Outbox::new(1, 0, 0);
    deliver(
        &mut endpoint,
        &mut out,
        ClientMessage::Hello {
            version: PROTOCOL_VERSION,
        },
    );
    execution.snapshots.fail();
    assert_no_heap(|| endpoint.poll(&mut out));
    assert!(matches!(out.pop(), Some(EngineMessage::Welcome { .. })));
    assert_no_heap(|| endpoint.poll(&mut out));
    assert_eq!(
        out.pop(),
        Some(EngineMessage::Disconnected(DisconnectReason::Fault))
    );
    assert_no_heap(|| endpoint.poll(&mut out));
    assert!(out.pop().is_none());
}
