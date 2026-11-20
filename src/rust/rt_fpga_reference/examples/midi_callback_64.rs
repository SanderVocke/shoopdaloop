#[path = "support/session_64.rs"]
mod session;

use session::*;
use shoop_rt_engine::{testing::OfflineHost, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::*;
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use std::{collections::BTreeMap, time::Instant};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Production {
    host: OfflineHost,
    loops: usize,
    frames: usize,
    audio: bool,
    position: usize,
}
fn key(slot: usize) -> SlotId {
    SlotId::new(slot as u32, 1)
}
fn strip(slot: usize) -> StripId {
    StripId(key(slot))
}
fn loop_id(slot: usize) -> LoopSlotId {
    LoopSlotId(key(slot))
}
fn strip_port(slot: usize, kind: PortKind, index: u8) -> EnginePort {
    EnginePort::Strip(StripPort {
        strip: strip(slot),
        kind,
        index,
    })
}
impl Production {
    fn request(&mut self, request: Request) -> Outcome {
        let correlation = self.host.request(request).unwrap();
        let mut result = None;
        for _ in 0..100_000 {
            while let Some(message) = self.host.receive() {
                if let EngineMessage::Result {
                    correlation: id,
                    outcome,
                } = message
                {
                    if id == correlation {
                        result = Some(outcome);
                    }
                }
            }
            if self.host.endpoint().is_idle() {
                if let Some(result) = result {
                    return result;
                }
            }
            self.host.process(QUANTUM).unwrap();
        }
        panic!("production control did not settle: {request:?}");
    }
    fn done(&mut self, request: Request) {
        let result = self.request(request);
        assert!(
            matches!(result, Outcome::Done | Outcome::ContentApplied { .. }),
            "{request:?}: {result:?}"
        );
    }
    fn upload(&mut self, id: u64, midi: bool, bytes: &[u8]) {
        self.done(Request::ContentBegin(ContentDescription {
            id,
            midi,
            frames: self.frames as u64,
            bytes: bytes.len() as u64,
        }));
        let mut pending = Vec::with_capacity(32);
        for (index, chunk) in bytes.chunks(content::CONTENT_CHUNK_BYTES).enumerate() {
            pending.push(
                self.host
                    .request(Request::ContentWrite {
                        id,
                        offset: (index * content::CONTENT_CHUNK_BYTES) as u64,
                        data: ContentChunk::new(chunk).unwrap(),
                    })
                    .unwrap(),
            );
            if pending.len() == 32 {
                self.flush(&pending);
                pending.clear();
            }
        }
        self.flush(&pending);
        self.done(Request::ContentFinish { id });
    }
    fn flush(&mut self, pending: &[u64]) {
        let mut received = [false; 32];
        for _ in 0..100_000 {
            while let Some(message) = self.host.receive() {
                if let EngineMessage::Result {
                    correlation,
                    outcome,
                } = message
                {
                    let index = pending.iter().position(|&id| id == correlation).unwrap();
                    assert_eq!(outcome, Outcome::Done);
                    assert!(!received[index]);
                    received[index] = true;
                }
            }
            if received[..pending.len()].iter().all(|&done| done) && self.host.endpoint().is_idle()
            {
                return;
            }
            self.host.process(QUANTUM).unwrap();
        }
        panic!("production upload window did not settle");
    }
    fn new(audio: bool, loops: usize, frames: usize) -> Self {
        let mut host = HostPorts::numbered(
            if audio { 2 } else { 0 },
            if audio { 2 } else { 0 },
            DESTINATIONS,
        );
        host.midi_inputs.truncate(1);
        let mut description = EngineDescription::default();
        description.sample_rate = SAMPLE_RATE as u32;
        description.quantum = QUANTUM;
        description.max_frames = QUANTUM;
        description.host = host;
        description.segments = vec![Segment {
            strips: loops.div_ceil(8) as u32,
            loops: loops as u32,
        }];
        description.strip_channels = 2;
        description.master_channels = 2;
        description.buses = 1;
        description.bus_channels = 2;
        description.master_connections = if audio { vec![(0, 0), (1, 1)] } else { vec![] };
        description.history.seconds = 0;
        description.history.audio_take_pages = 4;
        description.history.midi_pages = 4;
        description.history.midi_records = 64;
        description.history.writers = 1;
        description.page_bytes = 4096;
        description.resource_pages = (loops * frames * 8).div_ceil(4096) + 8192;
        description.midi_events = 1024;
        description.midi_bytes = 4096;
        description.smoothing_frames = 0;
        description.smoothing_limit_frames = 0;
        description.ramp_frames = 0;
        description.control_lead_frames = 0;
        description.pending_transitions = 256;
        description.reliable_credits = 128;
        description.telemetry_interval_frames = 0;
        let mut host = OfflineHost::new(&description).unwrap();
        host.connect().unwrap();
        while host.receive().is_some() {}
        let mut result = Self {
            host,
            loops,
            frames,
            audio,
            position: 0,
        };
        for owner in 0..loops.div_ceil(8) {
            result.done(Request::CreateStrip {
                strip: strip(owner),
                shape: StripShape {
                    audio_inputs: if audio { 2 } else { 0 },
                    audio_outputs: if audio { 2 } else { 0 },
                    midi_input: true,
                    midi_output: true,
                },
            });
            if audio {
                for channel in 0..2u8 {
                    result.done(Request::Connect {
                        port: strip_port(owner, PortKind::AudioIn, channel),
                        host: HostPortId(u32::from(channel)),
                        connected: true,
                    });
                    result.done(Request::Route {
                        port: StripPort {
                            strip: strip(owner),
                            kind: PortKind::AudioOut,
                            index: channel,
                        },
                        bus: BusPort {
                            bus: BusId::MASTER,
                            index: channel,
                        },
                        connected: true,
                    });
                }
            }
            result.done(Request::Connect {
                port: strip_port(owner, PortKind::MidiOut, 0),
                host: HostPortId(if audio { 5 } else { 1 } + owner as u32),
                connected: true,
            });
        }
        for slot in 0..loops {
            result.done(Request::CreateLoop {
                loop_id: loop_id(slot),
                strip: strip(slot / 8),
            });
            let audio_id = (slot * 2 + 1) as u64;
            let midi_id = audio_id + 1;
            let bytes: Vec<u8> = if audio {
                (0..frames)
                    .flat_map(|frame| {
                        (0..2).flat_map(move |channel| sample(slot, channel, frame).to_le_bytes())
                    })
                    .collect()
            } else {
                vec![0; frames * 8]
            };
            result.upload(audio_id, false, &bytes);
            let mut bytes = Vec::new();
            for record in records(slot, frames) {
                bytes.extend_from_slice(&record.frame.to_le_bytes());
                bytes.extend_from_slice(&(record.message.as_bytes().len() as u32).to_le_bytes());
                bytes.extend_from_slice(record.message.as_bytes());
            }
            result.upload(midi_id, true, &bytes);
            result.done(Request::ContentApply {
                loop_id: loop_id(slot),
                frames: frames as u64,
                dry_audio: None,
                recorded_state: None,
                audio: Some(audio_id),
                midi: Some(midi_id),
            });
            eprintln!("production prepared loop {}/{}", slot + 1, loops);
        }
        result
    }
    fn play(&mut self) -> u128 {
        for slot in 0..self.loops {
            self.host
                .request(Request::LoopTransition {
                    loop_id: loop_id(slot),
                    mode: LoopMode::Playing,
                    delay: None,
                })
                .unwrap();
        }
        for _ in 0..1024 {
            let start = Instant::now();
            self.host.process(QUANTUM).unwrap();
            let elapsed = start.elapsed().as_nanos();
            let output = self.host.midi_output(0);
            let first = (0..output.len()).any(|i| {
                let (_, bytes) = output.get(i).unwrap();
                bytes == [0xb0, 1, 1]
            });
            while let Some(message) = self.host.receive() {
                if let EngineMessage::Result { outcome, .. } = message {
                    assert_eq!(outcome, Outcome::Done);
                }
            }
            if first {
                self.position = 0;
                return elapsed;
            }
        }
        panic!("production did not start every loop");
    }
    fn validate(&self) {
        if !self.audio {
            assert_eq!(self.host.render.audio_graph_work().sample_reads, 0);
            assert_eq!(self.host.render.audio_graph_work().sample_writes, 0);
        }
        let mut actual = Vec::new();
        for destination in 0..DESTINATIONS {
            let output = self.host.midi_output(destination);
            for index in 0..output.len() {
                let (offset, bytes) = output.get(index).unwrap();
                actual.push((destination as u16, offset, bytes.to_vec()));
            }
        }
        let mut expected: Vec<_> = expected_events(self.loops, self.frames, self.position)
            .into_iter()
            .map(|e| (e.destination, e.offset, e.message.as_bytes().to_vec()))
            .collect();
        expected.sort();
        actual.sort();
        assert_eq!(
            actual, expected,
            "production MIDI position {}",
            self.position
        );
        if self.audio {
            for channel in 0..2 {
                for (offset, &value) in self.host.output(channel).iter().enumerate() {
                    let expected: f32 = (0..self.loops)
                        .map(|slot| sample(slot, channel, self.position + offset))
                        .sum();
                    assert_eq!(
                        value, expected,
                        "production audio position {}",
                        self.position
                    );
                }
            }
        }
    }
    fn callback(&mut self) {
        self.position = (self.position + QUANTUM) % self.frames;
        self.host.process(QUANTUM).unwrap();
    }
    fn pump(&mut self) {
        while self.host.receive().is_some() {}
    }
}

fn stats(mut values: Vec<u128>) -> serde_json::Value {
    values.sort_unstable();
    let n = values.len();
    serde_json::json!({"callbacks":n,"mean_us":values.iter().sum::<u128>() as f64/n as f64/1000.0,
        "p50_us":values[n/2] as f64/1000.0,"p95_us":values[(n*95/100).min(n-1)] as f64/1000.0,
        "p99_us":values[(n*99/100).min(n-1)] as f64/1000.0,"max_us":values[n-1] as f64/1000.0})
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let option = |key: &str, default: usize| {
        args.windows(2)
            .find(|p| p[0] == key)
            .map_or(default, |p| p[1].parse().unwrap())
    };
    let backend = args
        .windows(2)
        .find(|p| p[0] == "--backend")
        .map_or("fpga", |p| p[1].as_str());
    let audio = !args.iter().any(|a| a == "--midi-only");
    let loops = option("--loops", LOOPS);
    let frames = option("--frames", FRAMES);
    let callbacks = option("--callbacks", 9000);
    let repeats = option("--repeats", 3);
    let warmup = 150usize.min(frames / QUANTUM / 2);
    let mut rows = Vec::new();
    let mut raw = Vec::with_capacity(callbacks * repeats);
    let startup;
    let startup_packets;
    if backend == "fpga" {
        let mut engine = Reference::new(audio, loops, frames);
        engine.play();
        let start = Instant::now();
        assert_eq!(engine.callback().error, None);
        startup = start.elapsed().as_nanos();
        startup_packets = engine.emitted.len();
        for i in 1..warmup {
            let report = if i + 1 == warmup {
                assert_no_heap(|| engine.callback())
            } else {
                engine.callback()
            };
            assert_eq!(report.error, None);
            engine.validate();
        }
        for run in 0..repeats {
            let mut values = Vec::with_capacity(callbacks);
            for callback in 0..callbacks {
                let start = Instant::now();
                let report = engine.callback();
                values.push(start.elapsed().as_nanos());
                raw.push((
                    run,
                    callback,
                    *values.last().unwrap(),
                    engine.position(),
                    engine.emitted.len(),
                ));
                assert_eq!(report.error, None);
                engine.validate();
            }
            rows.push(stats(values));
        }
    } else {
        assert_eq!(backend, "production");
        let mut engine = Production::new(audio, loops, frames);
        startup = engine.play();
        startup_packets = (0..DESTINATIONS)
            .map(|i| engine.host.midi_output(i).len())
            .sum();
        for i in 1..warmup {
            if i + 1 == warmup {
                assert_no_heap(|| engine.callback());
            } else {
                engine.callback();
            }
            engine.validate();
            engine.pump();
        }
        for run in 0..repeats {
            let mut values = Vec::with_capacity(callbacks);
            for callback in 0..callbacks {
                let start = Instant::now();
                engine.callback();
                values.push(start.elapsed().as_nanos());
                raw.push((
                    run,
                    callback,
                    *values.last().unwrap(),
                    engine.position,
                    (0..DESTINATIONS)
                        .map(|i| engine.host.midi_output(i).len())
                        .sum(),
                ));
                engine.validate();
                engine.pump();
            }
            rows.push(stats(values));
        }
    }
    if let Some(path) = args.windows(2).find(|p| p[0] == "--csv") {
        use std::io::Write;
        let mut writer = std::io::BufWriter::new(std::fs::File::create(&path[1]).unwrap());
        writeln!(writer, "run,callback,elapsed_ns,position,midi_packets").unwrap();
        for (run, callback, ns, position, packets) in raw {
            writeln!(writer, "{run},{callback},{ns},{position},{packets}").unwrap();
        }
    }
    let metadata = BTreeMap::from([
        ("backend", backend),
        (
            "scenario",
            if audio {
                "stereo_and_midi"
            } else {
                "midi_only"
            },
        ),
    ]);
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({"metadata":metadata,
        "loops":loops,"sample_rate":SAMPLE_RATE,"quantum_frames":QUANTUM,"chunk_frames":QUANTUM,
        "sequence_frames":frames,"cc_per_second_per_loop":100,"destinations":DESTINATIONS,
        "warmup_callbacks":warmup,"start_playback_us":startup as f64/1000.0,"start_playback_midi_packets":startup_packets,"runs":rows}))
        .unwrap()
    );
}
