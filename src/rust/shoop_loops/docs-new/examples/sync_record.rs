//! Book example, chapter 7: record exactly four bars by starting Record on the bar loop's first tick and Stop on its fifth. Run with `cargo run --bin sync_record` from this directory.

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
        max_frames: 32,
        audio_inputs: 4,
        audio_outputs: 4,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 16,
        max_edges: 8,
        resource_slots: 8,
        resource_pages: 48,
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
            staging_bytes_per_group: 256,
        },
    )
    .unwrap();

    let bar = LoopId::from_parts(0, 1).unwrap();
    let take = LoopId::from_parts(1, 1).unwrap();
    let topology = compile(
        Limits {
            loop_slots: 2,
            pending_requests: 7,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![
                LoopSpec {
                    id: bar,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                },
                LoopSpec {
                    id: take,
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: Some(bar),
                },
            ],
            ..TopologyDescription::default()
        },
    )
    .unwrap();

    let bar_playback = NodeId::from_parts(0, 1).unwrap();
    let bar_capture = NodeId::from_parts(1, 1).unwrap();
    let take_playback = NodeId::from_parts(2, 1).unwrap();
    let take_capture = NodeId::from_parts(3, 1).unwrap();
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
        nodes: vec![bar_playback, bar_capture, take_playback, take_capture],
        connections: Vec::new(),
    };
    for (host_base, playback, capture) in
        [(0u32, bar_playback, bar_capture), (2, take_playback, take_capture)]
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
            normal_requests: 6,
            media_slots: 4,
            export_slots: 0,
        },
        vec![
            binding(bar, bar_playback, bar_capture),
            binding(take, take_playback, take_capture),
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

    // The take loop records between the bar loop's first and fifth ticks: exactly 4 bars.
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
        bar,
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
        bar,
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
        take,
        Timing::AfterTicks {
            source: bar,
            skip: 0,
        },
        Transition::Record {
            monitor_previous: false,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        0,
        take,
        Timing::AfterTicks {
            source: bar,
            skip: 4,
        },
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        26,
        take,
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
        30,
        bar,
        Timing::At,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        31,
        take,
        Timing::At,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );

    let bar_left_in = [1.0, 1.0, 1.0, 1.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let bar_right_in = [2.0, 2.0, 2.0, 2.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let take_left_in = [0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 21.0, 22.0, 23.0, 24.0, 25.0, 26.0, 27.0, 28.0, 29.0, 30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 36.0];
    let take_right_in = [0.0, 0.0, 0.0, 0.0, 50.0, 60.0, 70.0, 80.0, 110.0, 120.0, 130.0, 140.0, 150.0, 160.0, 170.0, 180.0, 210.0, 220.0, 230.0, 240.0, 250.0, 260.0, 270.0, 280.0, 290.0, 300.0, 310.0, 320.0, 330.0, 340.0, 350.0, 360.0];
    let mut bar_left_out = [0.0; 32];
    let mut bar_right_out = [0.0; 32];
    let mut take_left_out = [0.0; 32];
    let mut take_right_out = [0.0; 32];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 32,
                inputs: &[&bar_left_in, &bar_right_in, &take_left_in, &take_right_in],
                outputs: &mut [
                    &mut bar_left_out,
                    &mut bar_right_out,
                    &mut take_left_out,
                    &mut take_right_out,
                ],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    assert_eq!(runtime.next_frame(), Frame(32));

    for &(sequence, _) in &sequences {
        let completion = control.try_completion().unwrap();
        assert_eq!(completion.sequence, sequence);
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
    }
    assert!(control.try_completion().is_none());
    let mut applied = Vec::new();
    while let Some(execution) = control.try_execution() {
        assert_eq!(execution.result, Ok(()));
        let Some(Reply::Transition(completion)) = execution.operation_result else {
            panic!("expected a transition result");
        };
        applied.push((execution.sequence, execution.actual_frame, completion.outcome));
    }
    let expected_bar_left: [f32; 32] = {
        let mut v = [0.0; 32];
        for f in v.iter_mut().take(30).skip(4) {
            *f = 1.0;
        }
        v
    };
    assert_eq!(bar_left_out, expected_bar_left);
    let expected_bar_right: [f32; 32] = {
        let mut v = [0.0; 32];
        for f in v.iter_mut().take(30).skip(4) {
            *f = 2.0;
        }
        v
    };
    assert_eq!(bar_right_out, expected_bar_right);
    let expected_take_left: [f32; 32] = {
        let mut v = [0.0; 32];
        v[30] = 5.0;
        v
    };
    assert_eq!(take_left_out, expected_take_left);
    let expected_take_right: [f32; 32] = {
        let mut v = [0.0; 32];
        v[30] = 50.0;
        v
    };
    assert_eq!(take_right_out, expected_take_right);
    let record_start = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[2].0)
        .copied();
    let record_stop = applied
        .iter()
        .find(|(sequence, _, _)| *sequence == sequences[3].0)
        .copied();
    // Assert the tick-scheduled Record and Stop landed at the actual tick frames.
    assert_eq!(
        record_start,
        Some((sequences[2].0, Frame(0), Outcome::Applied { frame: Frame(0) }))
    );
    assert_eq!(
        record_stop,
        Some((sequences[3].0, Frame(16), Outcome::Applied { frame: Frame(16) }))
    );

    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    let usage = resources.usage();
    assert_eq!(usage.free_pages, 48);
    assert_eq!(usage.free_versions, 8);
    assert_eq!(usage.free_snapshots, 8);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(planner.media_usage().references, 0);

    println!("Bar loop wrapped at 8/12/16/20; take recorded 4 bars (frames 0-16) between sync ticks, then played back; cleanup verified.");
}
