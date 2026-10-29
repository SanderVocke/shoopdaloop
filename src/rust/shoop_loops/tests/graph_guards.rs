//! Generic runtime paths cannot bypass the loop's graph and node requirements.
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{compile_planned, GraphDescription},
    planner::DecisionBinding,
    resource::ResourceStore,
    transport::{Lane, Outcome as Admission},
    Budgets, Config, Epoch, Error, NodeId, PlannerId, PreparedConfig,
};
use shoop_loops::{
    audio::Smoothing,
    compile,
    runtime::{
        Binding, LoopNode, LoopPlanner, NodePairSettings, Operation, Options, Prepared, Reply,
        Settings, TopologySettings,
    },
    Limits, LoopId, LoopKind, LoopSpec, Mode, PreparedTopology, TopologyDescription,
};
use shoop_loops_test_support::{LoopControl, LoopRuntime, TestNode};
use shoop_wasm_test_support::{heap::assert_no_heap, shoop_test};

#[global_allocator]
static ALLOCATOR: shoop_wasm_test_support::heap::TrackingAllocator =
    shoop_wasm_test_support::heap::TrackingAllocator;
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

struct NoEffects;
impl rt_runtime::processor::Processor<LoopPlanner> for NoEffects {
    fn ports(&self) -> rt_runtime::graph::PortCounts {
        rt_runtime::graph::PortCounts::default()
    }
    fn process(
        &mut self,
        _: &mut rt_runtime::processor::ProcessBlock<'_, LoopPlanner>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

fn id(slot: u32) -> LoopId {
    LoopId::from_parts(slot, 1).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
fn config() -> PreparedConfig {
    Config {
        max_frames: 8,
        max_nodes: 6,
        max_ports: 1,
        max_edges: 1,
        audio_inputs: 0,
        audio_outputs: 0,
        midi_inputs: 0,
        midi_outputs: 0,
        command_slots: 8,
        scheduled_events: 8,
        resource_slots: 8,
        resource_pages: 8,
        page_bytes: 16,
        budgets: Budgets {
            commands: 8,
            events: 8,
            transfer_bytes: 64,
            validation_steps: 128,
            publications: 8,
            subranges: 0,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
/// Two timing-only loops in slots 0 and 1, plus an unassigned node-pair template in slot 2.
fn topology() -> PreparedTopology {
    compile(
        Limits {
            loop_slots: 3,
            pending_requests: 5,
            ..Limits::default()
        },
        &TopologyDescription {
            loops: (0..2)
                .map(|slot| LoopSpec {
                    id: id(slot),
                    kind: LoopKind::Primitive,
                    length: 0,
                    sync_source: None,
                })
                .collect(),
            ..TopologyDescription::default()
        },
    )
    .unwrap()
}
fn settings() -> Settings {
    Settings {
        planner: PlannerId::from_parts(0, 1).unwrap(),
        normal_requests: 4,
        media_slots: 8,
        export_slots: 0,
    }
}
fn bindings() -> Vec<Binding> {
    (0..3)
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
        .collect()
}
fn graph(nodes: impl IntoIterator<Item = u32>) -> GraphDescription {
    GraphDescription {
        nodes: nodes.into_iter().map(node).collect(),
        connections: vec![],
    }
}
fn prepare() -> (LoopControl<NoEffects>, LoopRuntime<NoEffects>) {
    LoopRuntime::prepare_with_configuration_staging(
        config(),
        topology(),
        ResourceStore::new(config()).unwrap(),
        settings(),
        bindings(),
        vec![],
        vec![],
        graph(0..4),
        TopologySettings::default(),
        NodePairSettings {
            candidates: 1,
            sources: 0,
            source_packets_per_callback: 0,
        },
        None,
    )
    .unwrap()
}
fn step(runtime: &mut LoopRuntime<NoEffects>) {
    let frame = runtime.next_frame();
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame,
                frames: 1,
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
fn outcome(
    control: &mut LoopControl<NoEffects>,
    runtime: &mut LoopRuntime<NoEffects>,
    command: Command<TestNode<NoEffects>, LoopPlanner>,
) -> Admission {
    let ticket = control
        .submit_command(Lane::Normal, command)
        .unwrap_or_else(|e| panic!("submit: {:?}", e.error));
    step(runtime);
    let completion = control.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    completion.outcome
}
/// Both loops still settle and answer state queries after a refused change.
fn assert_loops_unchanged(
    control: &mut LoopControl<NoEffects>,
    runtime: &mut LoopRuntime<NoEffects>,
) {
    for slot in 0..2 {
        let at = runtime.next_frame();
        control
            .submit(Lane::Normal, at, Operation::State { target: id(slot) })
            .unwrap_or_else(|e| panic!("state: {:?}", e.error));
    }
    step(runtime);
    while control.try_completion().is_some() {}
    for _ in 0..2 {
        let execution = control.try_execution().unwrap();
        assert_eq!(execution.result, Ok(()));
        let Some(Reply::State(cut)) = execution.operation_result else {
            panic!("state reply");
        };
        assert_eq!(cut.state.mode, Mode::Stopped);
        assert_eq!(cut.graph_epoch, Epoch(1));
    }
}

#[shoop_test]
fn generic_commit_without_a_loop_node_is_refused() {
    let (mut control, mut runtime) = prepare();
    let plan = control.compile_graph(&graph([0, 2, 3])).unwrap();
    assert_eq!(
        outcome(&mut control, &mut runtime, Command::commit(Epoch(1), plan)),
        Admission::Rejected(Error::InvalidShape)
    );
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_loops_unchanged(&mut control, &mut runtime);
}

#[shoop_test]
fn generic_commit_with_crossed_decision_bindings_is_refused() {
    let (mut control, mut runtime) = prepare();
    let planner = PlannerId::from_parts(0, 1).unwrap();
    let crossed: Vec<_> = (0..4)
        .map(|slot| DecisionBinding {
            planner,
            consumer: node(slot),
            decision_slot: 1 - (slot / 2) as usize,
        })
        .collect();
    let plan = compile_planned(control.catalog(), &graph(0..4), &crossed).unwrap();
    assert_eq!(
        outcome(&mut control, &mut runtime, Command::commit(Epoch(1), plan)),
        Admission::Rejected(Error::InvalidShape)
    );
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_loops_unchanged(&mut control, &mut runtime);
}

#[shoop_test]
fn generic_processor_changes_cannot_replace_loop_nodes() {
    let (mut control, mut runtime) = prepare();
    // The template's nodes are outside the active graph, so only the loop refuses removal.
    assert_eq!(
        outcome(
            &mut control,
            &mut runtime,
            Command::Remove { node: node(4) }
        ),
        Admission::Rejected(Error::Busy)
    );
    let replacement = node(5).checked_next_generation().unwrap();
    assert_eq!(
        outcome(
            &mut control,
            &mut runtime,
            Command::Install {
                prepared: Some(PreparedNode::new(replacement, TestNode::Effect(NoEffects))),
            },
        ),
        Admission::Rejected(Error::Busy)
    );
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_loops_unchanged(&mut control, &mut runtime);
}

#[shoop_test]
fn an_application_runtime_refuses_an_initial_graph_without_loop_nodes() {
    let mut resources = ResourceStore::new(config()).unwrap();
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config(),
        topology(),
        &mut resources,
        settings(),
        bindings(),
        vec![],
        Options {
            node_pairs: NodePairSettings {
                candidates: 1,
                sources: 0,
                source_packets_per_callback: 0,
            },
            ..Options::default()
        },
    )
    .unwrap();
    let initial = graph([0, 1, 2]);
    let refused = GraphRuntime::prepare_with_initial_graph(
        config(),
        nodes
            .into_iter()
            .map(|(id, node)| PreparedNode::new(id, node))
            .collect::<Vec<PreparedNode<LoopNode, LoopPlanner>>>(),
        planner,
        resources,
        &initial,
        &client.bindings_for(&initial),
    );
    assert_eq!(refused.err(), Some(Error::InvalidShape));
}
