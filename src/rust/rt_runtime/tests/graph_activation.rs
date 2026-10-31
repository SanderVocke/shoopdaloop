mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::{Command, Control, GraphRuntime, PreparedNode, Retired},
    graph::{
        self,
        activation::{GraphAccess, StageId, StageStatus},
        Connection, Endpoint, GraphDescription, PortCounts, PortType, PreparedPlan,
    },
    planner::*,
    processor::{ProcessBlock, Processor},
    resource::ResourceStore,
    schedule::{Action, Execution, ReceiptToken},
    transport::{self, Lane, Outcome},
    Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Sequence,
};
use shoop_wasm_test_support::shoop_test;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use support::assert_no_heap;
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
fn config(steps: usize) -> rt_runtime::PreparedConfig {
    Config {
        max_frames: 32,
        max_nodes: 3,
        max_ports: 12,
        max_edges: 8,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 1,
        midi_events_per_port: 64,
        midi_bytes_per_port: 192,
        resource_slots: 4,
        resource_pages: 8,
        page_bytes: 16,
        command_slots: 16,
        scheduled_events: 16,
        budgets: Budgets {
            commands: 16,
            events: 16,
            publications: 16,
            validation_steps: steps,
            transfer_bytes: 64,
            subranges: 0,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
fn description(second: u32, route_second: bool, reverse: bool) -> GraphDescription {
    let mut graph = GraphDescription {
        nodes: vec![node(0), node(second)],
        connections: [PortType::Audio, PortType::Midi]
            .into_iter()
            .map(|kind| Connection {
                source: Endpoint::node(node(if route_second { second } else { 0 }), kind, 0),
                destination: Endpoint::host(kind, 0),
            })
            .collect(),
    };
    if reverse {
        graph.nodes.reverse();
        graph.connections.reverse();
    }
    graph
}
fn bindings(second: u32, swapped: bool) -> Vec<DecisionBinding> {
    vec![
        DecisionBinding {
            planner: planner(),
            consumer: node(0),
            decision_slot: usize::from(swapped),
        },
        DecisionBinding {
            planner: planner(),
            consumer: node(second),
            decision_slot: usize::from(!swapped),
        },
    ]
}
fn oversized(plan: PreparedPlan) -> PreparedPlan {
    let mut descriptor = plan.descriptor().clone();
    descriptor.steps.reserve_exact(128);
    let oversized =
        PreparedPlan::from_planned_descriptor(plan.config(), descriptor, plan.planning().clone())
            .unwrap();
    assert!(oversized.storage_bytes().unwrap() > plan.storage_bytes().unwrap());
    oversized
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Activate {
        id: StageId,
        expected: Epoch,
        value: u32,
        refuse: bool,
    },
    Defer {
        id: StageId,
        expected: Epoch,
        value: u32,
        at: Frame,
    },
    Cancel,
    Inspect(StageId),
    Read,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Reply {
    Applied { epoch: Epoch, frame: Frame },
    Status(StageStatus),
    Cut { epoch: Epoch, values: [u32; 2] },
    Cancelled,
}
#[derive(Clone, Copy, Default)]
struct Observed {
    value: u32,
    injected: usize,
}
struct Domain {
    values: [u32; 2],
    live: u8,
    injected: [usize; 2],
    swapped: bool,
    epoch: Epoch,
    pending: Option<(ReceiptToken, StageId, Epoch, u32, Frame)>,
    log: [Option<(Epoch, [u32; 2])>; 128],
    counts: Arc<AtomicUsize>,
}
impl Domain {
    fn activate(
        &mut self,
        graphs: &mut GraphAccess<'_>,
        id: StageId,
        expected: Epoch,
        value: u32,
        refuse: bool,
    ) -> Result<Epoch, Error> {
        let permit = graphs.prepare_activation(id, expected)?;
        assert!(permit.current().is_some());
        if refuse
            || !permit.candidate().contains(node(0))
            || permit
                .candidate()
                .descriptor()
                .steps
                .iter()
                .any(|step| step.node.slot() > 1)
        {
            return Err(Error::InvalidParameter);
        }
        let other = value.checked_add(2).ok_or(Error::Exhausted)?;
        let swapped = permit
            .candidate()
            .planning()
            .bindings
            .iter()
            .find(|b| b.consumer == node(0))
            .ok_or(Error::InvalidGraph)?
            .decision_slot
            == 1;
        let epoch = permit.next_epoch();
        let live = 1 | (u8::from(permit.candidate().contains(node(1))) << 1);
        self.values = [value, other];
        self.live = live;
        self.swapped = swapped;
        self.epoch = epoch;
        assert_eq!(permit.commit(), epoch);
        Ok(epoch)
    }
}
impl Planner for Domain {
    type Decision = u32;
    type Observation = Observed;
    type Operation = Op;
    type Result = Reply;
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 2,
            observation_capacity: 3,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[u32] {
        &self.values
    }
    fn recovery_safe(operation: Op) -> bool {
        // Deliberately over-classified Activate proves the capability still denies a recovery swap.
        matches!(operation, Op::Cancel | Op::Activate { .. })
    }
    fn operation(
        &mut self,
        operation: Op,
        mut context: OperationContext<'_, Op, Reply>,
    ) -> Result<Disposition<Reply>, Error> {
        let reply = match operation {
            Op::Activate {
                id,
                expected,
                value,
                refuse,
            } => Reply::Applied {
                epoch: self.activate(&mut context.graphs, id, expected, value, refuse)?,
                frame: context.frame,
            },
            Op::Defer {
                id,
                expected,
                value,
                at,
            } => {
                if self.pending.is_some() || at < context.frame {
                    return Err(Error::InvalidState);
                }
                self.pending = Some((context.token, id, expected, value, at));
                return Ok(Disposition::Pending);
            }
            Op::Cancel => {
                if let Some((token, ..)) = self.pending.take() {
                    context.receipts.complete(token, Ok(Reply::Cancelled))?;
                }
                Reply::Cancelled
            }
            Op::Inspect(id) => Reply::Status(context.graphs.status(id)?),
            Op::Read => Reply::Cut {
                epoch: context.graph_epoch,
                values: self.values,
            },
        };
        Ok(Disposition::Complete(reply))
    }
    fn plan(&mut self, mut context: PlanContext<'_, Op, Reply>) -> Result<Frame, Error> {
        self.epoch = context.graph_epoch;
        if let Some((token, id, expected, value, at)) =
            self.pending.filter(|p| p.4 == context.frame)
        {
            let result = self.activate(&mut context.graphs, id, expected, value, false);
            context.receipts.complete(
                token,
                result.map(|epoch| Reply::Applied { epoch, frame: at }),
            )?;
            self.pending = None;
        }
        if let Some(row) = self.log.get_mut(context.frame.0 as usize) {
            *row = Some((self.epoch, self.values));
        }
        Ok(context.frame.checked_add(1)?.min(context.ceiling))
    }
    fn finish(&mut self, context: FinishContext<'_, Observed, Op, Reply>) -> Result<(), Error> {
        assert_eq!(context.observations.len(), self.live.count_ones() as usize);
        for observation in context.observations {
            let slot = observation.producer.slot() as usize;
            assert_eq!(
                observation.value.value,
                self.values[if self.swapped { 1 - slot } else { slot }]
            );
            assert_ne!(self.live & (1 << slot), 0);
            self.injected[slot] += observation.value.injected;
        }
        Ok(())
    }
}
impl Drop for Domain {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
        self.counts.fetch_add(1, Ordering::Relaxed);
    }
}
struct Fill {
    slot: u32,
    counts: Arc<AtomicUsize>,
}
impl Processor<Domain> for Fill {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
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
        let decision = block.decision.ok_or(Error::InvalidState)?;
        assert_eq!(decision.interval.start, block.frame);
        let value = *decision.value;
        block
            .audio
            .output(0)
            .unwrap()
            .fill((value + self.slot * 100) as f32);
        let injected = block.midi_inputs[0].len();
        for index in 0..injected {
            let (offset, bytes) = block.midi_inputs[0].get(index).unwrap();
            block.midi_outputs[0].push(offset, block.frames, bytes)?;
        }
        for frame in 0..block.frames {
            block.midi_outputs[0].push(
                frame as u32,
                block.frames,
                &[0x90, 60 + self.slot as u8, value as u8],
            )?;
        }
        block.observations.push(Observed { value, injected })
    }
}
impl Drop for Fill {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
        self.counts.fetch_add(1, Ordering::Relaxed);
    }
}
type Completion = transport::Completion<Command<Fill, Domain>, Retired<Fill>>;
struct Harness {
    control: Control<Fill, Domain>,
    runtime: GraphRuntime<Fill, Domain>,
    midi: [MidiBuffer; 1],
    samples: Vec<f32>,
    packets: Vec<(u64, [u8; 3])>,
    counts: Arc<AtomicUsize>,
    nodes: usize,
}
impl Harness {
    fn new(steps: usize, reverse: bool, extra: bool) -> Self {
        let cfg = config(steps);
        let counts = Arc::new(AtomicUsize::new(0));
        let domain = Domain {
            values: [3, 7],
            live: 3,
            injected: [0; 2],
            swapped: false,
            epoch: Epoch(1),
            pending: None,
            log: [None; 128],
            counts: counts.clone(),
        };
        let mut nodes: Vec<_> = (0..if extra { 3 } else { 2 })
            .map(|slot| {
                PreparedNode::new(
                    node(slot),
                    Fill {
                        slot,
                        counts: counts.clone(),
                    },
                )
            })
            .collect();
        if reverse {
            nodes.reverse();
        }
        let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
            cfg,
            nodes,
            domain,
            ResourceStore::new(cfg).unwrap(),
            &description(1, false, reverse),
            &bindings(1, false),
        )
        .unwrap();
        runtime.prepare_graph_staging(2).unwrap();
        control.prepare_submissions().unwrap();
        Self {
            control,
            runtime,
            midi: [MidiBuffer::new(64, 192).unwrap()],
            samples: Vec::new(),
            packets: Vec::new(),
            counts,
            nodes: if extra { 3 } else { 2 },
        }
    }
    fn plan(&self, new: bool) -> PreparedPlan {
        graph::compile_planned(
            self.runtime.catalog(),
            &description(1, new, false),
            &bindings(1, new),
        )
        .unwrap()
    }
    fn send(&mut self, lane: Lane, command: Command<Fill, Domain>) -> transport::Ticket {
        assert_no_heap(|| self.control.submit_prepared(lane, command))
            .unwrap_or_else(|e| panic!("{:?}", e.error))
    }
    fn op(&mut self, lane: Lane, at: Frame, operation: Op) -> Sequence {
        self.send(
            lane,
            Command::Schedule {
                at,
                action: Action::Planner {
                    planner: planner(),
                    operation,
                },
                payload: Box::new([]),
            },
        )
        .sequence()
    }
    fn step(&mut self, frames: usize) {
        let frame = self.runtime.next_frame();
        let mut output = [0.; 32];
        assert_no_heap(|| {
            self.runtime.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[],
                    outputs: &mut [&mut output[..frames]],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut self.midi,
                },
            )
        })
        .unwrap();
        self.samples.extend_from_slice(&output[..frames]);
        for i in 0..self.midi[0].len() {
            let (offset, bytes) = self.midi[0].get(i).unwrap();
            self.packets
                .push((frame.0 + u64::from(offset), bytes.try_into().unwrap()));
        }
    }
    fn completion(&mut self) -> Completion {
        assert_no_heap(|| self.control.try_completion()).expect("owning completion")
    }
    fn execution(&mut self) -> Execution<Op, Reply> {
        assert_no_heap(|| self.control.try_execution()).expect("execution")
    }
    fn stage(&mut self, slot: usize, plan: PreparedPlan) -> StageId {
        self.send(
            Lane::Normal,
            Command::stage_graph(slot, self.runtime.epoch(), plan),
        );
        for _ in 0..128 {
            self.step(1);
            if let Some(completion) = assert_no_heap(|| self.control.try_completion()) {
                assert_eq!(completion.outcome, Outcome::Applied);
                assert!(completion.retired.is_none());
                let Command::StageGraph {
                    id: Some(id),
                    plan: None,
                    ..
                } = completion.request
                else {
                    panic!("stage reply")
                };
                return id;
            }
        }
        panic!("finite descriptor validation");
    }
    fn apply(&mut self, op: Op) -> Execution<Op, Reply> {
        self.op(Lane::Normal, self.runtime.next_frame(), op);
        self.step(1);
        assert!(matches!(
            self.completion().outcome,
            Outcome::Scheduled { late_by: 0, .. }
        ));
        self.execution()
    }
    fn release(&mut self, id: StageId) -> PreparedPlan {
        self.send(Lane::Normal, Command::ReleaseGraph { id });
        self.step(1);
        let completion = self.completion();
        assert_eq!(completion.outcome, Outcome::Applied);
        let Some(Retired::Plan(plan)) = completion.retired else {
            panic!("returned graph owner")
        };
        plan
    }
    fn done(self) {
        let counts = self.counts.clone();
        let expected = self.nodes + 1;
        drop(self);
        assert_eq!(counts.load(Ordering::Relaxed), expected);
    }
}

#[shoop_test]
fn graph_and_domain_publish_at_one_internal_cut_across_partitions_and_orders() {
    for deferred in [false, true] {
        for (parts, reverse) in [
            (vec![12], false),
            (vec![1; 12], true),
            (vec![2, 3, 1, 6], false),
        ] {
            let mut h = Harness::new(4096, reverse, false);
            let candidate = h.plan(true);
            let id = h.stage(0, candidate);
            assert_eq!(h.runtime.next_frame(), Frame(1));
            let operation = if deferred {
                Op::Defer {
                    id,
                    expected: Epoch(1),
                    value: 11,
                    at: Frame(7),
                }
            } else {
                Op::Activate {
                    id,
                    expected: Epoch(1),
                    value: 11,
                    refuse: false,
                }
            };
            let sequence = h.op(Lane::Normal, Frame(if deferred { 2 } else { 7 }), operation);
            for frames in parts {
                h.step(frames);
                if h.runtime.next_frame() == Frame(7) {
                    assert_eq!(h.runtime.epoch(), Epoch(1));
                    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
                    let midi_len = h.midi[0].len();
                    assert_no_heap(|| {
                        h.runtime.process(
                            AudioBlock {
                                frame: Frame(7),
                                frames: 0,
                                inputs: &[],
                                outputs: &mut [&mut []],
                            },
                            MidiBlock {
                                inputs: &[],
                                outputs: &mut h.midi,
                            },
                        )
                    })
                    .unwrap();
                    assert_eq!(h.runtime.epoch(), Epoch(1));
                    assert_eq!(h.midi[0].len(), midi_len);
                }
            }
            assert_eq!(h.completion().sequence, sequence);
            let reply = h.execution();
            assert_eq!(reply.result, Ok(()));
            assert_eq!(
                reply.delivered_frame,
                Some(Frame(if deferred { 2 } else { 7 }))
            );
            assert_eq!(reply.actual_frame, Frame(7));
            assert_eq!(
                reply.operation_result,
                Some(Reply::Applied {
                    epoch: Epoch(2),
                    frame: Frame(7)
                })
            );
            assert_eq!(h.runtime.epoch(), Epoch(2));
            assert_eq!(h.runtime.diagnostics().epoch, Epoch(2));
            assert_eq!(
                h.samples,
                (0..13)
                    .map(|f| if f < 7 { 3. } else { 111. })
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                h.packets,
                (0..13)
                    .map(|f| (f, if f < 7 { [0x90, 60, 3] } else { [0x90, 61, 11] }))
                    .collect::<Vec<_>>()
            );
            let read = h.apply(Op::Read);
            assert_eq!(
                read.operation_result,
                Some(Reply::Cut {
                    epoch: Epoch(2),
                    values: [11, 13]
                })
            );
            assert_eq!(
                h.apply(Op::Inspect(id)).operation_result,
                Some(Reply::Status(StageStatus::Taken))
            );
            let retired = h.release(id);
            assert_eq!(retired.planning().bindings, bindings(1, false));
            drop(retired);
            let counts = h.counts.clone();
            drop(h.control);
            let (domain, resources) = h.runtime.into_planner_and_resources();
            for f in 0..13 {
                assert_eq!(
                    domain.log[f],
                    Some(if f < 7 {
                        (Epoch(1), [3, 7])
                    } else {
                        (Epoch(2), [11, 13])
                    })
                );
            }
            assert_eq!(resources.usage().free_pages, 8);
            drop(domain);
            drop(resources);
            assert_eq!(counts.load(Ordering::Relaxed), 3);
        }
    }
}

#[shoop_test]
fn refused_permits_stale_catalog_epochs_and_stage_generations_preserve_both_owners() {
    let mut h = Harness::new(4096, false, false);
    let first = h.stage(0, h.plan(true));
    let second = h.stage(1, h.plan(false));
    for (id, expected, value, refuse, error) in [
        (first, Epoch(9), 11, false, Error::StaleEpoch),
        (first, Epoch(1), 11, true, Error::InvalidParameter),
        (first, Epoch(1), u32::MAX, false, Error::Exhausted),
        (
            StageId::from_parts(first.parts()[0] + 1, 0, first.parts()[2]).unwrap(),
            Epoch(1),
            11,
            false,
            Error::InvalidId,
        ),
    ] {
        assert_eq!(
            h.apply(Op::Activate {
                id,
                expected,
                value,
                refuse
            })
            .result,
            Err(error)
        );
        assert_eq!(h.runtime.epoch(), Epoch(1));
        assert_eq!(*h.samples.last().unwrap(), 3.);
    }
    assert_eq!(
        h.apply(Op::Inspect(first)).operation_result,
        Some(Reply::Status(StageStatus::Ready {
            validated_epoch: Epoch(1)
        }))
    );
    h.send(Lane::Recovery, Command::ReleaseGraph { id: first });
    h.step(1);
    assert_eq!(
        h.completion().outcome,
        Outcome::Rejected(Error::InvalidState)
    );
    let recovery_owner = h.plan(false);
    h.send(
        Lane::Recovery,
        Command::stage_graph(0, Epoch(1), recovery_owner),
    );
    h.step(1);
    let rejected = h.completion();
    assert_eq!(rejected.outcome, Outcome::Rejected(Error::InvalidState));
    assert!(matches!(
        rejected.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    drop(rejected);
    let occupied = h.plan(false);
    h.send(Lane::Normal, Command::stage_graph(0, Epoch(1), occupied));
    h.step(1);
    let refused = h.completion();
    assert_eq!(refused.outcome, Outcome::Rejected(Error::Busy));
    assert!(matches!(
        refused.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    drop(refused);
    assert_eq!(
        h.apply(Op::Activate {
            id: first,
            expected: Epoch(1),
            value: 11,
            refuse: false
        })
        .result,
        Ok(())
    );
    assert_eq!(
        h.apply(Op::Activate {
            id: second,
            expected: Epoch(2),
            value: 21,
            refuse: false
        })
        .result,
        Err(Error::StaleEpoch)
    );
    assert_eq!(
        h.apply(Op::Activate {
            id: first,
            expected: Epoch(2),
            value: 21,
            refuse: false
        })
        .result,
        Err(Error::InvalidState)
    );
    let old = h.release(first);
    let unused = h.release(second);
    drop(unused);
    assert_eq!(h.apply(Op::Inspect(first)).result, Err(Error::InvalidId));
    let reused = h.stage(0, old);
    assert_eq!(reused.parts()[2], first.parts()[2] + 1);
    assert_eq!(
        h.apply(Op::Activate {
            id: first,
            expected: Epoch(2),
            value: 21,
            refuse: false
        })
        .result,
        Err(Error::InvalidId)
    );
    assert_eq!(
        h.apply(Op::Activate {
            id: reused,
            expected: Epoch(2),
            value: 21,
            refuse: false
        })
        .result,
        Ok(())
    );
    assert_eq!(*h.samples.last().unwrap(), 21.);
    assert_eq!(h.runtime.epoch(), Epoch(3));
    h.done();
    let mut h = Harness::new(4096, false, false);
    let owner = oversized(h.plan(false));
    h.send(Lane::Normal, Command::commit(Epoch(1), owner));
    h.step(1);
    let legacy = h.completion();
    assert_eq!(legacy.outcome, Outcome::Applied);
    drop(legacy);
    let id = h.stage(0, h.plan(true));
    assert_eq!(
        h.apply(Op::Activate {
            id,
            expected: Epoch(2),
            value: 11,
            refuse: false
        })
        .result,
        Err(Error::Capacity)
    );
    assert_eq!(h.runtime.epoch(), Epoch(2));
    assert_eq!(*h.samples.last().unwrap(), 3.);
    assert_eq!(
        h.apply(Op::Inspect(id)).operation_result,
        Some(Reply::Status(StageStatus::Ready {
            validated_epoch: Epoch(2)
        }))
    );
    h.done();
}

fn validation_survives_recovery() {
    for staged in [true, false] {
        let mut h = Harness::new(1, false, false);
        let plan = h.plan(false);
        let command = if staged {
            Command::stage_graph(0, Epoch(1), plan)
        } else {
            Command::commit(Epoch(1), plan)
        };
        let sequence = h.send(Lane::Normal, command).sequence();
        let mut normal = None;
        for _ in 0..128 {
            let recovery = h.op(Lane::Recovery, h.runtime.next_frame(), Op::Cancel);
            h.step(1);
            assert!(h.runtime.diagnostics().validation_steps <= 1);
            while let Some(completion) = assert_no_heap(|| h.control.try_completion()) {
                if completion.sequence == sequence {
                    assert!(normal.is_none());
                    normal = Some(completion);
                } else {
                    assert_eq!(completion.sequence, recovery);
                    assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
                }
            }
            let reply = h.execution();
            assert_eq!(reply.sequence, recovery);
            assert_eq!(reply.operation_result, Some(Reply::Cancelled));
            if normal.is_some() {
                break;
            }
        }
        let completed = normal.expect("recovery must not restart finite validation");
        assert_eq!(completed.outcome, Outcome::Applied);
        if staged {
            let Command::StageGraph {
                id: Some(id),
                plan: None,
                ..
            } = completed.request
            else {
                panic!("stage")
            };
            drop(h.release(id));
            assert_eq!(h.runtime.epoch(), Epoch(1));
        } else {
            assert!(matches!(completed.retired, Some(Retired::Plan(_))));
            assert_eq!(h.runtime.epoch(), Epoch(2));
        }
        assert!(h.samples.iter().all(|value| *value == 3.));
        h.done();
    }
}

#[shoop_test]
fn incremental_validation_and_malformed_or_cancelled_envelopes_never_publish_early() {
    validation_survives_recovery();
    let mut h = Harness::new(1, false, false);
    let source = h.plan(true);
    let mut bad_planning = source.planning().clone();
    bad_planning.step.as_mut().unwrap().before_step = 1;
    let malformed =
        PreparedPlan::from_planned_descriptor(config(1), source.descriptor().clone(), bad_planning)
            .unwrap();
    let id = h.stage(0, source);
    assert!(h.runtime.next_frame().0 > 1);
    assert_eq!(h.runtime.epoch(), Epoch(1));
    assert!(h.samples.iter().all(|s| *s == 3.));
    let source = h.release(id);
    let too_large = oversized(h.plan(true));
    h.send(Lane::Normal, Command::stage_graph(0, Epoch(1), too_large));
    h.step(1);
    let refused = h.completion();
    assert_eq!(refused.outcome, Outcome::Rejected(Error::Capacity));
    assert!(matches!(
        refused.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    drop(refused);
    h.send(Lane::Normal, Command::stage_graph(0, Epoch(1), malformed));
    let failed = loop {
        h.step(1);
        assert!(h.runtime.diagnostics().validation_steps <= 1);
        if let Some(value) = assert_no_heap(|| h.control.try_completion()) {
            break value;
        }
        assert!(h.runtime.next_frame().0 < 128);
    };
    assert_eq!(failed.outcome, Outcome::Rejected(Error::InvalidGraph));
    assert!(matches!(
        failed.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    drop(failed);
    let ticket = h.send(Lane::Normal, Command::stage_graph(0, Epoch(1), source));
    ticket.cancel();
    h.step(1);
    let cancelled = h.completion();
    assert_eq!(cancelled.outcome, Outcome::Cancelled);
    let Command::StageGraph {
        plan: Some(source),
        id: None,
        ..
    } = cancelled.request
    else {
        panic!("cancelled owner")
    };
    let next = h.stage(0, source);
    assert_eq!(next.parts()[2], id.parts()[2] + 1);
    assert_eq!(h.runtime.epoch(), Epoch(1));
    assert!(h.samples.iter().all(|s| *s == 3.));
    let now = h.runtime.next_frame();
    h.op(
        Lane::Normal,
        now,
        Op::Defer {
            id: next,
            expected: Epoch(1),
            value: 11,
            at: Frame(now.0 + 5),
        },
    );
    h.step(1);
    assert!(matches!(h.completion().outcome, Outcome::Scheduled { .. }));
    assert!(assert_no_heap(|| h.control.try_execution()).is_none());
    h.op(Lane::Recovery, h.runtime.next_frame(), Op::Cancel);
    h.step(1);
    assert!(matches!(h.completion().outcome, Outcome::Scheduled { .. }));
    for _ in 0..2 {
        assert_eq!(h.execution().operation_result, Some(Reply::Cancelled));
    }
    h.step(32);
    assert_eq!(h.runtime.epoch(), Epoch(1));
    assert!(h.samples.iter().all(|s| *s == 3.));
    let now = h.runtime.next_frame();
    h.op(
        Lane::Normal,
        now,
        Op::Defer {
            id: next,
            expected: Epoch(1),
            value: 11,
            at: Frame(now.0 + 3),
        },
    );
    h.step(1);
    assert!(matches!(h.completion().outcome, Outcome::Scheduled { .. }));
    let returned = h.release(next);
    drop(returned);
    h.step(2);
    assert_eq!(h.execution().result, Err(Error::InvalidId));
    assert_eq!(h.runtime.epoch(), Epoch(1));
    h.done();
}

#[shoop_test]
fn unread_results_and_retired_graphs_do_not_gate_music_and_recovery_cannot_activate() {
    let mut h = Harness::new(4096, false, false);
    let first = h.stage(0, h.plan(true));
    let second = h.stage(1, h.plan(false));
    h.op(
        Lane::Normal,
        Frame(2),
        Op::Activate {
            id: first,
            expected: Epoch(1),
            value: 11,
            refuse: false,
        },
    );
    for _ in 0..15 {
        h.op(Lane::Normal, Frame(2), Op::Read);
    }
    for _ in 0..4 {
        h.step(32);
    }
    assert_eq!(h.runtime.next_frame(), Frame(130));
    assert!(h.samples[2..].iter().all(|v| *v == 111.));
    h.op(
        Lane::Recovery,
        Frame(130),
        Op::Activate {
            id: second,
            expected: Epoch(2),
            value: 21,
            refuse: false,
        },
    );
    h.step(1);
    let mut recovery = None;
    let mut normal = 0;
    while let Some(result) = assert_no_heap(|| h.control.try_execution()) {
        if result.lane == Lane::Recovery {
            recovery = Some(result);
        } else {
            assert_eq!(result.result, Ok(()));
            normal += 1;
        }
    }
    assert_eq!(normal, 16);
    assert_eq!(recovery.unwrap().result, Err(Error::InvalidState));
    assert_eq!(h.runtime.epoch(), Epoch(2));
    let Harness {
        control,
        mut runtime,
        mut midi,
        counts,
        ..
    } = h;
    drop(control);
    for _ in 0..4 {
        let mut output = [0.; 32];
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: 32,
                    inputs: &[],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
        assert_eq!(output, [111.; 32]);
    }
    drop(runtime);
    assert_eq!(counts.load(Ordering::Relaxed), 3);
}

#[shoop_test]
fn physical_catalog_generation_changes_invalidate_staged_plans_without_losing_returns() {
    let mut h = Harness::new(4096, false, true);
    let plan = graph::compile_planned(
        h.runtime.catalog(),
        &description(2, true, false),
        &bindings(2, false),
    )
    .unwrap();
    let id = h.stage(0, plan);
    h.send(Lane::Normal, Command::Remove { node: node(2) });
    h.step(1);
    let removed = h.completion();
    assert_eq!(removed.outcome, Outcome::Applied);
    assert!(matches!(removed.retired, Some(Retired::Processor(_))));
    drop(removed);
    let replacement = NodeId::from_parts(2, 2).unwrap();
    let prepared = PreparedNode::new(
        replacement,
        Fill {
            slot: 2,
            counts: h.counts.clone(),
        },
    );
    h.send(
        Lane::Normal,
        Command::Install {
            prepared: Some(prepared),
        },
    );
    h.step(1);
    assert_eq!(h.completion().outcome, Outcome::Applied);
    // Legacy catalog edits keep the active graph and its bindings unchanged.
    assert_eq!(h.runtime.epoch(), Epoch(3));
    assert_eq!(
        h.apply(Op::Activate {
            id,
            expected: Epoch(3),
            value: 11,
            refuse: false
        })
        .result,
        Err(Error::StaleEpoch)
    );
    let owner = h.release(id);
    h.send(Lane::Normal, Command::stage_graph(0, Epoch(3), owner));
    h.step(1);
    let failed = h.completion();
    assert_eq!(failed.outcome, Outcome::Rejected(Error::InvalidId));
    assert!(matches!(
        failed.request,
        Command::StageGraph {
            plan: Some(_),
            id: None,
            ..
        }
    ));
    drop(failed);
    assert!(h.samples.iter().all(|v| *v == 3.));
    h.nodes += 1;
    h.done();
}

#[shoop_test]
fn same_frame_injected_midi_prevents_removal_until_delivery_and_never_replays_on_return() {
    let mut h = Harness::new(4096, false, false);
    let mut graph = description(1, false, false);
    graph.nodes.pop();
    let candidate =
        graph::compile_planned(h.runtime.catalog(), &graph, &bindings(1, false)[..1]).unwrap();
    let id = h.stage(0, candidate);
    let message = |at| Command::Schedule {
        at: Frame(at),
        action: Action::Midi {
            node: node(1),
            port: 0,
        },
        payload: Box::new([0x90, 90, 70]),
    };
    let midi = h.send(Lane::Normal, message(1)).sequence();
    let change = h.op(
        Lane::Normal,
        Frame(1),
        Op::Defer {
            id,
            expected: Epoch(1),
            value: 11,
            at: Frame(1),
        },
    );
    h.step(1);
    for _ in 0..2 {
        assert!(matches!(h.completion().outcome, Outcome::Scheduled { .. }));
    }
    let delivered = h.execution();
    assert_eq!((delivered.sequence, delivered.result), (midi, Ok(())));
    let rejected = h.execution();
    assert_eq!(
        (rejected.sequence, rejected.result),
        (change, Err(Error::Busy))
    );
    assert_eq!(h.runtime.epoch(), Epoch(1));
    assert_eq!(*h.samples.last().unwrap(), 3.);
    h.op(
        Lane::Normal,
        Frame(2),
        Op::Activate {
            id,
            expected: Epoch(1),
            value: 11,
            refuse: false,
        },
    );
    h.send(Lane::Normal, message(2));
    h.step(1);
    for _ in 0..2 {
        assert!(matches!(h.completion().outcome, Outcome::Scheduled { .. }));
    }
    assert_eq!(
        h.execution().operation_result,
        Some(Reply::Applied {
            epoch: Epoch(2),
            frame: Frame(2)
        })
    );
    assert_eq!(h.execution().result, Err(Error::InvalidId));
    assert_eq!(*h.samples.last().unwrap(), 11.);
    let retired = h.release(id);
    let id = h.stage(0, retired);
    assert_eq!(
        h.apply(Op::Activate {
            id,
            expected: Epoch(2),
            value: 21,
            refuse: false
        })
        .result,
        Ok(())
    );
    h.step(4);
    let counts = h.counts.clone();
    drop(h.control);
    let (domain, resources) = h.runtime.into_planner_and_resources();
    assert_eq!(domain.injected, [0, 1]);
    drop(domain);
    drop(resources);
    assert_eq!(counts.load(Ordering::Relaxed), 3);
}

struct Signal;
impl Processor for Signal {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        block.audio.output(0).unwrap().fill(5.);
        Ok(())
    }
}
#[shoop_test]
fn no_planner_legacy_commit_and_128_staging_returns_keep_the_original_lifecycle() {
    let cfg = config(4096);
    let graph = GraphDescription {
        nodes: vec![node(0)],
        connections: vec![Connection {
            source: Endpoint::node(node(0), PortType::Audio, 0),
            destination: Endpoint::host(PortType::Audio, 0),
        }],
    };
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        cfg,
        vec![PreparedNode::new(node(0), Signal)],
        NoPlanner,
        ResourceStore::new(cfg).unwrap(),
        &graph,
        &[],
    )
    .unwrap();
    assert_eq!(runtime.prepare_graph_staging(0), Err(Error::InvalidConfig));
    assert_eq!(
        runtime.prepare_graph_staging(usize::MAX),
        Err(Error::Capacity)
    );
    runtime.prepare_graph_staging(1).unwrap();
    assert_eq!(runtime.prepare_graph_staging(1), Err(Error::InvalidState));
    control.prepare_submissions().unwrap();
    let mut midi = [MidiBuffer::new(64, 192).unwrap()];
    let mut run = |runtime: &mut GraphRuntime<Signal>| {
        let mut output = [0.; 1];
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: 1,
                    inputs: &[],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut midi,
                },
            )
        })
        .unwrap();
        assert_eq!(output, [5.]);
    };
    let mut owner = graph::compile(runtime.catalog(), &graph).unwrap();
    for generation in 1..=128 {
        assert_no_heap(|| {
            control.submit_prepared(Lane::Normal, Command::stage_graph(0, Epoch(1), owner))
        })
        .unwrap_or_else(|e| panic!("{:?}", e.error));
        run(&mut runtime);
        let completion = assert_no_heap(|| control.try_completion()).unwrap();
        assert_eq!(completion.outcome, Outcome::Applied);
        let Command::StageGraph {
            id: Some(id),
            plan: None,
            ..
        } = completion.request
        else {
            panic!("stage")
        };
        assert_eq!(id.parts()[2], generation);
        assert_no_heap(|| control.submit_prepared(Lane::Normal, Command::ReleaseGraph { id }))
            .unwrap_or_else(|e| panic!("{:?}", e.error));
        run(&mut runtime);
        let completion = assert_no_heap(|| control.try_completion()).unwrap();
        assert_eq!(completion.outcome, Outcome::Applied);
        let Some(Retired::Plan(returned)) = completion.retired else {
            panic!("owner")
        };
        owner = returned;
    }
    assert_no_heap(|| control.submit_prepared(Lane::Normal, Command::commit(Epoch(1), owner)))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    run(&mut runtime);
    let completion = assert_no_heap(|| control.try_completion()).unwrap();
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Plan(_))));
    assert_eq!(runtime.epoch(), Epoch(2));
    assert_eq!(runtime.prepare_graph_staging(1), Err(Error::InvalidState));
}
