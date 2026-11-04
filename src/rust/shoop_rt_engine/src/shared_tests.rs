use super::*;
use crate::{prepare_with_shared, Engine, EngineDescription, HostPorts, Segment};
use alloc::vec::Vec;
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
use shoop_wasm_test_support::{
    heap::{assert_no_heap, TrackingAllocator},
    shoop_test,
};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

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
    fn new(q: usize, partition: usize) -> Self {
        let d = EngineDescription {
            quantum: q,
            sample_rate: 1000,
            max_frames: 128,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: 1,
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
                sample_rate: 1000,
                buffer_frames: 128,
            },
            SharedSettings {
                histories: vec![HistorySettings {
                    source: 0,
                    audio: Some(resource::shared_audio::Config {
                        channels: 2,
                        history_frames: (8 * q) as u64,
                        views: 24,
                        pages: 1024,
                    }),
                    midi: Some(resource::shared_midi::Config {
                        history_frames: (8 * q) as u64,
                        chunks: 8,
                        views: 24,
                        pages: 8192,
                        max_events: 64,
                        max_bytes: 2048,
                        max_packet: 1024,
                        transactions: 2,
                        edit_steps: 8,
                    }),
                }],
                candidates: 2,
                edit_steps_per_callback: 8,
            },
        )
        .unwrap();
        let outbox = engine.outbox();
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
            }
        }
        let [left, right] = &mut self.output;
        assert_no_heap(|| {
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
        })
        .unwrap();
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
    }
    fn run_to(&mut self, end: u64) {
        assert!(end >= self.now());
        while self.now() < end {
            self.render(self.partition.min((end - self.now()) as usize), true);
        }
    }
    fn request(&mut self, request: protocol::Request) {
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
                    assert_eq!(outcome, protocol::Outcome::Done, "{request:?}");
                    return;
                }
            }
            self.render(self.q, false);
        }
        panic!("request pending {request:?}");
    }
    fn send(&mut self, at: u64, operation: Operation) -> Sequence {
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
                assert_eq!(c.outcome, rt_runtime::transport::Outcome::Applied);
                let Command::Resource(command) = c.request else {
                    panic!("resource completion")
                };
                return command.reply().unwrap();
            }
            self.render(self.partition, true);
        }
        panic!("resource pending");
    }
    fn snapshot(&mut self, resource: rt_runtime::ResourceId) -> Snapshot {
        let resource::Reply::Snapshot(s) = self.resource(ResourceRequest::Snapshot { resource })
        else {
            panic!("snapshot")
        };
        s
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
        let Reply::Export { media, .. } = self.op(Operation::Export {
            target: target(slot),
            selection: Selection::Playback,
        }) else {
            panic!("export")
        };
        media
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
            assert_eq!(
                midi_events(&bytes(
                    &mut stopped.resources,
                    final_export.view.midi.unwrap().snapshot
                )),
                events
            );
            stopped.cleanup().unwrap();
            assert_eq!(stopped.resource_usage().free_pages, 12288);
        }
    }
}
