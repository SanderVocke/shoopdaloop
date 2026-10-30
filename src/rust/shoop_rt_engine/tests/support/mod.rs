#![allow(dead_code)] // Shared support is compiled independently by each integration test.

pub use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

use shoop_rt_engine::testing::OfflineHost;
use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use std::collections::BTreeMap;

pub fn description() -> EngineDescription {
    EngineDescription {
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![
            Segment {
                strips: 3,
                loops: 4,
            },
            Segment {
                strips: 1,
                loops: 2,
            },
        ],
        strip_channels: 2,
        master_channels: 2,
        master_connections: vec![(0, 0), (1, 1)],
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

pub fn strip(index: u32, generation: u32) -> StripId {
    StripId(SlotId::new(index, generation))
}
pub fn loop_id(index: u32, generation: u32) -> LoopSlotId {
    LoopSlotId(SlotId::new(index, generation))
}
pub fn shape(inputs: u8, outputs: u8, midi: bool) -> StripShape {
    StripShape {
        audio_inputs: inputs,
        audio_outputs: outputs,
        midi_input: midi,
        midi_output: midi,
    }
}
pub fn port(strip: StripId, kind: PortKind, index: u8) -> EnginePort {
    EnginePort::Strip(StripPort { strip, kind, index })
}
/// Route strip output `channel` to Master channel `target`.
pub fn route(strip: StripId, channel: u8, target: u8, connected: bool) -> Request {
    Request::Route {
        port: StripPort {
            strip,
            kind: PortKind::AudioOut,
            index: channel,
        },
        bus: BusPort {
            bus: BusId::Master,
            index: target,
        },
        connected,
    }
}
pub fn master(index: u8) -> EnginePort {
    EnginePort::Bus(BusPort {
        bus: BusId::Master,
        index,
    })
}
/// Host port identities: audio inputs, audio outputs, MIDI inputs, MIDI outputs.
pub fn audio_in(index: u32) -> HostPortId {
    HostPortId(index)
}
pub fn audio_out(index: u32) -> HostPortId {
    HostPortId(2 + index)
}
pub fn midi_in() -> HostPortId {
    HostPortId(4)
}
pub fn midi_out() -> HostPortId {
    HostPortId(5)
}

type Input = Box<dyn FnMut(usize, u64) -> f32>;

/// The offline host with collected engine messages. Every engine call is
/// heap-guarded; collecting messages happens outside the guard.
pub struct Harness {
    pub host: OfflineHost,
    pub results: BTreeMap<u64, Outcome>,
    pub telemetry: Vec<(u64, Telemetry)>,
    pub states: Vec<StateRecord>,
    pub chunks: Vec<BulkChunk>,
    pub lifecycle: Vec<EngineMessage>,
    pub input: Input,
    /// Output samples of every rendered frame, per host audio output.
    pub outputs: [Vec<f32>; 2],
    pub midi: Vec<(u64, Vec<u8>)>,
    /// MIDI events to feed into the host MIDI input at their frames.
    pub midi_input: Vec<(u64, Vec<u8>)>,
}

impl Harness {
    pub fn new(description: &EngineDescription) -> Self {
        let mut harness = Self {
            host: OfflineHost::new(description).unwrap(),
            results: BTreeMap::new(),
            telemetry: Vec::new(),
            states: Vec::new(),
            chunks: Vec::new(),
            lifecycle: Vec::new(),
            input: Box::new(|_, _| 0.),
            outputs: [Vec::new(), Vec::new()],
            midi: Vec::new(),
            midi_input: Vec::new(),
        };
        assert_no_heap(|| harness.host.connect()).unwrap();
        harness.pump();
        assert!(matches!(
            harness.lifecycle.first(),
            Some(EngineMessage::Welcome { .. })
        ));
        harness
    }
    pub fn frame(&self) -> u64 {
        self.host.next_frame()
    }
    pub fn pump(&mut self) {
        for count in 0.. {
            assert!(count < 100_000, "endless engine messages");
            let Some(message) = assert_no_heap(|| self.host.receive()) else {
                break;
            };
            let frame = self.frame();
            match message {
                EngineMessage::Result {
                    correlation,
                    outcome,
                } => {
                    assert!(
                        self.results.insert(correlation, outcome).is_none(),
                        "one result per request"
                    );
                }
                EngineMessage::Telemetry(record) => self.telemetry.push((frame, record)),
                EngineMessage::State(record) => self.states.push(record),
                EngineMessage::Bulk(chunk) => self.chunks.push(chunk),
                other => self.lifecycle.push(other),
            }
        }
    }
    pub fn send(&mut self, request: Request) -> u64 {
        let correlation = assert_no_heap(|| self.host.request(request)).unwrap();
        self.pump();
        correlation
    }
    /// Render `frames` frames in callbacks of `size` frames, pumping after each.
    pub fn run(&mut self, frames: usize, size: usize) {
        let mut remaining = frames;
        while remaining > 0 {
            let count = remaining.min(size);
            let start = self.frame();
            for port in 0..2 {
                for (offset, sample) in self.host.input_mut(port)[..count].iter_mut().enumerate() {
                    *sample = (self.input)(port, start + offset as u64);
                }
            }
            for (frame, bytes) in &self.midi_input {
                if *frame >= start && *frame < start + count as u64 {
                    self.host
                        .midi_input_mut(0)
                        .push((frame - start) as u32, count, bytes)
                        .unwrap();
                }
            }
            assert_no_heap(|| self.host.process(count)).unwrap();
            for port in 0..2 {
                self.outputs[port].extend_from_slice(&self.host.output(port)[..count]);
            }
            let output = self.host.midi_output(0);
            for index in 0..output.len() {
                let (offset, bytes) = output.get(index).unwrap();
                self.midi.push((start + u64::from(offset), bytes.to_vec()));
            }
            self.pump();
            remaining -= count;
        }
    }
    pub fn idle(&mut self) -> bool {
        self.host.endpoint().is_idle()
    }
    /// Run one-frame callbacks until the engine is idle.
    pub fn settle(&mut self) {
        for _ in 0..1000 {
            if self.idle() {
                return;
            }
            self.run(1, 1);
        }
        panic!("engine did not settle");
    }
    /// Send a request and render callbacks of `size` frames until the engine is idle.
    pub fn apply_in(&mut self, request: Request, size: usize) -> Outcome {
        let correlation = self.send(request);
        for _ in 0..1000 {
            if self.idle() {
                return self.results[&correlation];
            }
            self.run(size, size);
        }
        panic!("engine did not settle");
    }
    /// Send a request, settle and return its outcome.
    pub fn apply(&mut self, request: Request) -> Outcome {
        let correlation = self.send(request);
        self.settle();
        self.results[&correlation]
    }
    pub fn done(&mut self, request: Request) {
        assert_eq!(self.apply(request), Outcome::Done, "{request:?}");
    }
    pub fn last_loop(&self, id: LoopSlotId) -> Option<(u64, Telemetry)> {
        self.telemetry
            .iter()
            .rev()
            .find(|(_, record)| matches!(record, Telemetry::Loop { loop_id, .. } if *loop_id == id))
            .copied()
    }
    /// Read a loop's whole audio channel or MIDI through bulk chunks.
    pub fn read(&mut self, id: LoopSlotId, selector: BulkSelector) -> (u64, Vec<f32>, Vec<u8>) {
        let mut offset = 0;
        let (mut samples, mut bytes) = (Vec::new(), Vec::new());
        for request in 1.. {
            assert_no_heap(|| {
                self.host.bulk(BulkRead {
                    id: request,
                    loop_id: id,
                    selector,
                    offset,
                })
            })
            .unwrap();
            let mut waited = 0;
            let chunk = loop {
                self.pump();
                if let Some(index) = self.chunks.iter().position(|c| c.id == request) {
                    break self.chunks.remove(index);
                }
                waited += 1;
                assert!(waited < 2000, "bulk read {request} at {offset} unanswered");
                self.run(1, 1);
            };
            let len = match chunk.payload {
                BulkPayload::Audio { len, samples: data } => {
                    samples.extend_from_slice(&data[..len as usize]);
                    len as u64
                }
                BulkPayload::Midi { len, bytes: data } => {
                    bytes.extend_from_slice(&data[..len as usize]);
                    len as u64
                }
                BulkPayload::Refused(refusal) => panic!("bulk read refused: {refusal:?}"),
            };
            assert_eq!(chunk.offset, offset);
            offset += len;
            if offset >= chunk.total {
                return (chunk.total, samples, bytes);
            }
        }
        unreachable!()
    }
}
