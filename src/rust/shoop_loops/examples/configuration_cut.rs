//! A stopped logical-generation/media/route cut over fixed physical templates.

use rt_runtime::buffer::{AudioBlock, MidiBlock};
use rt_runtime::executor::{Command, Control, GraphRuntime, PreparedNode, Retired};
use rt_runtime::graph::{
    Catalog, Connection, Endpoint, GraphDescription, PortCounts, PortType, PreparedPlan,
};
use rt_runtime::planner::ConsumerAccess;
use rt_runtime::processor::{ProcessBlock, Processor};
use rt_runtime::resource::{
    CaptureGroupConfig, Cursor, Reply as ResourceReply, Request, ResourceStore,
};
use rt_runtime::transport::{Lane, Outcome as TransportOutcome, SubmitError, Ticket};
use rt_runtime::{Budgets, Config, Epoch, Error, Frame, NodeId, PlannerId, Revision, Sequence};
use shoop_loops::audio::Smoothing;
use shoop_loops::media::{Export, ExportId};
use shoop_loops::runtime::{
    Binding, Failure, GraphStageId, LoopClient, LoopNode, LoopPlanner, NodePairInitial,
    NodePairSettings, NodePairSourceStatus, NodePairStatus, Operation, Options, Prepared, Reply,
    Settings, TopologySettings,
};
use shoop_loops::topology::staging::{StageId, Status};
use shoop_loops::{
    compile, Limits, LoopId, LoopKind, LoopSpec, Mode, PreparedTopology, Timing,
    TopologyDescription, Transition,
};
use shoop_wasm_test_support::heap::{assert_no_heap, TrackingAllocator};

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;
fn owner(generation: u32) -> LoopId {
    LoopId::from_parts(0, generation).unwrap()
}
fn node(slot: u32) -> NodeId {
    NodeId::from_parts(slot, 1).unwrap()
}
struct Half;
impl Processor<LoopPlanner> for Half {
    fn ports(&self) -> PortCounts {
        PortCounts {
            audio_inputs: 1,
            audio_outputs: 1,
            ..PortCounts::default()
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        let (input, output) = block.audio.pair(0, 0).ok_or(Error::InvalidShape)?;
        for (input, output) in input.iter().zip(output) {
            *output = *input * 0.5;
        }
        Ok(())
    }
}
/// The application's node type: loop nodes next to its own processors.
enum Node {
    Loop(LoopNode),
    Half(Half),
}
impl Processor<LoopPlanner> for Node {
    fn ports(&self) -> PortCounts {
        match self {
            Self::Loop(node) => node.ports(),
            Self::Half(node) => node.ports(),
        }
    }
    fn planning(&self) -> ConsumerAccess {
        match self {
            Self::Loop(node) => node.planning(),
            Self::Half(node) => node.planning(),
        }
    }
    fn process(&mut self, block: &mut ProcessBlock<'_, LoopPlanner>) -> Result<(), Error> {
        match self {
            Self::Loop(node) => node.process(block),
            Self::Half(node) => node.process(block),
        }
    }
}
type Submitted = Result<Ticket, SubmitError<Command<Node, LoopPlanner>>>;
/// The application's control side: its runtime endpoint, the loop vocabulary and
/// a catalog for compiling route changes.
struct Controls {
    runtime: Control<Node, LoopPlanner>,
    loops: LoopClient,
    catalog: Catalog,
}
impl Controls {
    fn submit(&mut self, lane: Lane, at: Frame, operation: Operation) -> Submitted {
        self.runtime
            .submit_prepared(lane, self.loops.command(at, operation))
    }
    fn submit_payload(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
        payload: Box<[u8]>,
    ) -> Submitted {
        self.runtime.submit_prepared(
            lane,
            self.loops.command_with_payload(at, operation, payload),
        )
    }
    fn compile_graph(&self, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
        self.loops.compile_graph(&self.catalog, graph)
    }
    fn stage_graph(&mut self, slot: usize, expected_epoch: Epoch, plan: PreparedPlan) -> Submitted {
        self.runtime.submit_prepared(
            Lane::Normal,
            Command::stage_graph(slot, expected_epoch, plan),
        )
    }
    fn release_graph(&mut self, id: GraphStageId) -> Submitted {
        self.runtime
            .submit_prepared(Lane::Normal, Command::ReleaseGraph { id })
    }
    fn read(&mut self, cursor: Cursor, bytes: Box<[u8]>) -> Submitted {
        self.runtime.submit_prepared(
            Lane::Normal,
            Command::resource(Request::Read { cursor, bytes }),
        )
    }
}
fn limits() -> Limits {
    Limits {
        loop_slots: 1,
        relations: 4,
        program_entries: 4,
        program_segments: 8,
        pending_requests: 8,
        journal_events: 0,
    }
}
fn topology(generation: u32, length: u64) -> PreparedTopology {
    compile(
        limits(),
        &TopologyDescription {
            loops: vec![LoopSpec {
                id: owner(generation),
                kind: LoopKind::Primitive,
                length,
                sync_source: None,
            }],
            ..TopologyDescription::default()
        },
    )
    .unwrap()
}
fn route(source: Endpoint, destination: Endpoint) -> Connection {
    Connection {
        source,
        destination,
    }
}
fn graph(scaled_capture: bool) -> GraphDescription {
    let mut connections = vec![route(
        Endpoint::node(node(0), PortType::Audio, 0),
        Endpoint::host(PortType::Audio, 0),
    )];
    if scaled_capture {
        connections.push(route(
            Endpoint::host(PortType::Audio, 0),
            Endpoint::node(node(2), PortType::Audio, 0),
        ));
        connections.push(route(
            Endpoint::node(node(2), PortType::Audio, 0),
            Endpoint::node(node(1), PortType::Audio, 0),
        ));
    } else {
        connections.push(route(
            Endpoint::host(PortType::Audio, 0),
            Endpoint::node(node(1), PortType::Audio, 0),
        ));
    }
    GraphDescription {
        nodes: vec![node(0), node(2), node(1)],
        connections,
    }
}
fn step(runtime: &mut GraphRuntime<Node, LoopPlanner>, input: &[f32]) -> [f32; 4] {
    let mut output = [-99.0; 4];
    assert_no_heap(|| {
        runtime.process(
            AudioBlock {
                frame: runtime.next_frame(),
                frames: input.len(),
                inputs: &[input],
                outputs: &mut [&mut output],
            },
            MidiBlock {
                inputs: &[],
                outputs: &mut [],
            },
        )
    })
    .unwrap();
    assert_eq!(&output[input.len()..], &[-99.0; 4][input.len()..]);
    output
}
fn music(
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    input: &[f32; 3],
    parts: &[usize],
) -> [f32; 3] {
    let mut output = [0.0; 3];
    let mut offset = 0;
    for &frames in parts {
        let samples = step(runtime, &input[offset..offset + frames]);
        output[offset..offset + frames].copy_from_slice(&samples[..frames]);
        offset += frames;
    }
    assert_eq!(offset, 3);
    output
}
fn send(control: &mut Controls, at: Frame, operation: Operation) -> Sequence {
    control
        .submit(Lane::Normal, at, operation)
        .unwrap_or_else(|e| panic!("submit: {}", e.error))
        .sequence()
}
fn replies(control: &mut Controls, sequences: &[Sequence]) -> Vec<Reply> {
    for _ in sequences {
        let completion = control.runtime.try_completion().unwrap();
        assert!(sequences.contains(&completion.sequence));
        assert!(matches!(
            completion.outcome,
            TransportOutcome::Scheduled { .. }
        ));
        drop(completion);
    }
    let mut replies = vec![None; sequences.len()];
    for _ in sequences {
        let execution = control.runtime.try_execution().unwrap();
        assert_eq!(execution.result, Ok(()));
        let slot = sequences
            .iter()
            .position(|s| *s == execution.sequence)
            .unwrap();
        assert!(replies[slot].is_none());
        replies[slot] = execution.operation_result;
    }
    assert!(control.runtime.try_completion().is_none());
    assert!(control.runtime.try_execution().is_none());
    replies.into_iter().map(Option::unwrap).collect()
}
fn operation(
    control: &mut Controls,
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    operation: Operation,
) -> (Reply, f32) {
    let sequence = send(control, runtime.next_frame(), operation);
    let sample = step(runtime, &[0.0])[0];
    (replies(control, &[sequence])[0], sample)
}
fn payload(
    control: &mut Controls,
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    operation: Operation,
    bytes: Box<[u8]>,
) -> Reply {
    let sequence = control
        .submit_payload(Lane::Normal, runtime.next_frame(), operation, bytes)
        .unwrap_or_else(|e| panic!("payload: {}", e.error))
        .sequence();
    step(runtime, &[0.0]);
    replies(control, &[sequence])[0]
}
fn ready_topology(
    control: &mut Controls,
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    candidate: &PreparedTopology,
) -> StageId {
    let mut header = [0; 64];
    candidate.encode_header(&mut header);
    let Reply::TopologyStatus { id, .. } = payload(
        control,
        runtime,
        Operation::TopologyBegin { slot: 0 },
        Box::new(header),
    ) else {
        panic!("topology begin")
    };
    for first_record in 0..candidate.encoded_records() {
        let mut record = [0; 64];
        candidate.encode_record(first_record, &mut record).unwrap();
        payload(
            control,
            runtime,
            Operation::TopologyAppend { id, first_record },
            Box::new(record),
        );
    }
    for _ in 0..512 {
        let Reply::TopologyProgress { progress, .. } =
            operation(control, runtime, Operation::TopologyValidate { id }).0
        else {
            panic!("topology progress")
        };
        if progress.ready {
            return id;
        }
    }
    panic!("finite topology validation exceeded the example bound")
}
fn transition(target: LoopId, transition: Transition) -> Operation {
    Operation::Transition {
        target,
        timing: Timing::At,
        transition,
    }
}
fn play(target: LoopId) -> Operation {
    transition(
        target,
        Transition::Play {
            position: 0,
            retrigger: false,
        },
    )
}
fn exported(reply: Reply, target: LoopId, revision: u64) -> (ExportId, Export) {
    let Reply::Export { id, media, .. } = reply else {
        panic!("export")
    };
    assert_eq!(media.view.description.owner, target);
    assert_eq!(media.view.description.revision, Revision(revision));
    assert_eq!(media.view.description.frames, 2);
    (id, media)
}
fn read_audio(
    control: &mut Controls,
    runtime: &mut GraphRuntime<Node, LoopPlanner>,
    export: ExportId,
) -> [f32; 2] {
    let Reply::Cursor(cursor) = operation(
        control,
        runtime,
        Operation::Cursor {
            export,
            midi: false,
        },
    )
    .0
    else {
        panic!("cursor")
    };
    let ticket = control
        .read(cursor, vec![0; 16].into_boxed_slice())
        .unwrap_or_else(|e| panic!("read: {}", e.error));
    step(runtime, &[0.0]);
    let completion = control.runtime.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, TransportOutcome::Applied);
    let Command::Resource(command) = completion.request else {
        panic!("read owner")
    };
    assert_eq!(
        command.reply(),
        Some(ResourceReply::Read {
            written: 8,
            eof: true
        })
    );
    let Request::Read { bytes, .. } = command.into_request() else {
        panic!("read bytes")
    };
    std::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()))
}
fn run(parts: &[usize]) {
    let config = Config {
        max_frames: 4,
        max_nodes: 4,
        max_ports: 8,
        max_edges: 8,
        audio_inputs: 1,
        audio_outputs: 1,
        midi_inputs: 0,
        midi_outputs: 0,
        midi_events_per_port: 8,
        midi_bytes_per_port: 128,
        command_slots: 8,
        scheduled_events: 8,
        resource_slots: 16,
        resource_pages: 64,
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
    .unwrap();
    let mut resources = ResourceStore::with_capture_groups(
        config,
        None,
        CaptureGroupConfig {
            groups: 2,
            members_per_group: 1,
            staging_bytes_per_group: 16,
        },
    )
    .unwrap();
    let Prepared {
        planner,
        nodes,
        client,
    } = LoopPlanner::prepare(
        config,
        topology(1, 0),
        &mut resources,
        Settings {
            planner: PlannerId::from_parts(0, 1).unwrap(),
            normal_requests: 4,
            media_slots: 8,
            export_slots: 3,
        },
        vec![Binding {
            target: owner(1),
            playback: node(0),
            capture: node(1),
            channels: 1,
            midi: false,
            gain: 1.0,
            smoothing: Smoothing::None,
            midi_events: 0,
            midi_bytes: 0,
            packet_bytes: 0,
        }],
        vec![],
        Options {
            staging: TopologySettings {
                candidates: 1,
                records_per_callback: 1,
                validation_steps_per_callback: 64,
            },
            node_pairs: NodePairSettings {
                candidates: 1,
                sources: 1,
                source_packets_per_callback: 1,
            },
            observer: None,
        },
    )
    .unwrap();
    let initial = graph(false);
    let nodes = nodes
        .into_iter()
        .map(|(id, node)| PreparedNode::new(id, Node::Loop(node)))
        .chain([PreparedNode::new(node(2), Node::Half(Half))])
        .collect();
    let (runtime_control, mut runtime) = GraphRuntime::prepare_with_initial_graph(
        config,
        nodes,
        planner,
        resources,
        &initial,
        &client.bindings_for(&initial),
    )
    .unwrap();
    let mut control = Controls {
        catalog: runtime.catalog().clone(),
        runtime: runtime_control,
        loops: client,
    };
    control.runtime.prepare_submissions().unwrap();
    runtime.prepare_graph_staging(1).unwrap();
    let record = send(
        &mut control,
        Frame(0),
        transition(
            owner(1),
            Transition::Record {
                monitor_previous: false,
            },
        ),
    );
    let playback = send(&mut control, Frame(2), play(owner(1)));
    let old_export = send(
        &mut control,
        Frame(2),
        Operation::Export {
            target: owner(1),
            selection: shoop_loops::runtime::Selection::Latest,
        },
    );
    assert_eq!(
        music(&mut runtime, &[0.5, 0.25, 0.0], parts),
        [0.0, 0.0, 0.5]
    );
    let initial = replies(&mut control, &[record, playback, old_export]);
    let (old_export, old) = exported(initial[2], owner(1), 1);
    assert_eq!(old.view.audio.unwrap().snapshot.cut_frame, Frame(2));
    operation(
        &mut control,
        &mut runtime,
        transition(owner(1), Transition::Stop),
    );
    let Reply::NodePairSource {
        id: source,
        status: NodePairSourceStatus::Ready { max_packet: 0 },
        ..
    } = operation(
        &mut control,
        &mut runtime,
        Operation::NodePairSnapshot {
            resource: old.view.audio.unwrap().snapshot.resource,
        },
    )
    .0
    else {
        panic!("source")
    };
    let Reply::NodePairStatus {
        id: node_pairs,
        status:
            NodePairStatus::Receiving {
                received: 0,
                expected: 1,
            },
    } = operation(
        &mut control,
        &mut runtime,
        Operation::NodePairBegin {
            slot: 0,
            changes: 1,
        },
    )
    .0
    else {
        panic!("node_pair begin")
    };
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::NodePairAppend {
                id: node_pairs,
                previous: Some(owner(1)),
                initial: Some(NodePairInitial {
                    target: owner(2),
                    frames: 2,
                    audio: Some(source),
                    midi: None,
                    gain: 1.0,
                    preserve_input: false
                }),
            }
        )
        .0,
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Ready
        }
    );
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::NodePairSourceRelease { id: source }
        )
        .0,
        Reply::Failed(Failure::Runtime(Error::InvalidId))
    );
    let topology = ready_topology(&mut control, &mut runtime, &topology(2, 2));
    let candidate = control.compile_graph(&graph(true)).unwrap();
    let ticket = control
        .stage_graph(0, Epoch(1), candidate)
        .unwrap_or_else(|e| panic!("stage: {}", e.error));
    step(&mut runtime, &[0.0]);
    let completion = control.runtime.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, TransportOutcome::Applied);
    let Command::StageGraph {
        plan: None,
        id: Some(graph),
        ..
    } = &completion.request
    else {
        panic!("Ready graph")
    };
    let graph = *graph;
    drop((completion, ticket));
    let activate = Operation::ConfigurationActivate {
        node_pairs,
        topology,
        graph,
        graph_epoch: Epoch(1),
        topology_epoch: Epoch(0),
    };
    assert_eq!(operation(&mut control, &mut runtime, play(owner(1))).1, 0.5);
    let (refused, sample) = operation(&mut control, &mut runtime, activate);
    assert_eq!(
        refused,
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidState))
    );
    assert_eq!(sample, 0.25);
    let at = runtime.next_frame();
    let stop = send(&mut control, at, transition(owner(1), Transition::Stop));
    let same_frame = send(&mut control, at, activate);
    assert_eq!(step(&mut runtime, &[0.0])[0], 0.0);
    assert_eq!(replies(&mut control, &[stop, same_frame])[1], refused);
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::NodePairStatus { id: node_pairs }
        )
        .0,
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Ready
        }
    );
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::TopologyStatus { id: topology }
        )
        .0,
        Reply::TopologyStatus {
            id: topology,
            status: Status::Ready
        }
    );
    let cut = runtime.next_frame();
    let sequences = [
        send(&mut control, cut, activate),
        send(&mut control, cut, play(owner(2))),
        send(&mut control, cut, Operation::State { target: owner(2) }),
        send(&mut control, cut, Operation::State { target: owner(1) }),
        send(
            &mut control,
            cut,
            Operation::Export {
                target: owner(2),
                selection: shoop_loops::runtime::Selection::Playback,
            },
        ),
    ];
    let before = runtime.diagnostics();
    step(&mut runtime, &[]);
    assert_eq!(runtime.next_frame(), cut);
    assert_eq!(runtime.diagnostics(), before);
    assert!(control.runtime.try_completion().is_none());
    assert!(control.runtime.try_execution().is_none());
    assert_eq!(music(&mut runtime, &[0.0; 3], parts), [0.5, 0.25, 0.5]);
    let results = replies(&mut control, &sequences);
    assert_eq!(
        results[0],
        Reply::ConfigurationActivated {
            node_pairs,
            topology,
            graph,
            graph_epoch: Epoch(2),
            topology_epoch: Epoch(1)
        }
    );
    let Reply::State(state) = results[2] else {
        panic!("settled new owner")
    };
    assert_eq!(
        (state.frame, state.graph_epoch, state.topology_epoch),
        (cut, Epoch(2), Epoch(1))
    );
    assert_eq!(
        (
            state.state.id,
            state.state.mode,
            state.state.content_revision
        ),
        (owner(2), Mode::Playing, Revision(0))
    );
    assert_eq!(
        results[3],
        Reply::Failed(Failure::Timeline(shoop_loops::Error::InvalidId))
    );
    let (new_initial_export, new_initial) = exported(results[4], owner(2), 0);
    assert_eq!(new_initial.view.audio.unwrap().snapshot.cut_frame, Frame(2));
    assert_eq!(new_initial.requested_cut, cut);
    let start = runtime.next_frame();
    let sequences = [
        send(
            &mut control,
            start,
            transition(
                owner(2),
                Transition::Record {
                    monitor_previous: false,
                },
            ),
        ),
        send(&mut control, start.checked_add(2).unwrap(), play(owner(2))),
        send(
            &mut control,
            start.checked_add(2).unwrap(),
            Operation::Export {
                target: owner(2),
                selection: shoop_loops::runtime::Selection::Latest,
            },
        ),
    ];
    assert_eq!(
        music(&mut runtime, &[2.0, 4.0, 0.0], parts),
        [0.0, 0.0, 1.0]
    );
    let latest = replies(&mut control, &sequences);
    let (new_export, new) = exported(latest[2], owner(2), 1);
    assert_eq!(
        new.view.audio.unwrap().snapshot.cut_frame,
        start.checked_add(2).unwrap()
    );
    assert_eq!(
        read_audio(&mut control, &mut runtime, old_export),
        [0.5, 0.25]
    );
    assert_eq!(
        read_audio(&mut control, &mut runtime, new_initial_export),
        [0.5, 0.25]
    );
    assert_eq!(
        read_audio(&mut control, &mut runtime, new_export),
        [1.0, 2.0]
    );
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::NodePairRecycle { id: node_pairs }
        )
        .0,
        Reply::NodePairStatus {
            id: node_pairs,
            status: NodePairStatus::Idle
        }
    );
    assert_eq!(
        operation(
            &mut control,
            &mut runtime,
            Operation::TopologyRecycle { id: topology }
        )
        .0,
        Reply::TopologyStatus {
            id: topology,
            status: Status::Idle
        }
    );
    let ticket = control
        .release_graph(graph)
        .unwrap_or_else(|e| panic!("release: {}", e.error));
    step(&mut runtime, &[0.0]);
    let completion = control.runtime.try_completion().unwrap();
    assert_eq!(completion.sequence, ticket.sequence());
    assert_eq!(completion.outcome, TransportOutcome::Applied);
    assert!(
        matches!(&completion.retired, Some(Retired::Plan(plan)) if plan.descriptor().connections.contains(&route(Endpoint::host(PortType::Audio, 0), Endpoint::node(node(1), PortType::Audio, 0))))
    );
    drop((completion, ticket));
    for export in [old_export, new_initial_export, new_export] {
        assert_eq!(
            operation(&mut control, &mut runtime, Operation::Release { export }).0,
            Reply::Done
        );
    }
    operation(
        &mut control,
        &mut runtime,
        transition(owner(2), Transition::Stop),
    );
    drop(control);
    let (mut planner, mut resources) = runtime.into_planner_and_resources();
    planner.cleanup(&mut resources).unwrap();
    planner.cleanup(&mut resources).unwrap();
    resources.begin_callback();
    resources.reclaim();
    assert_eq!(resources.usage().free_pages, 64);
    assert_eq!(resources.usage().free_versions, 16);
    assert_eq!(resources.usage().free_snapshots, 16);
    assert_eq!(resources.usage().reserved_pages, 0);
    assert_eq!(resources.capture_group_usage().free_groups, 2);
    assert_eq!(planner.media_usage().references, 0);
}
fn main() {
    run(&[3]);
    run(&[1, 1, 1]);
    run(&[2, 1]);
    println!("configuration cut: refused running/same-frame Stop, partitioned generation/route/media publication, old/new exports and independent retirement verified");
}
