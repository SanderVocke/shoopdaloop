//! A prepared delay line retains state across variable-sized processing calls.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{PrepareProcessor, ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Error, NodeId, PreparedConfig};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct DelaySetup {
    frames: usize,
}

struct Delay {
    memory: Box<[f32]>,
    position: usize,
}

// PrepareProcessor allocates processor-owned memory (here a delay line) before rendering.
impl PrepareProcessor for DelaySetup {
    type Prepared = Delay;

    fn prepare(self, config: PreparedConfig) -> Result<Delay, Error> {
        if self.frames == 0 || self.frames > config.limits().max_frames {
            return Err(Error::InvalidParameter);
        }
        Ok(Delay {
            memory: vec![0.0; self.frames].into_boxed_slice(),
            position: 0,
        })
    }
}

impl Processor for Delay {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (&sample, output) in input.iter().zip(output) {
            *output = self.memory[self.position];
            self.memory[self.position] = sample;
            self.position = (self.position + 1) % self.memory.len();
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
    assert!(matches!(
        DelaySetup { frames: 0 }.prepare(config),
        Err(Error::InvalidParameter)
    ));
    let processor: Box<dyn Processor> = Box::new(DelaySetup { frames: 2 }.prepare(config)?);
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
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, processor)],
        NoPlanner,
        ResourceStore::new(config)?,
        &graph,
        &[],
    )?;

    let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let mut output = [0.0; 8];
    let mut start = 0;
    for frames in [3, 1, 4] {
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames,
                    inputs: &[&input[start..start + frames]],
                    outputs: &mut [&mut output[start..start + frames]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })?;
        start += frames;
    }
    assert_eq!(output, [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    drop(runtime);
    drop(control);
    println!(
        "prepared processor: two-frame delay across 3/1/4-frame callbacks, no render heap work"
    );
    Ok(())
}
