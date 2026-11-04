#![forbid(unsafe_code)]

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

mod commands;
mod composites;
mod management;
mod media;
#[path = "persistence/mod.rs"]
mod persistence_jobs;
mod preview;
mod projection;
mod publication;

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::sync::Arc;
use anyhow::{anyhow, Result};
use core::time::Duration;
use shoop_app_api::*;
use shoop_app_protocol::*;
use shoop_backend::*;

pub const MAX_TRACKS: usize = 64;
pub const MAX_LOOPS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// A fresh control-connection nonce supplied by the composition root.
pub struct ConnectionId(pub u128);

pub struct ServiceOptions {
    pub master_auto_connect: bool,
    pub loop_smoothing_ms: u32,
}

struct Lease {
    connection: ConnectionId,
    authority: Authority,
    ready: bool,
    snapshot_revision: Revision,
}

struct Track {
    backend: BackendTrackId,
    audio_channels: u32,
    midi: bool,
    monitoring_intent: Option<(u64, bool)>,
    view: TrackState,
    loops: Vec<LoopId>,
}
struct Bus {
    backend: BackendBusId,
    view: BusState,
}
struct Loop {
    backend: BackendLoopId,
    track: TrackId,
    view: LoopState,
}
struct Intent {
    connected: bool,
    pending: Option<u64>,
    error: Option<String>,
}

struct Port {
    backend: BackendPortId,
    view: ApplicationPortState,
}

#[derive(Clone, Debug)]
enum Effect {
    None,
    BusCreated(BusId),
    BusRemoved(BusId),
    BusControl(BusId),
    BusControls(Vec<BusId>),
    TrackCreated(TrackId, Vec<LoopId>),
    LoopCreated(TrackId, LoopId),
    TrackRemoved(TrackId),
    Shutdown,
    Persistence(persistence::Key),
    HostGraph(shoop_port_graph::OperationId),
    Connection(PortId, String),
    SessionPublished,
}

struct Pending {
    id: OperationId,
    accepted_at: Revision,
    first: u64,
    last: u64,
    completed: BTreeSet<u64>,
    error: Option<CommandError>,
    effect: Effect,
    publications: Vec<HostGraphLocalKey>,
    retiring: bool,
    rollback: bool,
}

#[derive(Clone, Debug)]
struct CommandError {
    reason: Refusal,
    message: String,
}
impl From<anyhow::Error> for CommandError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            reason: Refusal::Engine,
            message: error.to_string(),
        }
    }
}
impl CommandError {
    fn graph(error: shoop_port_graph::Error) -> Self {
        use shoop_port_graph::ErrorKind::*;
        let reason = match error.kind {
            Unsupported => Refusal::Unsupported,
            Unavailable => Refusal::NotReady,
            WrongEpoch | StalePort => Refusal::RevisionConflict,
            Invalid | Incompatible => Refusal::Invalid,
            Capacity | Busy => Refusal::Overloaded,
            Backend => Refusal::Engine,
        };
        Self {
            reason,
            message: error.message,
        }
    }
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            reason: Refusal::Invalid,
            message: message.into(),
        }
    }
    fn unsupported(capability: BackendCapability) -> Self {
        Self {
            reason: Refusal::Unsupported,
            message: capability.unavailable_reason().to_owned(),
        }
    }
}

/// Owns shared state and the same backend for the complete session lifetime.
pub struct Service {
    host_graph_controller: Option<Box<dyn crate::HostGraphController>>,
    host_graph: Arc<HostGraphState>,
    publications: BTreeMap<HostGraphLocalKey, publication::Publication>,
    graph_connections: BTreeMap<shoop_port_graph::OperationId, (PortId, String)>,
    master_graph_auto_done: bool,
    backend: Box<dyn Backend>,
    preview: ClickTrackState,
    preview_bus: Option<BusId>,
    preview_stage: Option<preview::Stage>,
    options: ServiceOptions,
    host_settings: Option<HostSettings>,
    host_diagnostics: Vec<String>,
    revision: Revision,
    generation: u64,
    lease: Option<Lease>,
    last_authority: Option<Authority>,
    engine_closed: bool,
    actors: BTreeMap<ClientId, u64>,
    updates: VecDeque<ServiceMessage>,
    pending: BTreeMap<OperationId, Pending>,
    results: VecDeque<OperationResult>,
    tracks: BTreeMap<TrackId, Track>,
    track_order: Vec<TrackId>,
    loops: BTreeMap<LoopId, Loop>,
    composites: BTreeMap<LoopId, composites::Owner>,
    next_composite_event: u64,
    composite_structure: Option<composites::structure::Job>,
    ports: BTreeMap<PortId, Port>,
    channels: BTreeMap<BusChannelId, BackendBusChannelId>,
    buses: BTreeMap<BusId, Bus>,
    bus_order: Vec<BusId>,
    next_bus: u64,
    next_channel: u64,
    next_track: u64,
    next_loop: u64,
    next_port: u64,
    global: GlobalControlState,
    connection_intent: BTreeMap<(PortId, String), Intent>,
    route_intent: BTreeMap<MixerRouteState, Intent>,
    piano_notes: BTreeMap<u8, BTreeSet<TrackId>>,
    released_notes: VecDeque<(TrackId, u8)>,
    observed: BackendSnapshot,
    init_through: u64,
    init_error: Option<String>,
    shutdown: Option<OperationId>,
    shutdown_ready: bool,
    closed: bool,
    xruns: u32,
    active_effect: Effect,
    media_loop: Option<LoopId>,
    media_pending: Option<(TransferId, LoopId)>,
    persistence_epoch: u64,
    persistence_job: Option<persistence_jobs::Job>,
    persistence_last: Option<(Authority, u64)>,
    persistence_closed: Option<(Authority, persistence::Key)>,
    channel_annotations: BTreeMap<(LoopId, bool, u32), (u64, Option<String>)>,
    bootstrap_track: Option<BackendTrackId>,
    bootstrap_loop: Option<BackendLoopId>,
}

impl Service {
    pub fn new(mut backend: Box<dyn Backend>, options: ServiceOptions) -> Result<Self> {
        let observed = backend.poll()?;
        if !observed.tracks.is_empty()
            || !observed.loops.is_empty()
            || !observed.composites.is_empty()
        {
            return Err(anyhow!("a fresh service cannot adopt an existing session"));
        }
        let mut service = Self {
            host_graph_controller: None,
            host_graph: Arc::default(),
            publications: BTreeMap::new(),
            graph_connections: BTreeMap::new(),
            master_graph_auto_done: false,
            backend,
            preview: ClickTrackState::default(),
            preview_bus: None,
            preview_stage: None,
            options,
            host_settings: None,
            host_diagnostics: Vec::new(),
            revision: Revision(1),
            generation: 0,
            lease: None,
            last_authority: None,
            engine_closed: false,
            actors: BTreeMap::new(),
            updates: VecDeque::new(),
            pending: BTreeMap::new(),
            results: VecDeque::new(),
            tracks: BTreeMap::new(),
            track_order: Vec::new(),
            loops: BTreeMap::new(),
            composites: BTreeMap::new(),
            next_composite_event: 1,
            composite_structure: None,
            ports: BTreeMap::new(),
            channels: BTreeMap::new(),
            buses: BTreeMap::new(),
            bus_order: Vec::new(),
            next_bus: 1,
            next_channel: 1,
            next_track: 1,
            next_loop: 1,
            next_port: 1,
            global: GlobalControlState::default(),
            connection_intent: BTreeMap::new(),
            route_intent: BTreeMap::new(),
            piano_notes: BTreeMap::new(),
            released_notes: VecDeque::new(),
            observed,
            init_through: 0,
            init_error: None,
            shutdown: None,
            shutdown_ready: false,
            closed: false,
            xruns: 0,
            active_effect: Effect::None,
            media_loop: None,
            media_pending: None,
            persistence_epoch: 1,
            persistence_job: None,
            persistence_last: None,
            persistence_closed: None,
            channel_annotations: BTreeMap::new(),
            bootstrap_track: None,
            bootstrap_loop: None,
        };
        service.register_buses()?;
        let created = service.backend.create_direct_track(DirectTrackRequest {
            port_name_base: "sync_loop".to_owned(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })?;
        service.bootstrap_track = Some(created.track_id);
        service.bootstrap_loop = created.loops.first().copied();
        service.insert_track(created, "Sync".to_owned(), 1, false, true)?;
        service
            .backend
            .set_loop_smoothing_ms(service.options.loop_smoothing_ms)?;
        if service.options.master_auto_connect {
            service.connect_master()?;
        }
        service.init_through = service.backend.mutation_cursor();
        Ok(service)
    }

    pub fn set_host_graph_controller(
        &mut self,
        controller: Box<dyn crate::HostGraphController>,
    ) -> Result<()> {
        if self.host_graph_controller.is_some() {
            return Err(anyhow!("a port graph controller is already installed"));
        }
        self.host_graph_controller = Some(controller);
        self.poll_host_graph();
        self.advance_publications();
        if self.dynamic_ports() && self.options.master_auto_connect && !self.master_graph_auto_done
        {
            self.connect_master()?;
        }
        Ok(())
    }

    pub fn bind_host_publication(
        &mut self,
        port: PortId,
        key: Option<HostGraphLocalKey>,
    ) -> Result<()> {
        let target = self
            .ports
            .get(&port)
            .ok_or_else(|| anyhow!("unknown application port"))?;
        if let Some(key) = key {
            if self
                .ports
                .values()
                .any(|p| p.view.id != port && p.view.publication == Some(key))
            {
                return Err(anyhow!(
                    "publication is already bound to another application port"
                ));
            }
            let publication = self
                .host_graph
                .publication(key)
                .ok_or_else(|| anyhow!("unknown host publication"))?;
            if publication.spec.data_type != target.view.data_type
                || publication.spec.direction != target.view.direction
            {
                return Err(anyhow!(
                    "publication does not match application port format"
                ));
            }
        }
        self.ports.get_mut(&port).unwrap().view.publication = key;
        self.bump();
        Ok(())
    }

    fn poll_host_graph(&mut self) {
        let Some(controller) = &mut self.host_graph_controller else {
            return;
        };
        while let Some(shoop_port_graph::Completion { id, result }) = controller.take_result() {
            publication::complete(&mut self.publications, id, &result);
            if let Some(key) = self.graph_connections.remove(&id) {
                if let Some(intent) = self.connection_intent.get_mut(&key) {
                    intent.pending = None;
                    intent.error = result.as_ref().err().map(|e| e.message.clone());
                }
            }
            for pending in self.pending.values_mut() {
                if matches!(pending.effect, Effect::HostGraph(candidate) if candidate == id) {
                    pending.effect = Effect::None;
                    if let Err(error) = &result {
                        pending.error = Some(CommandError::graph(error.clone()));
                    }
                }
            }
        }
        let graph = controller.snapshot();
        if graph != self.host_graph {
            self.reconcile_graph(&graph);
            self.host_graph = graph;
            self.bump();
        }
    }

    pub fn set_host_settings(
        &mut self,
        settings: HostSettings,
        diagnostics: Vec<String>,
    ) -> Result<()> {
        settings.validate().map_err(anyhow::Error::msg)?;
        self.host_settings = Some(settings);
        self.host_diagnostics = diagnostics
            .into_iter()
            .take(64)
            .map(|mut s| {
                let mut end = s.len().min(1024);
                while !s.is_char_boundary(end) {
                    end -= 1;
                }
                s.truncate(end);
                s
            })
            .collect();
        Ok(())
    }
    pub fn host_report(&self) -> HostReport {
        HostReport {
            requested: self.host_settings.clone(),
            effective: projection::status(&self.observed.status),
            diagnostics: self.host_diagnostics.clone(),
            ready: self.is_ready(),
            busy: self.lease.is_some(),
            closed: self.closed,
            master_auto_connect: self.options.master_auto_connect,
            loop_smoothing_ms: self.options.loop_smoothing_ms,
            pending_operations: self.pending.len() as u32,
            restart_required_fields: vec!["driver".into(), "pools".into()],
        }
    }

    pub fn failure(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    pub fn is_ready(&self) -> bool {
        !self.closed
            && self.shutdown.is_none()
            && !self.engine_closed
            && self.released_notes.is_empty()
            && self.init_error.is_none()
            && self.revision.0 != u64::MAX
            && !self.backend.mutations_pending_through(self.init_through)
            && self.initial_publications_ready()
            && self
                .bootstrap_track
                .is_none_or(|id| self.observed.tracks.contains_key(&id))
            && self
                .bootstrap_loop
                .is_none_or(|id| self.observed.loops.contains_key(&id))
            && !matches!(
                self.observed.status.driver_state,
                BackendDriverState::Failed | BackendDriverState::Stopped
            )
    }

    pub fn handle(
        &mut self,
        connection: ConnectionId,
        message: ClientMessage,
    ) -> Result<(), Refusal> {
        let _span =
            tracing::trace_span!("app.service.receive", connection = connection.0).entered();
        match message {
            ClientMessage::StageImage {
                authority,
                key,
                target,
                image,
            } => {
                self.check(connection, authority)?;
                if let Err(reason) = self.stage_image(authority, key, target, image) {
                    self.emit(ServiceMessage::Persistence {
                        authority,
                        reply: persistence::Reply::Refused { key, reason },
                    });
                }
            }
            ClientMessage::CaptureImage {
                authority,
                key,
                target,
            } => {
                self.check(connection, authority)?;
                if let Err(reason) = self.capture_image(authority, key, target, true) {
                    self.emit(ServiceMessage::Persistence {
                        authority,
                        reply: persistence::Reply::Refused { key, reason },
                    });
                }
            }
            ClientMessage::Persistence { authority, request } => {
                self.check(connection, authority)?;
                if !request.valid() {
                    return Err(Refusal::Invalid);
                }
                let key = request.key();
                if let Err(reason) = self.persistence_request(authority, request) {
                    self.emit(ServiceMessage::Persistence {
                        authority,
                        reply: persistence::Reply::Refused { key, reason },
                    });
                }
            }
            ClientMessage::ReadMedia {
                authority,
                transfer,
                loop_id,
            } => {
                self.check(connection, authority)?;
                if !self.lease.as_ref().is_some_and(|lease| lease.ready) {
                    return Err(Refusal::NotReady);
                }
                if transfer.0 == 0 {
                    return Err(Refusal::Invalid);
                }
                if self.persistence_reads_busy() {
                    self.media_pending = Some((transfer, loop_id));
                    self.media_loop = None;
                    return Ok(());
                }
                self.backend.cancel_media_reads();
                self.media_loop = None;
                self.media_pending = Some((transfer, loop_id));
            }
            ClientMessage::CancelMedia { authority } => {
                self.check(connection, authority)?;
                if self.persistence_reads_busy() {
                    self.media_pending = None;
                    self.media_loop = None;
                    return Ok(());
                }
                self.media_pending = None;
                self.media_loop = None;
                self.backend.cancel_media_reads();
            }
            ClientMessage::Attach { version } => {
                if version != VERSION {
                    return Err(Refusal::Version);
                }
                if connection.0 == 0 {
                    return Err(Refusal::Identity);
                }
                if self.lease.is_some() {
                    return Err(Refusal::Busy);
                }
                if !self.is_ready() {
                    return Err(Refusal::NotReady);
                }
                self.generation = self.generation.checked_add(1).ok_or(Refusal::Overloaded)?;
                let client = ClientId(u128::from(self.generation));
                self.actors.clear();
                self.actors.insert(client, 0);
                let authority = Authority {
                    lease: LeaseId(connection.0),
                    client,
                    generation: ConnectionGeneration(self.generation),
                };
                self.updates.clear();
                self.last_authority = Some(authority);
                self.lease = Some(Lease {
                    connection,
                    authority,
                    ready: false,
                    snapshot_revision: self.revision,
                });
                self.emit(ServiceMessage::Attached { authority });
                self.publish();
            }
            ClientMessage::Ready {
                authority,
                revision,
            } => {
                self.check(connection, authority)?;
                let lease = self.lease.as_mut().ok_or(Refusal::StaleAuthority)?;
                if revision < lease.snapshot_revision || revision > self.revision {
                    return Err(Refusal::RevisionConflict);
                }
                lease.ready = true;
            }
            ClientMessage::Detach { authority } => {
                self.check(connection, authority)?;
                self.detach(connection);
            }
            ClientMessage::Submit {
                authority,
                session_epoch,
                operation,
                expected_revision,
                command,
            } => {
                self.check(connection, authority)?;
                self.submit(
                    operation,
                    expected_revision,
                    Some(command),
                    Some(session_epoch),
                )?;
            }
            ClientMessage::Shutdown {
                authority,
                operation,
            } => {
                self.check(connection, authority)?;
                self.submit(operation, None, None, None)?;
            }
        }
        Ok(())
    }

    fn check(&self, connection: ConnectionId, authority: Authority) -> Result<(), Refusal> {
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| lease.connection == connection && lease.authority == authority)
        {
            Ok(())
        } else {
            Err(Refusal::StaleAuthority)
        }
    }

    pub fn detach(&mut self, connection: ConnectionId) {
        if self
            .lease
            .as_ref()
            .is_some_and(|lease| lease.connection == connection)
        {
            self.stop_preview();
            self.lease = None;
            for (note, tracks) in core::mem::take(&mut self.piano_notes) {
                for track in tracks {
                    self.released_notes.push_back((track, note));
                }
            }
            self.updates.clear();
            self.persistence_detach();
            self.media_loop = None;
            self.media_pending = None;
        }
    }

    pub fn take_update(&mut self, connection: ConnectionId) -> Option<ServiceMessage> {
        self.lease
            .as_ref()
            .filter(|lease| lease.connection == connection)?;
        self.updates.pop_front()
    }

    fn emit(&mut self, message: ServiceMessage) {
        if self.lease.is_none() {
            return;
        }
        if matches!(message, ServiceMessage::Snapshot { .. }) {
            self.updates
                .retain(|update| !matches!(update, ServiceMessage::Snapshot { .. }));
        }
        if self.updates.len() == QUEUE_CAPACITY {
            if let Some(connection) = self.lease.as_ref().map(|lease| lease.connection) {
                self.detach(connection);
            }
            return;
        }
        self.updates.push_back(message);
    }

    fn publish(&mut self) {
        if let Some(authority) = self.lease.as_ref().map(|lease| lease.authority) {
            let state = self.snapshot();
            self.emit(ServiceMessage::Snapshot { authority, state });
        }
    }

    fn submit(
        &mut self,
        id: OperationId,
        expected: Option<Revision>,
        command: Option<Command>,
        epoch: Option<u64>,
    ) -> Result<(), Refusal> {
        let lease = self.lease.as_ref().ok_or(Refusal::StaleAuthority)?;
        if !lease.ready {
            return Err(Refusal::NotReady);
        }
        self.admit(lease.authority, id, expected, command, epoch)
    }

    fn admit(
        &mut self,
        authority: Authority,
        id: OperationId,
        expected: Option<Revision>,
        command: Option<Command>,
        epoch: Option<u64>,
    ) -> Result<(), Refusal> {
        if id.client != authority.client || id.sequence == 0 {
            return Err(Refusal::Identity);
        }
        if let Some(result) = self.results.iter().find(|result| result.id == id).cloned() {
            self.emit(ServiceMessage::Finished { authority, result });
            return Ok(());
        }
        if let Some(pending) = self.pending.get(&id) {
            let operation = PendingOperation {
                id,
                accepted_at: pending.accepted_at,
            };
            self.emit(ServiceMessage::Accepted {
                authority,
                operation,
            });
            return Ok(());
        }
        let watermark = self.actors.get_mut(&id.client).ok_or(Refusal::Identity)?;
        if id.sequence <= *watermark {
            let result = OperationResult {
                id,
                outcome: refused(
                    Refusal::Duplicate,
                    "the operation is older than retained results",
                ),
            };
            self.emit(ServiceMessage::Finished { authority, result });
            return Ok(());
        }
        *watermark = id.sequence;
        let refused = if (!self.is_ready() && !(command.is_none() && self.engine_closed))
            || self.shutdown.is_some()
        {
            Some((Refusal::NotReady, "the session is not accepting commands"))
        } else if epoch.is_some_and(|epoch| epoch != self.persistence_epoch) {
            Some((Refusal::Identity, "the session was replaced"))
        } else if expected.is_some_and(|expected| expected != self.revision) {
            Some((Refusal::RevisionConflict, "the shared state changed"))
        } else if self.composite_structure_busy(command.as_ref()) {
            Some((
                Refusal::Busy,
                "a composite lifecycle operation is preserving dependent arrangements",
            ))
        } else if self.persistence_busy(command.as_ref()) {
            Some((
                Refusal::Busy,
                "persistence is preparing an immutable capture or applying a replacement",
            ))
        } else if command.as_ref().is_some_and(|command| !command.bounded()) {
            Some((Refusal::Invalid, "the command exceeds protocol bounds"))
        } else if self.pending.len() == MAX_PENDING_OPERATIONS
            || self.backend.mutation_capacity()
                < self.request_budget(command.as_ref()).saturating_add(
                    self.composite_structure
                        .as_ref()
                        .map_or(0, |job| job.reserve()),
                )
        {
            Some((Refusal::Overloaded, "operation tracking is full"))
        } else {
            None
        };
        if let Some((reason, message)) = refused {
            self.record(OperationResult {
                id,
                outcome: refused_outcome(reason, message),
            });
            self.publish();
            return Ok(());
        }
        let before = self.backend.mutation_cursor();
        self.pending.insert(
            id,
            Pending {
                id,
                accepted_at: self.revision,
                first: before.saturating_add(1),
                last: before,
                completed: BTreeSet::new(),
                error: None,
                effect: Effect::None,
                publications: Vec::new(),
                retiring: false,
                rollback: false,
            },
        );
        self.emit(ServiceMessage::Accepted {
            authority,
            operation: PendingOperation {
                id,
                accepted_at: self.revision,
            },
        });
        self.active_effect = Effect::None;
        let result = match command {
            Some(command) => self.execute(command),
            None => self.prepare_shutdown(),
        };
        let after = self.backend.mutation_cursor();
        if let Some(job) = self
            .composite_structure
            .as_mut()
            .filter(|job| job.operation.is_none())
        {
            job.operation = Some(id);
        }
        let pending = self.pending.get_mut(&id).expect("reserved operation");
        pending.last = after;
        pending.effect = self.active_effect.clone();
        match result {
            Ok(effect) => {
                if matches!(effect, Effect::Shutdown) {
                    self.shutdown = Some(id);
                }
                pending.effect = effect;
            }
            Err(error) => pending.error = Some(error),
        }
        let keys = self.effect_publications(&self.pending[&id].effect);
        self.pending.get_mut(&id).unwrap().publications = keys;
        self.bump();
        if before == after {
            self.tick(Duration::ZERO);
        } else {
            self.publish();
        }
        Ok(())
    }

    fn request_budget(&self, command: Option<&Command>) -> usize {
        match command {
            Some(
                Command::SetHostGraphConnected(_)
                | Command::SelectLoops { .. }
                | Command::SetTarget(_)
                | Command::Track {
                    action: TrackAction::NameChanged(_) | TrackAction::MoveBefore(_),
                    ..
                }
                | Command::Loop {
                    action:
                        LoopAction::NameChanged(_)
                        | LoopAction::IconClicked(_)
                        | LoopAction::IconDoubleClicked
                        | LoopAction::MoveBefore(_)
                        | LoopAction::SwapWith(_),
                    ..
                },
            ) => 0,
            _ => {
                3 * self.loops.len()
                    + 16 * self.tracks.len()
                    + 16 * self.buses.len()
                    + 64
                    + self.piano_notes.values().map(BTreeSet::len).sum::<usize>()
            }
        }
    }

    fn record(&mut self, result: OperationResult) {
        if self.results.len() == MAX_RETAINED_RESULTS {
            self.results.pop_front();
        }
        self.results.push_back(result.clone());
        if let Some(authority) = self.lease.as_ref().map(|lease| lease.authority) {
            self.emit(ServiceMessage::Finished { authority, result });
        }
    }

    fn finish(&mut self, id: OperationId) {
        let Some(pending) = self.pending.remove(&id) else {
            return;
        };
        if matches!(pending.effect, Effect::Shutdown) && pending.error.is_none() {
            self.shutdown_ready = true;
            self.pending.insert(id, pending);
            return;
        }
        let stopped_ancestors = self
            .composite_structure
            .as_ref()
            .filter(|job| job.operation == Some(id))
            .map(|job| job.stopped.clone())
            .unwrap_or_default();
        if self
            .composite_structure
            .as_ref()
            .is_some_and(|job| job.operation == Some(id))
        {
            self.composite_structure = None;
        }
        self.settle_effect(&pending.effect, pending.error.is_none());
        self.bump();
        let is_shutdown = matches!(pending.effect, Effect::Shutdown);
        let outcome = match pending.error {
            Some(error) if !stopped_ancestors.is_empty() => {
                let message = alloc::format!(
                    "{} ({} dependent arrangements were stopped.)",
                    error.message.chars().take(400).collect::<String>(),
                    stopped_ancestors.len()
                );
                refused_outcome(error.reason, &message)
            }
            Some(error) => refused_outcome(error.reason, &error.message),
            None => match pending.effect {
                Effect::TrackCreated(track, loops) => Outcome::TrackCreated {
                    revision: self.revision,
                    track,
                    loops,
                },
                Effect::LoopCreated(track, loop_id) => Outcome::LoopCreated {
                    revision: self.revision,
                    track,
                    loop_id,
                },
                _ if !stopped_ancestors.is_empty() => Outcome::AncestorsStopped {
                    revision: self.revision,
                    loops: stopped_ancestors,
                },
                _ => Outcome::Confirmed {
                    revision: self.revision,
                },
            },
        };
        if is_shutdown {
            self.shutdown = None;
        }
        self.record(OperationResult { id, outcome });
    }

    fn bump(&mut self) {
        self.revision.0 = self.revision.0.saturating_add(1);
    }

    pub fn tick(&mut self, elapsed: Duration) {
        if self.closed {
            self.publish();
            return;
        }
        let _span = tracing::trace_span!(
            "app.service.tick",
            revision = self.revision.0,
            pending = self.pending.len()
        )
        .entered();
        self.poll_host_graph();
        for _ in 0..MAX_MESSAGES_PER_TURN {
            if self.backend.mutation_capacity() == 0 {
                break;
            }
            let Some((id, note)) = self.released_notes.front().copied() else {
                break;
            };
            if let Some(track) = self.tracks.get(&id) {
                if self
                    .backend
                    .inject_midi_input(
                        track.backend,
                        &[BackendMidiEvent {
                            frame: 0,
                            data: vec![0x80, note, 0].into(),
                        }],
                    )
                    .is_err()
                {
                    break;
                }
            }
            self.released_notes.pop_front();
        }
        if self.engine_closed {
            let done: Vec<_> = self
                .pending
                .values()
                .filter(|pending| {
                    pending.first > pending.last
                        && !self.shutdown_ready
                        && !matches!(pending.effect, Effect::HostGraph(_))
                })
                .map(|pending| pending.id)
                .collect();
            for id in done {
                self.finish(id);
            }
            self.publish();
            return;
        }
        self.backend.advance(elapsed);
        self.advance_preview();
        match self.backend.poll() {
            Ok(mut observed) => {
                self.observe_composites(&observed);
                self.observe_composite_structure(&observed.mutation_completions);
                let completions = core::mem::take(&mut observed.mutation_completions);
                self.apply_observation(observed);
                self.refresh_natural_composites();
                for completion in completions {
                    publication::engine_complete(&mut self.publications, &completion);
                    for track in self.tracks.values_mut() {
                        if track
                            .monitoring_intent
                            .is_some_and(|(sequence, _)| sequence == completion.sequence)
                        {
                            track.monitoring_intent = None;
                        }
                    }
                    for intent in self
                        .connection_intent
                        .values_mut()
                        .chain(self.route_intent.values_mut())
                    {
                        if intent.pending == Some(completion.sequence) {
                            intent.pending = None;
                            intent.error = completion.result.as_ref().err().cloned();
                        }
                    }
                    if completion.sequence <= self.init_through {
                        if let Err(error) = &completion.result {
                            self.init_error = Some(error.clone());
                        }
                    }
                    for pending in self.pending.values_mut().filter(|pending| {
                        (pending.first..=pending.last).contains(&completion.sequence)
                    }) {
                        pending.completed.insert(completion.sequence);
                        if let Err(error) = &completion.result {
                            pending.error = Some(CommandError {
                                reason: Refusal::Engine,
                                message: error.clone(),
                            });
                        }
                    }
                }
                self.advance_composite_structure();
                self.advance_publications();
                self.advance_graph_connections();
                let done = self.completed_operations();
                for id in done {
                    self.finish(id);
                }
            }
            Err(error) => {
                self.init_error = Some(error.to_string());
                self.observed.status.driver_state = BackendDriverState::Failed;
                let ids: Vec<_> = self.pending.keys().copied().collect();
                for id in ids {
                    self.pending.get_mut(&id).unwrap().error = Some(CommandError {
                        reason: Refusal::Engine,
                        message: error.to_string(),
                    });
                    self.finish(id);
                }
            }
        }
        self.advance_persistence();
        if !self.persistence_reads_busy() {
            self.advance_media();
        }
        self.publish();
    }

    pub fn has_connection(&self, connection: ConnectionId) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|lease| lease.connection == connection)
    }

    pub fn read_audio(
        &mut self,
        connection: ConnectionId,
        authority: Authority,
        loop_id: LoopId,
    ) -> Result<Option<BackendAudioData>> {
        if self.persistence_reads_busy() {
            return Err(anyhow!("persistence capture is using media reads"));
        }
        self.check(connection, authority)
            .map_err(|reason| anyhow!("{reason:?}"))?;
        if !self.lease.as_ref().is_some_and(|lease| lease.ready) {
            return Err(anyhow!("NotReady"));
        }
        let backend = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown loop"))?
            .backend;
        if self.media_loop != Some(loop_id) {
            self.backend.cancel_media_reads();
            self.media_loop = Some(loop_id);
        }

        self.backend.loop_audio_data_with_metadata(backend)
    }

    pub fn read_midi(
        &mut self,
        connection: ConnectionId,
        authority: Authority,
        loop_id: LoopId,
    ) -> Result<Option<BackendMidiData>> {
        if self.persistence_reads_busy() {
            return Err(anyhow!("persistence capture is using media reads"));
        }
        self.check(connection, authority)
            .map_err(|reason| anyhow!("{reason:?}"))?;
        if !self.lease.as_ref().is_some_and(|lease| lease.ready) {
            return Err(anyhow!("NotReady"));
        }
        let backend = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown loop"))?
            .backend;
        if self.media_loop != Some(loop_id) {
            self.backend.cancel_media_reads();
            self.media_loop = Some(loop_id);
        }

        self.backend.loop_midi_data(backend)
    }

    pub fn shutdown_ready(&self) -> bool {
        self.shutdown_ready
    }

    /// The composition host calls this after device stop and resource cleanup finish.
    pub fn complete_shutdown(&mut self, result: Result<(), String>) {
        let Some(id) = self.shutdown.take() else {
            return;
        };
        self.shutdown_ready = false;
        self.pending.remove(&id);
        self.closed = result.is_ok();
        if self.engine_closed && !self.closed {
            self.observed.status.driver_state = BackendDriverState::Failed;
        }
        if self.closed {
            self.observed.status.driver_state = BackendDriverState::Stopped;
        }
        let outcome = match result {
            Ok(()) => Outcome::Confirmed {
                revision: self.revision,
            },
            Err(message) => refused_outcome(Refusal::ShutdownFailed, &message),
        };
        let result = OperationResult { id, outcome };
        if self.results.len() == MAX_RETAINED_RESULTS {
            self.results.pop_front();
        }
        self.results.push_back(result.clone());
        if let Some(authority) = self.lease.as_ref().map(|lease| lease.authority) {
            self.emit(ServiceMessage::ShutdownComplete { authority, result });
        }
    }

    #[cfg(feature = "std")]
    pub fn pump_local(
        &mut self,
        connection: ConnectionId,
        port: &local::ServicePort,
        elapsed: Duration,
    ) {
        for _ in 0..MAX_COMMANDS_PER_TURN {
            match port.commands.try_recv() {
                Ok(message) => {
                    if let Err(reason) = self.handle(connection, message) {
                        if port
                            .updates
                            .try_send(ServiceMessage::Rejected(reason))
                            .is_err()
                        {
                            self.detach(connection);
                            break;
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.detach(connection);
                    break;
                }
            }
        }
        self.tick(elapsed);
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some(update) = self.take_update(connection) else {
                break;
            };
            match port.updates.try_send(update) {
                Ok(()) => {}
                Err(std::sync::mpsc::TrySendError::Full(update)) => {
                    self.updates.push_front(update);
                    break;
                }
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    self.detach(connection);
                    break;
                }
            }
        }
    }
}

fn refused(reason: Refusal, message: &str) -> Outcome {
    refused_outcome(reason, message)
}
fn refused_outcome(reason: Refusal, message: &str) -> Outcome {
    let mut length = message.len().min(512);
    while !message.is_char_boundary(length) {
        length -= 1;
    }
    Outcome::Refused {
        reason,
        message: message[..length].to_owned(),
    }
}

fn allocate(next: &mut u64) -> Result<u64> {
    let id = *next;
    *next = next
        .checked_add(1)
        .ok_or_else(|| anyhow!("session identity space exhausted"))?;
    Ok(id)
}
