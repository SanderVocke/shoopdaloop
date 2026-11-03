use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint as Port, GraphDescription, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, NodeId};
use shoop_dsp::DelayCopy;
use shoop_dsp_link::{memory, service::ReferenceService, Buffer, Descriptor, Endpoint, Header};
use shoop_dsp_link_node::RemoteNode;
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn descriptor() -> Descriptor {
    Descriptor {
        channels: 2,
        quantum: 3,
        dsp_frames: 5,
        max_callback: 7,
        service_allowance: 11,
        latency: 36,
        slots: 16,
        result_budget: 16,
        midi_capacity: 8,
        ..Descriptor::default()
    }
}
fn graph<E: Endpoint>(endpoint: E) -> (Control<RemoteNode<E>>, GraphRuntime<RemoteNode<E>>) {
    let d = endpoint.descriptor();
    let config = Config {
        sample_rate: d.sample_rate,
        max_frames: d.max_callback as usize,
        audio_inputs: d.channels as usize,
        audio_outputs: d.channels as usize,
        midi_inputs: 1,
        midi_outputs: 1,
        max_nodes: 1,
        max_ports: 16,
        max_edges: 8,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let node = NodeId::from_parts(0, 1).unwrap();
    let processor = RemoteNode::prepare(endpoint, config).unwrap();
    let mut edges = Vec::new();
    for kind in [PortType::Audio, PortType::Midi] {
        let count = if kind == PortType::Audio {
            d.channels as u32
        } else {
            1
        };
        for p in 0..count {
            edges.push(Connection {
                source: Port::host(kind, p),
                destination: Port::node(node, kind, p),
            });
            edges.push(Connection {
                source: Port::node(node, kind, p),
                destination: Port::host(kind, p),
            });
        }
    }
    GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, processor)],
        NoPlanner,
        ResourceStore::new(config).unwrap(),
        &GraphDescription {
            nodes: vec![node],
            connections: edges,
        },
        &[],
    )
    .unwrap()
}

#[shoop_wasm_test_support::shoop_test]
fn actual_graph_preserves_l_plus_a_across_variable_callbacks_without_heap_work() {
    let d = descriptor();
    let (endpoint, peer) = memory::pair(d).unwrap();
    let (_control, mut graph) = graph(endpoint);
    let mut service = ReferenceService::prepare(peer).unwrap();
    let mut dsp = DelayCopy::prepare(d.dsp_config(), 7).unwrap();
    let mut incoming = [MidiBuffer::new(16, 256).unwrap()];
    let mut outgoing = [MidiBuffer::new(64, 512).unwrap()];
    let mut checked = 0;
    let mut midi_count = 0;
    for iteration in 0..200 {
        let n = [1, 7, 2, 3, 5][iteration % 5];
        let frame = graph.next_frame().0;
        let mut left = [0.; 7];
        let mut right = [0.; 7];
        for i in 0..n {
            left[i] = (frame + i as u64 + 1) as f32;
            right[i] = -left[i];
        }
        incoming[0].clear();
        outgoing[0].clear();
        for i in 0..n {
            if (frame + i as u64) % 23 == 0 {
                incoming[0].push(i as u32, n, &[0x90, 60, 100]).unwrap();
            }
        }
        let mut out_left = [0.; 7];
        let mut out_right = [0.; 7];
        assert_no_heap(|| {
            graph.process(
                AudioBlock {
                    frame: graph.next_frame(),
                    frames: n,
                    inputs: &[&left[..n], &right[..n]],
                    outputs: &mut [&mut out_left[..n], &mut out_right[..n]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut outgoing,
                },
            )
        })
        .unwrap();
        for i in 0..n {
            let f = frame + i as u64;
            let expected = if f < 43 { 0. } else { (f - 43 + 1) as f32 };
            assert_eq!(out_left[i], expected, "source frame {f}");
            assert_eq!(out_right[i], -expected);
            if expected != 0. {
                checked += 1;
            }
        }
        assert!(!outgoing[0].is_recovery());
        for i in 0..outgoing[0].len() {
            let (offset, bytes) = outgoing[0].get(i).unwrap();
            assert_eq!((frame + u64::from(offset) - 36) % 23, 0);
            assert_eq!(bytes, [0x90, 60, 100]);
            midi_count += 1;
        }
        assert_no_heap(|| service.poll(&mut dsp)).unwrap();
    }
    assert!(checked > 500 && midi_count > 20);
}

#[shoop_wasm_test_support::shoop_test]
fn late_result_cannot_patch_the_remaining_samples_of_a_committed_block() {
    let d = Descriptor {
        quantum: 4,
        dsp_frames: 4,
        max_callback: 3,
        service_allowance: 0,
        latency: 20,
        ..descriptor()
    };
    let (endpoint, mut peer) = memory::pair(d).unwrap();
    let (_control, mut graph) = graph(endpoint);
    let mut midi = [MidiBuffer::new(1, 3).unwrap()];
    let incoming = [MidiBuffer::new(1, 3).unwrap()];
    let mut out = [0.; 3];
    for _ in 0..7 {
        assert_no_heap(|| {
            graph.process(
                AudioBlock {
                    frame: graph.next_frame(),
                    frames: 3,
                    inputs: &[&[1.; 3], &[1.; 3]],
                    outputs: &mut [&mut out, &mut [0.; 3]],
                },
                MidiBlock {
                    inputs: &incoming,
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
    }
    assert_eq!(graph.next_frame().0, 21);
    for (frame, value) in [(0, 42.), (4, 99.)] {
        let mut result = peer.acquire_output().unwrap();
        result.set_header(Header {
            generation: d.generation,
            frame,
            ..Header::default()
        });
        result.audio_mut().fill(value);
        assert!(peer.publish(result).is_ok());
    }
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out, [0.; 3]);
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out, [99.; 3]);
    drop(peer);
    assert_no_heap(|| {
        graph.process(
            AudioBlock {
                frame: graph.next_frame(),
                frames: 3,
                inputs: &[&[1.; 3], &[1.; 3]],
                outputs: &mut [&mut out, &mut [0.; 3]],
            },
            MidiBlock {
                inputs: &incoming,
                outputs: &mut midi,
            },
        )
    })
    .unwrap();
    assert_eq!(out[1..], [0., 0.]);
}

#[shoop_wasm_test_support::shoop_test]
fn midi_reset_uses_reserved_capacity_and_rejects_invalid_offset() {
    let mut midi = MidiBuffer::new(0, 0).unwrap();
    assert_no_heap(|| {
        assert!(midi.request_reset(3, 3).is_err());
        assert!(midi.is_empty());
        midi.request_reset(0, 1).unwrap();
        assert_eq!(midi.len(), 32);
        assert!(midi.is_recovery());
        assert!(midi.push(0, 1, &[0x90, 60, 100]).is_err());
    });
}
