#[path = "../../rt_runtime/tests/support/mod.rs"]
mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType},
    midi_state::MidiState,
    planner::ConsumerAccess,
    processor::{Cadence, ProcessBlock, Processor},
    resource::*,
    schedule::Action,
    transport::Lane,
    Config, Error, Frame, NodeId, PlannerId, Revision,
};
use shoop_loops::{
    audio::Smoothing,
    media,
    runtime::{
        callback::{self, CallbackNode, CallbackPlanner},
        Binding, Settings,
    },
    *,
};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
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
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
fn upload(
    store: &mut ResourceStore,
    format: Format,
    frames: u64,
    bytes: &[u8],
    midi: bool,
) -> media::Source {
    let resource = store.create().unwrap();
    let description = Description {
        format,
        frames,
        bytes: bytes.len(),
    };
    let token = if midi {
        store
            .begin_indexed_midi_upload(resource, Revision(0), description, &MidiState::default())
            .unwrap()
    } else {
        store
            .begin_upload(resource, Revision(0), description)
            .unwrap()
    };
    for (page, chunk) in bytes.chunks(256).enumerate() {
        store.begin_callback();
        store.write_upload(token, page * 256, chunk).unwrap();
    }
    store.finish_upload(token).unwrap();
    store.commit(token).unwrap();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    media::Source {
        snapshot,
        index: store.read_index(snapshot.id).unwrap(),
    }
}
struct Fx {
    calls: Arc<AtomicUsize>,
}
enum Node {
    Loop(CallbackNode),
    Fx(Fx),
}
impl Processor<CallbackPlanner> for Node {
    fn cadence(&self) -> Cadence {
        Cadence::Callback
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(p) => p.ports(),
            Self::Fx(_) => PortCounts {
                audio_inputs: 2,
                audio_outputs: 2,
                midi_inputs: 1,
                midi_outputs: 1,
            },
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(p) => p.planning(),
            Self::Fx(_) => ConsumerAccess::default(),
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, CallbackPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(p) => p.process(b),
            Self::Fx(p) => {
                p.calls.fetch_add(1, Ordering::Relaxed);
                for channel in 0..2 {
                    let (input, output) = b.audio.pair(channel, channel).unwrap();
                    for (input, output) in input.iter().zip(output) {
                        *output = *input * 3.0;
                    }
                }
                // Clock packets pass through; the algorithm produces one held note.
                for offset in 0..b.frames {
                    for event in 0..b.midi_inputs[0].len() {
                        let (at, data) = b.midi_inputs[0].get(event).unwrap();
                        if at as usize == offset && data == [0xf8] {
                            b.midi_outputs[0].push(at, b.frames, data)?;
                        }
                    }
                    match b.frame.0 + offset as u64 {
                        2 => b.midi_outputs[0].push(offset as u32, b.frames, &[0x90, 72, 91])?,
                        9 => b.midi_outputs[0].push(offset as u32, b.frames, &[0x80, 72, 64])?,
                        _ => {}
                    }
                }
                Ok(())
            }
        }
    }
}
type Runtime = GraphRuntime<Node, CallbackPlanner>;
type Control = rt_runtime::executor::Control<Node, CallbackPlanner>;
fn setup(span_limit: usize) -> (Control, Runtime, Arc<AtomicUsize>) {
    let config = Config {
        max_frames: 32,
        audio_inputs: 0,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 1,
        max_nodes: 3,
        max_edges: 12,
        max_ports: 16,
        midi_events_per_port: 4096,
        midi_bytes_per_port: 16384,
        resource_slots: 16,
        resource_pages: 128,
        page_bytes: 256,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        Some(MidiIndexConfig {
            versions: 8,
            checkpoints_per_version: 16,
            replay_events: 4096,
        }),
        CaptureGroupConfig {
            groups: 1,
            members_per_group: 2,
            staging_bytes_per_group: 32768,
        },
    )
    .unwrap();
    let audio: Vec<_> = (1..=4)
        .flat_map(|i| [i as f32, i as f32 * 10.0])
        .flat_map(f32::to_le_bytes)
        .collect();
    let audio = upload(
        &mut resources,
        Format::Audio { channels: 2 },
        4,
        &audio,
        false,
    );
    let mut packet = [0; 32];
    let length = encode_midi_record(1, &[0xf8], &mut packet).unwrap();
    let midi = upload(&mut resources, Format::Midi, 4, &packet[..length], true);
    let topology = compile(
        Limits {
            loop_slots: 1,
            pending_requests: 8,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: id(),
                kind: LoopKind::Primitive,
                length: 4,
                sync_source: Some(id()),
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let prepared = CallbackPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: planner(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 4,
        },
        vec![Binding {
            target: id(),
            playback: node(0),
            capture: node(2),
            channels: 2,
            midi: true,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 1024,
            midi_bytes: 8192,
            packet_bytes: 8192,
        }],
        vec![media::Input {
            description: media::Description {
                owner: id(),
                revision: Revision(0),
                frames: 4,
                channels: 2,
                midi: true,
            },
            audio: Some(audio),
            midi: Some(midi),
        }],
        span_limit,
    )
    .unwrap();
    let bindings = prepared.planner.bindings().to_vec();
    let calls = Arc::new(AtomicUsize::new(0));
    let mut nodes: Vec<_> = prepared
        .nodes
        .into_iter()
        .map(|(id, p)| PreparedNode::new(id, Node::Loop(p)))
        .collect();
    nodes.push(PreparedNode::new(
        node(1),
        Node::Fx(Fx {
            calls: calls.clone(),
        }),
    ));
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(1), node(2)],
        connections: Vec::new(),
    };
    for (kind, ports) in [(PortType::Audio, 2), (PortType::Midi, 1)] {
        for port in 0..ports {
            for (source, target) in [
                (node(0), Some(node(1))),
                (node(1), Some(node(2))),
                (node(1), None),
            ] {
                graph.connections.push(Connection {
                    source: Endpoint::node(source, kind, port),
                    destination: target.map_or(Endpoint::host(kind, port), |id| {
                        Endpoint::node(id, kind, port)
                    }),
                });
            }
        }
    }
    let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        prepared.planner,
        resources,
        &graph,
        &bindings,
    )
    .unwrap();
    (control, runtime, calls)
}
fn command(control: &mut Control, operation: callback::Operation) {
    assert!(control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(0),
                action: Action::Planner {
                    planner: planner(),
                    operation
                },
                payload: Box::new([])
            }
        )
        .is_ok());
}
fn render(
    runtime: &mut Runtime,
    frames: usize,
) -> Result<(Vec<f32>, Vec<f32>, Vec<(u64, Vec<u8>)>), Error> {
    let at = runtime.next_frame();
    let mut left = vec![0.0; frames];
    let mut right = vec![0.0; frames];
    let mut midi = [MidiBuffer::new(4096, 16384).unwrap()];
    support::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: at,
                frames,
                inputs: &[],
                outputs: &mut [&mut left, &mut right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut midi,
            },
        )
    })?;
    let midi = (0..midi[0].len())
        .map(|i| {
            let (offset, data) = midi[0].get(i).unwrap();
            (at.0 + offset as u64, data.to_vec())
        })
        .collect();
    Ok((left, right, midi))
}
#[shoop_test]
fn real_dry_fx_wet_capture_uses_shared_wrapping_timing_and_actual_midi_checkpoint() {
    for partitions in [vec![17], vec![3, 4, 1, 9], vec![1; 17]] {
        let (mut control, mut runtime, calls) = setup(32);
        command(
            &mut control,
            callback::Operation::Transition {
                target: id(),
                at: Frame(0),
                timing: Timing::At,
                transition: Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            },
        );
        command(
            &mut control,
            callback::Operation::Capture {
                target: id(),
                start: Frame(5),
                end: Frame(15),
            },
        );
        let mut audio = Vec::new();
        let mut midi = Vec::new();
        for frames in &partitions {
            let (left, right, events) = render(&mut runtime, *frames).unwrap();
            assert_eq!(
                right,
                left.iter().map(|sample| sample * 10.0).collect::<Vec<_>>()
            );
            audio.extend(left);
            midi.extend(events);
        }
        assert_eq!(
            audio,
            (0..17)
                .map(|frame| 3.0 * (frame % 4 + 1) as f32)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            midi,
            vec![
                (1, vec![0xf8]),
                (2, vec![0x90, 72, 91]),
                (5, vec![0xf8]),
                (9, vec![0xf8]),
                (9, vec![0x80, 72, 64]),
                (13, vec![0xf8])
            ]
        );
        assert_eq!(calls.load(Ordering::Relaxed), partitions.len());
        let replies: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
        assert_eq!(replies.len(), 2);
        let captured = replies
            .iter()
            .find_map(|reply| match reply.operation_result {
                Some(callback::Reply::Captured {
                    media, start, end, ..
                }) => {
                    assert_eq!((start, end), (Frame(5), Frame(15)));
                    Some(media)
                }
                _ => None,
            })
            .unwrap();
        let (mut planner, mut resources) = runtime.into_planner_and_resources();
        assert_eq!(planner.frame(), Frame(17));
        assert_eq!(planner.state(id()).unwrap().position, 1);
        assert_eq!(planner.work_counts().0 as usize, partitions.len());
        let view = planner.ledger().view(captured).unwrap();
        let source = view.audio.unwrap();
        assert_eq!(source.snapshot.description.frames, 10);
        let mut cursor = resources.render_io().cursor_at(source.index, 0).unwrap();
        let mut bytes = vec![0; 80];
        resources.render_io().read(&mut cursor, &mut bytes).unwrap();
        let actual: Vec<_> = bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        let expected: Vec<_> = (5..15)
            .flat_map(|frame| {
                let x = 3.0 * (frame % 4 + 1) as f32;
                [x, x * 10.0]
            })
            .collect();
        assert_eq!(actual, expected);
        let source = view.midi.unwrap();
        let mut seek = resources.render_io().midi_seek(source.index, 0).unwrap();
        assert_eq!(
            seek.state.channel(0).unwrap().note(72).unwrap().velocity(),
            91
        );
        let mut events = Vec::new();
        while let Some(event) = resources.render_io().midi_next(&mut seek.cursor).unwrap() {
            let mut bytes = vec![0; event.byte_len()];
            resources.render_io().midi_read(event, &mut bytes).unwrap();
            events.push((event.frame(), bytes));
        }
        assert_eq!(
            events,
            [
                (0, vec![0xf8]),
                (4, vec![0xf8]),
                (4, vec![0x80, 72, 64]),
                (8, vec![0xf8])
            ]
        );
        planner.release_all(&mut resources).unwrap();
        assert_eq!(resources.capture_group_usage().free_groups, 1);
    }
}
