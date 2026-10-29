//! Standalone prepared stereo/MIDI record → playback → FX → capture lifecycle.
//! No device, application backend or warm-up callback is involved.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, Control, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::ConsumerAccess,
    processor::{ProcessBlock, Processor},
    resource::{CaptureGroupConfig, MidiIndexConfig, Request, ResourceStore},
    transport::Lane,
    Budgets, Config, Error, Frame, NodeId, PlannerId,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{
        Binding, LoopClient, LoopNode, LoopPlanner, Operation, Options, Prepared, Reply, Selection,
        Settings,
    },
    Limits, LoopId, LoopKind, LoopSpec, Mode, Timing, TopologyDescription, Transition,
};
/// The application's node type: loop nodes next to its own processors.
enum Node {
    Loop(LoopNode),
    Mix(Mix),
}
impl Processor<LoopPlanner> for Node {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::Mix(node) => node.ports(),
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(node) => node.planning(),
            Self::Mix(node) => node.planning(),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::Mix(node) => node.process(block),
        }
    }
}
/// The application's control side: its runtime endpoint and the loop vocabulary.
struct Controls {
    runtime: Control<Node, LoopPlanner>,
    loops: LoopClient,
}
struct Mix;
impl Processor<LoopPlanner> for Mix {
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
                let live = block.audio.input(channel).ok_or(Error::InvalidShape)?[frame];
                let playback = block.audio.input(channel + 2).ok_or(Error::InvalidShape)?[frame];
                block.audio.output(channel).ok_or(Error::InvalidShape)?[frame] =
                    (live + playback) * 0.5;
            }
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
            let (_, packet) = block.midi_inputs[port]
                .get(cursors[port])
                .ok_or(Error::InvalidState)?;
            cursors[port] += 1;
            block.midi_outputs[0].push(offset, block.frames, packet)?;
        }
        Ok(())
    }
}
fn send(control: &mut Controls, at: u64, operation: Operation) {
    control
        .runtime
        .submit_prepared(Lane::Normal, control.loops.command(Frame(at), operation))
        .unwrap_or_else(|e| panic!("submission: {:?}", e.error));
}
fn silence(
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    input: &[MidiBuffer; 1],
    midi: &mut [MidiBuffer; 1],
) {
    midi[0].clear();
    let mut left = [0.];
    let mut right = [0.];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 1,
                inputs: &[&[0.], &[0.]],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: input,
                outputs: midi,
            },
        )
        .unwrap();
    assert_eq!(left, [0.]);
    assert_eq!(right, [0.]);
}
fn main() {
    let config = Config {
        max_frames: 8,
        max_nodes: 4,
        max_ports: 24,
        max_edges: 16,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 256,
        midi_bytes_per_port: 1024,
        resource_slots: 16,
        resource_pages: 256,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
        budgets: Budgets {
            commands: 8,
            events: 8,
            subranges: 0,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 8,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 4,
            checkpoints_per_version: 4,
            replay_events: 128,
        }),
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 2,
            staging_bytes_per_group: 4096,
        },
    )
    .unwrap();
    let target = LoopId::from_parts(0, 1).unwrap();
    let playback = NodeId::from_parts(0, 1).unwrap();
    let capture = NodeId::from_parts(1, 1).unwrap();
    let effect = NodeId::from_parts(2, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 5,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let mut graph = GraphDescription {
        nodes: vec![playback, effect, capture],
        connections: Vec::new(),
    };
    for port in 0..2 {
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Audio, port),
                destination: Endpoint::node(effect, PortType::Audio, port),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Audio, port),
                destination: Endpoint::node(effect, PortType::Audio, port + 2),
            },
            Connection {
                source: Endpoint::node(effect, PortType::Audio, port),
                destination: Endpoint::node(capture, PortType::Audio, port),
            },
            Connection {
                source: Endpoint::node(effect, PortType::Audio, port),
                destination: Endpoint::host(PortType::Audio, port),
            },
        ]);
    }
    graph.connections.extend([
        Connection {
            source: Endpoint::host(PortType::Midi, 0),
            destination: Endpoint::node(effect, PortType::Midi, 0),
        },
        Connection {
            source: Endpoint::node(playback, PortType::Midi, 0),
            destination: Endpoint::node(effect, PortType::Midi, 1),
        },
        Connection {
            source: Endpoint::node(effect, PortType::Midi, 0),
            destination: Endpoint::node(capture, PortType::Midi, 0),
        },
        Connection {
            source: Endpoint::node(effect, PortType::Midi, 0),
            destination: Endpoint::host(PortType::Midi, 0),
        },
    ]);
    // The loop library prepares its planner, nodes and capture resources; the
    // application owns the store, the node type and the runtime.
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 6,
            export_slots: 2,
        },
        vec![Binding {
            target,
            playback,
            capture,
            channels: 2,
            midi: true,
            gain: 1.,
            smoothing: Smoothing::None,
            midi_events: 128,
            midi_bytes: 512,
            packet_bytes: 512,
        }],
        vec![],
        Options::default(),
    )
    .unwrap();
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, Node::Loop(node)))
        .chain([PreparedNode::new(effect, Node::Mix(Mix))])
        .collect();
    let (runtime_control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        planner,
        resources,
        &graph,
        &client.bindings_for(&graph),
    )
    .unwrap();
    let mut control = Controls {
        runtime: runtime_control,
        loops: client,
    };
    control.runtime.prepare_submissions().unwrap();
    // The held note at frame zero is observed while stopped, not prerecord audio.
    send(
        &mut control,
        1,
        Operation::Transition {
            target,
            timing: Timing::At,
            transition: Transition::Record {
                monitor_previous: false,
            },
        },
    );
    send(
        &mut control,
        4,
        Operation::Export {
            target,
            selection: Selection::Playback,
        },
    );
    send(
        &mut control,
        4,
        Operation::Transition {
            target,
            timing: Timing::At,
            transition: Transition::Play {
                position: 0,
                retrigger: false,
            },
        },
    );
    send(
        &mut control,
        8,
        Operation::Transition {
            target,
            timing: Timing::At,
            transition: Transition::Stop,
        },
    );
    let mut input = [MidiBuffer::new(2, 6).unwrap()];
    input[0].push(0, 8, &[0x90, 60, 100]).unwrap();
    input[0].push(2, 8, &[0x80, 60, 45]).unwrap();
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    let mut left = [0.; 8];
    let mut right = [0.; 8];
    runtime
        .process(
            AudioBlock {
                frame: Frame(0),
                frames: 8,
                inputs: &[
                    &[2., 4., 6., 8., 0., 0., 0., 0.],
                    &[20., 40., 60., 80., 0., 0., 0., 0.],
                ],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
        .unwrap();
    assert_eq!(left, [1., 2., 3., 4., 1., 1.5, 2., 1.]);
    assert_eq!(right, left.map(|sample| sample * 10.));
    while control.runtime.try_completion().is_some() {}
    let mut export = None;
    while let Some(execution) = control.runtime.try_execution() {
        assert_eq!(execution.result, Ok(()));
        if let Some(Reply::Export { id, cut, media }) = execution.operation_result {
            assert_eq!(cut.frame, Frame(4));
            assert_eq!(cut.state.mode, Mode::Playing);
            assert_eq!(media.view.description.frames, 3);
            export = Some((id, media.media));
        }
    }
    let (export, original) = export.unwrap();
    input[0].clear();
    // Callback-end stop belongs to the next nonempty callback.
    silence(&mut runtime, &input, &mut output);
    assert_eq!(output[0].len(), 1);
    assert_eq!(output[0].get(0).unwrap(), (0, &[0x80, 60, 0][..]));
    while control.runtime.try_execution().is_some() {}
    // Record a distinct silent revision, play/seek it, then replace it with the
    // original immutable media kept alive by the export.
    for (at, transition) in [
        (
            9,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            12,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (
            16,
            Transition::Seek {
                position: 1,
                retrigger: false,
            },
        ),
        (24, Transition::Stop),
    ] {
        send(
            &mut control,
            at,
            Operation::Transition {
                target,
                timing: Timing::At,
                transition,
            },
        );
    }
    send(
        &mut control,
        18,
        Operation::Select {
            target,
            timing: Timing::At,
            media: original,
            preserve_position: false,
        },
    );
    for expected in [[0.; 8], [0., 1., 1.5, 2., 1., 1.5, 2., 0.]] {
        runtime
            .process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames: 8,
                    inputs: &[&[0.; 8], &[0.; 8]],
                    outputs: &mut [&mut left, &mut right],
                },
                MidiBlock {
                    inputs: &input,
                    outputs: &mut output,
                },
            )
            .unwrap();
        assert_eq!(left, expected);
        assert_eq!(right, expected.map(|sample| sample * 10.));
    }
    while control.runtime.try_completion().is_some() {}
    let mut transitions = 0;
    while let Some(execution) = control.runtime.try_execution() {
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("transition outcome")
        };
        assert!(matches!(
            completion.outcome,
            shoop_loops::Outcome::Applied { .. }
        ));
        transitions += 1;
    }
    assert_eq!(transitions, 5);
    send(
        &mut control,
        runtime.next_frame().0,
        Operation::MidiInitial { export, channel: 0 },
    );
    silence(&mut runtime, &input, &mut output);
    while control.runtime.try_completion().is_some() {}
    let Some(Reply::MidiInitial { state, .. }) =
        control.runtime.try_execution().unwrap().operation_result
    else {
        panic!("initial state")
    };
    assert!(state.note(60).unwrap().held());
    assert_eq!(state.note(60).unwrap().velocity(), 100);
    for midi in [false, true] {
        send(
            &mut control,
            runtime.next_frame().0,
            Operation::Cursor { export, midi },
        );
        silence(&mut runtime, &input, &mut output);
        while control.runtime.try_completion().is_some() {}
        let Some(Reply::Cursor(cursor)) = control.runtime.try_execution().unwrap().operation_result
        else {
            panic!("cursor")
        };
        control
            .runtime
            .submit_prepared(
                Lane::Normal,
                Command::resource(Request::Read {
                    cursor,
                    bytes: vec![0; 64].into_boxed_slice(),
                }),
            )
            .unwrap_or_else(|e| panic!("read: {:?}", e.error));
        silence(&mut runtime, &input, &mut output);
        let Command::Resource(command) = control.runtime.try_completion().unwrap().request else {
            panic!("resource return")
        };
        let Some(rt_runtime::resource::Reply::Read { written, eof: true }) = command.reply() else {
            panic!("complete read")
        };
        let rt_runtime::resource::Request::Read { bytes, .. } = command.into_request() else {
            panic!("owning buffer")
        };
        let expected = if midi {
            [
                1u64.to_le_bytes().as_slice(),
                3u32.to_le_bytes().as_slice(),
                &[0x80, 60, 45],
            ]
            .concat()
        } else {
            [2f32, 20., 3., 30., 4., 40.]
                .into_iter()
                .flat_map(f32::to_le_bytes)
                .collect()
        };
        assert_eq!(&bytes[..written], expected);
    }
    send(
        &mut control,
        runtime.next_frame().0,
        Operation::Release { export },
    );
    silence(&mut runtime, &input, &mut output);
    while control.runtime.try_completion().is_some() {}
    assert_eq!(
        control.runtime.try_execution().unwrap().operation_result,
        Some(Reply::Done)
    );
    drop(control);
    // After callbacks stop, the planner's ledger returns its owners to the store.
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    for _ in 0..512 {
        resources.begin_callback();
        resources.reclaim();
    }
    assert_eq!(resources.usage().free_pages, 256);
    assert_eq!(resources.usage().free_versions, 16);
    assert_eq!(resources.usage().free_snapshots, 16);
    assert_eq!(planner.media_usage().references, 0);
    println!("Stereo/MIDI record→play/FX/export verified; initial held note preserved; all owners recovered.");
}
