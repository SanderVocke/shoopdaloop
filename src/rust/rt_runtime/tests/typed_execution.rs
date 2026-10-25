mod support;
use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{
    compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType,
};
use rt_runtime::planner::*;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::schedule::{Action, Execution, ReceiptToken};
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Sequence};
use shoop_wasm_test_support::shoop_test;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Set(u32),
    Delay { until: u64, value: u32 },
    Cancel(Sequence),
    Stop,
    Payload,
    Node(u32),
    Refuse,
    BadRecovery,
    StopOnPlan,
    FailFull,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reply {
    Applied(u32),
    Cancelled,
    Stopped,
    Refused,
}
struct Domain {
    value: [u32; 1],
    pending: [Option<(ReceiptToken, Frame, u32)>; 4],
    recovery: Option<ReceiptToken>,
}
impl Planner for Domain {
    type Decision = u32;
    type Observation = ();
    type Operation = Op;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 0,
            operation_bytes: 4,
        })
    }
    fn decisions(&self) -> &[u32] {
        &self.value
    }
    fn recovery_safe(operation: Op) -> bool {
        matches!(
            operation,
            Op::Cancel(_) | Op::Stop | Op::StopOnPlan | Op::BadRecovery
        )
    }
    fn operation(
        &mut self,
        operation: Op,
        mut context: OperationContext<'_, Op, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        match operation {
            Op::Set(value) => {
                self.value[0] = value;
                Ok(Disposition::Complete(Reply::Applied(value)))
            }
            Op::Payload => {
                let bytes: [u8; 4] = context
                    .payload
                    .try_into()
                    .map_err(|_| Error::InvalidEvent)?;
                self.value[0] = u32::from_le_bytes(bytes);
                Ok(Disposition::Complete(Reply::Applied(self.value[0])))
            }
            Op::Delay { until, value } => {
                if until < context.frame.0 {
                    return Err(Error::InvalidFrame);
                }
                let slot = self
                    .pending
                    .iter_mut()
                    .find(|slot| slot.is_none())
                    .ok_or(Error::Full)?;
                *slot = Some((context.token, Frame(until), value));
                Ok(Disposition::Pending)
            }
            Op::Cancel(sequence) => {
                if let Some(slot) = self
                    .pending
                    .iter_mut()
                    .find(|slot| slot.is_some_and(|(token, _, _)| token.sequence() == sequence))
                {
                    let (token, _, _) = slot.take().unwrap();
                    context.receipts.complete(token, Ok(Reply::Cancelled))?;
                } else {
                    context.receipts.cancel_queued(sequence, Reply::Cancelled)?;
                }
                Ok(Disposition::Complete(Reply::Cancelled))
            }
            Op::Stop => {
                for pending in &mut self.pending {
                    if let Some((token, _, _)) = pending.take() {
                        context.receipts.complete(token, Ok(Reply::Cancelled))?;
                    }
                }
                self.value[0] = 0;
                Ok(Disposition::Complete(Reply::Stopped))
            }
            Op::Refuse => Ok(Disposition::Complete(Reply::Refused)),
            Op::StopOnPlan => {
                assert!(context.recovery);
                assert!(self.recovery.is_none());
                self.recovery = Some(context.token);
                Ok(Disposition::Pending)
            }
            Op::FailFull => Err(Error::Full),
            Op::BadRecovery => Ok(Disposition::Pending),
            Op::Node(_) => Err(Error::InvalidEvent),
        }
    }
    fn plan(&mut self, mut context: PlanContext<'_, Op, Reply>) -> Result<Frame, Error> {
        if let Some(recovery) = self.recovery.take() {
            for pending in &mut self.pending {
                if let Some((token, _, _)) = pending.take() {
                    context.receipts.complete(token, Ok(Reply::Cancelled))?;
                }
            }
            self.value[0] = 0;
            context.receipts.complete(recovery, Ok(Reply::Stopped))?;
        }
        for pending in &mut self.pending {
            if pending.is_some_and(|(_, at, _)| at == context.frame) {
                let (token, _, value) = pending.take().unwrap();
                self.value[0] = value;
                context
                    .receipts
                    .complete(token, Ok(Reply::Applied(value)))?;
            }
        }
        Ok(context.frame.checked_add(1)?.min(context.ceiling))
    }
    fn finish(&mut self, context: FinishContext<'_, (), Op, Reply>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        Ok(())
    }
}
impl Drop for Domain {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}
struct Gain {
    factor: u32,
}
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
            observations: 0,
            operation_bytes: 4,
        }
    }
    fn operation(
        &mut self,
        operation: Op,
        context: rt_runtime::processor::OperationContext<'_>,
    ) -> Result<Reply, Error> {
        assert!(context.frame >= context.requested_frame);
        if let Op::Node(value) = operation {
            if !context.payload.is_empty() {
                return Err(Error::InvalidEvent);
            }
            self.factor = value;
            Ok(Reply::Applied(value))
        } else {
            Err(Error::InvalidEvent)
        }
    }
    fn set_parameter(&mut self, _: u32, value: f32) -> Result<(), Error> {
        self.factor = value as u32;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Domain>) -> Result<(), Error> {
        let gain = *block.decision.as_ref().unwrap().value * self.factor;
        let (input, output) = block.audio.pair(0, 0).unwrap();
        for (input, output) in input.iter().zip(output) {
            *output = *input * gain as f32;
        }
        Ok(())
    }
}
impl Drop for Gain {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
    }
}
fn config(events: usize) -> rt_runtime::PreparedConfig {
    Config {
        max_nodes: 1,
        max_ports: 4,
        max_edges: 2,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        midi_events_per_port: 4,
        midi_bytes_per_port: 8,
        resource_slots: 4,
        resource_pages: 4,
        page_bytes: 64,
        command_slots: 8,
        scheduled_events: events,
        budgets: Budgets {
            events,
            subranges: 0,
            transfer_bytes: 128,
            validation_steps: 64,
            ..Budgets::default()
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
struct Harness {
    control: Control<Gain, Domain>,
    runtime: GraphRuntime<Gain, Domain>,
    output: [f32; 128],
}
impl Harness {
    fn new(events: usize) -> Self {
        let (control, runtime) = GraphRuntime::prepare_with_planner(
            config(events),
            vec![PreparedNode::new(node(), Gain { factor: 1 })],
            Domain {
                value: [1],
                pending: [None; 4],
                recovery: None,
            },
        )
        .unwrap();
        let mut harness = Self {
            control,
            runtime,
            output: [0.0; 128],
        };
        harness.graph(true);
        harness
    }
    fn graph(&mut self, active: bool) {
        let host = Endpoint {
            node: None,
            kind: PortType::Audio,
            port: 0,
        };
        let port = Endpoint::node(node(), PortType::Audio, 0);
        let graph = if active {
            GraphDescription {
                nodes: vec![node()],
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
        } else {
            GraphDescription::default()
        };
        let bindings = if active {
            vec![DecisionBinding {
                planner: planner(),
                consumer: node(),
                decision_slot: 0,
            }]
        } else {
            vec![]
        };
        let plan = compile_planned(self.runtime.catalog(), &graph, &bindings).unwrap();
        self.submit(Lane::Normal, Command::commit(self.runtime.epoch(), plan));
    }
    fn submit(
        &mut self,
        lane: Lane,
        command: Command<Gain, Domain>,
    ) -> rt_runtime::transport::Ticket {
        self.control
            .submit(lane, command)
            .unwrap_or_else(|error| panic!("{:?}", error.error))
    }
    fn operation(&mut self, lane: Lane, at: u64, operation: Op, payload: &[u8]) -> Sequence {
        self.submit(
            lane,
            Command::Schedule {
                at: Frame(at),
                action: Action::Planner {
                    planner: planner(),
                    operation,
                },
                payload: payload.into(),
            },
        )
        .sequence()
    }
    fn step(&mut self, frames: usize) -> Result<(), Error> {
        let frame = self.runtime.next_frame();
        assert_no_heap(|| {
            self.runtime.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[&[1.0; 128]],
                    outputs: &mut [&mut self.output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
    }
    fn replies(&mut self) -> Vec<Execution<Op, Reply>> {
        std::iter::from_fn(|| self.control.try_execution()).collect()
    }
    fn drain(&mut self) -> Vec<Outcome> {
        std::iter::from_fn(|| self.control.try_completion())
            .map(|completion| completion.outcome)
            .collect()
    }
}

#[shoop_test]
fn pending_and_immediate_typed_results_and_samples_are_partition_invariant() {
    fn run(partitions: &[usize]) -> (Vec<f32>, Vec<Execution<Op, Reply>>) {
        let mut h = Harness::new(3);
        h.operation(Lane::Normal, 0, Op::Delay { until: 6, value: 3 }, &[]);
        h.submit(
            Lane::Normal,
            Command::Schedule {
                at: Frame(9),
                action: Action::Processor {
                    node: node(),
                    operation: Op::Node(4),
                },
                payload: Box::new([]),
            },
        );
        h.operation(Lane::Normal, 11, Op::Set(2), &[]);
        let mut samples = Vec::new();
        for &frames in partitions {
            h.step(frames).unwrap();
            samples.extend_from_slice(&h.output[..frames]);
        }
        for (frame, sample) in samples.iter().enumerate() {
            assert_eq!(
                *sample,
                if frame < 6 {
                    1.0
                } else if frame < 9 {
                    3.0
                } else if frame < 11 {
                    12.0
                } else {
                    8.0
                }
            );
        }
        (samples, h.replies())
    }
    let reference = run(&[64]);
    assert_eq!(
        reference
            .1
            .iter()
            .map(|r| r.actual_frame.0)
            .collect::<Vec<_>>(),
        [6, 9, 11]
    );
    assert_eq!(reference, run(&[1; 64]));
    assert_eq!(reference, run(&[6, 3, 2, 17, 36]));
}

#[shoop_test]
fn delivered_pending_request_completes_at_its_musical_boundary_not_at_delivery() {
    let mut h = Harness::new(1);
    h.step(8).unwrap();
    h.drain();
    let sequence = h.operation(
        Lane::Normal,
        2,
        Op::Delay {
            until: 12,
            value: 5,
        },
        &[],
    );
    h.step(4).unwrap();
    assert_eq!(h.output[..4], [1.0; 4]);
    assert!(h.replies().is_empty());
    assert_eq!(
        h.drain(),
        [Outcome::Scheduled {
            effective_frame: Frame(8),
            late_by: 6
        }]
    );
    h.step(0).unwrap();
    assert!(h.replies().is_empty());
    h.step(4).unwrap();
    assert_eq!(h.output[..4], [5.0; 4]);
    let replies = h.replies();
    assert_eq!(replies.len(), 1);
    let result = replies[0];
    assert_eq!(result.sequence, sequence);
    assert_eq!(result.delivered_frame, Some(Frame(8)));
    assert_eq!(result.actual_frame, Frame(12));
    assert_eq!(result.late_by(), 6);
    assert_eq!(result.operation_result, Some(Reply::Applied(5)));
}

#[shoop_test]
fn recovery_stop_cancels_pending_work_with_every_normal_receipt_reserved() {
    let mut h = Harness::new(1);
    let pending = h.operation(
        Lane::Normal,
        0,
        Op::Delay {
            until: 1000,
            value: 9,
        },
        &[],
    );
    h.step(8).unwrap();
    h.drain();
    h.operation(Lane::Normal, 8, Op::Set(7), &[]);
    let recovery = h.operation(Lane::Recovery, 8, Op::Stop, &[]);
    h.step(128).unwrap();
    assert_eq!(h.output, [0.0; 128]);
    assert_eq!(h.runtime.diagnostics().subranges, 128);
    let completions = h.drain();
    assert!(completions.contains(&Outcome::Rejected(Error::Full)));
    assert!(completions.contains(&Outcome::Scheduled {
        effective_frame: Frame(8),
        late_by: 0
    }));
    let replies = h.replies();
    assert_eq!(replies.len(), 2);
    assert_eq!(
        (replies[0].sequence, replies[0].operation_result),
        (pending, Some(Reply::Cancelled))
    );
    assert_eq!(
        (replies[1].sequence, replies[1].operation_result),
        (recovery, Some(Reply::Stopped))
    );
    assert_eq!(replies[1].lane, Lane::Recovery);
    h.operation(Lane::Recovery, 136, Op::Stop, &[]);
    h.step(1).unwrap();
    assert_eq!(h.replies()[0].operation_result, Some(Reply::Stopped));
    let mut together = Harness::new(1);
    together.operation(
        Lane::Normal,
        0,
        Op::Delay {
            until: 100,
            value: 9,
        },
        &[],
    );
    together.operation(Lane::Recovery, 0, Op::Stop, &[]);
    together.step(8).unwrap();
    assert_eq!(together.output[..8], [0.0; 8]);
    assert_eq!(together.runtime.diagnostics().scheduled_events, 2);
    assert_eq!(
        together
            .replies()
            .iter()
            .map(|r| r.operation_result)
            .collect::<Vec<_>>(),
        [Some(Reply::Cancelled), Some(Reply::Stopped)]
    );
}

#[shoop_test]
fn recovery_may_finish_during_planning_without_a_normal_pending_slot() {
    let mut h = Harness::new(4);
    for value in 2..6 {
        h.operation(Lane::Normal, 0, Op::Delay { until: 1000, value }, &[]);
    }
    h.step(8).unwrap();
    h.drain();
    assert!(h.replies().is_empty());
    h.operation(Lane::Recovery, 8, Op::StopOnPlan, &[]);
    h.step(8).unwrap();
    assert_eq!(h.output[..8], [0.0; 8]);
    let results = h.replies();
    assert_eq!(results.len(), 5);
    assert!(results[..4]
        .iter()
        .all(|result| result.operation_result == Some(Reply::Cancelled)));
    assert_eq!(results[4].operation_result, Some(Reply::Stopped));
    assert_eq!(results[4].actual_frame, Frame(8));
}

#[shoop_test]
fn recovery_can_cancel_a_future_queued_typed_operation_without_delivering_it() {
    let mut h = Harness::new(1);
    let future = h.operation(Lane::Normal, 100, Op::Set(9), &[]);
    h.step(8).unwrap();
    h.drain();
    h.operation(Lane::Recovery, 8, Op::Cancel(future), &[]);
    h.step(128).unwrap();
    assert_eq!(h.output, [1.0; 128]);
    let replies = h.replies();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].sequence, future);
    assert_eq!(replies[0].delivered_frame, None);
    assert_eq!(replies[0].actual_frame, Frame(8));
    assert_eq!(replies[0].operation_result, Some(Reply::Cancelled));
}

#[shoop_test]
fn unread_normal_results_do_not_block_recovery_or_automatic_intervals() {
    let mut h = Harness::new(1);
    h.operation(Lane::Normal, 0, Op::Set(4), &[]);
    h.step(1).unwrap();
    h.drain();
    h.operation(Lane::Recovery, 1, Op::Stop, &[]);
    h.step(128).unwrap();
    h.drain();
    h.operation(Lane::Recovery, 129, Op::Stop, &[]);
    h.step(1).unwrap();
    assert_eq!(h.drain(), [Outcome::Rejected(Error::Full)]);
    assert_eq!(h.output[0], 0.0);
    assert_eq!(h.replies().len(), 2);
}

#[shoop_test]
fn typed_processor_and_planner_actions_share_exact_frame_ordering_with_parameters() {
    let mut h = Harness::new(3);
    h.submit(
        Lane::Normal,
        Command::Schedule {
            at: Frame(7),
            action: Action::Processor {
                node: node(),
                operation: Op::Node(3),
            },
            payload: Box::new([]),
        },
    );
    h.operation(Lane::Normal, 7, Op::Set(2), &[]);
    h.submit(
        Lane::Normal,
        Command::Schedule {
            at: Frame(7),
            action: Action::Parameter {
                node: node(),
                parameter: 0,
                value: 2.0,
            },
            payload: Box::new([]),
        },
    );
    h.step(16).unwrap();
    assert_eq!(h.output[..7], [1.0; 7]);
    assert_eq!(h.output[7..16], [6.0; 9]);
    let replies = h.replies();
    assert_eq!(replies.len(), 3);
    assert!(matches!(replies[0].action, Action::Planner { .. }));
    assert!(matches!(replies[1].action, Action::Parameter { .. }));
    assert!(matches!(replies[2].action, Action::Processor { .. }));
    assert!(replies
        .iter()
        .all(|result| result.actual_frame == Frame(7) && result.result == Ok(())));
    assert_eq!(replies[1].operation_result, None);
    assert_eq!(replies[2].operation_result, Some(Reply::Applied(3)));
}

#[shoop_test]
fn payload_limits_malformed_operations_and_domain_refusal_return_distinct_outcomes() {
    let mut h = Harness::new(2);
    let oversized = h.operation(Lane::Normal, 0, Op::Payload, &[0; 5]);
    let malformed = h.operation(Lane::Normal, 0, Op::Payload, &[1, 2]);
    h.operation(Lane::Normal, 0, Op::Refuse, &[]);
    h.step(4).unwrap();
    let mut retained = false;
    while let Some(completion) = h.control.try_completion() {
        if completion.sequence == oversized {
            assert_eq!(completion.outcome, Outcome::Rejected(Error::Capacity));
            let Command::Schedule { payload, .. } = completion.request else {
                panic!("wrong owner")
            };
            assert_eq!(&*payload, &[0; 5]);
            retained = true;
        }
    }
    assert!(retained);
    let replies = h.replies();
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].sequence, malformed);
    assert_eq!(replies[0].result, Err(Error::InvalidEvent));
    assert_eq!(replies[0].operation_result, None);
    assert_eq!(replies[1].result, Ok(()));
    assert_eq!(replies[1].operation_result, Some(Reply::Refused));
    h.operation(Lane::Normal, 4, Op::Payload, &6u32.to_le_bytes());
    h.step(4).unwrap();
    assert_eq!(h.output[..4], [6.0; 4]);
    h.operation(Lane::Normal, 8, Op::FailFull, &[]);
    h.step(1).unwrap();
    assert_eq!(h.runtime.diagnostics().midi_overflows, 0);
    assert_eq!(h.output[0], 6.0);
    assert_eq!(h.replies().last().unwrap().result, Err(Error::Full));
}

#[shoop_test]
fn recovery_rejects_unsafe_future_payload_and_nonterminating_operations() {
    for (at, operation, payload, error) in [
        (0, Op::Set(2), vec![], Error::InvalidState),
        (1, Op::Stop, vec![], Error::InvalidEvent),
        (0, Op::Stop, vec![0], Error::InvalidEvent),
    ] {
        let mut h = Harness::new(1);
        h.operation(Lane::Recovery, at, operation, &payload);
        h.step(4).unwrap();
        assert!(h.drain().contains(&Outcome::Rejected(error)));
        assert!(h.replies().is_empty());
        assert_eq!(h.output[..4], [1.0; 4]);
    }
    let mut h = Harness::new(1);
    h.operation(Lane::Recovery, 0, Op::BadRecovery, &[]);
    assert_eq!(h.step(4), Err(Error::InvalidState));
    assert_eq!(h.output[..4], [0.0; 4]);
    assert!(h.replies().is_empty());
    assert_eq!(h.runtime.next_frame(), Frame(4));
}

#[shoop_test]
fn transport_cancellation_precedes_delivery_and_stale_planner_ids_are_rejected() {
    let mut h = Harness::new(1);
    let ticket = h.submit(
        Lane::Normal,
        Command::Schedule {
            at: Frame(0),
            action: Action::Planner {
                planner: planner(),
                operation: Op::Set(8),
            },
            payload: Box::new([]),
        },
    );
    ticket.cancel();
    h.submit(
        Lane::Normal,
        Command::Schedule {
            at: Frame(0),
            action: Action::Planner {
                planner: PlannerId::from_parts(0, 2).unwrap(),
                operation: Op::Set(7),
            },
            payload: Box::new([]),
        },
    );
    h.step(8).unwrap();
    let outcomes = h.drain();
    assert!(outcomes.contains(&Outcome::Cancelled));
    assert!(outcomes.contains(&Outcome::Rejected(Error::InvalidId)));
    assert!(h.replies().is_empty());
    assert_eq!(h.output[..8], [1.0; 8]);
}

#[shoop_test]
fn removed_processor_generation_returns_an_execution_failure_and_retired_owner() {
    let mut h = Harness::new(1);
    h.step(1).unwrap();
    h.drain();
    h.submit(
        Lane::Normal,
        Command::Schedule {
            at: Frame(8),
            action: Action::Processor {
                node: node(),
                operation: Op::Node(8),
            },
            payload: Box::new([]),
        },
    );
    h.graph(false);
    h.step(1).unwrap();
    h.drain();
    h.submit(Lane::Normal, Command::Remove { node: node() });
    h.step(1).unwrap();
    let completion = h.control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Processor(_))));
    h.submit(
        Lane::Normal,
        Command::Install {
            prepared: Some(PreparedNode::new(
                node().checked_next_generation().unwrap(),
                Gain { factor: 2 },
            )),
        },
    );
    h.step(8).unwrap();
    let result = h.replies();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].actual_frame, Frame(8));
    assert_eq!(result[0].result, Err(Error::InvalidId));
    assert_eq!(h.runtime.epoch(), Epoch(4));
}
