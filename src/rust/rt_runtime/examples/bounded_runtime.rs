//! Reproducible native callback measurements; not a device/deadline guarantee.
#[path = "../tests/support/mod.rs"]
mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{
    compile, Connection, Endpoint, GraphDescription, PortCounts, PortType, PreparedPlan,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::*;
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{Budgets, Config, Error, NodeId, ResourceId, Revision};
use std::io::Write;
use std::time::Instant;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
const FRAMES: usize = 128;
const RATE: usize = 48_000;
const CHUNK: usize = 2048;
const DEADLINE_NS: u64 = FRAMES as u64 * 1_000_000_000 / RATE as u64;

#[derive(Clone, Copy, PartialEq)]
enum Case {
    Idle,
    Dag,
    Dense,
    Midi,
    Capture,
    Graph,
    Transfer,
}
impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Dag => "dag-16",
            Self::Dense => "dense-dag-64",
            Self::Midi => "midi-64-through-8",
            Self::Capture => "stereo-capture",
            Self::Graph => "graph-transfers",
            Self::Transfer => "resource-roundtrip",
        }
    }
    fn nodes(self) -> usize {
        match self {
            Self::Idle => 0,
            Self::Dense => 64,
            Self::Dag | Self::Graph => 16,
            Self::Midi | Self::Transfer => 8,
            Self::Capture => 1,
        }
    }
}
struct Dsp {
    midi: bool,
    gain: f32,
    recording: Option<ResourceId>,
}
impl Processor for Dsp {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: usize::from(self.midi),
            midi_outputs: usize::from(self.midi),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        for port in 0..2 {
            let (source, target) = block.audio.pair(port, port).ok_or(Error::InvalidShape)?;
            for (source, target) in source.iter().zip(target) {
                *target = *source * self.gain + 0.03125;
            }
        }
        if self.midi {
            for index in 0..block.midi_inputs[0].len() {
                let (offset, packet) =
                    block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
                block.midi_outputs[0].push(offset, block.frames, packet)?;
            }
        }
        if let Some(resource) = self.recording {
            if let Ok(capture) = block.resources.active_capture(resource) {
                let mut samples = [0.; FRAMES * 2];
                for channel in 0..2 {
                    for (frame, sample) in block
                        .audio
                        .output(channel)
                        .ok_or(Error::InvalidShape)?
                        .iter()
                        .enumerate()
                    {
                        samples[frame * 2 + channel] = *sample;
                    }
                }
                block.resources.capture_audio(
                    capture,
                    block.frame,
                    &samples[..block.frames * 2],
                )?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy)]
enum Transfer {
    Begin,
    Write(UploadId, usize),
    Finish(UploadId),
    Commit(UploadId),
    Snapshot,
    Cursor(SnapshotId),
    Read(Cursor),
    Release(SnapshotId),
}
#[derive(Default)]
struct Metrics {
    copy: usize,
    steps: usize,
    nodes: usize,
    edges: usize,
    pages: usize,
    owners: usize,
    total_copy: u64,
    control_bytes: u64,
    completed: usize,
    refusals: usize,
    activations: usize,
}
type Returned = Completion<Command<Dsp>, Retired<Dsp>>;
struct Fixture {
    case: Case,
    config: Config,
    control: Control<Dsp>,
    runtime: GraphRuntime<Dsp>,
    input: [[f32; FRAMES]; 2],
    output: [[f32; FRAMES]; 2],
    midi_in: Vec<MidiBuffer>,
    midi_out: Vec<MidiBuffer>,
    recovery: MidiBuffer,
    expected: [f32; 2],
    swapped: bool,
    next_plan: Option<PreparedPlan>,
    pending: bool,
    resource: Option<ResourceId>,
    revision: Revision,
    capture: Option<CaptureId>,
    transfer: Transfer,
    upload: Option<UploadId>,
    snapshot: Option<SnapshotId>,
    bytes: Option<Box<[u8]>>,
    metrics: Metrics,
}
fn node(index: usize) -> NodeId {
    NodeId::from_parts(index as u32, 1).unwrap()
}
impl Fixture {
    fn plan(&self, swapped: bool, empty: bool) -> PreparedPlan {
        let count = if empty { 0 } else { self.case.nodes() };
        let mut graph = GraphDescription {
            nodes: (0..count).map(node).collect(),
            connections: Vec::new(),
        };
        for index in 0..count {
            for target in 0..2 {
                for source in 0..2 {
                    if source != target && (index == 0 || self.case != Case::Dense) {
                        continue;
                    }
                    graph.connections.push(Connection {
                        source: if index == 0 {
                            Endpoint::host(PortType::Audio, source)
                        } else {
                            Endpoint::node(node(index - 1), PortType::Audio, source)
                        },
                        destination: Endpoint::node(node(index), PortType::Audio, target),
                    });
                }
            }
            if self.case == Case::Midi {
                graph.connections.push(Connection {
                    source: if index == 0 {
                        Endpoint::host(PortType::Midi, 0)
                    } else {
                        Endpoint::node(node(index - 1), PortType::Midi, 0)
                    },
                    destination: Endpoint::node(node(index), PortType::Midi, 0),
                });
            }
        }
        if count > 0 {
            for channel in 0..2 {
                graph.connections.push(Connection {
                    source: Endpoint::node(
                        node(count - 1),
                        PortType::Audio,
                        if swapped { 1 - channel } else { channel },
                    ),
                    destination: Endpoint::host(PortType::Audio, channel),
                });
            }
            if self.case == Case::Midi {
                graph.connections.push(Connection {
                    source: Endpoint::node(node(count - 1), PortType::Midi, 0),
                    destination: Endpoint::host(PortType::Midi, 0),
                });
            }
        }
        compile(self.runtime.catalog(), &graph).unwrap()
    }
    fn new(case: Case) -> Self {
        let config = Config {
            sample_rate: RATE as u32,
            max_frames: FRAMES,
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: usize::from(case == Case::Midi),
            midi_outputs: usize::from(case == Case::Midi),
            max_nodes: 64,
            max_ports: if case == Case::Midi { 390 } else { 260 },
            max_edges: 256,
            midi_events_per_port: 64,
            midi_bytes_per_port: 256,
            command_slots: 4,
            resource_slots: 16,
            resource_pages: if case == Case::Transfer {
                64
            } else if case == Case::Capture {
                256
            } else {
                16
            },
            page_bytes: if case == Case::Transfer { CHUNK } else { 256 },
            resource_io_bytes: if case == Case::Capture { FRAMES * 8 } else { 0 },
            budgets: Budgets {
                commands: 8,
                publications: 8,
                transfer_bytes: 2048,
                validation_steps: 128,
                events: 64,
                ..Budgets::default()
            },
            ..Config::default()
        };
        let prepared = config.prepare().unwrap();
        let mut store = ResourceStore::new(prepared).unwrap();
        let resource = if matches!(case, Case::Capture | Case::Transfer) {
            Some(store.create().unwrap())
        } else {
            None
        };
        let processors = (0..case.nodes())
            .map(|index| {
                PreparedNode::new(
                    node(index),
                    Dsp {
                        midi: case == Case::Midi,
                        gain: if case == Case::Dense { 0.375 } else { 0.75 },
                        recording: if case == Case::Capture {
                            resource
                        } else {
                            None
                        },
                    },
                )
            })
            .collect();
        let (mut control, runtime) =
            GraphRuntime::prepare_with_resources(prepared, processors, store).unwrap();
        control.prepare_submissions().unwrap();
        let mut midi_in = Vec::new();
        let mut midi_out = Vec::new();
        if case == Case::Midi {
            let mut input = MidiBuffer::new(64, 256).unwrap();
            for index in 0..64 {
                input
                    .push(index * 2, FRAMES, &[0x90, index as u8, 64])
                    .unwrap();
            }
            midi_in.push(input);
            midi_out.push(MidiBuffer::new(64, 256).unwrap());
        }
        let mut expected = [0.25f32, 0.5];
        for index in 0..case.nodes() {
            let previous = expected;
            for channel in 0..2 {
                let value = if case == Case::Dense && index > 0 {
                    previous[0] + previous[1]
                } else {
                    previous[channel]
                };
                expected[channel] =
                    value * if case == Case::Dense { 0.375 } else { 0.75 } + 0.03125;
            }
        }
        if case == Case::Idle {
            expected = [0.; 2];
        }
        let mut bytes = vec![0; config.page_bytes].into_boxed_slice();
        for sample in bytes.chunks_exact_mut(4) {
            sample.copy_from_slice(&0.25f32.to_le_bytes());
        }
        let mut fixture = Self {
            case,
            config,
            control,
            runtime,
            input: [[0.25; FRAMES], [0.5; FRAMES]],
            output: [[0.; FRAMES]; 2],
            midi_in,
            midi_out,
            recovery: MidiBuffer::new(32, 96).unwrap(),
            expected,
            swapped: false,
            next_plan: None,
            pending: false,
            resource,
            revision: Revision(0),
            capture: None,
            transfer: Transfer::Begin,
            upload: None,
            snapshot: None,
            bytes: Some(bytes),
            metrics: Metrics::default(),
        };
        let plan = fixture.plan(false, false);
        let epoch = fixture.runtime.epoch();
        fixture.apply(Command::commit(epoch, plan), Lane::Normal);
        if case == Case::Graph {
            fixture.next_plan = Some(fixture.plan(true, false));
        }
        if case == Case::Capture {
            let completion = fixture.apply(
                Command::resource(Request::BeginCapture {
                    resource: resource.unwrap(),
                    expected_revision: Revision(0),
                    format: Format::Audio { channels: 2 },
                }),
                Lane::Normal,
            );
            let Reply::Capture(id) = Self::reply(&completion) else {
                panic!("capture")
            };
            fixture.capture = Some(id);
            fixture.revision = Revision(2);
        }
        fixture
    }
    fn render(&mut self) {
        let frame = self.runtime.next_frame();
        let (left, right) = self.output.split_at_mut(1);
        support::assert_no_heap(|| {
            self.runtime
                .process(
                    AudioBlock {
                        frame,
                        frames: FRAMES,
                        inputs: &[&self.input[0], &self.input[1]],
                        outputs: &mut [&mut left[0], &mut right[0]],
                    },
                    MidiBlock {
                        inputs: &self.midi_in,
                        outputs: &mut self.midi_out,
                    },
                )
                .unwrap();
            if !self.midi_out.is_empty() {
                self.recovery.clear();
                self.runtime
                    .drain_midi_recovery(0, &mut self.recovery, FRAMES)
                    .unwrap();
            }
        });
    }
    fn apply(&mut self, command: Command<Dsp>, lane: Lane) -> Returned {
        self.control
            .submit_prepared(lane, command)
            .map_err(|refusal| refusal.error)
            .unwrap();
        for _ in 0..4096 {
            self.render();
            if let Some(completion) = self.control.try_completion() {
                assert_eq!(completion.outcome, Outcome::Applied);
                return completion;
            }
        }
        panic!("finite command did not complete");
    }
    fn reply(completion: &Returned) -> Reply {
        let Command::Resource(command) = &completion.request else {
            panic!("resource")
        };
        command.reply().unwrap()
    }
    fn prepare(&mut self, index: usize) {
        if self.pending {
            return;
        }
        let (command, lane) = match self.case {
            Case::Graph => (
                Command::commit(self.runtime.epoch(), self.next_plan.take().unwrap()),
                Lane::Normal,
            ),
            Case::Capture if index % 32 == 0 => (
                Command::resource(Request::BeginCapture {
                    resource: self.resource.unwrap(),
                    expected_revision: self.revision,
                    format: Format::Audio { channels: 2 },
                }),
                Lane::Normal,
            ),
            Case::Transfer => {
                let request = match self.transfer {
                    Transfer::Begin => Request::BeginUpload {
                        resource: self.resource.unwrap(),
                        expected_revision: self.revision,
                        description: Description {
                            format: Format::Audio { channels: 1 },
                            frames: 16384,
                            bytes: 65536,
                        },
                    },
                    Transfer::Write(upload, offset) => Request::Write {
                        upload,
                        offset,
                        bytes: self.bytes.take().unwrap(),
                    },
                    Transfer::Finish(upload) => Request::Finish { upload },
                    Transfer::Commit(upload) => Request::Commit { upload },
                    Transfer::Snapshot => Request::Snapshot {
                        resource: self.resource.unwrap(),
                    },
                    Transfer::Cursor(snapshot) => Request::Cursor { snapshot },
                    Transfer::Read(cursor) => Request::Read {
                        cursor,
                        bytes: self.bytes.take().unwrap(),
                    },
                    Transfer::Release(snapshot) => Request::ReleaseSnapshot { snapshot },
                };
                let lane = if matches!(self.transfer, Transfer::Release(_)) {
                    Lane::Recovery
                } else {
                    Lane::Normal
                };
                (Command::resource(request), lane)
            }
            _ => return,
        };
        self.control
            .submit_prepared(lane, command)
            .map_err(|refusal| refusal.error)
            .unwrap();
        self.pending = true;
    }
    fn completed(&mut self, completion: Returned) {
        self.pending = false;
        self.metrics.completed += 1;
        if completion.outcome != Outcome::Applied {
            assert!(self.case == Case::Transfer && matches!(self.transfer, Transfer::Begin));
            assert_eq!(completion.outcome, Outcome::Rejected(Error::Capacity));
            self.metrics.refusals += 1;
            return;
        }
        if self.case == Case::Graph {
            self.swapped = !self.swapped;
            self.metrics.activations += 1;
            let Some(Retired::Plan(plan)) = completion.retired else {
                panic!("retired graph")
            };
            self.next_plan = Some(plan);
            return;
        }
        let reply = Self::reply(&completion);
        if self.case == Case::Capture {
            let Reply::Capture(id) = reply else {
                panic!("capture rotation")
            };
            self.capture = Some(id);
            self.revision = Revision(self.revision.0 + 1);
            return;
        }
        let Command::Resource(command) = completion.request else {
            panic!("resource transfer")
        };
        self.transfer = match (self.transfer, reply) {
            (Transfer::Begin, Reply::Upload(id)) => {
                self.upload = Some(id);
                Transfer::Write(id, 0)
            }
            (Transfer::Write(id, offset), Reply::Done) => {
                self.metrics.control_bytes += CHUNK as u64;
                if offset + CHUNK == 65536 {
                    Transfer::Finish(id)
                } else {
                    Transfer::Write(id, offset + CHUNK)
                }
            }
            (Transfer::Finish(id), Reply::Done) => Transfer::Commit(id),
            (Transfer::Commit(_), Reply::Revision(revision)) => {
                self.upload = None;
                self.revision = revision;
                Transfer::Snapshot
            }
            (Transfer::Snapshot, Reply::Snapshot(snapshot)) => {
                assert_eq!(snapshot.description.bytes, 65536);
                self.snapshot = Some(snapshot.id);
                Transfer::Cursor(snapshot.id)
            }
            (Transfer::Cursor(_), Reply::Cursor(cursor)) => Transfer::Read(cursor),
            (Transfer::Read(_), Reply::Read { written, eof }) => {
                let Request::Read { cursor, bytes } = command.request() else {
                    panic!("read")
                };
                assert_eq!(written, CHUNK);
                for sample in bytes[..written].chunks_exact(4) {
                    assert_eq!(sample, 0.25f32.to_le_bytes());
                }
                self.metrics.control_bytes += written as u64;
                if eof {
                    assert_eq!(cursor.position(), 65536);
                    Transfer::Release(self.snapshot.unwrap())
                } else {
                    Transfer::Read(*cursor)
                }
            }
            (Transfer::Release(_), Reply::Done) => {
                self.snapshot = None;
                Transfer::Begin
            }
            _ => panic!("unexpected transfer state"),
        };
        if let Request::Write { bytes, .. } | Request::Read { bytes, .. } = command.into_request() {
            assert!(self.bytes.is_none());
            self.bytes = Some(bytes);
        }
    }
    fn observe(&mut self) {
        if self.case == Case::Capture {
            self.revision = Revision(self.revision.0 + 1);
        }
        self.metrics.owners = self.metrics.owners.max(self.control.outstanding());
        while let Some(completion) = self.control.try_completion() {
            self.completed(completion);
        }
        for channel in 0..2 {
            let expected = self.expected[if self.swapped { 1 - channel } else { channel }];
            assert!(self.output[channel]
                .iter()
                .all(|sample| *sample == expected));
        }
        if self.case == Case::Midi {
            assert_eq!(self.midi_out[0].len(), 64);
            assert!(self.recovery.is_empty());
            for index in 0..64 {
                let (offset, bytes) = self.midi_out[0].get(index).unwrap();
                assert_eq!(offset as usize, index * 2);
                assert_eq!(bytes, &[0x90, index as u8, 64]);
            }
        }
        let d = self.runtime.diagnostics();
        assert_eq!(d.render_error, None);
        assert_eq!(d.midi_overflows, 0);
        assert!(
            d.transfer_bytes <= 2048
                && d.validation_steps <= 128
                && d.nodes_processed <= 64
                && d.connections_processed <= 256
        );
        self.metrics.copy = self.metrics.copy.max(d.transfer_bytes);
        self.metrics.steps = self.metrics.steps.max(d.validation_steps);
        self.metrics.nodes = self.metrics.nodes.max(d.nodes_processed);
        self.metrics.edges = self.metrics.edges.max(d.connections_processed);
        self.metrics.pages = self
            .metrics
            .pages
            .max(self.config.resource_pages - self.runtime.resource_usage().free_pages);
        self.metrics.total_copy += d.transfer_bytes as u64;
    }
    fn verify_capture(&mut self) {
        let resource = self.resource.unwrap();
        let info =
            Self::reply(&self.apply(Command::resource(Request::Info { resource }), Lane::Normal));
        let Reply::Info(info) = info else {
            panic!("capture metadata")
        };
        let capture = info.capture.unwrap();
        assert!(!capture.active && capture.incomplete.is_none());
        assert!(info.description.frames > 0 && info.description.frames <= (FRAMES * 32) as u64);
        assert_eq!(info.description.bytes, info.description.frames as usize * 8);
        let snapshot = Self::reply(&self.apply(
            Command::resource(Request::Snapshot { resource }),
            Lane::Normal,
        ));
        let Reply::Snapshot(snapshot) = snapshot else {
            panic!("capture snapshot")
        };
        let cursor = Self::reply(&self.apply(
            Command::resource(Request::Cursor {
                snapshot: snapshot.id,
            }),
            Lane::Normal,
        ));
        let Reply::Cursor(mut cursor) = cursor else {
            panic!("capture cursor")
        };
        let mut bytes = vec![0; 256].into_boxed_slice();
        let mut read = 0;
        for _ in 0..129 {
            let completion = self.apply(
                Command::resource(Request::Read { cursor, bytes }),
                Lane::Normal,
            );
            let Reply::Read { written, eof } = Self::reply(&completion) else {
                panic!("capture read")
            };
            let Command::Resource(command) = completion.request else {
                panic!("capture request")
            };
            let Request::Read {
                cursor: next,
                bytes: returned,
            } = command.into_request()
            else {
                panic!("capture buffer")
            };
            for frame in returned[..written].chunks_exact(8) {
                for (channel, word) in frame.chunks_exact(4).enumerate() {
                    assert_eq!(word, self.expected[channel].to_le_bytes());
                }
            }
            read += written;
            cursor = next;
            bytes = returned;
            if eof {
                break;
            }
        }
        assert_eq!(read, info.description.bytes);
        self.apply(
            Command::resource(Request::ReleaseSnapshot {
                snapshot: snapshot.id,
            }),
            Lane::Recovery,
        );
    }
    fn shutdown(&mut self) {
        for _ in 0..4096 {
            if !self.pending {
                break;
            }
            self.render();
            self.observe();
        }
        assert!(!self.pending);
        if let Some(capture) = self.capture.take() {
            self.apply(
                Command::resource(Request::StopCapture { capture }),
                Lane::Recovery,
            );
        }
        if self.case == Case::Capture {
            self.verify_capture();
        }
        if let Some(upload) = self.upload.take() {
            self.apply(
                Command::resource(Request::CancelUpload { upload }),
                Lane::Recovery,
            );
        }
        if let Some(snapshot) = self.snapshot.take() {
            self.apply(
                Command::resource(Request::ReleaseSnapshot { snapshot }),
                Lane::Recovery,
            );
        }
        let plan = self.plan(false, true);
        self.apply(Command::commit(self.runtime.epoch(), plan), Lane::Normal);
        for index in 0..self.case.nodes() {
            self.apply(Command::Remove { node: node(index) }, Lane::Normal);
        }
        if let Some(resource) = self.resource {
            self.apply(
                Command::resource(Request::Remove { resource }),
                Lane::Normal,
            );
        }
        for _ in 0..16 {
            self.render();
        }
        let usage = self.runtime.resource_usage();
        assert_eq!(usage.free_pages, self.config.resource_pages);
        assert_eq!(usage.free_versions, 16);
        assert_eq!(usage.free_snapshots, 16);
        assert_eq!(usage.reserved_pages, 0);
        assert_eq!(usage.retired_pages, 0);
        assert_eq!(usage.retired_versions, 0);
        assert_eq!(self.control.outstanding(), 0);
    }
}
#[derive(Clone, Copy)]
struct SystemSample {
    voluntary: u64,
    involuntary: u64,
    minor_faults: u64,
    major_faults: u64,
}
fn system_sample() -> Option<SystemSample> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let field = |name: &str| {
        status
            .lines()
            .find(|line| line.starts_with(name))?
            .split_whitespace()
            .nth(1)?
            .parse::<u64>()
            .ok()
    };
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<_> = stat.rsplit_once(") ")?.1.split_whitespace().collect();
    Some(SystemSample {
        voluntary: field("voluntary_ctxt_switches:")?,
        involuntary: field("nonvoluntary_ctxt_switches:")?,
        minor_faults: fields.get(7)?.parse().ok()?,
        major_faults: fields.get(9)?.parse().ok()?,
    })
}
fn system_delta(before: Option<SystemSample>, after: Option<SystemSample>) -> String {
    match (before, after) {
        (Some(a), Some(b)) => format!("{{\"voluntary_switches\":{},\"involuntary_switches\":{},\"minor_faults\":{},\"major_faults\":{}}}",
            b.voluntary.saturating_sub(a.voluntary), b.involuntary.saturating_sub(a.involuntary), b.minor_faults.saturating_sub(a.minor_faults), b.major_faults.saturating_sub(a.major_faults)),
        _ => "null".into(),
    }
}
fn peak_rss() -> String {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "null".into())
}
fn canary(kind: &str) {
    match kind {
        "allocation" => {
            let _value = support::assert_no_heap(|| {
                std::hint::black_box(Box::new(std::hint::black_box(7u64)))
            });
        }
        "reallocation" => {
            let mut bytes = Vec::with_capacity(1);
            bytes.push(1u8);
            support::assert_no_heap(|| {
                for _ in 0..256 {
                    bytes.push(std::hint::black_box(2));
                }
            });
        }
        "deallocation" => {
            let value = std::hint::black_box(Box::new(7u64));
            support::assert_no_heap(|| drop(value));
        }
        _ => panic!("unknown canary"),
    }
    panic!("heap guard failed to detect deliberate violation");
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if let Some(index) = args.iter().position(|arg| arg == "--fail-canary") {
        canary(&args[index + 1]);
    }
    let iterations: usize = args
        .iter()
        .position(|arg| arg == "--iterations")
        .map_or(20000, |index| args[index + 1].parse().unwrap());
    assert!((1024..=1_000_000).contains(&iterations));
    let raw_directory = args
        .iter()
        .position(|arg| arg == "--raw-dir")
        .map(|index| std::path::PathBuf::from(&args[index + 1]));
    if let Some(directory) = &raw_directory {
        std::fs::create_dir_all(directory).unwrap();
    }
    let (_, counts) = support::measure(|| {
        let mut bytes = Vec::with_capacity(1);
        for index in 0..256 {
            bytes.push(std::hint::black_box(index));
        }
        std::hint::black_box(&bytes);
        drop(bytes);
    });
    assert!(counts.allocations > 0 && counts.reallocations > 0 && counts.deallocations > 0);
    for case in [
        Case::Idle,
        Case::Dag,
        Case::Dense,
        Case::Midi,
        Case::Capture,
        Case::Graph,
        Case::Transfer,
    ] {
        let mut fixture = Fixture::new(case);
        let mut times = vec![0u64; iterations];
        let mut work = vec![(0usize, 0usize, 0usize, 0usize, 0u64); iterations];
        for value in &mut times {
            *std::hint::black_box(value) = 0;
        }
        for value in &mut work {
            *std::hint::black_box(value) = (0, 0, 0, 0, 0);
        }
        for index in 0..512 {
            fixture.prepare(index);
            fixture.render();
            fixture.observe();
        }
        fixture.metrics = Metrics::default();
        let system_before = system_sample();
        for (index, time) in times.iter_mut().enumerate() {
            fixture.prepare(index);
            let start = Instant::now();
            fixture.render();
            *time = start.elapsed().as_nanos().try_into().unwrap();
            let diagnostics = fixture.runtime.diagnostics();
            work[index] = (
                diagnostics.transfer_bytes,
                diagnostics.validation_steps,
                diagnostics.nodes_processed,
                diagnostics.connections_processed,
                fixture.runtime.epoch().0,
            );
            fixture.observe();
        }
        let system_after = system_sample();
        if let Some(directory) = &raw_directory {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(directory.join(format!("{}.csv", case.name()))).unwrap(),
            );
            writeln!(
                file,
                "callback,nanoseconds,copy_bytes,validation_steps,nodes,edges,epoch"
            )
            .unwrap();
            for (index, (time, (bytes, steps, nodes, edges, epoch))) in
                times.iter().zip(&work).enumerate()
            {
                writeln!(
                    file,
                    "{index},{time},{bytes},{steps},{nodes},{edges},{epoch}"
                )
                .unwrap();
            }
            file.flush().unwrap();
        }
        let total: u128 = times.iter().map(|value| u128::from(*value)).sum();
        let misses = times.iter().filter(|value| **value > DEADLINE_NS).count();
        times.sort_unstable();
        let percentile = |n: usize| times[(iterations - 1) * n / 1000];
        let m = &fixture.metrics;
        println!("{{\"case\":\"{}\",\"callbacks\":{},\"sample_rate\":{},\"frames_per_callback\":{},\"nodes\":{},\"edges_max\":{},\"mean_ns\":{},\"p50_ns\":{},\"p99_ns\":{},\"p999_ns\":{},\"max_ns\":{},\"deadline_ns\":{},\"deadline_misses\":{},\"copy_bytes_max\":{},\"validation_steps_max\":{},\"core_copy_bytes\":{},\"control_copy_bytes\":{},\"copy_mib_per_measured_callback_second\":{},\"copy_mib_per_simulated_second\":{},\"commands_completed\":{},\"refusals\":{},\"graph_activations\":{},\"outstanding_owners_max\":{},\"prepared_resource_bytes\":{},\"resource_used_pages_at_callback_boundary_max\":{},\"process_peak_rss_kib\":{},\"scheduler_counters\":{},\"rust_heap_operations\":0}}",
            case.name(), iterations, RATE, FRAMES, m.nodes, m.edges, total as f64 / iterations as f64, percentile(500), percentile(990), percentile(999), times[iterations - 1], DEADLINE_NS, misses,
            m.copy, m.steps, m.total_copy, m.control_bytes, m.control_bytes as f64 * 1e9 / total as f64 / 1048576., m.control_bytes as f64 * RATE as f64 / (iterations * FRAMES) as f64 / 1048576.,
            m.completed, m.refusals, m.activations, m.owners, fixture.config.resource_pages * fixture.config.page_bytes, m.pages, peak_rss(), system_delta(system_before, system_after));
        fixture.shutdown();
    }
}
