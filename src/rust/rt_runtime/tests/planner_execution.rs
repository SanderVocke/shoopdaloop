mod support;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode};
use rt_runtime::graph::{
    compile_planned, Connection, Endpoint, GraphDescription, PortCounts, PortType,
};
use rt_runtime::planner::*;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::schedule::Action;
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId};
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

fn id(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn planner_id() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
#[derive(Clone, Copy, Default)]
struct Decision {
    frame: Frame,
    gain: f32,
}
#[derive(Default)]
struct Counters {
    plans: AtomicUsize,
    finishes: AtomicUsize,
    observed_frames: AtomicUsize,
}
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Zero,
    BeyondCeiling,
    Shrink,
}
struct Clock {
    decision: [Decision; 1],
    single_sample: bool,
    fault: Fault,
    planned: bool,
    counters: Arc<Counters>,
}
impl Planner for Clock {
    type Decision = Decision;
    type Observation = usize;
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner_id(),
            decision_slots: 1,
            observation_capacity: 2,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[Decision] {
        if self.fault == Fault::Shrink && self.planned {
            &[]
        } else {
            &self.decision
        }
    }
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        self.planned = true;
        self.counters.plans.fetch_add(1, Ordering::Relaxed);
        self.decision[0] = Decision {
            frame: context.frame,
            gain: 1.0 + ((context.frame.0 / 5) % 2) as f32,
        };
        match self.fault {
            Fault::Zero => Ok(context.frame),
            Fault::BeyondCeiling => context.ceiling.checked_add(1),
            _ => context
                .frame
                .checked_add(if self.single_sample {
                    1
                } else {
                    5 - (context.frame.0 % 5) as usize
                })
                .map(|end| end.min(context.ceiling)),
        }
    }
    fn finish(&mut self, context: FinishContext<'_, usize>) -> Result<(), Error> {
        self.counters.finishes.fetch_add(1, Ordering::Relaxed);
        assert!(context.interval.end > context.interval.start);
        if !context.observations.is_empty() {
            assert_eq!(context.observations.len(), 2);
            assert_eq!(context.observations[0].producer, id(0));
            assert_eq!(context.observations[1].producer, id(2));
            let frames = (context.interval.end.0 - context.interval.start.0) as usize;
            for observation in context.observations {
                assert_eq!(observation.value, frames);
            }
            self.counters
                .observed_frames
                .fetch_add(frames, Ordering::Relaxed);
        }
        Ok(())
    }
}
struct Node {
    slot: u32,
    gain: f32,
}
impl Processor<Clock> for Node {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            midi_inputs: 1,
            midi_outputs: 1,
        }
    }
    fn planning(&self) -> ConsumerAccess {
        if self.slot == 1 {
            ConsumerAccess::default()
        } else {
            ConsumerAccess {
                decision_required: true,
                observations: 1,
                operation_bytes: 0,
            }
        }
    }
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 {
            return Err(Error::InvalidParameter);
        }
        self.gain = value;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        let mut gain = self.gain;
        if self.slot == 1 {
            assert!(block.decision.is_none());
            assert_eq!(block.observations.push(0), Err(Error::Capacity));
        } else {
            let view = block.decision.as_ref().unwrap();
            assert_eq!(view.planner, planner_id());
            assert_eq!(view.slot, 0);
            assert_eq!(view.interval.start, block.frame);
            assert_eq!(view.interval.end, block.frame.checked_add(block.frames)?);
            assert_eq!(view.value.frame, block.frame);
            if self.slot == 0 {
                gain *= view.value.gain;
            }
            block.observations.push(block.frames)?;
            assert_eq!(block.observations.remaining(), 0);
            assert_eq!(block.observations.push(usize::MAX), Err(Error::Capacity));
        }
        let (input, output) = block.audio.pair(0, 0).unwrap();
        for (input, output) in input.iter().zip(output) {
            *output = *input * gain;
        }
        for index in 0..block.midi_inputs[0].len() {
            let (offset, bytes) = block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
            block.midi_outputs[0].push(offset, block.frames, bytes)?;
        }
        Ok(())
    }
}
fn limits() -> Config {
    Config {
        max_nodes: 3,
        max_ports: 16,
        max_edges: 16,
        midi_events_per_port: 32,
        midi_bytes_per_port: 128,
        scheduled_events: 4,
        resource_slots: 4,
        resource_pages: 16,
        page_bytes: 64,
        audio_inputs: 1,
        audio_outputs: 1,
        command_slots: 4,
        budgets: Budgets {
            commands: 4,
            events: 4,
            subranges: 5,
            validation_steps: 256,
            ..Budgets::default()
        },
        ..Config::default()
    }
}
struct Harness {
    runtime: GraphRuntime<Node, Clock>,
    control: Control<Node, Clock>,
    counters: Arc<Counters>,
    midi_in: [MidiBuffer; 1],
    midi_out: [MidiBuffer; 1],
    output: [f32; 128],
}
impl Harness {
    fn new(single_sample: bool, fault: Fault, reverse: bool, config: Config) -> Self {
        let counters = Arc::new(Counters::default());
        let clock = Clock {
            decision: [Decision::default()],
            single_sample,
            fault,
            planned: false,
            counters: counters.clone(),
        };
        let mut nodes: Vec<_> = (0..3)
            .map(|slot| {
                PreparedNode::new(
                    id(slot),
                    Node {
                        slot,
                        gain: if slot == 1 { 3.0 } else { 1.0 },
                    },
                )
            })
            .collect();
        if reverse {
            nodes.reverse();
        }
        let (mut control, runtime) =
            GraphRuntime::prepare_with_planner(config.prepare().unwrap(), nodes, clock).unwrap();
        control.prepare_submissions().unwrap();
        let mut graph = GraphDescription {
            nodes: vec![id(0), id(1), id(2)],
            connections: vec![],
        };
        for kind in [PortType::Audio, PortType::Midi] {
            for (from, to) in [
                (None, Some(id(0))),
                (Some(id(0)), Some(id(1))),
                (Some(id(1)), Some(id(2))),
                (Some(id(2)), None),
            ] {
                graph.connections.push(Connection {
                    source: Endpoint {
                        node: from,
                        kind,
                        port: 0,
                    },
                    destination: Endpoint {
                        node: to,
                        kind,
                        port: 0,
                    },
                });
            }
        }
        let mut bindings: Vec<_> = [0, 2]
            .into_iter()
            .map(|slot| DecisionBinding {
                planner: planner_id(),
                consumer: id(slot),
                decision_slot: 0,
            })
            .collect();
        if reverse {
            graph.nodes.reverse();
            graph.connections.reverse();
            bindings.reverse();
        }
        let plan = compile_planned(runtime.catalog(), &graph, &bindings).unwrap();
        control
            .submit(Lane::Normal, Command::commit(Epoch(0), plan))
            .unwrap_or_else(|error| panic!("{:?}", error.error));
        Self {
            runtime,
            control,
            counters,
            midi_in: [MidiBuffer::new(32, 128).unwrap()],
            midi_out: [MidiBuffer::new(32, 128).unwrap()],
            output: [0.0; 128],
        }
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
                    inputs: &self.midi_in,
                    outputs: &mut self.midi_out,
                },
            )
        })
    }
    fn schedule(&mut self, at: u64, action: Action, payload: &[u8]) {
        self.control
            .submit(
                Lane::Normal,
                Command::Schedule {
                    at: Frame(at),
                    action,
                    payload: payload.into(),
                },
            )
            .unwrap_or_else(|error| panic!("{:?}", error.error));
    }
}

#[derive(Default)]
struct IngressCounts {
    initial: AtomicUsize,
    first: AtomicUsize,
    second: AtomicUsize,
    results: AtomicUsize,
    telemetry: AtomicUsize,
}

struct IngressProbe {
    incoming: rtrb::Consumer<Control<IngressProbe, Clock>>,
    control: Option<Control<IngressProbe, Clock>>,
    tickets: [Option<rt_runtime::transport::Ticket>; 2],
    submissions: usize,
    gain: usize,
    counts: Arc<IngressCounts>,
}
impl Processor<Clock> for IngressProbe {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn set_parameter(&mut self, _: u32, value: f32) -> Result<(), Error> {
        self.gain = value as usize;
        Ok(())
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Clock>) -> Result<(), Error> {
        if self.control.is_none() {
            self.control = self.incoming.pop().ok();
        }
        let control = self.control.as_mut().unwrap();
        if let Some(result) = control.try_execution() {
            let index = self.counts.results.fetch_add(1, Ordering::Relaxed);
            assert_eq!(result.actual_frame, Frame(128 * (index + 1) as u64));
            assert_eq!(result.result, Ok(()));
        }
        if control.try_telemetry().is_some() {
            self.counts.telemetry.fetch_add(1, Ordering::Relaxed);
        }
        if self.submissions == 0
            || (self.submissions == 1 && self.counts.results.load(Ordering::Relaxed) == 1)
        {
            let value = if self.submissions == 0 { 4.0 } else { 9.0 };
            self.tickets[self.submissions] = Some(
                control
                    .submit_prepared(
                        Lane::Normal,
                        Command::Schedule {
                            at: block.frame,
                            action: Action::Parameter {
                                node: id(0),
                                parameter: 0,
                                value,
                            },
                            payload: Box::new([]),
                        },
                    )
                    .unwrap_or_else(|error| panic!("{:?}", error.error)),
            );
            self.submissions += 1;
        }
        match self.gain {
            1 => &self.counts.initial,
            4 => &self.counts.first,
            9 => &self.counts.second,
            _ => panic!("unexpected parameter"),
        }
        .fetch_add(block.frames, Ordering::Relaxed);
        Ok(())
    }
}

#[shoop_test]
fn ingress_and_recycled_results_wait_until_the_next_callback_not_next_interval() {
    let counts = Arc::new(IngressCounts::default());
    let counters = Arc::new(Counters::default());
    let (mut transfer, incoming) = rtrb::RingBuffer::new(1);
    let node = IngressProbe {
        incoming,
        control: None,
        tickets: [None, None],
        submissions: 0,
        gain: 1,
        counts: counts.clone(),
    };
    let planner = Clock {
        decision: [Decision::default()],
        single_sample: true,
        fault: Fault::None,
        planned: false,
        counters: counters.clone(),
    };
    let mut config = limits();
    config.budgets.events = 1;
    config.budgets.subranges = 0;
    let (mut control, mut runtime) = GraphRuntime::prepare_with_planner(
        config.prepare().unwrap(),
        vec![PreparedNode::new(id(0), node)],
        planner,
    )
    .unwrap();
    control.prepare_submissions().unwrap();
    let graph = GraphDescription {
        nodes: vec![id(0)],
        connections: vec![],
    };
    let plan = compile_planned(runtime.catalog(), &graph, &[]).unwrap();
    let _commit_ticket = control
        .submit_prepared(Lane::Normal, Command::commit(Epoch(0), plan))
        .unwrap_or_else(|error| panic!("{:?}", error.error));
    assert!(transfer.push(control).is_ok());
    let input = [MidiBuffer::new(1, 3).unwrap()];
    let mut output = [MidiBuffer::new(1, 3).unwrap()];
    let mut samples = [0.0; 128];
    for callback in 0..3 {
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(callback * 128),
                    frames: 128,
                    inputs: &[&[1.0; 128]],
                    outputs: &mut [&mut samples],
                },
                MidiBlock {
                    inputs: &input,
                    outputs: &mut output,
                },
            )
        })
        .unwrap();
        assert_eq!(counts.initial.load(Ordering::Relaxed), 128);
        assert_eq!(
            counts.first.load(Ordering::Relaxed),
            if callback >= 1 { 128 } else { 0 }
        );
        assert_eq!(
            counts.second.load(Ordering::Relaxed),
            if callback >= 2 { 128 } else { 0 }
        );
        assert_eq!(counts.results.load(Ordering::Relaxed), callback as usize);
        assert_eq!(counts.telemetry.load(Ordering::Relaxed), callback as usize);
        assert_eq!(
            runtime.diagnostics().scheduled_events,
            usize::from(callback != 0)
        );
    }
    assert_eq!(counters.plans.load(Ordering::Relaxed), 384);
}

#[shoop_test]
fn dense_planning_uses_one_callback_envelope_without_subrange_quota() {
    let mut config = limits();
    config.budgets.events = 1;
    config.budgets.subranges = 0;
    let mut harness = Harness::new(true, Fault::None, false, config);
    harness.step(128).unwrap();
    assert_eq!(harness.runtime.epoch(), Epoch(1));
    assert_eq!(harness.runtime.diagnostics().subranges, 128);
    assert_eq!(harness.runtime.diagnostics().nodes_processed, 384);
    assert_eq!(
        harness.counters.observed_frames.load(Ordering::Relaxed),
        128
    );
    assert_eq!(harness.counters.plans.load(Ordering::Relaxed), 128);
    assert_eq!(harness.counters.finishes.load(Ordering::Relaxed), 128);
    assert!(harness.control.try_telemetry().is_some());
    assert!(harness.control.try_telemetry().is_none());
    for (frame, sample) in harness.output.iter().enumerate() {
        assert_eq!(*sample, 3.0 * (1 + (frame / 5) % 2) as f32);
    }
}

fn render(partitions: &[usize], single: bool, reverse: bool) -> (Vec<f32>, Vec<(u64, Vec<u8>)>) {
    let mut harness = Harness::new(single, Fault::None, reverse, limits());
    harness.schedule(
        7,
        Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 2.0,
        },
        &[],
    );
    harness.schedule(
        13,
        Action::Midi {
            node: id(0),
            port: 0,
        },
        &[0x90, 60, 100],
    );
    harness.schedule(
        32,
        Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 4.0,
        },
        &[],
    );
    let mut samples = Vec::new();
    let mut events = Vec::new();
    for &frames in partitions {
        let frame = harness.runtime.next_frame();
        harness.midi_in[0].clear();
        for at in [3, 7, 16, 32, 50] {
            if at >= frame.0 && at < frame.0 + frames as u64 {
                harness.midi_in[0]
                    .push((at - frame.0) as u32, frames, &[0x80, 60, 0])
                    .unwrap();
            }
        }
        harness.step(frames).unwrap();
        samples.extend_from_slice(&harness.output[..frames]);
        for index in 0..harness.midi_out[0].len() {
            let (offset, bytes) = harness.midi_out[0].get(index).unwrap();
            events.push((frame.0 + offset as u64, bytes.to_vec()));
        }
    }
    let mut executions = Vec::new();
    while let Some(execution) = harness.control.try_execution() {
        executions.push(execution);
    }
    assert_eq!(executions.len(), 3);
    assert_eq!(
        executions
            .iter()
            .map(|e| e.actual_frame.0)
            .collect::<Vec<_>>(),
        [7, 13, 32]
    );
    assert!(executions.iter().all(|e| e.result == Ok(())));
    for (frame, sample) in samples.iter().enumerate() {
        let parameter = if frame < 7 {
            1.0
        } else if frame < 32 {
            2.0
        } else {
            4.0
        };
        assert_eq!(*sample, 3.0 * parameter * (1 + (frame / 5) % 2) as f32);
    }
    (samples, events)
}

#[shoop_test]
fn decisions_scheduled_actions_and_midi_ignore_host_cuts_and_declarations() {
    let reference = render(&[64], false, false);
    assert_eq!(
        reference.1.iter().map(|e| e.0).collect::<Vec<_>>(),
        [3, 7, 13, 16, 32, 50]
    );
    assert_eq!(reference, render(&[1; 64], true, true));
    assert_eq!(reference, render(&[7, 6, 19, 1, 17, 14], false, true));
}

#[shoop_test]
fn callback_end_and_zero_length_do_not_settle_or_reexecute_boundaries() {
    let mut harness = Harness::new(true, Fault::None, false, limits());
    harness.schedule(
        8,
        Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 7.0,
        },
        &[],
    );
    harness.step(8).unwrap();
    assert!(harness.control.try_execution().is_none());
    let calls = harness.counters.plans.load(Ordering::Relaxed);
    harness.step(0).unwrap();
    assert_eq!(harness.counters.plans.load(Ordering::Relaxed), calls);
    assert_eq!(harness.runtime.next_frame(), Frame(8));
    harness.step(1).unwrap();
    assert_eq!(harness.output[0], 42.0);
    assert_eq!(
        harness.control.try_execution().unwrap().actual_frame,
        Frame(8)
    );
    assert!(harness.control.try_execution().is_none());
}

#[shoop_test]
fn malformed_intervals_and_changed_view_storage_fault_before_consumers() {
    for (fault, error) in [
        (Fault::Zero, Error::InvalidFrame),
        (Fault::BeyondCeiling, Error::InvalidFrame),
        (Fault::Shrink, Error::InvalidShape),
    ] {
        let mut harness = Harness::new(true, fault, false, limits());
        assert_eq!(harness.step(8), Err(error));
        assert_eq!(harness.output[..8], [0.0; 8]);
        assert_eq!(harness.runtime.next_frame(), Frame(8));
        assert_eq!(harness.runtime.diagnostics().render_error, Some(error));
        assert_eq!(harness.counters.finishes.load(Ordering::Relaxed), 0);
        assert_eq!(harness.runtime.diagnostics().nodes_processed, 0);
    }
}

struct BudgetClock {
    cursor: rt_runtime::resource::Cursor,
    read: Arc<AtomicUsize>,
    refused: Arc<AtomicUsize>,
}
impl Planner for BudgetClock {
    type Decision = ();
    type Observation = ();
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner_id(),
            decision_slots: 0,
            observation_capacity: 0,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[()] {
        &[]
    }
    fn plan(&mut self, mut context: PlanContext<'_>) -> Result<Frame, Error> {
        match context.resources.read(&mut self.cursor, &mut [0; 4]) {
            Ok(bytes) => {
                self.read.fetch_add(bytes, Ordering::Relaxed);
            }
            Err(Error::Budget) => {
                self.refused.fetch_add(1, Ordering::Relaxed);
            }
            Err(error) => return Err(error),
        }
        context.frame.checked_add(1)
    }
    fn finish(&mut self, context: FinishContext<'_, ()>) -> Result<(), Error> {
        assert!(context.observations.is_empty());
        Ok(())
    }
}
struct NoNodes;
impl Processor<BudgetClock> for NoNodes {
    fn ports(&self) -> PortCounts {
        PortCounts::default()
    }
    fn process(&mut self, _: &mut ProcessBlock<'_, BudgetClock>) -> Result<(), Error> {
        Err(Error::InvalidState)
    }
}

#[shoop_test]
fn resource_allowances_are_not_replenished_by_planning_or_internal_intervals() {
    use rt_runtime::resource::{Description, Format, ResourceStore};
    use rt_runtime::Revision;
    let mut config = limits();
    config.page_bytes = 64;
    config.budgets.transfer_bytes = 64;
    let config = config.prepare().unwrap();
    let mut store = ResourceStore::new(config).unwrap();
    let resource = store.create().unwrap();
    let upload = store
        .begin_upload(
            resource,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 128,
                bytes: 512,
            },
        )
        .unwrap();
    for offset in (0..512).step_by(64) {
        store.begin_callback();
        store.write_upload(upload, offset, &[0; 64]).unwrap();
    }
    store.finish_upload(upload).unwrap();
    store.commit(upload).unwrap();
    store.begin_callback();
    let snapshot = store.snapshot(resource, Frame(0)).unwrap();
    let cursor = store.cursor(snapshot.id).unwrap();
    let read = Arc::new(AtomicUsize::new(0));
    let refused = Arc::new(AtomicUsize::new(0));
    let planner = BudgetClock {
        cursor,
        read: read.clone(),
        refused: refused.clone(),
    };
    let (_control, mut runtime) =
        GraphRuntime::<NoNodes, BudgetClock>::prepare_with_planner_and_resources(
            config,
            vec![],
            planner,
            store,
        )
        .unwrap();
    let input = [MidiBuffer::new(1, 3).unwrap()];
    let mut output = [MidiBuffer::new(1, 3).unwrap()];
    let mut samples = [0.0; 128];
    for callback in 0..2 {
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: Frame(callback * 128),
                    frames: 128,
                    inputs: &[&[1.0; 128]],
                    outputs: &mut [&mut samples],
                },
                MidiBlock {
                    inputs: &input,
                    outputs: &mut output,
                },
            )
        })
        .unwrap();
        assert_eq!(runtime.diagnostics().subranges, 128);
        assert_eq!(runtime.diagnostics().transfer_bytes, 64);
        assert_eq!(read.load(Ordering::Relaxed), (callback as usize + 1) * 64);
        assert_eq!(
            refused.load(Ordering::Relaxed),
            (callback as usize + 1) * 112
        );
    }
}

#[shoop_test]
fn stale_planner_candidate_keeps_previous_graph_and_bindings_active() {
    let mut harness = Harness::new(true, Fault::None, false, limits());
    harness.step(1).unwrap();
    assert_eq!(
        harness.control.try_completion().unwrap().outcome,
        Outcome::Applied
    );
    let signal = rt_runtime::graph::PlanDescriptor {
        steps: vec![],
        connections: vec![],
    };
    let stale = PlanningDescriptor {
        step: Some(PlanningStep {
            planner: PlannerSpec {
                id: PlannerId::from_parts(0, 2).unwrap(),
                decision_slots: 1,
                observation_capacity: 2,
                operation_bytes: 0,
            },
            before_step: 0,
        }),
        bindings: vec![],
    };
    let plan = rt_runtime::graph::PreparedPlan::from_planned_descriptor(
        limits().prepare().unwrap(),
        signal,
        stale,
    )
    .unwrap();
    harness
        .control
        .submit(Lane::Normal, Command::commit(Epoch(1), plan))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    harness.step(8).unwrap();
    assert_eq!(harness.runtime.epoch(), Epoch(1));
    let completion = harness.control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Rejected(Error::InvalidId));
    assert!(matches!(
        completion.request,
        Command::Commit { plan: Some(_), .. }
    ));
    assert_eq!(harness.counters.observed_frames.load(Ordering::Relaxed), 9);
}
