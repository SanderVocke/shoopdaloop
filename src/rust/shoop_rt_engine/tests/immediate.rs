#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

mod support;
use shoop_rt_protocol::{
    immediate::{Batch, Command},
    snapshot::{Header, Record, Records, HEADER},
    *,
};
use shoop_rt_transport::carrier::CarrierError;
use shoop_rt_transport::client::ClientError;
use shoop_wasm_test_support::shoop_test;
use support::*;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn fixture() -> (Harness, (u64, u64)) {
    fixture_for(&description())
}
fn fixture_for(description: &shoop_rt_engine::EngineDescription) -> (Harness, (u64, u64)) {
    let mut h = Harness::new(description);
    h.done(Request::CreateStrip {
        strip: strip(0, 1),
        shape: shape(2, 2, false),
    });
    h.done(Request::CreateLoop {
        loop_id: loop_id(0, 1),
        strip: strip(0, 1),
    });
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(snapshot::SnapshotDelivery::Pull)
        .unwrap();
    h.host.client.request_snapshot(0).unwrap();
    h.run(16, 16);
    let config = h
        .lifecycle
        .iter()
        .rev()
        .find_map(|m| match m {
            EngineMessage::SnapshotConfiguration { graph, topology } => Some((*graph, *topology)),
            _ => None,
        })
        .unwrap();
    (h, config)
}

#[shoop_test]
fn partial_callbacks_leave_batches_queued_until_the_next_unrendered_quantum() {
    let mut d = description();
    d.quantum = 16;
    let (mut h, c) = fixture_for(&d);
    send(&mut h, c, &[Command::LoopGain(loop_id(0, 1), 0.25)]);
    h.run(1, 1);
    assert_eq!(gain(&mut h), 1.);
    h.run(15, 1);
    assert_eq!(gain(&mut h), 0.25);
}

#[cfg(not(target_arch = "wasm32"))]
#[shoop_test(no_wasm = "native callback timing smoke")]
fn full_callback_scalar_allowance_is_heap_free() {
    use std::time::Instant;
    let mut d = description();
    d.quantum = 16;
    let (mut h, c) = fixture_for(&d);
    let commands = [Command::LoopGain(loop_id(0, 1), 0.5); 32];
    let mut samples = Vec::new();
    for _ in 0..128 {
        for _ in 0..4 {
            send(&mut h, c, &commands);
        }
        let start = Instant::now();
        assert_no_heap(|| h.host.run(16, |_, _| 0.)).unwrap();
        samples.push(start.elapsed().as_nanos());
        h.pump();
    }
    samples.sort_unstable();
    eprintln!(
        "128 scalar commands + callback/snapshot, ns: median={}, max={}",
        samples[64], samples[127]
    );
    assert_eq!(gain(&mut h), 0.5);
}
fn gain(h: &mut Harness) -> f32 {
    h.lifecycle.clear();
    h.host.client.request_snapshot(0).unwrap();
    h.pump();
    let mut bytes = Vec::new();
    for m in &h.lifecycle {
        if let EngineMessage::SnapshotReply(c) = m {
            assert_eq!(bytes.len(), c.offset as usize);
            bytes.extend_from_slice(&c.bytes[..usize::from(c.len)]);
        }
    }
    let header = Header::read(&bytes).unwrap();
    assert!(header.records > 0);
    Records(&bytes[HEADER..])
        .find_map(|r| match r.unwrap() {
            Record::LoopLevels { gain, .. } => Some(gain),
            _ => None,
        })
        .unwrap()
}
fn send(h: &mut Harness, config: (u64, u64), commands: &[Command]) {
    assert_no_heap(|| {
        h.host
            .client
            .immediate(Batch::new(config.0, config.1, commands).unwrap())
    })
    .unwrap();
}

#[shoop_test]
fn invalid_target_rejects_whole_batch_and_exclusive_mode_blocks_legacy_edits() {
    let (mut h, c) = fixture();
    send(
        &mut h,
        c,
        &[
            Command::LoopGain(loop_id(0, 1), 0.25),
            Command::OutputGain(strip(0, 2), 0.5),
        ],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 1.);
    assert!(h
        .telemetry
        .iter()
        .any(|(_, t)| matches!(t,Telemetry::Immediate(d) if d.rt_refused == 1 && d.last_rt == 3)));
    send(&mut h, c, &[Command::LoopGain(loop_id(0, 1), 0.25)]);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.25);
    assert_eq!(
        h.apply(Request::LoopGain {
            loop_id: loop_id(0, 1),
            gain: 0.75
        }),
        Outcome::Refused(Refusal::Busy)
    );
    assert_eq!(
        h.apply(Request::RemoveLoop {
            loop_id: loop_id(0, 1)
        }),
        Outcome::Refused(Refusal::Busy)
    );
    send(
        &mut h,
        (c.0 + 1, c.1),
        &[Command::LoopGain(loop_id(0, 1), 0.75)],
    );
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.25);
}

#[shoop_test]
fn mixed_scalars_publish_a_single_consistent_snapshot() {
    let (mut h, c) = fixture();
    let s = strip(0, 1);
    let l = loop_id(0, 1);
    let b = BusId::MASTER;
    send(
        &mut h,
        c,
        &[
            Command::LoopGain(l, 0.25),
            Command::LoopBalance(l, -0.5),
            Command::InputGain(s, 0.5),
            Command::InputBalance(s, 0.25),
            Command::OutputGain(s, 0.75),
            Command::OutputBalance(s, -0.25),
            Command::OutputMute(s, true),
            Command::BusGain(b, 0.5),
            Command::BusBalance(b, 0.25),
            Command::BusMute(b, true),
        ],
    );
    h.run(16, 16);
    let mut bytes = vec![0; h.host.endpoint().snapshot_capacity()];
    let (_, len) = h.host.endpoint().read_snapshot(&mut bytes).unwrap();
    let records: Vec<_> = Records(&bytes[HEADER..len]).map(Result::unwrap).collect();
    assert!(records.contains(&Record::LoopLevels {
        id: l,
        gain: 0.25,
        balance: -0.5,
        repeat_sync: false
    }));
    for control in [
        StripControl::InputGain(0.5),
        StripControl::InputBalance(0.25),
        StripControl::OutputGain(0.75),
        StripControl::OutputBalance(-0.25),
        StripControl::OutputMute(true),
    ] {
        assert!(records.contains(&Record::State(StateRecord::StripControl {
            strip: s,
            control
        })));
    }
    for control in [
        BusControl::Gain(0.5),
        BusControl::Balance(0.25),
        BusControl::Mute(true),
    ] {
        assert!(records.contains(&Record::State(StateRecord::Bus { bus: b, control })));
    }
}

#[shoop_test]
fn bounded_staging_preserves_order_and_disconnect_does_not_replay_unsent_work() {
    let (mut h, c) = fixture();
    for i in 0..8 {
        send(
            &mut h,
            c,
            &[Command::LoopGain(loop_id(0, 1), i as f32 / 10.)],
        );
    }
    let batch = Batch::new(c.0, c.1, &[Command::LoopGain(loop_id(0, 1), 0.9)]).unwrap();
    assert_eq!(
        h.host.client.immediate(batch),
        Err(ClientError::Carrier(CarrierError::Full))
    );
    assert_eq!(h.host.client.reliable_in_flight(), 0);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.3);
    h.run(16, 16);
    assert_eq!(gain(&mut h), 0.7);
    for i in 0..8 {
        send(
            &mut h,
            c,
            &[Command::LoopGain(loop_id(0, 1), i as f32 / 10.)],
        );
    }
    h.host.disconnect();
    h.pump();
    h.host.connect().unwrap();
    h.pump();
    h.host.client.observe_snapshots(true).unwrap();
    h.host
        .client
        .configure_snapshots(snapshot::SnapshotDelivery::Pull)
        .unwrap();
    h.run(16, 16);
    h.run(16, 16);
    assert_eq!(
        gain(&mut h),
        0.3,
        "only the four RT-admitted groups survive disconnect"
    );
    assert_eq!(
        h.apply(Request::RemoveLoop {
            loop_id: loop_id(0, 1)
        }),
        Outcome::Refused(Refusal::Busy)
    );
}

#[shoop_test]
fn encoded_fragmentation_and_paused_receiver_preserve_bounded_command_delivery() {
    use shoop_rt_transport::{
        buffer::Stream,
        carrier::{RecordPort, RecordServer},
        framing::record_len,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    struct Port {
        input: Rc<RefCell<Stream>>,
        taken: Rc<Cell<usize>>,
        paused: Rc<Cell<bool>>,
        closed: Rc<Cell<bool>>,
    }
    impl RecordPort for Port {
        fn post(&mut self, _: &[u8]) -> Result<(), CarrierError> {
            if self.paused.get() {
                Err(CarrierError::Full)
            } else {
                Ok(())
            }
        }
        fn take(&mut self, out: &mut [u8]) -> Option<usize> {
            let mut stream = self.input.borrow_mut();
            let len = record_len(stream.pending()).unwrap()?;
            out[..len].copy_from_slice(&stream.pending()[..len]);
            stream.consume(len);
            stream.compact();
            self.taken.set(self.taken.get() + 1);
            Some(len)
        }
        fn failure(&mut self) -> Option<CarrierError> {
            self.closed.get().then_some(CarrierError::Disconnected)
        }
    }
    fn append(input: &Rc<RefCell<Stream>>, data: &[u8]) {
        let mut stream = input.borrow_mut();
        stream.free_mut()[..data.len()].copy_from_slice(data);
        assert!(stream.commit(data.len()));
    }
    fn observed(endpoint: &shoop_rt_engine::ControlEndpoint) -> f32 {
        let mut bytes = vec![0; 100_000];
        let (_, len) = endpoint.read_snapshot(&mut bytes).unwrap();
        Records(&bytes[HEADER..len])
            .find_map(|r| match r.unwrap() {
                Record::LoopLevels { gain, .. } => Some(gain),
                _ => None,
            })
            .unwrap()
    }
    let (h, c) = fixture();
    let (mut offline, client) = h.host.into_parts();
    let (endpoint, out) = client.into_carrier().into_parts();
    let input = Rc::new(RefCell::new(Stream::new(16 * MAX_RECORD_BYTES)));
    let taken = Rc::new(Cell::new(0));
    let paused = Rc::new(Cell::new(true));
    let closed = Rc::new(Cell::new(false));
    let mut server = RecordServer::new(
        endpoint,
        Port {
            input: input.clone(),
            taken: taken.clone(),
            paused: paused.clone(),
            closed: closed.clone(),
        },
        out,
    );
    let mut bytes = [0; MAX_RECORD_BYTES];
    let batch = Batch::new(c.0, c.1, &[Command::LoopGain(loop_id(0, 1), 0.25)]).unwrap();
    let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
    append(&input, &bytes[..len - 1]);
    assert_no_heap(|| server.pump(16));
    assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
    assert_eq!(taken.get(), 0);
    assert_eq!(observed(server.endpoint()), 1.);
    append(&input, &bytes[len - 1..len]);
    assert_no_heap(|| server.pump(16));
    assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
    assert_eq!(observed(server.endpoint()), 0.25);
    for i in 0..10 {
        let batch = Batch::new(
            c.0,
            c.1,
            &[Command::LoopGain(loop_id(0, 1), i as f32 / 10.)],
        )
        .unwrap();
        let len = codec::encode_client(&ClientMessage::Immediate(batch), &mut bytes).unwrap();
        append(&input, &bytes[..len]);
    }
    assert_no_heap(|| server.pump(16));
    let blocked = taken.get();
    assert_eq!(
        blocked, 10,
        "one initial, eight staged, one retained record"
    );
    assert_no_heap(|| server.pump(16));
    assert_eq!(taken.get(), blocked);
    for _ in 0..5 {
        assert_no_heap(|| offline.run(16, |_, _| 0.)).unwrap();
        assert_no_heap(|| server.pump(16));
    }
    assert_eq!(observed(server.endpoint()), 0.9);
    assert_eq!(taken.get(), 11);
    assert!(input.borrow().pending().is_empty());
    paused.set(false);
    assert_no_heap(|| server.pump(16));
    closed.set(true);
    assert_no_heap(|| server.pump(16));
    assert_eq!(server.failure(), Some(CarrierError::Disconnected));
}
