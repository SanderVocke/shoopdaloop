//! Prepare an active graph before the first callback and check its samples.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Epoch, Error, Frame, NodeId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct Gain(f32);

impl Processor for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * self.0;
        }
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let config = Config {
        max_frames: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 1,
        max_ports: 4,
        max_edges: 2,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        ..Config::default()
    }
    .prepare()?;
    let node = NodeId::from_parts(0, 1)?;
    let graph = GraphDescription {
        nodes: vec![node],
        connections: vec![
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: Endpoint::node(node, PortType::Audio, 0),
            },
            Connection {
                source: Endpoint::node(node, PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, 0),
            },
        ],
    };
    let resources = ResourceStore::new(config)?;
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, Gain(0.5))],
        NoPlanner,
        resources,
        &graph,
        &[],
    )?;
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(control.outstanding(), 0);

    let input = [1.0, 0.5, -1.0, 0.0, 0.25, -0.5, 2.0, -2.0];
    let mut output = [0.0; 8];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: input.len(),
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    assert_eq!(output, [0.5, 0.25, -0.5, 0.0, 0.125, -0.25, 1.0, -1.0]);
    assert_eq!(runtime.next_frame(), Frame(8));

    drop(runtime);
    drop(control);
    println!("first graph: active at startup, eight checked samples, no render heap work");
    Ok(())
}
