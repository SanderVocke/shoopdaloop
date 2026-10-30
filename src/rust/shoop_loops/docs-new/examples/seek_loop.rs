//! Book example, chapter 4: record six frames, play, seek to position 4 mid-playback, and stop. Run with `cargo run --bin seek_loop` from this directory.

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
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )
    .unwrap();

    let target = LoopId::from_parts(0, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 6,
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
        channels: 2,
        midi: false,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 0,
        midi_bytes: 0,
        packet_bytes: 0,
    };
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
            export_slots: 1,
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
    // Playback starts at 6; the seek at 9 skips position 3; stop at 12 silences the tail.
    assert_eq!(runtime.next_frame(), Frame(0));

    // Record 6 frames, play at 6, seek to position 4 at 9, stop at 12.
    let mut sequences = Vec::new();
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            6,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (
            9,
            Transition::Seek {
                position: 4,
                retrigger: false,
            },
        ),
        (12, Transition::Stop),
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

    let export_ticket = control
        .submit_prepared(
            Lane::Normal,
            client.command(
                Frame(10),
                Operation::Export {
                    target,
                    selection: shoop_loops::runtime::Selection::Latest,
                },
            ),
        )
        .unwrap_or_else(|refusal| panic!("export: {:?}", refusal.error));
    let export_sequence = export_ticket.sequence();

    // One 16-frame block; frames 0-5 are the recording, later values differ on purpose.
    let left_in = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 16];
    let mut right_out = [0.0; 16];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 16,
                inputs: &[&left_in, &right_in],
                outputs: &mut [&mut left_out, &mut right_out],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    assert_eq!(
        left_out,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 5.0, 6.0, 1.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        right_out,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 50.0, 60.0, 10.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(runtime.next_frame(), Frame(16));

    let mut admitted = Vec::new();
    while let Some(completion) = control.try_completion() {
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
        admitted.push(completion.sequence);
    }
    for &(sequence, _) in &sequences {
        assert!(admitted.contains(&sequence));
    }
    assert!(admitted.contains(&export_sequence));
    let mut transitions = Vec::new();
    let mut exported = None;
    while let Some(execution) = control.try_execution() {
        assert_eq!(execution.result, Ok(()));
        if execution.sequence == export_sequence {
            let Some(Reply::Export { id, cut, media }) = execution.operation_result else {
                panic!("expected export");
            };
            assert_eq!(cut.frame, Frame(10));
            assert_eq!(media.view.description.frames, 6);
            exported = Some((id, media.media));
        } else {
            let Some(Reply::Transition(completion)) = execution.operation_result else {
                panic!("expected a transition result");
            };
            transitions.push((execution.sequence, execution.actual_frame, completion.outcome));
        }
    }
    for &(sequence, frame) in &sequences {
        assert!(
            transitions.contains(&(sequence, frame, Outcome::Applied { frame })),
            "missing transition {:?} at {:?}",
            sequence,
            frame
        );
    }
    let (export_id, media_id) = exported.expect("export reply");
    println!("exported media {:?} as export {:?}", media_id, export_id);

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

    println!("Recorded six frames; play, seek to position 4, wrap, stop and cleanup verified.");
}
