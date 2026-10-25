mod support;

use rt_runtime::buffer::{AudioBlock, MidiBlock, MidiBuffer};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::*;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::transport::{Lane, Outcome};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId};
use shoop_wasm_test_support::shoop_test;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use support::assert_no_heap;

#[global_allocator]
static ALLOCATOR: support::TrackingAllocator = support::TrackingAllocator;

#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct TestProcessor {
    ports: PortCounts,
    gain: f32,
    counter: Option<u64>,
    drops: Arc<AtomicUsize>,
}
impl Processor for TestProcessor {
    fn set_parameter(&mut self, parameter: u32, value: f32) -> Result<(), Error> {
        if parameter != 0 {
            return Err(Error::InvalidParameter);
        }
        self.gain = value;
        Ok(())
    }
    fn ports(&self) -> PortCounts {
        self.ports
    }
    fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
        for port in 0..self.ports.audio_outputs {
            if let Some(counter) = &mut self.counter {
                for sample in block.audio.output(port).ok_or(Error::InvalidShape)? {
                    *sample = *counter as f32;
                    *counter += 1;
                }
            } else if port < self.ports.audio_inputs {
                let (source, target) = block.audio.pair(port, port).ok_or(Error::InvalidShape)?;
                for (source, target) in source.iter().zip(target) {
                    *target = *source * self.gain;
                }
            }
        }
        for port in 0..self.ports.midi_inputs.min(self.ports.midi_outputs) {
            for index in 0..block.midi_inputs[port].len() {
                let (offset, data) = block.midi_inputs[port]
                    .get(index)
                    .ok_or(Error::InvalidEvent)?;
                block.midi_outputs[port].push(offset, block.frames, data)?;
            }
        }
        Ok(())
    }
}
impl Drop for TestProcessor {
    fn drop(&mut self) {
        assert!(
            !support::is_rendering(),
            "processor destroyed during rendering"
        );
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}
fn id(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn audio(from: Option<NodeId>, source: u32, to: Option<NodeId>, target: u32) -> Connection {
    Connection {
        source: Endpoint {
            node: from,
            kind: PortType::Audio,
            port: source,
        },
        destination: Endpoint {
            node: to,
            kind: PortType::Audio,
            port: target,
        },
    }
}
fn midi(from: Option<NodeId>, source: u32, to: Option<NodeId>, target: u32) -> Connection {
    Connection {
        source: Endpoint {
            node: from,
            kind: PortType::Midi,
            port: source,
        },
        destination: Endpoint {
            node: to,
            kind: PortType::Midi,
            port: target,
        },
    }
}
fn node(
    slot: u32,
    gain: f32,
    ports: PortCounts,
    drops: &Arc<AtomicUsize>,
) -> PreparedNode<TestProcessor> {
    PreparedNode::new(
        id(slot),
        TestProcessor {
            gain,
            ports,
            counter: None,
            drops: Arc::clone(drops),
        },
    )
}
fn mono() -> PortCounts {
    PortCounts {
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 1,
        midi_outputs: 1,
    }
}
fn config() -> Config {
    Config {
        max_frames: 8,
        max_nodes: 8,
        max_ports: 64,
        audio_inputs: 2,
        audio_outputs: 2,
        midi_inputs: 2,
        midi_outputs: 2,
        command_slots: 2,
        budgets: Budgets {
            validation_steps: 3,
            ..Budgets::default()
        },
        ..Config::default()
    }
}
struct Harness {
    control: Control<TestProcessor>,
    runtime: GraphRuntime<TestProcessor>,
    input: [[f32; 8]; 2],
    output: [[f32; 8]; 2],
    midi_in: [MidiBuffer; 2],
    midi_out: [MidiBuffer; 2],
    budget: usize,
}
impl Harness {
    fn new(nodes: Vec<PreparedNode<TestProcessor>>) -> Self {
        Self::with_limits(nodes, config())
    }
    fn with_limits(nodes: Vec<PreparedNode<TestProcessor>>, limits: Config) -> Self {
        let (control, runtime) = GraphRuntime::prepare(limits.prepare().unwrap(), nodes).unwrap();
        Self {
            control,
            runtime,
            input: [[1.0; 8], [2.0; 8]],
            output: [[99.0; 8]; 2],
            midi_in: std::array::from_fn(|_| MidiBuffer::new(16, 128).unwrap()),
            midi_out: std::array::from_fn(|_| MidiBuffer::new(16, 128).unwrap()),
            budget: limits.budgets.validation_steps,
        }
    }
    fn submit(&mut self, command: Command<TestProcessor>) -> rt_runtime::Sequence {
        self.control
            .submit(Lane::Normal, command)
            .unwrap_or_else(|error| panic!("submit: {:?}", error.error))
            .sequence()
    }
    fn step(&mut self, frames: usize) {
        let [left, right] = &mut self.output;
        let frame = self.runtime.next_frame();
        assert_no_heap(|| {
            self.runtime.process(
                AudioBlock {
                    frame,
                    frames,
                    inputs: &[&self.input[0], &self.input[1]],
                    outputs: &mut [left, right],
                },
                MidiBlock {
                    inputs: &self.midi_in,
                    outputs: &mut self.midi_out,
                },
            )
        })
        .unwrap();
        assert!(self.runtime.diagnostics().validation_steps <= self.budget);
    }
    fn install(&mut self, graph: GraphDescription) {
        let plan = compile(self.runtime.catalog(), &graph).unwrap();
        let expected = self.submit(Command::commit(self.runtime.epoch(), plan));
        for _ in 0..128 {
            self.step(8);
            if let Some(result) = self.control.try_completion() {
                assert_eq!(result.sequence, expected);
                assert_eq!(result.outcome, Outcome::Applied);
                assert_eq!(result.epoch, self.runtime.epoch());
                assert_eq!(result.frame, Frame(self.runtime.next_frame().0 - 8));
                return;
            }
        }
        panic!("transaction did not complete within validation bound");
    }
}

#[shoop_test]
fn planar_fan_in_out_multichannel_routing_is_sample_exact_without_clipping() {
    let drops = Arc::new(AtomicUsize::new(0));
    let ports = PortCounts {
        audio_inputs: 2,
        audio_outputs: 2,
        ..mono()
    };
    let mut h = Harness::new(vec![
        node(0, 2.0, ports, &drops),
        node(1, 0.5, mono(), &drops),
    ]);
    h.input[0] = [1.0, -1.0, 0.5, -0.5, 2.0, -2.0, 0.0, 10.0];
    h.install(GraphDescription {
        nodes: vec![id(1), id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(None, 1, Some(id(0)), 1),
            audio(Some(id(0)), 0, Some(id(1)), 0),
            audio(None, 1, Some(id(1)), 0),
            audio(Some(id(1)), 0, None, 0),
            audio(Some(id(0)), 1, None, 0),
            audio(Some(id(0)), 0, None, 1),
        ],
    });
    for frames in [1, 3, 8, 5] {
        h.output = [[99.0; 8]; 2];
        h.step(frames);
        for index in 0..frames {
            assert_eq!(h.output[0][index], h.input[0][index] + 5.0);
            assert_eq!(h.output[1][index], h.input[0][index] * 2.0);
        }
        assert!(h
            .output
            .iter()
            .all(|channel| channel[frames..].iter().all(|&value| value == 99.0)));
        assert_eq!(h.runtime.diagnostics().nodes_processed, 2);
        assert_eq!(h.runtime.diagnostics().connections_processed, 7);
    }
    h.install(GraphDescription::default());
    assert_eq!(h.output, [[0.0; 8]; 2]);
    drop(h);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[shoop_test]
fn midi_merge_orders_offsets_then_sources_and_preserves_payload_bytes() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.midi_in[0].push(1, 8, &[0x90, 60, 100]).unwrap();
    h.midi_in[0].push(5, 8, &[0x80, 60, 0]).unwrap();
    h.midi_in[1].push(1, 8, &[0x90, 61, 110]).unwrap();
    h.midi_in[1].push(3, 8, &[0xf0, 1, 2, 3, 0xf7]).unwrap();
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            midi(None, 1, Some(id(0)), 0),
            midi(None, 0, Some(id(0)), 0),
            midi(Some(id(0)), 0, None, 0),
        ],
    });
    let result = &h.midi_out[0];
    assert_eq!(result.len(), 4);
    assert_eq!(result.get(0), Some((1, &[0x90, 60, 100][..])));
    assert_eq!(result.get(1), Some((1, &[0x90, 61, 110][..])));
    assert_eq!(result.get(2), Some((3, &[0xf0, 1, 2, 3, 0xf7][..])));
    assert_eq!(result.get(3), Some((5, &[0x80, 60, 0][..])));
    assert!(h.midi_out[1].is_empty());
}

#[shoop_test]
fn rewiring_keeps_state_running_and_activates_only_at_complete_block_boundaries() {
    let drops = Arc::new(AtomicUsize::new(0));
    let counter = PreparedNode::new(
        id(0),
        TestProcessor {
            ports: PortCounts {
                audio_outputs: 1,
                ..PortCounts::default()
            },
            gain: 0.0,
            counter: Some(0),
            drops,
        },
    );
    let mut h = Harness::new(vec![counter]);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![audio(Some(id(0)), 0, None, 0)],
    });
    assert_eq!(h.output[0], [0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    let next = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![id(0)],
            connections: vec![audio(Some(id(0)), 0, None, 1)],
        },
    )
    .unwrap();
    let sequence = h.submit(Command::commit(Epoch(1), next));
    let mut expected = 8.0;
    let mut old_cycles = 0;
    for cycle in 0..128 {
        let frames = [1, 3, 8][cycle % 3];
        h.step(frames);
        let channel = if h.runtime.epoch() == Epoch(1) {
            old_cycles += 1;
            0
        } else {
            1
        };
        for index in 0..frames {
            assert_eq!(h.output[channel][index], expected);
            assert_eq!(h.output[1 - channel][index], 0.0);
            expected += 1.0;
        }
        if let Some(result) = h.control.try_completion() {
            assert_eq!(result.sequence, sequence);
            assert_eq!(result.outcome, Outcome::Applied);
            assert!(matches!(result.retired, Some(Retired::Plan(_))));
            assert!(old_cycles > 0);
            return;
        }
    }
    panic!("rewire did not activate");
}

#[shoop_test]
fn stale_and_malformed_transactions_leave_the_last_valid_graph_audible() {
    let mut h = Harness::new(vec![]);
    h.install(GraphDescription {
        nodes: vec![],
        connections: vec![audio(None, 1, None, 0)],
    });
    let stale = compile(h.runtime.catalog(), &GraphDescription::default()).unwrap();
    h.submit(Command::commit(Epoch(0), stale));
    let invalid = PreparedPlan::from_descriptor(
        h.runtime.catalog().config(),
        PlanDescriptor {
            steps: vec![NodeStep {
                node: id(0),
                input_end: 0,
            }],
            connections: vec![],
        },
    )
    .unwrap();
    h.submit(Command::commit(Epoch(1), invalid));
    let mut outcomes = vec![];
    for _ in 0..32 {
        h.step(8);
        assert_eq!(h.output[0], [2.0; 8]);
        assert_eq!(h.runtime.epoch(), Epoch(1));
        while let Some(result) = h.control.try_completion() {
            assert!(matches!(
                result.request,
                Command::Commit { plan: Some(_), .. }
            ));
            outcomes.push(result.outcome);
        }
        if outcomes.len() == 2 {
            break;
        }
    }
    assert_eq!(
        outcomes,
        vec![
            Outcome::Rejected(Error::StaleEpoch),
            Outcome::Rejected(Error::InvalidId)
        ]
    );
}

#[shoop_test]
fn saturated_retirement_and_disconnected_control_never_free_during_rendering() {
    let mut h = Harness::new(vec![]);
    h.install(GraphDescription {
        nodes: vec![],
        connections: vec![audio(None, 0, None, 0)],
    });
    let second = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![audio(None, 1, None, 0)],
        },
    )
    .unwrap();
    let third = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![audio(None, 1, None, 1)],
        },
    )
    .unwrap();
    h.submit(Command::commit(Epoch(1), second));
    h.submit(Command::commit(Epoch(2), third));
    for _ in 0..64 {
        h.step(8);
    }
    assert_eq!(h.runtime.epoch(), Epoch(3));
    assert_eq!(h.output[1], [2.0; 8]);
    assert_eq!(h.control.outstanding(), 2);
    let refused = h
        .control
        .submit(Lane::Normal, Command::Remove { node: id(0) });
    assert!(matches!(refused, Err(error) if error.error == Error::Full));
    let Harness {
        control,
        mut runtime,
        input,
        mut output,
        midi_in,
        mut midi_out,
        ..
    } = h;
    drop(control); // Returned plans are reclaimed here, not in the next callback.
    for _ in 0..64 {
        let [left, right] = &mut output;
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: 8,
                    inputs: &[&input[0], &input[1]],
                    outputs: &mut [left, right],
                },
                MidiBlock {
                    inputs: &midi_in,
                    outputs: &mut midi_out,
                },
            )
        })
        .unwrap();
        assert_eq!(output[1], [2.0; 8]);
    }
    drop(runtime);
}

#[shoop_test]
fn removed_processors_are_reclaimed_on_control_and_slots_reject_stale_generations() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(Some(id(0)), 0, None, 0),
        ],
    });
    h.submit(Command::Remove { node: id(0) });
    h.step(8);
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::Busy)
    );
    h.install(GraphDescription::default());
    h.submit(Command::Remove { node: id(0) });
    h.step(8);
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    let completion = h.control.try_completion().unwrap();
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(matches!(completion.retired, Some(Retired::Processor(_))));
    assert_eq!(drops.load(Ordering::Relaxed), 0);
    drop(completion);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    h.submit(Command::Remove { node: id(0) });
    h.step(8);
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Rejected(Error::InvalidId)
    );
    let fresh = id(0).checked_next_generation().unwrap();
    h.submit(Command::Install {
        prepared: Some(PreparedNode::new(
            fresh,
            TestProcessor {
                ports: mono(),
                gain: 4.0,
                counter: None,
                drops: Arc::clone(&drops),
            },
        )),
    });
    h.step(8);
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Applied
    );
    h.install(GraphDescription {
        nodes: vec![fresh],
        connections: vec![
            audio(None, 0, Some(fresh), 0),
            audio(Some(fresh), 0, None, 0),
        ],
    });
    assert_eq!(h.output[0], [4.0; 8]);
    drop(h);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[shoop_test]
fn invalid_and_zero_host_blocks_do_not_consume_graph_transactions() {
    let mut h = Harness::new(vec![]);
    let plan = compile(
        h.runtime.catalog(),
        &GraphDescription {
            nodes: vec![],
            connections: vec![audio(None, 0, None, 0)],
        },
    )
    .unwrap();
    h.submit(Command::commit(Epoch(0), plan));
    let mut output = [99.0; 8];
    assert_no_heap(|| {
        h.runtime.process(
            AudioBlock {
                frame: Frame(99),
                frames: 0,
                inputs: &[],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(output, [99.0; 8]);
    assert_eq!(
        assert_no_heap(|| h.runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 8,
                inputs: &[],
                outputs: &mut [&mut output]
            },
            MidiBlock {
                inputs: &h.midi_in,
                outputs: &mut h.midi_out
            },
        )),
        Err(Error::InvalidShape)
    );
    assert_eq!(output, [0.0; 8]);
    assert_eq!(h.runtime.next_frame(), Frame(0));
    assert_eq!(h.runtime.epoch(), Epoch(0));
    assert!(h.control.try_completion().is_none());
    for _ in 0..32 {
        h.step(8);
    }
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Applied
    );
    assert_eq!(h.output[0], [1.0; 8]);
}

#[shoop_test]
fn incompatible_slot_replacement_returns_its_processor_without_rt_destruction() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.submit(Command::Remove { node: id(0) });
    h.step(8);
    drop(h.control.try_completion().unwrap());
    let replacement = PreparedNode::new(
        id(0).checked_next_generation().unwrap(),
        TestProcessor {
            ports: PortCounts {
                audio_outputs: 2,
                ..mono()
            },
            gain: 1.0,
            counter: None,
            drops: Arc::clone(&drops),
        },
    );
    h.submit(Command::Install {
        prepared: Some(replacement),
    });
    h.step(8);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    let result = h.control.try_completion().unwrap();
    assert_eq!(result.outcome, Outcome::Rejected(Error::InvalidShape));
    assert!(matches!(
        result.request,
        Command::Install { prepared: Some(_) }
    ));
    drop(result);
    assert_eq!(drops.load(Ordering::Relaxed), 2);
}

#[shoop_test]
fn scheduled_parameters_apply_at_exact_frames_and_report_execution_separately() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(Some(id(0)), 0, None, 0),
        ],
    });
    let start = h.runtime.next_frame();
    for (offset, value) in [(0, 2.0), (3, 4.0)] {
        h.submit(Command::Schedule {
            at: Frame(start.0 + offset),
            action: Action::Parameter {
                node: id(0),
                parameter: 0,
                value,
            },
            payload: Box::new([]),
        });
    }
    h.step(8);
    assert_eq!(h.output[0], [2.0, 2.0, 2.0, 4.0, 4.0, 4.0, 4.0, 4.0]);
    assert_eq!(h.runtime.diagnostics().scheduled_events, 2);
    assert_eq!(h.runtime.diagnostics().subranges, 2);
    for offset in [0, 3] {
        assert_eq!(
            h.control.try_completion().unwrap().outcome,
            Outcome::Scheduled {
                effective_frame: Frame(start.0 + offset),
                late_by: 0
            }
        );
        let execution = h.control.try_execution().unwrap();
        assert_eq!(execution.actual_frame, Frame(start.0 + offset));
        assert_eq!(execution.result, Ok(()));
    }
    assert!(h.control.try_execution().is_none());
}

#[shoop_test]
fn block_edge_parameters_and_injected_midi_preserve_host_offsets_across_splits() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(Some(id(0)), 0, None, 0),
            midi(None, 0, Some(id(0)), 0),
            midi(Some(id(0)), 0, None, 0),
        ],
    });
    let start = h.runtime.next_frame();
    h.midi_in[0].push(2, 8, &[0x90, 60, 100]).unwrap();
    h.midi_in[0].push(5, 8, &[0x80, 60, 0]).unwrap();
    h.submit(Command::Schedule {
        at: Frame(start.0 + 8),
        action: Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 3.0,
        },
        payload: Box::new([]),
    });
    h.submit(Command::Schedule {
        at: Frame(start.0 + 3),
        action: Action::Midi {
            node: id(0),
            port: 0,
        },
        payload: vec![0xf0, 0x7d, 1, 0xf7].into_boxed_slice(),
    });
    h.step(8);
    assert_eq!(h.output[0], [1.0; 8]);
    assert_eq!(h.midi_out[0].len(), 3);
    assert_eq!(h.midi_out[0].get(0), Some((2, &[0x90, 60, 100][..])));
    assert_eq!(h.midi_out[0].get(1), Some((3, &[0xf0, 0x7d, 1, 0xf7][..])));
    assert_eq!(h.midi_out[0].get(2), Some((5, &[0x80, 60, 0][..])));
    assert_eq!(
        h.control.try_execution().unwrap().actual_frame,
        Frame(start.0 + 3)
    );
    assert!(h.control.try_execution().is_none());
    h.midi_in[0].clear();
    h.step(4);
    assert_eq!(&h.output[0][..4], &[3.0; 4]);
    assert_eq!(
        h.control.try_execution().unwrap().actual_frame,
        Frame(start.0 + 8)
    );
}

#[shoop_test]
fn late_schedules_and_processor_parameter_errors_are_explicit() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(Some(id(0)), 0, None, 0),
        ],
    });
    let start = h.runtime.next_frame();
    h.submit(Command::Schedule {
        at: Frame(0),
        action: Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 2.0,
        },
        payload: Box::new([]),
    });
    h.submit(Command::Schedule {
        at: start,
        action: Action::Parameter {
            node: id(0),
            parameter: 99,
            value: 3.0,
        },
        payload: Box::new([]),
    });
    h.step(8);
    assert_eq!(h.output[0], [2.0; 8]);
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Scheduled {
            effective_frame: start,
            late_by: start.0
        }
    );
    assert_eq!(h.control.try_execution().unwrap().late_by(), start.0);
    assert_eq!(
        h.control.try_execution().unwrap().result,
        Err(Error::InvalidParameter)
    );
}

#[shoop_test]
fn midi_overflow_keeps_audio_running_and_recovers_through_reserved_one_event_sinks() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut limits = config();
    limits.midi_events_per_port = 4;
    limits.budgets.events = 1;
    limits.budgets.subranges = 2;
    let mut h = Harness::with_limits(vec![node(0, 2.0, mono(), &drops)], limits);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![
            audio(None, 0, Some(id(0)), 0),
            audio(Some(id(0)), 0, None, 0),
            midi(None, 0, Some(id(0)), 0),
            midi(None, 1, Some(id(0)), 0),
            midi(Some(id(0)), 0, None, 0),
        ],
    });
    for port in &mut h.midi_in {
        for offset in 0..3 {
            port.push(offset, 8, &[0x90, 60, 100]).unwrap();
        }
    }
    h.step(8);
    assert_eq!(h.output[0], [2.0; 8]);
    assert!(h.runtime.diagnostics().midi_overflows >= 1);
    assert!(h.midi_out.iter().all(MidiBuffer::is_empty));
    for port in &mut h.midi_in {
        port.clear();
    }
    let mut sink = MidiBuffer::new(1, 3).unwrap();
    for index in 0..32 {
        if index > 0 {
            h.step(8);
        }
        for port in 0..2 {
            assert!(h.runtime.midi_recovery_pending(port).unwrap());
            sink.clear();
            assert_eq!(
                assert_no_heap(|| h.runtime.drain_midi_recovery(port, &mut sink, 8)).unwrap(),
                1
            );
            assert_eq!(
                sink.get(0),
                Some((
                    0,
                    &[
                        0xb0 | (index / 2) as u8,
                        if index % 2 == 0 { 120 } else { 123 },
                        0
                    ][..]
                ))
            );
            sink.clear();
            assert_eq!(
                assert_no_heap(|| h.runtime.drain_midi_recovery(port, &mut sink, 8)).unwrap(),
                0
            );
        }
        assert_eq!(h.output[0], [2.0; 8]);
    }
    assert!(!h.runtime.midi_recovery_pending(0).unwrap());
    h.midi_in[0].push(0, 8, &[0x90, 61, 100]).unwrap();
    h.step(8);
    assert_eq!(h.midi_out[0].get(0), Some((0, &[0x90, 61, 100][..])));
}

#[shoop_test]
fn sysex_copy_budget_defers_admission_without_delaying_an_accepted_deadline() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut limits = config();
    limits.page_bytes = 4;
    limits.budgets.transfer_bytes = 4;
    let mut h = Harness::with_limits(vec![node(0, 1.0, mono(), &drops)], limits);
    h.install(GraphDescription {
        nodes: vec![id(0)],
        connections: vec![midi(Some(id(0)), 0, None, 0)],
    });
    let start = h.runtime.next_frame();
    for value in [1, 2] {
        h.submit(Command::Schedule {
            at: Frame(start.0 + 10),
            action: Action::Midi {
                node: id(0),
                port: 0,
            },
            payload: vec![0xf0, 0x7d, value, 0xf7].into_boxed_slice(),
        });
    }
    h.step(8);
    assert_eq!(h.runtime.diagnostics().transfer_bytes, 4);
    assert!(matches!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Scheduled { late_by: 0, .. }
    ));
    assert!(h.control.try_completion().is_none());
    assert!(h.control.try_execution().is_none());
    h.step(8);
    assert_eq!(h.runtime.diagnostics().transfer_bytes, 4);
    assert!(matches!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Scheduled { late_by: 0, .. }
    ));
    assert_eq!(h.midi_out[0].get(0), Some((2, &[0xf0, 0x7d, 1, 0xf7][..])));
    assert_eq!(h.midi_out[0].get(1), Some((2, &[0xf0, 0x7d, 2, 0xf7][..])));
    for _ in 0..2 {
        assert_eq!(
            h.control.try_execution().unwrap().actual_frame,
            Frame(start.0 + 10)
        );
    }
}

#[shoop_test]
fn scheduled_target_removed_before_execution_cannot_address_its_next_generation() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    h.submit(Command::Schedule {
        at: Frame(8),
        action: Action::Parameter {
            node: id(0),
            parameter: 0,
            value: 2.0,
        },
        payload: Box::new([]),
    });
    h.submit(Command::Remove { node: id(0) });
    h.step(8);
    assert!(matches!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Scheduled { .. }
    ));
    assert_eq!(
        h.control.try_completion().unwrap().outcome,
        Outcome::Applied
    );
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert!(h.control.try_execution().is_none());
    h.step(8);
    let execution = h.control.try_execution().unwrap();
    assert_eq!(execution.actual_frame, Frame(8));
    assert_eq!(execution.result, Err(Error::InvalidId));
}

#[shoop_test]
fn reset_command_uses_reserved_recovery_admission_when_normal_control_is_full() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let mut h = Harness::new(vec![node(0, 1.0, mono(), &drops)]);
    for value in [1.0, 2.0] {
        h.submit(Command::Schedule {
            at: Frame(1000),
            action: Action::Parameter {
                node: id(0),
                parameter: 0,
                value,
            },
            payload: Box::new([]),
        });
    }
    let sequence = h
        .control
        .submit(Lane::Recovery, Command::ResetMidi)
        .map_err(|error| error.error)
        .unwrap()
        .sequence();
    h.step(8);
    let completion = h.control.try_completion().unwrap();
    assert_eq!(completion.sequence, sequence);
    assert_eq!(completion.outcome, Outcome::Applied);
    assert!(h.runtime.midi_recovery_pending(0).unwrap());
    assert!(h.runtime.midi_recovery_pending(1).unwrap());
}

#[shoop_test]
fn scheduled_journal_is_sample_identical_under_seeded_block_partitions() {
    use rt_runtime::schedule::Action;
    let drops = Arc::new(AtomicUsize::new(0));
    let offsets = [0, 1, 7, 8, 15, 31, 32, 63];
    for seed in 1..=16u32 {
        let mut limits = config();
        limits.command_slots = 16;
        limits.budgets.commands = 16;
        let mut h = Harness::with_limits(vec![node(0, 1.0, mono(), &drops)], limits);
        h.install(GraphDescription {
            nodes: vec![id(0)],
            connections: vec![
                audio(None, 0, Some(id(0)), 0),
                audio(Some(id(0)), 0, None, 0),
            ],
        });
        let start = h.runtime.next_frame();
        for (index, offset) in offsets.iter().enumerate() {
            h.submit(Command::Schedule {
                at: Frame(start.0 + offset),
                action: Action::Parameter {
                    node: id(0),
                    parameter: 0,
                    value: (index + 1) as f32,
                },
                payload: Box::new([]),
            });
        }
        let mut random = seed;
        let mut rendered = 0;
        while rendered < 64 {
            random = random.wrapping_mul(1664525).wrapping_add(1013904223);
            let frames = (1 + random as usize % 8).min(64 - rendered);
            h.step(frames);
            for index in 0..frames {
                let expected = offsets
                    .iter()
                    .filter(|&&offset| offset <= (rendered + index) as u64)
                    .count() as f32;
                assert_eq!(h.output[0][index], expected);
            }
            assert!(h.runtime.diagnostics().subranges <= limits.budgets.events + 1);
            assert!(h.runtime.diagnostics().scheduled_events <= limits.budgets.events);
            rendered += frames;
        }
        for offset in offsets {
            let execution = h.control.try_execution().unwrap();
            assert_eq!(execution.actual_frame, Frame(start.0 + offset));
            assert_eq!(execution.result, Ok(()));
        }
    }
}

#[shoop_test]
fn overflow_delivers_reserved_resets_to_internal_processors_not_only_host_outputs() {
    struct Synth {
        active: bool,
        resets: Arc<AtomicUsize>,
    }
    impl Processor for Synth {
        fn ports(&self) -> PortCounts {
            PortCounts {
                audio_outputs: 1,
                midi_inputs: 1,
                ..PortCounts::default()
            }
        }
        fn process(&mut self, block: &mut ProcessBlock<'_>) -> Result<(), Error> {
            for index in 0..block.midi_inputs[0].len() {
                let (_, data) = block.midi_inputs[0].get(index).ok_or(Error::InvalidEvent)?;
                match rt_runtime::midi::Message::parse(data, 3)?.kind() {
                    rt_runtime::midi::MessageKind::NoteOn { .. } => self.active = true,
                    rt_runtime::midi::MessageKind::NoteOff { .. } => self.active = false,
                    rt_runtime::midi::MessageKind::ControlChange {
                        controller: 120, ..
                    } => {
                        self.active = false;
                        self.resets.fetch_add(1, Ordering::Relaxed);
                    }
                    _ => {}
                }
            }
            block
                .audio
                .output(0)
                .ok_or(Error::InvalidShape)?
                .fill(if self.active { 1.0 } else { 0.0 });
            Ok(())
        }
    }
    let resets = Arc::new(AtomicUsize::new(0));
    let limits = Config {
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 2,
        midi_outputs: 0,
        midi_events_per_port: 2,
        ..config()
    }
    .prepare()
    .unwrap();
    let (mut control, mut runtime) = GraphRuntime::prepare(
        limits,
        vec![PreparedNode::new(
            id(0),
            Synth {
                active: false,
                resets: Arc::clone(&resets),
            },
        )],
    )
    .unwrap();
    let plan = compile(
        runtime.catalog(),
        &GraphDescription {
            nodes: vec![id(0)],
            connections: vec![
                midi(None, 0, Some(id(0)), 0),
                midi(None, 1, Some(id(0)), 0),
                audio(Some(id(0)), 0, None, 0),
            ],
        },
    )
    .unwrap();
    control
        .submit(Lane::Normal, Command::commit(Epoch(0), plan))
        .map_err(|error| error.error)
        .unwrap();
    let mut input = [
        MidiBuffer::new(2, 6).unwrap(),
        MidiBuffer::new(2, 6).unwrap(),
    ];
    let mut output = [0.0; 8];
    for _ in 0..32 {
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: 8,
                    inputs: &[],
                    outputs: &mut [&mut output],
                },
                MidiBlock {
                    inputs: &input,
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
        if control.try_completion().is_some() {
            break;
        }
    }
    assert_eq!(runtime.epoch(), Epoch(1));
    input[0].push(0, 8, &[0x90, 60, 100]).unwrap();
    let frame = runtime.next_frame();
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames: 8,
                inputs: &[],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(output, [1.0; 8]);
    input[0].push(0, 8, &[0x90, 61, 100]).unwrap();
    input[1].push(0, 8, &[0x80, 60, 0]).unwrap();
    let frame = runtime.next_frame();
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames: 8,
                inputs: &[],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &input,
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(output, [0.0; 8]);
    assert_eq!(resets.load(Ordering::Relaxed), 16);
    assert!(runtime.diagnostics().midi_overflows > 0);
}

#[shoop_test]
fn seeded_dags_match_an_independent_scalar_reference_for_varying_blocks() {
    let drops = Arc::new(AtomicUsize::new(0));
    let mut random = 73u32;
    for _ in 0..64 {
        let mut h = Harness::new(
            (0..8)
                .map(|slot| node(slot, (slot + 1) as f32 * 0.125, mono(), &drops))
                .collect(),
        );
        let mut graph = GraphDescription {
            nodes: (0..8).rev().map(id).collect(),
            connections: vec![],
        };
        for target in 0..8 {
            graph
                .connections
                .push(audio(None, target % 2, Some(id(target)), 0));
            graph.connections.push(audio(Some(id(target)), 0, None, 0));
            for source in 0..target {
                random = random.wrapping_mul(1664525).wrapping_add(1013904223);
                if random & 3 == 0 {
                    graph
                        .connections
                        .push(audio(Some(id(source)), 0, Some(id(target)), 0));
                }
            }
        }
        // Interpret logical edges directly, independent of descriptor ranges and
        // executor buffer routing. Binary fractions keep comparison exact.
        let mut values = [0.0f32; 8];
        for target in 0..8 {
            let mut sum = if target % 2 == 0 { 1.0 } else { 2.0 };
            for source in 0..target {
                if graph.connections.contains(&audio(
                    Some(id(source as u32)),
                    0,
                    Some(id(target as u32)),
                    0,
                )) {
                    sum += values[source];
                }
            }
            values[target] = sum * (target + 1) as f32 * 0.125;
        }
        let expected: f32 = values.iter().sum();
        graph.connections.reverse();
        h.install(graph);
        for frames in [1, 7, 8, 3] {
            h.step(frames);
            assert_eq!(&h.output[0][..frames], &vec![expected; frames]);
            assert_eq!(&h.output[1][..frames], &vec![0.0; frames]);
        }
    }
    assert_eq!(drops.load(Ordering::Relaxed), 64 * 8);
}
