//! Host block validation, processor failure, and continued rendering at next_frame.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::NoPlanner;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Error, Frame, NodeId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

struct FailOnce(bool);

impl Processor for FailOnce {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }

    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        if self.0 {
            self.0 = false;
            return Err(Error::InvalidParameter);
        }
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        output.copy_from_slice(input);
        Ok(())
    }
}

fn callback(
    runtime: &mut GraphRuntime<FailOnce>,
    input: &[f32],
    output: &mut [f32],
    frames: usize,
) -> Result<(), Error> {
    runtime.process(
        AudioBlock {
            frame: runtime.next_frame(),
            frames,
            inputs: &[input],
            outputs: &mut [output],
        },
        MidiBlock {
            inputs: &[],
            outputs: &mut [],
        },
    )
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
    let (control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, FailOnce(true))],
        NoPlanner,
        ResourceStore::new(config)?,
        &graph,
        &[],
    )?;
    let input = [1.0; 8];
    let mut output = [9.0; 8];

    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(999),
                frames: 0,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })?;
    assert_eq!(runtime.next_frame(), Frame(0));

    let invalid_shape = assert_no_heap(|| callback(&mut runtime, &input[..2], &mut output, 4));
    assert_eq!(invalid_shape, Err(Error::InvalidShape));
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(output[..4], [0.0; 4]);
    assert_eq!(output[4..], [9.0; 4]);

    let invalid_frame = assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(99),
                frames: 4,
                inputs: &[&input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    });
    assert_eq!(invalid_frame, Err(Error::InvalidFrame));
    assert_eq!(runtime.next_frame(), Frame(0));

    let processor_error = assert_no_heap(|| callback(&mut runtime, &input, &mut output, 4));
    assert_eq!(processor_error, Err(Error::InvalidParameter));
    assert_eq!(runtime.next_frame(), Frame(4));
    assert_eq!(output[..4], [0.0; 4]);
    assert_eq!(
        runtime.diagnostics().render_error,
        Some(Error::InvalidParameter)
    );

    output.fill(9.0);
    assert_no_heap(|| callback(&mut runtime, &input, &mut output, 2))?;
    assert_eq!(runtime.next_frame(), Frame(6));
    assert_eq!(output[..2], [1.0; 2]);
    assert_eq!(output[2..], [9.0; 6]);
    assert_eq!(runtime.diagnostics().render_error, None);

    drop(runtime);
    drop(control);
    println!(
        "callback adapter: zero/invalid/error/continued blocks checked, no callback heap work"
    );
    Ok(())
}
