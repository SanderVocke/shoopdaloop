//! Borrow-scoped graph/domain publication and independent retirement ownership.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::activation::{GraphAccess, StageId, StageStatus};
use rt_runtime::graph::{
    compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType,
};
use rt_runtime::planner::{
    ConsumerAccess, DecisionBinding, Disposition, FinishContext, OperationContext, PlanContext,
    Planner, PlannerSpec,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::{Action, Execution, ReceiptToken};
use rt_runtime::transport::{Completion, Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
fn graph(swapped: bool) -> GraphDescription {
    GraphDescription {
        nodes: vec![node(0), node(1)],
        connections: (0..2)
            .map(|slot| Connection {
                source: Endpoint::node(node(slot), PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, if swapped { 1 - slot } else { slot }),
            })
            .collect(),
    }
}
fn bindings(swapped: bool) -> [DecisionBinding; 2] {
    std::array::from_fn(|slot| DecisionBinding {
        planner: planner(),
        consumer: node(slot as u32),
        decision_slot: if swapped { 1 - slot } else { slot },
    })
}

#[derive(Clone, Copy)]
enum Attempt {
    Unused,
    Refuse,
    Publish,
}
#[derive(Clone, Copy)]
enum Op {
    Inspect(StageId),
    Try {
        id: StageId,
        expected: Epoch,
        attempt: Attempt,
    },
    Defer {
        id: StageId,
        expected: Epoch,
        at: Frame,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reply {
    Status(StageStatus),
    Unused,
    Activated(Epoch),
}

struct Domain {
    values: [f32; 2],
    swapped: bool,
    active: bool,
    epoch: Epoch,
    pending: Option<(ReceiptToken, StageId, Epoch, Frame)>,
}
impl Domain {
    fn activate(
        &mut self,
        graphs: &mut GraphAccess<'_>,
        id: StageId,
        expected: Epoch,
        attempt: Attempt,
    ) -> Result<Reply, Error> {
        let permit = graphs.prepare_activation(id, expected)?;
        if matches!(attempt, Attempt::Unused) {
            return Ok(Reply::Unused);
        }
        if matches!(attempt, Attempt::Refuse) {
            return Err(Error::InvalidParameter);
        }
        let candidate = permit.candidate();
        if candidate.descriptor().steps.len() != 2 || candidate.descriptor().connections.len() != 2
        {
            return Err(Error::InvalidGraph);
        }
        for (consumer, decision_slot) in [(node(0), 1), (node(1), 0)] {
            if !candidate.contains(consumer)
                || !candidate.planning().bindings.iter().any(|binding| {
                    binding.consumer == consumer
                        && binding.planner == planner()
                        && binding.decision_slot == decision_slot
                })
            {
                return Err(Error::InvalidGraph);
            }
            let route = Connection {
                source: Endpoint::node(consumer, PortType::Audio, 0),
                destination: Endpoint::host(PortType::Audio, decision_slot as u32),
            };
            if !candidate.descriptor().connections.contains(&route) {
                return Err(Error::InvalidGraph);
            }
        }
        let epoch = permit.next_epoch();
        self.values = [0.75, 0.125];
        self.swapped = true;
        self.active = true;
        self.epoch = epoch;
        assert_eq!(permit.commit(), epoch);
        Ok(Reply::Activated(epoch))
    }
}
impl Planner for Domain {
    type Decision = f32;
    type Observation = f32;
    type Operation = Op;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 2,
            observation_capacity: 2,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[f32] {
        &self.values
    }
    fn operation(
        &mut self,
        operation: Op,
        mut context: OperationContext<'_, Op, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        let reply = match operation {
            Op::Inspect(id) => Reply::Status(context.graphs.status(id)?),
            Op::Try {
                id,
                expected,
                attempt,
            } => self.activate(&mut context.graphs, id, expected, attempt)?,
            Op::Defer { id, expected, at } => {
                if self.pending.is_some() || at < context.frame {
                    return Err(Error::InvalidState);
                }
                self.pending = Some((context.token, id, expected, at));
                return Ok(Disposition::Pending);
            }
        };
        Ok(Disposition::Complete(reply))
    }
    fn plan(&mut self, mut context: PlanContext<'_, Op, Reply>) -> Result<Frame, Error> {
        self.epoch = context.graph_epoch;
        if self
            .pending
            .is_some_and(|(_, _, _, at)| at == context.frame)
        {
            let (token, id, expected, _) = self.pending.take().unwrap();
            let result = self.activate(&mut context.graphs, id, expected, Attempt::Publish);
            context.receipts.complete(token, result)?;
        }
        Ok(self
            .pending
            .map_or(context.ceiling, |(_, _, _, at)| at.min(context.ceiling)))
    }
    fn finish(&mut self, context: FinishContext<'_, f32, Op, Reply>) -> Result<(), Error> {
        assert_eq!(context.observations.len(), if self.active { 2 } else { 0 });
        for observation in context.observations {
            let slot = observation.producer.slot() as usize;
            assert_eq!(
                observation.value,
                self.values[if self.swapped { 1 - slot } else { slot }]
            );
        }
        Ok(())
    }
}
struct Fill;
impl Processor<Domain> for Fill {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            observations: 1,
            operation_bytes: 0,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Domain>) -> Result<(), Error> {
        let value = *block.decision.as_ref().ok_or(Error::InvalidState)?.value;
        block
            .audio
            .output(0)
            .ok_or(Error::InvalidShape)?
            .fill(value);
        block.observations.push(value)
    }
}

fn prepare(initial: bool) -> Result<(Control<Fill, Domain>, GraphRuntime<Fill, Domain>), Error> {
    let config = Config {
        max_frames: 16,
        audio_inputs: 0,
        audio_outputs: 2,
        midi_inputs: 0,
        midi_outputs: 0,
        max_nodes: 4,
        max_ports: 8,
        max_edges: 8,
        resource_slots: 1,
        resource_pages: 1,
        page_bytes: 64,
        budgets: Budgets {
            events: 8,
            validation_steps: 128,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let nodes = vec![
        PreparedNode::new(node(0), Fill),
        PreparedNode::new(node(1), Fill),
    ];
    let domain = Domain {
        values: [0.5, 0.25],
        swapped: false,
        active: initial,
        epoch: Epoch(u64::from(initial)),
        pending: None,
    };
    let resources = ResourceStore::new(config)?;
    let (control, mut runtime) = if initial {
        GraphRuntime::prepare_with_initial_graph(
            config,
            nodes,
            domain,
            resources,
            &graph(false),
            &bindings(false),
        )?
    } else {
        GraphRuntime::prepare_with_planner_and_resources(config, nodes, domain, resources)?
    };
    // Opt in to one staging slot before the first callback; a staged plan becomes Ready, not active.
    runtime.prepare_graph_staging(1)?;
    Ok((control, runtime))
}

fn render(
    runtime: &mut GraphRuntime<Fill, Domain>,
    frames: usize,
    output: &mut [[f32; 16]; 2],
) -> Result<(), Error> {
    let [left, right] = output;
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames,
                inputs: &[],
                outputs: &mut [left, right],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
}
fn apply(
    control: &mut Control<Fill, Domain>,
    runtime: &mut GraphRuntime<Fill, Domain>,
    command: Command<Fill, Domain>,
    output: &mut [[f32; 16]; 2],
) -> Completion<Command<Fill, Domain>, Retired<Fill>> {
    let ticket = control
        .submit(Lane::Normal, command)
        .map_err(|refusal| refusal.error)
        .unwrap();
    for _ in 0..64 {
        render(runtime, 1, output).unwrap();
        if let Some(completion) = control.try_completion() {
            assert_eq!(completion.sequence, ticket.sequence());
            return completion;
        }
    }
    panic!("staged command exceeded the example's finite validation bound");
}
fn operation(
    control: &mut Control<Fill, Domain>,
    runtime: &mut GraphRuntime<Fill, Domain>,
    op: Op,
    output: &mut [[f32; 16]; 2],
) -> Execution<Op, Reply> {
    let at = runtime.next_frame();
    let completion = apply(
        control,
        runtime,
        Command::Schedule {
            at,
            action: Action::Planner {
                planner: planner(),
                operation: op,
            },
            payload: Box::default(),
        },
        output,
    );
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    let sequence = completion.sequence;
    drop(completion);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.sequence, sequence);
    execution
}
fn stage(
    control: &mut Control<Fill, Domain>,
    runtime: &mut GraphRuntime<Fill, Domain>,
    output: &mut [[f32; 16]; 2],
) -> StageId {
    let plan = compile_planned(runtime.catalog(), &graph(true), &bindings(true)).unwrap();
    let epoch = runtime.epoch();
    let completion = apply(
        control,
        runtime,
        // Stage the candidate; only a later planner permit publishes it at one boundary.
        Command::stage_graph(0, epoch, plan),
        output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    let Command::StageGraph {
        plan: None,
        id: Some(id),
        ..
    } = &completion.request
    else {
        panic!("not a Ready stage")
    };
    let id = *id;
    drop(completion);
    assert_eq!(runtime.epoch(), epoch);
    id
}

fn exact_cut(partition: &[usize]) -> Result<(), Error> {
    let (mut control, mut runtime) = prepare(true)?;
    let mut output = [[0.0; 16]; 2];
    let id = stage(&mut control, &mut runtime, &mut output);
    assert_eq!([output[0][0], output[1][0]], [0.5, 0.25]);
    let status = operation(&mut control, &mut runtime, Op::Inspect(id), &mut output);
    assert_eq!(
        status.operation_result,
        Some(Reply::Status(StageStatus::Ready {
            validated_epoch: Epoch(1)
        }))
    );
    for (attempt, expected, result, reply) in [
        (Attempt::Unused, Epoch(1), Ok(()), Some(Reply::Unused)),
        (
            Attempt::Refuse,
            Epoch(1),
            Err(Error::InvalidParameter),
            None,
        ),
        (Attempt::Publish, Epoch(0), Err(Error::StaleEpoch), None),
    ] {
        let execution = operation(
            &mut control,
            &mut runtime,
            Op::Try {
                id,
                expected,
                attempt,
            },
            &mut output,
        );
        assert_eq!(execution.result, result);
        assert_eq!(execution.operation_result, reply);
        assert_eq!(runtime.epoch(), Epoch(1));
        assert_eq!([output[0][0], output[1][0]], [0.5, 0.25]);
    }
    let cut = runtime.next_frame().checked_add(4)?;
    let delivered = runtime.next_frame();
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::Schedule {
            at: delivered,
            action: Action::Planner {
                planner: planner(),
                operation: Op::Defer {
                    id,
                    expected: Epoch(1),
                    at: cut,
                },
            },
            payload: Box::default(),
        },
        &mut output,
    );
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    let sequence = completion.sequence;
    drop(completion);
    assert!(control.try_execution().is_none());
    let start = runtime.next_frame();
    let mut rendered = 0;
    for &frames in partition {
        let frame = runtime.next_frame();
        if frame == cut {
            render(&mut runtime, 0, &mut output)?;
            assert_eq!(runtime.epoch(), Epoch(1));
            assert!(control.try_execution().is_none());
        }
        render(&mut runtime, frames, &mut output)?;
        for offset in 0..frames {
            let expected = if frame.checked_add(offset)? < cut {
                [0.5, 0.25]
            } else {
                [0.75, 0.125]
            };
            assert_eq!([output[0][offset], output[1][offset]], expected);
        }
        rendered += frames;
    }
    assert_eq!(rendered, 8);
    assert_eq!(start.checked_add(3)?, cut);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.sequence, sequence);
    assert_eq!(execution.delivered_frame, Some(delivered));
    assert_eq!(execution.actual_frame, cut);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(execution.operation_result, Some(Reply::Activated(Epoch(2))));
    assert_eq!(runtime.epoch(), Epoch(2));
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::ReleaseGraph { id },
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(
        matches!(&completion.retired, Some(Retired::Plan(plan)) if plan.planning().bindings == bindings(false))
    );
    drop(completion);
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::ReleaseGraph { id },
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Rejected(Error::InvalidId));
    drop(completion);
    let next = stage(&mut control, &mut runtime, &mut output);
    assert_ne!(next, id);
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::ReleaseGraph { id: next },
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(&completion.retired, Some(Retired::Plan(_))));
    drop(completion);
    assert_eq!(control.outstanding(), 0);
    let (domain, resources) = runtime.into_planner_and_resources();
    assert_eq!(domain.values, [0.75, 0.125]);
    assert_eq!(domain.epoch, Epoch(2));
    assert!(domain.swapped && domain.pending.is_none());
    drop((domain, resources, control));
    Ok(())
}
fn initially_empty() -> Result<(), Error> {
    let (mut control, mut runtime) = prepare(false)?;
    let mut output = [[0.0; 16]; 2];
    let id = stage(&mut control, &mut runtime, &mut output);
    assert_eq!([output[0][0], output[1][0]], [0.0; 2]);
    let execution = operation(
        &mut control,
        &mut runtime,
        Op::Try {
            id,
            expected: Epoch(0),
            attempt: Attempt::Publish,
        },
        &mut output,
    );
    assert_eq!(execution.operation_result, Some(Reply::Activated(Epoch(1))));
    assert_eq!([output[0][0], output[1][0]], [0.75, 0.125]);
    let completion = apply(
        &mut control,
        &mut runtime,
        Command::ReleaseGraph { id },
        &mut output,
    );
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(completion.retired.is_none());
    drop(completion);
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    Ok(())
}
fn main() -> Result<(), Error> {
    exact_cut(&[8])?;
    exact_cut(&[3, 5])?;
    exact_cut(&[1; 8])?;
    exact_cut(&[2, 4, 2])?;
    initially_empty()?;
    println!("staged routes: unpublished permits, exact route/binding cuts, checked epochs and owning retirement verified");
    Ok(())
}
