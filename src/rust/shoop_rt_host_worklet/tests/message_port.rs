//! The worklet host driven as the AudioWorklet runs it. A protocol client on the
//! main thread exchanges `postMessage` batches with a stand-in for the processor
//! script, which copies posted bytes into the module's buffers as space allows,
//! renders a quantum and posts what the module collected. Every module call is
//! heap-guarded; the message plumbing around it allocates like JavaScript does.

use std::collections::{BTreeMap, VecDeque};

use shoop_rt_engine::{EngineDescription, HostPorts, Segment};
use shoop_rt_host_worklet::lane::{self, MidiEvent, MAX_EVENT_BYTES};
use shoop_rt_host_worklet::{
    framing, Counters, Inbound, Outbound, Quantum, QuantumError, WorkletConfig, WorkletHost,
};
use shoop_rt_protocol::carrier::{CarrierError, RecordClient, RecordPort};
use shoop_rt_protocol::client::{ClientState, ProtocolClient};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use shoop_wasm_test_support::shoop_test;

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

const QUANTUM: usize = 16;
/// `currentFrame` when the module is prepared.
const START: u64 = 48_000;

fn description() -> EngineDescription {
    EngineDescription {
        sample_rate: 1000,
        max_frames: QUANTUM,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 2,
            loops: 4,
        }],
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

const AUDIO_IN: HostPortId = HostPortId(0);
const AUDIO_OUT: HostPortId = HostPortId(2);
const MIDI_IN: HostPortId = HostPortId(4);
const MIDI_OUT: HostPortId = HostPortId(5);

fn port(strip: StripId, kind: PortKind) -> EnginePort {
    EnginePort::Strip(StripPort {
        strip,
        kind,
        index: 0,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Kind {
    Records,
    Midi,
}

struct Message {
    kind: Kind,
    bytes: Vec<u8>,
}

/// The main thread's record port: records posted during a tick leave as one
/// batch; received batches are split into records.
#[derive(Default)]
struct MainPort {
    outgoing: Vec<u8>,
    received: VecDeque<Vec<u8>>,
}

impl RecordPort for MainPort {
    fn post(&mut self, record: &[u8]) -> Result<(), CarrierError> {
        self.outgoing.extend_from_slice(record);
        Ok(())
    }
    fn take(&mut self, buf: &mut [u8]) -> Option<usize> {
        let record = self.received.pop_front()?;
        buf[..record.len()].copy_from_slice(&record);
        Some(record.len())
    }
}

/// What the processor script does around the module.
struct Processor {
    host: WorkletHost,
    pending: [VecDeque<Vec<u8>>; 2],
    /// `currentFrame` of the next quantum.
    frame: u64,
    load: Option<f32>,
}

impl Processor {
    fn receive(&mut self, message: Message) {
        let lane = message.kind as usize;
        self.pending[lane].push_back(message.bytes);
    }

    fn feed(&mut self, kind: Kind) {
        let lane = match kind {
            Kind::Records => Inbound::Records,
            Kind::Midi => Inbound::Midi,
        };
        let host = &mut self.host;
        let queue = &mut self.pending[kind as usize];
        while let Some(head) = queue.front_mut() {
            let written = assert_no_heap(|| host.write(lane, head));
            if written < head.len() {
                head.drain(..written);
                break;
            }
            queue.pop_front();
        }
    }

    fn run(
        &mut self,
        frames: usize,
        input: &dyn Fn(usize, u64) -> f32,
        posted: &mut VecDeque<Message>,
        outputs: &mut [Vec<f32>],
    ) -> Result<(), QuantumError> {
        self.feed(Kind::Records);
        self.feed(Kind::Midi);
        let stride = self.host.max_frames();
        let channels = self.host.audio_inputs();
        let samples = self.host.audio_input_mut();
        for channel in 0..channels {
            for offset in 0..frames {
                samples[channel * stride + offset] = input(channel, self.frame + offset as u64);
            }
        }
        let quantum = Quantum {
            frames,
            input_channels: channels,
            frame: self.frame,
            load: self.load,
        };
        let result = assert_no_heap(|| self.host.process(quantum));
        self.frame += frames as u64;
        let output = self.host.audio_output();
        for (channel, collected) in outputs.iter_mut().enumerate() {
            collected.extend_from_slice(&output[channel * stride..channel * stride + frames]);
        }
        for (lane, kind) in [
            (Outbound::Records, Kind::Records),
            (Outbound::Midi, Kind::Midi),
        ] {
            let bytes = self.host.read(lane, <[u8]>::to_vec);
            if !bytes.is_empty() {
                posted.push_back(Message { kind, bytes });
            }
        }
        result
    }
}

/// The main thread, the message channel and the processor.
struct Browser {
    client: ProtocolClient<RecordClient<MainPort>>,
    processor: Processor,
    to_worklet: VecDeque<Message>,
    to_main: VecDeque<Message>,
    input: Box<dyn Fn(usize, u64) -> f32>,
    results: BTreeMap<u64, Outcome>,
    telemetry: Vec<Telemetry>,
    chunks: Vec<BulkChunk>,
    lifecycle: Vec<EngineMessage>,
    outputs: Vec<Vec<f32>>,
    midi_out: Vec<(u64, u8, Vec<u8>)>,
}

impl Browser {
    fn new(config: &WorkletConfig) -> Self {
        let host = WorkletHost::prepare(config).unwrap();
        let outputs = vec![Vec::new(); host.audio_outputs()];
        Self {
            client: ProtocolClient::new(RecordClient::new(MainPort::default())),
            processor: Processor {
                host,
                pending: Default::default(),
                frame: START,
                load: None,
            },
            to_worklet: VecDeque::new(),
            to_main: VecDeque::new(),
            input: Box::new(|_, _| 0.),
            results: BTreeMap::new(),
            telemetry: Vec::new(),
            chunks: Vec::new(),
            lifecycle: Vec::new(),
            outputs,
            midi_out: Vec::new(),
        }
    }

    fn connected(config: &WorkletConfig) -> Self {
        let mut browser = Self::new(config);
        browser.client.connect().unwrap();
        browser.run(QUANTUM);
        assert_eq!(browser.client.state(), ClientState::Connected);
        browser
    }

    fn counters(&self) -> Counters {
        self.processor.host.counters()
    }

    fn post_midi(&mut self, events: &[(u64, u8, &[u8])]) {
        let mut bytes = vec![0; events.len() * MAX_EVENT_BYTES];
        let mut len = 0;
        for &(frame, port, message) in events {
            let event = MidiEvent {
                frame,
                port,
                bytes: message,
            };
            len += event.encode(&mut bytes[len..]).unwrap();
        }
        bytes.truncate(len);
        self.to_worklet.push_back(Message {
            kind: Kind::Midi,
            bytes,
        });
    }

    /// One main-thread turn and one render quantum.
    fn tick(&mut self, frames: usize) -> Result<(), QuantumError> {
        let port = self.client.carrier_mut().port_mut();
        if !port.outgoing.is_empty() {
            self.to_worklet.push_back(Message {
                kind: Kind::Records,
                bytes: std::mem::take(&mut port.outgoing),
            });
        }
        while let Some(message) = self.to_worklet.pop_front() {
            self.processor.receive(message);
        }
        let result = self
            .processor
            .run(frames, &*self.input, &mut self.to_main, &mut self.outputs);
        while let Some(message) = self.to_main.pop_front() {
            match message.kind {
                Kind::Records => {
                    let port = self.client.carrier_mut().port_mut();
                    for record in framing::records(&message.bytes) {
                        port.received.push_back(record.unwrap().to_vec());
                    }
                }
                Kind::Midi => {
                    for event in lane::events(&message.bytes) {
                        let event = event.unwrap();
                        self.midi_out
                            .push((event.frame, event.port, event.bytes.to_vec()));
                    }
                }
            }
        }
        while let Some(message) = self.client.poll() {
            match message {
                EngineMessage::Result {
                    correlation,
                    outcome,
                } => {
                    assert!(self.results.insert(correlation, outcome).is_none());
                }
                EngineMessage::Telemetry(record) => self.telemetry.push(record),
                EngineMessage::Bulk(chunk) => self.chunks.push(chunk),
                other => self.lifecycle.push(other),
            }
        }
        result
    }

    fn run(&mut self, frames: usize) {
        for _ in 0..frames.div_ceil(QUANTUM) {
            self.tick(QUANTUM).unwrap();
        }
    }

    fn outcome(&mut self, request: Request) -> Outcome {
        let correlation = self.client.request(request).unwrap();
        for _ in 0..1000 {
            if let Some(&outcome) = self.results.get(&correlation) {
                return outcome;
            }
            self.run(QUANTUM);
        }
        panic!("{request:?} unanswered");
    }

    fn done(&mut self, request: Request) {
        assert_eq!(self.outcome(request), Outcome::Done, "{request:?}");
    }

    fn read(&mut self, id: LoopSlotId, channel: u8) -> Vec<f32> {
        let mut samples = Vec::new();
        for request in 1.. {
            self.client
                .bulk(BulkRead {
                    id: request,
                    loop_id: id,
                    selector: BulkSelector::Audio { channel },
                    offset: samples.len() as u64,
                })
                .unwrap();
            let chunk = loop {
                if let Some(index) = self.chunks.iter().position(|c| c.id == request) {
                    break self.chunks.remove(index);
                }
                self.run(QUANTUM);
            };
            let BulkPayload::Audio { len, samples: data } = chunk.payload else {
                panic!("bulk read refused: {:?}", chunk.payload);
            };
            samples.extend_from_slice(&data[..len as usize]);
            if samples.len() as u64 >= chunk.total {
                return samples;
            }
        }
        unreachable!()
    }

    fn last_engine(&self) -> (u32, f32, DriverStatus) {
        self.telemetry
            .iter()
            .rev()
            .find_map(|record| match *record {
                Telemetry::Engine {
                    xruns,
                    load,
                    driver,
                    ..
                } => Some((xruns, load, driver)),
                _ => None,
            })
            .unwrap()
    }
}

/// A strip with one audio channel each way and MIDI, connected to the first
/// host ports, with one loop.
fn track(browser: &mut Browser, monitoring: bool) -> (StripId, LoopSlotId) {
    let track = StripId(SlotId::new(0, 1));
    let looped = LoopSlotId(SlotId::new(0, 1));
    browser.done(Request::CreateStrip {
        strip: track,
        shape: StripShape {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_input: true,
            midi_output: true,
        },
    });
    for (kind, host) in [
        (PortKind::AudioIn, AUDIO_IN),
        (PortKind::AudioOut, AUDIO_OUT),
        (PortKind::MidiIn, MIDI_IN),
        (PortKind::MidiOut, MIDI_OUT),
    ] {
        browser.done(Request::Connect {
            port: port(track, kind),
            host,
            connected: true,
        });
    }
    browser.done(Request::StripControl {
        strip: track,
        control: StripControl::Monitoring(monitoring),
    });
    browser.done(Request::CreateLoop {
        loop_id: looped,
        strip: track,
    });
    (track, looped)
}

fn transition(loop_id: LoopSlotId, mode: LoopMode) -> Request {
    Request::LoopTransition {
        loop_id,
        mode,
        delay: None,
    }
}

#[shoop_test]
fn records_and_plays_a_loop_through_the_message_port() {
    let mut browser = Browser::connected(&WorkletConfig::new(description()));
    let Some(&EngineMessage::Welcome { limits, driver, .. }) = browser.lifecycle.first() else {
        panic!("no welcome: {:?}", browser.lifecycle);
    };
    assert_eq!((limits.strips, limits.loops, limits.max_frames), (2, 4, 16));
    assert_eq!(driver.kind, DriverKind::WebAudio);
    let (_, looped) = track(&mut browser, false);

    browser.input = Box::new(|channel, frame| {
        if channel == 0 {
            (frame - START) as f32
        } else {
            0.
        }
    });
    browser.done(transition(looped, LoopMode::Recording));
    browser.run(40);
    browser.input = Box::new(|_, _| 0.);
    let played = browser.outputs[0].len();
    browser.done(transition(looped, LoopMode::Playing));
    let content = browser.read(looped, 0);
    let length = content.len();
    assert!(length >= 40, "recorded {length} frames");
    assert!(content.windows(2).all(|pair| pair[1] == pair[0] + 1.));
    browser.run(3 * length);
    let output = &browser.outputs[0][played..];
    let start = output.iter().position(|&s| s == content[0]).unwrap();
    assert_eq!(output[start..start + 2 * length], content.repeat(2));
    assert!(browser.telemetry.iter().rev().any(|record| matches!(
        *record,
        Telemetry::Loop { loop_id, mode: LoopMode::Playing, length: reported, .. }
            if loop_id == looped && reported == length as u64
    )));
    assert_eq!(browser.counters(), Counters::default());
}

#[shoop_test]
fn reports_driver_state_load_and_xruns_in_telemetry() {
    let mut browser = Browser::connected(&WorkletConfig::new(description()));
    browser.processor.load = Some(0.25);
    browser.run(QUANTUM);
    let (xruns, load, driver) = browser.last_engine();
    assert_eq!((xruns, load), (0, 0.25));
    assert_eq!(driver.state, DriverRunState::Running);
    assert_eq!(driver.buffer_frames, QUANTUM as u32);

    browser.processor.frame += 5;
    browser.run(QUANTUM);
    browser.processor.load = Some(1.5);
    browser.run(QUANTUM);
    browser.processor.load = None;
    browser.run(QUANTUM);
    assert_eq!(browser.last_engine().0, 2);
    let counters = browser.counters();
    assert_eq!((counters.discontinuities, counters.overruns), (1, 1));
}

#[shoop_test]
fn delivers_timestamped_midi_and_returns_monitored_output() {
    let mut browser = Browser::connected(&WorkletConfig::new(description()));
    track(&mut browser, true);
    let next = browser.processor.frame;
    let ahead = next + 2 * QUANTUM as u64 + 3;
    browser.post_midi(&[
        (ahead, 0, &[0x90, 60, 100]),
        (next + 1, 1, &[0x90, 61, 100]),
        (next - 7, 0, &[0x90, 62, 100]),
        (next + 2, 0, &[0x90]),
    ]);
    browser.run(4 * QUANTUM);
    assert_eq!(
        browser.midi_out,
        [
            (next, 0, vec![0x90, 62, 100]),
            (ahead, 0, vec![0x90, 60, 100])
        ]
    );
    let counters = browser.counters();
    assert_eq!((counters.midi_in_late, counters.midi_in_dropped), (1, 2));

    let mut lane = Vec::new();
    lane.extend_from_slice(&[0; lane::EVENT_HEADER_BYTES]);
    browser.to_worklet.push_back(Message {
        kind: Kind::Midi,
        bytes: lane,
    });
    let next = browser.processor.frame;
    browser.post_midi(&[(next, 0, &[0x80, 60, 0])]);
    browser.run(QUANTUM);
    assert_eq!(browser.counters().framing_errors, 1);
    browser.post_midi(&[(next + QUANTUM as u64, 0, &[0x80, 60, 0])]);
    browser.run(QUANTUM);
    // The event behind the malformed one in the same stream was discarded.
    assert_eq!(browser.midi_out.len(), 3);
    assert_eq!(
        browser.midi_out.last(),
        Some(&(next + QUANTUM as u64, 0, vec![0x80, 60, 0]))
    );
}

#[shoop_test]
fn bounds_midi_output_per_quantum() {
    let config = WorkletConfig {
        midi_out_bytes: MAX_EVENT_BYTES,
        ..WorkletConfig::new(description())
    };
    let mut browser = Browser::connected(&config);
    track(&mut browser, true);
    let next = browser.processor.frame;
    let notes: Vec<[u8; 3]> = (0..30).map(|note| [0x90, note, 100]).collect();
    let events: Vec<(u64, u8, &[u8])> = notes.iter().map(|note| (next, 0, &note[..])).collect();
    browser.post_midi(&events);
    browser.run(QUANTUM);
    let fitting = MAX_EVENT_BYTES / (lane::EVENT_HEADER_BYTES + 3);
    assert_eq!(browser.midi_out.len(), fitting);
    assert_eq!(browser.counters().midi_out_dropped as usize, 30 - fitting);
}

#[shoop_test]
fn delivers_a_bounded_number_of_records_per_quantum() {
    let config = WorkletConfig {
        records_in_bytes: MAX_RECORD_BYTES,
        records_per_quantum: 2,
        ..WorkletConfig::new(EngineDescription {
            reliable_credits: 64,
            ..description()
        })
    };
    let mut browser = Browser::connected(&config);
    let missing = LoopSlotId(SlotId::new(1, 1));
    let correlations: Vec<u64> = (0..64)
        .map(|_| {
            browser
                .client
                .request(Request::LoopGain {
                    loop_id: missing,
                    gain: 0.5,
                })
                .unwrap()
        })
        .collect();
    assert!(browser.client.carrier_mut().port_mut().outgoing.len() > MAX_RECORD_BYTES);
    for quantum in 1..=32 {
        browser.run(QUANTUM);
        assert!(browser.results.len() <= 2 * quantum);
    }
    browser.run(4 * QUANTUM);
    for correlation in correlations {
        assert!(matches!(
            browser.results.get(&correlation),
            Some(Outcome::Refused(_))
        ));
    }
}

#[shoop_test]
fn a_framing_error_ends_the_connection_and_a_new_hello_reconnects() {
    let mut browser = Browser::connected(&WorkletConfig::new(description()));
    browser.to_worklet.push_back(Message {
        kind: Kind::Records,
        bytes: vec![0, 0, 1, 2, 3],
    });
    browser.run(QUANTUM);
    assert_eq!(
        browser.lifecycle.last(),
        Some(&EngineMessage::Disconnected(
            DisconnectReason::ProtocolError
        ))
    );
    assert_eq!(browser.client.state(), ClientState::Disconnected);
    assert_eq!(browser.counters().framing_errors, 1);
    browser.client.connect().unwrap();
    browser.run(QUANTUM);
    assert_eq!(browser.client.state(), ClientState::Connected);
}

#[shoop_test]
fn refuses_invalid_quanta_without_rendering() {
    let mut browser = Browser::new(&WorkletConfig::new(description()));
    let host = &mut browser.processor.host;
    for (frames, input_channels) in [(0, 0), (QUANTUM + 1, 0), (QUANTUM, 3)] {
        let quantum = Quantum {
            frames,
            input_channels,
            frame: START,
            load: None,
        };
        assert_eq!(host.process(quantum), Err(QuantumError::Invalid));
    }
    assert_eq!(host.fault(), None);
}
