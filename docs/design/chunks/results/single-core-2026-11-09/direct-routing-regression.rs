mod support;

use rt_runtime::audio_refs::{GainEnvelope, ReferenceBlock};
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{AudioRenderMode, Config, Error, NodeId, Quantum};
use shoop_wasm_test_support::shoop_test;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct Pipe {
    gain: f32,
    references: bool,
}
impl Processor for Pipe {
    fn compact_graph_supported() -> bool {
        true
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 2,
            audio_outputs: 2,
            midi_inputs: 0,
            midi_outputs: 0,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        for port in 0..2 {
            let (input, output) = block.audio.pair(port, port).ok_or(Error::InvalidShape)?;
            for (input, output) in input.iter().zip(output) {
                *output = *input * self.gain;
            }
        }
        Ok(())
    }
    fn process_references(
        &mut self,
        block: &mut ReferenceBlock<'_, rt_runtime::planner::NoPlanner>,
    ) -> Result<bool, Error> {
        if !self.references {
            return Ok(false);
        }
        for port in 0..2 {
            block.outputs[port] = block.inputs[port].scaled(GainEnvelope::constant(self.gain));
        }
        Ok(true)
    }
    fn publish_references(
        &mut self,
        block: &mut ReferenceBlock<'_, rt_runtime::planner::NoPlanner>,
    ) -> Result<(), Error> {
        for port in 0..2 {
            let input = block.inputs[port];
            let output = block.outputs[port];
            let expected = block.peak(input)? * self.gain.abs();
            assert_eq!(block.peak(output)?, expected);
        }
        Ok(())
    }
}

fn edge(from: Option<NodeId>, to: Option<NodeId>, port: u32) -> Connection {
    Connection {
        source: Endpoint {
            node: from,
            kind: PortType::Audio,
            port,
        },
        destination: Endpoint {
            node: to,
            kind: PortType::Audio,
            port,
        },
    }
}

#[shoop_test]
fn compact_routes_preserve_samples_and_publication_across_slot_orders_and_sample_fallback() {
    for reverse in [false, true] {
        for mixed in [false, true] {
            let ids = if reverse { [2, 1, 0] } else { [0, 1, 2] }
                .map(|slot| NodeId::from_parts(slot, 1).unwrap());
            let config = Config {
                max_frames: 8,
                max_nodes: 4,
                max_ports: 32,
                max_edges: 16,
                audio_inputs: 2,
                audio_outputs: 2,
                midi_inputs: 0,
                midi_outputs: 0,
                ..Config::default()
            }
            .prepare()
            .unwrap();
            let quantum = Quantum::new(1).unwrap();
            let mut resources = ResourceStore::new(config, quantum).unwrap();
            resources.set_audio_render_mode(AudioRenderMode::Compact);
            let nodes = ids
                .into_iter()
                .zip([2., 0.5, 4.])
                .enumerate()
                .map(|(index, (id, gain))| {
                    PreparedNode::new(
                        id,
                        Pipe {
                            gain,
                            references: index == 1,
                        },
                    )
                })
                .collect();
            let (_control, mut runtime) =
                GraphRuntime::prepare_with_resources(config, quantum, nodes, resources).unwrap();
            let mut graph = GraphDescription {
                nodes: ids.to_vec(),
                connections: Vec::new(),
            };
            for port in 0..2 {
                graph.connections.extend([
                    edge(None, Some(ids[0]), port),
                    edge(Some(ids[0]), Some(ids[1]), port),
                    edge(Some(ids[1]), Some(ids[2]), port),
                    edge(Some(ids[2]), None, port),
                ]);
            }
            if mixed {
                let mut extra = edge(None, Some(ids[2]), 0);
                extra.source.port = 1;
                graph.connections.push(extra);
            }
            graph.connections.reverse();
            runtime.prepare_initial_graph(&graph).unwrap();
            let left = [-1., 0.25, 2., -0.5, 0., 0.75, -2., 1.];
            let right = [0.5, -1., 0.25, 1., -0.5, 2., 0.75, -2.];
            for frames in [1, 3, 8, 5] {
                let mut a = [99.; 8];
                let mut b = [99.; 8];
                let frame = runtime.next_frame();
                support::assert_no_heap(|| {
                    runtime.process(
                        AudioBlock {
                            frame,
                            frames,
                            inputs: &[&left, &right],
                            outputs: &mut [&mut a, &mut b],
                        },
                        MidiBlock {
                            inputs: &[],
                            outputs: &mut [],
                        },
                    )
                })
                .unwrap();
                for i in 0..frames {
                    assert_eq!(a[i], (left[i] + if mixed { right[i] } else { 0. }) * 4.);
                    assert_eq!(b[i], right[i] * 4.);
                }
                assert_eq!(&a[frames..], &[99.; 8][frames..]);
                assert_eq!(&b[frames..], &[99.; 8][frames..]);
            }
        }
    }
}
