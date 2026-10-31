mod support;
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{Command, GraphRuntime, PreparedNode},
    graph::{self, Connection, Endpoint, GraphDescription, PortCounts, PortType},
    planner::*,
    processor::{ProcessBlock, Processor},
    resource::*,
    transport::{Lane, Outcome},
    Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Revision,
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
fn node() -> NodeId {
    NodeId::from_parts(0, 1).unwrap()
}
fn planner() -> PlannerId {
    PlannerId::from_parts(0, 1).unwrap()
}
fn config() -> rt_runtime::PreparedConfig {
    Config {
        max_frames: 2,
        max_nodes: 2,
        max_ports: 8,
        max_edges: 4,
        audio_inputs: 0,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        resource_slots: 8,
        resource_pages: 4,
        page_bytes: 16,
        command_slots: 1,
        scheduled_events: 1,
        budgets: Budgets {
            commands: 1,
            events: 1,
            subranges: 0,
            transfer_bytes: 16,
            validation_steps: 1,
            publications: 1,
        },
        ..Config::default()
    }
    .prepare()
    .unwrap()
}
#[derive(Default)]
struct Counts {
    calls: AtomicUsize,
    processors: AtomicUsize,
    planners: AtomicUsize,
}
struct Owner {
    decisions: [f32; 1],
    finished: Frame,
    snapshot: Option<Snapshot>,
    counts: Arc<Counts>,
}
impl Planner for Owner {
    type Decision = f32;
    type Observation = Frame;
    type Operation = ();
    type Result = ();
    fn spec(&self) -> Option<PlannerSpec> {
        Some(PlannerSpec {
            id: planner(),
            decision_slots: 1,
            observation_capacity: 1,
            operation_bytes: 0,
        })
    }
    fn decisions(&self) -> &[f32] {
        &self.decisions
    }
    fn plan(&mut self, context: PlanContext<'_>) -> Result<Frame, Error> {
        assert_eq!(context.frame, self.finished);
        self.decisions[0] = 7.0 + context.frame.0 as f32;
        Ok(context.ceiling)
    }
    fn finish(&mut self, context: FinishContext<'_, Frame>) -> Result<(), Error> {
        assert_eq!(context.observations.len(), 1);
        assert_eq!(context.observations[0].producer, node());
        assert_eq!(context.observations[0].value, context.interval.start);
        self.finished = context.interval.end;
        Ok(())
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
        self.counts.planners.fetch_add(1, Ordering::Relaxed);
    }
}
struct Fill(Arc<Counts>);
impl Processor<Owner> for Fill {
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
            observations: 1,
            operation_bytes: 0,
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, Owner>) -> Result<(), Error> {
        let view = block.decision.ok_or(Error::InvalidState)?;
        assert_eq!(view.planner, planner());
        assert_eq!(view.slot, 0);
        assert_eq!(view.interval.start, block.frame);
        block.audio.output(0).unwrap().fill(*view.value);
        block.observations.push(block.frame)?;
        self.0.calls.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for Fill {
    fn drop(&mut self) {
        assert!(!support::is_rendering());
        self.0.processors.fetch_add(1, Ordering::Relaxed);
    }
}
fn setup(counts: &Arc<Counts>) -> (ResourceStore, Owner, Vec<PreparedNode<Fill, Owner>>) {
    let mut resources = ResourceStore::new(config()).unwrap();
    let id = resources.create().unwrap();
    let upload = resources
        .begin_upload(
            id,
            Revision(0),
            Description {
                format: Format::Audio { channels: 1 },
                frames: 1,
                bytes: 4,
            },
        )
        .unwrap();
    resources.begin_callback();
    resources
        .write_upload(upload, 0, &1f32.to_le_bytes())
        .unwrap();
    resources.finish_upload(upload).unwrap();
    resources.commit(upload).unwrap();
    let snapshot = resources.snapshot(id, Frame(0)).unwrap();
    (
        resources,
        Owner {
            decisions: [0.],
            finished: Frame(0),
            snapshot: Some(snapshot),
            counts: counts.clone(),
        },
        vec![PreparedNode::new(node(), Fill(counts.clone()))],
    )
}
fn graph() -> GraphDescription {
    GraphDescription {
        nodes: vec![node()],
        connections: vec![Connection {
            source: Endpoint::node(node(), PortType::Audio, 0),
            destination: Endpoint::host(PortType::Audio, 0),
        }],
    }
}
fn binding() -> DecisionBinding {
    DecisionBinding {
        planner: planner(),
        consumer: node(),
        decision_slot: 0,
    }
}
#[shoop_test]
fn prepared_graph_executes_at_frame_zero_and_extraction_retains_the_owner_ledger() {
    let counts = Arc::new(Counts::default());
    let (resources, owner, nodes) = setup(&counts);
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config(),
        nodes,
        owner,
        resources,
        &graph(),
        &[binding()],
    )
    .unwrap();
    assert_eq!(runtime.epoch(), Epoch(1));
    assert_eq!(runtime.next_frame(), Frame(0));
    assert_eq!(counts.calls.load(Ordering::Relaxed), 0);
    assert!(control.try_completion().is_none());
    let mut samples = [99.; 2];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 0,
                inputs: &[],
                outputs: &mut [&mut samples],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(samples, [99.; 2]);
    assert_eq!(counts.calls.load(Ordering::Relaxed), 0);
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 2,
                inputs: &[],
                outputs: &mut [&mut samples],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(samples, [7.; 2]);
    assert_eq!(runtime.next_frame(), Frame(2));
    assert_eq!(counts.calls.load(Ordering::Relaxed), 1);
    assert!(control.try_completion().is_none());
    drop(control);
    let (mut owner, mut resources) = runtime.into_planner_and_resources();
    assert_eq!(owner.finished, Frame(2));
    assert_eq!(counts.processors.load(Ordering::Relaxed), 1);
    assert_eq!(counts.planners.load(Ordering::Relaxed), 0);
    assert_no_heap(|| {
        let snapshot = owner.snapshot.take().unwrap();
        let index = resources.read_index(snapshot.id).unwrap();
        assert_eq!(resources.render_io().snapshot(index).unwrap(), snapshot);
        resources.release_snapshot(snapshot.id).unwrap();
        resources.remove(snapshot.resource).unwrap();
        for _ in 0..16 {
            resources.begin_callback();
            resources.reclaim();
        }
        assert_eq!(resources.usage().free_pages, 4);
        assert_eq!(resources.usage().free_versions, 8);
        assert_eq!(resources.usage().free_snapshots, 8);
    });
    drop(owner);
    assert_eq!(counts.planners.load(Ordering::Relaxed), 1);
}
#[shoop_test]
fn preparation_rejects_invalid_initial_signal_or_planning_graphs_and_drops_off_render() {
    for case in 0..4 {
        let counts = Arc::new(Counts::default());
        let (resources, owner, nodes) = setup(&counts);
        let mut graph = graph();
        let mut bindings = vec![binding()];
        match case {
            0 => bindings.clear(),
            1 => bindings[0].decision_slot = 1,
            2 => bindings[0].planner = PlannerId::from_parts(0, 2).unwrap(),
            _ => graph.connections.push(Connection {
                source: Endpoint::node(node(), PortType::Audio, 0),
                destination: Endpoint::node(node(), PortType::Audio, 0),
            }),
        }
        assert!(GraphRuntime::prepare_with_initial_graph(
            config(),
            nodes,
            owner,
            resources,
            &graph,
            &bindings
        )
        .is_err());
        assert_eq!(counts.calls.load(Ordering::Relaxed), 0);
        assert_eq!(counts.processors.load(Ordering::Relaxed), 1);
        assert_eq!(counts.planners.load(Ordering::Relaxed), 1);
    }
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
        assert!(block.decision.is_none());
        block.audio.output(0).unwrap().fill(4.);
        Ok(())
    }
}
#[shoop_test]
fn signal_only_initial_graph_preserves_normal_later_commit_epochs_and_receipts() {
    let cfg = config();
    let (mut control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        cfg,
        vec![PreparedNode::new(node(), Signal)],
        NoPlanner,
        ResourceStore::new(cfg).unwrap(),
        &graph(),
        &[],
    )
    .unwrap();
    let mut output = [0.; 2];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: Frame(0),
                frames: 2,
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
    assert_eq!(output, [4.; 2]);
    let plan = graph::compile(runtime.catalog(), &GraphDescription::default()).unwrap();
    let sequence = control
        .submit(Lane::Normal, Command::commit(Epoch(1), plan))
        .unwrap_or_else(|e| panic!("{:?}", e.error));
    let mut done = false;
    for _ in 0..16 {
        let frame = runtime.next_frame();
        assert_no_heap(|| {
            runtime.process(
                AudioBlock {
                    frame,
                    frames: 2,
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
        if let Some(result) = control.try_completion() {
            assert_eq!(result.sequence, sequence.sequence());
            assert_eq!(result.outcome, Outcome::Applied);
            assert_eq!(result.epoch, Epoch(2));
            assert_eq!(runtime.epoch(), Epoch(2));
            assert_eq!(output, [0.; 2]);
            done = true;
            break;
        } else {
            assert_eq!(output, [4.; 2]);
        }
    }
    assert!(done);
}
