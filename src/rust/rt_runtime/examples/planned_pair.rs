//! One planner, two consumers, exact automation and bounded observations.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::{FinishContext, PlanContext, Planner};
use rt_runtime::processor::{ProcessBlock, Processor, ProcessorOperation};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Clone, Copy)]
enum Op {
    Set(f32),
}
impl ProcessorOperation for Op {
    type Result = f32;
}

struct Clock {
    id: PlannerId,
    values: [f32; 1],
    callback_origins: [Frame; 32],
    observed: [usize; 2],
    finishes: usize,
}

impl Planner for Clock {
    type Operation = ();
    type Result = ();

    fn id(&self) -> Option<PlannerId> {
        Some(self.id)
    }
    fn begin_tick(&mut self, context: PlanContext<'_>) -> Result<(), Error> {
        assert_eq!(context.graph_epoch, Epoch(1));
        assert_eq!(
            context.callback_start,
            self.callback_origins[context.frame.0 as usize]
        );
        self.values[0] = 1.0 + ((context.frame.0 / 8) % 2) as f32;
        Ok(())
    }
    fn finish_tick(&mut self, context: FinishContext<'_>) -> Result<(), Error> {
        for frames in self.observed {
            assert_eq!(frames as u64, context.interval.end.0);
        }
        self.finishes += 1;
        Ok(())
    }
}

struct Gain(f32);
impl Processor<Clock, Op> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }

    fn operation(
        &mut self,
        operation: Op,
        context: rt_runtime::processor::OperationContext<'_>,
    ) -> Result<f32, Error> {
        if !context.payload.is_empty() {
            return Err(Error::InvalidEvent);
        }
        let Op::Set(value) = operation;
        if !value.is_finite() {
            return Err(Error::InvalidParameter);
        }
        self.0 = value;
        Ok(value)
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        let slot = block.node.slot() as usize;
        assert_eq!(block.domain.observed[slot] as u64, block.frame.0);
        block.domain.observed[slot] += block.frames;
        let gain = block.domain.values[0] * self.0;
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * gain;
        }
        Ok(())
    }
}

fn demonstrate(partition: &[usize], _quantum: usize, reverse: bool) -> Result<(), Error> {
    let config = Config {
        max_frames: 32,
        audio_inputs: 1,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 2,
        max_ports: 7,
        max_edges: 4,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            events: 1,
            subranges: 0,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let a = NodeId::from_parts(0, 1)?;
    let b = NodeId::from_parts(1, 1)?;
    let id = PlannerId::from_parts(0, 1)?;
    let mut nodes = vec![
        PreparedNode::new(a, Gain(1.0)),
        PreparedNode::new(b, Gain(0.25)),
    ];
    if reverse {
        nodes.reverse();
    }
    let mut graph = GraphDescription {
        nodes: vec![a, b],
        connections: Vec::new(),
    };
    for (node, output) in [(a, 0), (b, 1)] {
        let port = Endpoint::node(node, PortType::Audio, 0);
        graph.connections.extend([
            Connection {
                source: Endpoint::host(PortType::Audio, 0),
                destination: port,
            },
            Connection {
                source: port,
                destination: Endpoint::host(PortType::Audio, output),
            },
        ]);
    }

    let mut callback_origins = [Frame(0); 32];
    let mut offset = 0;
    for &frames in partition {
        callback_origins[offset..offset + frames].fill(Frame(offset as u64));
        offset += frames;
    }
    assert_eq!(offset, 32);
    let planner = Clock {
        id,
        values: [1.0],
        callback_origins,
        observed: [0; 2],
        finishes: 0,
    };
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        planner,
        ResourceStore::new(config)?,
        &graph,
    )?;
    let ticket = control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(13),
                action: Action::Processor {
                    node: a,
                    operation: Op::Set(0.5),
                },
                payload: Box::default(),
            },
        )
        .map_err(|refusal| refusal.error)?;
    let mut left = [0.0; 32];
    let mut right = [0.0; 32];
    let mut offset = 0;
    for &frames in partition {
        if offset == 13 {
            let before = runtime.diagnostics();
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: runtime.next_frame(),
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
            assert_eq!(runtime.diagnostics(), before);
            assert_eq!(runtime.next_frame(), Frame(13));
        }
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames,
                    inputs: &[&[1.0; 32]],
                    outputs: &mut [
                        &mut left[offset..offset + frames],
                        &mut right[offset..offset + frames],
                    ],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })?;
        offset += frames;
    }
    assert_eq!(offset, 32);
    for frame in 0..32 {
        let decision = (1 + (frame / 8) % 2) as f32;
        assert_eq!(left[frame], decision * if frame < 13 { 1.0 } else { 0.5 });
        assert_eq!(right[frame], decision * 0.25);
    }
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert!(matches!(
        completion.outcome,
        Outcome::Scheduled {
            effective_frame: Frame(13),
            late_by: 0
        }
    ));
    drop(completion);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.sequence, ticket.sequence());
    assert_eq!(execution.actual_frame, Frame(13));
    assert_eq!(execution.processor_result, Some(0.5));
    assert_eq!(execution.result, Ok(()));
    assert_eq!(control.outstanding(), 0);
    let (planner, resources) = runtime.into_planner_and_resources();
    assert_eq!(planner.observed, [32; 2]);
    if partition == [32] {
        assert_eq!(planner.finishes, 32);
    }
    drop(planner);
    drop(resources);
    drop(control);
    Ok(())
}

// One planner alternates a gain decision every 8 frames; two consumers borrow it per interval.
fn main() -> Result<(), Error> {
    for reverse in [false, true] {
        demonstrate(&[32], 8, reverse)?;
        demonstrate(&[8, 5, 3, 8, 8], 8, reverse)?;
        demonstrate(&[3, 10, 19], 8, reverse)?;
        demonstrate(&[32], 1, reverse)?;
    }
    println!("planned pair: exact samples, two stamped observations, callback partitions and one-frame progress verified");
    Ok(())
}
