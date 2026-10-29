mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{
    compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType, PreparedPlan,
};
use rt_runtime::planner::*;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Config, Epoch, Error, Frame, NodeId, PlannerId};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn node(slot: u32, generation: u32) -> NodeId {
    NodeId::from_parts(slot, generation).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}

/// Requires `required` in every published graph and protects `protected` slots.
struct Guard {
    decisions: [f32; 1],
    required: NodeId,
    protected: NodeId,
}
impl Planner for Guard {
    type Decision = f32;
    type Observation = ();
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 0,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[f32] {
        &self.decisions
    }
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        Ok(context.ceiling)
    }
    fn finish(&mut self, _context: FinishContext<'_, ()>) -> Result<(), Error> {
        Ok(())
    }
    fn admit_graph(&self, candidate: &PreparedPlan) -> Result<(), Error> {
        if candidate.contains(self.required) {
            Ok(())
        } else {
            Err(Error::InvalidGraph)
        }
    }
    fn admit_processor_change(&self, node: NodeId) -> Result<(), Error> {
        if node == self.protected {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
}

struct Gain(f32);
impl Processor<Guard> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Guard>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * self.0;
        }
        Ok(())
    }
}
impl Drop for Gain {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}

fn graph(through: NodeId) -> GraphDescription {
    let port = Endpoint::node(through, PortType::Audio, 0);
    let host = Endpoint {
        node: None,
        kind: PortType::Audio,
        port: 0,
    };
    GraphDescription {
        nodes: vec![through],
        connections: vec![
            Connection {
                source: host,
                destination: port,
            },
            Connection {
                source: port,
                destination: host,
            },
        ],
    }
}

struct Harness {
    control: Control<Gain, Guard>,
    runtime: GraphRuntime<Gain, Guard>,
    output: [f32; 4],
}
impl Harness {
    fn new(required: NodeId, protected: NodeId) -> Self {
        let config = Config {
            max_frames: 4,
            max_nodes: 2,
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 0,
            midi_outputs: 0,
            ..Config::default()
        }
        .prepare()
        .unwrap();
        let (control, runtime) = GraphRuntime::prepare_with_initial_graph(
            config,
            vec![
                PreparedNode::new(node(0, 1), Gain(2.0)),
                PreparedNode::new(node(1, 1), Gain(3.0)),
            ],
            Guard {
                decisions: [0.0],
                required,
                protected,
            },
            ResourceStore::new(config).unwrap(),
            &graph(node(0, 1)),
            &[],
        )
        .unwrap();
        Self {
            control,
            runtime,
            output: [0.0; 4],
        }
    }
    fn step(&mut self) -> Vec<Outcome> {
        let frame = self.runtime.next_frame();
        assert_no_heap(|| {
            self.runtime.process(
                AudioBlock {
                    frame,
                    frames: 4,
                    inputs: &[&[1.0; 4]],
                    outputs: &mut [&mut self.output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        std::iter::from_fn(|| self.control.try_completion())
            .map(|completion| completion.outcome)
            .collect()
    }
    fn commit(&mut self, through: NodeId) {
        let plan = compile_planned(self.runtime.catalog(), &graph(through), &[]).unwrap();
        self.control
            .submit(Lane::Normal, Command::commit(self.runtime.epoch(), plan))
            .unwrap_or_else(|error| panic!("{:?}", error.error));
    }
}

#[shoop_test]
fn planner_refuses_generic_graph_commits_without_changing_the_active_graph() {
    let mut h = Harness::new(node(0, 1), node(1, 9));
    assert_eq!(h.runtime.epoch(), Epoch(1));
    h.commit(node(1, 1));
    assert_eq!(h.step(), [Outcome::Rejected(Error::InvalidGraph)]);
    assert_eq!(h.runtime.epoch(), Epoch(1));
    assert_eq!(h.output, [2.0; 4]);
    h.commit(node(0, 1));
    assert_eq!(h.step(), [Outcome::Applied]);
    assert_eq!(h.runtime.epoch(), Epoch(2));
    assert_eq!(h.output, [2.0; 4]);
}

#[shoop_test]
fn planner_refuses_generic_processor_removal_and_installation() {
    let mut h = Harness::new(node(0, 1), node(1, 1));
    h.control
        .submit(Lane::Normal, Command::Remove { node: node(1, 1) })
        .unwrap_or_else(|error| panic!("{:?}", error.error));
    assert_eq!(h.step(), [Outcome::Rejected(Error::Busy)]);
    assert_eq!(h.runtime.epoch(), Epoch(1));

    let mut h = Harness::new(node(0, 1), node(1, 2));
    h.control
        .submit(Lane::Normal, Command::Remove { node: node(1, 1) })
        .unwrap_or_else(|error| panic!("{:?}", error.error));
    let completion = {
        let frame = h.runtime.next_frame();
        assert_no_heap(|| {
            h.runtime.process(
                AudioBlock {
                    frame,
                    frames: 4,
                    inputs: &[&[1.0; 4]],
                    outputs: &mut [&mut h.output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        h.control.try_completion().unwrap()
    };
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Processor(_))));
    assert_eq!(h.runtime.epoch(), Epoch(2));
    h.control
        .submit(
            Lane::Normal,
            Command::Install {
                prepared: Some(PreparedNode::new(node(1, 2), Gain(5.0))),
            },
        )
        .unwrap_or_else(|error| panic!("{:?}", error.error));
    let completion = {
        let frame = h.runtime.next_frame();
        assert_no_heap(|| {
            h.runtime.process(
                AudioBlock {
                    frame,
                    frames: 4,
                    inputs: &[&[1.0; 4]],
                    outputs: &mut [&mut h.output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        h.control.try_completion().unwrap()
    };
    assert_eq!(completion.outcome, Outcome::Rejected(Error::Busy));
    assert!(matches!(
        completion.request,
        Command::Install { prepared: Some(_) }
    ));
    assert_eq!(h.runtime.epoch(), Epoch(2));
    drop(completion);
}

#[shoop_test]
fn planner_refuses_an_initial_graph_without_its_requirements() {
    let config = Config {
        max_frames: 4,
        max_nodes: 2,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let refused = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![
            PreparedNode::new(node(0, 1), Gain(2.0)),
            PreparedNode::new(node(1, 1), Gain(3.0)),
        ],
        Guard {
            decisions: [0.0],
            required: node(0, 1),
            protected: node(1, 9),
        },
        ResourceStore::new(config).unwrap(),
        &graph(node(1, 1)),
        &[],
    );
    assert_eq!(refused.err(), Some(Error::InvalidGraph));
}
