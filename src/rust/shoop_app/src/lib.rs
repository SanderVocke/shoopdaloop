use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use shoop_app_api::{
    AppIntent, AppNotification, AppSnapshot, AudioDriverState, ChannelId, ConnectionErrorKind,
    ConnectionErrorState, ConnectionViewState, DirectTrackSpec, ExternalPortConnectionState,
    GlobalControlAction, LocalPortConnectionState, LoopAction, LoopDetailsState, LoopId, LoopMode,
    LoopState, NotificationLevel, PortDataType, PortDirection, PortId, PortRole, StatusState,
    TrackAction, TrackControlState, TrackId, TrackState, WaveformChannelState,
};
use shoop_backend::{
    Backend, BackendConnectionSnapshot, BackendGrabRequest, BackendLoopId, BackendLoopMode,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortRole, BackendSnapshot, BackendTrackControl, BackendTrackId, DirectTrackRequest,
};

const COMMAND_CAPACITY: usize = 1024;
const MAX_COOPERATIVE_COMMANDS_PER_TICK: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone)]
pub struct ApplicationHandle {
    sender: SyncSender<ApplicationMessage>,
    snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
    saturated_connection: Arc<Mutex<Option<(PortId, String)>>>,
}

impl ApplicationHandle {
    pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
        match self.sender.try_send(ApplicationMessage::Intent(intent)) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(ApplicationMessage::Intent(AppIntent::SetPortConnected {
                port_id,
                external_port,
                ..
            }))) => {
                *self
                    .saturated_connection
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = Some((port_id, external_port));
                Err(DispatchError::Full)
            }
            Err(error) => Err(DispatchError::from(error)),
        }
    }

    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        self.snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    Full,
    Disconnected,
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => formatter.write_str("application command queue is full"),
            Self::Disconnected => formatter.write_str("application actor is disconnected"),
        }
    }
}

impl std::error::Error for DispatchError {}

impl From<TrySendError<ApplicationMessage>> for DispatchError {
    fn from(error: TrySendError<ApplicationMessage>) -> Self {
        match error {
            TrySendError::Full(_) => Self::Full,
            TrySendError::Disconnected(_) => Self::Disconnected,
        }
    }
}

pub struct ApplicationRuntime {
    handle: ApplicationHandle,
    join: Option<JoinHandle<()>>,
}

impl ApplicationRuntime {
    pub fn start(mut backend: Box<dyn Backend + Send>) -> Result<Self> {
        let model = ApplicationModel::initialize(&mut *backend)?;
        let initial = Arc::new(model.snapshot());
        let snapshot = Arc::new(RwLock::new(initial));
        let (sender, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let saturated_connection = Arc::new(Mutex::new(None));
        let handle = ApplicationHandle {
            sender,
            snapshot: Arc::clone(&snapshot),
            saturated_connection: Arc::clone(&saturated_connection),
        };
        let join = thread::Builder::new()
            .name("shoop-application".to_owned())
            .spawn(move || run_actor(model, backend, receiver, snapshot, saturated_connection))?;
        Ok(Self {
            handle,
            join: Some(join),
        })
    }

    pub fn handle(&self) -> ApplicationHandle {
        self.handle.clone()
    }
}

impl Drop for ApplicationRuntime {
    fn drop(&mut self) {
        let _ = self.handle.sender.send(ApplicationMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct CooperativeApplicationRuntime {
    model: ApplicationModel,
    backend: Box<dyn Backend>,
    commands: VecDeque<AppIntent>,
    snapshot: Arc<AppSnapshot>,
}

impl CooperativeApplicationRuntime {
    pub fn start(mut backend: Box<dyn Backend>) -> Result<Self> {
        let model = ApplicationModel::initialize(&mut *backend)?;
        let snapshot = Arc::new(model.snapshot());
        Ok(Self {
            model,
            backend,
            commands: VecDeque::with_capacity(COMMAND_CAPACITY),
            snapshot,
        })
    }

    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
        if self.commands.len() >= COMMAND_CAPACITY {
            if let AppIntent::SetPortConnected {
                port_id,
                external_port,
                ..
            } = intent
            {
                self.model
                    .report_connection_saturation(port_id, external_port);
                self.snapshot = Arc::new(self.model.snapshot());
            }
            return Err(DispatchError::Full);
        }
        self.commands.push_back(intent);
        Ok(())
    }

    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        Arc::clone(&self.snapshot)
    }

    pub fn tick(&mut self, elapsed: Duration) {
        for _ in 0..MAX_COOPERATIVE_COMMANDS_PER_TICK {
            let Some(intent) = self.commands.pop_front() else {
                break;
            };
            self.model.handle_intent(&mut *self.backend, intent);
        }
        update_application(&mut self.model, &mut *self.backend, elapsed, |snapshot| {
            self.snapshot = snapshot
        });
    }

    pub fn has_pending_commands(&self) -> bool {
        !self.commands.is_empty()
    }
}

enum ApplicationMessage {
    Intent(AppIntent),
    Shutdown,
}

fn run_actor(
    mut model: ApplicationModel,
    mut backend: Box<dyn Backend + Send>,
    receiver: Receiver<ApplicationMessage>,
    published: Arc<RwLock<Arc<AppSnapshot>>>,
    saturated_connection: Arc<Mutex<Option<(PortId, String)>>>,
) {
    let mut last_update = Instant::now();
    loop {
        match receiver.recv_timeout(POLL_INTERVAL) {
            Ok(ApplicationMessage::Intent(intent)) => model.handle_intent(&mut *backend, intent),
            Ok(ApplicationMessage::Shutdown) => break,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
        if let Some((port_id, external_port)) = saturated_connection
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            model.report_connection_saturation(port_id, external_port);
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(last_update);
        last_update = now;
        update_application(&mut model, &mut *backend, elapsed, |snapshot| {
            *published.write().unwrap_or_else(|error| error.into_inner()) = snapshot;
        });
    }
}

fn update_application(
    model: &mut ApplicationModel,
    backend: &mut dyn Backend,
    elapsed: Duration,
    publish: impl FnOnce(Arc<AppSnapshot>),
) {
    backend.advance(elapsed);
    model.age_pending_connections(elapsed);
    match backend.poll() {
        Ok(snapshot) => model.apply_backend_snapshot(snapshot),
        Err(error) => {
            model.connection_backend_available = false;
            model.push_connection_error(ConnectionErrorState {
                port_id: None,
                external_port: None,
                kind: ConnectionErrorKind::BackendUnavailable,
                message: format!("backend poll failed: {error}"),
            });
            model.notify_error(format!("backend poll failed: {error}"));
        }
    }
    if let Err(error) = model.refresh_selected_audio(backend) {
        model.notify_error(error);
    }
    model.revision = model.revision.wrapping_add(1);
    publish(Arc::new(model.snapshot()));
}

struct ApplicationModel {
    revision: u64,
    next_track_id: u64,
    next_loop_id: u64,
    next_port_id: u64,
    tracks: Vec<TrackModel>,
    loops: BTreeMap<LoopId, LoopModel>,
    connection_ports: BTreeMap<PortId, ConnectionPortModel>,
    pending_connections: BTreeMap<(PortId, String), PendingConnection>,
    connection_errors: Vec<ConnectionErrorState>,
    connection_revision: u64,
    connection_backend_available: bool,
    connection_view: Arc<ConnectionViewState>,
    global: shoop_app_api::GlobalControlState,
    status: StatusState,
    notifications: Vec<AppNotification>,
}

struct TrackModel {
    id: TrackId,
    backend_id: BackendTrackId,
    name: String,
    port_name_base: String,
    is_sync: bool,
    audio_channels: u8,
    loops: Vec<LoopId>,
    port_ids: Arc<[PortId]>,
    controls: TrackControlState,
}

struct ConnectionPortModel {
    id: PortId,
    backend_id: BackendPortId,
    track_id: TrackId,
    name: String,
    data_type: PortDataType,
    direction: PortDirection,
    role: PortRole,
    candidates: BTreeMap<String, (bool, bool)>,
}

struct PendingConnection {
    desired_connected: bool,
    age: Duration,
}

struct LoopModel {
    id: LoopId,
    backend_id: BackendLoopId,
    track_id: TrackId,
    name: String,
    state: LoopState,
    length: u32,
    position: u32,
    audio_data: Option<Vec<Arc<[f32]>>>,
}

impl ApplicationModel {
    fn initialize(backend: &mut dyn Backend) -> Result<Self> {
        let created = backend.create_direct_track(DirectTrackRequest {
            port_name_base: "sync_loop".to_owned(),
            audio_channels: 1,
            midi: false,
            initial_loops: 1,
        })?;
        backend.wait_idle();
        let backend_loop = created.loops[0];
        let track_id = TrackId::from_raw(1);
        let loop_id = LoopId::from_raw(1);
        let mut next_port_id = 1;
        let mut connection_ports = BTreeMap::new();
        let port_ids = register_backend_ports(
            track_id,
            &created.ports,
            &mut next_port_id,
            &mut connection_ports,
        );
        let loop_model = LoopModel {
            id: loop_id,
            backend_id: backend_loop,
            track_id,
            name: "sync loop".to_owned(),
            state: LoopState {
                id: loop_id,
                name: "sync loop".to_owned(),
                sync: true,
                show_gain: true,
                ..Default::default()
            },
            length: 0,
            position: 0,
            audio_data: None,
        };
        Ok(Self {
            revision: 1,
            next_track_id: 2,
            next_loop_id: 2,
            next_port_id,
            tracks: vec![TrackModel {
                id: track_id,
                backend_id: created.track_id,
                name: "Sync".to_owned(),
                port_name_base: "sync_loop".to_owned(),
                is_sync: true,
                audio_channels: 1,
                loops: vec![loop_id],
                port_ids,
                controls: Default::default(),
            }],
            loops: BTreeMap::from([(loop_id, loop_model)]),
            connection_ports,
            pending_connections: BTreeMap::new(),
            connection_errors: Vec::new(),
            connection_revision: 1,
            connection_backend_available: false,
            connection_view: Arc::new(ConnectionViewState::default()),
            global: Default::default(),
            status: Default::default(),
            notifications: Vec::new(),
        })
    }

    fn handle_intent(&mut self, backend: &mut dyn Backend, intent: AppIntent) {
        let result = match intent {
            AppIntent::Loop {
                track_id,
                loop_id,
                action,
            } => self.handle_loop_action(backend, track_id, loop_id, action),
            AppIntent::Global(action) => self.handle_global_action(backend, action),
            AppIntent::Track { track_id, action } => {
                self.handle_track_action(backend, track_id, action)
            }
            AppIntent::AddTrack(spec) => self.add_track(backend, spec),
            AppIntent::AddLoop { track_id } => self.add_aligned_loop_row(backend, track_id),
            AppIntent::SetPortConnected {
                port_id,
                external_port,
                connected,
            } => self.set_port_connected(backend, port_id, external_port, connected),
        };
        if let Err(error) = result {
            self.notify_error(error);
        }
    }

    fn add_track(
        &mut self,
        backend: &mut dyn Backend,
        spec: DirectTrackSpec,
    ) -> Result<(), String> {
        spec.validate()
            .map_err(|error| format!("invalid track: {error:?}"))?;
        let slot_count = self
            .tracks
            .iter()
            .filter(|track| !track.is_sync)
            .map(|track| track.loops.len())
            .max()
            .unwrap_or(0)
            .max(8);
        let track_id = TrackId::from_raw(self.next_track_id);
        let port_name_base = self.unique_port_name(&spec.name, track_id);
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: port_name_base.clone(),
                audio_channels: spec.audio_channels,
                midi: spec.midi,
                initial_loops: slot_count,
            })
            .map_err(|error| format!("could not create track: {error}"))?;
        let sync_backend = self.sync_backend_loop();
        for backend_loop in &created.loops {
            backend
                .set_loop_sync_source(*backend_loop, sync_backend)
                .map_err(|error| format!("could not synchronize new loop: {error}"))?;
        }
        let port_ids = register_backend_ports(
            track_id,
            &created.ports,
            &mut self.next_port_id,
            &mut self.connection_ports,
        );
        self.next_track_id = self.next_track_id.saturating_add(1);
        let mut loop_ids = Vec::with_capacity(created.loops.len());
        for (index, backend_loop) in created.loops.into_iter().enumerate() {
            loop_ids.push(self.insert_loop(
                track_id,
                backend_loop,
                format!("({})", index + 1),
                spec.audio_channels,
            ));
        }
        self.tracks.push(TrackModel {
            id: track_id,
            backend_id: created.track_id,
            name: spec.name,
            port_name_base,
            is_sync: false,
            audio_channels: spec.audio_channels,
            loops: loop_ids,
            port_ids,
            controls: Default::default(),
        });
        Ok(())
    }

    fn add_aligned_loop_row(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
    ) -> Result<(), String> {
        let target = self
            .tracks
            .iter()
            .position(|track| track.id == track_id && !track.is_sync)
            .ok_or_else(|| format!("stale, unknown, or sync track {track_id}"))?;
        let previous_len = self.tracks[target].loops.len();
        let after_len = previous_len + 1;
        let max_after = self
            .tracks
            .iter()
            .filter(|track| !track.is_sync)
            .map(|track| track.loops.len())
            .max()
            .unwrap_or(0)
            .max(after_len);
        let affected: Vec<_> = self
            .tracks
            .iter()
            .enumerate()
            .filter(|(index, track)| {
                !track.is_sync
                    && (*index == target
                        || after_len == max_after && track.loops.len() == previous_len)
            })
            .map(|(index, track)| (index, track.backend_id, track.audio_channels))
            .collect();
        let mut created = Vec::with_capacity(affected.len());
        for (index, backend_track, audio_channels) in affected {
            let backend_loop = backend
                .add_loop_to_track(backend_track)
                .map_err(|error| format!("could not add aligned loop: {error}"))?;
            if let Some(sync) = self.sync_backend_loop() {
                backend
                    .set_loop_sync_source(backend_loop, Some(sync))
                    .map_err(|error| format!("could not synchronize added loop: {error}"))?;
            }
            created.push((index, backend_loop, audio_channels));
        }
        for (track_index, backend_loop, audio_channels) in created {
            let name = format!("({})", self.tracks[track_index].loops.len() + 1);
            let id = self.insert_loop(
                self.tracks[track_index].id,
                backend_loop,
                name,
                audio_channels,
            );
            self.tracks[track_index].loops.push(id);
        }
        Ok(())
    }

    fn handle_track_action(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        action: TrackAction,
    ) -> Result<(), String> {
        let track = self
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        let backend_action = match action {
            TrackAction::NameChanged(name) => {
                track.name = name;
                return Ok(());
            }
            TrackAction::OutputGainChanged(value) => BackendTrackControl::OutputGainDb(value),
            TrackAction::OutputBalanceChanged(value) => BackendTrackControl::OutputBalance(value),
            TrackAction::OutputMuteChanged(value) => BackendTrackControl::OutputMute(value),
            TrackAction::InputGainChanged(value) => BackendTrackControl::InputGainDb(value),
            TrackAction::InputBalanceChanged(value) => BackendTrackControl::InputBalance(value),
            TrackAction::InputMonitoringChanged(value) => {
                BackendTrackControl::InputMonitoring(value)
            }
        };
        backend
            .set_track_control(track.backend_id, backend_action)
            .map_err(|error| format!("could not update track {track_id}: {error}"))
    }

    fn insert_loop(
        &mut self,
        track_id: TrackId,
        backend_id: BackendLoopId,
        name: String,
        audio_channels: u8,
    ) -> LoopId {
        let id = LoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            LoopModel {
                id,
                backend_id,
                track_id,
                name: name.clone(),
                state: LoopState {
                    id,
                    name,
                    show_gain: audio_channels > 0,
                    stereo: audio_channels == 2,
                    ..Default::default()
                },
                length: 0,
                position: 0,
                audio_data: None,
            },
        );
        id
    }

    fn sync_backend_loop(&self) -> Option<BackendLoopId> {
        self.tracks
            .iter()
            .find(|track| track.is_sync)
            .and_then(|track| track.loops.first())
            .and_then(|id| self.loops.get(id))
            .map(|model| model.backend_id)
    }

    fn unique_port_name(&self, name: &str, id: TrackId) -> String {
        let base: String = name
            .trim()
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        if self.tracks.iter().any(|track| track.port_name_base == base) {
            format!("{base}_{}", id.raw())
        } else {
            base
        }
    }

    fn handle_loop_action(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        loop_id: LoopId,
        action: LoopAction,
    ) -> Result<(), String> {
        let Some(loop_model) = self.loops.get(&loop_id) else {
            return Err(format!("stale or unknown loop {loop_id}"));
        };
        if loop_model.track_id != track_id {
            return Err(format!(
                "loop {loop_id} does not belong to track {track_id}"
            ));
        }
        match action {
            LoopAction::IconClicked(modifiers) => {
                let was_selected = self
                    .loops
                    .get(&loop_id)
                    .is_some_and(|model| model.state.selected);
                let was_targeted = self
                    .loops
                    .get(&loop_id)
                    .is_some_and(|model| model.state.targeted);
                if was_targeted {
                    if let Some(model) = self.loops.get_mut(&loop_id) {
                        model.state.targeted = false;
                        model.state.selected = false;
                    }
                } else {
                    if !modifiers.additive && !was_selected {
                        for model in self.loops.values_mut() {
                            model.state.selected = false;
                        }
                    }
                    if let Some(model) = self.loops.get_mut(&loop_id) {
                        model.state.targeted = false;
                        model.state.selected = !was_selected;
                    }
                }
                self.refresh_selected_audio(backend)?;
                Ok(())
            }
            LoopAction::IconDoubleClicked => {
                let was_targeted = self
                    .loops
                    .get(&loop_id)
                    .is_some_and(|model| model.state.targeted);
                for model in self.loops.values_mut() {
                    model.state.targeted = false;
                }
                if !was_targeted {
                    if let Some(model) = self.loops.get_mut(&loop_id) {
                        model.state.selected = false;
                        model.state.targeted = true;
                    }
                }
                self.refresh_selected_audio(backend)?;
                Ok(())
            }
            LoopAction::PlayClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Playing)
            }
            LoopAction::PlayDryClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::PlayingDryThroughWet)
            }
            LoopAction::RecordClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Recording)
            }
            LoopAction::GrabClicked => self.grab_targets(backend, loop_id),
            LoopAction::RerecordClicked => self.rerecord_targets(backend, loop_id),
            LoopAction::StopClicked => {
                self.transition_targets(backend, loop_id, BackendLoopMode::Stopped)
            }
            LoopAction::GainChanged(value) => {
                let value = value.clamp(0.0, 1.0);
                if (loop_model.state.gain - value).abs() <= f32::EPSILON {
                    return Ok(());
                }
                backend
                    .set_loop_gain(loop_model.backend_id, value)
                    .map_err(|error| format!("could not set loop gain: {error}"))?;
                if let Some(model) = self.loops.get_mut(&loop_id) {
                    model.state.gain = value;
                }
                Ok(())
            }
            LoopAction::BalanceChanged(value) => {
                if !loop_model.state.stereo {
                    return Err(format!("loop {loop_id} has no stereo balance"));
                }
                let value = value.clamp(-1.0, 1.0);
                if (loop_model.state.balance - value).abs() <= f32::EPSILON {
                    return Ok(());
                }
                backend
                    .set_loop_balance(loop_model.backend_id, value)
                    .map_err(|error| format!("could not set loop balance: {error}"))?;
                if let Some(model) = self.loops.get_mut(&loop_id) {
                    model.state.balance = value;
                }
                Ok(())
            }
        }
    }

    fn refresh_selected_audio(&mut self, backend: &mut dyn Backend) -> Result<(), String> {
        let selected: Vec<_> = self
            .loops
            .values()
            .filter(|model| model.state.selected)
            .map(|model| (model.id, model.backend_id))
            .collect();
        for model in self.loops.values_mut() {
            if !model.state.selected {
                model.audio_data = None;
            }
        }
        if let [(id, backend_id)] = selected.as_slice() {
            if self
                .loops
                .get(id)
                .is_some_and(|model| model.audio_data.is_some())
            {
                return Ok(());
            }
            let data = backend
                .loop_audio_data(*backend_id)
                .map_err(|error| format!("could not fetch selected loop audio: {error}"))?;
            if let (Some(model), Some(data)) = (self.loops.get_mut(id), data) {
                model.audio_data = Some(data);
            }
        }
        Ok(())
    }

    fn action_target_ids(&self, initiating_loop: LoopId) -> Vec<LoopId> {
        let initiating_selected = self
            .loops
            .get(&initiating_loop)
            .is_some_and(|model| model.state.selected);
        self.loops
            .values()
            .filter(|model| {
                model.id == initiating_loop || initiating_selected && model.state.selected
            })
            .map(|model| model.id)
            .collect()
    }

    fn rerecord_targets(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
    ) -> Result<(), String> {
        let sync_length = self.sync_length();
        let initiating = self
            .loops
            .get(&initiating_loop)
            .ok_or_else(|| format!("stale or unknown loop {initiating_loop}"))?;
        let cycles = if sync_length == 0 {
            1
        } else {
            initiating.length.div_ceil(sync_length).max(1)
        };
        let current_cycle = if sync_length == 0 {
            0
        } else {
            initiating.position / sync_length
        };
        let delay = if self.loops.values().any(|model| model.state.targeted) {
            self.target_delay()
        } else {
            cycles.saturating_sub(current_cycle).saturating_sub(1)
        };
        for id in self.action_target_ids(initiating_loop) {
            let model = self.loops.get(&id).expect("action target exists");
            let previous = backend_loop_mode(model.state.mode);
            backend
                .transition_loop(
                    model.backend_id,
                    BackendLoopMode::RecordingDryIntoWet,
                    Some(delay),
                )
                .map_err(|error| format!("could not start loop re-record {id}: {error}"))?;
            backend
                .transition_loop(
                    model.backend_id,
                    previous,
                    Some(delay.saturating_add(cycles)),
                )
                .map_err(|error| format!("could not finish loop re-record {id}: {error}"))?;
        }
        Ok(())
    }

    fn grab_targets(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
    ) -> Result<(), String> {
        let sync_length = self.sync_length();
        if sync_length == 0 {
            return Err("cannot grab before the sync loop has a length".to_owned());
        }
        let target = self
            .loops
            .values()
            .find(|model| model.state.targeted)
            .map(|model| {
                let cycles = model.length.div_ceil(sync_length).max(1);
                let current = model.position / sync_length;
                (cycles, current)
            });
        let n_cycles = self.global.apply_n_cycles.max(1);
        let ids = if self.global.sync {
            self.action_target_ids(initiating_loop)
        } else {
            vec![initiating_loop]
        };
        let post_mode = if self.global.play_after_record {
            BackendLoopMode::Playing
        } else {
            BackendLoopMode::Unknown
        };
        let requests = ids
            .iter()
            .map(|id| {
                let model = self.loops.get(id).expect("action target exists");
                let (reverse_start_cycle, cycles_length, go_to_cycle, go_to_mode) =
                    if self.global.sync {
                        if let Some((target_cycles, target_current)) = target {
                            (
                                Some((target_current.saturating_add(target_cycles)) as i32),
                                Some(target_cycles as i32),
                                Some(target_current as i32),
                                post_mode,
                            )
                        } else {
                            (
                                Some(n_cycles as i32),
                                Some(n_cycles as i32),
                                Some(0),
                                post_mode,
                            )
                        }
                    } else if let Some((_, target_current)) = target {
                        (
                            None,
                            Some(target_current.saturating_add(1) as i32),
                            Some(target_current as i32),
                            BackendLoopMode::Recording,
                        )
                    } else {
                        (
                            None,
                            Some(n_cycles as i32),
                            Some(n_cycles.saturating_sub(1) as i32),
                            BackendLoopMode::Recording,
                        )
                    };
                BackendGrabRequest {
                    loop_id: model.backend_id,
                    reverse_start_cycle,
                    cycles_length,
                    go_to_cycle,
                    go_to_mode,
                }
            })
            .collect::<Vec<_>>();
        backend
            .grab_loops(&requests)
            .map_err(|error| format!("could not grab loop recording: {error}"))?;

        if !self.global.sync {
            let delay = target.map(|_| self.target_delay()).unwrap_or(0);
            let finish = if self.global.play_after_record {
                BackendLoopMode::Playing
            } else {
                BackendLoopMode::Stopped
            };
            for id in &ids {
                let model = self.loops.get(id).expect("action target exists");
                backend
                    .transition_loop(model.backend_id, finish, Some(delay))
                    .map_err(|error| format!("could not finish loop grab {id}: {error}"))?;
            }
        }
        if self.global.solo {
            let target_tracks: Vec<_> = ids
                .iter()
                .filter_map(|id| self.loops.get(id).map(|model| model.track_id))
                .collect();
            let others: Vec<_> = self
                .loops
                .values()
                .filter(|model| target_tracks.contains(&model.track_id) && !ids.contains(&model.id))
                .map(|model| (model.id, model.backend_id))
                .collect();
            for (id, backend_id) in others {
                backend
                    .transition_loop(backend_id, BackendLoopMode::Stopped, None)
                    .map_err(|error| format!("could not solo-stop loop {id}: {error}"))?;
            }
        }
        Ok(())
    }

    fn sync_length(&self) -> u32 {
        self.tracks
            .iter()
            .find(|track| track.is_sync)
            .and_then(|track| track.loops.first())
            .and_then(|id| self.loops.get(id))
            .map(|model| model.length)
            .unwrap_or(0)
    }

    fn transition_targets(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
        mode: BackendLoopMode,
    ) -> Result<(), String> {
        let initiating_selected = self
            .loops
            .get(&initiating_loop)
            .is_some_and(|model| model.state.selected);
        let targets: Vec<_> = self
            .loops
            .values()
            .filter(|model| {
                model.id == initiating_loop || initiating_selected && model.state.selected
            })
            .map(|model| (model.id, model.track_id, model.backend_id))
            .collect();
        let delay = self.global.sync.then_some(self.target_delay());
        if self.global.solo
            && matches!(
                mode,
                BackendLoopMode::Playing
                    | BackendLoopMode::PlayingDryThroughWet
                    | BackendLoopMode::Recording
            )
        {
            let track_ids: Vec<_> = targets.iter().map(|(_, track_id, _)| *track_id).collect();
            let selected_ids: Vec<_> = targets.iter().map(|(id, _, _)| *id).collect();
            let others: Vec<_> = self
                .loops
                .values()
                .filter(|model| {
                    track_ids.contains(&model.track_id) && !selected_ids.contains(&model.id)
                })
                .map(|model| (model.id, model.backend_id))
                .collect();
            for (id, backend_id) in others {
                backend
                    .transition_loop(backend_id, BackendLoopMode::Stopped, delay)
                    .map_err(|error| format!("could not solo-stop loop {id}: {error}"))?;
            }
        }
        for (id, _, backend_id) in targets {
            backend
                .transition_loop(backend_id, mode, delay)
                .map_err(|error| format!("could not transition loop {id}: {error}"))?;
            if mode == BackendLoopMode::Recording && self.global.apply_n_cycles > 0 {
                let after = delay
                    .unwrap_or(0)
                    .saturating_add(self.global.apply_n_cycles);
                let next = if self.global.play_after_record {
                    BackendLoopMode::Playing
                } else {
                    BackendLoopMode::Stopped
                };
                backend
                    .transition_loop(backend_id, next, Some(after))
                    .map_err(|error| {
                        format!("could not schedule recording end for {id}: {error}")
                    })?;
            }
        }
        Ok(())
    }

    fn target_delay(&self) -> u32 {
        let Some(target) = self.loops.values().find(|model| model.state.targeted) else {
            return 0;
        };
        if let Some(delay) = target.state.next_transition_delay {
            return delay;
        }
        let sync_length = self
            .tracks
            .iter()
            .find(|track| track.is_sync)
            .and_then(|track| track.loops.first())
            .and_then(|id| self.loops.get(id))
            .map(|model| model.length)
            .unwrap_or(0);
        if sync_length == 0 || target.length <= target.position {
            0
        } else {
            (target.length - target.position) / sync_length
        }
    }

    fn set_port_connected(
        &mut self,
        backend: &mut dyn Backend,
        port_id: PortId,
        external_port: String,
        connected: bool,
    ) -> Result<(), String> {
        if external_port.trim().is_empty() {
            let message = "external endpoint name must not be empty".to_owned();
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::EndpointUnavailable,
                message: message.clone(),
            });
            return Err(message);
        }
        let Some(port) = self.connection_ports.get(&port_id) else {
            let message = format!("stale or unknown local port {port_id}");
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::StaleLocalPort,
                message: message.clone(),
            });
            return Err(message);
        };
        let Some((eligible, confirmed_connected)) = port.candidates.get(&external_port).copied()
        else {
            let message = format!("external endpoint disappeared: {external_port}");
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::EndpointUnavailable,
                message: message.clone(),
            });
            return Err(message);
        };
        if !eligible {
            let message = format!("external endpoint is incompatible: {external_port}");
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::Incompatible,
                message: message.clone(),
            });
            return Err(message);
        }
        let key = (port_id, external_port.clone());
        if self
            .pending_connections
            .get(&key)
            .is_some_and(|pending| pending.desired_connected == connected)
            || confirmed_connected == connected && !self.pending_connections.contains_key(&key)
        {
            return Ok(());
        }
        let backend_id = port.backend_id;
        if let Err(error) = backend.set_port_connected(backend_id, &external_port, connected) {
            let message = format!("connection request rejected: {error}");
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::BackendRejected,
                message: message.clone(),
            });
            return Err(message);
        }
        self.connection_errors.retain(|error| {
            error.port_id != Some(port_id)
                || error.external_port.as_deref() != Some(external_port.as_str())
        });
        self.pending_connections.insert(
            key,
            PendingConnection {
                desired_connected: connected,
                age: Duration::ZERO,
            },
        );
        Ok(())
    }

    fn age_pending_connections(&mut self, elapsed: Duration) {
        let timed_out: Vec<_> = self
            .pending_connections
            .iter_mut()
            .filter_map(|(key, pending)| {
                pending.age = pending.age.saturating_add(elapsed);
                (pending.age >= CONNECTION_TIMEOUT).then(|| key.clone())
            })
            .collect();
        for (port_id, external_port) in timed_out {
            self.pending_connections
                .remove(&(port_id, external_port.clone()));
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port.clone()),
                kind: ConnectionErrorKind::TimedOut,
                message: format!("connection request timed out: {external_port}"),
            });
        }
    }

    fn handle_global_action(
        &mut self,
        backend: &mut dyn Backend,
        action: GlobalControlAction,
    ) -> Result<(), String> {
        match action {
            GlobalControlAction::StopAll => {
                let targets: Vec<_> = self.loops.values().map(|model| model.backend_id).collect();
                for target in targets {
                    backend
                        .transition_loop(
                            target,
                            BackendLoopMode::Stopped,
                            self.global.sync.then_some(0),
                        )
                        .map_err(|error| format!("could not stop loop: {error}"))?;
                }
            }
            GlobalControlAction::DeselectAll => {
                for model in self.loops.values_mut() {
                    model.state.selected = false;
                    model.audio_data = None;
                }
            }
            GlobalControlAction::ClearRecordings { include_sync }
            | GlobalControlAction::ClearAll { include_sync } => {
                let targets: Vec<_> = self
                    .loops
                    .values()
                    .filter(|model| {
                        include_sync
                            || !self
                                .tracks
                                .iter()
                                .find(|track| track.id == model.track_id)
                                .is_some_and(|track| track.is_sync)
                    })
                    .map(|model| model.backend_id)
                    .collect();
                for target in targets {
                    backend
                        .clear_loop(target)
                        .map_err(|error| format!("could not clear loop: {error}"))?;
                }
            }
            GlobalControlAction::SetDefaultRecordingAction(value) => {
                self.global.default_recording_action = value;
            }
            GlobalControlAction::SetPlayAfterRecord(value) => {
                self.global.play_after_record = value;
            }
            GlobalControlAction::SetSync(value) => self.global.sync = value,
            GlobalControlAction::SetSolo(value) => self.global.solo = value,
            GlobalControlAction::SetApplyNCycles(value) => self.global.apply_n_cycles = value,
        }
        Ok(())
    }

    fn apply_backend_snapshot(&mut self, snapshot: BackendSnapshot) {
        self.status.dsp_load_percent = snapshot.status.dsp_load_percent;
        self.status.xruns = self.status.xruns.saturating_add(snapshot.status.xruns);
        self.status.buffer_size = snapshot.status.buffer_size;
        self.status.sample_rate = snapshot.status.sample_rate;
        self.status.audio_driver = match snapshot.status.driver_state {
            shoop_backend::BackendDriverState::Dummy => AudioDriverState::Dummy,
            shoop_backend::BackendDriverState::AwaitingGesture => AudioDriverState::AwaitingGesture,
            shoop_backend::BackendDriverState::RequestingPermission => {
                AudioDriverState::RequestingPermission
            }
            shoop_backend::BackendDriverState::Starting => AudioDriverState::Starting,
            shoop_backend::BackendDriverState::Running => AudioDriverState::Running,
            shoop_backend::BackendDriverState::Suspended => AudioDriverState::Suspended,
            shoop_backend::BackendDriverState::Denied => AudioDriverState::Denied,
            shoop_backend::BackendDriverState::Unsupported => AudioDriverState::Unsupported,
            shoop_backend::BackendDriverState::Failed => AudioDriverState::Failed,
            shoop_backend::BackendDriverState::Stopped => AudioDriverState::Stopped,
        };
        self.status.callback_count = snapshot.status.callback_count;
        self.status.processed_frames = snapshot.status.processed_frames;
        self.status.input_peak = snapshot.status.input_peak;
        self.status.output_peak = snapshot.status.output_peak;
        self.status.callback_budget_overruns = snapshot.status.callback_budget_overruns;
        self.status.render_discontinuities = snapshot.status.render_discontinuities;
        self.status.memory_growths = snapshot.status.memory_growths;
        self.status.command_overflows = snapshot.status.command_overflows;
        self.status.storage_low_channels = snapshot.status.storage_low_channels;
        self.status.storage_exhaustions = snapshot.status.storage_exhaustions;
        for track in &mut self.tracks {
            let Some(backend_state) = snapshot.tracks.get(&track.backend_id) else {
                continue;
            };
            let controls = &mut track.controls;
            controls.has_output = backend_state.audio_channels > 0 || backend_state.midi;
            controls.has_output_audio = backend_state.audio_channels > 0;
            controls.output_stereo = backend_state.audio_channels == 2;
            controls.output_gain_db = backend_state.output_gain_db;
            controls.output_balance = backend_state.output_balance;
            controls.output_muted = backend_state.output_muted;
            controls.output_peak_left_db = backend_state
                .output_peaks
                .first()
                .copied()
                .unwrap_or(-200.0);
            controls.output_peak_right_db = backend_state
                .output_peaks
                .get(1)
                .copied()
                .unwrap_or(controls.output_peak_left_db);
            controls.output_midi_activity = backend_state.output_midi_activity;
            controls.has_input = backend_state.audio_channels > 0 || backend_state.midi;
            controls.has_input_audio = backend_state.audio_channels > 0;
            controls.input_stereo = backend_state.audio_channels == 2;
            controls.input_gain_db = backend_state.input_gain_db;
            controls.input_balance = backend_state.input_balance;
            controls.input_monitoring = backend_state.input_monitoring;
            controls.input_peak_left_db =
                backend_state.input_peaks.first().copied().unwrap_or(-200.0);
            controls.input_peak_right_db = backend_state
                .input_peaks
                .get(1)
                .copied()
                .unwrap_or(controls.input_peak_left_db);
            controls.input_midi_activity = backend_state.input_midi_activity;
            controls.clamp();
        }
        for model in self.loops.values_mut() {
            let Some(backend_state) = snapshot.loops.get(&model.backend_id) else {
                continue;
            };
            model.length = backend_state.length;
            model.position = backend_state.position;
            model.state.mode = app_loop_mode(backend_state.mode);
            model.state.next_mode = backend_state
                .next_mode
                .map(app_loop_mode)
                .unwrap_or(model.state.mode);
            model.state.next_transition_delay = backend_state.next_transition_delay;
            model.state.empty = backend_state.length == 0;
            model.state.position = if backend_state.length == 0 {
                0.0
            } else {
                backend_state.position as f32 / backend_state.length as f32
            };
            model.state.stereo = backend_state.stereo;
            model.state.gain = backend_state.gain;
            model.state.balance = backend_state.balance;
            model.state.peak_left_db = backend_state.audio_peaks.first().copied().unwrap_or(-200.0);
            model.state.peak_right_db = backend_state
                .audio_peaks
                .get(1)
                .copied()
                .unwrap_or(model.state.peak_left_db);
            model.state.midi_activity = backend_state.midi_activity;
        }
        self.apply_connection_snapshot(snapshot.connections);
    }

    fn apply_connection_snapshot(&mut self, snapshot: BackendConnectionSnapshot) {
        self.connection_backend_available = snapshot.available;
        for failure in snapshot.failures {
            let Some(port_id) = self
                .connection_ports
                .values()
                .find(|port| port.backend_id == failure.port_id)
                .map(|port| port.id)
            else {
                continue;
            };
            self.pending_connections
                .remove(&(port_id, failure.external_port.clone()));
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(failure.external_port.clone()),
                kind: ConnectionErrorKind::BackendRejected,
                message: failure.message,
            });
        }
        for port in self.connection_ports.values_mut() {
            let Some(observed) = snapshot.ports.get(&port.backend_id) else {
                port.candidates.clear();
                continue;
            };
            port.candidates = observed
                .candidates
                .iter()
                .map(|candidate| {
                    (
                        candidate.full_name.clone(),
                        (candidate.eligible, candidate.connected),
                    )
                })
                .collect();
        }
        let pending_keys: Vec<_> = self.pending_connections.keys().cloned().collect();
        for (port_id, external_port) in pending_keys {
            let key = (port_id, external_port.clone());
            let observed = self
                .connection_ports
                .get(&port_id)
                .and_then(|port| port.candidates.get(&external_port))
                .copied();
            let desired = self.pending_connections[&key].desired_connected;
            match observed {
                Some((true, connected)) if connected == desired => {
                    self.pending_connections.remove(&key);
                }
                None => {
                    self.pending_connections.remove(&key);
                    self.push_connection_error(ConnectionErrorState {
                        port_id: Some(port_id),
                        external_port: Some(external_port.clone()),
                        kind: ConnectionErrorKind::EndpointUnavailable,
                        message: format!("external endpoint disappeared: {external_port}"),
                    });
                }
                _ => {}
            }
        }
        self.rebuild_connection_view();
    }

    fn rebuild_connection_view(&mut self) {
        let ports: Arc<[LocalPortConnectionState]> = self
            .connection_ports
            .values()
            .map(|port| {
                let candidates = port
                    .candidates
                    .iter()
                    .map(|(full_name, (eligible, connected))| {
                        let pending = self
                            .pending_connections
                            .get(&(port.id, full_name.clone()))
                            .map(|pending| pending.desired_connected);
                        let error = self
                            .connection_errors
                            .iter()
                            .rev()
                            .find(|error| {
                                error.port_id == Some(port.id)
                                    && error.external_port.as_deref() == Some(full_name.as_str())
                            })
                            .map(|error| error.message.clone());
                        ExternalPortConnectionState {
                            full_name: full_name.clone(),
                            eligible: *eligible,
                            connected: *connected,
                            pending,
                            error,
                        }
                    })
                    .collect::<Vec<_>>()
                    .into();
                LocalPortConnectionState {
                    id: port.id,
                    track_id: port.track_id,
                    name: port.name.clone(),
                    data_type: port.data_type,
                    direction: port.direction,
                    role: port.role,
                    candidates,
                }
            })
            .collect::<Vec<_>>()
            .into();
        let errors: Arc<[ConnectionErrorState]> = self.connection_errors.clone().into();
        let changed = self.connection_view.loading
            || self.connection_view.backend_available != self.connection_backend_available
            || self.connection_view.ports.as_ref() != ports.as_ref()
            || self.connection_view.errors.as_ref() != errors.as_ref();
        if changed {
            self.connection_revision = self.connection_revision.wrapping_add(1);
            self.connection_view = Arc::new(ConnectionViewState {
                revision: self.connection_revision,
                loading: false,
                backend_available: self.connection_backend_available,
                ports,
                errors,
            });
        }
    }

    fn report_connection_saturation(&mut self, port_id: PortId, external_port: String) {
        let message = format!("connection command queue is full: {external_port}");
        self.push_connection_error(ConnectionErrorState {
            port_id: Some(port_id),
            external_port: Some(external_port),
            kind: ConnectionErrorKind::CommandSaturated,
            message: message.clone(),
        });
        self.notify_error(message);
    }

    fn push_connection_error(&mut self, error: ConnectionErrorState) {
        self.connection_errors.push(error);
        const MAX_CONNECTION_ERRORS: usize = 16;
        if self.connection_errors.len() > MAX_CONNECTION_ERRORS {
            self.connection_errors
                .drain(..self.connection_errors.len() - MAX_CONNECTION_ERRORS);
        }
        self.rebuild_connection_view();
    }

    fn snapshot(&self) -> AppSnapshot {
        AppSnapshot {
            revision: self.revision,
            tracks: self
                .tracks
                .iter()
                .map(|track| TrackState {
                    id: track.id,
                    name: track.name.clone(),
                    is_sync: track.is_sync,
                    loops: track
                        .loops
                        .iter()
                        .filter_map(|id| self.loops.get(id))
                        .map(|model| {
                            let mut state = model.state.clone();
                            state.name.clone_from(&model.name);
                            state
                        })
                        .collect(),
                    controls: track.controls.clone(),
                    port_ids: Arc::clone(&track.port_ids),
                })
                .collect(),
            global_controls: self.global.clone(),
            status: self.status.clone(),
            details: self.details_snapshot(),
            connections: Arc::clone(&self.connection_view),
            notifications: self.notifications.clone(),
        }
    }

    fn details_snapshot(&self) -> Option<LoopDetailsState> {
        let mut selected = self.loops.values().filter(|model| model.state.selected);
        let model = selected.next()?;
        if selected.next().is_some() {
            return None;
        }
        let channels = model
            .audio_data
            .as_ref()
            .map(|channels| {
                channels
                    .iter()
                    .enumerate()
                    .map(|(index, samples)| WaveformChannelState {
                        id: ChannelId::from_raw((model.id.raw() << 8) | index as u64 + 1),
                        label: format!("audio {}", index + 1),
                        samples: Arc::clone(samples),
                        start_offset: 0,
                        loop_length: model.length as u64,
                        played_sample: matches!(model.state.mode, LoopMode::Playing)
                            .then_some(model.position as i64),
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(LoopDetailsState {
            generation: self.revision,
            loop_id: model.id,
            title: model.name.clone(),
            loading: model.audio_data.is_none(),
            channels,
        })
    }

    fn notify_error(&mut self, message: String) {
        self.notifications.push(AppNotification {
            level: NotificationLevel::Error,
            message,
        });
        const MAX_NOTIFICATIONS: usize = 32;
        if self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications
                .drain(..self.notifications.len() - MAX_NOTIFICATIONS);
        }
    }
}

fn register_backend_ports(
    track_id: TrackId,
    descriptors: &[BackendPortDescriptor],
    next_port_id: &mut u64,
    ports: &mut BTreeMap<PortId, ConnectionPortModel>,
) -> Arc<[PortId]> {
    let mut ids = Vec::with_capacity(descriptors.len());
    for descriptor in descriptors {
        let id = PortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.insert(
            id,
            ConnectionPortModel {
                id,
                backend_id: descriptor.id,
                track_id,
                name: descriptor.name.clone(),
                data_type: match descriptor.data_type {
                    BackendPortDataType::Audio => PortDataType::Audio,
                    BackendPortDataType::Midi => PortDataType::Midi,
                },
                direction: match descriptor.direction {
                    BackendPortDirection::Input => PortDirection::Input,
                    BackendPortDirection::Output => PortDirection::Output,
                },
                role: match descriptor.role {
                    BackendPortRole::AudioInput => PortRole::AudioInput,
                    BackendPortRole::AudioOutput => PortRole::AudioOutput,
                    BackendPortRole::AudioSend => PortRole::AudioSend,
                    BackendPortRole::AudioReturn => PortRole::AudioReturn,
                    BackendPortRole::MidiInput => PortRole::MidiInput,
                    BackendPortRole::MidiOutput => PortRole::MidiOutput,
                    BackendPortRole::MidiSend => PortRole::MidiSend,
                },
                candidates: BTreeMap::new(),
            },
        );
        ids.push(id);
    }
    ids.into()
}

fn backend_loop_mode(mode: LoopMode) -> BackendLoopMode {
    match mode {
        LoopMode::Unknown => BackendLoopMode::Unknown,
        LoopMode::Stopped => BackendLoopMode::Stopped,
        LoopMode::Playing => BackendLoopMode::Playing,
        LoopMode::Recording => BackendLoopMode::Recording,
        LoopMode::Replacing => BackendLoopMode::Replacing,
        LoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}

fn app_loop_mode(mode: BackendLoopMode) -> LoopMode {
    match mode {
        BackendLoopMode::Unknown => LoopMode::Unknown,
        BackendLoopMode::Stopped => LoopMode::Stopped,
        BackendLoopMode::Playing => LoopMode::Playing,
        BackendLoopMode::Recording => LoopMode::Recording,
        BackendLoopMode::Replacing => LoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => LoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => LoopMode::RecordingDryIntoWet,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use shoop_app_api::{SelectionModifiers, TrackAction};
    use shoop_backend::{BackendPortDataType, BackendPortDirection, EngineBackend, FakeBackend};

    use super::*;

    fn wait_for(
        handle: &ApplicationHandle,
        predicate: impl Fn(&AppSnapshot) -> bool,
    ) -> Arc<AppSnapshot> {
        let started = Instant::now();
        loop {
            let snapshot = handle.snapshot();
            if predicate(&snapshot) {
                return snapshot;
            }
            assert!(started.elapsed() < Duration::from_secs(2));
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn actor_initializes_a_distinct_sync_track() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let snapshot = runtime.handle().snapshot();
        assert_eq!(snapshot.tracks.len(), 1);
        assert!(snapshot.tracks[0].is_sync);
        assert_eq!(snapshot.tracks[0].loops.len(), 1);
        assert!(snapshot.tracks[0].loops[0].sync);
        assert!(snapshot.tracks[0].id.is_valid());
        assert!(snapshot.tracks[0].loops[0].id.is_valid());
    }

    #[test]
    fn actor_applies_intents_and_publishes_backend_state() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let snapshot = handle.snapshot();
        let track = &snapshot.tracks[0];
        let loop_state = &track.loops[0];
        handle
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: loop_state.id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        let updated = wait_for(&handle, |snapshot| {
            snapshot.tracks[0].loops[0].mode == LoopMode::Playing
        });
        assert!(updated.revision > snapshot.revision);
    }

    #[test]
    fn actor_rejects_stale_and_mismatched_ids_observably() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::Loop {
                track_id: TrackId::from_raw(900),
                loop_id: LoopId::from_raw(901),
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| !snapshot.notifications.is_empty());
        assert!(snapshot.notifications[0]
            .message
            .contains("stale or unknown"));

        handle
            .dispatch(AppIntent::Track {
                track_id: TrackId::from_raw(902),
                action: TrackAction::NameChanged("nope".to_owned()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.notifications.len() >= 2);
        assert!(snapshot.notifications[1]
            .message
            .contains("stale or unknown track"));
    }

    #[test]
    fn failed_track_creation_is_observable_and_not_partially_published() {
        let mut backend = FakeBackend::default();
        backend.fail_track_creation_after(1);
        let runtime = ApplicationRuntime::start(Box::new(backend)).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Will fail".to_owned(),
                audio_channels: 2,
                midi: false,
            }))
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| !snapshot.notifications.is_empty());
        assert_eq!(snapshot.tracks.len(), 1);
        assert!(snapshot
            .connections
            .ports
            .iter()
            .all(|port| port.track_id == snapshot.tracks[0].id));
        assert!(snapshot.notifications[0]
            .message
            .contains("injected track creation failure"));
    }

    #[test]
    fn direct_track_creation_and_aligned_rows_are_published() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        for (name, audio_channels, midi) in [
            ("Stereo", 2, true),
            ("Mono", 1, false),
            ("MIDI", 0, true),
            ("Silent", 0, false),
        ] {
            handle
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi,
                }))
                .unwrap();
        }
        let snapshot = wait_for(&handle, |snapshot| snapshot.tracks.len() == 5);
        assert!(snapshot.tracks[1..]
            .iter()
            .all(|track| track.loops.len() == 8));
        assert!(snapshot.tracks[1].controls.output_stereo);
        assert!(!snapshot.tracks[4].controls.has_output);

        let first = snapshot.tracks[1].id;
        handle
            .dispatch(AppIntent::AddLoop { track_id: first })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.tracks[1].loops.len() == 9);
        assert!(snapshot.tracks[1..]
            .iter()
            .all(|track| track.loops.len() == 9));
    }

    #[test]
    fn controls_selection_details_solo_and_fixed_recording_are_functional() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Track".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.tracks.len() == 2);
        let track_id = snapshot.tracks[1].id;
        let first = snapshot.tracks[1].loops[0].id;
        let second = snapshot.tracks[1].loops[1].id;
        handle
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::OutputGainChanged(-6.0),
            })
            .unwrap();
        handle
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id: first,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].controls.output_gain_db == -6.0
                && snapshot
                    .details
                    .as_ref()
                    .is_some_and(|details| details.loop_id == first)
        });
        assert_eq!(snapshot.details.as_ref().unwrap().channels.len(), 2);

        handle
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id: first,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot.tracks[1].loops[0].mode == LoopMode::Playing
        });
        handle
            .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)))
            .unwrap();
        handle
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(2)))
            .unwrap();
        handle
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id: second,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].loops[0].mode == LoopMode::Stopped
                && snapshot.tracks[1].loops[1].mode == LoopMode::Playing
        });
        assert!(snapshot.global_controls.solo);
        assert_eq!(snapshot.global_controls.apply_n_cycles, 2);
    }

    #[test]
    fn target_delay_is_derived_from_target_and_sync_lengths() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend).unwrap();
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Track".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 100;
        let target = model.tracks[1].loops[0];
        let initiating = model.tracks[1].loops[1];
        let target_model = model.loops.get_mut(&target).unwrap();
        target_model.state.targeted = true;
        target_model.length = 400;
        target_model.position = 100;
        model
            .transition_targets(&mut backend, initiating, BackendLoopMode::Playing)
            .unwrap();
        assert!(backend.operations().iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::Transition(
                id,
                BackendLoopMode::Playing,
                Some(3)
            ) if *id == model.loops[&initiating].backend_id
        )));
    }

    #[test]
    fn expanded_loop_actions_route_qml_equivalent_modes_grab_and_balance() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend).unwrap();
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Stereo".to_owned(),
                    audio_channels: 2,
                    midi: false,
                },
            )
            .unwrap();
        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 100;
        let loop_id = model.tracks[1].loops[0];
        let loop_model = model.loops.get_mut(&loop_id).unwrap();
        loop_model.length = 200;
        loop_model.position = 50;
        loop_model.state.mode = LoopMode::Playing;
        loop_model.state.stereo = true;

        model
            .handle_loop_action(
                &mut backend,
                model.tracks[1].id,
                loop_id,
                LoopAction::PlayDryClicked,
            )
            .unwrap();
        model
            .handle_loop_action(
                &mut backend,
                model.tracks[1].id,
                loop_id,
                LoopAction::BalanceChanged(0.5),
            )
            .unwrap();
        model
            .handle_loop_action(
                &mut backend,
                model.tracks[1].id,
                loop_id,
                LoopAction::RerecordClicked,
            )
            .unwrap();
        model
            .handle_loop_action(
                &mut backend,
                model.tracks[1].id,
                loop_id,
                LoopAction::GrabClicked,
            )
            .unwrap();

        assert!(backend.operations().iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::Transition(
                id,
                BackendLoopMode::PlayingDryThroughWet,
                Some(0)
            ) if *id == model.loops[&loop_id].backend_id
        )));
        assert!(backend.operations().iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::SetLoopBalance(id, balance)
                if *id == model.loops[&loop_id].backend_id && *balance == 0.5
        )));
        assert!(backend.operations().iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::Transition(
                id,
                BackendLoopMode::RecordingDryIntoWet,
                Some(1)
            ) if *id == model.loops[&loop_id].backend_id
        )));
        assert!(backend.operations().iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::GrabLoops(requests)
                if requests.len() == 1
                    && requests[0].loop_id == model.loops[&loop_id].backend_id
                    && requests[0].reverse_start_cycle == Some(1)
                    && requests[0].cycles_length == Some(1)
                    && requests[0].go_to_mode == BackendLoopMode::Playing
        )));
    }

    #[test]
    fn grab_policy_covers_targeted_selection_solo_and_immediate_completion() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend).unwrap();
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Track".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let track_id = model.tracks[1].id;
        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 100;
        let initiating = model.tracks[1].loops[0];
        let selected = model.tracks[1].loops[1];
        let target = model.tracks[1].loops[2];
        model.loops.get_mut(&initiating).unwrap().state.selected = true;
        model.loops.get_mut(&selected).unwrap().state.selected = true;
        let target_model = model.loops.get_mut(&target).unwrap();
        target_model.state.targeted = true;
        target_model.length = 300;
        target_model.position = 100;
        model.global.apply_n_cycles = 2;
        model.global.play_after_record = false;
        model.global.solo = true;

        let before = backend.operations().len();
        model
            .handle_loop_action(&mut backend, track_id, initiating, LoopAction::GrabClicked)
            .unwrap();
        let operations = &backend.operations()[before..];
        let requests = operations
            .iter()
            .find_map(|operation| match operation {
                shoop_backend::FakeOperation::GrabLoops(requests) => Some(requests),
                _ => None,
            })
            .unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| {
            request.reverse_start_cycle == Some(4)
                && request.cycles_length == Some(3)
                && request.go_to_cycle == Some(1)
                && request.go_to_mode == BackendLoopMode::Unknown
        }));
        assert!(operations.iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::Transition(_, BackendLoopMode::Stopped, None)
        )));

        model.global.sync = false;
        model.global.play_after_record = true;
        let before = backend.operations().len();
        model
            .handle_loop_action(&mut backend, track_id, initiating, LoopAction::GrabClicked)
            .unwrap();
        let operations = &backend.operations()[before..];
        let requests = operations
            .iter()
            .find_map(|operation| match operation {
                shoop_backend::FakeOperation::GrabLoops(requests) => Some(requests),
                _ => None,
            })
            .unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].reverse_start_cycle, None);
        assert_eq!(requests[0].cycles_length, Some(2));
        assert_eq!(requests[0].go_to_cycle, Some(1));
        assert_eq!(requests[0].go_to_mode, BackendLoopMode::Recording);
        assert!(operations.iter().any(|operation| matches!(
            operation,
            shoop_backend::FakeOperation::Transition(_, BackendLoopMode::Playing, Some(2))
        )));
    }

    #[test]
    fn snapshot_reads_are_independent_of_actor_progress() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let held = handle.snapshot();
        handle
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        let updated = wait_for(&handle, |snapshot| !snapshot.global_controls.sync);
        assert!(held.global_controls.sync);
        assert!(!updated.global_controls.sync);
    }

    #[test]
    fn actor_publishes_owned_ports_and_serializes_connection_churn_and_failure() {
        let backend = FakeBackend::default();
        let control = backend.connection_control();
        let runtime = ApplicationRuntime::start(Box::new(backend)).unwrap();
        let handle = runtime.handle();
        let initial = wait_for(&handle, |snapshot| {
            !snapshot.connections.loading && !snapshot.connections.ports.is_empty()
        });
        assert!(initial.connections.backend_available);
        assert!(initial.connections.ports.iter().all(|port| {
            port.track_id == initial.tracks[0].id && initial.tracks[0].port_ids.contains(&port.id)
        }));

        handle
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Connections".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.tracks.len() == 2
                && snapshot
                    .connections
                    .ports
                    .iter()
                    .filter(|port| port.track_id == snapshot.tracks[1].id)
                    .count()
                    == 6
        });
        let track = &snapshot.tracks[1];
        assert_eq!(track.port_ids.len(), 6);
        let input = snapshot
            .connections
            .ports
            .iter()
            .find(|port| port.track_id == track.id && port.role == PortRole::AudioInput)
            .unwrap();
        let input_id = input.id;
        let input_name = input.name.clone();
        assert!(input
            .candidates
            .iter()
            .any(|candidate| candidate.full_name == "system:capture_1"));
        assert!(!input
            .candidates
            .iter()
            .any(|candidate| candidate.full_name == "system:playback_1"));

        control.defer_mutations(true);
        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: input_id,
                external_port: "system:capture_1".to_owned(),
                connected: true,
            })
            .unwrap();
        let pending = wait_for(&handle, |snapshot| {
            snapshot
                .connections
                .ports
                .iter()
                .find(|port| port.id == input_id)
                .and_then(|port| {
                    port.candidates
                        .iter()
                        .find(|candidate| candidate.full_name == "system:capture_1")
                })
                .is_some_and(|candidate| candidate.pending == Some(true))
        });
        let held_revision = pending.connections.revision;
        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: input_id,
                external_port: "system:capture_1".to_owned(),
                connected: true,
            })
            .unwrap();
        thread::sleep(Duration::from_millis(20));
        assert_eq!(control.pending_len(), 1);
        control.complete_pending(false);
        let failed = wait_for(&handle, |snapshot| {
            snapshot.connections.revision > held_revision
                && snapshot.connections.errors.iter().any(|error| {
                    error.port_id == Some(input_id)
                        && error.kind == ConnectionErrorKind::BackendRejected
                })
        });
        assert!(
            !failed
                .connections
                .ports
                .iter()
                .find(|port| port.id == input_id)
                .unwrap()
                .candidates
                .iter()
                .find(|candidate| candidate.full_name == "system:capture_1")
                .unwrap()
                .connected
        );

        control.defer_mutations(false);
        control.add_external_port(
            "new-client:audio_source",
            BackendPortDirection::Output,
            BackendPortDataType::Audio,
        );
        wait_for(&handle, |snapshot| {
            snapshot
                .connections
                .ports
                .iter()
                .find(|port| port.id == input_id)
                .is_some_and(|port| {
                    port.candidates
                        .iter()
                        .any(|candidate| candidate.full_name == "new-client:audio_source")
                })
        });
        let backend_port = control.port_id_by_name(&input_name).unwrap();
        control.externally_set_connected(backend_port, "new-client:audio_source", true);
        wait_for(&handle, |snapshot| {
            snapshot
                .connections
                .ports
                .iter()
                .find(|port| port.id == input_id)
                .and_then(|port| {
                    port.candidates
                        .iter()
                        .find(|candidate| candidate.full_name == "new-client:audio_source")
                })
                .is_some_and(|candidate| candidate.connected)
        });
        control.remove_external_port("new-client:audio_source");
        wait_for(&handle, |snapshot| {
            snapshot
                .connections
                .ports
                .iter()
                .find(|port| port.id == input_id)
                .is_some_and(|port| {
                    !port
                        .candidates
                        .iter()
                        .any(|candidate| candidate.full_name == "new-client:audio_source")
                })
        });

        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: PortId::from_raw(999_999),
                external_port: "system:capture_1".to_owned(),
                connected: true,
            })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot.connections.errors.iter().any(|error| {
                error.port_id == Some(PortId::from_raw(999_999))
                    && error.kind == ConnectionErrorKind::StaleLocalPort
            })
        });
    }

    #[test]
    fn cooperative_connection_timeout_retains_confirmed_truth() {
        let backend = FakeBackend::default();
        let control = backend.connection_control();
        control.defer_mutations(true);
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        let port = snapshot
            .connections
            .ports
            .iter()
            .find(|port| port.role == PortRole::AudioInput)
            .unwrap();
        let port_id = port.id;
        runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id,
                external_port: "system:capture_1".to_owned(),
                connected: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime
                .snapshot()
                .connections
                .ports
                .iter()
                .find(|port| port.id == port_id)
                .unwrap()
                .candidates
                .iter()
                .find(|candidate| candidate.full_name == "system:capture_1")
                .unwrap()
                .pending,
            Some(true)
        );
        runtime.tick(CONNECTION_TIMEOUT);
        let timed_out = runtime.snapshot();
        assert!(timed_out.connections.errors.iter().any(|error| {
            error.port_id == Some(port_id) && error.kind == ConnectionErrorKind::TimedOut
        }));
        let candidate = timed_out
            .connections
            .ports
            .iter()
            .find(|port| port.id == port_id)
            .unwrap()
            .candidates
            .iter()
            .find(|candidate| candidate.full_name == "system:capture_1")
            .unwrap();
        assert!(!candidate.connected);
        assert_eq!(candidate.pending, None);
    }

    #[test]
    fn unchanged_connection_views_are_structurally_shared_across_polls() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        let first = runtime.snapshot();
        runtime.tick(Duration::from_millis(16));
        let second = runtime.snapshot();
        assert!(Arc::ptr_eq(&first.connections, &second.connections));
        assert!(Arc::ptr_eq(
            &first.connections.ports,
            &second.connections.ports
        ));
    }

    #[test]
    fn cooperative_runtime_drives_the_engine_backed_dummy_workflow() {
        let backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Browser".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.tracks.len(), 2);
        let track_id = snapshot.tracks[1].id;
        let loop_id = snapshot.tracks[1].loops[0].id;

        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(20));
        let recording = runtime.snapshot();
        assert_eq!(recording.tracks[1].loops[0].mode, LoopMode::Recording);
        assert!(recording.details.is_some());
        assert!(recording.details.as_ref().unwrap().channels.len() == 2);

        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(6));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().details.as_ref().is_some_and(|details| {
            details
                .channels
                .first()
                .is_some_and(|channel| !channel.samples.is_empty())
        }));

        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(6));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
    }

    #[test]
    fn cooperative_runtime_bounds_command_work_and_reports_capacity() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        for _ in 0..COMMAND_CAPACITY {
            runtime
                .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
                .unwrap();
        }
        assert_eq!(
            runtime.dispatch(AppIntent::Global(GlobalControlAction::SetSync(true))),
            Err(DispatchError::Full)
        );
        assert_eq!(
            runtime.dispatch(AppIntent::SetPortConnected {
                port_id: PortId::from_raw(77),
                external_port: "device:port".to_owned(),
                connected: true,
            }),
            Err(DispatchError::Full)
        );
        assert!(runtime.snapshot().connections.errors.iter().any(|error| {
            error.port_id == Some(PortId::from_raw(77))
                && error.kind == ConnectionErrorKind::CommandSaturated
        }));
        runtime.tick(Duration::ZERO);
        assert!(runtime.has_pending_commands());
        for _ in 0..COMMAND_CAPACITY / MAX_COOPERATIVE_COMMANDS_PER_TICK {
            runtime.tick(Duration::ZERO);
        }
        assert!(!runtime.has_pending_commands());
    }
}
