//! Prepared loop planner/consumers and a restricted runtime/control facade.
use crate::{
    audio, media, midi, Decision, LoopId, LoopKind, Mode, PreparedTopology, RequestId, Resolver,
    State, Timing, Transition,
};
use rt_runtime::{
    buffer::{AudioBlock, MidiBlock},
    executor::{self, Command, GraphRuntime, PreparedNode},
    graph::{Catalog, GraphDescription, PreparedPlan},
    midi_state::MidiState,
    planner::*,
    processor::Processor,
    resource::*,
    schedule::{self, ReceiptToken},
    transport::{self, Lane},
    Epoch, Error, Frame, NodeId, PlannerId, PreparedConfig, Revision, Sequence,
};
mod planner;
mod processor;
mod topology;
use crate::topology::staging::{Progress, StageId, Status};
pub use processor::Node;
pub use rt_runtime::graph::activation::{StageId as GraphStageId, StageStatus as GraphStageStatus};
pub use topology::TopologySettings;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Failure {
    Timeline(crate::Error),
    Runtime(Error),
}
impl From<crate::Error> for Failure {
    fn from(error: crate::Error) -> Self {
        Self::Timeline(error)
    }
}
impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Self::Runtime(error)
    }
}
fn runtime_error(error: crate::Error) -> Error {
    match error {
        crate::Error::Capacity => Error::Capacity,
        crate::Error::Exhausted => Error::Exhausted,
        crate::Error::InvalidFrame => Error::InvalidFrame,
        crate::Error::InvalidId => Error::InvalidId,
        _ => Error::InvalidState,
    }
}
fn timeline_error(error: Error) -> crate::Error {
    match error {
        Error::Capacity | Error::Budget | Error::Full => crate::Error::Capacity,
        Error::Exhausted => crate::Error::Exhausted,
        _ => crate::Error::InvalidState,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub planner: PlannerId,
    pub normal_requests: usize,
    pub media_slots: usize,
    pub export_slots: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub target: LoopId,
    pub playback: NodeId,
    pub capture: NodeId,
    pub channels: usize,
    pub midi: bool,
    pub gain: f32,
    pub smoothing: audio::Smoothing,
    pub midi_events: usize,
    pub midi_bytes: usize,
    pub packet_bytes: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedMedia {
    pub target: LoopId,
    pub revision: Revision,
    pub media: media::MediaId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Selection {
    Playback,
    Latest,
    Recording,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    GraphStatus {
        id: GraphStageId,
    },
    GraphActivate {
        id: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    TopologyGraphActivate {
        id: StageId,
        graph: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    TopologyBegin {
        slot: usize,
    },
    TopologyAppend {
        id: StageId,
        first_record: usize,
    },
    TopologyValidate {
        id: StageId,
    },
    TopologyStatus {
        id: StageId,
    },
    TopologyCancel {
        id: StageId,
    },
    TopologyRecycle {
        id: StageId,
    },
    TopologyActivate {
        id: StageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    ProgramActivate {
        id: StageId,
        target: LoopId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    Transition {
        target: LoopId,
        timing: Timing,
        transition: Transition,
    },
    Select {
        target: LoopId,
        timing: Timing,
        media: media::MediaId,
        preserve_position: bool,
    },
    Gain {
        target: LoopId,
        value: f32,
    },
    State {
        target: LoopId,
    },
    Export {
        target: LoopId,
        selection: Selection,
    },
    Release {
        export: media::ExportId,
    },
    Retire {
        media: media::MediaId,
    },
    Cursor {
        export: media::ExportId,
        midi: bool,
    },
    MidiInitial {
        export: media::ExportId,
        channel: u8,
    },
    Cancel {
        sequence: Sequence,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub frame: Frame,
    pub graph_epoch: Epoch,
    pub topology_epoch: Epoch,
    pub state: State,
    pub playback: Option<media::MediaId>,
    pub latest: Option<media::MediaId>,
    pub gain: f32,
    pub capture_failure: Option<Error>,
    pub midi_overflows: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reply {
    GraphStatus {
        id: GraphStageId,
        status: GraphStageStatus,
    },
    GraphActivated {
        id: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    TopologyGraphActivated {
        id: StageId,
        graph: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    TopologyStatus {
        id: StageId,
        status: Status,
    },
    TopologyProgress {
        id: StageId,
        progress: Progress,
    },
    TopologyActivated {
        id: StageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    ProgramChanged {
        id: StageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
        completion: crate::Completion,
    },
    Done,
    Failed(Failure),
    Transition(crate::Completion),
    State(Cut),
    Export {
        id: media::ExportId,
        cut: Cut,
        media: media::Export,
    },
    Cursor(Cursor),
    MidiInitial {
        channel: u8,
        state: rt_runtime::midi_state::ChannelState,
        system_reset_seen: bool,
    },
    Cancelled {
        sequence: Sequence,
        delivered: bool,
    },
}
#[derive(Clone, Copy)]
pub struct View {
    pub decision: Decision,
    pub audio: Option<ReadIndex>,
    pub midi: Option<ReadIndex>,
    pub capture: Option<CaptureIntervalId>,
    pub gain: f32,
}
#[derive(Clone, Copy)]
pub enum Observed {
    Playback { target: LoopId, overflow: bool },
    Input { target: LoopId, state: MidiState },
    NoMidiInput { target: LoopId },
}
impl Default for Observed {
    fn default() -> Self {
        Self::Input {
            target: LoopId::from_parts(0, 1).expect("fixed identity"),
            state: MidiState::default(),
        }
    }
}
#[derive(Clone, Copy)]
struct Take {
    revision: Revision,
    media: media::MediaId,
    bank: usize,
    token: Option<CaptureTakeId>,
}
#[derive(Clone, Copy)]
struct Row {
    binding: Binding,
    groups: [Option<CaptureGroupId>; 2],
    resources: [[Option<rt_runtime::ResourceId>; 2]; 2],
    active: Option<Take>,
    prepared: Option<Take>,
    interval: Option<CaptureIntervalId>,
    next_bank: usize,
    frozen: [Option<Snapshot>; 2],
    failure: Option<Error>,
    overflows: u64,
    seen_playback: bool,
    seen_capture: bool,
}
#[derive(Clone, Copy)]
enum PendingKind {
    Program {
        id: StageId,
        request: RequestId,
        graph_epoch: Epoch,
    },
    Transition {
        request: RequestId,
        pin: Option<media::PinId>,
    },
    Query(Operation),
}
#[derive(Clone, Copy)]
struct Pending {
    token: ReceiptToken,
    kind: PendingKind,
    forwarded: bool,
}

/// Optional bounded diagnostic observer. Called once after each settled boundary,
/// before rendering. Implementations obey the same no-heap/nonblocking contract
/// as processors; journal loss must not affect musical execution.
pub trait BoundaryObserver: Send {
    fn boundary(
        &mut self,
        frame: Frame,
        decisions: &[Decision],
        events: &[crate::Event],
        dropped: u64,
    );
    fn as_any(&self) -> &dyn std::any::Any;
}
struct NoObserver;
impl BoundaryObserver for NoObserver {
    fn boundary(
        &mut self,
        _frame: Frame,
        _decisions: &[Decision],
        _events: &[crate::Event],
        _dropped: u64,
    ) {
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
pub struct LoopPlanner {
    observer: Box<dyn BoundaryObserver>,
    settings: Settings,
    resolver: Resolver,
    topologies: topology::Topologies,
    graph_epoch: Epoch,
    ledger: media::Ledger,
    rows: Box<[Option<Row>]>,
    inputs: Box<[MidiState]>,
    views: Box<[View]>,
    pending: Box<[Option<Pending>]>,
    failures: Vec<crate::Observation>,
}
pub struct Engine<P: Processor<LoopPlanner>> {
    runtime: GraphRuntime<Node<P>, LoopPlanner>,
}
pub struct Controller<P: Processor<LoopPlanner>> {
    control: executor::Control<Node<P>, LoopPlanner>,
    planner: PlannerId,
    media: Box<[PreparedMedia]>,
    graph_catalog: Catalog,
    graph_bindings: Box<[DecisionBinding]>,
}
pub struct Stopped {
    planner: LoopPlanner,
    resources: ResourceStore,
}
impl<P: Processor<LoopPlanner>> Controller<P> {
    pub fn media(&self) -> &[PreparedMedia] {
        &self.media
    }
    /// Allocating control-side compilation for the prepared physical roles.
    pub fn compile_graph(&self, graph: &GraphDescription) -> Result<PreparedPlan, Error> {
        rt_runtime::graph::compile_planned(&self.graph_catalog, graph, &self.graph_bindings)
    }
    pub fn stage_graph(
        &mut self,
        slot: usize,
        expected_epoch: Epoch,
        plan: PreparedPlan,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::stage_graph(slot, expected_epoch, plan),
        )
    }
    pub fn release_graph(
        &mut self,
        id: GraphStageId,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.control
            .submit_prepared(Lane::Normal, Command::ReleaseGraph { id })
    }
    pub fn submit(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.submit_payload(lane, at, operation, Box::new([]))
    }
    pub fn submit_payload(
        &mut self,
        lane: Lane,
        at: Frame,
        operation: Operation,
        payload: Box<[u8]>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.control.submit_prepared(
            lane,
            Command::Schedule {
                at,
                action: schedule::Action::Planner {
                    planner: self.planner,
                    operation,
                },
                payload,
            },
        )
    }
    pub fn read(
        &mut self,
        cursor: Cursor,
        bytes: Box<[u8]>,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::resource(rt_runtime::resource::Request::Read { cursor, bytes }),
        )
    }
    pub fn parameter(
        &mut self,
        at: Frame,
        node: NodeId,
        parameter: u32,
        value: f32,
    ) -> Result<transport::Ticket, transport::SubmitError<Command<Node<P>, LoopPlanner>>> {
        self.control.submit_prepared(
            Lane::Normal,
            Command::Schedule {
                at,
                action: schedule::Action::Parameter {
                    node,
                    parameter,
                    value,
                },
                payload: Box::new([]),
            },
        )
    }
    pub fn try_completion(
        &mut self,
    ) -> Option<transport::Completion<Command<Node<P>, LoopPlanner>, executor::Retired<Node<P>>>>
    {
        self.control.try_completion()
    }
    pub fn try_execution(&mut self) -> Option<schedule::Execution<Operation, Reply>> {
        self.control.try_execution()
    }
    pub fn connected(&self) -> bool {
        self.control.connected()
    }
}
impl<P: Processor<LoopPlanner>> Engine<P> {
    /// Allocating preparation. Every primitive requires both role consumers;
    /// effects must declare no planning access. Graph staging is separately opt-in.
    pub fn prepare(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
    ) -> Result<(Controller<P>, Self), Failure> {
        Self::prepare_with_observer(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            Box::new(NoObserver),
        )
    }
    pub fn prepare_with_observer(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        observer: Box<dyn BoundaryObserver>,
    ) -> Result<(Controller<P>, Self), Failure> {
        Self::prepare_with_staging(
            config,
            topology,
            resources,
            settings,
            bindings,
            imports,
            effects,
            graph,
            TopologySettings::default(),
            Some(observer),
        )
    }
    /// Prepare reusable timing candidates for the fixed graph and existing role identities.
    /// Immediate changes require stopped programs; owning program changes use musical cuts.
    /// Logical-role replacement is not exposed by this preparation path.
    pub fn prepare_with_staging(
        config: PreparedConfig,
        topology: PreparedTopology,
        mut resources: ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        effects: Vec<(NodeId, P)>,
        graph: GraphDescription,
        staging: TopologySettings,
        observer: Option<Box<dyn BoundaryObserver>>,
    ) -> Result<(Controller<P>, Self), Failure> {
        let (topologies, topology) = topology::Topologies::prepare(topology, staging)?;
        let observer = observer.unwrap_or_else(|| Box::new(NoObserver));
        let limits = topology.limits();
        let cfg = config.limits();
        if settings.normal_requests == 0 || settings.normal_requests >= limits.pending_requests {
            return Err(Error::InvalidConfig.into());
        }
        let slots = limits.loop_slots;
        let bytes = slots
            .checked_mul(
                size_of::<Row>()
                    + size_of::<MidiState>()
                    + size_of::<View>()
                    + 2 * size_of::<rt_runtime::planner::Observation<Observed>>(),
            )
            .and_then(|n| {
                settings
                    .normal_requests
                    .checked_add(1)
                    .and_then(|c| c.checked_mul(size_of::<Option<Pending>>()))
                    .and_then(|p| n.checked_add(p))
            })
            .ok_or(Error::Capacity)?;
        if bytes > i32::MAX as usize {
            return Err(Error::Capacity.into());
        }
        let mut resolver = Resolver::prepare(topology)?;
        let mut ledger = media::Ledger::prepare(
            slots,
            settings.media_slots,
            settings.normal_requests,
            settings.export_slots,
        )?;
        let mut rows = vec![None; slots].into_boxed_slice();
        let mut nodes = Vec::new();
        let mut decision_bindings = Vec::new();
        for binding in bindings {
            let spec = resolver.topology().node(binding.target)?;
            let slot = binding.target.slot() as usize;
            if spec.kind != LoopKind::Primitive
                || rows[slot].is_some()
                || !binding.gain.is_finite()
                || binding.channels > u32::MAX as usize
            {
                return Err(Error::InvalidShape.into());
            }
            if !graph.nodes.contains(&binding.playback)
                || !graph.nodes.contains(&binding.capture)
                || binding.playback == binding.capture
            {
                return Err(Error::InvalidId.into());
            }
            if binding.midi && resources.midi_index_usage().versions == 0 {
                return Err(Error::InvalidConfig.into());
            }
            if binding.midi && binding.packet_bytes < binding.midi_bytes.max(3) {
                return Err(Error::Capacity.into());
            }
            ledger.bind(binding.target)?;
            let mut row = Row {
                binding,
                groups: [None; 2],
                resources: [[None; 2]; 2],
                active: None,
                prepared: None,
                interval: None,
                next_bank: 0,
                frozen: [None; 2],
                failure: None,
                overflows: 0,
                seen_playback: false,
                seen_capture: false,
            };
            for bank in 0..2 {
                let mut members = Vec::new();
                if binding.channels > 0 {
                    let resource = resources.create()?;
                    row.resources[bank][0] = Some(resource);
                    members.push(CaptureMember {
                        resource,
                        format: Format::Audio {
                            channels: binding.channels as u32,
                        },
                        midi_events: 0,
                        midi_bytes: 0,
                    });
                }
                if binding.midi {
                    let resource = resources.create()?;
                    row.resources[bank][1] = Some(resource);
                    members.push(CaptureMember {
                        resource,
                        format: Format::Midi,
                        midi_events: binding
                            .midi_events
                            .max(rt_runtime::midi::ResetState::EVENTS),
                        midi_bytes: binding.midi_bytes.max(rt_runtime::midi::ResetState::BYTES),
                    });
                }
                if !members.is_empty() {
                    row.groups[bank] = Some(resources.bind_capture_group(&members)?);
                }
            }
            nodes.push(PreparedNode::new(
                binding.playback,
                Node::playback(binding, cfg.max_frames)?,
            ));
            nodes.push(PreparedNode::new(
                binding.capture,
                Node::capture(binding, cfg.max_frames)?,
            ));
            for consumer in [binding.playback, binding.capture] {
                decision_bindings.push(DecisionBinding {
                    planner: settings.planner,
                    consumer,
                    decision_slot: slot,
                });
            }
            rows[slot] = Some(row);
        }
        for spec in resolver.topology().nodes() {
            if spec.kind == LoopKind::Primitive && rows[spec.id.slot() as usize].is_none() {
                return Err(Error::InvalidId.into());
            }
        }
        let mut catalog = Vec::new();
        for input in imports {
            let description = input.description;
            let row = rows
                .get(description.owner.slot() as usize)
                .and_then(|r| *r)
                .filter(|r| r.binding.target == description.owner)
                .ok_or(Error::InvalidId)?;
            if description.channels as usize != row.binding.channels
                || description.midi != row.binding.midi
                || catalog.iter().any(|m: &PreparedMedia| {
                    m.target == description.owner && m.revision == description.revision
                })
            {
                return Err(Error::InvalidShape.into());
            }
            if let Some(source) = input.midi {
                let mut cursor = resources.render_io().midi_seek(source.index, 0)?.cursor;
                while let Some(event) = resources.render_io().midi_next(&mut cursor)? {
                    if event.byte_len() > row.binding.packet_bytes {
                        return Err(Error::Capacity.into());
                    }
                }
            }
            let id = ledger
                .adopt(&mut resources.io(), input)
                .map_err(|e| e.error)?;
            resolver.reserve_media_revision(description.owner, description.revision)?;
            if description.revision == Revision(0) {
                if description.frames != resolver.state(description.owner)?.content_length {
                    return Err(Error::InvalidShape.into());
                }
                ledger.set_playback(&mut resources.io(), description.owner, Some(id))?;
            }
            catalog.push(PreparedMedia {
                target: description.owner,
                revision: description.revision,
                media: id,
            });
        }
        for row in rows.iter().flatten() {
            let target = row.binding.target;
            if ledger.playback(target)?.is_none() {
                let length = resolver.state(target)?.content_length;
                if length > 0 && (row.binding.channels > 0 || row.binding.midi) {
                    return Err(Error::InvalidShape.into());
                }
                let id = ledger
                    .adopt(
                        &mut resources.io(),
                        media::Input {
                            description: media::Description {
                                owner: target,
                                revision: Revision(0),
                                frames: length,
                                channels: row.binding.channels as u32,
                                midi: row.binding.midi,
                            },
                            audio: None,
                            midi: None,
                        },
                    )
                    .map_err(|e| e.error)?;
                ledger.set_playback(&mut resources.io(), target, Some(id))?;
                catalog.push(PreparedMedia {
                    target,
                    revision: Revision(0),
                    media: id,
                });
            }
        }
        for (id, effect) in effects {
            if effect.planning() != ConsumerAccess::default() || !graph.nodes.contains(&id) {
                return Err(Error::InvalidShape.into());
            }
            nodes.push(PreparedNode::new(id, Node::Effect(effect)));
        }
        if graph.nodes.len() != nodes.len() {
            return Err(Error::InvalidShape.into());
        }
        catalog.sort_by_key(|m| (m.target, m.revision));
        let dummy = LoopId::from_parts(0, 1)?;
        let initial = View {
            decision: Decision {
                state: State::new(dummy, 0),
                playback_discontinuity: false,
                origin: None,
            },
            audio: None,
            midi: None,
            capture: None,
            gain: 1.,
        };
        let mut views = vec![initial; slots].into_boxed_slice();
        let mut inputs = vec![MidiState::default(); slots].into_boxed_slice();
        for value in &mut views {
            *std::hint::black_box(value) = initial;
        }
        for value in &mut inputs {
            *std::hint::black_box(value) = MidiState::default();
        }
        let failure_capacity = slots
            .checked_mul(2)
            .and_then(|n| n.checked_add(limits.pending_requests))
            .ok_or(Error::Capacity)?;
        let mut failures =
            vec![crate::Observation::CaptureExhausted { target: dummy }; failure_capacity];
        failures.clear();
        let planner = LoopPlanner {
            observer,
            settings,
            resolver,
            topologies,
            graph_epoch: Epoch(1),
            ledger,
            rows,
            inputs,
            views,
            pending: vec![None; settings.normal_requests + 1].into_boxed_slice(),
            failures,
        };
        let (mut control, runtime) = GraphRuntime::prepare_with_initial_graph(
            config,
            nodes,
            planner,
            resources,
            &graph,
            &decision_bindings,
        )?;
        control.prepare_submissions()?;
        let graph_nodes = graph
            .nodes
            .iter()
            .map(|id| runtime.catalog().node(*id))
            .collect::<Result<Vec<_>, _>>()?;
        let graph_consumers = graph
            .nodes
            .iter()
            .map(|id| {
                Ok(ConsumerSpec {
                    node: *id,
                    access: runtime.catalog().consumer(*id)?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        let graph_catalog = Catalog::with_planner(
            config,
            &graph_nodes,
            runtime.catalog().planner().expect("loop planner"),
            &graph_consumers,
        )?;
        Ok((
            Controller {
                control,
                planner: settings.planner,
                media: catalog.into_boxed_slice(),
                graph_catalog,
                graph_bindings: decision_bindings.into_boxed_slice(),
            },
            Self { runtime },
        ))
    }
    /// Allocating preparation before the first nonempty callback; physical roles stay fixed.
    pub fn prepare_graph_staging(&mut self, candidates: usize) -> Result<(), Error> {
        self.runtime.prepare_graph_staging(candidates)
    }
    pub fn process(&mut self, audio: AudioBlock<'_, '_>, midi: MidiBlock<'_>) -> Result<(), Error> {
        self.runtime.process(audio, midi)
    }
    pub fn next_frame(&self) -> Frame {
        self.runtime.next_frame()
    }
    pub fn resource_usage(&self) -> PoolUsage {
        self.runtime.resource_usage()
    }
    pub fn diagnostics(&self) -> executor::Diagnostics {
        self.runtime.diagnostics()
    }
    pub fn drain_midi_recovery(
        &mut self,
        port: usize,
        sink: &mut rt_runtime::buffer::MidiBuffer,
        frames: usize,
    ) -> Result<usize, Error> {
        self.runtime.drain_midi_recovery(port, sink, frames)
    }
    pub fn into_stopped(self) -> Stopped {
        let (planner, resources) = self.runtime.into_planner_and_resources();
        Stopped { planner, resources }
    }
}
impl Stopped {
    pub fn observer(&self) -> &dyn BoundaryObserver {
        &*self.planner.observer
    }
    pub fn resource_usage(&self) -> PoolUsage {
        self.resources.usage()
    }
    pub fn media_usage(&self) -> media::Usage {
        self.planner.ledger.usage()
    }
    pub fn resources(&mut self) -> &mut ResourceStore {
        &mut self.resources
    }
    pub fn cleanup(&mut self) -> Result<(), Error> {
        self.planner.cleanup(&mut self.resources)
    }
}
