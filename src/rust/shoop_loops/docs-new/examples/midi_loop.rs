use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{CaptureGroupConfig, MidiIndexConfig, ResourceStore},
    transport::{Lane, Outcome as Admission},
    Config, Frame, NodeId, PlannerId,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{Binding, LoopPlanner, Operation, Options, Prepared, Reply, Settings},
    Limits, LoopId, LoopKind, LoopSpec, Outcome, Timing, TopologyDescription, Transition,
};

fn main() {
    let config = Config {
        max_frames: 8,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 1,
        midi_outputs: 1,
        midi_events_per_port: 256,
        midi_bytes_per_port: 1024,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 2,
        resource_slots: 16,
        resource_pages: 32,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
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
            members_per_group: 1,
            staging_bytes_per_group: 8192,
        },
    )
    .unwrap();

    let target = LoopId::from_parts(0, 1).unwrap();
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

    let playback = NodeId::from_parts(0, 1).unwrap();
    let capture = NodeId::from_parts(1, 1).unwrap();
    let binding = Binding {
        target,
        playback,
        capture,
        channels: 0,
        midi: true,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 128,
        midi_bytes: 512,
        packet_bytes: 512,
    };
    let graph = GraphDescription {
        nodes: vec![playback, capture],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Midi, 0),
                destination: Endpoint::node(capture, PortType::Midi, 0),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Midi, 0),
                destination: Endpoint::host(PortType::Midi, 0),
            },
        ],
    };

    let prepared = LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 4,
            export_slots: 0,
        },
        vec![binding],
        vec![],
        Options::default(),
    );
    let Prepared {
        planner,
        nodes,
        client,
    } = match prepared {
        Ok(value) => value,
        Err(failure) => panic!("prepare: {:?}", failure),
    };
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, node))
        .collect();
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        planner,
        resources,
        &graph,
        &client.bindings_for(&graph),
    )
    .unwrap();
    control.prepare_submissions().unwrap();
    assert_eq!(runtime.next_frame(), Frame(0));

    let mut sequences = Vec::new();
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (7, Transition::Stop),
    ] {
        let command = client.command(
            Frame(at),
            Operation::Transition {
                target,
                timing: Timing::At,
                transition,
            },
        );
        let ticket = control
            .submit_prepared(Lane::Normal, command)
            .unwrap_or_else(|refusal| panic!("submission: {:?}", refusal.error));
        sequences.push((ticket.sequence(), Frame(at)));
    }

    let mut input = [MidiBuffer::new(256, 1024).unwrap()];
    input[0].push(0, 8, &[0x90, 60, 100]).unwrap();
    input[0].push(1, 8, &[0xB0, 64, 127]).unwrap();
    let mut output = [MidiBuffer::new(256, 1024).unwrap()];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
        .unwrap();
    assert_eq!(output[0].len(), 68);
    let events = (0..output[0].len())
        .map(|i| output[0].get(i).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events[events.len() - 4], (4, &[0x90, 60, 100][..]));
    assert_eq!(events[events.len() - 3], (5, &[0xB0, 64, 127][..]));
    assert_eq!(events[events.len() - 2], (7, &[0xB0, 64, 0][..]));
    assert_eq!(events[events.len() - 1], (7, &[0x80, 60, 0][..]));
    assert_eq!(runtime.next_frame(), Frame(8));

    input[0].clear();
    output[0].clear();
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut output,
            },
        )
        .unwrap();
    assert_eq!(output[0].len(), 0);
    assert_eq!(runtime.next_frame(), Frame(16));

    for &(sequence, _) in &sequences {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, sequence);
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
    assert!(control.try_completion().is_none());
    for &(sequence, frame) in &sequences {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.sequence, sequence);
        assert_eq!(execution.actual_frame, frame);
        assert_eq!(execution.result, Ok(()));
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("expected a transition result");
        };
        assert_eq!(completion.outcome, Outcome::Applied { frame });
    }
    assert!(control.try_execution().is_none());

    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    let usage = resources.usage();
    assert_eq!(usage.free_pages, 32);
    assert_eq!(usage.free_versions, 16);
    assert_eq!(usage.free_snapshots, 16);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(planner.media_usage().references, 0);

    println!("MIDI loop recorded two events, played them at frame 4, wrapped, and stopped; cleanup verified.");
}
