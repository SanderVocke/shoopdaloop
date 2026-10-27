//! Typed domain outcomes, pending receipts and bounded cancellation/recovery.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::{Connection, Endpoint, GraphDescription, PortCounts, PortType};
use rt_runtime::planner::{
    ConsumerAccess, DecisionBinding, Disposition, FinishContext, OperationContext, PlanContext,
    Planner, PlannerSpec,
};
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::ResourceStore;
use rt_runtime::schedule::{Action, ReceiptToken};
use rt_runtime::transport::{Lane, Outcome, Ticket};
use rt_runtime::{Budgets, Config, Error, Frame, NodeId, PlannerId, Sequence};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Wait { until: Frame, value: f32 },
    Multiply(f32),
    Cancel(Sequence),
    Stop,
    Payload,
    Refuse,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reply {
    Applied(f32),
    Cancelled,
    Stopped,
    Refused,
}

struct Domain {
    value: [f32; 1],
    pending: Option<(ReceiptToken, Frame, f32)>,
    recovery: Option<ReceiptToken>,
}
impl Planner for Domain {
    type Decision = f32;
    type Observation = ();
    type Operation = Op;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: PlannerId::from_parts(0, 1).unwrap(),
            decision_slots: 1,
            observation_capacity: 0,
            operation_bytes: 4,
        })
    }
    fn decisions(&self) -> &[f32] {
        &self.value
    }
    fn recovery_safe(operation: Op) -> bool {
        matches!(operation, Op::Cancel(_) | Op::Stop)
    }
    fn operation(
        &mut self,
        operation: Op,
        mut context: OperationContext<'_, Op, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        if !matches!(operation, Op::Payload) && !context.payload.is_empty() {
            return Err(Error::InvalidEvent);
        }
        match operation {
            Op::Wait { until, value } => {
                if until < context.frame || !value.is_finite() {
                    return Err(Error::InvalidEvent);
                }
                if self.pending.is_some() {
                    return Ok(Disposition::Complete(Reply::Refused));
                }
                self.pending = Some((context.token, until, value));
                Ok(Disposition::Pending)
            }
            Op::Cancel(sequence) => {
                if self
                    .pending
                    .is_some_and(|(token, _, _)| token.sequence() == sequence)
                {
                    let (token, _, _) = self.pending.take().unwrap();
                    context.receipts.complete(token, Ok(Reply::Cancelled))?;
                    assert_eq!(
                        context.receipts.complete(token, Ok(Reply::Cancelled)),
                        Err(Error::InvalidState)
                    );
                } else {
                    context.receipts.cancel_queued(sequence, Reply::Cancelled)?;
                }
                Ok(Disposition::Complete(Reply::Cancelled))
            }
            Op::Stop => {
                if !context.recovery || self.recovery.is_some() {
                    return Err(Error::InvalidEvent);
                }
                self.recovery = Some(context.token);
                Ok(Disposition::Pending)
            }
            Op::Payload => {
                let bytes: [u8; 4] = context
                    .payload
                    .try_into()
                    .map_err(|_| Error::InvalidEvent)?;
                let value = f32::from_le_bytes(bytes);
                if !value.is_finite() {
                    return Err(Error::InvalidParameter);
                }
                self.value[0] = value;
                Ok(Disposition::Complete(Reply::Applied(value)))
            }
            Op::Refuse => Ok(Disposition::Complete(Reply::Refused)),
            Op::Multiply(_) => Err(Error::InvalidEvent),
        }
    }
    fn plan(&mut self, mut context: PlanContext<'_, Op, Reply>) -> Result<Frame, Error> {
        if let Some(recovery) = self.recovery.take() {
            if let Some((token, _, _)) = self.pending.take() {
                context.receipts.complete(token, Ok(Reply::Cancelled))?;
            }
            self.value[0] = 0.0;
            context.receipts.complete(recovery, Ok(Reply::Stopped))?;
        }
        if self
            .pending
            .is_some_and(|(_, until, _)| until == context.frame)
        {
            let (token, _, value) = self.pending.take().unwrap();
            self.value[0] = value;
            context
                .receipts
                .complete(token, Ok(Reply::Applied(value)))?;
        }
        context
            .frame
            .checked_add(1)
            .map(|end| end.min(context.ceiling))
    }
    fn finish(&mut self, context: FinishContext<'_, (), Op, Reply>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        Ok(())
    }
}

struct Gain(f32);
impl Processor<Domain> for Gain {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn planning(&self) -> ConsumerAccess {
        ConsumerAccess {
            decision_required: true,
            ..ConsumerAccess::default()
        }
    }
    fn operation(
        &mut self,
        operation: Op,
        context: rt_runtime::processor::OperationContext<'_>,
    ) -> Result<Reply, Error> {
        let Op::Multiply(value) = operation else {
            return Err(Error::InvalidEvent);
        };
        if !value.is_finite() || !context.payload.is_empty() {
            return Err(Error::InvalidEvent);
        }
        self.0 = value;
        Ok(Reply::Applied(value))
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Domain>) -> Result<(), Error> {
        let gain = *block.decision.as_ref().ok_or(Error::InvalidState)?.value * self.0;
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = input * gain;
        }
        Ok(())
    }
}

fn prepare() -> Result<(Control<Gain, Domain>, GraphRuntime<Gain, Domain>), Error> {
    let config = Config {
        max_frames: 32,
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
        scheduled_events: 1,
        budgets: Budgets {
            events: 1,
            subranges: 0,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()?;
    let node = NodeId::from_parts(0, 1)?;
    let id = PlannerId::from_parts(0, 1)?;
    let port = Endpoint::node(node, PortType::Audio, 0);
    let host = Endpoint::host(PortType::Audio, 0);
    GraphRuntime::prepare_with_initial_graph(
        config,
        vec![PreparedNode::new(node, Gain(1.0))],
        Domain {
            value: [1.0],
            pending: None,
            recovery: None,
        },
        ResourceStore::new(config)?,
        &GraphDescription {
            nodes: vec![node],
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
        },
        &[DecisionBinding {
            planner: id,
            consumer: node,
            decision_slot: 0,
        }],
    )
}

fn send(
    control: &mut Control<Gain, Domain>,
    lane: Lane,
    at: Frame,
    operation: Op,
) -> Result<Ticket, Error> {
    control
        .submit(
            lane,
            Command::Schedule {
                at,
                action: Action::Planner {
                    planner: PlannerId::from_parts(0, 1)?,
                    operation,
                },
                payload: Box::default(),
            },
        )
        .map_err(|refusal| refusal.error)
}

fn render(
    runtime: &mut GraphRuntime<Gain, Domain>,
    frames: usize,
    output: &mut [f32; 32],
) -> Result<(), Error> {
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames,
                inputs: &[&[1.0; 32]],
                outputs: &mut [output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
}

fn delayed_result() -> Result<(), Error> {
    let (mut control, mut runtime) = prepare()?;
    let mut output = [0.0; 32];
    render(&mut runtime, 10, &mut output)?;
    let ticket = send(
        &mut control,
        Lane::Normal,
        Frame(1),
        Op::Wait {
            until: Frame(20),
            value: 3.0,
        },
    )?;
    render(&mut runtime, 10, &mut output)?;
    assert_eq!(output[..10], [1.0; 10]);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(
        completion.outcome,
        Outcome::Scheduled {
            effective_frame: Frame(10),
            late_by: 9
        }
    );
    drop(completion);
    assert!(control.try_execution().is_none());
    render(&mut runtime, 0, &mut output)?;
    assert!(control.try_execution().is_none());
    render(&mut runtime, 4, &mut output)?;
    assert_eq!(output[..4], [3.0; 4]);
    let execution = control.try_execution().unwrap();
    assert_eq!(execution.requested_frame, Frame(1));
    assert_eq!(execution.delivered_frame, Some(Frame(10)));
    assert_eq!(execution.actual_frame, Frame(20));
    assert_eq!(execution.late_by(), 9);
    assert_eq!(execution.result, Ok(()));
    assert_eq!(execution.operation_result, Some(Reply::Applied(3.0)));
    control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(25),
                action: Action::Processor {
                    node: NodeId::from_parts(0, 1)?,
                    operation: Op::Multiply(0.5),
                },
                payload: Box::default(),
            },
        )
        .map_err(|refusal| refusal.error)?;
    render(&mut runtime, 4, &mut output)?;
    assert_eq!(output[..4], [3.0, 1.5, 1.5, 1.5]);
    drop(control.try_completion().unwrap());
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Applied(0.5))
    );
    assert_eq!(control.outstanding(), 0);
    let (domain, resources) = runtime.into_planner_and_resources();
    assert!(domain.pending.is_none());
    assert!(domain.recovery.is_none());
    drop((domain, resources, control));
    Ok(())
}

fn cancellation_and_capacity() -> Result<(), Error> {
    let (mut control, mut runtime) = prepare()?;
    let mut output = [0.0; 32];
    let pending = send(
        &mut control,
        Lane::Normal,
        Frame(0),
        Op::Wait {
            until: Frame(1000),
            value: 9.0,
        },
    )?;
    render(&mut runtime, 1, &mut output)?;
    drop(control.try_completion().unwrap());
    let full = send(&mut control, Lane::Normal, Frame(1), Op::Refuse)?;
    let stop = send(&mut control, Lane::Recovery, Frame(1), Op::Stop)?;
    render(&mut runtime, 8, &mut output)?;
    assert_eq!(output[..8], [0.0; 8]);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, stop.sequence());
    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
    drop(completion);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, full.sequence());
    assert_eq!(completion.outcome, Outcome::Rejected(Error::Full));
    drop(completion);
    send(&mut control, Lane::Recovery, Frame(9), Op::Stop)?;
    render(&mut runtime, 1, &mut output)?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Rejected(Error::Full));
    drop(completion);
    let cancelled = control.try_execution().unwrap();
    assert_eq!(cancelled.sequence, pending.sequence());
    assert_eq!(cancelled.actual_frame, Frame(1));
    assert_eq!(cancelled.operation_result, Some(Reply::Cancelled));
    let stopped = control.try_execution().unwrap();
    assert_eq!(stopped.sequence, stop.sequence());
    assert_eq!(stopped.actual_frame, Frame(1));
    assert_eq!(stopped.operation_result, Some(Reply::Stopped));

    let future = send(
        &mut control,
        Lane::Normal,
        Frame(100),
        Op::Wait {
            until: Frame(1000),
            value: 8.0,
        },
    )?;
    render(&mut runtime, 1, &mut output)?;
    drop(control.try_completion().unwrap());
    let cut = runtime.next_frame();
    send(
        &mut control,
        Lane::Recovery,
        cut,
        Op::Cancel(future.sequence()),
    )?;
    render(&mut runtime, 1, &mut output)?;
    drop(control.try_completion().unwrap());
    let cancelled = control.try_execution().unwrap();
    assert_eq!(cancelled.sequence, future.sequence());
    assert_eq!(cancelled.delivered_frame, None);
    assert_eq!(cancelled.actual_frame, cut);
    assert_eq!(cancelled.operation_result, Some(Reply::Cancelled));
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Cancelled)
    );

    let ticket = send(&mut control, Lane::Normal, runtime.next_frame(), Op::Refuse)?;
    ticket.cancel();
    render(&mut runtime, 1, &mut output)?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Cancelled);
    drop(completion);
    assert!(control.try_execution().is_none());
    let pending = send(
        &mut control,
        Lane::Normal,
        runtime.next_frame(),
        Op::Wait {
            until: Frame(1000),
            value: 7.0,
        },
    )?;
    render(&mut runtime, 1, &mut output)?;
    drop(control.try_completion().unwrap());
    send(
        &mut control,
        Lane::Recovery,
        runtime.next_frame(),
        Op::Cancel(pending.sequence()),
    )?;
    render(&mut runtime, 1, &mut output)?;
    drop(control.try_completion().unwrap());
    let cancelled = control.try_execution().unwrap();
    assert_eq!(cancelled.sequence, pending.sequence());
    assert!(cancelled.delivered_frame.is_some());
    assert_eq!(cancelled.operation_result, Some(Reply::Cancelled));
    assert_eq!(
        control.try_execution().unwrap().operation_result,
        Some(Reply::Cancelled)
    );
    assert_eq!(control.outstanding(), 0);
    let (domain, resources) = runtime.into_planner_and_resources();
    assert!(domain.pending.is_none());
    assert!(domain.recovery.is_none());
    drop((domain, resources, control));
    Ok(())
}

fn refusal_results() -> Result<(), Error> {
    let (mut control, mut runtime) = prepare()?;
    let mut output = [0.0; 32];
    for (operation, payload, expected, domain) in [
        (Op::Payload, vec![1, 2, 3], Err(Error::InvalidEvent), None),
        (Op::Refuse, vec![], Ok(()), Some(Reply::Refused)),
        (Op::Multiply(2.0), vec![], Err(Error::InvalidEvent), None),
    ] {
        control
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at: runtime.next_frame(),
                    action: Action::Planner {
                        planner: PlannerId::from_parts(0, 1)?,
                        operation,
                    },
                    payload: payload.into_boxed_slice(),
                },
            )
            .map_err(|refusal| refusal.error)?;
        render(&mut runtime, 1, &mut output)?;
        let completion = control.try_completion().unwrap();
        assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
        drop(completion);
        let result = control.try_execution().unwrap();
        assert_eq!(result.result, expected);
        assert_eq!(result.operation_result, domain);
    }
    control
        .submit(
            Lane::Normal,
            Command::Schedule {
                at: runtime.next_frame(),
                action: Action::Planner {
                    planner: PlannerId::from_parts(0, 2)?,
                    operation: Op::Refuse,
                },
                payload: Box::default(),
            },
        )
        .map_err(|refusal| refusal.error)?;
    render(&mut runtime, 1, &mut output)?;
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Rejected(Error::InvalidId));
    drop(completion);
    assert!(control.try_execution().is_none());
    assert_eq!(control.outstanding(), 0);
    drop(runtime);
    drop(control);
    Ok(())
}

fn main() -> Result<(), Error> {
    delayed_result()?;
    cancellation_and_capacity()?;
    refusal_results()?;
    println!("pending operations: four frames, retained outcomes, three cancellation cuts and finite recovery verified");
    Ok(())
}
