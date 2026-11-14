mod support;
use shoop_rt_protocol::{
    snapshot::{Header, Record, Records, HEADER},
    *,
};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn read(h: &mut Harness) -> (u32, Header, Vec<Record>) {
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (sequence, len) = assert_no_heap(|| h.host.endpoint().read_snapshot(&mut bytes)).unwrap();
    let header = Header::read(&bytes[..len]).unwrap();
    let records = Records(&bytes[HEADER..len])
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(header.records as usize, records.len());
    (sequence, header, records)
}

#[shoop_test]
fn callback_publication_survives_stalls_and_tracks_configuration_and_generation() {
    let mut h = Harness::new(&description());
    let track = strip(0, 1);
    let id = loop_id(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, true),
    });
    h.done(Request::CreateLoop {
        loop_id: id,
        strip: track,
    });
    h.done(Request::LoopGain {
        loop_id: id,
        gain: 0.25,
    });
    h.done(Request::LoopBalance {
        loop_id: id,
        balance: -0.5,
    });
    h.run(16, 16);
    let (sequence, header, records) = read(&mut h);
    assert_eq!(header.loops, 1);
    assert_eq!(header.strips, 1);
    assert_eq!(header.buses, 1);
    assert!(records.iter().any(|r| matches!(r, Record::LoopLevels { id: actual, gain: 0.25, balance: -0.5, .. } if *actual == id)));
    // Control does not poll while RT wraps the ten-slot ring repeatedly.
    for _ in 0..32 {
        assert_no_heap(|| h.host.process(16)).unwrap();
    }
    h.pump();
    let (new_sequence, next, _) = read(&mut h);
    assert!(new_sequence > sequence);
    assert!(next.frame > header.frame);
    h.done(Request::RemoveLoop { loop_id: id });
    let replacement = loop_id(0, 2);
    h.done(Request::CreateLoop {
        loop_id: replacement,
        strip: track,
    });
    h.run(16, 16);
    let (_, changed, records) = read(&mut h);
    assert!(changed.graph > header.graph);
    assert!(records
        .iter()
        .any(|r| matches!(r, Record::LoopLevels { id: actual, .. } if *actual == replacement)));
    assert!(!records
        .iter()
        .any(|r| matches!(r, Record::LoopLevels { id: actual, .. } if *actual == id)));
    assert_eq!(h.host.endpoint().snapshot_error(), None);
}

#[shoop_test]
fn capacity_refusal_keeps_the_session_and_legacy_results_working() {
    let mut h = Harness::new(&description());
    let track = strip(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, false),
    });
    let capacity = h.host.endpoint().snapshot_capacity();
    assert!(matches!(
        h.apply(Request::CreateLoop {
            loop_id: loop_id(999, 1),
            strip: track
        }),
        Outcome::Refused(Refusal::Capacity)
    ));
    h.run(16, 16);
    let (_, header, _) = read(&mut h);
    assert_eq!(header.strips, 1);
    assert_eq!(header.loops, 0);
    assert_eq!(h.host.endpoint().snapshot_capacity(), capacity);
}

#[shoop_test]
fn sixty_four_stereo_loops_have_a_bounded_complete_payload() {
    let mut config = description();
    config.segments = vec![shoop_rt_engine::Segment {
        strips: 8,
        loops: 64,
    }];
    config.buses = 1;
    let mut h = Harness::new(&config);
    for strip_index in 0..8 {
        let track = strip(strip_index, 1);
        h.done(Request::CreateStrip {
            strip: track,
            shape: shape(2, 2, false),
        });
        for index in 0..8 {
            h.done(Request::CreateLoop {
                loop_id: loop_id(strip_index * 8 + index, 1),
                strip: track,
            });
        }
    }
    h.run(16, 16);
    let (_, header, _) = read(&mut h);
    assert_eq!((header.loops, header.strips, header.buses), (64, 8, 1));
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let start = std::time::Instant::now();
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    eprintln!(
        "64-loop snapshot: {len} bytes; one debug read {:?}",
        start.elapsed()
    );
    assert!(len < bytes.len());
}

#[shoop_test]
fn pending_graph_change_keeps_publishing_the_old_active_configuration() {
    let mut config = description();
    config.control_lead_frames = 32;
    let mut h = Harness::new(&config);
    let track = strip(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, false),
    });
    h.done(Request::CreateLoop {
        loop_id: loop_id(0, 1),
        strip: track,
    });
    h.run(16, 16);
    let (sequence, before, _) = read(&mut h);
    h.send(Request::CreateLoop {
        loop_id: loop_id(1, 1),
        strip: track,
    });
    h.run(1, 1);
    let (new_sequence, pending, _) = read(&mut h);
    assert!(new_sequence > sequence);
    assert_eq!(pending.graph, before.graph);
    assert_eq!(pending.loops, 1);
    h.settle();
    h.run(16, 16);
    let (_, after, _) = read(&mut h);
    assert!(after.graph > before.graph);
    assert_eq!(after.loops, 2);
}

#[shoop_test]
fn pull_requests_are_bounded_and_complete_without_an_audio_callback() {
    use shoop_rt_protocol::snapshot::SnapshotDelivery;
    use shoop_rt_transport::client::ClientError;
    let mut h = Harness::new(&description());
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(SnapshotDelivery::Pull)
        .unwrap();
    h.host.client.request_snapshot(0).unwrap();
    assert_eq!(h.host.client.request_snapshot(0), Err(ClientError::Busy));
    h.pump();
    assert!(!h.host.client.snapshot_in_flight());
    assert_eq!(h.frame(), 0);
    assert!(h
        .lifecycle
        .iter()
        .any(|m| matches!(m, EngineMessage::SnapshotReady { sequence: 0 })));
    h.lifecycle.clear();
    h.run(32, 1);
    assert!(!h.lifecycle.iter().any(|m| matches!(
        m,
        EngineMessage::Snapshot(_) | EngineMessage::SnapshotReply(_)
    )));
    let (sequence, header, _) = read(&mut h);
    assert_eq!(header.frame, 31);
    assert_eq!(sequence, 64);
    h.host.client.request_snapshot(0).unwrap();
    h.pump();
    assert!(h
        .lifecycle
        .iter()
        .any(|m| matches!(m, EngineMessage::SnapshotReply(chunk) if chunk.sequence == sequence)));
    assert!(h.lifecycle.iter().any(
        |m| matches!(m, EngineMessage::SnapshotReady { sequence: actual } if *actual == sequence)
    ));
    h.lifecycle.clear();
    h.host.client.request_snapshot(sequence).unwrap();
    h.pump();
    assert!(!h.host.client.snapshot_in_flight());
    assert!(!h
        .lifecycle
        .iter()
        .any(|m| matches!(m, EngineMessage::SnapshotReply(_))));
    assert_eq!(h.frame(), 32);
    h.host.client.request_snapshot(sequence).unwrap();
    h.host.client.observe_snapshots(false).unwrap();
    h.pump();
    assert!(!h.host.client.snapshot_in_flight());
}

#[shoop_test]
fn push_rates_can_change_and_pull_bypasses_the_push_timer() {
    use shoop_rt_protocol::snapshot::SnapshotDelivery;
    let mut h = Harness::new(&description());
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(SnapshotDelivery::push(60).unwrap())
        .unwrap();
    h.run(200, 1);
    let frames: Vec<_> = h
        .lifecycle
        .iter()
        .filter_map(|m| match m {
            EngineMessage::Snapshot(chunk) if chunk.offset == 0 => Some(
                Header::read(&chunk.bytes[..chunk.len as usize])
                    .unwrap()
                    .frame,
            ),
            _ => None,
        })
        .collect();
    assert!(frames.len() >= 10 && frames.len() <= 12);
    assert!(frames.windows(2).all(|pair| pair[1] - pair[0] >= 17));
    h.lifecycle.clear();
    h.host
        .client
        .configure_snapshots(SnapshotDelivery::push(120).unwrap())
        .unwrap();
    h.run(100, 1);
    let frames: Vec<_> = h
        .lifecycle
        .iter()
        .filter_map(|m| match m {
            EngineMessage::Snapshot(chunk) if chunk.offset == 0 => Some(
                Header::read(&chunk.bytes[..chunk.len as usize])
                    .unwrap()
                    .frame,
            ),
            _ => None,
        })
        .collect();
    assert!(frames.len() >= 10 && frames.len() <= 12);
    assert!(frames.windows(2).all(|pair| pair[1] - pair[0] >= 9));
    let (sequence, header, _) = read(&mut h);
    assert_eq!(sequence, 600);
    assert_eq!(header.frame, 299);
    h.lifecycle.clear();
    h.host
        .client
        .configure_snapshots(SnapshotDelivery::Pull)
        .unwrap();
    h.host.client.request_snapshot(0).unwrap();
    h.pump();
    assert!(h
        .lifecycle
        .iter()
        .any(|m| matches!(m, EngineMessage::SnapshotReply(chunk) if chunk.sequence == sequence)));
    assert!(!h.host.client.snapshot_in_flight());
}

#[shoop_test]
fn an_in_progress_pull_finishes_reliably_after_switching_to_push() {
    use shoop_rt_protocol::snapshot::SnapshotDelivery;
    use shoop_rt_transport::carrier::{Endpoint, Outbox};
    let mut h = Harness::new(&description());
    let track = strip(0, 1);
    h.done(Request::CreateStrip {
        strip: track,
        shape: shape(2, 2, false),
    });
    for index in 0..3 {
        h.done(Request::CreateLoop {
            loop_id: loop_id(index, 1),
            strip: track,
        });
    }
    h.run(16, 16);
    let frame = h.frame();
    let mut out = Outbox::new(1, 2, 1);
    let endpoint = h.host.endpoint();
    endpoint.deliver(ClientMessage::ObserveSnapshots(true), &mut out);
    endpoint.deliver(
        ClientMessage::ConfigureSnapshots(SnapshotDelivery::Pull),
        &mut out,
    );
    endpoint.deliver(ClientMessage::RequestSnapshot { after: 0 }, &mut out);
    endpoint.poll(&mut out);
    assert!(matches!(
        out.pop(),
        Some(EngineMessage::SnapshotConfiguration { .. })
    ));
    endpoint.deliver(
        ClientMessage::ConfigureSnapshots(SnapshotDelivery::push(60).unwrap()),
        &mut out,
    );
    let mut chunks = 0;
    let mut complete = false;
    for _ in 0..20 {
        endpoint.poll(&mut out);
        while let Some(message) = out.pop() {
            match message {
                EngineMessage::SnapshotReply(_) => chunks += 1,
                EngineMessage::SnapshotReady { .. } => complete = true,
                EngineMessage::Snapshot(_) => panic!("a pull response changed lanes mid-transfer"),
                _ => {}
            }
        }
        if complete {
            break;
        }
    }
    assert!(complete);
    assert!(chunks > 1);
    assert_eq!(h.frame(), frame);
}
