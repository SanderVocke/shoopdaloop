#![forbid(unsafe_code)]

mod commands;
mod management;
mod media;
mod projection;

use anyhow::{anyhow, Result};
use shoop_app_api::*;
use shoop_app_protocol::*;
use shoop_backend::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

pub const MAX_TRACKS: usize = 64;
pub const MAX_LOOPS: usize = 256;
const MAX_ACTORS: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConnectionId(pub u64);

pub struct ServiceOptions {
    pub host: HostId,
    pub session: SessionId,
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
    TrackCreated(TrackId, Vec<LoopId>),
    LoopCreated(TrackId, LoopId),
    TrackRemoved(TrackId),
    SessionReset(Vec<TrackId>),
    Shutdown,
}

struct Pending {
    id: OperationId,
    accepted_at: Revision,
    first: u64,
    last: u64,
    completed: BTreeSet<u64>,
    error: Option<CommandError>,
    effect: Effect,
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
    backend: Box<dyn Backend>,
    options: ServiceOptions,
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
    ports: BTreeMap<PortId, Port>,
    channels: BTreeMap<BusChannelId, BackendBusChannelId>,
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
}

impl Service {
    pub fn new(mut backend: Box<dyn Backend>, options: ServiceOptions) -> Result<Self> {
        if options.host.0 == 0 || options.session.0 == 0 {
            return Err(anyhow!("invalid host/session identity"));
        }
        let observed = backend.poll()?;
        if !observed.tracks.is_empty() || !observed.loops.is_empty() {
            return Err(anyhow!("a fresh service cannot adopt an existing session"));
        }
        let mut service = Self {
            backend,
            options,
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
            ports: BTreeMap::new(),
            channels: BTreeMap::new(),
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
        };
        service.register_master()?;
        let created = service.backend.create_direct_track(DirectTrackRequest {
            port_name_base: "sync_loop".to_owned(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })?;
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
            && self
                .tracks
                .get(&TrackId::from_raw(1))
                .is_some_and(|track| track.view.structural_state == StructuralState::Confirmed)
            && self
                .loops
                .get(&LoopId::from_raw(1))
                .is_some_and(|entry| entry.view.structural_state == StructuralState::Confirmed)
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
                self.backend.cancel_media_reads();
                self.media_loop = None;
                self.media_pending = Some((transfer, loop_id));
            }
            ClientMessage::CancelMedia { authority } => {
                self.check(connection, authority)?;
                self.media_pending = None;
                self.media_loop = None;
                self.backend.cancel_media_reads();
            }
            ClientMessage::Attach {
                version,
                host,
                session,
                client,
            } => {
                if version != VERSION {
                    return Err(Refusal::Version);
                }
                if connection.0 == 0
                    || host != self.options.host
                    || session != self.options.session
                    || client.0 == 0
                {
                    return Err(Refusal::Identity);
                }
                if self.lease.is_some() {
                    return Err(Refusal::Busy);
                }
                if !self.is_ready() {
                    return Err(Refusal::NotReady);
                }
                if !self.actors.contains_key(&client) && self.actors.len() == MAX_ACTORS {
                    return Err(Refusal::Overloaded);
                }
                self.generation = self.generation.checked_add(1).ok_or(Refusal::Overloaded)?;
                self.actors.entry(client).or_insert(0);
                let authority = Authority {
                    session,
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
                self.emit(ServiceMessage::Attached { authority, host });
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
                operation,
                expected_revision,
                command,
            } => {
                self.check(connection, authority)?;
                self.submit(operation, expected_revision, Some(command))?;
            }
            ClientMessage::Shutdown {
                authority,
                operation,
            } => {
                self.check(connection, authority)?;
                self.submit(operation, None, None)?;
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
            self.lease = None;
            for (note, tracks) in std::mem::take(&mut self.piano_notes) {
                for track in tracks {
                    self.released_notes.push_back((track, note));
                }
            }
            self.updates.clear();
            self.backend.cancel_media_reads();
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
    ) -> Result<(), Refusal> {
        let lease = self.lease.as_ref().ok_or(Refusal::StaleAuthority)?;
        if !lease.ready {
            return Err(Refusal::NotReady);
        }
        self.admit(lease.authority, id, expected, command)
    }

    fn admit(
        &mut self,
        authority: Authority,
        id: OperationId,
        expected: Option<Revision>,
        command: Option<Command>,
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
        } else if expected.is_some_and(|expected| expected != self.revision) {
            Some((Refusal::RevisionConflict, "the shared state changed"))
        } else if command.as_ref().is_some_and(|command| !command.bounded()) {
            Some((Refusal::Invalid, "the command exceeds protocol bounds"))
        } else if self.pending.len() == MAX_PENDING_OPERATIONS
            || self.backend.mutation_capacity() < self.request_budget(command.as_ref())
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
                Command::SelectLoops { .. }
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
        self.settle_effect(&pending.effect, pending.error.is_none());
        self.bump();
        let is_shutdown = matches!(pending.effect, Effect::Shutdown);
        let outcome = match pending.error {
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
                            time: 0,
                            data: vec![0x80, note, 0],
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
                .filter(|pending| pending.first > pending.last && !self.shutdown_ready)
                .map(|pending| pending.id)
                .collect();
            for id in done {
                self.finish(id);
            }
            self.publish();
            return;
        }
        self.backend.advance(elapsed);
        match self.backend.poll() {
            Ok(mut observed) => {
                let completions = std::mem::take(&mut observed.mutation_completions);
                self.apply_observation(observed);
                for completion in completions {
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
                let done: Vec<_> = self
                    .pending
                    .values()
                    .filter(|pending| {
                        pending.completed.len() as u64
                            == pending
                                .last
                                .checked_sub(pending.first)
                                .map_or(0, |count| count + 1)
                            && !(self.shutdown_ready && self.shutdown == Some(pending.id))
                    })
                    .map(|pending| pending.id)
                    .collect();
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
        self.advance_media();
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
