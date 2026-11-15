//! The real runtime, planner, and media store execute both sample policies.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortType},
    resource::{CaptureGroupConfig, ResourceStore},
    transport::Lane,
    Config, Frame, NodeId, PlannerId, Quantum, Sample, Q824,
};
use shoop_loops::{audio::Smoothing, runtime::*, *};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};
#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn session<S: Sample>(input: [S; 64], gain: S) -> (Vec<S>, Vec<S>) {
    let q = Quantum::new(32).unwrap();
    let config = Config {
        max_frames: 32,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        page_bytes: 128,
        resource_pages: 64,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::<S>::with_capture_groups(
        config,
        q,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 128,
        },
    )
    .unwrap();
    let target = LoopId::from_parts(0, 1).unwrap();
    let playback = NodeId::from_parts(0, 1).unwrap();
    let capture = NodeId::from_parts(1, 1).unwrap();
    let topology = compile(
        Limits::default(),
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: target,
                kind: LoopKind::Primitive,
                length: 0,
                sync_source: None,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        q,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 8,
            media_slots: 8,
            export_slots: 2,
        },
        vec![Binding {
            target,
            playback,
            capture,
            channels: 1,
            auxiliary_channels: 0,
            dry_wet: false,
            midi: false,
            gain,
            smoothing: Smoothing::None,
            midi_events: 1,
            midi_bytes: 4,
            packet_bytes: 4,
        }],
        vec![],
        Options::default(),
    )
    .unwrap();
    let graph = GraphDescription {
        nodes: vec![capture, playback],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(capture, PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(playback, PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
        ],
    };
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, node))
        .collect();
    let (mut control, mut runtime) =
        GraphRuntime::prepare_with_initial_graph(config, q, nodes, planner, resources, &graph)
            .unwrap();
    control.prepare_submissions().unwrap();
    for (frame, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (64, Transition::Stop),
        (
            96,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
    ] {
        control
            .submit_prepared(
                Lane::Normal,
                client.command(
                    Frame(frame),
                    Operation::Transition {
                        target,
                        timing: Timing::At,
                        transition,
                    },
                ),
            )
            .map_err(|e| e.error)
            .unwrap();
    }
    let mut rendered = Vec::new();
    for quantum in 0..8 {
        if quantum == 3 {
            control
                .submit_prepared(
                    Lane::Normal,
                    client.command(
                        Frame(96),
                        Operation::Export {
                            target,
                            selection: Selection::Playback,
                        },
                    ),
                )
                .map_err(|e| e.error)
                .unwrap();
        }
        let source = if quantum < 2 {
            &input[quantum * 32..quantum * 32 + 32]
        } else {
            &[S::ZERO; 32]
        };
        let mut output = [S::ZERO; 32];
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(quantum as u64 * 32),
                    frames: 32,
                    inputs: &[source],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        rendered.extend(output);
        while control.try_completion().is_some() {}
    }
    let mut source = None;
    while let Some(execution) = control.try_execution() {
        assert_eq!(execution.result, Ok(()));
        if let Some(Reply::Export { media, .. }) = execution.operation_result {
            source = media.view.audio;
        }
    }
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    let mut cursor = resources
        .render_io()
        .cursor_at(source.unwrap().index, 0)
        .unwrap();
    let mut bytes = [0; 256];
    let mut copied = 0;
    while copied < bytes.len() {
        let count = resources
            .render_io()
            .read(&mut cursor, &mut bytes[copied..])
            .unwrap();
        assert!(count > 0);
        copied += count;
    }
    let stored = bytes
        .chunks_exact(4)
        .map(|b| S::from_le_bytes(b.try_into().unwrap()))
        .collect();
    planner.cleanup(&mut resources).unwrap();
    (rendered, stored)
}

#[shoop_test]
fn production_capture_playback_and_wrap_keep_raw_fixed_bits_without_float_roundtrips() {
    let input = core::array::from_fn(|i| {
        Q824(if i % 2 == 0 {
            16_777_217 + i as i32
        } else {
            -16_777_219 - i as i32
        })
    });
    let gain = Q824(-8_388_609);
    let (fixed, stored) = session(input, gain);
    assert_eq!(stored, input);
    assert_eq!(&fixed[..96], &[Q824::ZERO; 96]);
    for (i, &actual) in fixed[96..].iter().enumerate() {
        assert_eq!(actual, input[i % 64].multiply(gain));
    }
    let floats = input.map(Sample::to_f32);
    let (floating, stored_float) = session(floats, gain.to_f32());
    assert_eq!(stored_float, floats);
    let max_error = fixed
        .iter()
        .zip(&floating)
        .map(|(q, f)| (q.to_f32() - f).abs())
        .fold(0f32, f32::max);
    // Input/gain f32 conversion and one product each contribute less than 2^-22
    // here (magnitudes <= 2); leave a 2^-21 bound including final conversion.
    assert!(
        max_error <= 2f32.powi(-21),
        "maximum float/fixed error {max_error}"
    );
}
