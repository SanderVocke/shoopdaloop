//! Book example, chapters 10-11: a 4-frame bar loop is the shared sync source; a composite arranger runs a program that plays a 1-bar bass loop and a 2-bar drums loop on bar boundaries. Run with `cargo run --bin program_loop` from this directory.

use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{CaptureGroupConfig, Format, ResourceStore},
    transport::{Lane, Outcome as Admission},
    Config, Frame, NodeId, PlannerId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{Binding, LoopPlanner, Operation, Options, Prepared, Reply, Settings},
    media, Entry, Limits, LoopId, LoopKind, LoopSpec, Outcome, ProgramDescription, Section, Timing,
    TopologyDescription, Transition,
};

// Upload interleaved little-endian f32 samples as a new resource revision and return a source handle for importing it as a loop's initial content.
fn load_stereo(
    resources: &mut ResourceStore,
    left: &[f32],
    right: &[f32],
) -> media::Source {
    let mut bytes = Vec::new();
    for (l, r) in left.iter().zip(right.iter()) {
        bytes.extend_from_slice(&l.to_le_bytes());
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    let resource = resources.create().unwrap();
    let description = rt_runtime::resource::Description {
        format: Format::Audio { channels: 2 },
        frames: left.len() as u64,
        bytes: bytes.len(),
    };
    let upload = resources
        .begin_upload(resource, Revision(0), description)
        .unwrap();
    resources.begin_callback();
    resources.write_upload(upload, 0, &bytes).unwrap();
    resources.finish_upload(upload).unwrap();
    resources.commit(upload).unwrap();
    let snapshot = resources.snapshot(resource, Frame(0)).unwrap();
    let index = resources.read_index(snapshot.id).unwrap();
    media::Source { snapshot, index }
}

fn main() {
    let config = Config {
        max_frames: 16,
        audio_inputs: 6,
        audio_outputs: 6,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 6,
        max_ports: 24,
        max_edges: 12,
        resource_slots: 16,
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
            groups: 6,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )
    .unwrap();

    let bar = LoopId::from_parts(0, 1).unwrap();
    let arranger = LoopId::from_parts(1, 1).unwrap();
    let bass = LoopId::from_parts(2, 1).unwrap();
    let drums = LoopId::from_parts(3, 1).unwrap();
    // One bar loop (4 frames) is the sync source for the arranger AND both children; lengths are whole bars.
    let topology_result = compile(
        Limits {
            loop_slots: 4,
            pending_requests: 6,
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
                    id: arranger,
                    kind: LoopKind::Composite,
                    length: 0,
                    sync_source: Some(bar),
                },
                LoopSpec {
                    id: bass,
                    kind: LoopKind::Primitive,
                    length: 4,
                    sync_source: Some(bar),
                },
                LoopSpec {
                    id: drums,
                    kind: LoopKind::Primitive,
                    length: 8,
                    sync_source: Some(bar),
                },
            ],
    // The program: 1 unit = 1 bar (nominal_unit_frames: 4). Bass plays for bar 1, drums for bars 2-3.
            programs: vec![ProgramDescription {
                source: arranger,
                nominal_unit_frames: 4,
                sections: vec![Section {
                    entries: vec![
                        Entry {
                            target: bass,
                            delay: 0,
                            duration: Some(1),
                            repetitions: 1,
                            mode: None,
                        },
                        Entry {
                            target: drums,
                            delay: 1,
                            duration: Some(2),
                            repetitions: 1,
                            mode: None,
                        },
                    ],
                }],
            }],
            ..TopologyDescription::default()
        },
    );
    let topology = match topology_result {
        Ok(value) => value,
        Err(error) => panic!("compile: {:?}", error),
    };
    let node = |slot: u32| NodeId::from_parts(slot, 1).unwrap();
    let binding = |target, playback, capture, channels| Binding {
        target,
        playback,
        capture,
        channels,
        midi: false,
        gain: 1.0,
        smoothing: Smoothing::None,
        midi_events: 0,
        midi_bytes: 0,
        packet_bytes: 0,
    };
    let bar_playback = node(0);
    let bar_capture = node(1);
    let bass_playback = node(2);
    let bass_capture = node(3);
    let drums_playback = node(4);
    let drums_capture = node(5);
    let mut graph = GraphDescription {
        nodes: vec![
            bar_playback,
            bar_capture,
            bass_playback,
            bass_capture,
            drums_playback,
            drums_capture,
        ],
        connections: Vec::new(),
    };
    for (host_base, playback, capture) in [
        (0u32, bar_playback, bar_capture),
        (2, bass_playback, bass_capture),
        (4, drums_playback, drums_capture),
    ] {
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

    // Import content for the children instead of recording: bass is 1 bar, drums 2 bars.
    let bass_audio = load_stereo(
        &mut resources,
        &[1.0, 2.0, 3.0, 4.0],
        &[10.0, 20.0, 30.0, 40.0],
    );
    let drums_audio = load_stereo(
        &mut resources,
        &[5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0],
        &[50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0],
    );
    let imports = vec![
        media::Input {
            description: media::Description {
                owner: bass,
                revision: Revision(0),
                frames: 4,
                channels: 2,
                midi: false,
            },
            audio: Some(bass_audio),
            midi: None,
        },
        media::Input {
            description: media::Description {
                owner: drums,
                revision: Revision(0),
                frames: 8,
                channels: 2,
                midi: false,
            },
            audio: Some(drums_audio),
            midi: None,
        },
    ];
    let prepared = LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 5,
            media_slots: 16,
            export_slots: 0,
        },
        vec![
            binding(bar, bar_playback, bar_capture, 2),
            binding(bass, bass_playback, bass_capture, 2),
            binding(drums, drums_playback, drums_capture, 2),
        ],
        imports,
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

    // Record the bar loop, play it from frame 4, start the arranger, stop both at 12.
    let mut sequences = Vec::new();
    let submit = |at: u64,
                      target: LoopId,
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
                timing: Timing::At,
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
        arranger,
        Transition::Play {
            position: 0,
            retrigger: true,
        },
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        12,
        bar,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );
    submit(
        12,
        arranger,
        Transition::Stop,
        &mut control,
        &client,
        &mut sequences,
    );

    // One 16-frame block; the bar loop records frames 0-3.
    let bar_in = [1.0, 1.0, 1.0, 1.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0, 9.0];
    let silent2 = [0.0; 16];
    let bass_in = silent2;
    let drums_in = silent2;
    let mut bar_left_out = [0.0; 16];
    let mut bar_right_out = [0.0; 16];
    let mut bass_left_out = [0.0; 16];
    let mut bass_right_out = [0.0; 16];
    let mut drums_left_out = [0.0; 16];
    let mut drums_right_out = [0.0; 16];
    runtime
        .process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: 16,
                inputs: &[&bar_in, &bar_in, &bass_in, &bass_in, &drums_in, &drums_in],
                outputs: &mut [
                    &mut bar_left_out,
                    &mut bar_right_out,
                    &mut bass_left_out,
                    &mut bass_right_out,
                    &mut drums_left_out,
                    &mut drums_right_out,
                ],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
        .unwrap();
    // Bar repeats from frame 4; bass plays its bar, then drums play theirs — all on bar boundaries.
    let expected_bar_left: [f32; 16] = {
        let mut v = [0.0; 16];
        for f in v.iter_mut().take(12).skip(4) {
            *f = 1.0;
        }
        v
    };
    assert_eq!(bar_left_out, expected_bar_left);
    assert_eq!(bar_right_out, expected_bar_left);
    assert_eq!(
        bass_left_out,
        [1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        bass_right_out,
        [10.0, 20.0, 30.0, 40.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        drums_left_out,
        [0.0, 0.0, 0.0, 0.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        drums_right_out,
        [0.0, 0.0, 0.0, 0.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0, 110.0, 120.0, 0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(runtime.next_frame(), Frame(16));

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
    for &(sequence, frame) in &sequences {
        assert!(
            applied.contains(&(sequence, frame, Outcome::Applied { frame })),
            "missing applied transition for sequence {:?} at {:?}",
            sequence,
            frame
        );
    }

    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    let usage = resources.usage();
    assert_eq!(usage.free_pages, 46);
    assert_eq!(usage.free_versions, 14);
    assert_eq!(usage.free_snapshots, 16);
    assert_eq!(usage.reserved_pages, 0);
    assert_eq!(planner.media_usage().references, 0);

    println!("Bar loop ticked each 4 frames; arranger program played the 1-bar bass at bar 1 and the 2-bar drums at bars 2-3; stop and cleanup verified.");
}
