//! Guarded native loop measurements with scalar sample/packet checks outside timing.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, Control, Diagnostics, GraphRuntime},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    planner::{NoPlanner, Planner},
    processor::{ProcessBlock, Processor},
    resource::{self, CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, PlannerId, ResourceId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    compile, media,
    runtime::{Binding, BoundaryObserver, Controller, Engine, Operation, Reply, Settings},
    Decision, Entry, Event, EventKind, Limits, LoopId, LoopKind, LoopSpec, Mode,
    ProgramDescription, Section, Timing, TopologyDescription, Transition, Trigger,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
use std::{
    io::Write,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
const FRAMES: usize = 128;
const PAGES: usize = 4096;
const PAGE: usize = 256;
const DEADLINE: u64 = FRAMES as u64 * 1_000_000_000 / 48_000;

#[derive(Clone, Copy)]
struct Case {
    name: &'static str,
    loops: usize,
    length: u64,
    midi: bool,
    kind: u8,
}
const CASES: &[Case] = &[
    Case {
        name: "cascade-bursts",
        loops: 4,
        length: 1,
        midi: true,
        kind: 7,
    },
    Case {
        name: "no-planner-idle",
        loops: 0,
        length: 257,
        midi: false,
        kind: 0,
    },
    Case {
        name: "loop-idle",
        loops: 0,
        length: 257,
        midi: false,
        kind: 1,
    },
    Case {
        name: "audio-1-257",
        loops: 1,
        length: 257,
        midi: false,
        kind: 1,
    },
    Case {
        name: "audio-4-257",
        loops: 4,
        length: 257,
        midi: false,
        kind: 1,
    },
    Case {
        name: "audio-16-257",
        loops: 16,
        length: 257,
        midi: false,
        kind: 1,
    },
    Case {
        name: "audio-4-4",
        loops: 4,
        length: 4,
        midi: false,
        kind: 1,
    },
    Case {
        name: "midi-1-4",
        loops: 1,
        length: 4,
        midi: true,
        kind: 1,
    },
    Case {
        name: "midi-4-4",
        loops: 4,
        length: 4,
        midi: true,
        kind: 1,
    },
    Case {
        name: "cascade-4-1",
        loops: 4,
        length: 1,
        midi: true,
        kind: 2,
    },
    Case {
        name: "nested-regular",
        loops: 1,
        length: 4,
        midi: false,
        kind: 3,
    },
    Case {
        name: "seek-replace",
        loops: 1,
        length: 257,
        midi: false,
        kind: 4,
    },
    Case {
        name: "fx-capture",
        loops: 1,
        length: 128,
        midi: false,
        kind: 5,
    },
    Case {
        name: "upload-export",
        loops: 1,
        length: 257,
        midi: false,
        kind: 6,
    },
];
fn id(slot: usize) -> LoopId {
    LoopId::from_parts(slot as u32, 1).unwrap()
}
fn node(slot: usize) -> NodeId {
    NodeId::from_parts(slot as u32, 1).unwrap()
}
struct Fx {
    midi: bool,
}
impl<L: Planner> Processor<L> for Fx {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            midi_inputs: usize::from(self.midi),
            midi_outputs: usize::from(self.midi),
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, L>) -> Result<(), Error> {
        for ch in 0..2 {
            for f in 0..b.frames {
                let value = (b.audio.input(ch).ok_or(Error::InvalidShape)?[f]
                    + b.audio.input(ch + 2).ok_or(Error::InvalidShape)?[f])
                    * 0.5;
                b.audio.output(ch).ok_or(Error::InvalidShape)?[f] = value;
            }
        }
        if self.midi {
            for i in 0..b.midi_inputs[0].len() {
                let (at, packet) = b.midi_inputs[0].get(i).ok_or(Error::InvalidEvent)?;
                b.midi_outputs[0].push(at, b.frames, packet)?;
            }
        }
        Ok(())
    }
}
#[derive(Default)]
struct Counts {
    boundaries: AtomicU64,
    transitions: AtomicU64,
    decisions: AtomicU64,
    starts: AtomicU64,
}
struct Observer(Arc<Counts>, bool);
impl BoundaryObserver for Observer {
    fn boundary(&mut self, _: Frame, d: &[Decision], e: &[Event], lost: u64) {
        assert_eq!(lost, 0);
        if self.1 {
            let starts = e
                .iter()
                .filter(|event| {
                    matches!(event.kind, EventKind::Transition(Transition::Play { .. }))
                })
                .count();
            self.0.starts.fetch_add(starts as u64, Ordering::Relaxed);
        }
        self.0.boundaries.fetch_add(1, Ordering::Relaxed);
        self.0
            .transitions
            .fetch_add(e.len() as u64, Ordering::Relaxed);
        self.0
            .decisions
            .fetch_add(d.len() as u64, Ordering::Relaxed);
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
fn source(
    store: &mut ResourceStore,
    format: resource::Format,
    length: u64,
    bytes: &[u8],
    initial: Option<&MidiState>,
    ids: &mut Vec<ResourceId>,
) -> media::Source {
    let resource = store.create().unwrap();
    ids.push(resource);
    let description = resource::Description {
        format,
        frames: length,
        bytes: bytes.len(),
    };
    let upload = if let Some(initial) = initial {
        store.begin_indexed_midi_upload(resource, Revision(0), description, initial)
    } else {
        store.begin_upload(resource, Revision(0), description)
    }
    .unwrap();
    for (index, chunk) in bytes.chunks(PAGE).enumerate() {
        store.begin_callback();
        store.write_upload(upload, index * PAGE, chunk).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let index = store.read_index(snapshot.id).unwrap();
    media::Source { snapshot, index }
}
enum Driver {
    Bare(GraphRuntime<Fx>),
    Loops(Engine<Fx>),
}
impl Driver {
    fn frame(&self) -> Frame {
        match self {
            Self::Bare(r) => r.next_frame(),
            Self::Loops(r) => r.next_frame(),
        }
    }
    fn diagnostics(&self) -> Diagnostics {
        match self {
            Self::Bare(r) => r.diagnostics(),
            Self::Loops(r) => r.diagnostics(),
        }
    }
    fn usage(&self) -> resource::PoolUsage {
        match self {
            Self::Bare(r) => r.resource_usage(),
            Self::Loops(r) => r.resource_usage(),
        }
    }
    fn render(&mut self, a: AudioBlock<'_, '_>, m: MidiBlock<'_>) -> Result<(), Error> {
        match self {
            Self::Bare(r) => r.process(a, m),
            Self::Loops(r) => r.process(a, m),
        }
    }
}
struct Bench {
    case: Case,
    driver: Driver,
    control: Option<Controller<Fx>>,
    bare_control: Option<Control<Fx>>,
    input: [[f32; FRAMES]; 2],
    output: [[f32; FRAMES]; 2],
    midi_in: Vec<MidiBuffer>,
    midi_out: Vec<MidiBuffer>,
    counts: Arc<Counts>,
    imported: Vec<ResourceId>,
    resource_slots: usize,
    prepared: Config,
    pending_domain: usize,
    pending_resource: usize,
    expected_position: u64,
    catalog: Vec<media::MediaId>,
    recording: bool,
    value: f32,
    commands: u64,
    resource_commands: u64,
    upload_resource: Option<ResourceId>,
    upload_revision: Revision,
    upload_phase: usize,
    upload: Option<resource::UploadId>,
    write_buffer: Option<Box<[u8]>>,
    export: Option<media::ExportId>,
    cursor: Option<resource::Cursor>,
    read_buffer: Option<Box<[u8]>>,
    exported_bytes: u64,
    uploaded_bytes: u64,
}
impl Bench {
    fn new(case: Case) -> Self {
        let extra = usize::from(case.kind == 3);
        let primitives = case.loops + extra;
        let slots = primitives + if extra > 0 { 2 } else { 0 };
        let event_capacity = if case.midi {
            case.loops * (FRAMES + 1) * 70
        } else {
            64
        };
        let cfg = Config {
            max_frames: FRAMES,
            max_nodes: (primitives * 3).max(1),
            max_ports: (primitives * 14 + 6).max(8),
            max_edges: (primitives * 12).max(1),
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: usize::from(case.midi),
            midi_events_per_port: event_capacity,
            midi_bytes_per_port: event_capacity * 3,
            resource_slots: (primitives * 8 + 8).max(16),
            resource_pages: PAGES,
            page_bytes: PAGE,
            command_slots: 64,
            scheduled_events: 64,
            budgets: Budgets {
                commands: 64,
                events: 64,
                subranges: 0,
                transfer_bytes: 4096,
                validation_steps: 256,
                publications: 64,
            },
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let counts = Arc::new(Counts::default());
        let mut imported = Vec::new();
        let (driver, control, bare_control, catalog, upload_resource) = if case.kind == 0 {
            let (c, r) = GraphRuntime::prepare_with_initial_graph(
                cfg,
                vec![],
                NoPlanner,
                ResourceStore::new(cfg).unwrap(),
                &GraphDescription {
                    nodes: vec![],
                    connections: vec![],
                },
                &[],
            )
            .unwrap();
            (Driver::Bare(r), None, Some(c), vec![], None)
        } else {
            let mut resources = ResourceStore::with_capture_groups(
                cfg,
                Some(MidiIndexConfig {
                    versions: primitives * 4 + 4,
                    checkpoints_per_version: 4,
                    replay_events: 64,
                }),
                CaptureGroupConfig {
                    groups: primitives * 2 + 1,
                    members_per_group: 2,
                    staging_bytes_per_group: 4096,
                },
            )
            .unwrap();
            let mut imports = Vec::new();
            for slot in 0..case.loops {
                for revision in [0, 7] {
                    let value = if revision == 0 { 1f32 } else { 2f32 };
                    let bytes: Vec<u8> = (0..case.length)
                        .flat_map(|_| [value, value * 10.].into_iter().flat_map(f32::to_le_bytes))
                        .collect();
                    let audio = source(
                        &mut resources,
                        resource::Format::Audio { channels: 2 },
                        case.length,
                        &bytes,
                        None,
                        &mut imported,
                    );
                    let midi = if case.midi {
                        let mut initial = MidiState::default();
                        initial
                            .apply_packet(&[0x90, 60 + slot as u8, 100], 3)
                            .unwrap();
                        Some(source(
                            &mut resources,
                            resource::Format::Midi,
                            case.length,
                            &[],
                            Some(&initial),
                            &mut imported,
                        ))
                    } else {
                        None
                    };
                    imports.push(media::Input {
                        description: media::Description {
                            owner: id(slot),
                            revision: Revision(revision),
                            frames: case.length,
                            channels: 2,
                            midi: case.midi,
                        },
                        audio: Some(audio),
                        midi,
                    });
                }
            }
            let upload_resource = if case.kind == 6 {
                Some(resources.create().unwrap())
            } else {
                None
            };
            let mut description = TopologyDescription {
                loops: (0..slots)
                    .map(|slot| LoopSpec {
                        id: id(slot),
                        kind: if slot < primitives {
                            LoopKind::Primitive
                        } else {
                            LoopKind::Composite
                        },
                        length: if slot < case.loops {
                            case.length
                        } else if slot < primitives {
                            1
                        } else {
                            0
                        },
                        sync_source: (slot >= primitives).then(|| id(case.loops)),
                    })
                    .collect(),
                ..Default::default()
            };
            if matches!(case.kind, 2 | 7) {
                description.triggers = (0..case.loops - 1)
                    .map(|slot| Trigger {
                        source: id(slot),
                        target: id(slot + 1),
                        mode: Mode::Playing,
                    })
                    .collect();
            }
            if case.kind == 3 {
                for (parent, child) in [(primitives, primitives + 1), (primitives + 1, 0)] {
                    description.programs.push(ProgramDescription {
                        source: id(parent),
                        nominal_unit_frames: 1,
                        sections: vec![Section {
                            entries: vec![Entry {
                                target: id(child),
                                delay: 0,
                                duration: Some(8),
                                repetitions: 1,
                                mode: None,
                            }],
                        }],
                    });
                }
            }
            let topology = compile(
                Limits {
                    loop_slots: slots.max(1),
                    pending_requests: 64,
                    journal_events: (slots * FRAMES * 8 + 64).max(1024),
                    ..Default::default()
                },
                &description,
            )
            .unwrap();
            let mut graph = GraphDescription {
                nodes: vec![],
                connections: vec![],
            };
            let mut bindings = Vec::new();
            let mut effects = Vec::new();
            for slot in 0..primitives {
                let p = node(slot * 3);
                let c = node(slot * 3 + 1);
                let fx = node(slot * 3 + 2);
                let audible = slot < case.loops;
                graph.nodes.extend([p, c]);
                bindings.push(Binding {
                    target: id(slot),
                    playback: p,
                    capture: c,
                    channels: if audible { 2 } else { 0 },
                    midi: audible && case.midi,
                    gain: 1.,
                    smoothing: Smoothing::None,
                    midi_events: if case.midi { 128 } else { 0 },
                    midi_bytes: if case.midi { 512 } else { 0 },
                    packet_bytes: if case.midi { 512 } else { 0 },
                });
                if !audible {
                    continue;
                }
                graph.nodes.push(fx);
                effects.push((fx, Fx { midi: case.midi }));
                for ch in 0..2 {
                    for (src, dst) in [
                        (
                            Endpoint::host(PortType::Audio, ch),
                            Endpoint::node(fx, PortType::Audio, ch),
                        ),
                        (
                            Endpoint::node(p, PortType::Audio, ch),
                            Endpoint::node(fx, PortType::Audio, ch + 2),
                        ),
                        (
                            Endpoint::node(fx, PortType::Audio, ch),
                            Endpoint::node(c, PortType::Audio, ch),
                        ),
                        (
                            Endpoint::node(fx, PortType::Audio, ch),
                            Endpoint::host(PortType::Audio, ch),
                        ),
                    ] {
                        graph.connections.push(Connection {
                            source: src,
                            destination: dst,
                        });
                    }
                }
                if case.midi {
                    for (src, dst) in [
                        (
                            Endpoint::node(p, PortType::Midi, 0),
                            Endpoint::node(fx, PortType::Midi, 0),
                        ),
                        (
                            Endpoint::node(fx, PortType::Midi, 0),
                            Endpoint::node(c, PortType::Midi, 0),
                        ),
                        (
                            Endpoint::node(fx, PortType::Midi, 0),
                            Endpoint::host(PortType::Midi, 0),
                        ),
                    ] {
                        graph.connections.push(Connection {
                            source: src,
                            destination: dst,
                        });
                    }
                }
            }
            let (c, r) = Engine::prepare_with_observer(
                cfg,
                topology,
                resources,
                Settings {
                    planner: PlannerId::from_parts(0, 1).unwrap(),
                    normal_requests: 63,
                    media_slots: primitives * 4 + 16,
                    export_slots: 4,
                },
                bindings,
                imports,
                effects,
                graph,
                Box::new(Observer(counts.clone(), case.kind == 7)),
            )
            .unwrap();
            let catalog = c.media().iter().map(|m| m.media).collect();
            (Driver::Loops(r), Some(c), None, catalog, upload_resource)
        };
        let mut s = Self {
            case,
            driver,
            control,
            bare_control,
            input: [[0.; FRAMES]; 2],
            output: [[0.; FRAMES]; 2],
            midi_in: vec![],
            midi_out: if case.midi {
                vec![MidiBuffer::new(event_capacity, event_capacity * 3).unwrap()]
            } else {
                vec![]
            },
            counts,
            imported,
            resource_slots: cfg.limits().resource_slots,
            prepared: cfg.limits(),
            pending_domain: 0,
            pending_resource: 0,
            expected_position: 0,
            catalog,
            recording: false,
            value: 1.,
            commands: 0,
            resource_commands: 0,
            upload_resource,
            upload_revision: Revision(0),
            upload_phase: 0,
            upload: None,
            write_buffer: Some(vec![0x35; 64].into_boxed_slice()),
            export: None,
            cursor: None,
            read_buffer: Some(vec![0; 64].into_boxed_slice()),
            exported_bytes: 0,
            uploaded_bytes: 0,
        };
        if s.case.kind == 3 {
            s.transition(
                s.case.loops,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            );
            s.transition(
                s.case.loops + 1,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            );
        } else if s.case.kind != 5 {
            for slot in 0..if matches!(s.case.kind, 2 | 7) {
                1
            } else {
                s.case.loops
            } {
                s.transition(
                    slot,
                    Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                );
            }
        }
        s
    }
    fn send(&mut self, op: Operation) {
        self.send_at(self.driver.frame(), op);
    }
    fn send_at(&mut self, at: Frame, op: Operation) {
        self.control
            .as_mut()
            .unwrap()
            .submit(Lane::Normal, at, op)
            .unwrap_or_else(|e| panic!("submission {:?}", e.error));
        self.pending_domain += 1;
    }
    fn transition(&mut self, slot: usize, transition: Transition) {
        self.send(Operation::Transition {
            target: id(slot),
            timing: Timing::At,
            transition,
        });
    }
    fn prepare(&mut self, index: usize) {
        if self.case.kind == 7 {
            if self.driver.frame().0 != 0 {
                self.transition(
                    0,
                    Transition::Play {
                        position: 0,
                        retrigger: false,
                    },
                );
            }
            for slot in 0..self.case.loops {
                self.send_at(
                    Frame(self.driver.frame().0 + FRAMES as u64 - 1),
                    Operation::Transition {
                        target: id(slot),
                        timing: Timing::At,
                        transition: Transition::Stop,
                    },
                );
            }
        }
        if self.case.kind == 4 && self.driver.frame().0 != 0 {
            if index % 2 == 0 {
                let selected = (index / 2) % 2;
                self.value = if selected == 0 { 1. } else { 2. };
                self.expected_position = 0;
                self.send(Operation::Select {
                    target: id(0),
                    timing: Timing::At,
                    media: self.catalog[selected],
                    preserve_position: false,
                });
            } else {
                self.expected_position = (index as u64 * 17) % self.case.length;
                self.transition(
                    0,
                    Transition::Seek {
                        position: self.expected_position,
                        retrigger: false,
                    },
                );
            }
            self.send(Operation::State { target: id(0) });
        }
        if self.case.kind == 5 {
            self.recording = index % 2 == 0;
            self.transition(
                0,
                if self.recording {
                    Transition::Record {
                        monitor_previous: false,
                    }
                } else {
                    Transition::Play {
                        position: 0,
                        retrigger: false,
                    }
                },
            );
            if !self.recording {
                self.send(Operation::State { target: id(0) });
            }
            self.input[0].fill(if self.recording { 2. } else { 0. });
            self.input[1].fill(if self.recording { 20. } else { 0. });
        }
        if self.case.kind == 6 {
            let req = match self.upload_phase {
                0 => resource::Request::BeginUpload {
                    resource: self.upload_resource.unwrap(),
                    expected_revision: self.upload_revision,
                    description: resource::Description {
                        format: resource::Format::Audio { channels: 2 },
                        frames: 8,
                        bytes: 64,
                    },
                },
                1 => resource::Request::Write {
                    upload: self.upload.unwrap(),
                    offset: 0,
                    bytes: self.write_buffer.take().unwrap(),
                },
                2 => resource::Request::Finish {
                    upload: self.upload.unwrap(),
                },
                _ => resource::Request::Commit {
                    upload: self.upload.unwrap(),
                },
            };
            self.control
                .as_mut()
                .unwrap()
                .resource(Lane::Normal, req)
                .unwrap_or_else(|e| panic!("resource {:?}", e.error));
            self.pending_resource += 1;
            if let Some(cursor) = self.cursor.take() {
                self.control
                    .as_mut()
                    .unwrap()
                    .read(cursor, self.read_buffer.take().unwrap())
                    .unwrap_or_else(|e| panic!("read {:?}", e.error));
                self.pending_resource += 1;
            } else if let Some(export) = self.export {
                self.send(Operation::Cursor {
                    export,
                    midi: false,
                });
            } else {
                self.send(Operation::Export {
                    target: id(0),
                    selection: shoop_loops::runtime::Selection::Playback,
                });
            }
        }
    }
    fn render(&mut self) {
        assert_no_heap(|| {
            for m in &mut self.midi_out {
                m.clear();
            }
            let [left, right] = &mut self.output;
            self.driver
                .render(
                    AudioBlock {
                        frame: self.driver.frame(),
                        frames: FRAMES,
                        inputs: &[&self.input[0], &self.input[1]],
                        outputs: &mut [&mut left[..], &mut right[..]],
                    },
                    MidiBlock {
                        inputs: &self.midi_in,
                        outputs: &mut self.midi_out,
                    },
                )
                .unwrap()
        });
    }
    fn observe(&mut self, start: Frame) {
        let expected = if self.case.kind == 5 {
            if self.recording {
                1.
            } else {
                0.5
            }
        } else {
            self.case.loops as f32 * self.value * 0.5
        };
        for ch in 0..2 {
            for (offset, sample) in self.output[ch].iter().copied().enumerate() {
                let value = if self.case.kind == 7 && offset == FRAMES - 1 {
                    0.
                } else {
                    expected
                };
                assert_eq!(
                    sample,
                    value * if ch == 0 { 1. } else { 10. },
                    "{} frame {}",
                    self.case.name,
                    start.0
                );
            }
        }
        if self.case.midi {
            let mut expected = Vec::new();
            for slot in 0..self.case.loops {
                for offset in 0..FRAMES {
                    let at = start.0 + offset as u64;
                    if at % self.case.length != 0 {
                        continue;
                    }
                    if self.case.kind == 7 && offset == FRAMES - 1 {
                        expected.push((offset as u32, [0x80, 60 + slot as u8, 0]));
                        continue;
                    }
                    if at != 0 && !(self.case.kind == 7 && offset == 0) {
                        expected.push((offset as u32, [0x80, 60 + slot as u8, 0]));
                    }
                    for channel in 0..16 {
                        for packet in [
                            [0xe0 | channel, 0, 64],
                            [0xb0 | channel, 64, 0],
                            [0xb0 | channel, 69, 0],
                            [0xb0 | channel, 66, 0],
                        ] {
                            expected.push((offset as u32, packet));
                        }
                        if channel == 0 {
                            expected.push((offset as u32, [0x90, 60 + slot as u8, 100]));
                        }
                    }
                }
            }
            expected.sort_by_key(|e| e.0);
            assert_eq!(
                self.midi_out[0].len(),
                expected.len(),
                "{} MIDI count at {}",
                self.case.name,
                start.0
            );
            for (i, (at, packet)) in expected.iter().enumerate() {
                assert_eq!(
                    self.midi_out[0].get(i),
                    Some((*at, packet.as_slice())),
                    "{} MIDI packet {i}",
                    self.case.name
                );
            }
        }
        let d = self.driver.diagnostics();
        assert_eq!(d.render_error, None);
        assert_eq!(d.midi_overflows, 0);
        assert!(!d.counter_overflow);
        if let Some(c) = &mut self.control {
            while let Some(execution) = c.try_execution() {
                self.pending_domain = self.pending_domain.checked_sub(1).unwrap();
                self.commands += 1;
                assert_eq!(execution.result, Ok(()));
                match execution.operation_result.unwrap() {
                    Reply::Transition(t) => {
                        assert!(matches!(t.outcome, shoop_loops::Outcome::Applied { .. }))
                    }
                    Reply::State(cut) => {
                        assert_eq!(cut.frame, start);
                        assert_eq!(cut.capture_failure, None);
                        if self.case.kind == 4 {
                            assert_eq!(cut.state.position, self.expected_position);
                            assert_eq!(
                                cut.playback,
                                Some(self.catalog[usize::from(self.value == 2.)])
                            );
                        } else {
                            assert!(cut.latest.is_some());
                            assert_eq!(cut.state.content_length, FRAMES as u64);
                        }
                    }
                    Reply::Export { id, .. } => self.export = Some(id),
                    Reply::Cursor(cursor) => self.cursor = Some(cursor),
                    Reply::Done => {}
                    other => panic!("unexpected result {other:?}"),
                }
            }
            while let Some(completion) = c.try_completion() {
                if let Command::Resource(command) = completion.request {
                    self.pending_resource = self.pending_resource.checked_sub(1).unwrap();
                    self.resource_commands += 1;
                    let reply = command.reply().expect("resource success");
                    match command.into_request() {
                        resource::Request::BeginUpload { .. } => {
                            let resource::Reply::Upload(id) = reply else {
                                panic!("upload")
                            };
                            self.upload = Some(id);
                            self.upload_phase = 1;
                        }
                        resource::Request::Write { bytes, .. } => {
                            self.write_buffer = Some(bytes);
                            self.uploaded_bytes += 64;
                            self.upload_phase = 2;
                        }
                        resource::Request::Finish { .. } => self.upload_phase = 3,
                        resource::Request::Commit { .. } => {
                            let resource::Reply::Revision(rev) = reply else {
                                panic!("revision")
                            };
                            assert_eq!(rev.0, self.upload_revision.0 + 1);
                            self.upload_revision = rev;
                            self.upload_phase = 0;
                        }
                        resource::Request::Read { cursor, bytes } => {
                            let resource::Reply::Read { written, eof } = reply else {
                                panic!("read")
                            };
                            let expected = [1f32.to_le_bytes(), 10f32.to_le_bytes()].concat();
                            for (i, b) in bytes[..written].iter().enumerate() {
                                assert_eq!(*b, expected[(cursor.position() - written + i) % 8]);
                            }
                            self.exported_bytes += written as u64;
                            self.read_buffer = Some(bytes);
                            if !eof {
                                self.cursor = Some(cursor);
                            }
                        }
                        resource::Request::CancelUpload { .. } => {}
                        _ => panic!("unexpected resource completion"),
                    }
                }
            }
        }
        assert_eq!(self.pending_domain, 0);
        assert_eq!(self.pending_resource, 0);
    }
    fn shutdown(mut self) {
        if let Driver::Loops(ref mut engine) = self.driver {
            if let Some(export) = self.export.take() {
                self.control
                    .as_mut()
                    .unwrap()
                    .submit(
                        Lane::Normal,
                        engine.next_frame(),
                        Operation::Release { export },
                    )
                    .unwrap_or_else(|e| panic!("release {:?}", e.error));
                self.pending_domain += 1;
            }
            if let Some(upload) = self.upload.take().filter(|_| self.upload_phase != 0) {
                self.control
                    .as_mut()
                    .unwrap()
                    .resource(Lane::Recovery, resource::Request::CancelUpload { upload })
                    .unwrap_or_else(|e| panic!("cancel {:?}", e.error));
                self.pending_resource += 1;
            }
        }
        if self.case.kind != 7 {
            let start = self.driver.frame();
            self.render();
            self.observe(start);
        }
        drop(self.control.take());
        drop(self.bare_control.take());
        match self.driver {
            Driver::Loops(e) => {
                let mut s = e.into_stopped();
                s.cleanup().unwrap();
                for r in self.imported.into_iter().chain(self.upload_resource) {
                    s.resources().remove(r).unwrap();
                }
                for _ in 0..PAGES {
                    s.resources().begin_callback();
                    s.resources().reclaim();
                }
                let u = s.resource_usage();
                assert_eq!(u.free_pages, PAGES);
                assert_eq!(u.free_versions, self.resource_slots);
                assert_eq!(u.free_snapshots, self.resource_slots);
                assert_eq!(u.reserved_pages, 0);
                assert_eq!(u.retired_pages, 0);
                assert_eq!(u.retired_versions, 0);
                assert_eq!(s.media_usage().references, 0);
            }
            Driver::Bare(r) => {
                let usage = r.into_resources().usage();
                assert_eq!(usage.free_pages, PAGES);
                assert_eq!(usage.free_versions, self.resource_slots);
                assert_eq!(usage.free_snapshots, self.resource_slots);
            }
        }
        assert!(self.write_buffer.is_some());
        assert!(self.read_buffer.is_some());
    }
}
#[derive(Clone, Copy, Default)]
struct Sample {
    ns: u64,
    intervals: usize,
    nodes: usize,
    edges: usize,
    transitions: u64,
    render_bytes: usize,
    maintenance_bytes: usize,
    render_steps: usize,
    maintenance_steps: usize,
    pages: usize,
    reserved: usize,
    versions: usize,
    snapshots: usize,
    domain_pending: usize,
    resource_pending: usize,
    cascade_starts: u64,
}
fn configuration(c: Config, domain_credits: usize) -> String {
    format!("{{\"max_frames\":{},\"max_nodes\":{},\"max_ports\":{},\"max_edges\":{},\"midi_events_per_port\":{},\"midi_bytes_per_port\":{},\"resource_slots\":{},\"resource_pages\":{},\"page_bytes\":{},\"command_slots\":{},\"scheduled_events\":{},\"domain_credits\":{},\"transfer_bytes\":{},\"validation_steps\":{},\"publications\":{}}}", c.max_frames,c.max_nodes,c.max_ports,c.max_edges,c.midi_events_per_port,c.midi_bytes_per_port,c.resource_slots,c.resource_pages,c.page_bytes,c.command_slots,c.scheduled_events,domain_credits,c.budgets.transfer_bytes,c.budgets.validation_steps,c.budgets.publications)
}
fn process_peak_rss_kib() -> Option<usize> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmHWM:")?
                .split_ascii_whitespace()
                .next()?
                .parse()
                .ok()
        })
}
fn canary(kind: &str) {
    match kind {
        "allocation" => {
            let _ = assert_no_heap(|| std::hint::black_box(Box::new(std::hint::black_box(7u64))));
        }
        "reallocation" => {
            let mut v = Vec::with_capacity(1);
            v.push(1u8);
            assert_no_heap(|| {
                for i in 0..256 {
                    v.push(std::hint::black_box(i as u8));
                }
            });
        }
        "deallocation" => {
            let v = Box::new(7u64);
            assert_no_heap(|| drop(std::hint::black_box(v)));
        }
        _ => panic!("unknown canary"),
    }
    panic!("heap guard did not detect violation")
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(at) = args.iter().position(|a| a == "--fail-canary") {
        canary(&args[at + 1]);
    }
    let iterations: usize = args
        .iter()
        .position(|a| a == "--iterations")
        .map_or(4096, |i| args[i + 1].parse().unwrap());
    assert!((1..=1_000_000).contains(&iterations));
    let directory = args
        .iter()
        .position(|a| a == "--raw-dir")
        .map(|i| std::path::PathBuf::from(&args[i + 1]));
    if let Some(d) = &directory {
        std::fs::create_dir_all(d).unwrap();
    }
    for &case in CASES {
        let mut bench = Bench::new(case);
        let mut samples = vec![Sample::default(); iterations];
        std::hint::black_box(&mut *samples).fill(Sample::default());
        for i in 0..128 {
            bench.prepare(i);
            let start = bench.driver.frame();
            bench.render();
            bench.observe(start);
        }
        bench.commands = 0;
        bench.resource_commands = 0;
        bench.uploaded_bytes = 0;
        bench.exported_bytes = 0;
        let before = bench.counts.transitions.load(Ordering::Relaxed);
        for (i, s) in samples.iter_mut().enumerate() {
            bench.prepare(i + 128);
            s.domain_pending = bench.pending_domain;
            s.resource_pending = bench.pending_resource;
            let frame = bench.driver.frame();
            let old = bench.counts.transitions.load(Ordering::Relaxed);
            let starts = bench.counts.starts.load(Ordering::Relaxed);
            let start = Instant::now();
            bench.render();
            s.ns = start.elapsed().as_nanos().try_into().unwrap();
            let d = bench.driver.diagnostics();
            let u = bench.driver.usage();
            s.intervals = d.subranges;
            s.nodes = d.nodes_processed;
            s.edges = d.connections_processed;
            s.transitions = bench.counts.transitions.load(Ordering::Relaxed) - old;
            s.cascade_starts = bench.counts.starts.load(Ordering::Relaxed) - starts;
            if case.kind == 7 {
                assert_eq!(s.cascade_starts, case.loops as u64);
            }
            s.render_bytes = d.render_copied_bytes;
            s.maintenance_bytes = d.transfer_bytes;
            s.render_steps = d.render_validation_steps;
            s.maintenance_steps = d.validation_steps;
            s.pages = PAGES - u.free_pages;
            s.reserved = u.reserved_pages;
            s.versions = u.free_versions;
            s.snapshots = u.free_snapshots;
            bench.observe(frame);
        }
        if let Some(dir) = &directory {
            let mut file = std::io::BufWriter::new(
                std::fs::File::create(dir.join(format!("{}.csv", case.name))).unwrap(),
            );
            writeln!(file,"callback,nanoseconds,intervals,nodes,edges,transitions,render_bytes,maintenance_bytes,render_steps,maintenance_steps,pages,reserved_pages,free_versions,free_snapshots,domain_pending,resource_pending,cascade_starts").unwrap();
            for (i, s) in samples.iter().enumerate() {
                writeln!(
                    file,
                    "{i},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                    s.ns,
                    s.intervals,
                    s.nodes,
                    s.edges,
                    s.transitions,
                    s.render_bytes,
                    s.maintenance_bytes,
                    s.render_steps,
                    s.maintenance_steps,
                    s.pages,
                    s.reserved,
                    s.versions,
                    s.snapshots,
                    s.domain_pending,
                    s.resource_pending,
                    s.cascade_starts
                )
                .unwrap();
            }
            file.flush().unwrap();
        }
        let mut times: Vec<u64> = samples.iter().map(|s| s.ns).collect();
        times.sort_unstable();
        let pct = |p| times[(iterations - 1) * p / 1000];
        let config = configuration(bench.prepared, if case.kind == 0 { 0 } else { 63 });
        let rss = process_peak_rss_kib().map_or_else(|| "null".to_owned(), |n| n.to_string());
        println!("{{\"case\":\"{}\",\"callbacks\":{},\"frames_per_callback\":128,\"sample_rate\":48000,\"loops\":{},\"loop_length\":{},\"midi\":{},\"warmup_callbacks\":128,\"p50_ns\":{},\"p99_ns\":{},\"p999_ns\":{},\"max_ns\":{},\"deadline_ns\":{},\"deadline_misses\":{},\"transitions\":{},\"render_bytes\":{},\"maintenance_bytes\":{},\"nodes\":{},\"intervals\":{},\"pages_at_boundary_max\":{},\"prepared_resource_bytes\":{},\"commands\":{},\"resource_commands\":{},\"uploaded_bytes\":{},\"exported_bytes\":{},\"rust_heap_operations\":0,\"configuration\":{},\"process_lifetime_peak_rss_kib\":{},\"pending_requests_after_drain\":0,\"cascade_starts\":{}}}",case.name,iterations,case.loops,case.length,case.midi,pct(500),pct(990),pct(999),times[iterations-1],DEADLINE,times.iter().filter(|&&t|t>DEADLINE).count(),bench.counts.transitions.load(Ordering::Relaxed)-before,samples.iter().map(|s|s.render_bytes as u64).sum::<u64>(),samples.iter().map(|s|s.maintenance_bytes as u64).sum::<u64>(),samples.iter().map(|s|s.nodes as u64).sum::<u64>(),samples.iter().map(|s|s.intervals as u64).sum::<u64>(),samples.iter().map(|s|s.pages).max().unwrap(),PAGES*PAGE,bench.commands,bench.resource_commands,bench.uploaded_bytes,bench.exported_bytes,config,rss,samples.iter().map(|s|s.cascade_starts).sum::<u64>());
        bench.shutdown();
    }
}
