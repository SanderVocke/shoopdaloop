#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::Command,
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    processor::{ProcessBlock, Processor},
    resource::{self, CaptureGroupConfig, Format, MidiIndexConfig, ResourceStore},
    transport::{Lane, Outcome as RuntimeOutcome},
    Budgets, Config, Error, Frame, NodeId, PlannerId, Revision, Sequence,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    decision::EventKind,
    media,
    runtime::{Binding, BoundaryObserver, Cut, LoopPlanner, Operation, Reply, Selection, Settings},
    Completion, Decision, Entry, Event, Limits, LoopId, LoopKind, LoopSpec, Mode, Outcome,
    ProgramDescription, Section, State, Timing, TopologyDescription, Transition, Trigger,
};
use shoop_loops_test_support::{LoopControl, LoopRuntime};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[path = "support/nested_lifecycle.rs"]
mod nested_lifecycle;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
struct Log {
    events: Vec<Event>,
    states: Box<[[Option<State>; 6]; 64]>,
    dropped: u64,
}
impl Log {
    fn new() -> Self {
        let mut events = vec![
            Event {
                frame: Frame(0),
                target: id(0),
                kind: EventKind::Pulse
            };
            512
        ];
        events.clear();
        Self {
            events,
            states: Box::new([[None; 6]; 64]),
            dropped: 0,
        }
    }
}
impl BoundaryObserver for Log {
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], events: &[Event], dropped: u64) {
        assert_eq!(decisions.len(), 6);
        assert!(self.events.len() + events.len() <= self.events.capacity());
        self.events.extend_from_slice(events);
        self.dropped = dropped;
        for decision in decisions {
            self.states[frame.0 as usize][decision.state.id.slot() as usize] = Some(decision.state);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
struct Mix;
impl Drop for Mix {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}
impl Processor<LoopPlanner> for Mix {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 1,
            midi_inputs: 2,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        for frame in 0..block.frames {
            let value =
                (block.audio.input(0).unwrap()[frame] + block.audio.input(1).unwrap()[frame]) * 0.5;
            block.audio.output(0).unwrap()[frame] = value;
        }
        let mut cursors = [0; 2];
        while let Some((offset, port)) = (0..2)
            .filter_map(|port| {
                block.midi_inputs[port]
                    .get(cursors[port])
                    .map(|(offset, _)| (offset, port))
            })
            .min()
        {
            let (_, packet) = block.midi_inputs[port].get(cursors[port]).unwrap();
            cursors[port] += 1;
            block.midi_outputs[0].push(offset, block.frames, packet)?;
        }
        Ok(())
    }
}
fn encoded(events: &[(u64, Vec<u8>)]) -> Vec<u8> {
    events
        .iter()
        .flat_map(|(frame, bytes)| {
            frame
                .to_le_bytes()
                .into_iter()
                .chain((bytes.len() as u32).to_le_bytes())
                .chain(bytes.iter().copied())
        })
        .collect()
}
fn upload(store: &mut ResourceStore, format: Format, frames: u64, bytes: &[u8]) -> media::Source {
    let resource = store.create().unwrap();
    let description = resource::Description {
        format,
        frames,
        bytes: bytes.len(),
    };
    let token = if format == Format::Midi {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, &MidiState::default())
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    for (page, chunk) in bytes.chunks(64).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 64, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
fn setup(
    journal: usize,
    reversed: bool,
) -> (
    LoopControl<Mix>,
    LoopRuntime<Mix>,
    Vec<rt_runtime::ResourceId>,
) {
    setup_with_pages(journal, reversed, 137)
}
fn setup_with_pages(
    journal: usize,
    reversed: bool,
    pages: usize,
) -> (
    LoopControl<Mix>,
    LoopRuntime<Mix>,
    Vec<rt_runtime::ResourceId>,
) {
    let config = Config {
        max_frames: 32,
        max_nodes: 16,
        max_ports: 64,
        max_edges: 64,
        audio_inputs: 4,
        audio_outputs: 4,
        midi_inputs: 4,
        midi_outputs: 4,
        midi_events_per_port: 4096,
        midi_bytes_per_port: 16384,
        resource_slots: 64,
        resource_pages: pages,
        page_bytes: 64,
        command_slots: 64,
        scheduled_events: 64,
        budgets: Budgets {
            commands: 64,
            events: 64,
            subranges: 0,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 64,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 8,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 8,
            members_per_group: 2,
            staging_bytes_per_group: 9216,
        },
    )
    .unwrap();
    let mut imports = Vec::new();
    let mut handles = Vec::new();
    for (target, revision, samples, packets) in [
        (1, 1, vec![4f32], vec![(0, vec![0x90, 61, 80])]),
        (
            2,
            0,
            vec![6f32, 12.],
            vec![(0, vec![0x90, 62, 90]), (1, vec![0x80, 62, 32])],
        ),
        (3, 0, vec![8f32], vec![(0, vec![0x90, 63, 70])]),
    ] {
        let frames = samples.len() as u64;
        let bytes: Vec<u8> = samples.into_iter().flat_map(f32::to_le_bytes).collect();
        let audio = upload(
            &mut resources,
            Format::Audio { channels: 1 },
            frames,
            &bytes,
        );
        let midi = upload(&mut resources, Format::Midi, frames, &encoded(&packets));
        handles.extend([audio.snapshot.resource, midi.snapshot.resource]);
        imports.push(media::Input {
            description: media::Description {
                owner: id(target),
                revision: Revision(revision),
                frames,
                channels: 1,
                midi: true,
            },
            audio: Some(audio),
            midi: Some(midi),
        });
    }
    let mut description = TopologyDescription {
        loops: (0..6)
            .map(|slot| LoopSpec {
                id: id(slot),
                kind: if slot < 4 {
                    LoopKind::Primitive
                } else {
                    LoopKind::Composite
                },
                length: match slot {
                    2 => 2,
                    3 => 1,
                    _ => 0,
                },
                sync_source: (slot >= 4).then_some(id(1)),
            })
            .collect(),
        triggers: vec![
            Trigger {
                source: id(0),
                target: id(1),
                mode: Mode::Playing,
            },
            Trigger {
                source: id(1),
                target: id(2),
                mode: Mode::Playing,
            },
        ],
        programs: vec![
            ProgramDescription {
                source: id(4),
                nominal_unit_frames: 1,
                sections: vec![Section {
                    entries: vec![Entry {
                        target: id(5),
                        delay: 1,
                        duration: Some(5),
                        repetitions: 1,
                        mode: None,
                    }],
                }],
            },
            ProgramDescription {
                source: id(5),
                nominal_unit_frames: 1,
                sections: [(Mode::Playing, 2), (Mode::Recording, 2), (Mode::Stopped, 1)]
                    .into_iter()
                    .map(|(mode, duration)| Section {
                        entries: vec![Entry {
                            target: id(3),
                            delay: 0,
                            duration: Some(duration),
                            repetitions: 1,
                            mode: Some(mode),
                        }],
                    })
                    .collect(),
            },
        ],
    };
    if reversed {
        description.loops.reverse();
        description.triggers.reverse();
        description.programs.reverse();
        imports.reverse();
    }
    let topology = compile(
        Limits {
            loop_slots: 6,
            pending_requests: 33,
            journal_events: journal,
            ..Limits::default()
        },
        &description,
    )
    .unwrap();
    let mut graph = GraphDescription {
        nodes: (0..12).map(node).collect(),
        connections: Vec::new(),
    };
    let mut bindings = Vec::new();
    let mut effects = Vec::new();
    for slot in 0..4 {
        let playback = node(3 * slot);
        let effect = node(3 * slot + 1);
        let capture = node(3 * slot + 2);
        effects.push((effect, Mix));
        bindings.push(Binding {
            target: id(slot),
            playback,
            capture,
            channels: 1,
            midi: true,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 512,
            midi_bytes: 2048,
            packet_bytes: 2048,
        });
        for kind in [PortType::Audio, PortType::Midi] {
            graph.connections.extend([
                Connection {
                    source: Endpoint::host(kind, slot),
                    destination: Endpoint::node(effect, kind, 0),
                },
                Connection {
                    source: Endpoint::node(playback, kind, 0),
                    destination: Endpoint::node(effect, kind, 1),
                },
                Connection {
                    source: Endpoint::node(effect, kind, 0),
                    destination: Endpoint::node(capture, kind, 0),
                },
                Connection {
                    source: Endpoint::node(effect, kind, 0),
                    destination: Endpoint::host(kind, slot),
                },
            ]);
        }
    }
    if reversed {
        graph.nodes.reverse();
        graph.connections.reverse();
        bindings.reverse();
        effects.reverse();
    }
    let (control, engine) = LoopRuntime::prepare_with_observer(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 32,
            media_slots: 16,
            export_slots: 4,
        },
        bindings,
        imports,
        effects,
        graph,
        Box::new(Log::new()),
    )
    .unwrap();
    (control, engine, handles)
}
struct Host {
    input: [[f32; 32]; 4],
    output: [[f32; 32]; 4],
    midi_in: [MidiBuffer; 4],
    midi_out: [MidiBuffer; 4],
}
impl Host {
    fn new() -> Self {
        Self {
            input: [[0.; 32]; 4],
            output: [[0.; 32]; 4],
            midi_in: std::array::from_fn(|_| MidiBuffer::new(2, 6).unwrap()),
            midi_out: std::array::from_fn(|_| MidiBuffer::new(4096, 16384).unwrap()),
        }
    }
    fn step(&mut self, engine: &mut LoopRuntime<Mix>, frames: usize) {
        let at = engine.next_frame();
        for port in 0..4 {
            self.midi_in[port].clear();
            self.midi_out[port].clear();
            for offset in 0..frames {
                let frame = at.0 + offset as u64;
                self.input[port][offset] = match port {
                    0 if frame < 4 => 2. * (frame + 1) as f32,
                    3 => 2.,
                    _ => 0.,
                };
            }
        }
        for (frame, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
            if frame >= at.0 && frame < at.0 + frames as u64 {
                self.midi_in[0]
                    .push((frame - at.0) as u32, frames, &packet)
                    .unwrap();
            }
        }
        let [a, b, c, d] = &mut self.output;
        assert_no_heap(|| {
            engine.process(
                AudioBlock {
                    frame: at,
                    frames,
                    inputs: &[
                        &self.input[0][..frames],
                        &self.input[1][..frames],
                        &self.input[2][..frames],
                        &self.input[3][..frames],
                    ],
                    outputs: &mut [
                        &mut a[..frames],
                        &mut b[..frames],
                        &mut c[..frames],
                        &mut d[..frames],
                    ],
                },
                MidiBlock {
                    inputs: &self.midi_in,
                    outputs: &mut self.midi_out,
                },
            )
        })
        .unwrap();
    }
}
fn submit(control: &mut LoopControl<Mix>, frame: u64, operation: Operation) -> Sequence {
    assert_no_heap(|| control.submit(Lane::Normal, Frame(frame), operation))
        .unwrap_or_else(|error| panic!("{:?}", error.error))
        .sequence()
}
fn transition(
    control: &mut LoopControl<Mix>,
    frame: u64,
    target: u32,
    transition: Transition,
) -> Sequence {
    submit(
        control,
        frame,
        Operation::Transition {
            target: id(target),
            timing: Timing::At,
            transition,
        },
    )
}
fn drain(control: &mut LoopControl<Mix>) -> Vec<rt_runtime::schedule::Execution<Operation, Reply>> {
    while let Some(completion) = assert_no_heap(|| control.try_completion()) {
        assert!(matches!(
            completion.outcome,
            RuntimeOutcome::Scheduled { late_by: 0, .. }
        ));
    }
    let mut replies = Vec::new();
    while let Some(reply) = assert_no_heap(|| control.try_execution()) {
        assert_eq!(reply.result, Ok(()));
        assert!(!matches!(reply.operation_result, Some(Reply::Failed(_))));
        replies.push(reply);
    }
    replies
}
fn read(
    control: &mut LoopControl<Mix>,
    engine: &mut LoopRuntime<Mix>,
    host: &mut Host,
    export: media::ExportId,
    midi: bool,
) -> Vec<u8> {
    submit(
        control,
        engine.next_frame().0,
        Operation::Cursor { export, midi },
    );
    host.step(engine, 1);
    let replies = drain(control);
    assert_eq!(replies.len(), 1);
    let Some(Reply::Cursor(cursor)) = replies[0].operation_result else {
        panic!("cursor")
    };
    control
        .read(cursor, vec![0; 64].into_boxed_slice())
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    host.step(engine, 1);
    let completion = assert_no_heap(|| control.try_completion()).unwrap();
    assert_eq!(completion.outcome, RuntimeOutcome::Applied);
    let Command::Resource(command) = completion.request else {
        panic!("resource")
    };
    let Some(resource::Reply::Read { written, eof: true }) = command.reply() else {
        panic!("complete content")
    };
    let resource::Request::Read { bytes, .. } = command.into_request() else {
        panic!("owning bytes")
    };
    bytes[..written].to_vec()
}
fn restored(frame: u64, out: &mut Vec<(u64, Vec<u8>)>) {
    for channel in 0..16 {
        for packet in [
            [0xe0 | channel, 0, 64],
            [0xb0 | channel, 64, 0],
            [0xb0 | channel, 69, 0],
            [0xb0 | channel, 66, 0],
        ] {
            out.push((frame, packet.to_vec()));
        }
    }
}
fn midi_reference() -> [Vec<(u64, Vec<u8>)>; 4] {
    let mut out: [_; 4] = std::array::from_fn(|_| Vec::new());
    out[0].extend([(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])]);
    for frame in [4, 8, 12, 16] {
        restored(frame, &mut out[0]);
        out[0].push((frame, vec![0x90, 60, 100]));
        if frame + 2 < 18 {
            out[0].push((frame + 2, vec![0x80, 60, 45]));
        }
    }
    out[0].push((18, vec![0x80, 60, 0]));
    for frame in 4..18 {
        if frame > 4 {
            out[1].push((frame, vec![0x80, 61, 0]));
        }
        restored(frame, &mut out[1]);
        out[1].push((frame, vec![0x90, 61, 80]));
    }
    out[1].push((18, vec![0x80, 61, 0]));
    for frame in (4..18).step_by(2) {
        restored(frame, &mut out[2]);
        out[2].push((frame, vec![0x90, 62, 90]));
        out[2].push((frame + 1, vec![0x80, 62, 32]));
    }
    for frame in [7, 8, 13, 14, 15, 16] {
        if frame != 7 && frame != 13 {
            out[3].push((frame, vec![0x80, 63, 0]));
        }
        restored(frame, &mut out[3]);
        out[3].push((frame, vec![0x90, 63, 70]));
        if frame == 8 || frame == 16 {
            out[3].push((frame + 1, vec![0x80, 63, 0]));
        }
    }
    out
}
fn semantic_reference() -> Vec<Event> {
    let mut events = Vec::new();
    for frame in 0..19 {
        let mut add = |target, kind| {
            events.push(Event {
                frame: Frame(frame),
                target: id(target),
                kind,
            })
        };
        if frame == 0 {
            add(
                0,
                EventKind::Transition(Transition::Record {
                    monitor_previous: false,
                }),
            );
            add(0, EventKind::Pulse);
            add(1, EventKind::Refused(shoop_loops::Error::EmptyContent));
        }
        if frame == 3 {
            add(
                1,
                EventKind::Transition(Transition::Replace {
                    length: 1,
                    revision: Revision(1),
                    preserve_position: false,
                }),
            );
        }
        if frame == 10 {
            add(3, EventKind::CaptureExhausted);
        }
        if frame == 18 {
            for target in [0, 1, 2, 4, 5] {
                add(target, EventKind::Transition(Transition::Stop));
            }
            continue;
        }
        if !(4..18).contains(&frame) {
            continue;
        }
        if frame == 4 {
            add(
                0,
                EventKind::Transition(Transition::Play {
                    position: 0,
                    retrigger: false,
                }),
            );
            add(0, EventKind::Pulse);
        } else if frame % 4 == 0 {
            add(0, EventKind::Wrap);
            add(0, EventKind::Pulse);
        }
        if frame == 4 {
            add(
                1,
                EventKind::Transition(Transition::Play {
                    position: 0,
                    retrigger: false,
                }),
            );
        }
        if frame > 4 {
            add(1, EventKind::Wrap);
        }
        add(1, EventKind::Pulse);
        if frame == 4 {
            add(
                2,
                EventKind::Transition(Transition::Play {
                    position: 0,
                    retrigger: false,
                }),
            );
            add(2, EventKind::Pulse);
        } else if frame % 2 == 0 {
            add(2, EventKind::Wrap);
            add(2, EventKind::Pulse);
        }
        if frame == 6 {
            add(
                4,
                EventKind::Transition(Transition::PlayIteration {
                    iteration: 0,
                    retrigger: true,
                }),
            );
        }
        add(4, EventKind::Pulse);
        if frame >= 6 {
            add(
                4,
                EventKind::Iteration {
                    iteration: (frame - 6) % 6,
                    passes: (frame - 6) / 6,
                },
            );
        }
        if frame == 6 || frame == 12 {
            add(5, EventKind::Transition(Transition::Stop));
        } else {
            if frame == 7 || frame == 13 {
                add(
                    5,
                    EventKind::Transition(Transition::PlayIteration {
                        iteration: 0,
                        retrigger: true,
                    }),
                );
            }
            add(5, EventKind::Pulse);
            if frame >= 7 {
                add(
                    5,
                    EventKind::Iteration {
                        iteration: (frame - 7) % 6,
                        passes: 0,
                    },
                );
            }
        }
        match frame {
            17 => add(3, EventKind::Transition(Transition::Stop)),
            7 | 13 => {
                add(
                    3,
                    EventKind::Transition(Transition::Play {
                        position: 0,
                        retrigger: true,
                    }),
                );
                add(3, EventKind::Pulse);
            }
            8 | 14 | 16 => {
                add(3, EventKind::Wrap);
                add(3, EventKind::Pulse);
            }
            9 => {
                add(
                    3,
                    EventKind::Transition(Transition::Record {
                        monitor_previous: false,
                    }),
                );
                add(3, EventKind::Pulse);
            }
            15 => {
                add(3, EventKind::Refused(shoop_loops::Error::Capacity));
                add(3, EventKind::Wrap);
                add(3, EventKind::Pulse);
            }
            _ => {}
        }
    }
    events
}
fn check_state(frame: u64, state: State) {
    let frame = frame.min(18);
    let mut mode = Mode::Stopped;
    let mut length = 0;
    let mut revision = 0;
    let mut position = 0;
    let mut cycles = 0;
    let mut capture_frames = 0;
    let mut capture_revision = 0;
    let mut incomplete = false;
    let mut last_pulse = None;
    let mut composite = None;
    match state.id.slot() {
        0 => {
            capture_revision = 1;
            capture_frames = frame.min(4);
            if frame < 4 {
                mode = Mode::Recording;
                last_pulse = Some(0);
            } else {
                length = 4;
                revision = 1;
                cycles = (frame.min(17) - 4) / 4;
                last_pulse = Some(4 + 4 * cycles);
                if frame < 18 {
                    mode = Mode::Playing;
                    position = (frame - 4) % 4;
                }
            }
        }
        1 => {
            if frame >= 3 {
                length = 1;
                revision = 1;
            }
            if frame >= 4 {
                cycles = frame.min(17) - 4;
                last_pulse = Some(frame.min(17));
                if frame < 18 {
                    mode = Mode::Playing;
                }
            }
        }
        2 => {
            length = 2;
            if frame >= 4 {
                cycles = (frame.min(17) - 4) / 2;
                last_pulse = Some(4 + 2 * cycles);
                if frame < 18 {
                    mode = Mode::Playing;
                    position = (frame - 4) % 2;
                }
            }
        }
        3 => {
            length = 1;
            if [7, 8, 13, 14, 15, 16].contains(&frame) {
                mode = Mode::Playing;
                cycles = if frame < 9 { frame - 7 } else { frame - 13 };
                last_pulse = Some(frame);
            } else if (9..13).contains(&frame) {
                cycles = 1;
                last_pulse = Some(9);
                if frame == 9 {
                    mode = Mode::Recording;
                    position = 1;
                }
            } else if frame >= 17 {
                cycles = 3;
                last_pulse = Some(16);
            }
            if frame >= 9 {
                capture_revision = 1;
            }
            if frame >= 10 {
                capture_frames = 1;
                incomplete = true;
            }
        }
        4 => {
            length = 6;
            let mut cursor = shoop_loops::CompositeState::default();
            if frame >= 4 {
                last_pulse = Some(frame.min(17));
            }
            if frame >= 6 {
                cursor.passes = (frame.min(17) - 6) / 6;
                cursor.entered_at = Some(Frame(frame));
                if frame < 18 {
                    mode = Mode::Playing;
                    cursor.iteration = (frame - 6) % 6;
                }
            }
            composite = Some(cursor);
        }
        5 => {
            length = 5;
            let mut cursor = shoop_loops::CompositeState::default();
            if frame >= 4 {
                last_pulse = Some(match frame {
                    6 => 5,
                    12 => 11,
                    18 => 17,
                    _ => frame,
                });
            }
            if frame >= 6 {
                cursor.entered_at = Some(Frame(frame));
                if (7..12).contains(&frame) || (13..18).contains(&frame) {
                    mode = Mode::Playing;
                    cursor.iteration = (frame - 7) % 6;
                }
            }
            composite = Some(cursor);
        }
        _ => unreachable!(),
    }
    assert_eq!(
        (
            state.mode,
            state.content_length,
            state.content_revision,
            state.position,
            state.cycles
        ),
        (mode, length, Revision(revision), position, cycles),
        "frame {frame} loop {:?}",
        state.id
    );
    assert_eq!(
        (
            state.capture_frames,
            state.capture_revision,
            state.capture_incomplete,
            state.monitor_previous
        ),
        (
            capture_frames,
            Revision(capture_revision),
            incomplete,
            false
        ),
        "frame {frame} loop {:?}",
        state.id
    );
    assert_eq!(
        state.last_pulse,
        last_pulse.map(Frame),
        "frame {frame} loop {:?}",
        state.id
    );
    assert_eq!(
        state.composite, composite,
        "frame {frame} loop {:?}",
        state.id
    );
}
fn state_reply(
    replies: &[rt_runtime::schedule::Execution<Operation, Reply>],
    sequence: Sequence,
) -> Cut {
    let Some(Reply::State(cut)) = replies
        .iter()
        .find(|r| r.sequence == sequence)
        .unwrap()
        .operation_result
    else {
        panic!("state cut")
    };
    cut
}
#[shoop_test]
fn cascades_nested_clocked_program_and_capture_outcomes_match_independent_media_and_journals() {
    let expected_midi = midi_reference();
    let expected_events = semantic_reference();
    let prior_pages = 3 * 2 + 2;
    let midi_reservation = (512usize * 12 + 2048).div_ceil(64);
    assert_eq!(prior_pages + 1 + midi_reservation, 137);
    let captured_pages = 1 + 15usize.div_ceil(64);
    assert_eq!(prior_pages + captured_pages + midi_reservation, 138);
    assert_eq!(prior_pages + captured_pages + 1 + midi_reservation, 139);
    for (partition, journal, reversed) in [
        (vec![19], 1024, false),
        (vec![1; 19], 0, true),
        (vec![3, 2, 7, 1, 6], 1, true),
    ] {
        let (mut control, mut engine, handles) = setup(journal, reversed);
        let mut host = Host::new();
        let selected = control
            .media()
            .iter()
            .find(|m| m.target == id(1) && m.revision == Revision(1))
            .unwrap()
            .media;
        transition(
            &mut control,
            0,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let countdown = submit(
            &mut control,
            0,
            Operation::Transition {
                target: id(4),
                timing: Timing::AfterTicks {
                    source: id(1),
                    skip: 2,
                },
                transition: Transition::PlayIteration {
                    iteration: 0,
                    retrigger: true,
                },
            },
        );
        submit(
            &mut control,
            3,
            Operation::Select {
                target: id(1),
                timing: Timing::At,
                media: selected,
                preserve_position: false,
            },
        );
        transition(
            &mut control,
            4,
            0,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        let export_a = submit(
            &mut control,
            4,
            Operation::Export {
                target: id(0),
                selection: Selection::Playback,
            },
        );
        let mut queries = Vec::new();
        for (frame, target) in [
            (4, 0),
            (4, 1),
            (4, 2),
            (6, 4),
            (7, 5),
            (9, 3),
            (10, 3),
            (15, 3),
        ] {
            queries.push((
                frame,
                target,
                submit(&mut control, frame, Operation::State { target: id(target) }),
            ));
        }
        let export_d = submit(
            &mut control,
            10,
            Operation::Export {
                target: id(3),
                selection: Selection::Latest,
            },
        );
        let export_later = submit(
            &mut control,
            15,
            Operation::Export {
                target: id(3),
                selection: Selection::Latest,
            },
        );
        for target in [0, 1, 2, 3, 4] {
            transition(&mut control, 18, target, Transition::Stop);
        }
        let mut packets: [Vec<(u64, Vec<u8>)>; 4] = std::array::from_fn(|_| Vec::new());
        for frames in partition {
            let at = engine.next_frame().0;
            host.step(&mut engine, frames);
            for offset in 0..frames {
                let frame = at + offset as u64;
                let expected = [
                    if frame < 4 {
                        (frame + 1) as f32
                    } else if frame < 18 {
                        ((frame - 4) % 4 + 1) as f32 * 0.5
                    } else {
                        0.
                    },
                    if (4..18).contains(&frame) { 2. } else { 0. },
                    if (4..18).contains(&frame) {
                        if frame % 2 == 0 {
                            3.
                        } else {
                            6.
                        }
                    } else {
                        0.
                    },
                    if [7, 8, 13, 14, 15, 16].contains(&frame) {
                        5.
                    } else {
                        1.
                    },
                ];
                for port in 0..4 {
                    assert_eq!(
                        host.output[port][offset], expected[port],
                        "frame {frame} port {port}"
                    );
                }
            }
            for port in 0..4 {
                for index in 0..host.midi_out[port].len() {
                    let (offset, packet) = host.midi_out[port].get(index).unwrap();
                    packets[port].push((at + u64::from(offset), packet.to_vec()));
                }
            }
        }
        assert_eq!(packets, expected_midi);
        assert_eq!(
            engine.resource_usage().free_pages,
            137 - prior_pages - captured_pages
        );
        let replies = drain(&mut control);
        for reply in &replies {
            if let Some(Reply::Transition(completion)) = reply.operation_result {
                assert_eq!(completion.sequence, reply.sequence);
                assert_eq!(
                    completion.outcome,
                    Outcome::Applied {
                        frame: reply.actual_frame
                    }
                );
                assert_eq!(completion.requested, reply.requested_frame);
                assert_eq!(Some(completion.effective), reply.delivered_frame);
            }
        }
        let completed = replies.iter().find(|r| r.sequence == countdown).unwrap();
        assert_eq!(completed.requested_frame, Frame(0));
        assert_eq!(completed.delivered_frame, Some(Frame(0)));
        assert_eq!(completed.actual_frame, Frame(6));
        assert_eq!(completed.late_by(), 0);
        assert!(matches!(
            completed.operation_result,
            Some(Reply::Transition(Completion {
                outcome: Outcome::Applied { frame: Frame(6) },
                ..
            }))
        ));
        for (frame, target, sequence) in queries {
            let cut = state_reply(&replies, sequence);
            assert_eq!(cut.frame, Frame(frame));
            assert_eq!(cut.state.id, id(target));
            match (frame, target) {
                (4, 0..=2) => {
                    assert_eq!(cut.state.mode, Mode::Playing);
                    assert_eq!(cut.state.position, 0);
                    assert_eq!(cut.state.last_pulse, Some(Frame(4)));
                }
                (6, 4) | (7, 5) => {
                    assert_eq!(cut.state.mode, Mode::Playing);
                    let cursor = cut.state.composite.unwrap();
                    assert_eq!(cursor.iteration, 0);
                    assert_eq!(cursor.entered_at, Some(Frame(frame)));
                }
                (9, 3) => {
                    assert_eq!(cut.state.mode, Mode::Recording);
                    assert_eq!(cut.state.capture_frames, 0);
                    assert_eq!(cut.state.capture_revision, Revision(1));
                    assert_eq!(cut.capture_failure, None);
                }
                (10, 3) | (15, 3) => {
                    assert_eq!(
                        cut.state.mode,
                        if frame == 10 {
                            Mode::Stopped
                        } else {
                            Mode::Playing
                        }
                    );
                    assert_eq!(cut.state.capture_frames, 1);
                    assert_eq!(cut.state.capture_revision, Revision(1));
                    assert!(cut.state.capture_incomplete);
                    assert_eq!(cut.capture_failure, Some(Error::Capacity));
                }
                _ => unreachable!(),
            }
        }
        let mut exports = Vec::new();
        for (sequence, cut_frame, revision, frames) in [
            (export_a, 4, 1, 4),
            (export_d, 10, 1, 1),
            (export_later, 15, 1, 1),
        ] {
            let Some(Reply::Export {
                id: export,
                cut,
                media,
            }) = replies
                .iter()
                .find(|r| r.sequence == sequence)
                .unwrap()
                .operation_result
            else {
                panic!("export")
            };
            assert_eq!(cut.frame, Frame(cut_frame));
            assert_eq!(media.requested_cut, Frame(cut_frame));
            assert_eq!(media.view.description.revision, Revision(revision));
            assert_eq!(media.view.description.frames, frames);
            for source in [media.view.audio, media.view.midi].into_iter().flatten() {
                assert_eq!(source.snapshot.description.frames, frames);
                assert_eq!(
                    source.snapshot.cut_frame,
                    Frame(if sequence == export_a { 4 } else { 10 })
                );
                assert_eq!(
                    source.snapshot.capture.unwrap().incomplete,
                    if sequence == export_a {
                        None
                    } else {
                        Some(Error::Capacity)
                    }
                );
            }
            exports.push((export, sequence == export_a));
        }
        for &(export, a) in &exports {
            let samples = if a {
                vec![1f32, 2., 3., 4.]
            } else {
                vec![1f32]
            };
            assert_eq!(
                read(&mut control, &mut engine, &mut host, export, false),
                samples
                    .into_iter()
                    .flat_map(f32::to_le_bytes)
                    .collect::<Vec<_>>()
            );
            let source = if a {
                vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])]
            } else {
                vec![(0, vec![0x80, 63, 0])]
            };
            assert_eq!(
                read(&mut control, &mut engine, &mut host, export, true),
                encoded(&source)
            );
        }
        let export = exports[1].0;
        submit(
            &mut control,
            engine.next_frame().0,
            Operation::MidiInitial { export, channel: 0 },
        );
        host.step(&mut engine, 1);
        let reply = drain(&mut control);
        let Some(Reply::MidiInitial { state, .. }) = reply[0].operation_result else {
            panic!("initial state")
        };
        assert!(state.note(63).unwrap().held());
        assert_eq!(state.note(63).unwrap().velocity(), 70);
        for &(export, _) in &exports {
            submit(
                &mut control,
                engine.next_frame().0,
                Operation::Release { export },
            );
        }
        host.step(&mut engine, 1);
        let released = drain(&mut control);
        assert_eq!(released.len(), 3);
        assert!(released
            .iter()
            .all(|r| r.operation_result == Some(Reply::Done)));
        drop(control);
        let mut stopped = engine.into_stopped();
        let log = stopped.observer().as_any().downcast_ref::<Log>().unwrap();
        let mut retained = Vec::new();
        for frame in 0..19 {
            retained.extend(
                expected_events
                    .iter()
                    .filter(|event| event.frame == Frame(frame))
                    .take(journal)
                    .copied(),
            );
        }
        assert_eq!(log.events, retained);
        assert_eq!(log.dropped, (expected_events.len() - retained.len()) as u64);
        for (frame, states) in log.states.iter().enumerate() {
            for state in states.iter().flatten() {
                check_state(frame as u64, *state);
            }
        }
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            for resource in &handles {
                stopped.resources().remove(*resource).unwrap();
            }
            for _ in 0..1024 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.media_usage().references, 0);
            assert_eq!(stopped.resource_usage().free_pages, 137);
            assert_eq!(stopped.resource_usage().free_versions, 64);
            assert_eq!(stopped.resource_usage().free_snapshots, 64);
        });
    }
}
