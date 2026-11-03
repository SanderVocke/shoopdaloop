mod support;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::Command;
use rt_runtime::executor::{GraphRuntime, PreparedNode};
use rt_runtime::graph::activation::StageId;
use rt_runtime::graph::*;
use rt_runtime::planner::*;
use rt_runtime::processor::{Cadence, ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::Action;
use rt_runtime::transport::Lane;
use rt_runtime::{Config, Epoch, Error, Frame, NodeId, PlannerId};
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

#[derive(Clone, Copy)]
enum Change {
    Activate { id: StageId, epoch: Epoch },
}

struct Feedback {
    decisions: [usize; 1],
    finishes: usize,
}
impl Planner for Feedback {
    type Decision = usize;
    type Observation = usize;
    type Operation = Change;
    type Result = Epoch;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 1,
            operation_bytes: 0,
        })
    }
    fn operation(
        &mut self,
        operation: Change,
        mut c: OperationContext<'_, Change, Epoch>,
    ) -> Result<Disposition<Epoch>, Error> {
        let Change::Activate { id, epoch } = operation;
        Ok(Disposition::Complete(
            c.graphs.prepare_activation(id, epoch)?.commit(),
        ))
    }
    fn decisions(&self) -> &[usize] {
        &self.decisions
    }
    fn plan(&mut self, c: PlanContext<'_, Change, Epoch>) -> Result<Frame, Error> {
        Ok(Frame(
            c.frame
                .0
                .saturating_add(self.decisions[0] as u64)
                .min(c.ceiling.0),
        ))
    }
    fn finish(&mut self, c: FinishContext<'_, usize, Change, Epoch>) -> Result<(), Error> {
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
fn observation_dependent_region_preserves_full_block_predecessor_and_successor() {
    for full_index in [0, 1] {
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
                        full: full_index == 0,
                        calls: calls[0].clone(),
                    },
                ),
                PreparedNode::new(
                    node(1),
                    Probe {
                        full: full_index == 1,
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
        assert!(calls[1 - full_index].load(Ordering::Relaxed) > 1);
        assert_eq!(
            calls[full_index].load(Ordering::Relaxed),
            1,
            "full-block DSP was fragmented"
        );
        let (planner, _) = runtime.into_planner_and_resources();
        assert_eq!(
            planner.finishes,
            calls[1 - full_index].load(Ordering::Relaxed)
        );
    }
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

#[shoop_test]
fn local_activation_keeps_its_frame_refuses_parent_changes_and_retires_owners_off_render() {
    let config = Config {
        audio_inputs: 1,
        audio_outputs: 1,
        max_nodes: 2,
        max_edges: 8,
        max_ports: 16,
        budgets: rt_runtime::Budgets {
            validation_steps: 128,
            ..rt_runtime::Budgets::default()
        },
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
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
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
    runtime.prepare_graph_staging(2).unwrap();
    let mut child_edit = graph.clone();
    child_edit
        .connections
        .push(edge(Some(0), None, PortType::Audio));
    let mut parent_edit = graph.clone();
    parent_edit
        .connections
        .retain(|edge| *edge != self::edge(Some(1), None, PortType::Midi));
    for (slot, graph) in [child_edit, parent_edit].into_iter().enumerate() {
        let plan = compile_planned(runtime.catalog(), &graph, &[]).unwrap();
        assert!(control
            .submit(Lane::Normal, Command::stage_graph(slot, Epoch(1), plan))
            .is_ok());
    }
    let input = [1.0; 8];
    let mut output = [0.0; 8];
    let midi_in = [MidiBuffer::new(8, 24).unwrap()];
    let mut midi_out = [MidiBuffer::new(8, 24).unwrap()];
    let process = |runtime: &mut GraphRuntime<Probe, Feedback>,
                   output: &mut [f32],
                   midi_out: &mut [MidiBuffer],
                   frames| {
        let frame = runtime.next_frame();
        support::assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[&input],
                    outputs: &mut [output],
                },
                MidiBlock {
                    inputs: &midi_in,
                    outputs: midi_out,
                },
            )
        })
        .unwrap();
    };
    process(&mut runtime, &mut output, &mut midi_out, 1);
    let mut staged = [None; 2];
    while let Some(completion) = control.try_completion() {
        if let Command::StageGraph { slot, id, .. } = completion.request {
            staged[slot] = id;
        }
    }
    let at = runtime.next_frame();
    for (offset, id) in [
        (1, staged[1].unwrap()),
        (2, staged[0].unwrap()),
        (4, staged[0].unwrap()),
    ] {
        assert!(control
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at: at.checked_add(offset).unwrap(),
                    action: Action::Planner {
                        planner: planner(),
                        operation: Change::Activate {
                            id,
                            epoch: Epoch(1)
                        }
                    },
                    payload: Box::new([])
                }
            )
            .is_ok());
    }
    for (offset, packet) in [(2, [0x90, 60, 100]), (4, [0x80, 60, 64])] {
        assert!(control
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at: at.checked_add(offset).unwrap(),
                    action: Action::Midi {
                        node: node(0),
                        port: 0
                    },
                    payload: packet.into(),
                }
            )
            .is_ok());
    }
    let before = calls[1].load(Ordering::Relaxed);
    process(&mut runtime, &mut output, &mut midi_out, 8);
    assert_eq!(output, [6.0, 6.0, 8.0, 8.0, 8.0, 8.0, 8.0, 8.0]);
    assert_eq!(calls[1].load(Ordering::Relaxed), before + 1);
    assert_eq!(runtime.epoch(), Epoch(2));
    assert_eq!(midi_out[0].get(0), Some((2, &[0x90, 60, 100][..])));
    assert_eq!(midi_out[0].get(1), Some((4, &[0x80, 60, 64][..])));
    assert_eq!(midi_out[0].len(), 2);
    let executions: Vec<_> = std::iter::from_fn(|| control.try_execution()).collect();
    assert_eq!(executions.len(), 5);
    for reply in executions
        .iter()
        .filter(|reply| matches!(reply.action, Action::Midi { .. }))
    {
        assert_eq!(reply.result, Ok(()));
        assert_eq!(reply.actual_frame, reply.requested_frame);
    }
    let replies: Vec<_> = executions
        .iter()
        .filter(|reply| matches!(reply.action, Action::Planner { .. }))
        .collect();
    assert_eq!(replies.len(), 3);
    assert_eq!(replies[0].result, Err(Error::InvalidGraph));
    assert_eq!(replies[0].actual_frame, at.checked_add(1).unwrap());
    assert_eq!(replies[1].result, Ok(()));
    assert_eq!(replies[1].operation_result, Some(Epoch(2)));
    assert_eq!(replies[1].actual_frame, at.checked_add(2).unwrap());
    assert_eq!(replies[2].result, Err(Error::StaleEpoch));
    while control.try_completion().is_some() {}
    for id in staged.into_iter().flatten() {
        assert!(control
            .submit(Lane::Normal, Command::ReleaseGraph { id })
            .is_ok());
    }
    process(&mut runtime, &mut output, &mut midi_out, 1);
    let mut retired = 0;
    while let Some(completion) = control.try_completion() {
        if completion.retired.is_some() {
            retired += 1;
        }
    }
    assert_eq!(retired, 2);
}

#[shoop_test]
fn declared_readiness_edges_validate_signal_and_control_cycles_incrementally() {
    let config = Config::default().prepare().unwrap();
    let specs: Vec<_> = (0..3)
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
        .with_dependencies(&[(node(2), node(0))])
        .unwrap();
    let graph = GraphDescription {
        nodes: vec![node(0), node(1), node(2)],
        connections: vec![edge(Some(0), Some(1), PortType::Audio)],
    };
    let plan = compile(&catalog, &graph).unwrap();
    assert_eq!(
        plan.descriptor()
            .steps
            .iter()
            .map(|step| step.node)
            .collect::<Vec<_>>(),
        [node(2), node(0), node(1)]
    );
    let mut validator = PlanValidator::new(config);
    validator.begin();
    loop {
        let progress = support::assert_no_heap(|| validator.advance(&plan, &catalog, 1));
        match progress.status {
            ValidationStatus::Pending => {}
            ValidationStatus::Valid => break,
            status => panic!("{status:?}"),
        }
    }
    let cyclic = catalog
        .clone()
        .with_dependencies(&[(node(2), node(0)), (node(1), node(2))])
        .unwrap();
    assert_eq!(compile(&cyclic, &graph).err(), Some(Error::Cycle));
    let missing = GraphDescription {
        nodes: vec![node(0)],
        connections: vec![],
    };
    assert_eq!(compile(&catalog, &missing).err(), Some(Error::InvalidGraph));
    assert!(catalog
        .clone()
        .with_dependencies(&[(node(0), node(0))])
        .is_err());
    assert!(catalog
        .with_dependencies(&[(node(0), node(1)), (node(0), node(1))])
        .is_err());
}

#[shoop_test]
fn failed_fault_cleanup_is_reported_without_rewinding_the_outer_clock() {
    struct FaultPlanner;
    impl Planner for FaultPlanner {
        type Decision = ();
        type Observation = ();
        type Operation = ();
        type Result = ();
        fn spec(&self) -> Option<PlannerSpec> {
            None
        }
        fn decisions(&self) -> &[()] {
            &[]
        }
        fn plan(&mut self, c: PlanContext<'_>) -> Result<Frame, Error> {
            Ok(c.ceiling)
        }
        fn finish(&mut self, _: FinishContext<'_, ()>) -> Result<(), Error> {
            Ok(())
        }
        fn abort(&mut self, c: FinishContext<'_, ()>, error: Error) -> Result<(), Error> {
            assert_eq!(error, Error::InvalidParameter);
            assert_eq!(
                c.interval,
                Interval {
                    start: Frame(0),
                    end: Frame(4)
                }
            );
            Err(Error::Capacity)
        }
    }
    struct Fault;
    impl Processor<FaultPlanner> for Fault {
        fn ports(&self) -> PortCounts {
            PortCounts::default()
        }
        fn process(&mut self, _: &mut ProcessBlock<'_, FaultPlanner>) -> Result<(), Error> {
            Err(Error::InvalidParameter)
        }
    }
    let config = Config {
        max_frames: 4,
        max_nodes: 1,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let (_control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node(0), Fault)],
        FaultPlanner,
        ResourceStore::new(config).unwrap(),
        &GraphDescription {
            nodes: vec![node(0)],
            connections: vec![],
        },
        &[],
    )
    .unwrap();
    let result = support::assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 4,
                inputs: &[],
                outputs: &mut [],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    });
    assert_eq!(result, Err(Error::Capacity));
    assert_eq!(runtime.diagnostics().render_error, Some(Error::Capacity));
    assert_eq!(runtime.next_frame(), Frame(4));
}
