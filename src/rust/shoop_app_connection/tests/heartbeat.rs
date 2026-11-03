use core::time::Duration;
use shoop_app_connection::{Heartbeat, HeartbeatConfig};
use shoop_app_protocol::{codec, WireMessage};
use shoop_transport::{CarrierError, RecordPort};
use shoop_wasm_test_support::shoop_test;

#[derive(Default)]
struct Sink {
    messages: Vec<WireMessage>,
    full: bool,
}
impl RecordPort for Sink {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        if self.full {
            return Err(CarrierError::Full);
        }
        self.messages.push(codec::decode(record).unwrap());
        Ok(())
    }
    fn take(&mut self, _: &mut [u8]) -> Option<usize> {
        None
    }
}
#[shoop_test]
fn matching_replies_extend_the_lease_but_stale_or_late_replies_do_not() {
    let mut heartbeat = Heartbeat::new(HeartbeatConfig::default(), 7).unwrap();
    let mut sink = Sink::default();
    heartbeat.advance(Duration::from_secs(1));
    heartbeat.pump(&mut sink).unwrap();
    assert_eq!(sink.messages.pop(), Some(WireMessage::Ping(7)));
    heartbeat.receive(&WireMessage::Pong(6));
    heartbeat.advance(Duration::from_secs(4));
    assert!(!heartbeat.expired());
    heartbeat.receive(&WireMessage::Pong(7));
    heartbeat.advance(Duration::from_secs(1));
    heartbeat.pump(&mut sink).unwrap();
    assert_eq!(sink.messages.pop(), Some(WireMessage::Ping(8)));
    heartbeat.advance(Duration::from_secs(5));
    assert!(heartbeat.expired());
    heartbeat.receive(&WireMessage::Pong(8));
    assert!(heartbeat.expired());
    assert_eq!(heartbeat.pump(&mut sink), Err(CarrierError::Disconnected));
}
#[shoop_test]
fn responses_progress_under_backpressure_and_a_full_or_one_way_link_expires() {
    let config = HeartbeatConfig::default();
    let mut a = Heartbeat::new(config, 1).unwrap();
    let mut b = Heartbeat::new(config, 100).unwrap();
    let mut ab = Sink::default();
    let mut ba = Sink::default();
    for _ in 0..40 {
        a.advance(Duration::from_millis(100));
        b.advance(Duration::from_millis(100));
        a.pump(&mut ab).unwrap();
        b.pump(&mut ba).unwrap();
        for message in ab.messages.drain(..) {
            b.receive(&message);
        }
        for message in ba.messages.drain(..) {
            a.receive(&message);
        }
    }
    assert!(!a.expired() && !b.expired());
    for _ in 0..80 {
        a.advance(Duration::from_millis(100));
        b.advance(Duration::from_millis(100));
        let _ = a.pump(&mut ab);
        let _ = b.pump(&mut ba);
        for message in ab.messages.drain(..) {
            b.receive(&message);
        }
        ba.messages.clear();
    }
    assert!(a.expired() && b.expired());
    let mut blocked = Heartbeat::new(config, 1).unwrap();
    let mut full = Sink {
        full: true,
        ..Default::default()
    };
    blocked.advance(Duration::from_secs(1));
    blocked.pump(&mut full).unwrap();
    blocked.advance(Duration::MAX);
    assert!(blocked.expired());
    assert!(Heartbeat::new(
        HeartbeatConfig {
            interval: Duration::ZERO,
            timeout: Duration::from_secs(1)
        },
        1
    )
    .is_err());
}
