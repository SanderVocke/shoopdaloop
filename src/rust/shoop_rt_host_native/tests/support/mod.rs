#![allow(dead_code)] // Shared support is compiled independently by each integration test.

#[cfg(feature = "cpal")]
pub mod cpal_mock;

use std::any::Any;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub use shoop_wasm_test_support::heap::{assert_no_heap, measure, TrackingAllocator};

use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_host_native::{MidiInputSender, MidiSink, MidiSource, NativeClient};
use shoop_rt_protocol::*;

pub fn description() -> EngineDescription {
    EngineDescription {
        sample_rate: 48_000,
        max_frames: 64,
        host: HostPorts::numbered(0, 0, 0),
        segments: vec![Segment {
            strips: 4,
            loops: 4,
        }],
        strip_channels: 2,
        master_channels: 2,
        master_connections: Vec::new(),
        smoothing_frames: 0,
        smoothing_limit_frames: 8,
        ramp_frames: 4,
        control_lead_frames: 0,
        resource_pages: 512,
        page_bytes: 256,
        midi_events: 64,
        midi_bytes: 512,
        pending_transitions: 32,
        reliable_credits: 8,
        bulk_credits: 2,
        telemetry_interval_frames: 0,
    }
}

pub fn strip(index: u32) -> StripId {
    StripId(SlotId::new(index, 1))
}
pub fn loop_id(index: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(index, 1))
}
pub fn shape(audio: u8, midi: bool) -> StripShape {
    StripShape {
        audio_inputs: audio,
        audio_outputs: audio,
        midi_input: midi,
        midi_output: midi,
    }
}
pub fn connect(strip: StripId, kind: PortKind, host: u32) -> Request {
    Request::Connect {
        port: EnginePort::Strip(StripPort {
            strip,
            kind,
            index: 0,
        }),
        host: HostPortId(host),
        connected: true,
    }
}
pub fn monitoring(strip: StripId) -> Request {
    Request::StripControl {
        strip,
        control: StripControl::Monitoring(true),
    }
}

/// Allocation checks would pass vacuously without the tracking allocator.
pub fn assert_heap_guard_is_live() {
    let ((), counts) = measure(|| drop(std::hint::black_box(vec![0u8; 16])));
    assert_eq!(counts.allocations, 1);
    assert_eq!(counts.deallocations, 1);
}

/// A MIDI source whose sender the test pushes into.
#[derive(Clone, Default)]
pub struct TestSource(pub Arc<Mutex<Option<MidiInputSender>>>);

impl MidiSource for TestSource {
    fn connect(self: Box<Self>, sender: MidiInputSender) -> Result<Box<dyn Any + Send>, String> {
        *self.0.lock().unwrap() = Some(sender);
        Ok(Box::new(()))
    }
}

impl TestSource {
    pub fn push(&self, message: &[u8]) -> bool {
        let mut sender = self.0.lock().unwrap();
        let sender = sender.as_mut().expect("connected");
        assert_no_heap(|| sender.push(message))
    }
}

/// A MIDI sink that records what the pump sends.
#[derive(Clone, Default)]
pub struct TestSink(pub Arc<Mutex<Vec<Vec<u8>>>>);

impl MidiSink for TestSink {
    fn send(&mut self, message: &[u8]) -> bool {
        self.0.lock().unwrap().push(message.to_vec());
        true
    }
}

impl TestSink {
    pub fn sent(&self) -> Vec<Vec<u8>> {
        self.0.lock().unwrap().clone()
    }
    /// Wait until `count` messages were sent.
    pub fn wait_for(&self, count: usize) -> Vec<Vec<u8>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.0.lock().unwrap().len() < count && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        self.sent()
    }
}

/// What a client has received.
#[derive(Default)]
pub struct Received {
    pub results: BTreeMap<u64, Outcome>,
    pub telemetry: Vec<Telemetry>,
    pub state: Vec<StateRecord>,
    pub lifecycle: Vec<EngineMessage>,
}

impl Received {
    pub fn take(&mut self, client: &mut NativeClient) {
        while let Some(message) = client.poll() {
            match message {
                EngineMessage::Result {
                    correlation,
                    outcome,
                } => {
                    self.results.insert(correlation, outcome);
                }
                EngineMessage::Telemetry(telemetry) => self.telemetry.push(telemetry),
                EngineMessage::State(record) => self.state.push(record),
                other => self.lifecycle.push(other),
            }
        }
    }

    pub fn last_engine(&self) -> Option<(u64, u32, DriverStatus)> {
        self.telemetry
            .iter()
            .rev()
            .find_map(|telemetry| match *telemetry {
                Telemetry::Engine {
                    frame,
                    xruns,
                    driver,
                    ..
                } => Some((frame, xruns, driver)),
                _ => None,
            })
    }

    pub fn strip_peaks(&self, strip: StripId) -> Option<(Peaks, Peaks)> {
        self.telemetry
            .iter()
            .rev()
            .find_map(|telemetry| match *telemetry {
                Telemetry::Strip {
                    strip: id,
                    input_peaks,
                    output_peaks,
                    ..
                } if id == strip => Some((input_peaks, output_peaks)),
                _ => None,
            })
    }
}

/// Poll a client of a threaded driver until `done` holds, failing after a few seconds.
pub fn until(
    client: &mut NativeClient,
    received: &mut Received,
    mut done: impl FnMut(&Received) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        received.take(client);
        if done(received) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}

pub fn connected(client: &mut NativeClient, received: &mut Received) {
    client.connect().unwrap();
    until(client, received, |r| {
        r.lifecycle
            .iter()
            .any(|message| matches!(message, EngineMessage::Welcome { .. }))
    });
}

pub fn request_done(client: &mut NativeClient, received: &mut Received, request: Request) {
    let correlation = client.request(request).unwrap();
    until(client, received, |r| r.results.contains_key(&correlation));
    assert_eq!(received.results[&correlation], Outcome::Done, "{request:?}");
}
