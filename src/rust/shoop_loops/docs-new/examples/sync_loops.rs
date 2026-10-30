//! Book example, chapter 6: a 4-frame bar loop ticks a synced child loop, whose Play is scheduled on the bar's first tick instead of an absolute frame. Run with `cargo run --bin sync_loops` from this directory.

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{CaptureGroupConfig, ResourceStore},
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
        max_frames: 16,
        audio_inputs: 4,
        audio_outputs: 4,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 8,
        resource_slots: 8,
        resource_pages: 16,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: 8,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 4,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )
    .unwrap();

    let parent = LoopId::from_parts(0, 1).unwrap();
    let child = LoopId::from_parts(1, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 2,
            pending_requests: 6,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![
                LoopSpec {
                    id: parent,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
                LoopSpec {
                    id: child,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(parent),
                },
            ],
            ..TopologyDescription::default()
        },
    )
    .unwrap();

    let parent_playback = NodeId::from_parts(0, 1).unwrap();
    let parent_capture = NodeId::from_parts(1, 1).unwrap();
    let child_playback = NodeId::from_parts(2, 1).unwrap();
    let child_capture = NodeId::from_parts(3, 1).unwrap();
    let binding = |target, playback, capture| Binding {
        target,
        playback,
        capture,
        channels: 2,
        midi: false,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 0,
        midi_bytes: 0,
        packet_bytes: 0,
    };
    let mut graph = GraphDescription {
        nodes: vec![parent_playback, parent_capture, child_playback, child_capture],
        connections: Vec::new(),
    };
    for (host_base, (playback, capture)) in
        [(0, (parent_playback, parent_capture)), (2, (child_playback, child_capture))]
    {
        for channel in 0..2 {
            graph.connections.extend([
                Connection {
                    source: Endpoint::host(PortType::Audio, host_base + channel),
                    destination: Endpoint::node(capture, PortType::Audio, channel),
                },
                Connection {
                    source: Endpoint::node(playback, PortType::Audio, channel),
                    destination: Endpoint::host(PortType::Audio, host_base + channel),
                },
            ]);
        }
    }

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
            normal_requests: 5,
            media_slots: 4,
            export_slots: 0,
        },
        vec![
            binding(parent, parent_playback, parent_capture),
            binding(child, child_playback, child_capture),
        ],
        vec![],
        Options::default(),
    )
    .unwrap();
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

    // The child's Play uses Timing::AfterTicks, so it fires on the bar loop's first tick, not at a named frame.
    let mut sequences = Vec::new();
    let submit = |at: u64,
                      target: LoopId,
                      timing: Timing,
                      transition: Transition,
                      control: &mut rt_runtime::executor::Control<
        shoop_loops::runtime::LoopNode,
        shoop_loops::runtime::LoopPlanner,
    >,
                      client: &shoop_loops::runtime::LoopClient,
                      sequences: &mut Vec<(rt_runtime::Sequence, Frame)>| {
        let command = client.command(
            Frame(at),
            Operation::Transition {
                target,
                timing,
                transition,
            },
        );
        let ticket = control
            .submit_prepared(Lane::Normal, command)
            .unwrap_or_else(|refusal| panic!("submission: {:?}", refusal.error));
        sequences.push((ticket.sequence(), Frame(at)));
    };
    submit(
        0,
        parent,
        Timing::At,
        Transition::Record {
            monitor_previous: false,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        4,
        parent,
        Timing::At,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        0,
        child,
        Timing::At,
        Transition::Record {
            monitor_previous: false,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        4,
        child,
        Timing::AfterTicks {
            source: parent,
            skip: 0,
        },
        Transition::Play {
            position: 0,
            retrigger: false,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        14,
        parent,
        Timing::At,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        14,
        child,
        Timing::At,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );

    let parent_left_in = [1.0, 2.0, 3.0, 4.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let parent_right_in = [10.0, 20.0, 30.0, 40.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let child_left_in = [5.0, 6.0, 7.0, 8.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let child_right_in = [50.0, 60.0, 70.0, 80.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut parent_left_out = [0.0; 16];
    let mut parent_right_out = [0.0; 16];
    let mut child_left_out = [0.0; 16];
    let mut child_right_out = [0.0; 16];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 16,
                inputs: &[&parent_left_in, &parent_right_in, &child_left_in, &child_right_in],
                outputs: &mut [
                    &mut parent_left_out,
                    &mut parent_right_out,
                    &mut child_left_out,
                    &mut child_right_out,
                ],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    // The child starts at frame 4 (the bar's first tick) and wraps with its 4-frame take.
    assert_eq!(
        parent_left_out,
        [0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 3.0, 4.0, 1.0, 2.0, 0.0, 0.0]
    );
    assert_eq!(
        parent_right_out,
        [0.0, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 40.0, 10.0, 20.0, 30.0, 40.0, 10.0, 20.0, 0.0, 0.0]
    );
    assert_eq!(
        child_left_out,
        [0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 7.0, 8.0, 5.0, 6.0, 0.0, 0.0]
    );
    assert_eq!(
        child_right_out,
        [0.0, 0.0, 0.0, 0.0, 50.0, 60.0, 70.0, 80.0, 50.0, 60.0, 70.0, 80.0, 50.0, 60.0, 0.0, 0.0]
    );
    assert_eq!(runtime.next_frame(), Frame(16));

    for &(sequence, _) in &sequences {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, sequence);
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
    assert!(control.try_completion().is_none());
    let mut applied = Vec::new();
    for _ in &sequences {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.result, Ok(()));
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("expected a transition result");
        };
        applied.push((execution.sequence, execution.actual_frame, completion.outcome));
    }
    assert!(control.try_execution().is_none());
    // The execution stream reports the real boundary (frame 4), not the requested frame.
    let child_play = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[3].0)
        .copied();
    assert_eq!(
        child_play,
        Some((sequences[3].0, Frame(4), Outcome::Applied { frame: Frame(4) }))
    );

    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    let usage = resources.usage();
    assert_eq!(usage.free_pages, 16);
    assert_eq!(usage.free_versions, 8);
    assert_eq!(usage.free_snapshots, 8);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(planner.media_usage().references, 0);

    println!("Parent bar loop recorded and played; child started on the parent's first tick at frame 4; stop and cleanup verified.");
}
