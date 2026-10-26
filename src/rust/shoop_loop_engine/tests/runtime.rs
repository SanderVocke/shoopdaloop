#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    processor::{ProcessBlock, Processor},
    resource::{CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    transport::{Lane, Outcome as RuntimeOutcome},
    Budgets, Config, Error, Frame, NodeId, PlannerId, PreparedConfig, Sequence,
};
use shoop_loop_engine::{audio::Smoothing, runtime::*, *};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;
#[path = "support/runtime_scenarios.rs"]
mod scenarios;
use scenarios::make_imports;
#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn id() -> LoopId {
    LoopId::from_parts(0, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
struct Log {
    events: Vec<Event>,
    last_frame: Option<Frame>,
    dropped: u64,
}
impl Log {
    fn new() -> Self {
        let mut events = vec![
            Event {
                frame: Frame(0),
                target: id(),
                kind: shoop_loop_engine::decision::EventKind::Pulse
            };
            512
        ];
        events.clear();
        Self {
            events,
            last_frame: None,
            dropped: 0,
        }
    }
}
impl BoundaryObserver for Log {
    fn boundary(&mut self, frame: Frame, decisions: &[Decision], events: &[Event], dropped: u64) {
        assert!(self.last_frame.is_none_or(|old| old < frame));
        self.last_frame = Some(frame);
        self.dropped = dropped;
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].state.id, id());
        assert!(self.events.len() + events.len() <= self.events.capacity());
        for event in events {
            assert_eq!(event.frame, frame);
            self.events.push(*event);
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
struct Fx;
impl Drop for Fx {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}
impl Processor<LoopPlanner> for Fx {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 4,
            audio_outputs: 2,
            midi_inputs: 2,
            midi_outputs: 1,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        for channel in 0..2 {
            for frame in 0..block.frames {
                let a = block.audio.input(channel).unwrap()[frame];
                let b = block.audio.input(channel + 2).unwrap()[frame];
                block.audio.output(channel).unwrap()[frame] = (a + b) * 0.5;
            }
        }
        let mut positions = [0; 2];
        loop {
            let next = (0..2)
                .filter_map(|port| {
                    block.midi_inputs[port]
                        .get(positions[port])
                        .map(|(offset, _)| (offset, port))
                })
                .min();
            let Some((offset, port)) = next else { break };
            let (_, packet) = block.midi_inputs[port].get(positions[port]).unwrap();
            positions[port] += 1;
            block.midi_outputs[0].push(offset, block.frames, packet)?;
        }
        Ok(())
    }
}
fn config() -> PreparedConfig {
    Config {
        max_frames: 32,
        max_nodes: 4,
        max_ports: 24,
        max_edges: 16,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 2048,
        midi_bytes_per_port: 8192,
        resource_slots: 32,
        resource_pages: 512,
        page_bytes: 64,
        command_slots: 32,
        scheduled_events: 32,
        budgets: Budgets {
            commands: 32,
            events: 32,
            subranges: 0,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 32,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
#[derive(Clone, Copy)]
struct Options {
    pages: usize,
    checkpoints: usize,
    requests: usize,
    media: usize,
    exports: usize,
    self_sync: bool,
    imports: bool,
    smoothing: Smoothing,
    input_events: usize,
    one_sample: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            pages: 512,
            checkpoints: 8,
            requests: 16,
            media: 8,
            exports: 4,
            self_sync: false,
            imports: false,
            smoothing: Smoothing::None,
            input_events: 512,
            one_sample: false,
        }
    }
}
fn setup(journal: usize, reversed: bool) -> (Controller<Fx>, Engine<Fx>) {
    let (control, engine, _) = setup_with(journal, reversed, Options::default());
    (control, engine)
}
fn setup_with(
    journal: usize,
    reversed: bool,
    options: Options,
) -> (Controller<Fx>, Engine<Fx>, Vec<rt_runtime::ResourceId>) {
    let mut cfg = config().limits();
    cfg.resource_pages = options.pages;
    if options.one_sample {
        cfg.midi_events_per_port = 4096;
        cfg.midi_bytes_per_port = 16384;
    }
    let config = cfg.prepare().unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: if options.one_sample { 2 } else { 8 },
            checkpoints_per_version: if options.one_sample {
                1
            } else {
                options.checkpoints
            },
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 9216,
        },
    )
    .unwrap();
    let topology = compile(
        Limits {
            loop_slots: 2,
            pending_requests: 17,
            journal_events: journal,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: id(),
                kind: LoopKind::Primitive,
                length: if options.one_sample {
                    1
                } else if options.imports {
                    4
                } else {
                    0
                },
                sync_source: options.self_sync.then_some(id()),
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let binding = Binding {
        target: id(),
        playback: node(0),
        capture: node(1),
        channels: 2,
        midi: true,
        gain: 1.,
        smoothing: options.smoothing,
        midi_events: options.input_events,
        midi_bytes: 2048,
        packet_bytes: 2048,
    };
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(1), node(2)],
        connections: Vec::new(),
    };
    for channel in 0..2 {
        graph.connections.push(Connection {
            source: Endpoint::host(PortType::Audio, channel),
            destination: Endpoint::node(node(2), PortType::Audio, channel),
        });
        graph.connections.push(Connection {
            source: Endpoint::node(node(0), PortType::Audio, channel),
            destination: Endpoint::node(node(2), PortType::Audio, channel + 2),
        });
        graph.connections.push(Connection {
            source: Endpoint::node(node(2), PortType::Audio, channel),
            destination: Endpoint::node(node(1), PortType::Audio, channel),
        });
        graph.connections.push(Connection {
            source: Endpoint::node(node(2), PortType::Audio, channel),
            destination: Endpoint::host(PortType::Audio, channel),
        });
    }
    graph.connections.extend([
        Connection {
            source: Endpoint::host(PortType::Midi, 0),
            destination: Endpoint::node(node(2), PortType::Midi, 0),
        },
        Connection {
            source: Endpoint::node(node(0), PortType::Midi, 0),
            destination: Endpoint::node(node(2), PortType::Midi, 1),
        },
        Connection {
            source: Endpoint::node(node(2), PortType::Midi, 0),
            destination: Endpoint::node(node(1), PortType::Midi, 0),
        },
        Connection {
            source: Endpoint::node(node(2), PortType::Midi, 0),
            destination: Endpoint::host(PortType::Midi, 0),
        },
    ]);
    if reversed {
        graph.nodes.reverse();
        graph.connections.reverse();
    }
    let imports = if options.imports {
        make_imports(&mut resources, options.one_sample)
    } else {
        Vec::new()
    };
    let imported_resources = imports
        .iter()
        .flat_map(|input| {
            [input.audio, input.midi]
                .into_iter()
                .flatten()
                .map(|source| source.snapshot.resource)
        })
        .collect();
    let (control, engine) = Engine::prepare_with_observer(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: options.requests,
            media_slots: options.media,
            export_slots: options.exports,
        },
        vec![binding],
        imports,
        vec![(node(2), Fx)],
        graph,
        Box::new(Log::new()),
    )
    .unwrap();
    (control, engine, imported_resources)
}
struct Host {
    early: bool,
    tail: f32,
    input: [[f32; 32]; 2],
    output: [[f32; 32]; 2],
    midi_in: [MidiBuffer; 1],
    midi_out: [MidiBuffer; 1],
}
impl Host {
    fn new() -> Self {
        Self {
            early: true,
            tail: 0.,
            input: [[0.; 32]; 2],
            output: [[0.; 32]; 2],
            midi_in: [MidiBuffer::new(16, 64).unwrap()],
            midi_out: [MidiBuffer::new(4096, 16384).unwrap()],
        }
    }
    fn step(&mut self, engine: &mut Engine<Fx>, frames: usize) {
        let at = engine.next_frame().0;
        self.midi_in[0].clear();
        self.midi_out[0].clear();
        for channel in 0..2 {
            for frame in 0..frames {
                let absolute = at + frame as u64;
                self.input[channel][frame] = if self.early && absolute < 4 {
                    (absolute + 1) as f32 * if channel == 0 { 2. } else { 20. }
                } else {
                    self.tail * if channel == 0 { 1. } else { 10. }
                };
            }
        }
        for (frame, packet) in [(0, &[0x90, 60, 100][..]), (2, &[0x80, 60, 45][..])] {
            if self.early && frame >= at && frame < at + frames as u64 {
                self.midi_in[0]
                    .push((frame - at) as u32, frames, packet)
                    .unwrap();
            }
        }
        let [left, right] = &mut self.output;
        assert_no_heap(|| {
            engine.process(
                AudioBlock {
                    frame: Frame(at),
                    frames,
                    inputs: &[&self.input[0][..frames], &self.input[1][..frames]],
                    outputs: &mut [&mut left[..frames], &mut right[..frames]],
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
fn submit(control: &mut Controller<Fx>, at: u64, operation: Operation) -> Sequence {
    assert_no_heap(|| control.submit(Lane::Normal, Frame(at), operation))
        .unwrap_or_else(|e| panic!("{:?}", e.error))
        .sequence()
}
fn transition(control: &mut Controller<Fx>, at: u64, transition: Transition) -> Sequence {
    submit(
        control,
        at,
        Operation::Transition {
            target: id(),
            timing: Timing::At,
            transition,
        },
    )
}
fn drain(control: &mut Controller<Fx>, replies: &mut Vec<(Sequence, Reply)>) {
    while let Some(completion) = control.try_completion() {
        assert!(matches!(
            completion.outcome,
            RuntimeOutcome::Scheduled { late_by: 0, .. }
        ));
    }
    while let Some(execution) = control.try_execution() {
        assert_eq!(execution.result, Ok(()));
        replies.push((execution.sequence, execution.operation_result.unwrap()));
    }
}
fn restore(frame: u64, note: bool, out: &mut Vec<(u64, Vec<u8>)>) {
    for channel in 0..16 {
        for packet in [
            [0xe0 | channel, 0, 64],
            [0xb0 | channel, 64, 0],
            [0xb0 | channel, 69, 0],
            [0xb0 | channel, 66, 0],
        ] {
            out.push((frame, packet.to_vec()));
        }
        if channel == 0 && note {
            out.push((frame, vec![0x90, 60, 100]));
        }
    }
}
fn midi_oracle() -> Vec<(u64, Vec<u8>)> {
    let mut result = vec![(0, vec![0x90, 60, 100]), (2, vec![0x80, 60, 45])];
    for frame in [4, 8, 12] {
        restore(frame, false, &mut result);
        result.push((frame, vec![0x90, 60, 100]));
        result.push((frame + 2, vec![0x80, 60, 45]));
    }
    restore(16, false, &mut result);
    restore(16, false, &mut result);
    result.push((16, vec![0x90, 60, 100]));
    result.push((18, vec![0x80, 60, 0]));
    restore(18, true, &mut result);
    result.push((19, vec![0x80, 60, 45]));
    restore(21, false, &mut result);
    restore(21, false, &mut result);
    result.push((21, vec![0x90, 60, 100]));
    result.push((22, vec![0x80, 60, 0]));
    result
}
fn read_export(
    control: &mut Controller<Fx>,
    engine: &mut Engine<Fx>,
    host: &mut Host,
    export: shoop_loop_engine::media::ExportId,
    midi: bool,
) -> Vec<u8> {
    submit(
        control,
        engine.next_frame().0,
        Operation::Cursor { export, midi },
    );
    host.step(engine, 1);
    let mut cursor = None;
    while let Some(completion) = control.try_completion() {
        assert!(matches!(
            completion.outcome,
            RuntimeOutcome::Scheduled { late_by: 0, .. }
        ));
    }
    while let Some(execution) = control.try_execution() {
        assert_eq!(execution.result, Ok(()));
        if let Some(Reply::Cursor(value)) = execution.operation_result {
            cursor = Some(value);
        } else {
            panic!("unexpected reply {:?}", execution.operation_result);
        }
    }
    let mut cursor = cursor.unwrap();
    let mut output = Vec::new();
    loop {
        control
            .read(cursor, vec![0; 64].into_boxed_slice())
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        host.step(engine, 1);
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.outcome, RuntimeOutcome::Applied);
        let rt_runtime::executor::Command::Resource(command) = completion.request else {
            panic!("resource reply")
        };
        let rt_runtime::resource::Reply::Read { written, eof } = command.reply().unwrap() else {
            panic!("read reply")
        };
        let rt_runtime::resource::Request::Read {
            cursor: next,
            bytes,
        } = command.into_request()
        else {
            panic!("read owner")
        };
        assert_eq!(next.position(), cursor.position() + written);
        cursor = next;
        output.extend_from_slice(&bytes[..written]);
        if eof {
            break;
        }
        assert!(written > 0);
    }
    output
}
fn event_oracle() -> Vec<Event> {
    use shoop_loop_engine::decision::EventKind::*;
    [
        (
            0,
            Transition(shoop_loop_engine::Transition::Record {
                monitor_previous: false,
            }),
        ),
        (0, Pulse),
        (
            4,
            Transition(shoop_loop_engine::Transition::Play {
                position: 0,
                retrigger: false,
            }),
        ),
        (4, Pulse),
        (8, Wrap),
        (8, Pulse),
        (
            12,
            Transition(shoop_loop_engine::Transition::Record {
                monitor_previous: true,
            }),
        ),
        (12, Wrap),
        (12, Pulse),
        (
            16,
            Transition(shoop_loop_engine::Transition::Play {
                position: 0,
                retrigger: false,
            }),
        ),
        (16, Pulse),
        (
            18,
            Transition(shoop_loop_engine::Transition::Seek {
                position: 1,
                retrigger: false,
            }),
        ),
        (21, Wrap),
        (21, Pulse),
        (22, Transition(shoop_loop_engine::Transition::Stop)),
    ]
    .into_iter()
    .map(|(frame, kind)| Event {
        frame: Frame(frame),
        target: id(),
        kind,
    })
    .collect()
}
fn encode_midi(events: &[(u64, Vec<u8>)]) -> Vec<u8> {
    events
        .iter()
        .flat_map(|(frame, packet)| {
            frame
                .to_le_bytes()
                .into_iter()
                .chain((packet.len() as u32).to_le_bytes())
                .chain(packet.iter().copied())
        })
        .collect()
}

#[shoop_test]
fn coordinated_stereo_midi_record_play_seek_export_and_repeated_capture_match_independent_oracles()
{
    let expected_midi = midi_oracle();
    for (partition, journal, reversed) in [
        (vec![24], 0, false),
        (vec![1; 24], 1, true),
        (vec![3, 2, 7, 1, 5, 6], 1024, true),
        (vec![24], 1024, false),
        (vec![1; 24], 1024, true),
    ] {
        let (mut control, mut engine) = setup(journal, reversed);
        let mut host = Host::new();
        let mut outcomes = Vec::new();
        let record = transition(
            &mut control,
            0,
            Transition::Record {
                monitor_previous: false,
            },
        );
        let first_prefix = submit(
            &mut control,
            3,
            Operation::Export {
                target: id(),
                selection: Selection::Recording,
            },
        );
        let first_take = submit(
            &mut control,
            4,
            Operation::Export {
                target: id(),
                selection: Selection::Playback,
            },
        );
        let play = transition(
            &mut control,
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(
            &mut control,
            12,
            Transition::Record {
                monitor_previous: true,
            },
        );
        let second_take = submit(
            &mut control,
            16,
            Operation::Export {
                target: id(),
                selection: Selection::Latest,
            },
        );
        transition(
            &mut control,
            16,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        );
        transition(
            &mut control,
            18,
            Transition::Seek {
                position: 1,
                retrigger: false,
            },
        );
        transition(&mut control, 22, Transition::Stop);
        let state0 = submit(&mut control, 0, Operation::State { target: id() });
        let state4 = submit(&mut control, 4, Operation::State { target: id() });
        host.step(&mut engine, 0);
        assert!(control.try_completion().is_none());
        assert_eq!(engine.next_frame(), Frame(0));
        let mut packets = Vec::new();
        for frames in partition {
            let at = engine.next_frame().0;
            host.step(&mut engine, frames);
            for frame in 0..frames {
                let absolute = at + frame as u64;
                let expected = match absolute {
                    0..=3 => (absolute + 1) as f32,
                    4..=15 => ((absolute - 4) % 4 + 1) as f32 * 0.5,
                    16 => 0.25,
                    17 | 18 => 0.5,
                    19 => 0.75,
                    20 => 1.,
                    21 => 0.25,
                    _ => 0.,
                };
                if host.output[0][frame] != expected {
                    drain(&mut control, &mut outcomes);
                    eprintln!("boundary outcomes: {outcomes:?}");
                }
                assert_eq!(host.output[0][frame], expected, "sample {absolute}");
                assert_eq!(
                    host.output[1][frame],
                    expected * 10.,
                    "right sample {absolute}"
                );
            }
            for i in 0..host.midi_out[0].len() {
                let (offset, packet) = host.midi_out[0].get(i).unwrap();
                packets.push((at + u64::from(offset), packet.to_vec()));
            }
        }
        assert_eq!(packets, expected_midi);
        drain(&mut control, &mut outcomes);
        for (sequence, frame) in [(record, 0), (play, 4)] {
            assert!(outcomes.iter().any(|(s,r)|*s==sequence && matches!(r,Reply::Transition(completion) if completion.outcome==Outcome::Applied { frame:Frame(frame) })));
        }
        for (sequence, mode, length) in [(state0, Mode::Recording, 0), (state4, Mode::Playing, 4)] {
            let Reply::State(cut) = outcomes.iter().find(|(s, _)| *s == sequence).unwrap().1 else {
                panic!("state")
            };
            assert_eq!(cut.state.mode, mode);
            assert_eq!(cut.state.content_length, length);
            assert_eq!(cut.state.position, 0);
        }
        for (sequence, frames, revision, gain, second) in [
            (first_prefix, 3, 1, 1., false),
            (first_take, 4, 1, 1., false),
            (second_take, 4, 2, 0.5, true),
        ] {
            let Reply::Export {
                id: export,
                cut,
                media,
            } = outcomes.iter().find(|(s, _)| *s == sequence).unwrap().1
            else {
                panic!("export {:?}", outcomes)
            };
            assert_eq!(media.requested_cut, cut.frame);
            assert_eq!(media.view.description.frames, frames);
            assert_eq!(media.view.description.revision, Revision(revision));
            let audio = read_export(&mut control, &mut engine, &mut host, export, false);
            let expected: Vec<u8> = (0..frames)
                .flat_map(|frame| [(frame + 1) as f32 * gain, (frame + 1) as f32 * 10. * gain])
                .flat_map(f32::to_le_bytes)
                .collect();
            assert_eq!(audio, expected);
            let midi = read_export(&mut control, &mut engine, &mut host, export, true);
            let mut expected = Vec::new();
            if second {
                restore(0, false, &mut expected);
            }
            expected.push((0, vec![0x90, 60, 100]));
            expected.push((2, vec![0x80, 60, 45]));
            assert_eq!(midi, encode_midi(&expected));
            submit(
                &mut control,
                engine.next_frame().0,
                Operation::Release { export },
            );
            host.step(&mut engine, 1);
            drain(&mut control, &mut Vec::new());
        }
        drop(control);
        let mut stopped = engine.into_stopped();
        let log = stopped.observer().as_any().downcast_ref::<Log>().unwrap();
        let full = event_oracle();
        let expected: Vec<_> = full
            .iter()
            .copied()
            .filter(|event| {
                journal > 0
                    && (journal > 1 || full.iter().find(|e| e.frame == event.frame) == Some(event))
            })
            .collect();
        assert_eq!(log.events, expected);
        assert_eq!(log.dropped, (full.len() - expected.len()) as u64);
        assert_no_heap(|| {
            stopped.cleanup().unwrap();
            stopped.cleanup().unwrap();
            for _ in 0..1024 {
                stopped.resources().begin_callback();
                stopped.resources().reclaim();
            }
            assert_eq!(stopped.media_usage().references, 0);
            assert_eq!(stopped.resource_usage().free_pages, 512);
            assert_eq!(stopped.resource_usage().free_versions, 32);
            assert_eq!(stopped.resource_usage().free_snapshots, 32);
        });
    }
}
