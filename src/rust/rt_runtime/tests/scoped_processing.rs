mod support;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::*;
use rt_runtime::planner::*;
use rt_runtime::processor::{Cadence, ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::{Config, Error, Frame, NodeId, PlannerId};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
fn edge(source: Option<u32>, target: Option<u32>, kind: PortType) -> Connection {
    Connection {
        source: source.map_or(Endpoint::host(kind, 0), |n| {
            Endpoint::node(node(n), kind, 0)
        }),
        destination: target.map_or(Endpoint::host(kind, 0), |n| {
            Endpoint::node(node(n), kind, 0)
        }),
    }
}

struct Feedback {
    decisions: [usize; 1],
    finishes: usize,
}
impl Planner for Feedback {
    type Decision = usize;
    type Observation = usize;
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 1,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[usize] {
        &self.decisions
    }
    fn plan(&mut self, c: PlanContext<'_>) -> Result<Frame, Error> {
        Ok(Frame(
            c.frame
                .0
                .saturating_add(self.decisions[0] as u64)
                .min(c.ceiling.0),
        ))
    }
    fn finish(&mut self, c: FinishContext<'_, usize>) -> Result<(), Error> {
        self.decisions[0] = c.observations.first().ok_or(Error::InvalidState)?.value;
        self.finishes += 1;
        Ok(())
    }
}

struct Probe {
    full: bool,
    calls: Arc<AtomicUsize>,
}
impl Processor<Feedback> for Probe {
    fn cadence(&self) -> Cadence {
        if self.full {
            Cadence::Callback
        } else {
            Cadence::Interval
        }
    }
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            observations: usize::from(!self.full),
            ..ConsumerAccess::default()
        }
    }
    fn process(&mut self, b: &mut ProcessBlock<'_, Feedback>) -> Result<(), Error> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let (input, output) = b.audio.pair(0, 0).unwrap();
        for (i, o) in input.iter().zip(output) {
            *o = *i * if self.full { 3.0 } else { 2.0 };
        }
        for i in 0..b.midi_inputs[0].len() {
            let (at, data) = b.midi_inputs[0].get(i).unwrap();
            b.midi_outputs[0].push(at, b.frames, data)?;
        }
        if !self.full {
            b.observations.push(1 + (b.frame.0 as usize % 3))?;
        }
        Ok(())
    }
}

#[shoop_test]
fn observation_dependent_region_does_not_fragment_full_block_successor() {
    let config = Config {
        audio_inputs: 1,
        audio_outputs: 1,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let calls = [Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0))];
    let graph = GraphDescription {
        nodes: vec![node(0), node(1)],
        connections: [PortType::Audio, PortType::Midi]
            .into_iter()
            .flat_map(|kind| {
                [
                    edge(None, Some(0), kind),
                    edge(Some(0), Some(1), kind),
                    edge(Some(1), None, kind),
                ]
            })
            .collect(),
    };
    let (_, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![
            PreparedNode::new(
                node(0),
                Probe {
                    full: false,
                    calls: calls[0].clone(),
                },
            ),
            PreparedNode::new(
                node(1),
                Probe {
                    full: true,
                    calls: calls[1].clone(),
                },
            ),
        ],
        Feedback {
            decisions: [2],
            finishes: 0,
        },
        ResourceStore::new(config).unwrap(),
        &graph,
        &[],
    )
    .unwrap();
    let input: Vec<_> = (0..17).map(|i| i as f32 + 1.0).collect();
    let mut output = vec![0.0; 17];
    let mut midi_in = MidiBuffer::new(8, 24).unwrap();
    for at in [0, 1, 5, 16] {
        midi_in.push(at, 17, &[0x90, 60, 100]).unwrap();
    }
    let midi_in = [midi_in];
    let mut midi_out = [MidiBuffer::new(8, 24).unwrap()];
    support::assert_no_heap(|| {
        runtime
            .process(
                AudioBlock {
                    frame: Frame(0),
                    frames: 17,
                    inputs: &[&input],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &midi_in,
                    outputs: &mut midi_out,
                },
            )
            .unwrap()
    });
    assert_eq!(output, input.iter().map(|x| x * 6.0).collect::<Vec<_>>());
    assert_eq!(
        (0..midi_out[0].len())
            .map(|i| midi_out[0].get(i).unwrap().0)
            .collect::<Vec<_>>(),
        [0, 1, 5, 16]
    );
    assert!(calls[0].load(Ordering::Relaxed) > 1);
    assert_eq!(
        calls[1].load(Ordering::Relaxed),
        1,
        "full-block DSP was fragmented"
    );
    let (planner, _) = runtime.into_planner_and_resources();
    assert_eq!(planner.finishes, calls[0].load(Ordering::Relaxed));
}

#[shoop_test]
fn contraction_refuses_media_cycles_and_reorders_independent_ready_nodes() {
    let config = Config::default().prepare().unwrap();
    let specs: Vec<_> = (0..4)
        .map(|slot| NodeSpec {
            id: node(slot),
            ports: PortCounts {
                audio_inputs: 1,
                audio_outputs: 1,
                ..PortCounts::default()
            },
        })
        .collect();
    let catalog = Catalog::new(config, &specs)
        .unwrap()
        .with_cadences(&[(node(1), Cadence::Callback)])
        .unwrap();
    let cycle = GraphDescription {
        nodes: vec![node(0), node(1), node(2)],
        connections: vec![
            edge(Some(0), Some(1), PortType::Audio),
            edge(Some(1), Some(2), PortType::Audio),
        ],
    };
    assert_eq!(compile(&catalog, &cycle).err(), Some(Error::Cycle));
    // Node 1 is ready before node 2 by ID, but the interval region is atomic.
    let valid = GraphDescription {
        nodes: vec![node(1), node(3), node(2), node(0)],
        connections: vec![edge(Some(0), Some(1), PortType::Audio)],
    };
    let plan = compile(&catalog, &valid).unwrap();
    assert_eq!(
        plan.descriptor()
            .steps
            .iter()
            .map(|step| step.node)
            .collect::<Vec<_>>(),
        [node(0), node(2), node(3), node(1)]
    );
    let mut malformed = plan.descriptor().clone();
    malformed.steps.swap(1, 3);
    let raw = PreparedPlan::from_descriptor(config, malformed).unwrap();
    let mut validator = PlanValidator::new(config);
    validator.begin();
    let result = support::assert_no_heap(|| validator.advance(&raw, &catalog, usize::MAX));
    assert!(matches!(result.status, ValidationStatus::Invalid(_)));
    assert_eq!(
        catalog
            .clone()
            .with_cadences(&[(node(0), Cadence::Callback), (node(0), Cadence::Interval)])
            .err()
            .map(|_| Error::InvalidId),
        Some(Error::InvalidId)
    );
    assert!(Catalog::new(config, &specs)
        .unwrap()
        .with_cadences(&[(NodeId::from_parts(1, 2).unwrap(), Cadence::Callback)])
        .is_err());
}
