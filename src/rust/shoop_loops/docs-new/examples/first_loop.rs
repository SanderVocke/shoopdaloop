//! Book example, chapter 3: prepare one stereo loop, record three frames, play them immediately, observe the wrap, stop, and reclaim storage. Run with `cargo run --bin first_loop` from this directory.

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
    // 1. Reserve runtime storage: host port counts, graph capacity, resource pools.
    let config = Config {
        max_frames: 8,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 4,
        resource_slots: 8,
        resource_pages: 16,
        page_bytes: 64,
        command_slots: 4,
        scheduled_events: 4,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    // Capture storage for the loop's takes: two banks, one stereo member each.
    let mut resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 64,
        },
    )
    .unwrap();

    // 2. Describe the musical loop: one empty primitive loop, no sync source.
    let target = LoopId::from_parts(0, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 4,
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
    // 3. Bind the loop to its two signal nodes (playback outputs, capture inputs).
    let binding = Binding {
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
    // 4. Connect the signal graph: host in -> capture, playback -> host out.
    let mut graph = GraphDescription {
        nodes: vec![playback, capture],
        connections: Vec::new(),
    };
    for channel in 0..2 {
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Audio, channel),
                destination: Endpoint::node(capture, PortType::Audio, channel),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Audio, channel),
                destination: Endpoint::host(PortType::Audio, channel),
            },
        ]);
    }

    // 5. Prepare the loop parts: planner, nodes, and control-side client.
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
            normal_requests: 3,
            media_slots: 4,
            export_slots: 0,
        },
        vec![binding],
        vec![],
        Options::default(),
    )
    .unwrap();
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, node))
        .collect();
    // 6. Install everything in a runtime; the control endpoint takes submissions.
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

    // Submit Record at 0, Play at 3, Stop at 7 — all before the first callback.
    let mut sequences = Vec::new();
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            3,
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

    // Process one 8-frame host block; the first three frames are the recording.
    let left_in = [1.0, 2.0, 3.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 8];
    let mut right_out = [0.0; 8];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 8,
                inputs: &[&left_in, &right_in],
                outputs: &mut [&mut left_out, &mut right_out],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    // Frames 0-2 silent (no live route), 3-5 play the take, 6 wraps, 7 stops.
    assert_eq!(left_out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 1.0, 0.0]);
    assert_eq!(right_out, [0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 10.0, 0.0]);
    assert_eq!(runtime.next_frame(), Frame(8));

    // Admission results: the runtime accepted each request for delivery.
    for &(sequence, _) in &sequences {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, sequence);
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
    assert!(control.try_completion().is_none());
    // Execution results: the musical outcome at each actual boundary.
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

    // Shutdown: extract planner and store after callbacks stop, then reclaim.
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

    println!("Recorded three stereo frames; immediate playback, wrap, stop and cleanup verified.");
}
