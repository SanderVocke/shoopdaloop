use super::*;
use crate::{prepare_with_shared, Engine, EngineDescription, HostPorts, Segment};
use alloc::vec::Vec;
use rt_runtime::midi_state::MidiState;
use rt_runtime::{
    executor::Command,
    resource::{self, Request as ResourceRequest, ResourceCommand, Snapshot},
    schedule::Execution,
    transport::Lane,
    Frame, Sequence,
};
use shoop_loops::{
    media,
    runtime::{
        CandidateStatus, HistorySettings, Operation, Reply, Selection, SharedSettings, SharedTake,
    },
    LoopId, Timing, Transition,
};
use shoop_rt_protocol::{self as protocol, DriverKind, DriverRunState, DriverStatus};
use shoop_rt_transport::carrier::{Endpoint, Outbox};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[cfg(not(target_arch = "wasm32"))]
#[path = "shared_measurements.rs"]
mod measurements;
#[path = "shared_persistence_tests.rs"]
mod persistence;

type Completion = rt_runtime::transport::Completion<
    crate::EngineCommand,
    rt_runtime::executor::Retired<crate::EngineNode>,
>;
type Executed = Execution<Operation, Reply, crate::EngineOp, ()>;

struct Fixture {
    engine: Engine,
    outbox: Outbox,
    q: usize,
    partition: usize,
    input: [Vec<f32>; 2],
    output: [Vec<f32>; 2],
    midi_input: MidiBuffer,
    midi_output: MidiBuffer,
    samples: [Vec<f32>; 2],
    packets: Vec<(u64, Vec<u8>)>,
    completions: Vec<Completion>,
    executions: Vec<Executed>,
    stimulus: Option<u64>,
    burst: Option<(u64, usize)>,
    last_execution: Option<Executed>,
    midi_overflows: usize,
    transfer_bytes: usize,
    read_buffer: Option<(resource::Cursor, Box<[u8]>)>,
    #[cfg(not(target_arch = "wasm32"))]
    timings: Option<Vec<measurements::Callback>>,
    #[cfg(not(target_arch = "wasm32"))]
    measure_phase: &'static str,
}
fn target(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn packets(frame: u64, q: usize) -> Vec<Vec<u8>> {
    let q = q as u64;
    let tick = frame / q;
    let mut result = Vec::new();
    if frame % q == 3.min(q - 1) {
        result.push(vec![0xb0 + (tick % 16) as u8, 1, (tick % 128) as u8]);
    }
    if frame % q == q - 1 {
        result.push(vec![
            if tick % 2 == 0 { 0x90 } else { 0x80 },
            60,
            if tick % 2 == 0 { 100 } else { 0 },
        ]);
    }
    if frame % (8 * q) == 1.min(q - 1) {
        let mut sysex = vec![1; 600];
        sysex[0] = 0xf0;
        sysex[599] = 0xf7;
        result.push(sysex);
    }
    result
}
impl Fixture {
    fn audio_candidate(&mut self, take: SharedTake) -> shoop_loops::runtime::CandidateId {
        let Reply::Candidate { id, .. } = self.op(Operation::CandidateBegin {
            target: target(0),
            take,
        }) else {
            panic!("candidate")
        };
        let patch = self.snapshot(take.audio.unwrap());
        assert!(matches!(
            self.op(Operation::CandidateAudio {
                id,
                position: 0,
                input: patch.id,
                offset: 0,
                frames: self.q,
            }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ));
        self.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
        id
    }
    fn new(q: usize, partition: usize) -> Self {
        Self::new_with_rate(q, partition, 1000)
    }
    fn new_with_rate(q: usize, partition: usize, rate: u32) -> Self {
        Self::new_shapes(q, partition, rate, true, true)
    }
    fn new_shapes(q: usize, partition: usize, rate: u32, audio: bool, midi: bool) -> Self {
        Self::new_prepared_shapes(q, partition, rate, audio, midi, false, false)
    }
    fn new_pooled(q: usize, partition: usize) -> Self {
        Self::new_prepared_shapes(q, partition, 1000, true, false, true, false)
    }
    fn new_prepared_shapes(
        q: usize,
        partition: usize,
        rate: u32,
        audio: bool,
        midi: bool,
        pooled: bool,
        indexed: bool,
    ) -> Self {
        let history_frames = if pooled { 32 * q } else { 8 * q };
        let d = EngineDescription {
            quantum: q,
            sample_rate: rate,
            max_frames: 128,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: if pooled { 2 } else { 1 },
                loops: 2,
            }],
            resource_pages: 12288,
            page_bytes: 256,
            midi_events: 512,
            midi_bytes: 16384,
            ramp_frames: 0,
            smoothing_frames: 0,
            control_lead_frames: 0,
            ..EngineDescription::default()
        };
        let engine = prepare_with_shared(
            &d,
            DriverStatus {
                kind: DriverKind::Dummy,
                state: DriverRunState::Running,
                sample_rate: rate,
                buffer_frames: 128,
            },
            SharedSettings {
                audio_pool: pooled.then_some(resource::shared_audio::Config {
                    channels: 2,
                    history_frames,
                    views: 48,
                    pages: 2048,
                }),
                histories: (0..if pooled { 2 } else { 1 })
                    .map(|source| HistorySettings {
                        source,
                        audio: audio.then_some(resource::shared_audio::Config {
                            channels: 2,
                            history_frames,
                            views: 24,
                            pages: 1024,
                        }),
                        midi: (midi && (source == 0 || indexed)).then_some(
                            resource::shared_midi::Config {
                                history_frames: if indexed {
                                    history_frames as u64
                                } else {
                                    (8 * q) as u64
                                },
                                chunks: 8,
                                views: 24,
                                pages: 8192,
                                max_events: 64,
                                max_bytes: 2048,
                                max_packet: 1024,
                                transactions: 2,
                                edit_steps: 8,
                            },
                        ),
                    })
                    .collect(),
                midi_pool: indexed.then_some(resource::indexed_midi::Config {
                    packets: resource::indexed_midi::PacketLimits {
                        pages: 1024,
                        bodies: 2048,
                        records: 4096,
                        max_packet: 1024,
                        bases: 4,
                    },
                    indexes: resource::indexed_midi::IndexLimits {
                        leases: 96,
                        versions: 64,
                        category_pages: 512,
                        packet_nodes: 32768,
                        scalar_nodes: 32768,
                        hold_nodes: 8192,
                        sostenuto_nodes: 8192,
                    },
                    sources: 2,
                    history_frames: history_frames as u64,
                    history_events: 128,
                    history_bytes: 16384,
                    views: 48,
                    writers: 4,
                    imports: 2,
                    events_per_tick: 64,
                    bytes_per_tick: 2048,
                }),
                candidates: 2,
                edit_steps_per_callback: 8,
            },
        )
        .unwrap();
        let outbox = engine.outbox();
        let transfer_bytes = crate::runtime_capacities(&d, &engine.endpoint.pool, q)
            .budgets
            .transfer_bytes;
        let mut h = Self {
            engine,
            outbox,
            q,
            partition,
            input: [vec![0.; 128], vec![0.; 128]],
            output: [vec![0.; 128], vec![0.; 128]],
            midi_input: MidiBuffer::new(512, 16384).unwrap(),
            midi_output: MidiBuffer::new(4096, 32768).unwrap(),
            samples: [Vec::new(), Vec::new()],
            packets: Vec::new(),
            completions: Vec::new(),
            executions: Vec::new(),
            stimulus: None,
            burst: None,
            last_execution: None,
            midi_overflows: 0,
            transfer_bytes,
            read_buffer: None,
            #[cfg(not(target_arch = "wasm32"))]
            timings: None,
            #[cfg(not(target_arch = "wasm32"))]
            measure_phase: "capture",
        };
        h.engine.endpoint.deliver(
            protocol::message::ClientMessage::Hello {
                version: protocol::PROTOCOL_VERSION,
            },
            &mut h.outbox,
        );
        while h.outbox.pop().is_some() {}
        let strip = protocol::StripId(protocol::SlotId::new(0, 1));
        h.request(protocol::Request::CreateStrip {
            strip,
            shape: protocol::StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: true,
                midi_output: true,
            },
        });
        for (kind, index, host) in [
            (protocol::PortKind::AudioIn, 0, 0),
            (protocol::PortKind::AudioIn, 1, 1),
            (protocol::PortKind::AudioOut, 0, 2),
            (protocol::PortKind::AudioOut, 1, 3),
            (protocol::PortKind::MidiIn, 0, 4),
            (protocol::PortKind::MidiOut, 0, 5),
        ] {
            h.request(protocol::Request::Connect {
                port: protocol::EnginePort::Strip(protocol::StripPort { strip, kind, index }),
                host: protocol::HostPortId(host),
                connected: true,
            });
        }
        for slot in 0..2 {
            h.request(protocol::Request::CreateLoop {
                loop_id: protocol::LoopSlotId(protocol::SlotId::new(slot, 1)),
                strip,
            });
        }
        h.stimulus = Some(h.now());
        h
    }
    fn now(&self) -> u64 {
        self.engine.render.next_frame().0
    }
    fn aligned_future(&self, gap: u64) -> u64 {
        let q = self.q as u64;
        (self.now() + gap).div_ceil(q) * q
    }
    fn render(&mut self, frames: usize, raw: bool) {
        self.render_result(frames, raw).unwrap();
    }
    fn render_result(&mut self, frames: usize, raw: bool) -> Result<crate::Report, Error> {
        let start = self.now();
        self.midi_input.clear();
        self.midi_output.clear();
        for i in 0..frames {
            let frame = start + i as u64;
            let enabled = self.stimulus.is_some_and(|s| frame >= s);
            self.input[0][i] = if enabled { (frame + 1) as f32 } else { 0. };
            self.input[1][i] = -2. * self.input[0][i];
            if enabled {
                for packet in packets(frame, self.q) {
                    self.midi_input.push(i as u32, frames, &packet).unwrap();
                }
                if let Some((_, count)) = self.burst.filter(|(at, _)| *at == frame) {
                    for value in 0..count {
                        self.midi_input
                            .push(i as u32, frames, &[0xbf, 7, (value % 128) as u8])
                            .unwrap();
                    }
                }
            }
        }
        let [left, right] = &mut self.output;
        #[cfg(not(target_arch = "wasm32"))]
        let clock = self.timings.as_ref().map(|_| std::time::Instant::now());
        let result = assert_no_heap(|| {
            self.engine.render.process(
                AudioBlock {
                    frame: Frame(start),
                    frames,
                    inputs: &[&self.input[0][..frames], &self.input[1][..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
                },
                MidiBlock {
                    inputs: core::slice::from_ref(&self.midi_input),
                    outputs: core::slice::from_mut(&mut self.midi_output),
                },
            )
        });
        let midi_overflows = self.engine.render.diagnostics().midi_overflows;
        self.midi_overflows += midi_overflows;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(clock) = clock {
            let elapsed = clock.elapsed().as_nanos();
            self.timings.as_mut().unwrap().push(measurements::Callback {
                frame: start,
                frames,
                nanos: elapsed,
                work: self.engine.render.resource_work(),
                usage: self.engine.render.resource_usage(),
                ticks: self.engine.render.diagnostics().subranges,
                phase: self.measure_phase,
                midi_overflows,
            });
        }
        for ch in 0..2 {
            self.samples[ch].extend_from_slice(&self.output[ch][..frames]);
        }
        for i in 0..self.midi_output.len() {
            let (offset, data) = self.midi_output.get(i).unwrap();
            self.packets
                .push((start + u64::from(offset), data.to_vec()));
        }
        if raw {
            while let Some(c) = self.engine.endpoint.control.try_completion() {
                self.completions.push(c);
            }
            while let Some(e) = self.engine.endpoint.control.try_execution() {
                self.executions.push(e);
            }
        }
        result
    }
    fn run_to(&mut self, end: u64) {
        assert!(end >= self.now());
        while self.now() < end {
            self.render(self.partition.min((end - self.now()) as usize), true);
        }
    }
    fn request(&mut self, request: protocol::Request) {
        assert_eq!(
            self.request_outcome(request),
            protocol::Outcome::Done,
            "{request:?}"
        );
    }
    fn request_outcome(&mut self, request: protocol::Request) -> protocol::Outcome {
        self.engine.endpoint.deliver(
            protocol::message::ClientMessage::Request {
                correlation: 1,
                request,
            },
            &mut self.outbox,
        );
        for _ in 0..10000 {
            self.engine.endpoint.poll(&mut self.outbox);
            while let Some(m) = self.outbox.pop() {
                if let protocol::EngineMessage::Result {
                    correlation: 1,
                    outcome,
                } = m
                {
                    return outcome;
                }
            }
            self.render(self.q, false);
        }
        panic!("request pending {request:?}");
    }
    fn send(&mut self, at: u64, operation: Operation) -> Sequence {
        self.drain_protocol();
        let command = self.engine.endpoint.loops.command(Frame(at), operation);
        assert_no_heap(|| {
            self.engine
                .endpoint
                .control
                .submit_prepared(Lane::Normal, command)
        })
        .unwrap_or_else(|e| panic!("{:?}", e.error))
        .sequence()
    }
    fn result(&mut self, sequence: Sequence) -> Reply {
        for _ in 0..10000 {
            if let Some(i) = self.executions.iter().position(|e| e.sequence == sequence) {
                let e = self.executions.remove(i);
                assert_eq!(e.result, Ok(()));
                self.last_execution = Some(e);
                return e.operation_result.unwrap();
            }
            self.render(self.partition, true);
        }
        panic!("loop operation pending");
    }
    fn op(&mut self, operation: Operation) -> Reply {
        let seq = self.send(self.aligned_future(128), operation);
        self.result(seq)
    }
    fn resource(&mut self, request: ResourceRequest) -> resource::Reply {
        self.resource_result(request).unwrap()
    }
    fn resource_result(&mut self, request: ResourceRequest) -> Result<resource::Reply, Error> {
        self.drain_protocol();
        let seq = self
            .engine
            .endpoint
            .control
            .submit_prepared(
                Lane::Normal,
                Command::Resource(ResourceCommand::new(request)),
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error))
            .sequence();
        for _ in 0..10000 {
            if let Some(i) = self.completions.iter().position(|c| c.sequence == seq) {
                let c = self.completions.remove(i);
                if let rt_runtime::transport::Outcome::Rejected(error) = c.outcome {
                    return Err(error);
                }
                assert_eq!(c.outcome, rt_runtime::transport::Outcome::Applied);
                let Command::Resource(command) = c.request else {
                    panic!("resource completion")
                };
                let reply = command.reply().unwrap();
                if let ResourceRequest::Read { cursor, bytes } = command.into_request() {
                    self.read_buffer = Some((cursor, bytes));
                }
                return Ok(reply);
            }
            self.render(self.partition, true);
        }
        panic!("resource pending");
    }
    fn drain_protocol(&mut self) {
        for _ in 0..10000 {
            if self.engine.endpoint.is_idle() {
                return;
            }
            self.engine.endpoint.poll(&mut self.outbox);
            self.render(self.q, false);
        }
        panic!("protocol owning returns pending");
    }
    fn snapshot(&mut self, resource: rt_runtime::ResourceId) -> Snapshot {
        let resource::Reply::Snapshot(s) = self.resource(ResourceRequest::Snapshot { resource })
        else {
            panic!("snapshot")
        };
        s
    }
    fn read(&mut self, snapshot: Snapshot) -> Vec<u8> {
        let resource::Reply::Cursor(mut cursor) = self.resource(ResourceRequest::Cursor {
            snapshot: snapshot.id,
        }) else {
            panic!("cursor")
        };
        let mut result = Vec::new();
        while result.len() < snapshot.description.bytes {
            let resource::Reply::Read { written, .. } = self.resource(ResourceRequest::Read {
                cursor,
                bytes: vec![0; 256].into_boxed_slice(),
            }) else {
                panic!("read")
            };
            assert!(written > 0);
            let (next, buffer) = self.read_buffer.take().unwrap();
            result.extend_from_slice(&buffer[..written]);
            cursor = next;
        }
        result
    }
    fn node_source(
        &mut self,
        resource: rt_runtime::ResourceId,
    ) -> shoop_loops::runtime::NodePairSourceId {
        let Reply::NodePairSource { id, mut status, .. } =
            self.op(Operation::NodePairSnapshot { resource })
        else {
            panic!("node pair source")
        };
        while !matches!(
            status,
            shoop_loops::runtime::NodePairSourceStatus::Ready { .. }
        ) {
            let Reply::NodePairSourceStatus { status: next, .. } =
                self.op(Operation::NodePairSourceValidate { id })
            else {
                panic!("validation")
            };
            status = next;
        }
        id
    }
    fn adopt(&mut self, slot: u32, take: SharedTake) -> media::MediaId {
        let audio = take.audio.map(|id| self.node_source(id));
        let midi = take.midi.map(|id| self.node_source(id));
        let Reply::Adopted { media, .. } = self.op(Operation::Adopt {
            target: target(slot),
            frames: take.frames,
            audio,
            midi,
        }) else {
            panic!("adopt")
        };
        let reply = self.op(Operation::Select {
            target: target(slot),
            media,
            timing: Timing::At,
            preserve_position: false,
        });
        assert!(matches!(reply, Reply::Transition(_)), "{reply:?}");
        assert_eq!(self.op(Operation::Retire { media }), Reply::Done);
        media
    }
    fn export(&mut self, slot: u32) -> media::Export {
        self.export_owned(slot).1
    }
    fn export_owned(&mut self, slot: u32) -> (media::ExportId, media::Export) {
        let Reply::Export { id, media, .. } = self.op(Operation::Export {
            target: target(slot),
            selection: Selection::Playback,
        }) else {
            panic!("export")
        };
        (id, media)
    }
    fn grabs(&mut self) -> (SharedTake, SharedTake, u64) {
        let Reply::History(status) = self.op(Operation::HistoryStatus { source: 0 }) else {
            panic!("history")
        };
        let at = self.aligned_future((128 + 8 * self.q) as u64);
        let first = self.send(
            at,
            Operation::HistoryGrab {
                source: status.id,
                start: Frame(at - 5 * self.q as u64),
                end: Frame(at - self.q as u64),
            },
        );
        let second = self.send(
            at,
            Operation::HistoryGrab {
                source: status.id,
                start: Frame(at - 4 * self.q as u64),
                end: Frame(at),
            },
        );
        let Reply::Grabbed(a) = self.result(first) else {
            panic!("first grab")
        };
        let Reply::Grabbed(b) = self.result(second) else {
            panic!("second grab")
        };
        (a, b, at - 5 * self.q as u64)
    }
}
fn failed(error: Error) -> Reply {
    Reply::Failed(shoop_loops::runtime::Failure::Runtime(error))
}
fn recover_all(stopped: &mut Stopped, pages: usize) {
    let midi = stopped
        .planner
        .shared_stores(0)
        .ok()
        .and_then(|(_, midi)| midi);
    let slots = stopped.resources.config().limits().resource_slots;
    stopped.cleanup().unwrap();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, pages);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(usage.free_versions, slots);
    assert_eq!(usage.free_snapshots, slots);
    assert_eq!((usage.retired_versions, usage.retired_pages), (0, 0));
    let groups = stopped.capture_group_usage();
    assert_eq!(groups.free_groups, groups.groups);
    let indexes = stopped.resources.midi_index_usage();
    assert_eq!(indexes.free_versions, indexes.versions);
    assert_eq!(stopped.media_usage().references, 0);
    if let Some(midi) = midi {
        let usage = stopped.resources.midi_store_usage(midi).unwrap();
        assert_eq!(
            (usage.owners, usage.retiring, usage.reserved_pages),
            (0, 0, 0)
        );
    }
}

#[shoop_test]
fn buffered_production_preparation_accepts_dynamic_host_banks_beyond_sixty_four_ports() {
    let description = EngineDescription {
        quantum: 32,
        max_frames: 128,
        host: HostPorts::numbered(128, 128, 1),
        segments: vec![Segment {
            strips: 1,
            loops: 1,
        }],
        ..EngineDescription::default()
    };
    let engine = crate::prepare(
        &description,
        DriverStatus {
            kind: DriverKind::Dummy,
            state: DriverRunState::Running,
            sample_rate: description.sample_rate,
            buffer_frames: 128,
        },
    )
    .unwrap();
    let mut stopped = engine.render.into_stopped();
    stopped.cleanup().unwrap();
    assert_eq!(
        stopped.resource_usage().free_pages,
        description.resource_pages
    );
}

#[shoop_test]
fn stopped_sources_support_audio_only_and_midi_only_completed_grabs() {
    for (audio, midi) in [(true, false), (false, true)] {
        let mut h = Fixture::new_shapes(32, 7, 1000, audio, midi);
        let (take, sibling, _) = h.grabs();
        assert_eq!((take.audio.is_some(), take.midi.is_some()), (audio, midi));
        assert_eq!(
            (sibling.audio.is_some(), sibling.midi.is_some()),
            (audio, midi)
        );
        let mut stopped = h.engine.render.into_stopped();
        recover_all(&mut stopped, 12288);
    }
}

#[shoop_test]
fn pooled_sources_capture_distinct_signal_paths_at_one_fixed_cut_and_cleanup_once() {
    for (q, partition) in [(1, 1), (32, 7), (128, 128)] {
        let mut h = Fixture::new_pooled(q, partition);
        let strip = protocol::StripId(protocol::SlotId::new(1, 1));
        h.request(protocol::Request::CreateStrip {
            strip,
            shape: protocol::StripShape {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_input: false,
                midi_output: false,
            },
        });
        h.request(protocol::Request::StripControl {
            strip,
            control: protocol::StripControl::InputGain(3.),
        });
        for channel in 0..2 {
            h.request(protocol::Request::Connect {
                port: protocol::EnginePort::Strip(protocol::StripPort {
                    strip,
                    kind: protocol::PortKind::AudioIn,
                    index: channel,
                }),
                host: protocol::HostPortId(u32::from(channel)),
                connected: true,
            });
        }
        let Reply::SharedCapacity(capacity) = h.op(Operation::SharedCapacity) else {
            panic!("capacity")
        };
        assert_eq!(
            (
                capacity.sources,
                capacity.reserved_payload_bytes,
                capacity.midi_state_bytes
            ),
            (2, 2048 * 256, 0)
        );
        let Reply::History(a) = h.op(Operation::HistoryStatus { source: 0 }) else {
            panic!("source a")
        };
        let Reply::History(b) = h.op(Operation::HistoryStatus { source: 1 }) else {
            panic!("source b")
        };
        let at = h.aligned_future((128 + 8 * q) as u64);
        let start = at - 2 * q as u64;
        let a = h.send(
            at,
            Operation::HistoryGrab {
                source: a.id,
                start: Frame(start),
                end: Frame(at),
            },
        );
        let b = h.send(
            at,
            Operation::HistoryGrab {
                source: b.id,
                start: Frame(start),
                end: Frame(at),
            },
        );
        let Reply::Grabbed(a) = h.result(a) else {
            panic!("grab a")
        };
        let Reply::Grabbed(b) = h.result(b) else {
            panic!("grab b")
        };
        assert_eq!(a.audio_store, b.audio_store);
        assert_ne!(a.audio, b.audio);
        for (take, gain) in [(a, 1f32), (b, 3f32)] {
            let snapshot = h.snapshot(take.audio.unwrap());
            let bytes = h.read(snapshot);
            assert_eq!(bytes.len(), 2 * q * 2 * 4);
            for (local, frame) in bytes.chunks_exact(8).enumerate() {
                let value = (start + local as u64 + 1) as f32 * gain;
                assert_eq!(f32::from_le_bytes(frame[..4].try_into().unwrap()), value);
                assert_eq!(
                    f32::from_le_bytes(frame[4..].try_into().unwrap()),
                    -2. * value
                );
            }
            h.resource(ResourceRequest::ReleaseSnapshot {
                snapshot: snapshot.id,
            });
        }
        let mut stopped = h.engine.render.into_stopped();
        recover_all(&mut stopped, 12288);
    }
}

#[shoop_test]
fn fixed_shared_seek_stop_and_output_recovery_preserve_the_selected_revision() {
    for partition in [1, 7, 31, 128] {
        let mut h = Fixture::new(32, partition);
        let (take, _, start) = h.grabs();
        h.adopt(0, take);
        let old = h.export(0);
        let at = h.aligned_future(256);
        let mut sequences = Vec::new();
        for (frame, transition) in [
            (
                at,
                Transition::Play {
                    position: 32,
                    retrigger: false,
                },
            ),
            (
                at + 64,
                Transition::Play {
                    position: 64,
                    retrigger: true,
                },
            ),
            (at + 128, Transition::Stop),
        ] {
            sequences.push(h.send(
                frame,
                Operation::Transition {
                    target: target(0),
                    timing: Timing::At,
                    transition,
                },
            ));
        }
        h.run_to(at + 224);
        for sequence in sequences {
            assert!(matches!(h.result(sequence), Reply::Transition(_)));
        }
        for f in at + 32..at + 160 {
            let core = f - 32;
            let position = if core < at + 64 {
                32 + core - at
            } else {
                64 + core - at - 64
            };
            assert_eq!(h.samples[0][f as usize], (start + position + 1) as f32);
        }
        assert!(h.samples[0][(at + 160) as usize..]
            .iter()
            .all(|sample| *sample == 0.));
        let overflows = h.midi_overflows;
        h.midi_output = MidiBuffer::new(1, 3).unwrap();
        h.op(Operation::Transition {
            target: target(0),
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: true,
            },
        });
        let end = h.aligned_future(256);
        h.run_to(end);
        assert_eq!(h.engine.render.phase(), Phase::Running);
        assert!(h.midi_overflows > overflows);
        h.midi_output = MidiBuffer::new(4096, 32768).unwrap();
        let end = h.aligned_future(256);
        h.run_to(end);
        assert_eq!(
            h.read(old.view.audio.unwrap().snapshot).len(),
            take.frames as usize * 8
        );
        assert_eq!(
            midi_events(&h.read(old.view.midi.unwrap().snapshot)),
            expected_midi(start, take.frames, 32)
        );
        let mut stopped = h.engine.render.into_stopped();
        recover_all(&mut stopped, 12288);
    }
}

#[shoop_test]
fn candidate_fences_preserve_controls_and_refuse_superseded_content_and_writers() {
    let mut h = Fixture::new(32, 7);
    let (take, sibling, _) = h.grabs();
    h.adopt(0, take);
    let old = h.export(0);
    let empty = match h.op(Operation::CandidateBegin {
        target: target(0),
        take,
    }) {
        Reply::Candidate { id, .. } => id,
        other => panic!("{other:?}"),
    };
    let patch = h.snapshot(take.audio.unwrap());
    h.op(Operation::CandidateAudio {
        id: empty,
        position: 0,
        input: patch.id,
        offset: 0,
        frames: 0,
    });
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    assert_eq!(
        h.op(Operation::CandidatePublish { id: empty }),
        failed(Error::InvalidState)
    );
    h.op(Operation::CandidateCancel { id: empty });

    let id = h.audio_candidate(take);
    h.op(Operation::Gain {
        target: target(0),
        value: 0.25,
    });
    let Reply::CandidatePublished { take: current, .. } = h.op(Operation::CandidatePublish { id })
    else {
        panic!("control changes must survive publication")
    };
    let Reply::State(cut) = h.op(Operation::State { target: target(0) }) else {
        panic!("state")
    };
    assert_eq!(cut.gain, 0.25);

    let id = h.audio_candidate(current);
    h.adopt(0, sibling);
    assert_eq!(
        h.op(Operation::CandidatePublish { id }),
        failed(Error::StaleRevision)
    );
    h.op(Operation::CandidateCancel { id });

    let id = h.audio_candidate(sibling);
    let audio = sibling.audio.unwrap();
    let resource::Reply::Info(info) = h.resource(ResourceRequest::Info { resource: audio }) else {
        panic!("info")
    };
    let resource::Reply::Upload(upload) = h.resource(ResourceRequest::BeginUpload {
        resource: audio,
        expected_revision: info.revision,
        description: info.description,
    }) else {
        panic!("upload")
    };
    assert_eq!(
        h.op(Operation::CandidatePublish { id }),
        failed(Error::Busy)
    );
    let data: Vec<u8> = (0..info.description.bytes / 4)
        .flat_map(|_| 0.5f32.to_le_bytes())
        .collect();
    for (page, bytes) in data.chunks(256).enumerate() {
        h.resource(ResourceRequest::Write {
            upload,
            offset: page * 256,
            bytes: bytes.into(),
        });
    }
    h.resource(ResourceRequest::Finish { upload });
    h.resource(ResourceRequest::Commit { upload });
    assert_eq!(
        h.op(Operation::CandidatePublish { id }),
        failed(Error::StaleRevision)
    );
    h.op(Operation::CandidateCancel { id });
    let live = h.export(0);
    let mut stopped = h.engine.render.into_stopped();
    assert_ne!(
        bytes(&mut stopped.resources, old.view.audio.unwrap().snapshot),
        data
    );
    assert_ne!(
        bytes(&mut stopped.resources, live.view.audio.unwrap().snapshot),
        data
    );
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn shutdown_reclaims_published_owners_even_when_the_terminal_result_is_unread() {
    let mut h = Fixture::new(32, 32);
    let (take, sibling, _) = h.grabs();
    h.adopt(0, take);
    let id = h.audio_candidate(take);
    let resource = sibling.audio.unwrap();
    let resource::Reply::Info(info) = h.resource(ResourceRequest::Info { resource }) else {
        panic!("info")
    };
    let resource::Reply::Upload(upload) = h.resource(ResourceRequest::BeginUpload {
        resource,
        expected_revision: info.revision,
        description: info.description,
    }) else {
        panic!("pending upload")
    };
    let at = h.aligned_future(128);
    let seq = h.send(at, Operation::CandidatePublish { id });
    while h.now() < at + 32 {
        h.render(32, false);
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
    assert_eq!(
        stopped.resources.cancel_upload(upload),
        Err(Error::InvalidId)
    );
    let mut found = false;
    while let Some(executed) = h.engine.endpoint.control.try_execution() {
        if executed.sequence == seq {
            let Some(Reply::CandidatePublished { take, .. }) = executed.operation_result else {
                panic!("accepted result remains readable")
            };
            for resource in [take.audio, take.midi].into_iter().flatten() {
                assert_eq!(stopped.resources.info(resource), Err(Error::InvalidId));
            }
            found = true;
        }
    }
    assert!(found);
}

#[shoop_test]
fn active_recording_refuses_private_publication_and_preserves_old_exports() {
    let mut h = Fixture::new(32, 7);
    let (take, _, start) = h.grabs();
    h.adopt(0, take);
    let old = h.export(0);
    let id = h.audio_candidate(take);
    h.request(protocol::Request::LoopTransition {
        loop_id: protocol::LoopSlotId(protocol::SlotId::new(0, 1)),
        mode: protocol::LoopMode::Recording,
        delay: None,
    });
    assert_eq!(
        h.op(Operation::CandidatePublish { id }),
        failed(Error::Busy)
    );
    assert_eq!(
        h.op(Operation::CandidateBegin {
            target: target(0),
            take
        }),
        failed(Error::Busy)
    );
    h.op(Operation::CandidateCancel { id });
    let mut stopped = h.engine.render.into_stopped();
    let audio = bytes(&mut stopped.resources, old.view.audio.unwrap().snapshot);
    assert_eq!(
        f32::from_le_bytes(audio[..4].try_into().unwrap()),
        (start + 1) as f32
    );
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn saturated_unread_results_do_not_interrupt_history_and_shutdown_recovers_private_work() {
    let mut h = Fixture::new(32, 32);
    let (take, _, _) = h.grabs();
    h.adopt(0, take);
    let id = h.audio_candidate(take);
    let at = h.aligned_future(128);
    let mut admitted = 0;
    loop {
        let command = h
            .engine
            .endpoint
            .loops
            .command(Frame(at), Operation::CandidateStatus { id });
        match h
            .engine
            .endpoint
            .control
            .submit_prepared(Lane::Normal, command)
        {
            Ok(_) => admitted += 1,
            Err(error) => {
                assert_eq!(error.error, Error::Busy);
                break;
            }
        }
    }
    assert!(admitted > 1);
    while h.now() < at + 512 {
        h.render(32, false);
    }
    assert_eq!(h.engine.render.phase(), Phase::Running);
    let end = h.now();
    let mut stopped = h.engine.render.into_stopped();
    assert_eq!(
        stopped
            .planner
            .history_status(0, &stopped.resources.io())
            .unwrap()
            .end,
        Frame(end)
    );
    let mut returned = 0;
    while h.engine.endpoint.control.try_execution().is_some() {
        returned += 1;
    }
    assert!(returned > 1);
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn late_graph_fault_does_not_certify_history_written_in_an_unfinished_tick() {
    let mut h = Fixture::new(32, 32);
    let (take, _, _) = h.grabs();
    let retained = h.snapshot(take.audio.unwrap());
    let start = h.now();
    h.engine
        .render
        .cells
        .fail_reader_tick
        .store(true, core::sync::atomic::Ordering::Relaxed);
    assert_eq!(h.render_result(32, true), Err(Error::InvalidState));
    assert_eq!(h.engine.render.phase(), Phase::Faulted(Error::InvalidState));
    let mut stopped = h.engine.render.into_stopped();
    let status = stopped
        .planner
        .history_status(0, &stopped.resources.io())
        .unwrap();
    assert_eq!(status.end, Frame(start));
    let (audio, _) = stopped.planner.shared_stores(0).unwrap();
    assert_eq!(
        stopped
            .resources
            .audio_coverage(audio.unwrap())
            .unwrap()
            .end,
        Frame(start + 32)
    );
    assert_eq!(
        bytes(&mut stopped.resources, retained).len(),
        retained.description.bytes
    );
    stopped.resources.release_snapshot(retained.id).unwrap();
    recover_all(&mut stopped, 12288);
}
fn bytes(resources: &mut ResourceStore, snapshot: Snapshot) -> Vec<u8> {
    let mut output = vec![0; snapshot.description.bytes];
    let mut cursor = resources.cursor(snapshot.id).unwrap();
    let mut copied = 0;
    assert_no_heap(|| {
        while copied < output.len() {
            resources.begin_callback();
            let end = (copied + 7).min(output.len());
            copied += resources
                .read(&mut cursor, &mut output[copied..end])
                .unwrap();
        }
    });
    output
}
fn expected_midi(start: u64, frames: u64, q: usize) -> Vec<(u64, Vec<u8>)> {
    (start..start + frames)
        .flat_map(|f| packets(f, q).into_iter().map(move |p| (f - start, p)))
        .collect()
}
fn midi_events(data: &[u8]) -> Vec<(u64, Vec<u8>)> {
    let mut events = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let frame = u64::from_le_bytes(data[at..at + 8].try_into().unwrap());
        let n = u32::from_le_bytes(data[at + 8..at + 12].try_into().unwrap()) as usize;
        events.push((frame, data[at + 12..at + 12 + n].to_vec()));
        at += 12 + n;
    }
    events
}

fn input_state(start: u64, end: u64, q: usize) -> MidiState {
    let mut state = MidiState::default();
    for frame in start..end {
        for packet in packets(frame, q) {
            state.apply_packet(&packet, 1024).unwrap();
        }
    }
    state
}
fn rendered_midi(
    old: &[(u64, Vec<u8>)],
    new: &[(u64, Vec<u8>)],
    initial: MidiState,
    new_initial: MidiState,
    play: u64,
    publish: u64,
    end: u64,
    length: u64,
    q: usize,
) -> Vec<(u64, Vec<u8>)> {
    let latency = if q == 1 { 0 } else { q as u64 };
    let mut output = Vec::new();
    let mut state = MidiState::default();
    let mut active = false;
    for tick in (play..end - latency).step_by(q) {
        let position = (tick - play) % length;
        let events = if tick < publish { old } else { new };
        if tick == play || tick == publish || position == 0 {
            if active {
                assert_eq!(
                    shoop_loops::midi_restore::cleanup(&state, |p| {
                        output.push((tick + latency, p.to_vec()));
                        Ok::<(), Error>(())
                    })
                    .first_error,
                    None
                );
            }
            state = if tick < publish { initial } else { new_initial };
            for (frame, packet) in events.iter().take_while(|e| e.0 < position) {
                let _ = frame;
                state.apply_packet(packet, 1024).unwrap();
            }
            assert_eq!(
                shoop_loops::midi_restore::restore(&state, |p| {
                    output.push((tick + latency, p.to_vec()));
                    Ok::<(), Error>(())
                })
                .first_error,
                None
            );
            active = true;
        }
        for (frame, packet) in events
            .iter()
            .filter(|e| e.0 >= position && e.0 < position + q as u64)
        {
            output.push((tick + latency + frame - position, packet.clone()));
            state.apply_packet(packet, 1024).unwrap();
        }
    }
    output
}

#[shoop_test]
fn partial_host_input_never_becomes_a_retroactive_history_cut() {
    let mut h = Fixture::new(32, 1);
    let Reply::History(before) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("history")
    };
    h.render(1, true);
    assert_eq!(h.engine.render.diagnostics().subranges, 0);
    let requested = h.now();
    let effective = requested.div_ceil(32) * 32;
    let seq = h.send(
        requested,
        Operation::HistoryGrab {
            source: before.id,
            start: Frame(effective - 32),
            end: Frame(effective),
        },
    );
    let Reply::Grabbed(take) = h.result(seq) else {
        panic!("eligible grab")
    };
    let result = h.last_execution.unwrap();
    assert_eq!(result.requested_frame, Frame(requested));
    assert_eq!(result.delivered_frame, Some(Frame(effective)));
    assert_eq!(h.now(), effective + 32);
    let snap = h.snapshot(take.audio.unwrap());
    for resource in [take.audio, take.midi].into_iter().flatten() {
        h.resource(ResourceRequest::Remove { resource });
    }
    let mut stopped = h.engine.render.into_stopped();
    let original = bytes(&mut stopped.resources, snap);
    assert_eq!(
        f32::from_le_bytes(original[..4].try_into().unwrap()),
        (effective - 32 + 1) as f32
    );
    stopped.resources.release_snapshot(snap.id).unwrap();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn midi_storage_refusal_keeps_observation_and_recovers_a_truthful_segment() {
    let mut h = Fixture::new(32, 1);
    let (old, sibling, start) = h.grabs();
    let retained = h.snapshot(old.midi.unwrap());
    let at = h.aligned_future(128);
    h.burst = Some((at, 65));
    let failed = h.send(at + 32, Operation::HistoryStatus { source: 0 });
    let Reply::History(gap) = h.result(failed) else {
        panic!("gap status")
    };
    assert_eq!(gap.failure, Some(Error::Capacity));
    assert_eq!(gap.end, Frame(at));
    let Reply::History(recovered) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("recovery")
    };
    assert_eq!(recovered.failure, None);
    assert_ne!(recovered.id, gap.id);
    assert!(recovered.start >= Frame(at + 32));
    let stale = h.op(Operation::HistoryGrab {
        source: gap.id,
        start: Frame(at - 32),
        end: Frame(at),
    });
    assert_eq!(
        stale,
        Reply::Failed(shoop_loops::runtime::Failure::Runtime(Error::InvalidId))
    );
    let next = h.aligned_future(128);
    let seq = h.send(
        next,
        Operation::HistoryGrab {
            source: recovered.id,
            start: Frame(next - 32),
            end: Frame(next),
        },
    );
    let Reply::Grabbed(fresh) = h.result(seq) else {
        panic!("recovered grab")
    };
    let current = h.snapshot(fresh.midi.unwrap());
    for take in [old, sibling, fresh] {
        for resource in [take.audio, take.midi].into_iter().flatten() {
            h.resource(ResourceRequest::Remove { resource });
        }
    }
    let mut stopped = h.engine.render.into_stopped();
    assert_eq!(
        midi_events(&bytes(&mut stopped.resources, retained)),
        expected_midi(start, 128, 32)
    );
    let index = stopped.resources.read_index(current.id).unwrap();
    assert_eq!(
        stopped
            .resources
            .render_io()
            .midi_seek(index, 0)
            .unwrap()
            .state
            .channel(15)
            .unwrap()
            .controller(7),
        Some(64)
    );
    stopped.resources.release_snapshot(retained.id).unwrap();
    stopped.resources.release_snapshot(current.id).unwrap();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn audio_rerouting_preserves_the_unchanged_midi_sources_pre_cut_projection() {
    let mut h = Fixture::new(32, 7);
    let (old, _, start) = h.grabs();
    let retained = h.snapshot(old.audio.unwrap());
    let strip = protocol::StripId(protocol::SlotId::new(0, 1));
    h.request(protocol::Request::Connect {
        port: protocol::EnginePort::Strip(protocol::StripPort {
            strip,
            kind: protocol::PortKind::AudioIn,
            index: 0,
        }),
        host: protocol::HostPortId(0),
        connected: false,
    });
    let Reply::History(status) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("history")
    };
    let at = h.aligned_future(128);
    let seq = h.send(
        at,
        Operation::HistoryGrab {
            source: status.id,
            start: Frame(at - 32),
            end: Frame(at),
        },
    );
    let Reply::Grabbed(fresh) = h.result(seq) else {
        panic!("fresh segment")
    };
    let midi = h.snapshot(fresh.midi.unwrap());
    let audio = h.snapshot(fresh.audio.unwrap());
    let expected = input_state(h.stimulus.unwrap(), at - 32, 32);
    let mut stopped = h.engine.render.into_stopped();
    let index = stopped.resources.read_index(midi.id).unwrap();
    assert_eq!(
        stopped
            .resources
            .render_io()
            .midi_seek(index, 0)
            .unwrap()
            .state,
        expected
    );
    let samples = bytes(&mut stopped.resources, audio);
    for (i, frame) in samples.chunks_exact(8).enumerate() {
        assert_eq!(f32::from_le_bytes(frame[..4].try_into().unwrap()), 0.);
        assert_eq!(
            f32::from_le_bytes(frame[4..].try_into().unwrap()),
            -2. * (at - 32 + i as u64 + 1) as f32
        );
    }
    let previous = bytes(&mut stopped.resources, retained);
    assert_eq!(
        f32::from_le_bytes(previous[..4].try_into().unwrap()),
        (start + 1) as f32
    );
    for snapshot in [retained, midi, audio] {
        stopped.resources.release_snapshot(snapshot.id).unwrap();
    }
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn paired_grab_refusal_releases_the_first_private_owner_and_reroutes_invalidate_source_ids() {
    let mut h = Fixture::new(32, 7);
    let Reply::History(status) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("history")
    };
    let mut empty = Vec::new();
    loop {
        match h.resource_result(ResourceRequest::Create) {
            Ok(resource::Reply::Created(id)) => empty.push(id),
            Err(Error::Capacity) => break,
            other => panic!("{other:?}"),
        }
    }
    let released = empty.pop().unwrap();
    h.resource(ResourceRequest::Remove { resource: released });
    let versions = h.engine.render.resource_usage().free_versions;
    let at = h.aligned_future(128);
    let seq = h.send(
        at,
        Operation::HistoryGrab {
            source: status.id,
            start: Frame(at - 32),
            end: Frame(at),
        },
    );
    assert_eq!(
        h.result(seq),
        Reply::Failed(shoop_loops::runtime::Failure::Runtime(Error::Capacity))
    );
    for resource in empty {
        h.resource(ResourceRequest::Remove { resource });
    }
    assert!(h.engine.render.resource_usage().free_versions >= versions);
    let strip = protocol::StripId(protocol::SlotId::new(0, 1));
    h.request(protocol::Request::Connect {
        port: protocol::EnginePort::Strip(protocol::StripPort {
            strip,
            kind: protocol::PortKind::MidiIn,
            index: 0,
        }),
        host: protocol::HostPortId(4),
        connected: false,
    });
    let Reply::History(next) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("new source")
    };
    assert_ne!(next.id, status.id);
    assert_eq!(
        h.op(Operation::HistoryGrab {
            source: status.id,
            start: Frame(at - 32),
            end: Frame(at)
        }),
        Reply::Failed(shoop_loops::runtime::Failure::Runtime(Error::StaleEpoch))
    );
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn candidate_preparation_is_fair_callback_bounded_and_cancellation_invalidates_the_id() {
    let mut h = Fixture::new(32, 128);
    let (first, second, _) = h.grabs();
    h.adopt(0, first);
    h.adopt(1, second);
    let Reply::Candidate { id: a, .. } = h.op(Operation::CandidateBegin {
        target: target(0),
        take: first,
    }) else {
        panic!("candidate a")
    };
    let Reply::Candidate { id: b, .. } = h.op(Operation::CandidateBegin {
        target: target(1),
        take: second,
    }) else {
        panic!("candidate b")
    };
    let patch = h.snapshot(first.midi.unwrap());
    let start = h.now().div_ceil(128) * 128;
    h.run_to(start);
    let at = start + 128;
    let aa = h.send(
        at,
        Operation::CandidateMidi {
            id: a,
            position: 0,
            input: patch.id,
        },
    );
    let bb = h.send(
        at,
        Operation::CandidateMidi {
            id: b,
            position: 0,
            input: patch.id,
        },
    );
    let ac = h.send(at + 32, Operation::CandidateStatus { id: a });
    let bc = h.send(at + 32, Operation::CandidateStatus { id: b });
    let w1 = h.send(at + 32, Operation::SharedWork);
    let cancel = h.send(at + 64, Operation::CandidateCancel { id: a });
    let w2 = h.send(at + 96, Operation::SharedWork);
    h.run_to(at + 128);
    assert!(matches!(
        h.result(aa),
        Reply::Candidate {
            status: CandidateStatus::Building,
            ..
        }
    ));
    assert!(matches!(
        h.result(bb),
        Reply::Candidate {
            status: CandidateStatus::Building,
            ..
        }
    ));
    let Reply::Candidate { steps: as_, .. } = h.result(ac) else {
        panic!("progress a")
    };
    let Reply::Candidate { steps: bs_, .. } = h.result(bc) else {
        panic!("progress b")
    };
    assert_eq!((as_, bs_), (4, 4));
    let Reply::SharedWork(work1) = h.result(w1) else {
        panic!("work")
    };
    let Reply::SharedWork(work2) = h.result(w2) else {
        panic!("work")
    };
    assert_eq!(work1.edit_steps, 8);
    assert_eq!(work2.edit_steps, 8);
    assert!(work2.jobs_scanned <= 16);
    assert_eq!(h.result(cancel), Reply::Done);
    assert_eq!(
        h.op(Operation::CandidateStatus { id: a }),
        Reply::Failed(shoop_loops::runtime::Failure::Runtime(Error::InvalidId))
    );
    let mut ready = false;
    for _ in 0..16 {
        if matches!(
            h.op(Operation::CandidateStatus { id: b }),
            Reply::Candidate {
                status: CandidateStatus::Ready,
                ..
            }
        ) {
            ready = true;
            break;
        }
    }
    assert!(ready);
    assert_eq!(h.op(Operation::CandidateCancel { id: b }), Reply::Done);
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    for take in [first, second] {
        for resource in [take.audio, take.midi].into_iter().flatten() {
            h.resource(ResourceRequest::Remove { resource });
        }
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn history_playback_private_preparation_and_ready_publication_survive_exhausted_background_copy() {
    let mut h = Fixture::new(32, 128);
    let (first, second, _) = h.grabs();
    h.adopt(0, first);
    h.adopt(1, second);
    h.op(Operation::Transition {
        target: target(0),
        timing: Timing::At,
        transition: Transition::Play {
            position: 0,
            retrigger: false,
        },
    });
    let a = h.audio_candidate(first);
    let Reply::Candidate { id: b, .. } = h.op(Operation::CandidateBegin {
        target: target(1),
        take: second,
    }) else {
        panic!("candidate")
    };
    let patch = h.snapshot(second.midi.unwrap());
    let source = h.snapshot(first.audio.unwrap());
    let resource::Reply::Cursor(cursor) = h.resource(ResourceRequest::Cursor {
        snapshot: source.id,
    }) else {
        panic!("cursor")
    };
    let at = h.now().div_ceil(128) * 128 + 256;
    let start_reader = Command::Schedule {
        at: Frame(at),
        action: rt_runtime::schedule::Action::Processor {
            node: h.engine.endpoint.pool.reader(),
            operation: crate::EngineOp::ReadStart {
                cursor,
                bytes: 1024,
            },
        },
        payload: Box::new([]),
    };
    h.engine
        .endpoint
        .control
        .submit_prepared(Lane::Normal, start_reader)
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    let publish = h.send(at + 32, Operation::CandidatePublish { id: a });
    let build = h.send(
        at + 32,
        Operation::CandidateMidi {
            id: b,
            position: 0,
            input: patch.id,
        },
    );
    let work = h.send(at + 64, Operation::SharedWork);
    h.run_to(at);
    for _ in 0..(h.transfer_bytes - 1024) / 256 {
        h.engine
            .endpoint
            .control
            .submit_prepared(
                Lane::Normal,
                Command::Resource(ResourceCommand::new(ResourceRequest::Read {
                    cursor,
                    bytes: vec![0; 256].into_boxed_slice(),
                })),
            )
            .unwrap_or_else(|e| panic!("{:?}", e.error));
    }
    h.render(128, true);
    assert!(
        matches!(h.result(publish), Reply::CandidatePublished { frame, .. } if frame == Frame(at + 32))
    );
    assert!(matches!(
        h.result(build),
        Reply::Candidate {
            status: CandidateStatus::Building,
            ..
        }
    ));
    let Reply::SharedWork(work) = h.result(work) else {
        panic!("work")
    };
    assert_eq!(work.background_bytes_remaining, 0);
    assert!(work.edit_steps > 0 && work.edit_steps <= 8);
    assert_eq!(work.history_failures, 0);
    assert!(h.samples[0][(at + 32) as usize..(at + 128) as usize]
        .iter()
        .all(|sample| *sample > 0.));
    h.op(Operation::CandidateCancel { id: b });
    h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
    h.resource(ResourceRequest::ReleaseSnapshot {
        snapshot: source.id,
    });
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn shared_grabs_private_wrapping_edits_and_fixed_playback_preserve_old_owners() {
    for q in [1, 16, 32, 64, 128] {
        for partition in [1, 7, 31, 128] {
            let mut h = Fixture::new(q, partition);
            let (take, sibling, start) = h.grabs();
            h.adopt(0, take);
            h.adopt(1, sibling);
            let old = h.export(0);
            let other = h.export(1);
            let play = h.aligned_future(256);
            let play_result = h.send(
                play,
                Operation::Transition {
                    target: target(0),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            );
            assert!(matches!(h.result(play_result), Reply::Transition(_)));
            let Reply::Candidate { id, .. } = h.op(Operation::CandidateBegin {
                target: target(0),
                take,
            }) else {
                panic!("candidate begin")
            };
            let Reply::History(status) = h.op(Operation::HistoryStatus { source: 0 }) else {
                panic!("history")
            };
            let patch_cut = h.aligned_future((128 + 4 * q) as u64);
            let patch_start = patch_cut - 2 * q as u64;
            let patch_seq = h.send(
                patch_cut,
                Operation::HistoryGrab {
                    source: status.id,
                    start: Frame(patch_start),
                    end: Frame(patch_cut),
                },
            );
            let Reply::Grabbed(patch) = h.result(patch_seq) else {
                panic!("patch grab")
            };
            let patch_audio = h.snapshot(patch.audio.unwrap());
            let patch_midi = h.snapshot(patch.midi.unwrap());
            let length = take.frames;
            let audio_position = length - (q.max(2) / 2) as u64;
            assert!(matches!(
                h.op(Operation::CandidateAudio {
                    id,
                    position: audio_position,
                    input: patch_audio.id,
                    offset: 0,
                    frames: q
                }),
                Reply::Candidate {
                    status: CandidateStatus::Ready,
                    ..
                }
            ));
            let at = h.aligned_future(128);
            let editing = h.send(
                at,
                Operation::CandidateMidi {
                    id,
                    position: length - 1,
                    input: patch_midi.id,
                },
            );
            let early = h.send(at, Operation::CandidatePublish { id });
            assert!(matches!(
                h.result(editing),
                Reply::Candidate {
                    status: CandidateStatus::Building,
                    ..
                }
            ));
            assert_eq!(
                h.result(early),
                Reply::Failed(shoop_loops::runtime::Failure::Runtime(Error::Busy))
            );
            for _ in 0..64 {
                if matches!(
                    h.op(Operation::CandidateStatus { id }),
                    Reply::Candidate {
                        status: CandidateStatus::Ready,
                        ..
                    }
                ) {
                    break;
                }
            }
            let publish = h.aligned_future(256);
            let seq = h.send(publish, Operation::CandidatePublish { id });
            let Reply::CandidatePublished {
                take: edited,
                frame,
                ..
            } = h.result(seq)
            else {
                panic!("publication")
            };
            assert_eq!(frame, Frame(publish));
            let final_export = h.export(0);
            let end = h.aligned_future(2 * length + 256);
            h.run_to(end);
            let latency = if q == 1 { 0 } else { q as u64 };
            let mut expected: Vec<f32> = (0..length).map(|i| (start + i + 1) as f32).collect();
            for i in 0..q {
                expected[(audio_position as usize + i) % length as usize] =
                    (patch_start + i as u64 + 1) as f32;
            }
            for f in play + latency..end {
                let pos = ((f - play - latency) % length) as usize;
                let value = if f < publish + latency {
                    (start + pos as u64 + 1) as f32
                } else {
                    expected[pos]
                };
                assert_eq!(
                    h.samples[0][f as usize], value,
                    "Q={q}, partition={partition}, frame={f}"
                );
                assert_eq!(h.samples[1][f as usize], -2. * value);
            }
            h.resource(ResourceRequest::ReleaseSnapshot {
                snapshot: patch_audio.id,
            });
            h.resource(ResourceRequest::ReleaseSnapshot {
                snapshot: patch_midi.id,
            });
            for take in [take, sibling, patch, edited] {
                for resource in [take.audio, take.midi].into_iter().flatten() {
                    h.resource(ResourceRequest::Remove { resource });
                }
            }
            let mut stopped = h.engine.render.into_stopped();
            assert_eq!(
                midi_events(&bytes(
                    &mut stopped.resources,
                    old.view.midi.unwrap().snapshot
                )),
                expected_midi(start, length, q)
            );
            assert_eq!(
                midi_events(&bytes(
                    &mut stopped.resources,
                    other.view.midi.unwrap().snapshot
                )),
                expected_midi(start + q as u64, length, q)
            );
            let original = bytes(&mut stopped.resources, old.view.audio.unwrap().snapshot);
            for (i, data) in original.chunks_exact(8).enumerate() {
                let value = (start + i as u64 + 1) as f32;
                assert_eq!(f32::from_le_bytes(data[..4].try_into().unwrap()), value);
                assert_eq!(
                    f32::from_le_bytes(data[4..].try_into().unwrap()),
                    -2. * value
                );
            }
            let changed = bytes(
                &mut stopped.resources,
                final_export.view.audio.unwrap().snapshot,
            );
            for (i, data) in changed.chunks_exact(8).enumerate() {
                assert_eq!(
                    f32::from_le_bytes(data[..4].try_into().unwrap()),
                    expected[i]
                );
            }
            let mut events = expected_midi(start, length, q);
            let inserted = expected_midi(patch_start, 2 * q as u64, q);
            let position = length - 1;
            events.retain(|(f, _)| !(*f >= position || *f < (position + 2 * q as u64) % length));
            for (frame, packet) in inserted {
                events.push(((position + frame) % length, packet));
            }
            events.sort_by_key(|e| e.0);
            let old_initial = input_state(h.stimulus.unwrap(), start, q);
            let mut new_initial = input_state(h.stimulus.unwrap(), patch_start, q);
            for (_, packet) in expected_midi(patch_start, 2 * q as u64, q)
                .iter()
                .take_while(|e| e.0 < 1)
            {
                new_initial.apply_packet(packet, 1024).unwrap();
            }
            let expected_packets = rendered_midi(
                &expected_midi(start, length, q),
                &events,
                old_initial,
                new_initial,
                play,
                publish,
                end,
                length,
                q,
            );
            let actual_packets: Vec<_> = h
                .packets
                .iter()
                .filter(|(frame, _)| *frame >= play + latency && *frame < end)
                .cloned()
                .collect();
            assert_eq!(
                actual_packets, expected_packets,
                "MIDI Q={q}, partition={partition}"
            );
            assert_eq!(
                midi_events(&bytes(
                    &mut stopped.resources,
                    final_export.view.midi.unwrap().snapshot
                )),
                events
            );
            recover_all(&mut stopped, 12288);
        }
    }
}

#[shoop_test]
fn host_partitions_agree_at_identical_history_playback_and_replacement_cuts() {
    for q in [1, 16, 32, 64, 128] {
        let mut baseline = None;
        for partition in [1, 7, 31, 128] {
            let mut h = Fixture::new(q, partition);
            let cut = (h.stimulus.unwrap() + 4096).div_ceil(128) * 128;
            let Reply::History(history) = h.op(Operation::HistoryStatus { source: 0 }) else {
                panic!("history")
            };
            let first = h.send(
                cut,
                Operation::HistoryGrab {
                    source: history.id,
                    start: Frame(cut - 4 * q as u64),
                    end: Frame(cut),
                },
            );
            let second = h.send(
                cut,
                Operation::HistoryGrab {
                    source: history.id,
                    start: Frame(cut - 5 * q as u64),
                    end: Frame(cut - q as u64),
                },
            );
            let Reply::Grabbed(take) = h.result(first) else {
                panic!("take")
            };
            let Reply::Grabbed(sibling) = h.result(second) else {
                panic!("sibling")
            };
            h.adopt(0, take);
            h.adopt(1, sibling);
            let play = cut + 8192;
            assert!(h.now() < play);
            let started = h.send(
                play,
                Operation::Transition {
                    target: target(0),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            );
            assert!(matches!(h.result(started), Reply::Transition(_)));
            assert_eq!(h.last_execution.unwrap().delivered_frame, Some(Frame(play)));
            let id = h.audio_candidate(take);
            let patch = h.snapshot(sibling.midi.unwrap());
            h.op(Operation::CandidateMidi {
                id,
                position: take.frames - 1,
                input: patch.id,
            });
            h.resource(ResourceRequest::ReleaseSnapshot { snapshot: patch.id });
            let mut ready = false;
            for _ in 0..128 {
                if matches!(
                    h.op(Operation::CandidateStatus { id }),
                    Reply::Candidate {
                        status: CandidateStatus::Ready,
                        ..
                    }
                ) {
                    ready = true;
                    break;
                }
            }
            assert!(ready);
            let publish = play + 8192;
            assert!(h.now() < publish);
            let selected = h.send(publish, Operation::CandidatePublish { id });
            assert!(
                matches!(h.result(selected), Reply::CandidatePublished { frame, .. } if frame == Frame(publish))
            );
            let latency = if q == 1 { 0 } else { q as u64 };
            let end = publish + 3 * take.frames + latency + 128;
            h.run_to(end);
            let audio = [
                h.samples[0][(play + latency) as usize..end as usize].to_vec(),
                h.samples[1][(play + latency) as usize..end as usize].to_vec(),
            ];
            let midi: Vec<_> = h
                .packets
                .iter()
                .filter(|(frame, _)| *frame >= play + latency && *frame < end)
                .cloned()
                .collect();
            if let Some(expected) = &baseline {
                assert_eq!(&(audio, midi), expected, "Q={q}, host={partition}");
            } else {
                baseline = Some((audio, midi));
            }
            let mut stopped = h.engine.render.into_stopped();
            recover_all(&mut stopped, 12288);
        }
    }
}

#[shoop_test]
fn shared_raw_windows_retain_history_across_manual_periods_and_host_partitions() {
    use shoop_loops::windows::{ChannelUpdate, Edit};
    for q in [1, 16, 32, 64, 128] {
        for partition in [1, 7, 31, 128] {
            let mut h = Fixture::new(q, partition);
            let (take, _, start) = h.grabs();
            h.adopt(0, take);
            let old = h.export(0);
            let Reply::State(before) = h.op(Operation::State { target: target(0) }) else {
                panic!()
            };
            let Reply::Timing(short) = h.op(Operation::EditWindows {
                target: target(0),
                expected_revision: before.timing_revision,
                expected_content: before.state.content_revision,
                edit: Edit {
                    length: Some(2 * q as u64),
                    ..Default::default()
                },
            }) else {
                panic!("period edit")
            };
            let mut change = Edit {
                count: 3,
                ..Default::default()
            };
            change.channels[0] = ChannelUpdate {
                channel: 0,
                start_offset: Some(1),
                preplay: Some(q as u32 + 1),
                ..Default::default()
            };
            change.channels[1] = ChannelUpdate {
                channel: 1,
                start_offset: Some(-1),
                ..Default::default()
            };
            change.channels[2] = ChannelUpdate {
                midi: true,
                channel: 0,
                start_offset: Some(take.frames as i32 + 1),
                ..Default::default()
            };
            let Reply::Timing(edited) = h.op(Operation::EditWindows {
                target: target(0),
                expected_revision: short.timing_revision,
                expected_content: short.state.content_revision,
                edit: change,
            }) else {
                panic!("channel edit")
            };
            assert_eq!(
                (edited.raw_audio_frames, edited.raw_midi_frames),
                (take.frames, take.frames)
            );
            let play = h.aligned_future(256);
            let command = h.send(
                play,
                Operation::Transition {
                    target: target(0),
                    timing: Timing::At,
                    transition: Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                },
            );
            assert!(matches!(h.result(command), Reply::Transition(_)));
            let latency = if q == 1 { 0 } else { q as u64 };
            h.run_to(h.now().max(play + 5 * q as u64));
            for frame in play + latency..play + 5 * q as u64 {
                let logical = (frame - play - latency) % (2 * q as u64);
                assert_eq!(
                    h.samples[0][frame as usize],
                    (start + logical + 2) as f32,
                    "Q={q}, partition={partition}"
                );
                assert_eq!(
                    h.samples[1][frame as usize],
                    if logical == 0 {
                        0.
                    } else {
                        -2. * (start + logical) as f32
                    }
                );
            }
            h.op(Operation::Transition {
                target: target(0),
                timing: Timing::At,
                transition: Transition::Stop,
            });
            let Reply::Timing(extended) = h.op(Operation::EditWindows {
                target: target(0),
                expected_revision: edited.timing_revision,
                expected_content: edited.state.content_revision,
                edit: Edit {
                    length: Some(take.frames),
                    ..Default::default()
                },
            }) else {
                panic!()
            };
            assert_eq!(
                extended.raw_audio_frames,
                old.view.audio.unwrap().snapshot.description.frames
            );
            let mut stopped = h.engine.render.into_stopped();
            let raw = bytes(&mut stopped.resources, old.view.audio.unwrap().snapshot);
            assert_eq!(raw.len() as u64, take.frames * 8);
            recover_all(&mut stopped, 12288);
        }
    }
}

#[shoop_test]
fn timing_edits_fence_an_already_prepared_shared_candidate() {
    let mut h = Fixture::new(32, 7);
    let (take, _, _) = h.grabs();
    h.adopt(0, take);
    let candidate = h.audio_candidate(take);
    let Reply::State(before) = h.op(Operation::State { target: target(0) }) else {
        panic!()
    };
    let Reply::Timing(edited) = h.op(Operation::EditWindows {
        target: target(0),
        expected_revision: before.timing_revision,
        expected_content: before.state.content_revision,
        edit: shoop_loops::windows::Edit {
            preplay: Some(7),
            ..Default::default()
        },
    }) else {
        panic!()
    };
    assert_eq!(
        h.op(Operation::CandidatePublish { id: candidate }),
        failed(Error::StaleRevision)
    );
    h.op(Operation::CandidateCancel { id: candidate });
    let Reply::State(after) = h.op(Operation::State { target: target(0) }) else {
        panic!()
    };
    assert_eq!(
        (after.windows, after.timing_revision),
        (edited.windows, edited.timing_revision)
    );
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}

#[shoop_test]
fn pooled_indexed_midi_histories_drive_mixed_grabs_ordinary_playback_and_reclaim_all_sources() {
    let q = 32;
    let mut h = Fixture::new_prepared_shapes(q, 7, 1000, true, true, true, true);
    let strip = protocol::StripId(protocol::SlotId::new(1, 1));
    h.request(protocol::Request::CreateStrip {
        strip,
        shape: protocol::StripShape {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_input: true,
            midi_output: true,
        },
    });
    let (take, sibling, first) = h.grabs();
    assert!(take.indexed_midi_store.is_some());
    assert_eq!(take.indexed_midi_store, sibling.indexed_midi_store);
    assert_eq!(take.midi_store, None);
    let snapshot = h.snapshot(take.midi.unwrap());
    let actual = h.read(snapshot);
    let mut expected = Vec::new();
    for frame in first..first + take.frames {
        for packet in packets(frame, q) {
            let mut record = vec![0; 12 + packet.len()];
            resource::encode_midi_record(frame - first, &packet, &mut record).unwrap();
            expected.extend_from_slice(&record);
        }
    }
    assert_eq!(actual, expected);
    let old = h.snapshot(take.audio.unwrap());
    let old_audio = h.read(old);
    let Reply::History(independent) = h.op(Operation::HistoryStatus { source: 1 }) else {
        panic!("second history")
    };
    let at = h.aligned_future(256);
    let sequence = h.send(
        at,
        Operation::HistoryGrab {
            source: independent.id,
            start: Frame(at - 64),
            end: Frame(at),
        },
    );
    let Reply::Grabbed(silent) = h.result(sequence) else {
        panic!("silent history")
    };
    assert_eq!(silent.indexed_midi_store, take.indexed_midi_store);
    let empty = h.snapshot(silent.midi.unwrap());
    assert_eq!(empty.description.bytes, 0);
    h.adopt(0, take);
    h.op(Operation::Transition {
        target: target(0),
        timing: Timing::At,
        transition: Transition::Play {
            position: 0,
            retrigger: false,
        },
    });
    h.run_to(h.aligned_future(256));
    assert_eq!(h.read(snapshot), expected);
    assert_eq!(h.read(old), old_audio);
    let Reply::History(final_history) = h.op(Operation::HistoryStatus { source: 0 }) else {
        panic!("final history")
    };
    assert_eq!(final_history.failure, None);
    let Reply::SharedCapacity(capacity) = h.op(Operation::SharedCapacity) else {
        panic!("capacity")
    };
    assert_eq!(capacity.reserved_payload_bytes, (2048 + 1024) * 256);
    assert!(capacity.midi_state_bytes > 0);
    assert!(capacity.store_metadata_bytes < 32 * 1024 * 1024);
    for snapshot in [snapshot, old, empty] {
        h.resource(ResourceRequest::ReleaseSnapshot {
            snapshot: snapshot.id,
        });
    }
    let mut stopped = h.engine.render.into_stopped();
    recover_all(&mut stopped, 12288);
}
