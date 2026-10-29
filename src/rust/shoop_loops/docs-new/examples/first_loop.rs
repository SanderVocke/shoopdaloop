use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    processor::Processor,
    resource::{CaptureGroupConfig, ResourceStore},
    transport::{Lane, Outcome as Admission},
    Config, Frame, NodeId, PlannerId,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{Binding, Engine, LoopPlanner, Operation, Reply, Settings},
    Limits, LoopId, LoopKind, LoopSpec, Outcome, Timing, TopologyDescription, Transition,
};

fn main() {
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
    let resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 64,
        },
    )
    .unwrap();

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

    let (mut control, mut engine) = Engine::prepare(
        config,
        topology,
        resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 3,
            media_slots: 4,
            export_slots: 0,
        },
        vec![binding],
        vec![],
        Vec::<(NodeId, Box<dyn Processor<LoopPlanner>>)>::new(),
        graph,
    )
    .unwrap();
    assert_eq!(engine.next_frame(), Frame(0));

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
        let ticket = control
            .submit(
                Lane::Normal,
                Frame(at),
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition,
                },
            )
            .unwrap_or_else(|refusal| panic!("submission: {:?}", refusal.error));
        sequences.push((ticket.sequence(), Frame(at)));
    }

    let left_in = [1.0, 2.0, 3.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let right_in = [10.0, 20.0, 30.0, 90.0, 90.0, 90.0, 90.0, 90.0];
    let mut left_out = [0.0; 8];
    let mut right_out = [0.0; 8];
    engine
        .process(
            AudioBlock {
                frame: engine.next_frame(),
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
    assert_eq!(left_out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 1.0, 0.0]);
    assert_eq!(right_out, [0.0, 0.0, 0.0, 10.0, 20.0, 30.0, 10.0, 0.0]);
    assert_eq!(engine.next_frame(), Frame(8));

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
    let mut stopped = engine.into_stopped();
    stopped.cleanup().unwrap();
    stopped.resources().begin_callback();
    stopped.resources().reclaim();
    let usage = stopped.resource_usage();
    assert_eq!(usage.free_pages, 16);
    assert_eq!(usage.free_versions, 8);
    assert_eq!(usage.free_snapshots, 8);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(stopped.media_usage().references, 0);

    println!("Recorded three stereo frames; immediate playback, wrap, stop and cleanup verified.");
}
