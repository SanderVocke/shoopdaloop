//! Nested musical control in an application-owned runtime with timing-only node pairs.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, Control, GraphRuntime, PreparedNode},
    graph::GraphDescription,
    resource::ResourceStore,
    transport::{Lane, Outcome as Admission, SubmitError, Ticket},
    Budgets, Config, Frame, NodeId, PlannerId, Revision, Sequence,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{
        Binding, LoopClient, LoopNode, LoopPlanner, Operation, Options, Prepared, Reply, Settings,
    },
    Entry, Limits, LoopId, LoopKind, LoopSpec, Mode, Outcome, ProgramDescription, Section, Timing,
    TopologyDescription, Transition,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};
#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
/// With no other processors, loop nodes are the runtime's node type.
struct Controls {
    runtime: Control<LoopNode, LoopPlanner>,
    loops: LoopClient,
}
impl Controls {
    fn submit(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
    ) -> Result<Ticket, SubmitError<Command<LoopNode, LoopPlanner>>> {
        self.runtime
            .submit_prepared(lane, self.loops.command(at, operation))
    }
}
fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn entry(target: u32, delay: u64, duration: u64, mode: Option<Mode>) -> Section {
    Section {
        entries: vec![Entry {
            target: id(target),
            delay,
            duration: Some(duration),
            repetitions: 1,
            mode,
        }],
    }
}
fn send(
    control: &mut Controls,
    frame: u64,
    target: u32,
    timing: Timing,
    transition: Transition,
) -> Sequence {
    control
        .submit(
            Lane::Normal,
            Frame(frame),
            Operation::Transition {
                target: id(target),
                timing,
                transition,
            },
        )
        .unwrap_or_else(|e| panic!("submit: {}", e.error))
        .sequence()
}
fn run(parts: &[usize]) {
    let config = Config {
        max_frames: 16,
        max_nodes: 4,
        max_ports: 1,
        max_edges: 1,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
        command_slots: 32,
        scheduled_events: 32,
        resource_slots: 8,
        resource_pages: 8,
        page_bytes: 16,
        budgets: Budgets {
            commands: 32,
            events: 32,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 32,
            subranges: 0,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap();
    let topology = compile(
        Limits {
            loop_slots: 4,
            pending_requests: 8,
            journal_events: 0,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: (0..4)
                .map(|slot| LoopSpec {
                    id: id(slot),
                    kind: if slot < 2 {
                        LoopKind::Primitive
                    } else {
                        LoopKind::Composite
                    },
                    length: if slot == 0 {
                        1
                    } else if slot == 1 {
                        2
                    } else {
                        0
                    },
                    sync_source: (slot >= 2).then_some(id(0)),
                })
                .collect(),
            programs: vec![
                ProgramDescription {
                    source: id(2),
                    nominal_unit_frames: 1,
                    sections: vec![entry(3, 1, 5, None)],
                },
                ProgramDescription {
                    source: id(3),
                    nominal_unit_frames: 1,
                    sections: vec![
                        entry(1, 0, 2, Some(Mode::Playing)),
                        entry(1, 0, 2, Some(Mode::Recording)),
                        entry(1, 0, 1, Some(Mode::Stopped)),
                    ],
                },
            ],
            ..TopologyDescription::default()
        },
    )
    .unwrap();
    let bindings = (0..2)
        .map(|slot| Binding {
            target: id(slot),
            playback: node(slot * 2),
            capture: node(slot * 2 + 1),
            channels: 0,
            midi: false,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 0,
            midi_bytes: 0,
            packet_bytes: 0,
        })
        .collect();
    let mut resources = ResourceStore::new(config).unwrap();
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        topology,
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 0,
        },
        bindings,
        vec![],
        Options::default(),
    )
    .unwrap();
    let graph = GraphDescription {
        nodes: (0..4).map(node).collect(),
        connections: vec![],
    };
    let (runtime_control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes
            .into_iter()
            .map(|(id, node)| PreparedNode::new(id, node))
            .collect(),
        planner,
        resources,
        &graph,
        &client.bindings_for(&graph),
    )
    .unwrap();
    let mut control = Controls {
        runtime: runtime_control,
        loops: client,
    };
    control.runtime.prepare_submissions().unwrap();
    send(
        &mut control,
        0,
        0,
        Timing::At,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    );
    send(
        &mut control,
        0,
        2,
        Timing::At,
        Transition::PlayIteration {
            iteration: 0,
            retrigger: true,
        },
    );
    for (frame, iteration) in [(3, 3), (6, 1), (8, 3)] {
        send(
            &mut control,
            frame,
            2,
            Timing::At,
            Transition::SeekIteration {
                iteration,
                retrigger: false,
            },
        );
    }
    send(&mut control, 8, 1, Timing::At, Transition::Stop);
    let pending = send(
        &mut control,
        9,
        2,
        Timing::AfterTicks {
            source: id(0),
            skip: 4,
        },
        Transition::PlayIteration {
            iteration: 2,
            retrigger: true,
        },
    );
    send(&mut control, 10, 2, Timing::At, Transition::Stop);
    send(&mut control, 10, 0, Timing::At, Transition::Stop);
    let queries: Vec<_> = (0..11)
        .map(|frame| {
            control
                .submit(
                    Lane::Normal,
                    Frame(frame),
                    Operation::State { target: id(1) },
                )
                .unwrap_or_else(|e| panic!("state: {}", e.error))
                .sequence()
        })
        .collect();
    for &frames in parts {
        if runtime.next_frame() == Frame(8) {
            let before = runtime.diagnostics();
            assert_no_heap(|| {
                runtime.process(
                    AudioBlock {
                        frame: Frame(8),
                        frames: 0,
                        inputs: &[],
                        outputs: &mut [],
                    },
                    MidiBlock {
                        inputs: &[],
                        outputs: &mut [],
                    },
                )
            })
            .unwrap();
            assert_eq!(runtime.next_frame(), Frame(8));
            assert_eq!(runtime.diagnostics(), before);
        }
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame: runtime.next_frame(),
                    frames,
                    inputs: &[],
                    outputs: &mut [],
                },
                MidiBlock {
                    inputs: &[],
                    outputs: &mut [],
                },
            )
        })
        .unwrap();
    }
    assert_eq!(runtime.next_frame(), Frame(11));
    let mut admitted = 0;
    while let Some(completion) = control.runtime.try_completion() {
        assert!(matches!(completion.outcome, Admission::Scheduled { .. }));
        admitted += 1;
    }
    assert_eq!(admitted, 20);
    let modes = [
        Mode::Stopped,
        Mode::Playing,
        Mode::Playing,
        Mode::Recording,
        Mode::Recording,
        Mode::Stopped,
        Mode::Playing,
        Mode::Playing,
        Mode::Stopped,
        Mode::Stopped,
        Mode::Stopped,
    ];
    let mut states = [None; 11];
    let mut cancelled = false;
    let mut terminal = 0;
    while let Some(execution) = control.runtime.try_execution() {
        terminal += 1;
        assert_eq!(execution.result, Ok(()));
        if execution.sequence == pending {
            assert_eq!(
                (
                    execution.requested_frame,
                    execution.delivered_frame,
                    execution.actual_frame
                ),
                (Frame(9), Some(Frame(9)), Frame(10))
            );
            let Some(Reply::Transition(completion)) = execution.operation_result else {
                panic!("countdown")
            };
            assert_eq!(
                (completion.requested, completion.effective),
                (Frame(9), Frame(9))
            );
            assert_eq!(completion.outcome, Outcome::Cancelled { frame: Frame(10) });
            cancelled = true;
        } else if let Some(index) = queries.iter().position(|seq| *seq == execution.sequence) {
            let Some(Reply::State(cut)) = execution.operation_result else {
                panic!("settled query")
            };
            assert_eq!(cut.frame, Frame(index as u64));
            assert_eq!(cut.state.mode, modes[index]);
            assert_eq!(
                cut.state.capture_frames,
                match index {
                    4 => 1,
                    5.. => 2,
                    _ => 0,
                }
            );
            assert_eq!(cut.state.capture_revision, Revision(u64::from(index >= 3)));
            assert_eq!(cut.state.content_revision, Revision(u64::from(index >= 5)));
            assert!(!cut.state.capture_incomplete);
            states[index] = Some(cut.state);
        } else {
            let Some(Reply::Transition(completion)) = execution.operation_result else {
                panic!("musical result")
            };
            assert!(matches!(completion.outcome, Outcome::Applied { .. }));
        }
    }
    assert_eq!(terminal, admitted);
    assert!(cancelled && states.iter().all(Option::is_some));
    assert_eq!(states[6].unwrap().position, 0);
    assert_eq!(states[7].unwrap().position, 1);
    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    planner.cleanup(&mut resources).unwrap();
    assert_eq!(planner.media_usage().references, 0);
    assert_eq!(resources.usage().free_pages, 8);
    assert_eq!(resources.usage().free_versions, 8);
    assert_eq!(resources.usage().free_snapshots, 8);
}
fn main() {
    run(&[11]);
    run(&[1; 11]);
    run(&[3, 5, 3]);
    println!("nested timing: clock, seek/record/freeze/replay, direct Stop precedence, pending cancellation and cleanup verified");
}
