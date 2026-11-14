mod support;
use shoop_backend::{Backend, BackendTrackControl, DirectTrackRequest};
use shoop_rt_backend::{Observation, SnapshotDelivery};
use shoop_wasm_test_support::shoop_test;
use support::*;

#[shoop_test]
fn snapshots_drive_display_controls_and_legacy_remains_selectable() {
    let mut backend = stack(&description());
    assert_eq!(backend.observation(), Observation::Snapshots);
    assert_eq!(
        backend.snapshot_delivery(),
        if cfg!(target_arch = "wasm32") {
            SnapshotDelivery::default()
        } else {
            SnapshotDelivery::Pull
        }
    );
    let track = backend
        .create_direct_track(DirectTrackRequest {
            port_name_base: "snapshots".into(),
            audio_channels: 2,
            midi: false,
            initial_loops: 2,
        })
        .unwrap();
    backend.wait_idle();
    backend
        .set_track_control(track.track_id, BackendTrackControl::OutputBalance(0.25))
        .unwrap();
    backend.set_loop_gain(track.loops[0], 0.25).unwrap();
    backend.set_loop_balance(track.loops[0], -0.5).unwrap();
    backend.wait_idle();
    let snapshot = backend.poll().unwrap();
    assert_eq!(snapshot.tracks[&track.track_id].output_balance, 0.25);
    assert_eq!(snapshot.loops[&track.loops[0]].gain, 0.25);
    assert_eq!(snapshot.loops[&track.loops[0]].balance, -0.5);
    assert!(backend.snapshot_frame().is_some());
    assert!(snapshot.mutation_failures.is_empty());
    let pull_frame = backend.snapshot_frame().unwrap();
    backend
        .set_snapshot_delivery(SnapshotDelivery::push(120).unwrap())
        .unwrap();
    backend.clock_mut().render(32);
    backend.pump();
    assert!(backend.snapshot_frame().unwrap() > pull_frame);
    backend
        .set_snapshot_delivery(SnapshotDelivery::Pull)
        .unwrap();
    backend.set_observation(Observation::Legacy).unwrap();
    backend
        .set_track_control(track.track_id, BackendTrackControl::OutputBalance(-0.5))
        .unwrap();
    backend.wait_idle();
    assert_eq!(
        backend.poll().unwrap().tracks[&track.track_id].output_balance,
        -0.5
    );
    backend.set_observation(Observation::Snapshots).unwrap();
    backend.clock_mut().render(16);
    backend.pump();
    assert_eq!(
        backend.poll().unwrap().tracks[&track.track_id].output_balance,
        -0.5
    );
}

#[shoop_test]
fn encoded_delivery_defaults_to_push_and_supports_nonblocking_pull() {
    use shoop_rt_backend::RtBackend;
    use shoop_rt_protocol::{codec, ClientMessage, EngineMessage, MAX_RECORD_BYTES};
    use shoop_rt_transport::{
        carrier::{Carrier, CarrierError},
        client::ProtocolClient,
    };
    use std::{cell::Cell, rc::Rc};
    struct Encoded<C> {
        inner: C,
        bytes: [u8; MAX_RECORD_BYTES],
        paused: Rc<Cell<bool>>,
        requests: Rc<Cell<usize>>,
    }
    impl<C: Carrier> Carrier for Encoded<C> {
        fn send(&mut self, message: ClientMessage) -> Result<(), CarrierError> {
            if matches!(message, ClientMessage::RequestSnapshot { .. }) {
                self.requests.set(self.requests.get() + 1);
            }
            let len = codec::encode_client(&message, &mut self.bytes).unwrap();
            self.inner
                .send(codec::decode_client(&self.bytes[..len]).unwrap())
        }
        fn receive(&mut self) -> Option<EngineMessage> {
            if self.paused.get() {
                return None;
            }
            let message = self.inner.receive()?;
            let len = codec::encode_engine(&message, &mut self.bytes).unwrap();
            Some(codec::decode_engine(&self.bytes[..len]).unwrap())
        }
    }
    let paused = Rc::new(Cell::new(false));
    let requests = Rc::new(Cell::new(0));
    let (clock, client) = parts(&description());
    let mut backend = RtBackend::connect(
        ProtocolClient::new(Encoded {
            inner: client.into_carrier(),
            bytes: [0; MAX_RECORD_BYTES],
            paused: paused.clone(),
            requests: requests.clone(),
        }),
        clock,
    )
    .unwrap();
    assert_eq!(backend.snapshot_delivery(), SnapshotDelivery::default());
    backend.wait_idle();
    assert_eq!(requests.get(), 0);
    let first = backend.snapshot_frame().unwrap();
    backend
        .set_snapshot_delivery(SnapshotDelivery::Pull)
        .unwrap();
    paused.set(true);
    let frame = backend.clock().frame();
    for _ in 0..20 {
        backend.pump();
    }
    assert_eq!(requests.get(), 1);
    assert_eq!(backend.clock().frame(), frame);
    assert_eq!(backend.snapshot_frame(), Some(first));
    backend.clock_mut().render(32);
    paused.set(false);
    backend.pump();
    assert_eq!(requests.get(), 1);
    assert!(backend.snapshot_frame().unwrap() > first);
    backend.clock_mut().render(32);
    backend.pump();
    assert_eq!(requests.get(), 2);
    backend
        .set_snapshot_delivery(SnapshotDelivery::push(120).unwrap())
        .unwrap();
    let before = backend.snapshot_frame().unwrap();
    backend.clock_mut().render(32);
    backend.pump();
    assert_eq!(requests.get(), 2);
    assert!(backend.snapshot_frame().unwrap() > before);
}
