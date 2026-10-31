use super::*;
use crate::{guarded, scenario};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock, MidiBuffer},
    executor::Command,
    transport::{Lane, Outcome},
    Frame,
};
use shoop_loops::{
    runtime::{Operation, Reply},
    Timing, Transition,
};
use std::sync::{atomic::AtomicU32, Arc};

fn record(sequence: u64, id: Id, payload: Payload) -> Record {
    Record {
        session: 7,
        sequence,
        id,
        payload,
    }
}
fn send(receiver: &mut Receiver, id: Id, payload: Payload) -> Result<Phase, Error> {
    let bytes = record(receiver.last_sequence() + 1, id, payload)
        .encode()
        .unwrap();
    guarded(|| receiver.ingest(&bytes))
}
fn empty(receiver: &mut Receiver, id: Id) {
    send(
        receiver,
        id,
        Payload::Begin {
            epoch: Epoch(1),
            counts: Counts {
                steps: 0,
                edges: 0,
                bindings: 0,
            },
            planned: false,
        },
    )
    .unwrap();
    send(receiver, id, Payload::Seal).unwrap();
}
pub(crate) fn transfer(receiver: &mut Receiver, id: Id, epoch: Epoch, plan: &PreparedPlan) {
    send(
        receiver,
        id,
        Payload::Begin {
            epoch,
            counts: Counts {
                steps: plan.descriptor().steps.len() as u32,
                edges: plan.descriptor().connections.len() as u32,
                bindings: plan.planning().bindings.len() as u32,
            },
            planned: plan.planning().step.is_some(),
        },
    )
    .unwrap();
    for (offset, &step) in plan.descriptor().steps.iter().enumerate() {
        send(
            receiver,
            id,
            Payload::Step {
                offset: offset as u32,
                step,
            },
        )
        .unwrap();
    }
    for (offset, &edge) in plan.descriptor().connections.iter().enumerate() {
        send(
            receiver,
            id,
            Payload::Edge {
                offset: offset as u32,
                edge,
            },
        )
        .unwrap();
    }
    if let Some(planning) = plan.planning().step {
        send(receiver, id, Payload::Planning(planning)).unwrap();
    }
    for (offset, &binding) in plan.planning().bindings.iter().enumerate() {
        send(
            receiver,
            id,
            Payload::Binding {
                offset: offset as u32,
                binding,
            },
        )
        .unwrap();
    }
    send(receiver, id, Payload::Seal).unwrap();
    let received = receiver.plan(id).unwrap();
    assert_eq!(received.descriptor(), plan.descriptor());
    assert_eq!(received.planning(), plan.planning());
}
fn config() -> PreparedConfig {
    rt_runtime::Config {
        max_nodes: 4,
        max_edges: 16,
        ..rt_runtime::Config::default()
    }
    .prepare()
    .unwrap()
}
fn recycle(receiver: &mut Receiver, id: Id, plan: PreparedPlan) {
    guarded(|| receiver.recycle(id, plan))
        .unwrap_or_else(|(error, _)| panic!("recycle: {error:?}"));
}

#[test]
fn canonical_records_preserve_full_width_fields_and_reject_all_padding_and_truncations() {
    let id = Id {
        slot: u32::MAX,
        generation: u32::MAX,
    };
    let node = NodeId::from_parts(u32::MAX - 1, u32::MAX).unwrap();
    let planner = PlannerId::from_parts(u32::MAX - 1, u32::MAX).unwrap();
    let payloads = [
        Payload::Begin {
            epoch: Epoch(u64::MAX),
            counts: Counts {
                steps: u32::MAX,
                edges: u32::MAX,
                bindings: u32::MAX,
            },
            planned: true,
        },
        Payload::Step {
            offset: u32::MAX,
            step: NodeStep {
                node,
                input_end: usize::MAX,
            },
        },
        Payload::Edge {
            offset: u32::MAX,
            edge: Connection {
                source: Endpoint::host(PortType::Audio, u32::MAX),
                destination: Endpoint::node(node, PortType::Midi, u32::MAX),
            },
        },
        Payload::Planning(PlanningStep {
            planner: PlannerSpec {
                id: planner,
                decision_slots: usize::MAX,
                observation_capacity: usize::MAX,
                operation_bytes: usize::MAX,
            },
            before_step: usize::MAX,
        }),
        Payload::Binding {
            offset: u32::MAX,
            binding: DecisionBinding {
                planner,
                consumer: node,
                decision_slot: usize::MAX,
            },
        },
        Payload::Seal,
        Payload::Abort,
    ];
    let used: [&[std::ops::Range<usize>]; 7] = [
        &[0..7, 8..53],
        &[0..7, 8..52],
        &[0..7, 8..45, 48..61, 64..68],
        &[0..7, 8..72],
        &[0..7, 8..60],
        &[0..7, 8..32],
        &[0..7, 8..32],
    ];
    for (index, payload) in payloads.into_iter().enumerate() {
        let original = Record {
            session: u64::MAX,
            sequence: u64::MAX,
            id,
            payload,
        };
        let bytes = guarded(|| original.encode()).unwrap();
        assert_eq!(guarded(|| Record::decode(&bytes)), Ok(original));
        let mut expected = [0; RECORD_BYTES];
        expected[..6].copy_from_slice(b"LGP1\x01\x00");
        expected[6] = index as u8 + 1;
        expected[8..32].fill(255);
        match index {
            0 => {
                expected[32..52].fill(255);
                expected[52] = 1;
            }
            1 => {
                expected[32..44].fill(255);
                expected[36] = 254;
                expected[44..52].copy_from_slice(&(usize::MAX as u64).to_le_bytes());
            }
            2 => {
                expected[32..36].fill(255);
                expected[48..60].fill(255);
                expected[52] = 254;
                expected[60] = 1;
                expected[64..68].fill(255);
            }
            3 => {
                expected[32..40].fill(255);
                expected[32] = 254;
                for at in [40, 48, 56, 64] {
                    expected[at..at + 8].copy_from_slice(&(usize::MAX as u64).to_le_bytes());
                }
            }
            4 => {
                expected[32..52].fill(255);
                expected[36] = 254;
                expected[44] = 254;
                expected[52..60].copy_from_slice(&(usize::MAX as u64).to_le_bytes());
            }
            _ => {}
        }
        assert_eq!(bytes, expected);
        for length in 0..RECORD_BYTES {
            assert_eq!(
                guarded(|| Record::decode(&bytes[..length])),
                Err(Error::InvalidShape)
            );
        }
        for offset in 0..RECORD_BYTES {
            if !used[index].iter().any(|range| range.contains(&offset)) {
                assert_eq!(bytes[offset], 0, "{index}:{offset}");
                let mut bad = bytes;
                bad[offset] = 1;
                assert!(
                    guarded(|| Record::decode(&bad)).is_err(),
                    "padding {index}:{offset}"
                );
            }
        }
        let mut long = [0; RECORD_BYTES + 1];
        long[..RECORD_BYTES].copy_from_slice(&bytes);
        assert_eq!(Record::decode(&long), Err(Error::InvalidShape));
    }
    let bytes = record(
        1,
        Id {
            slot: 0,
            generation: 1,
        },
        payloads[2],
    )
    .encode()
    .unwrap();
    let mut host_slot = bytes;
    host_slot[36] = 1;
    assert_eq!(Record::decode(&host_slot), Err(Error::InvalidId));
    let mut kind = bytes;
    kind[44] = 2;
    assert_eq!(Record::decode(&kind), Err(Error::PortType));
    let mut zero_generation = bytes;
    zero_generation[28..32].fill(0);
    assert_eq!(Record::decode(&zero_generation), Err(Error::InvalidId));
}

#[test]
fn receiving_is_transactional_ordered_and_independent_of_other_banks() {
    let mut receiver = Receiver::new(7, config()).unwrap();
    let a = receiver.id(0).unwrap();
    let b = receiver.id(1).unwrap();
    let counts = Counts {
        steps: 1,
        edges: 1,
        bindings: 1,
    };
    send(
        &mut receiver,
        a,
        Payload::Begin {
            epoch: Epoch(9),
            counts,
            planned: true,
        },
    )
    .unwrap();
    empty(&mut receiver, b);
    let loan = guarded(|| receiver.take(b)).unwrap();
    let node = NodeId::from_parts(0, 1).unwrap();
    let edge = Connection {
        source: Endpoint::host(PortType::Audio, 0),
        destination: Endpoint::node(node, PortType::Audio, 0),
    };
    let planner = PlannerId::from_parts(0, 1).unwrap();
    for payload in [
        Payload::Seal,
        Payload::Edge { offset: 0, edge },
        Payload::Step {
            offset: 1,
            step: NodeStep { node, input_end: 1 },
        },
        Payload::Binding {
            offset: 0,
            binding: DecisionBinding {
                planner,
                consumer: node,
                decision_slot: 0,
            },
        },
    ] {
        let before = receiver.last_sequence();
        assert_eq!(send(&mut receiver, a, payload), Err(Error::InvalidOffset));
        assert_eq!(receiver.last_sequence(), before);
        assert!(receiver.plan(a).unwrap().descriptor().steps.is_empty());
    }
    send(
        &mut receiver,
        a,
        Payload::Step {
            offset: 0,
            step: NodeStep { node, input_end: 1 },
        },
    )
    .unwrap();
    assert_eq!(
        send(&mut receiver, a, Payload::Seal),
        Err(Error::InvalidOffset)
    );
    send(&mut receiver, a, Payload::Edge { offset: 0, edge }).unwrap();
    let planning = PlanningStep {
        planner: PlannerSpec {
            id: planner,
            decision_slots: 1,
            observation_capacity: 1,
            operation_bytes: 0,
        },
        before_step: 0,
    };
    send(&mut receiver, a, Payload::Planning(planning)).unwrap();
    assert_eq!(
        send(&mut receiver, a, Payload::Planning(planning)),
        Err(Error::InvalidOffset)
    );
    send(
        &mut receiver,
        a,
        Payload::Binding {
            offset: 0,
            binding: DecisionBinding {
                planner,
                consumer: node,
                decision_slot: 0,
            },
        },
    )
    .unwrap();
    send(&mut receiver, a, Payload::Seal).unwrap();
    let snapshot = receiver.plan(a).unwrap().descriptor().clone();
    let before = receiver.last_sequence();
    assert_eq!(send(&mut receiver, a, Payload::Abort).unwrap(), Phase::Idle);
    assert_eq!(receiver.last_sequence(), before + 1);
    assert_eq!(receiver.phase(a), Err(Error::InvalidId));
    assert_eq!(
        receiver.plan(receiver.id(0).unwrap()).unwrap().descriptor(),
        &snapshot
    );
    recycle(&mut receiver, b, loan.plan);
    assert_eq!(receiver.id(0).unwrap().generation, 2);
    assert_eq!(receiver.id(1).unwrap().generation, 2);
}

#[test]
fn busy_stale_foreign_and_malformed_records_do_not_advance_or_destroy_owners() {
    let mut receiver = Receiver::new(7, config()).unwrap();
    let id = receiver.id(0).unwrap();
    let capacity = receiver.capacities(id).unwrap();
    let begin = Payload::Begin {
        epoch: Epoch(0),
        counts: Counts {
            steps: capacity.steps + 1,
            edges: 0,
            bindings: 0,
        },
        planned: false,
    };
    assert_eq!(send(&mut receiver, id, begin), Err(Error::Capacity));
    assert_eq!(receiver.last_sequence(), 0);
    let malformed = Payload::Begin {
        epoch: Epoch(0),
        counts: Counts {
            steps: 0,
            edges: 0,
            bindings: 1,
        },
        planned: false,
    };
    assert_eq!(send(&mut receiver, id, malformed), Err(Error::InvalidShape));
    empty(&mut receiver, id);
    let loan = guarded(|| receiver.take(id)).unwrap();
    assert_eq!(receiver.phase(id).unwrap(), Phase::Loaned);
    assert!(matches!(guarded(|| receiver.take(id)), Err(Error::Busy)));
    let last = receiver.last_sequence();
    let abort = record(last + 1, id, Payload::Abort);
    assert_eq!(
        guarded(|| receiver.ingest(&abort.encode().unwrap())),
        Err(Error::Busy)
    );
    let mut foreign = abort;
    foreign.session = 8;
    assert_eq!(
        guarded(|| receiver.ingest(&foreign.encode().unwrap())),
        Err(Error::InvalidId)
    );
    let mut stale = abort;
    stale.id.generation += 1;
    assert_eq!(
        guarded(|| receiver.ingest(&stale.encode().unwrap())),
        Err(Error::InvalidId)
    );
    assert_eq!(receiver.last_sequence(), last);
    let (error, plan) = guarded(|| {
        receiver.recycle(
            Id {
                generation: 2,
                ..id
            },
            loan.plan,
        )
    })
    .err()
    .unwrap();
    assert_eq!(error, Error::InvalidId);
    let other_config = rt_runtime::Config {
        max_nodes: 3,
        ..rt_runtime::Config::default()
    }
    .prepare()
    .unwrap();
    let wrong = PreparedPlan::with_capacity(other_config).unwrap();
    let (error, wrong) = guarded(|| receiver.recycle(id, wrong)).err().unwrap();
    assert_eq!(error, Error::InvalidConfig);
    drop(wrong);
    let oversized = PreparedPlan::from_descriptor(
        config(),
        rt_runtime::graph::PlanDescriptor {
            steps: Vec::with_capacity(32),
            connections: Vec::with_capacity(128),
        },
    )
    .unwrap();
    let (error, oversized) = guarded(|| receiver.recycle(id, oversized)).err().unwrap();
    assert_eq!(error, Error::Capacity);
    drop(oversized);
    recycle(&mut receiver, id, plan);
    let next = receiver.id(0).unwrap();
    assert_eq!(next.generation, 2);
    assert_eq!(
        send(&mut receiver, id, Payload::Abort),
        Err(Error::InvalidId)
    );
    empty(&mut receiver, next);
    let owner = PreparedPlan::with_capacity(config()).unwrap();
    let (error, owner) = guarded(|| receiver.recycle(next, owner)).err().unwrap();
    assert_eq!(error, Error::Busy);
    drop(owner);
}

#[test]
fn generation_and_sequence_exhaustion_never_wrap_and_recycled_capacities_are_truthful() {
    let cfg = config();
    let mut receiver = Receiver::new(7, cfg).unwrap();
    receiver.banks[0].generation = u32::MAX;
    let id = receiver.id(0).unwrap();
    empty(&mut receiver, id);
    let loan = guarded(|| receiver.take(id)).unwrap();
    recycle(&mut receiver, id, loan.plan);
    assert_eq!(receiver.id(0), Err(Error::Exhausted));
    assert_eq!(receiver.banks[0].generation, 0);
    receiver.banks[1].generation = u32::MAX;
    let other = receiver.id(1).unwrap();
    empty(&mut receiver, other);
    let bytes = record(u64::MAX, other, Payload::Abort).encode().unwrap();
    assert_eq!(guarded(|| receiver.ingest(&bytes)), Ok(Phase::Idle));
    assert_eq!(receiver.last_sequence(), u64::MAX);
    assert_eq!(
        guarded(|| receiver.ingest(&bytes)),
        Err(Error::InvalidState)
    );
    assert_eq!(receiver.id(1), Err(Error::Exhausted));

    let mut receiver = Receiver::new(7, cfg).unwrap();
    let id = receiver.id(0).unwrap();
    empty(&mut receiver, id);
    let loan = receiver.take(id).unwrap();
    drop(loan.plan);
    let small =
        PreparedPlan::from_descriptor(cfg, rt_runtime::graph::PlanDescriptor::default()).unwrap();
    recycle(&mut receiver, id, small);
    let next = receiver.id(0).unwrap();
    assert_eq!(
        receiver.capacities(next).unwrap(),
        Counts {
            steps: 0,
            edges: 0,
            bindings: 0
        }
    );
    assert_eq!(
        send(
            &mut receiver,
            next,
            Payload::Begin {
                epoch: Epoch(1),
                counts: Counts {
                    steps: 1,
                    edges: 0,
                    bindings: 0
                },
                planned: false
            }
        ),
        Err(Error::Capacity)
    );
    empty(&mut receiver, next);
}

struct Host {
    control: crate::assembly::LoopControl<scenario::Mix>,
    engine: crate::assembly::LoopRuntime<scenario::Mix>,
    input: [MidiBuffer; 1],
    output: [MidiBuffer; 1],
    samples: Vec<[f32; 2]>,
    midi: Vec<(u64, Vec<u8>)>,
}
impl Host {
    fn new() -> Self {
        let (control, mut engine, _) = scenario::prepare_with_fault(0, Arc::new(AtomicU32::new(0)));
        engine.prepare_graph_staging(2).unwrap();
        Self {
            control,
            engine,
            input: [MidiBuffer::new(2, 6).unwrap()],
            output: [MidiBuffer::new(256, 1024).unwrap()],
            samples: Vec::new(),
            midi: Vec::new(),
        }
    }
    fn step(&mut self, frames: usize) {
        let start = self.engine.next_frame().0;
        let mut a = [0.; 128];
        let mut b = [0.; 128];
        let mut x = [0.; 128];
        let mut y = [0.; 128];
        for offset in 0..frames {
            if start + (offset as u64) < 4 {
                a[offset] = 2. * (start + offset as u64 + 1) as f32;
                b[offset] = a[offset] * 10.;
            }
        }
        self.input[0].clear();
        self.output[0].clear();
        for (at, packet) in [(0, [0x90, 60, 100]), (2, [0x80, 60, 45])] {
            if at >= start && at < start + frames as u64 {
                self.input[0]
                    .push((at - start) as u32, frames, &packet)
                    .unwrap();
            }
        }
        guarded(|| {
            self.engine.process(
                AudioBlock {
                    frame: Frame(start),
                    frames,
                    inputs: &[&a[..frames], &b[..frames]],
                    outputs: &mut [&mut x[..frames], &mut y[..frames]],
                },
                MidiBlock {
                    inputs: &self.input,
                    outputs: &mut self.output,
                },
            )
        })
        .unwrap();
        for offset in 0..frames {
            self.samples.push([x[offset], y[offset]]);
        }
        for index in 0..self.output[0].len() {
            let (offset, packet) = self.output[0].get(index).unwrap();
            self.midi.push((start + u64::from(offset), packet.to_vec()));
        }
    }
    fn finish(self) -> Vec<u8> {
        drop(self.control);
        let mut stopped = self.engine.into_stopped();
        let semantic = stopped
            .observer()
            .as_any()
            .downcast_ref::<crate::journal::Journal>()
            .unwrap()
            .bytes()
            .to_vec();
        stopped.cleanup().unwrap();
        for _ in 0..512 {
            stopped.resources().begin_callback();
            stopped.resources().reclaim();
        }
        assert_eq!(stopped.resource_usage().free_pages, 256);
        assert_eq!(stopped.resource_usage().free_versions, 16);
        assert_eq!(stopped.resource_usage().free_snapshots, 16);
        assert_eq!(stopped.media_usage().references, 0);
        semantic
    }
}

#[test]
fn receiver_descriptors_stage_activate_and_recycle_real_facade_graph_owners() {
    for parts in [vec![3, 8, 1], vec![1; 12], vec![0, 3, 0, 5, 3, 1, 0]] {
        let mut host = Host::new();
        let reference = host.control.compile_graph(&scenario::routes(true)).unwrap();
        let mut receiver = Receiver::new(7, reference.config()).unwrap();
        let id = receiver.id(0).unwrap();
        transfer(&mut receiver, id, Epoch(1), &reference);
        let loan = guarded(|| receiver.take(id)).unwrap();
        let staged = guarded(|| host.control.stage_graph(0, loan.epoch, loan.plan))
            .unwrap_or_else(|e| panic!("stage: {:?}", e.error));
        let target = shoop_loops::LoopId::from_parts(0, 1).unwrap();
        let mut requested = Vec::new();
        for (at, transition) in [
            (
                0,
                Transition::Record {
                    monitor_previous: false,
                },
            ),
            (
                4,
                Transition::Play {
                    position: 0,
                    retrigger: false,
                },
            ),
            (12, Transition::Stop),
        ] {
            requested.push(
                guarded(|| {
                    host.control.submit(
                        Lane::Normal,
                        Frame(at),
                        Operation::Transition {
                            target,
                            timing: Timing::At,
                            transition,
                        },
                    )
                })
                .unwrap_or_else(|e| panic!("transition: {:?}", e.error))
                .sequence(),
            );
        }
        host.step(1);
        let ready = guarded(|| host.control.try_completion()).unwrap();
        assert_eq!(ready.sequence, staged.sequence());
        assert_eq!(ready.outcome, Outcome::Applied);
        let Command::StageGraph {
            id: Some(graph),
            plan: None,
            ..
        } = ready.request
        else {
            panic!("ready owner");
        };
        drop(ready.retired);
        let switched = guarded(|| {
            host.control.submit(
                Lane::Normal,
                Frame(4),
                Operation::GraphActivate {
                    id: graph,
                    graph_epoch: Epoch(1),
                    topology_epoch: Epoch(0),
                },
            )
        })
        .unwrap_or_else(|e| panic!("activation: {:?}", e.error));
        for frames in parts {
            host.step(frames);
        }
        assert_eq!(host.engine.next_frame(), Frame(13));
        while let Some(completion) = guarded(|| host.control.try_completion()) {
            assert!(matches!(completion.outcome, Outcome::Scheduled { .. }));
        }
        let mut transitions = Vec::new();
        let mut saw_switch = false;
        while let Some(execution) = guarded(|| host.control.try_execution()) {
            assert_eq!(execution.result, Ok(()));
            if execution.sequence == switched.sequence() {
                assert_eq!(execution.requested_frame, Frame(4));
                assert_eq!(execution.delivered_frame, Some(Frame(4)));
                assert_eq!(execution.actual_frame, Frame(4));
                assert_eq!(
                    execution.operation_result,
                    Some(Reply::GraphActivated {
                        id: graph,
                        graph_epoch: Epoch(2),
                        topology_epoch: Epoch(0)
                    })
                );
                saw_switch = true;
            } else {
                assert!(requested.contains(&execution.sequence));
                assert!(matches!(
                    execution.operation_result,
                    Some(Reply::Transition(shoop_loops::Completion {
                        outcome: shoop_loops::Outcome::Applied { .. },
                        ..
                    }))
                ));
                transitions.push((
                    execution.requested_frame.0,
                    execution.delivered_frame.unwrap().0,
                    execution.actual_frame.0,
                ));
            }
        }
        assert!(saw_switch);
        assert_eq!(transitions, [(0, 0, 0), (4, 4, 4), (12, 12, 12)]);
        let expected: Vec<_> = [1., 2., 3., 4., 1., 2., 3., 4., 1., 2., 3., 4., 0.]
            .into_iter()
            .map(|left| [left, left * 10.])
            .collect();
        assert_eq!(host.samples, expected);
        assert_eq!(host.midi, crate::run(&[13]).midi);
        let released = guarded(|| host.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("release: {:?}", e.error));
        host.step(1);
        let completed = guarded(|| host.control.try_completion()).unwrap();
        assert_eq!(completed.sequence, released.sequence());
        assert_eq!(completed.outcome, Outcome::Applied);
        let Some(rt_runtime::executor::Retired::Plan(retired)) = completed.retired else {
            panic!("retired initial graph");
        };
        let initial = host
            .control
            .compile_graph(&scenario::routes(false))
            .unwrap();
        assert_eq!(retired.descriptor(), initial.descriptor());
        assert_eq!(retired.planning(), initial.planning());
        recycle(&mut receiver, id, retired);
        let next = receiver.id(0).unwrap();
        transfer(&mut receiver, next, Epoch(2), &initial);
        let loan = guarded(|| receiver.take(next)).unwrap();
        recycle(&mut receiver, next, loan.plan);
        assert_eq!(receiver.id(0).unwrap().generation, 3);
        assert_eq!(host.finish(), crate::journal::expected_primitive());
    }
}

#[test]
fn cancelled_stale_and_invalid_runtime_candidates_return_the_exact_prepared_owner() {
    let mut host = Host::new();
    let reference = host
        .control
        .compile_graph(&scenario::routes(false))
        .unwrap();
    let mut receiver = Receiver::new(7, reference.config()).unwrap();
    for case in 0..128 {
        let id = receiver.id(0).unwrap();
        transfer(
            &mut receiver,
            id,
            Epoch(if case % 4 == 1 { 0 } else { 1 }),
            &reference,
        );
        let mut loan = guarded(|| receiver.take(id)).unwrap();
        if case % 4 == 2 {
            loan.plan.set_planning_step(None);
        }
        let before = loan.plan.storage_bytes().unwrap();
        let cancelled = case % 4 == 0;
        let ticket = guarded(|| host.control.stage_graph(0, loan.epoch, loan.plan))
            .unwrap_or_else(|e| panic!("stage: {:?}", e.error));
        if cancelled {
            guarded(|| ticket.cancel());
        }
        host.step(1);
        let completion = guarded(|| host.control.try_completion()).unwrap();
        assert_eq!(completion.sequence, ticket.sequence());
        if case % 4 == 3 {
            assert_eq!(completion.outcome, Outcome::Applied);
            let Command::StageGraph {
                id: Some(graph),
                plan: None,
                ..
            } = completion.request
            else {
                panic!("ready");
            };
            let release = guarded(|| host.control.release_graph(graph))
                .unwrap_or_else(|e| panic!("release: {:?}", e.error));
            host.step(1);
            let completion = guarded(|| host.control.try_completion()).unwrap();
            assert_eq!(completion.sequence, release.sequence());
            assert_eq!(completion.outcome, Outcome::Applied);
            let Some(rt_runtime::executor::Retired::Plan(plan)) = completion.retired else {
                panic!("returned unactivated owner");
            };
            assert_eq!(plan.storage_bytes().unwrap(), before);
            recycle(&mut receiver, id, plan);
        } else {
            assert_eq!(
                completion.outcome,
                match case % 4 {
                    0 => Outcome::Cancelled,
                    1 => Outcome::Rejected(Error::StaleEpoch),
                    _ => Outcome::Rejected(Error::InvalidGraph),
                }
            );
            let Command::StageGraph {
                plan: Some(plan),
                id: None,
                ..
            } = completion.request
            else {
                panic!("owning refusal");
            };
            assert_eq!(plan.storage_bytes().unwrap(), before);
            assert_eq!(plan.descriptor(), reference.descriptor());
            recycle(&mut receiver, id, plan);
        }
        assert_eq!(receiver.id(0).unwrap().generation, case + 2);
    }
    host.finish();
}

#[test]
fn ingress_refusal_stalled_returns_and_disconnect_keep_all_graph_owners_explicit() {
    let mut host = Host::new();
    let target = shoop_loops::LoopId::from_parts(0, 1).unwrap();
    let reference = host
        .control
        .compile_graph(&scenario::routes(false))
        .unwrap();
    let mut receiver = Receiver::new(7, reference.config()).unwrap();
    for _ in 0..8 {
        guarded(|| {
            host.control.submit(
                Lane::Normal,
                Frame(0),
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition: Transition::Stop,
                },
            )
        })
        .unwrap_or_else(|e| panic!("fill ingress: {:?}", e.error));
    }
    let id = receiver.id(0).unwrap();
    transfer(&mut receiver, id, Epoch(1), &reference);
    let loan = guarded(|| receiver.take(id)).unwrap();
    let refused = guarded(|| host.control.stage_graph(0, loan.epoch, loan.plan))
        .err()
        .unwrap();
    assert_eq!(refused.error, Error::Full);
    let Command::StageGraph {
        plan: Some(plan),
        id: None,
        ..
    } = refused.value
    else {
        panic!("owning ingress refusal");
    };
    assert_eq!(plan.descriptor(), reference.descriptor());
    recycle(&mut receiver, id, plan);
    host.finish();

    let mut host = Host::new();
    let reference = host
        .control
        .compile_graph(&scenario::routes(false))
        .unwrap();
    let mut receiver = Receiver::new(7, reference.config()).unwrap();
    let ids = [receiver.id(0).unwrap(), receiver.id(1).unwrap()];
    for id in ids {
        transfer(&mut receiver, id, Epoch(1), &reference);
        let loan = guarded(|| receiver.take(id)).unwrap();
        guarded(|| {
            host.control
                .stage_graph(id.slot as usize, loan.epoch, loan.plan)
        })
        .unwrap_or_else(|e| panic!("stage: {:?}", e.error));
    }
    for (at, transition) in [
        (
            0,
            Transition::Record {
                monitor_previous: false,
            },
        ),
        (
            4,
            Transition::Play {
                position: 0,
                retrigger: false,
            },
        ),
        (12, Transition::Stop),
    ] {
        guarded(|| {
            host.control.submit(
                Lane::Normal,
                Frame(at),
                Operation::Transition {
                    target,
                    timing: Timing::At,
                    transition,
                },
            )
        })
        .unwrap_or_else(|e| panic!("musical request: {:?}", e.error));
    }
    host.step(1);
    host.step(12);
    let expected = crate::run(&[13]);
    assert_eq!(host.samples, expected.samples);
    assert_eq!(host.midi, expected.midi);
    for id in ids {
        assert_eq!(receiver.phase(id), Ok(Phase::Loaned));
        assert!(matches!(guarded(|| receiver.take(id)), Err(Error::Busy)));
        let before = receiver.last_sequence();
        assert_eq!(send(&mut receiver, id, Payload::Abort), Err(Error::Busy));
        assert_eq!(receiver.last_sequence(), before);
    }
    let mut ready = [None; 2];
    while let Some(completion) = guarded(|| host.control.try_completion()) {
        match completion.request {
            Command::StageGraph {
                slot,
                id: Some(graph),
                plan: None,
                ..
            } => {
                assert_eq!(completion.outcome, Outcome::Applied);
                ready[slot] = Some(graph);
            }
            Command::Schedule { .. } => {
                assert!(matches!(completion.outcome, Outcome::Scheduled { .. }))
            }
            _ => panic!("unexpected owning completion"),
        }
        assert!(completion.retired.is_none());
    }
    let mut transitions = Vec::new();
    while let Some(execution) = guarded(|| host.control.try_execution()) {
        assert_eq!(execution.result, Ok(()));
        transitions.push((
            execution.requested_frame.0,
            execution.delivered_frame.unwrap().0,
            execution.actual_frame.0,
        ));
    }
    assert_eq!(transitions, [(0, 0, 0), (4, 4, 4), (12, 12, 12)]);
    for graph in ready.into_iter().map(Option::unwrap) {
        guarded(|| host.control.release_graph(graph))
            .unwrap_or_else(|e| panic!("release: {:?}", e.error));
    }
    host.step(1);
    for id in ids {
        let completion = guarded(|| host.control.try_completion()).unwrap();
        assert_eq!(completion.outcome, Outcome::Applied);
        let Some(rt_runtime::executor::Retired::Plan(plan)) = completion.retired else {
            panic!("owning graph return");
        };
        assert_eq!(plan.descriptor(), reference.descriptor());
        recycle(&mut receiver, id, plan);
        assert_eq!(receiver.id(id.slot as usize).unwrap().generation, 2);
    }
    assert!(guarded(|| host.control.try_completion()).is_none());
    host.finish();

    for rendered in [false, true] {
        let mut host = Host::new();
        let reference = host
            .control
            .compile_graph(&scenario::routes(false))
            .unwrap();
        let mut receiver = Receiver::new(7, reference.config()).unwrap();
        let ids = [receiver.id(0).unwrap(), receiver.id(1).unwrap()];
        for id in ids {
            transfer(&mut receiver, id, Epoch(1), &reference);
            let loan = guarded(|| receiver.take(id)).unwrap();
            guarded(|| {
                host.control
                    .stage_graph(id.slot as usize, loan.epoch, loan.plan)
            })
            .unwrap_or_else(|e| panic!("stage before disconnect: {:?}", e.error));
        }
        if rendered {
            host.step(1);
        }
        host.finish();
        // Abandonment disposes runtime owners off-render; it does not fabricate returns.
        for id in ids {
            assert_eq!(receiver.phase(id), Ok(Phase::Loaned));
        }
    }
}
