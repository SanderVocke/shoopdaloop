//! The loop planner, its playback/capture nodes and a control-side client, for
//! installation into an application-owned runtime.
//!
//! One resolver settles musical state; separate physical playback/capture nodes execute borrowed
//! decisions. Prepared timing/source/node pair banks and graph slots have distinct
//! ownership, cut and recycle contracts. Publication is not general rollback.
use crate::{
    audio, media, midi, Decision, LoopId, LoopKind, Mode, PreparedTopology, RequestId, Resolver,
    State, Timing, Transition,
};
use rt_runtime::{
    executor::Command,
    graph::{Catalog, GraphDescription, PreparedPlan},
    midi_state::MidiState,
    planner::*,
    processor::{Processor, ProcessorOperation},
    resource::*,
    schedule::{self, ReceiptToken},
    Epoch, Error, Frame, NodeId, PlannerId, PreparedConfig, Revision, Sequence,
};
mod configuration;
mod node_pairs;
mod planner;
mod processor;
mod topology;
use crate::topology::staging::{Progress, StageId, Status};
pub use node_pairs::{
    NodePairInitial, NodePairSettings, NodePairSourceId, NodePairSourceStatus, NodePairStageId,
    NodePairStatus,
};
pub use processor::{CaptureNode, LoopNode, LoopNodes, PlaybackNode};
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
/// Typed musical/control work. Admission, runtime delivery and domain outcome are
/// distinct. Recovery permits Cancel, TopologyCancel, NodePairCancel, NodePairSourceRelease
/// and nonfuture Timing::At Stop; activation/queries/exports use the normal lane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Operation {
    NodePairSnapshot {
        resource: rt_runtime::ResourceId,
    },
    NodePairSourceValidate {
        id: NodePairSourceId,
    },
    NodePairSourceStatus {
        id: NodePairSourceId,
    },
    NodePairSourceRelease {
        id: NodePairSourceId,
    },
    NodePairBegin {
        slot: usize,
        changes: usize,
    },
    NodePairAppend {
        id: NodePairStageId,
        previous: Option<LoopId>,
        initial: Option<NodePairInitial>,
    },
    NodePairStatus {
        id: NodePairStageId,
    },
    NodePairCancel {
        id: NodePairStageId,
    },
    NodePairRecycle {
        id: NodePairStageId,
    },
    /// Publish stopped logical-owner edits, initial media, timing and routes.
    /// Queuing Stop for the same frame does not satisfy the already-stopped check.
    ConfigurationActivate {
        node_pairs: NodePairStageId,
        topology: StageId,
        graph: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    GraphStatus {
        id: GraphStageId,
    },
    /// Immediate prepared routing change with fixed logical/physical node pairs.
    GraphActivate {
        id: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
    /// Immediate timing/routing pair with fixed owners; changed programs are stopped.
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
    /// Own one program transaction until stopped/real zero, then publish its routes.
    /// The referenced Ready graph is not locked and must be revalidated at the cut.
    ProgramGraphActivate {
        id: StageId,
        target: LoopId,
        graph: GraphStageId,
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
/// Settled pre-render state at one frame, including actual publication epochs.
/// Media identities here are borrowed; use a successful export for independent ownership.
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
    NodePairSource {
        id: NodePairSourceId,
        snapshot: Snapshot,
        status: NodePairSourceStatus,
    },
    NodePairSourceStatus {
        id: NodePairSourceId,
        status: NodePairSourceStatus,
    },
    NodePairSourceReleased {
        id: NodePairSourceId,
    },
    NodePairStatus {
        id: NodePairStageId,
        status: NodePairStatus,
    },
    ConfigurationActivated {
        node_pairs: NodePairStageId,
        topology: StageId,
        graph: GraphStageId,
        graph_epoch: Epoch,
        topology_epoch: Epoch,
    },
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
    ProgramGraphChanged {
        id: StageId,
        graph: GraphStageId,
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
    pub playback_node: NodeId,
    pub capture_node: NodeId,
    pub reset_input: bool,
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
    assigned: bool,
    reset_input: bool,
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
        graph: Option<GraphStageId>,
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
    node_pairs: node_pairs::NodePairs,
    graph_epoch: Epoch,
    ledger: media::Ledger,
    rows: Box<[Option<Row>]>,
    inputs: Box<[MidiState]>,
    views: Box<[View]>,
    pending: Box<[Option<Pending>]>,
    failures: Vec<crate::Observation>,
}

/// Preparation choices beyond the loops themselves; the default enables none.
#[derive(Default)]
pub struct Options {
    /// Timing candidates for staged topology and program changes.
    pub staging: TopologySettings,
    /// Edit banks and sources for publishing new logical owners on prepared node pairs.
    pub node_pairs: NodePairSettings,
    pub observer: Option<Box<dyn BoundaryObserver>>,
}

/// Prepared parts for installation into an application-owned runtime.
pub struct Prepared {
    /// The runtime's planner.
    pub planner: LoopPlanner,
    /// Processors to install under their identities.
    pub nodes: LoopNodes,
    pub client: LoopClient,
}

/// Control-side loop vocabulary. It owns no transport: submit its commands through
/// the application's runtime control endpoint.
pub struct LoopClient {
    planner: PlannerId,
    media: Box<[PreparedMedia]>,
    bindings: Box<[DecisionBinding]>,
}
impl LoopClient {
    pub fn planner(&self) -> PlannerId {
        self.planner
    }
    /// Startup checked identities, not a live settled-state query or owning leases.
    pub fn media(&self) -> &[PreparedMedia] {
        &self.media
    }
    /// Decision bindings of every prepared loop node, including unassigned templates.
    pub fn bindings(&self) -> &[DecisionBinding] {
        &self.bindings
    }
    /// Decision bindings for the loop nodes present in `graph`.
    pub fn bindings_for(&self, graph: &GraphDescription) -> Vec<DecisionBinding> {
        self.bindings
            .iter()
            .filter(|binding| graph.nodes.contains(&binding.consumer))
            .copied()
            .collect()
    }
    /// Allocating control-side compilation of `graph` with the loop's bindings.
    pub fn compile_graph(
        &self,
        catalog: &Catalog,
        graph: &GraphDescription,
    ) -> Result<PreparedPlan, Error> {
        rt_runtime::graph::compile_planned(catalog, graph, &self.bindings_for(graph))
    }
    /// Typed loop work at `at`. Admission, delivery and the domain outcome are
    /// reported separately by the runtime.
    pub fn command<P, Q: ProcessorOperation>(
        &self,
        at: Frame,
        operation: Operation,
    ) -> Command<P, LoopPlanner, Q> {
        self.command_with_payload(at, operation, Box::new([]))
    }
    /// Build payloads off-render. Only topology header/chunk operations accept them.
    pub fn command_with_payload<P, Q: ProcessorOperation>(
        &self,
        at: Frame,
        operation: Operation,
        payload: Box<[u8]>,
    ) -> Command<P, LoopPlanner, Q> {
        Command::Schedule {
            at,
            action: schedule::Action::Planner {
                planner: self.planner,
                operation,
            },
            payload,
        }
    }
}

impl LoopPlanner {
    /// Allocating preparation of the planner, its playback/capture nodes and the
    /// control-side client. Capture resources and imported media are created in
    /// `resources`, which the application then gives to its runtime together with
    /// the planner and nodes. Every primitive requires a binding; unassigned
    /// templates need node-pair edit banks and stay outside the initial graph.
    pub fn prepare(
        config: PreparedConfig,
        topology: PreparedTopology,
        resources: &mut ResourceStore,
        settings: Settings,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
        options: Options,
    ) -> Result<Prepared, Failure> {
        let (topologies, topology) = topology::Topologies::prepare(topology, options.staging)?;
        let observer = options.observer.unwrap_or_else(|| Box::new(NoObserver));
        let limits = topology.limits();
        let cfg = config.limits();
        if settings.normal_requests == 0 || settings.normal_requests >= limits.pending_requests {
            return Err(Error::InvalidConfig.into());
        }
        let slots = limits.loop_slots;
        let node_pairs = node_pairs::NodePairs::prepare(slots, options.node_pairs)?;
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
            let spec = resolver.topology().node(binding.target).ok();
            let assigned = spec.is_some();
            let slot = binding.target.slot() as usize;
            if slot >= slots || (!assigned && options.node_pairs.candidates == 0) {
                return Err(Error::InvalidId.into());
            }
            if spec.is_some_and(|spec| spec.kind != LoopKind::Primitive)
                || rows[slot].is_some()
                || !binding.gain.is_finite()
                || binding.channels > u32::MAX as usize
            {
                return Err(Error::InvalidShape.into());
            }
            if binding.playback == binding.capture {
                return Err(Error::InvalidId.into());
            }
            if binding.midi && resources.midi_index_usage().versions == 0 {
                return Err(Error::InvalidConfig.into());
            }
            if binding.midi && binding.packet_bytes < binding.midi_bytes.max(3) {
                return Err(Error::Capacity.into());
            }
            if assigned {
                ledger.bind(binding.target)?;
            }
            let mut row = Row {
                binding,
                assigned,
                reset_input: false,
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
            nodes.push((
                binding.playback,
                LoopNode::Playback(PlaybackNode::prepare(binding, cfg.max_frames)?),
            ));
            nodes.push((
                binding.capture,
                LoopNode::Capture(CaptureNode::prepare(binding, cfg.max_frames)?),
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
            if spec.kind == LoopKind::Primitive
                && rows[spec.id.slot() as usize]
                    .is_none_or(|row| !row.assigned || row.binding.target != spec.id)
            {
                return Err(Error::InvalidId.into());
            }
        }
        let mut catalog = Vec::new();
        for input in imports {
            let description = input.description;
            let row = rows
                .get(description.owner.slot() as usize)
                .and_then(|r| *r)
                .filter(|r| r.assigned && r.binding.target == description.owner)
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
        for row in rows.iter().flatten().filter(|r| r.assigned) {
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
            playback_node: NodeId::from_parts(0, 1)?,
            capture_node: NodeId::from_parts(0, 1)?,
            reset_input: false,
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
            node_pairs,
            graph_epoch: Epoch(1),
            ledger,
            rows,
            inputs,
            views,
            pending: vec![None; settings.normal_requests + 1].into_boxed_slice(),
            failures,
        };
        Ok(Prepared {
            planner,
            nodes: LoopNodes(nodes),
            client: LoopClient {
                planner: settings.planner,
                media: catalog.into_boxed_slice(),
                bindings: decision_bindings.into_boxed_slice(),
            },
        })
    }
    pub fn observer(&self) -> &dyn BoundaryObserver {
        &*self.observer
    }
    pub fn media_usage(&self) -> media::Usage {
        self.ledger.usage()
    }
}
