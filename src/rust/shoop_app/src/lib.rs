use std::collections::{BTreeMap, VecDeque};
use std::fmt;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::Result;
use shoop_app_api::{
    AppIntent, AppNotification, AppSnapshot, AudioChannelMappingState, AudioChannelSelectionState,
    AudioDriverState, ChannelId, ConnectionErrorKind, ConnectionErrorState, ConnectionViewState,
    DirectTrackSpec, ExternalPortConnectionState, GlobalControlAction, IoTaskKind, IoTaskState,
    IoTaskStatus, LocalPortConnectionState, LoopAction, LoopAudioExportFormat, LoopDetailsState,
    LoopId, LoopMode, LoopState, NotificationLevel, PortDataType, PortDirection, PortId, PortRole,
    SampleRateWarning, StatusState, TaskId, TrackAction, TrackControlState, TrackId, TrackState,
    WaveformChannelState,
};
use shoop_backend::{
    Backend, BackendAudioContent, BackendConnectionSnapshot, BackendGrabRequest,
    BackendLoopContent, BackendLoopId, BackendLoopMode, BackendMidiContent, BackendMidiEvent,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortRole, BackendSessionData, BackendSessionPort, BackendSessionReplacement,
    BackendSessionTrack, BackendSnapshot, BackendTrackControl, BackendTrackId, BackendTrackState,
    DirectTrackRequest,
};
use shoop_session::{
    decode_exact_midi, decode_loop_audio, decode_session, decode_standard_midi, decode_wav,
    encode_exact_midi, encode_float_wav, encode_loop_audio, encode_session, encode_standard_midi,
    resample_exact_midi, resample_loop_audio, resample_session, AudioPayload, ChannelDocument,
    ChannelModeDocument, ConnectabilityDocument, DataTypeDocument, ExactMidi, ExactMidiEvent,
    GlobalControlsDocument, LoopAudio, LoopAudioChannel, LoopDocument, MediaPayload,
    MidiControlDocument, PortDirectionDocument, PortDocument, PortRoleDocument,
    RecordingActionDocument, SessionBundle, SessionDocument, TrackControlsDocument, TrackDocument,
    TrackGroupDocument, TrackTopologyDocument,
};

const COMMAND_CAPACITY: usize = 1024;
const MAX_COOPERATIVE_COMMANDS_PER_TICK: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Clone, Debug)]
pub struct ApplicationFileOutput {
    pub task_id: TaskId,
    pub suggested_name: String,
    pub mime_type: String,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone)]
pub struct ApplicationHandle {
    sender: SyncSender<ApplicationMessage>,
    snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
    saturated_connection: Arc<Mutex<Option<(PortId, String)>>>,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
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

    pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
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
        let file_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let model = ApplicationModel::initialize(&mut *backend, Arc::clone(&file_outputs), true)?;
        let initial = Arc::new(model.snapshot());
        let snapshot = Arc::new(RwLock::new(initial));
        let (sender, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let saturated_connection = Arc::new(Mutex::new(None));
        let handle = ApplicationHandle {
            sender,
            snapshot: Arc::clone(&snapshot),
            saturated_connection: Arc::clone(&saturated_connection),
            file_outputs,
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
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
}

impl CooperativeApplicationRuntime {
    pub fn start(mut backend: Box<dyn Backend>) -> Result<Self> {
        let file_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let model = ApplicationModel::initialize(&mut *backend, Arc::clone(&file_outputs), false)?;
        let snapshot = Arc::new(model.snapshot());
        Ok(Self {
            model,
            backend,
            commands: VecDeque::with_capacity(COMMAND_CAPACITY),
            snapshot,
            file_outputs,
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

    pub fn take_file_output(&self) -> Option<ApplicationFileOutput> {
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
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
    model.advance_io(backend);
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
    next_task_id: u64,
    io_task: Option<IoTaskState>,
    pending_io: Option<PendingIo>,
    session_encoding: Option<Receiver<Result<Vec<u8>, String>>>,
    #[cfg(not(target_arch = "wasm32"))]
    background_session_encoding: bool,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
}

struct TrackModel {
    id: TrackId,
    backend_id: BackendTrackId,
    name: String,
    port_name_base: String,
    is_sync: bool,
    audio_channels: u32,
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

enum PendingIo {
    SaveSession,
    #[cfg(not(target_arch = "wasm32"))]
    AwaitingSessionEncoding,
    AwaitingSessionLoad {
        name: String,
        bundle: SessionBundle,
    },
    CommitSessionLoad {
        name: String,
        bundle: SessionBundle,
        backend_data: BackendSessionData,
    },
    AwaitingLoopAudioExportSelection {
        loop_id: LoopId,
        format: LoopAudioExportFormat,
    },
    ExportLoopAudio {
        loop_id: LoopId,
        format: LoopAudioExportFormat,
        channels: Vec<u32>,
    },
    ExportLoopMidi {
        loop_id: LoopId,
        standard: bool,
    },
    AwaitingLoopAudioImport {
        loop_id: LoopId,
        audio: LoopAudio,
        update_loop_length: bool,
    },
    AwaitingLoopAudioMapping {
        loop_id: LoopId,
        audio: LoopAudio,
        update_loop_length: bool,
    },
    AwaitingLoopMidiImport {
        loop_id: LoopId,
        midi: ExactMidi,
        update_loop_length: bool,
    },
    PrepareLoopAudioImport {
        loop_id: LoopId,
        audio: LoopAudio,
        update_loop_length: bool,
    },
    PrepareLoopMidiImport {
        loop_id: LoopId,
        midi: ExactMidi,
        update_loop_length: bool,
    },
    CommitLoopImport {
        loop_id: LoopId,
        backend_data: BackendSessionData,
        message: String,
    },
}

impl ApplicationModel {
    fn initialize(
        backend: &mut dyn Backend,
        file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
        background_session_encoding: bool,
    ) -> Result<Self> {
        #[cfg(target_arch = "wasm32")]
        let _ = background_session_encoding;
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
                has_audio: true,
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
            next_task_id: 1,
            io_task: None,
            pending_io: None,
            session_encoding: None,
            #[cfg(not(target_arch = "wasm32"))]
            background_session_encoding,
            file_outputs,
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
            AppIntent::RequestSaveSession => self.begin_save_session(),
            AppIntent::RequestLoadSessionPicker
            | AppIntent::RequestLoopAudioImportPicker { .. }
            | AppIntent::RequestLoopMidiImportPicker { .. } => Ok(()),
            AppIntent::LoadSessionBytes { name, bytes } => self.begin_load_session(name, &bytes),
            AppIntent::ConfirmSampleRateConversion { task_id, accept } => {
                self.confirm_sample_rate_conversion(backend, task_id, accept)
            }
            AppIntent::ConfirmAudioChannelMapping {
                task_id,
                source_for_destination,
            } => self.confirm_audio_channel_mapping(task_id, source_for_destination),
            AppIntent::ConfirmAudioChannelSelection { task_id, channels } => {
                self.confirm_audio_channel_selection(task_id, channels)
            }
            AppIntent::CancelIoTask { task_id } => self.cancel_io_task(task_id),
            AppIntent::ReportFileIoError { task_id, message } => {
                if task_id.is_some_and(|id| self.io_task.as_ref().is_some_and(|task| task.id == id))
                {
                    self.finish_io(IoTaskStatus::Failed, &message);
                }
                self.notify_error(message);
                Ok(())
            }
            AppIntent::RequestLoopAudioExport { loop_id, format } => {
                self.export_loop_audio(backend, loop_id, format)
            }
            AppIntent::ImportLoopAudioBytes {
                loop_id,
                name,
                bytes,
                update_loop_length,
            } => self.import_loop_audio(backend, loop_id, name, &bytes, update_loop_length),
            AppIntent::RequestLoopMidiExport { loop_id, standard } => {
                self.export_loop_midi(backend, loop_id, standard)
            }
            AppIntent::ImportLoopMidiBytes {
                loop_id,
                name,
                bytes,
                update_loop_length,
            } => self.import_loop_midi(backend, loop_id, name, &bytes, update_loop_length),
        };
        if let Err(error) = result {
            self.notify_error(error);
        }
    }

    fn begin_save_session(&mut self) -> Result<(), String> {
        self.ensure_io_idle()?;
        let task_id = self.start_io_task(IoTaskKind::SaveSession, "Capturing session");
        self.pending_io = Some(PendingIo::SaveSession);
        self.io_task = Some(IoTaskState {
            id: task_id,
            kind: IoTaskKind::SaveSession,
            status: IoTaskStatus::Running,
            progress: 0.05,
            message: "Capturing session".to_owned(),
            sample_rate_warning: None,
            audio_channel_mapping: None,
            audio_channel_selection: None,
        });
        Ok(())
    }

    fn begin_load_session(&mut self, name: String, bytes: &[u8]) -> Result<(), String> {
        self.ensure_io_idle()?;
        let task_id = self.start_io_task(IoTaskKind::LoadSession, "Validating session");
        let bundle = decode_session(bytes).map_err(|error| error.to_string())?;
        if bundle.document.sample_rate != self.status.sample_rate {
            let source_rate = bundle.document.sample_rate;
            let target_rate = self.status.sample_rate;
            self.pending_io = Some(PendingIo::AwaitingSessionLoad { name, bundle });
            self.io_task = Some(IoTaskState {
                id: task_id,
                kind: IoTaskKind::LoadSession,
                status: IoTaskStatus::AwaitingSampleRateConfirmation,
                progress: 0.2,
                message: format!("Resample session from {source_rate} Hz to {target_rate} Hz?"),
                sample_rate_warning: Some(SampleRateWarning {
                    source_rate,
                    target_rate,
                    affected_media: "all session audio, MIDI, loops, offsets, and cycle timing"
                        .to_owned(),
                }),
                audio_channel_mapping: None,
                audio_channel_selection: None,
            });
            return Ok(());
        }
        let backend_data = session_bundle_to_backend(&bundle)?;
        self.pending_io = Some(PendingIo::CommitSessionLoad {
            name,
            bundle,
            backend_data,
        });
        self.set_io_progress(0.7, "Staging session");
        Ok(())
    }

    fn confirm_sample_rate_conversion(
        &mut self,
        _backend: &mut dyn Backend,
        task_id: TaskId,
        accept: bool,
    ) -> Result<(), String> {
        if self.io_task.as_ref().map(|task| task.id) != Some(task_id) {
            return Err(format!("stale I/O task {task_id}"));
        }
        let pending = self
            .pending_io
            .take()
            .ok_or_else(|| "I/O task is not awaiting sample-rate confirmation".to_owned())?;
        if !accept {
            self.finish_io(IoTaskStatus::Cancelled, "I/O cancelled");
            return Ok(());
        }
        self.set_io_progress(0.35, "Resampling media");
        match pending {
            PendingIo::AwaitingSessionLoad { name, bundle } => {
                let bundle = resample_session(&bundle, self.status.sample_rate)
                    .map_err(|error| error.to_string())?;
                let backend_data = session_bundle_to_backend(&bundle)?;
                self.pending_io = Some(PendingIo::CommitSessionLoad {
                    name,
                    bundle,
                    backend_data,
                });
                self.set_io_progress(0.7, "Staging session");
            }
            PendingIo::AwaitingLoopAudioImport {
                loop_id,
                audio,
                update_loop_length,
            } => {
                let audio = resample_loop_audio(&audio, self.status.sample_rate)
                    .map_err(|error| error.to_string())?;
                self.begin_audio_channel_mapping(loop_id, audio, update_loop_length)?;
            }
            PendingIo::AwaitingLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            } => {
                let midi = resample_exact_midi(&midi, self.status.sample_rate)
                    .map_err(|error| error.to_string())?;
                self.pending_io = Some(PendingIo::PrepareLoopMidiImport {
                    loop_id,
                    midi,
                    update_loop_length,
                });
            }
            other => {
                self.pending_io = Some(other);
                return Err("I/O task is not awaiting sample-rate confirmation".to_owned());
            }
        }
        Ok(())
    }

    fn begin_audio_channel_mapping(
        &mut self,
        loop_id: LoopId,
        audio: LoopAudio,
        update_loop_length: bool,
    ) -> Result<(), String> {
        if audio.channels.is_empty() {
            return Err("audio file contains no channels".to_owned());
        }
        let loop_model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let destinations = self
            .tracks
            .iter()
            .find(|track| track.id == loop_model.track_id)
            .ok_or_else(|| "target loop track is unavailable".to_owned())?
            .audio_channels as usize;
        let default_mapping = (0..destinations)
            .map(|index| (index % audio.channels.len()) as u32)
            .collect::<Vec<_>>();
        if let Some(task) = &mut self.io_task {
            task.status = IoTaskStatus::AwaitingChannelMapping;
            task.progress = 0.45;
            task.message = "Map source channels to loop channels".to_owned();
            task.sample_rate_warning = None;
            task.audio_channel_selection = None;
            task.audio_channel_mapping = Some(AudioChannelMappingState {
                source_channels: audio
                    .channels
                    .iter()
                    .map(|channel| channel.label.clone())
                    .collect(),
                destination_channels: (0..destinations)
                    .map(|index| format!("Loop channel {}", index + 1))
                    .collect(),
                default_mapping,
            });
        }
        self.pending_io = Some(PendingIo::AwaitingLoopAudioMapping {
            loop_id,
            audio,
            update_loop_length,
        });
        Ok(())
    }

    fn confirm_audio_channel_mapping(
        &mut self,
        task_id: TaskId,
        source_for_destination: Vec<u32>,
    ) -> Result<(), String> {
        if self.io_task.as_ref().map(|task| task.id) != Some(task_id) {
            return Err(format!("stale I/O task {task_id}"));
        }
        let Some(PendingIo::AwaitingLoopAudioMapping {
            loop_id,
            audio,
            update_loop_length,
        }) = self.pending_io.take()
        else {
            return Err("I/O task is not awaiting an audio channel mapping".to_owned());
        };
        let expected = self
            .io_task
            .as_ref()
            .and_then(|task| task.audio_channel_mapping.as_ref())
            .map(|mapping| mapping.destination_channels.len())
            .unwrap_or(0);
        if source_for_destination.len() != expected
            || source_for_destination
                .iter()
                .any(|source| *source as usize >= audio.channels.len())
        {
            self.pending_io = Some(PendingIo::AwaitingLoopAudioMapping {
                loop_id,
                audio,
                update_loop_length,
            });
            return Err("invalid audio channel mapping".to_owned());
        }
        let mapped = LoopAudio {
            sample_rate: audio.sample_rate,
            channels: source_for_destination
                .into_iter()
                .map(|source| audio.channels[source as usize].clone())
                .collect(),
        };
        self.pending_io = Some(PendingIo::PrepareLoopAudioImport {
            loop_id,
            audio: mapped,
            update_loop_length,
        });
        self.set_io_progress(0.6, "Preparing loop audio");
        Ok(())
    }

    fn confirm_audio_channel_selection(
        &mut self,
        task_id: TaskId,
        channels: Vec<u32>,
    ) -> Result<(), String> {
        if self.io_task.as_ref().map(|task| task.id) != Some(task_id) {
            return Err(format!("stale I/O task {task_id}"));
        }
        let Some(PendingIo::AwaitingLoopAudioExportSelection { loop_id, format }) =
            self.pending_io.take()
        else {
            return Err("I/O task is not awaiting an audio channel selection".to_owned());
        };
        let available = self
            .io_task
            .as_ref()
            .and_then(|task| task.audio_channel_selection.as_ref())
            .map(|selection| selection.available_channels.len())
            .unwrap_or(0);
        let mut unique = channels.clone();
        unique.sort_unstable();
        unique.dedup();
        if channels.is_empty()
            || unique.len() != channels.len()
            || channels
                .iter()
                .any(|channel| *channel as usize >= available)
        {
            self.pending_io = Some(PendingIo::AwaitingLoopAudioExportSelection { loop_id, format });
            return Err("invalid audio channel selection".to_owned());
        }
        self.pending_io = Some(PendingIo::ExportLoopAudio {
            loop_id,
            format,
            channels,
        });
        self.set_io_progress(0.5, "Exporting selected audio channels");
        Ok(())
    }

    fn cancel_io_task(&mut self, task_id: TaskId) -> Result<(), String> {
        if self.io_task.as_ref().map(|task| task.id) != Some(task_id) {
            return Err(format!("stale I/O task {task_id}"));
        }
        self.pending_io = None;
        self.finish_io(IoTaskStatus::Cancelled, "I/O cancelled");
        Ok(())
    }

    fn ensure_io_idle(&self) -> Result<(), String> {
        if self.pending_io.is_some() {
            Err("another I/O task is active".to_owned())
        } else {
            Ok(())
        }
    }

    fn start_io_task(&mut self, kind: IoTaskKind, message: &str) -> TaskId {
        let id = TaskId::from_raw(self.next_task_id);
        self.next_task_id = self.next_task_id.saturating_add(1);
        self.io_task = Some(IoTaskState {
            id,
            kind,
            status: IoTaskStatus::Running,
            progress: 0.0,
            message: message.to_owned(),
            sample_rate_warning: None,
            audio_channel_mapping: None,
            audio_channel_selection: None,
        });
        id
    }

    fn set_io_progress(&mut self, progress: f32, message: &str) {
        if let Some(task) = &mut self.io_task {
            task.status = IoTaskStatus::Running;
            task.progress = progress.clamp(0.0, 1.0);
            task.message = message.to_owned();
            task.sample_rate_warning = None;
            task.audio_channel_mapping = None;
            task.audio_channel_selection = None;
        }
    }

    fn finish_io(&mut self, status: IoTaskStatus, message: &str) {
        if let Some(task) = &mut self.io_task {
            task.status = status;
            task.progress = if status == IoTaskStatus::Completed {
                1.0
            } else {
                task.progress
            };
            task.message = message.to_owned();
            task.sample_rate_warning = None;
            task.audio_channel_mapping = None;
            task.audio_channel_selection = None;
        }
        self.pending_io = None;
        self.session_encoding = None;
    }

    fn fail_io(&mut self, message: String) {
        self.finish_io(IoTaskStatus::Failed, &message);
        self.notify_error(message);
    }

    fn start_session_encoding(&mut self, bundle: SessionBundle) {
        self.set_io_progress(0.45, "Compressing session");
        #[cfg(not(target_arch = "wasm32"))]
        if self.background_session_encoding {
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let result = encode_session(&bundle, env!("CARGO_PKG_VERSION"))
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
            });
            self.session_encoding = Some(receiver);
            self.pending_io = Some(PendingIo::AwaitingSessionEncoding);
            return;
        }
        match encode_session(&bundle, env!("CARGO_PKG_VERSION")) {
            Ok(bytes) => self.complete_session_encoding(bytes),
            Err(error) => self.fail_io(error.to_string()),
        }
    }

    fn complete_session_encoding(&mut self, bytes: Vec<u8>) {
        let task_id = self
            .io_task
            .as_ref()
            .map(|task| task.id)
            .unwrap_or_default();
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(ApplicationFileOutput {
                task_id,
                suggested_name: "session.shoop".to_owned(),
                mime_type: "application/x-shoop-session".to_owned(),
                bytes: Arc::from(bytes),
            });
        self.finish_io(IoTaskStatus::Completed, "Session ready to save");
    }

    fn advance_io(&mut self, backend: &mut dyn Backend) {
        let Some(pending) = self.pending_io.take() else {
            return;
        };
        match pending {
            PendingIo::SaveSession => match backend.capture_session() {
                Ok(capture) => match self.session_bundle_from_backend(&capture) {
                    Ok(bundle) => self.start_session_encoding(bundle),
                    Err(error) => self.fail_io(error),
                },
                Err(error) if io_pending_error(&error.to_string()) => {
                    self.pending_io = Some(PendingIo::SaveSession);
                }
                Err(error) => self.fail_io(format!("could not capture session: {error}")),
            },
            #[cfg(not(target_arch = "wasm32"))]
            PendingIo::AwaitingSessionEncoding => {
                let result = self
                    .session_encoding
                    .as_ref()
                    .map(|receiver| receiver.try_recv())
                    .unwrap_or(Err(TryRecvError::Disconnected));
                match result {
                    Ok(Ok(bytes)) => self.complete_session_encoding(bytes),
                    Ok(Err(error)) => self.fail_io(error),
                    Err(TryRecvError::Empty) => {
                        self.pending_io = Some(PendingIo::AwaitingSessionEncoding);
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.fail_io("session encoding worker stopped unexpectedly".to_owned());
                    }
                }
            }
            PendingIo::AwaitingSessionLoad { name, bundle } => {
                self.pending_io = Some(PendingIo::AwaitingSessionLoad { name, bundle });
            }
            PendingIo::CommitSessionLoad {
                name,
                bundle,
                backend_data,
            } => match backend.replace_session(&backend_data) {
                Ok(replacement) => {
                    match self.apply_loaded_session(backend, &bundle, &replacement) {
                        Ok(()) => {
                            self.finish_io(
                                IoTaskStatus::Completed,
                                &format!("Loaded session {name}"),
                            );
                        }
                        Err(error) => self.fail_io(error),
                    }
                }
                Err(error) if io_pending_error(&error.to_string()) => {
                    self.pending_io = Some(PendingIo::CommitSessionLoad {
                        name,
                        bundle,
                        backend_data,
                    });
                }
                Err(error) => self.fail_io(format!("could not replace session: {error}")),
            },
            PendingIo::AwaitingLoopAudioExportSelection { loop_id, format } => {
                self.pending_io =
                    Some(PendingIo::AwaitingLoopAudioExportSelection { loop_id, format });
            }
            PendingIo::ExportLoopAudio {
                loop_id,
                format,
                channels,
            } => {
                if let Err(error) = self.export_loop_audio_now(backend, loop_id, format, &channels)
                {
                    if io_pending_error(&error) {
                        self.pending_io = Some(PendingIo::ExportLoopAudio {
                            loop_id,
                            format,
                            channels,
                        });
                    } else {
                        self.fail_io(error);
                    }
                }
            }
            PendingIo::ExportLoopMidi { loop_id, standard } => {
                if let Err(error) = self.export_loop_midi_now(backend, loop_id, standard) {
                    if io_pending_error(&error) {
                        self.pending_io = Some(PendingIo::ExportLoopMidi { loop_id, standard });
                    } else {
                        self.fail_io(error);
                    }
                }
            }
            PendingIo::AwaitingLoopAudioImport {
                loop_id,
                audio,
                update_loop_length,
            } => {
                self.pending_io = Some(PendingIo::AwaitingLoopAudioImport {
                    loop_id,
                    audio,
                    update_loop_length,
                });
            }
            PendingIo::AwaitingLoopAudioMapping {
                loop_id,
                audio,
                update_loop_length,
            } => {
                self.pending_io = Some(PendingIo::AwaitingLoopAudioMapping {
                    loop_id,
                    audio,
                    update_loop_length,
                });
            }
            PendingIo::AwaitingLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            } => {
                self.pending_io = Some(PendingIo::AwaitingLoopMidiImport {
                    loop_id,
                    midi,
                    update_loop_length,
                });
            }
            PendingIo::PrepareLoopAudioImport {
                loop_id,
                audio,
                update_loop_length,
            } => match self.prepare_loop_audio_import(backend, loop_id, &audio, update_loop_length)
            {
                Ok(backend_data) => {
                    self.pending_io = Some(PendingIo::CommitLoopImport {
                        loop_id,
                        backend_data,
                        message: "Loop audio imported".to_owned(),
                    });
                    self.set_io_progress(0.75, "Committing loop audio");
                }
                Err(error) if io_pending_error(&error) => {
                    self.pending_io = Some(PendingIo::PrepareLoopAudioImport {
                        loop_id,
                        audio,
                        update_loop_length,
                    });
                }
                Err(error) => self.fail_io(error),
            },
            PendingIo::PrepareLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            } => match self.prepare_loop_midi_import(backend, loop_id, &midi, update_loop_length) {
                Ok(backend_data) => {
                    self.pending_io = Some(PendingIo::CommitLoopImport {
                        loop_id,
                        backend_data,
                        message: "Loop MIDI imported".to_owned(),
                    });
                    self.set_io_progress(0.75, "Committing loop MIDI");
                }
                Err(error) if io_pending_error(&error) => {
                    self.pending_io = Some(PendingIo::PrepareLoopMidiImport {
                        loop_id,
                        midi,
                        update_loop_length,
                    });
                }
                Err(error) => self.fail_io(error),
            },
            PendingIo::CommitLoopImport {
                loop_id,
                backend_data,
                message,
            } => match backend.replace_session(&backend_data) {
                Ok(replacement) => {
                    let length = self.loops.get(&loop_id).and_then(|model| {
                        target_length_for_source(&backend_data, model.backend_id.raw())
                    });
                    match self.remap_backend_entities(&backend_data, &replacement) {
                        Ok(()) => {
                            if let Some(model) = self.loops.get_mut(&loop_id) {
                                if let Some(length) = length {
                                    model.length = length;
                                }
                                model.state.empty = false;
                                model.audio_data = None;
                            }
                            self.finish_io(IoTaskStatus::Completed, &message);
                        }
                        Err(error) => self.fail_io(error),
                    }
                }
                Err(error) if io_pending_error(&error.to_string()) => {
                    self.pending_io = Some(PendingIo::CommitLoopImport {
                        loop_id,
                        backend_data,
                        message,
                    });
                }
                Err(error) => self.fail_io(format!("could not commit loop import: {error}")),
            },
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
        audio_channels: u32,
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
                    has_audio: audio_channels > 0,
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
        let track_capabilities = self
            .tracks
            .iter()
            .filter_map(|track| {
                snapshot
                    .tracks
                    .get(&track.backend_id)
                    .map(|state| (track.id, (state.audio_channels > 0, state.midi)))
            })
            .collect::<BTreeMap<_, _>>();
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
            if let Some((has_audio, has_midi)) = track_capabilities.get(&model.track_id) {
                model.state.has_audio = *has_audio;
                model.state.has_midi = *has_midi;
                model.state.show_gain = *has_audio;
            }
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

    fn session_bundle_from_backend(
        &self,
        capture: &BackendSessionData,
    ) -> Result<SessionBundle, String> {
        let mut media = BTreeMap::new();
        let mut next_channel_id = 1_u64;
        let mut sync_tracks = Vec::new();
        let mut main_tracks = Vec::new();
        for track in &self.tracks {
            let captured = capture
                .tracks
                .iter()
                .find(|candidate| candidate.source_id == track.backend_id.raw())
                .ok_or_else(|| format!("backend omitted track {}", track.id))?;
            let mut ports = Vec::with_capacity(captured.ports.len());
            for captured_port in &captured.ports {
                let app_port = self
                    .connection_ports
                    .values()
                    .find(|port| port.backend_id.raw() == captured_port.source_id)
                    .ok_or_else(|| "backend omitted application port mapping".to_owned())?;
                ports.push(PortDocument {
                    id: app_port.id.raw(),
                    name: captured_port.descriptor.name.clone(),
                    data_type: session_data_type(app_port.data_type),
                    direction: session_port_direction(app_port.direction),
                    role: session_port_role(app_port.role),
                    input_connectability: if app_port.direction == PortDirection::Input {
                        vec![ConnectabilityDocument::External]
                    } else {
                        vec![ConnectabilityDocument::Internal]
                    },
                    output_connectability: if app_port.direction == PortDirection::Output {
                        vec![ConnectabilityDocument::External]
                    } else {
                        vec![ConnectabilityDocument::Internal]
                    },
                    gain: 1.0,
                    muted: false,
                    passthrough_muted: false,
                    internal_connections: Vec::new(),
                    external_connections: captured_port.external_connections.clone(),
                    ringbuffer_frames: 0,
                });
            }
            let audio_inputs = ports
                .iter()
                .filter(|port| port.role == PortRoleDocument::AudioInput)
                .map(|port| port.id)
                .collect::<Vec<_>>();
            let audio_outputs = ports
                .iter()
                .filter(|port| port.role == PortRoleDocument::AudioOutput)
                .map(|port| port.id)
                .collect::<Vec<_>>();
            let midi_ports = ports
                .iter()
                .filter(|port| port.data_type == DataTypeDocument::Midi)
                .map(|port| port.id)
                .collect::<Vec<_>>();
            let mut loops = Vec::with_capacity(track.loops.len());
            for loop_id in &track.loops {
                let model = self
                    .loops
                    .get(loop_id)
                    .ok_or_else(|| format!("missing loop {loop_id}"))?;
                let content = captured
                    .loops
                    .iter()
                    .find(|candidate| candidate.source_id == model.backend_id.raw())
                    .ok_or_else(|| format!("backend omitted loop {loop_id}"))?;
                let mut channels = Vec::with_capacity(content.audio.len() + content.midi.len());
                for (index, audio) in content.audio.iter().enumerate() {
                    let channel_id = next_channel_id;
                    next_channel_id = next_channel_id.saturating_add(1);
                    let media_id = format!("audio_t{}_l{}_c{index}", track.id.raw(), loop_id.raw());
                    let data_length_frames = audio.samples.len() as u64;
                    if !audio.samples.is_empty() {
                        media.insert(
                            media_id.clone(),
                            MediaPayload::Audio(AudioPayload {
                                samples: audio.samples.clone(),
                            }),
                        );
                    }
                    let mut connected_port_ids = Vec::new();
                    if let Some(port) = audio_inputs.get(index) {
                        connected_port_ids.push(*port);
                    }
                    if let Some(port) = audio_outputs.get(index) {
                        connected_port_ids.push(*port);
                    }
                    channels.push(ChannelDocument {
                        id: channel_id,
                        mode: ChannelModeDocument::Direct,
                        data_type: DataTypeDocument::Audio,
                        data_length_frames,
                        start_offset_frames: i64::from(audio.start_offset),
                        preplay_frames: u64::from(audio.preplay),
                        gain: audio.gain,
                        connected_port_ids,
                        media_id: (data_length_frames > 0).then_some(media_id),
                        recording_started_at: None,
                        recording_fx_state_id: None,
                    });
                }
                for (index, midi) in content.midi.iter().enumerate() {
                    let channel_id = next_channel_id;
                    next_channel_id = next_channel_id.saturating_add(1);
                    let media_id = format!("midi_t{}_l{}_c{index}", track.id.raw(), loop_id.raw());
                    if midi.length > 0 || !midi.events.is_empty() || !midi.start_state.is_empty() {
                        media.insert(
                            media_id.clone(),
                            MediaPayload::Midi(ExactMidi {
                                sample_rate: capture.sample_rate,
                                length_frames: u64::from(midi.length),
                                start_state: midi.start_state.clone(),
                                events: midi
                                    .events
                                    .iter()
                                    .enumerate()
                                    .map(|(order, event)| ExactMidiEvent {
                                        frame: u64::from(event.time),
                                        order: order as u32,
                                        data: event.data.clone(),
                                    })
                                    .collect(),
                            }),
                        );
                    }
                    channels.push(ChannelDocument {
                        id: channel_id,
                        mode: ChannelModeDocument::Direct,
                        data_type: DataTypeDocument::Midi,
                        data_length_frames: u64::from(midi.length),
                        start_offset_frames: i64::from(midi.start_offset),
                        preplay_frames: u64::from(midi.preplay),
                        gain: 1.0,
                        connected_port_ids: midi_ports.clone(),
                        media_id: (midi.length > 0
                            || !midi.events.is_empty()
                            || !midi.start_state.is_empty())
                        .then_some(media_id),
                        recording_started_at: None,
                        recording_fx_state_id: None,
                    });
                }
                loops.push(LoopDocument {
                    id: loop_id.raw(),
                    name: model.name.clone(),
                    length_frames: u64::from(content.length),
                    is_sync: model.state.sync,
                    gain: content.gain,
                    balance: content.balance,
                    channels,
                    composite: None,
                });
            }
            let document = TrackDocument {
                id: track.id.raw(),
                name: track.name.clone(),
                port_name_base: track.port_name_base.clone(),
                is_sync: track.is_sync,
                width: None,
                topology: TrackTopologyDocument::Direct {
                    audio_channels: track.audio_channels,
                    midi: captured.state.midi,
                },
                controls: TrackControlsDocument {
                    output_gain_db: captured.state.output_gain_db,
                    output_balance: captured.state.output_balance,
                    output_muted: captured.state.output_muted,
                    input_gain_db: captured.state.input_gain_db,
                    input_balance: captured.state.input_balance,
                    input_monitoring: captured.state.input_monitoring,
                },
                loops,
                ports,
                fx_chain: None,
            };
            if track.is_sync {
                sync_tracks.push(document);
            } else {
                main_tracks.push(document);
            }
        }
        let document = SessionDocument {
            sample_rate: capture.sample_rate,
            global: GlobalControlsDocument {
                default_recording_action: match self.global.default_recording_action {
                    shoop_app_api::DefaultRecordingAction::Record => {
                        RecordingActionDocument::Record
                    }
                    shoop_app_api::DefaultRecordingAction::Grab => RecordingActionDocument::Grab,
                },
                play_after_record: self.global.play_after_record,
                sync: self.global.sync,
                solo: self.global.solo,
                apply_n_cycles: self.global.apply_n_cycles,
            },
            track_groups: vec![
                TrackGroupDocument {
                    name: "sync".to_owned(),
                    tracks: sync_tracks,
                },
                TrackGroupDocument {
                    name: "main".to_owned(),
                    tracks: main_tracks,
                },
            ],
            selected_loop_ids: self
                .loops
                .values()
                .filter(|loop_| loop_.state.selected)
                .map(|loop_| loop_.id.raw())
                .collect(),
            targeted_loop_id: self
                .loops
                .values()
                .find(|loop_| loop_.state.targeted)
                .map(|loop_| loop_.id.raw()),
            buses: Vec::new(),
            global_ports: Vec::new(),
            fx_states: Vec::new(),
            scripts: Vec::new(),
            midi_control: MidiControlDocument::default(),
            settings: Vec::new(),
        };
        Ok(SessionBundle { document, media })
    }

    fn apply_loaded_session(
        &mut self,
        backend: &mut dyn Backend,
        bundle: &SessionBundle,
        replacement: &BackendSessionReplacement,
    ) -> Result<(), String> {
        let mut tracks = Vec::new();
        let mut loops = BTreeMap::new();
        let mut connection_ports = BTreeMap::new();
        let selected = bundle
            .document
            .selected_loop_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let target = bundle.document.targeted_loop_id;
        let sync_source = bundle
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .find(|track| track.is_sync)
            .and_then(|track| track.loops.first())
            .and_then(|loop_| replacement.loops.get(&loop_.id))
            .copied();
        for track_document in bundle
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
        {
            let created = replacement
                .tracks
                .get(&track_document.id)
                .ok_or_else(|| format!("backend omitted loaded track {}", track_document.id))?;
            let (audio_channels, _) = direct_topology(track_document)?;
            if created.ports.len() != track_document.ports.len()
                || created.loops.len() != track_document.loops.len()
            {
                return Err("loaded backend topology shape mismatch".to_owned());
            }
            let mut port_ids = Vec::with_capacity(track_document.ports.len());
            for (document, created_port) in track_document.ports.iter().zip(&created.ports) {
                let id = PortId::from_raw(document.id);
                port_ids.push(id);
                connection_ports.insert(
                    id,
                    ConnectionPortModel {
                        id,
                        backend_id: created_port.id,
                        track_id: TrackId::from_raw(track_document.id),
                        name: document.name.clone(),
                        data_type: app_data_type(document.data_type),
                        direction: app_port_direction(document.direction),
                        role: app_port_role(document.role),
                        candidates: BTreeMap::new(),
                    },
                );
            }
            let mut loop_ids = Vec::with_capacity(track_document.loops.len());
            for (loop_document, backend_loop) in track_document.loops.iter().zip(&created.loops) {
                let id = LoopId::from_raw(loop_document.id);
                loop_ids.push(id);
                if !track_document.is_sync {
                    backend
                        .set_loop_sync_source(*backend_loop, sync_source)
                        .map_err(|error| format!("could not restore loop sync: {error}"))?;
                }
                let has_audio = loop_document
                    .channels
                    .iter()
                    .any(|channel| channel.data_type == DataTypeDocument::Audio);
                let empty = loop_document
                    .channels
                    .iter()
                    .all(|channel| channel.data_length_frames == 0);
                loops.insert(
                    id,
                    LoopModel {
                        id,
                        backend_id: *backend_loop,
                        track_id: TrackId::from_raw(track_document.id),
                        name: loop_document.name.clone(),
                        state: LoopState {
                            id,
                            name: loop_document.name.clone(),
                            mode: LoopMode::Stopped,
                            empty,
                            sync: loop_document.is_sync,
                            targeted: target == Some(loop_document.id),
                            selected: selected.contains(&loop_document.id),
                            show_gain: has_audio,
                            has_audio,
                            has_midi: loop_document
                                .channels
                                .iter()
                                .any(|channel| channel.data_type == DataTypeDocument::Midi),
                            gain: loop_document.gain,
                            balance: loop_document.balance,
                            stereo: audio_channels == 2,
                            play_after_record: bundle.document.global.play_after_record,
                            ..Default::default()
                        },
                        length: u32::try_from(loop_document.length_frames)
                            .map_err(|_| "loop length exceeds engine range".to_owned())?,
                        position: 0,
                        audio_data: None,
                    },
                );
            }
            tracks.push(TrackModel {
                id: TrackId::from_raw(track_document.id),
                backend_id: created.track_id,
                name: track_document.name.clone(),
                port_name_base: track_document.port_name_base.clone(),
                is_sync: track_document.is_sync,
                audio_channels,
                loops: loop_ids,
                port_ids: Arc::from(port_ids),
                controls: TrackControlState {
                    output_gain_db: track_document.controls.output_gain_db,
                    output_balance: track_document.controls.output_balance,
                    output_muted: track_document.controls.output_muted,
                    input_gain_db: track_document.controls.input_gain_db,
                    input_balance: track_document.controls.input_balance,
                    input_monitoring: track_document.controls.input_monitoring,
                    ..Default::default()
                },
            });
        }
        self.next_track_id = tracks
            .iter()
            .map(|track| track.id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_loop_id = loops
            .keys()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_port_id = connection_ports
            .keys()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.tracks = tracks;
        self.loops = loops;
        self.connection_ports = connection_ports;
        self.pending_connections.clear();
        self.connection_errors.clear();
        self.connection_revision = self.connection_revision.wrapping_add(1);
        self.connection_view = Arc::new(ConnectionViewState::default());
        self.global.default_recording_action = match bundle.document.global.default_recording_action
        {
            RecordingActionDocument::Record => shoop_app_api::DefaultRecordingAction::Record,
            RecordingActionDocument::Grab => shoop_app_api::DefaultRecordingAction::Grab,
        };
        self.global.play_after_record = bundle.document.global.play_after_record;
        self.global.sync = bundle.document.global.sync;
        self.global.solo = bundle.document.global.solo;
        self.global.apply_n_cycles = bundle.document.global.apply_n_cycles;
        Ok(())
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
            io_task: self.io_task.clone(),
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

    fn prepare_loop_audio_import(
        &self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        audio: &LoopAudio,
        update_loop_length: bool,
    ) -> Result<BackendSessionData, String> {
        if audio.channels.is_empty() {
            return Err("audio file contains no channels".to_owned());
        }
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let mut capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture target loop: {error}"))?;
        let target = capture
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted target loop".to_owned())?;
        if audio.channels.len() != target.audio.len() {
            return Err(format!(
                "mapped audio has {} channels but the target loop has {}",
                audio.channels.len(),
                target.audio.len()
            ));
        }
        for (channel, source) in target.audio.iter_mut().zip(&audio.channels) {
            channel.samples = source.samples.clone();
        }
        if update_loop_length {
            target.length = target
                .audio
                .iter()
                .map(|channel| channel.samples.len() as u32)
                .max()
                .unwrap_or(0);
        }
        Ok(capture)
    }

    fn prepare_loop_midi_import(
        &self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        midi: &ExactMidi,
        update_loop_length: bool,
    ) -> Result<BackendSessionData, String> {
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let mut capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture target loop: {error}"))?;
        let target = capture
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted target loop".to_owned())?;
        let channel = target
            .midi
            .first_mut()
            .ok_or_else(|| "target loop has no MIDI channel".to_owned())?;
        *channel = BackendMidiContent {
            length: u32::try_from(midi.length_frames)
                .map_err(|_| "MIDI duration exceeds engine range".to_owned())?,
            start_state: midi.start_state.clone(),
            events: midi
                .events
                .iter()
                .map(|event| {
                    Ok(BackendMidiEvent {
                        time: u32::try_from(event.frame)
                            .map_err(|_| "MIDI event exceeds engine range".to_owned())?,
                        data: event.data.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            start_offset: channel.start_offset,
            preplay: channel.preplay,
        };
        if update_loop_length {
            target.length = channel.length;
        }
        Ok(capture)
    }

    fn export_loop_audio(
        &mut self,
        _backend: &mut dyn Backend,
        loop_id: LoopId,
        format: LoopAudioExportFormat,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        let loop_model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let channels = self
            .tracks
            .iter()
            .find(|track| track.id == loop_model.track_id)
            .ok_or_else(|| "loop track is unavailable".to_owned())?
            .audio_channels;
        if channels == 0 {
            return Err("loop has no audio channels".to_owned());
        }
        self.start_io_task(IoTaskKind::ExportLoopAudio, "Select loop audio channels");
        if let Some(task) = &mut self.io_task {
            task.status = IoTaskStatus::AwaitingChannelSelection;
            task.progress = 0.2;
            task.audio_channel_selection = Some(AudioChannelSelectionState {
                available_channels: (0..channels)
                    .map(|index| format!("Direct channel {}", index + 1))
                    .collect(),
                default_selection: (0..channels).collect(),
            });
        }
        self.pending_io = Some(PendingIo::AwaitingLoopAudioExportSelection { loop_id, format });
        Ok(())
    }

    fn import_loop_audio(
        &mut self,
        _backend: &mut dyn Backend,
        loop_id: LoopId,
        name: String,
        bytes: &[u8],
        update_loop_length: bool,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        let task_id = self.start_io_task(IoTaskKind::ImportLoopAudio, "Loading loop audio");
        let audio = if name.to_ascii_lowercase().ends_with(".wav") {
            decode_wav(bytes)
        } else {
            decode_loop_audio(bytes)
        }
        .map_err(|error| error.to_string())?;
        if audio.sample_rate != self.status.sample_rate {
            self.pending_io = Some(PendingIo::AwaitingLoopAudioImport {
                loop_id,
                audio: audio.clone(),
                update_loop_length,
            });
            if let Some(task) = &mut self.io_task {
                task.status = IoTaskStatus::AwaitingSampleRateConfirmation;
                task.progress = 0.2;
                task.message = format!(
                    "Resample loop audio from {} Hz to {} Hz?",
                    audio.sample_rate, self.status.sample_rate
                );
                task.sample_rate_warning = Some(SampleRateWarning {
                    source_rate: audio.sample_rate,
                    target_rate: self.status.sample_rate,
                    affected_media: "the selected loop audio".to_owned(),
                });
            }
        } else {
            self.begin_audio_channel_mapping(loop_id, audio, update_loop_length)?;
        }
        debug_assert_eq!(self.io_task.as_ref().map(|task| task.id), Some(task_id));
        Ok(())
    }

    fn export_loop_midi(
        &mut self,
        _backend: &mut dyn Backend,
        loop_id: LoopId,
        standard: bool,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        self.start_io_task(IoTaskKind::ExportLoopMidi, "Exporting loop MIDI");
        self.pending_io = Some(PendingIo::ExportLoopMidi { loop_id, standard });
        Ok(())
    }

    fn import_loop_midi(
        &mut self,
        _backend: &mut dyn Backend,
        loop_id: LoopId,
        name: String,
        bytes: &[u8],
        update_loop_length: bool,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        self.start_io_task(IoTaskKind::ImportLoopMidi, "Loading loop MIDI");
        let midi = if name.to_ascii_lowercase().ends_with(".mid") {
            decode_standard_midi(bytes, self.status.sample_rate)
        } else {
            decode_exact_midi(bytes)
        }
        .map_err(|error| error.to_string())?;
        if midi.sample_rate != self.status.sample_rate {
            self.pending_io = Some(PendingIo::AwaitingLoopMidiImport {
                loop_id,
                midi: midi.clone(),
                update_loop_length,
            });
            if let Some(task) = &mut self.io_task {
                task.status = IoTaskStatus::AwaitingSampleRateConfirmation;
                task.progress = 0.2;
                task.message = format!(
                    "Resample loop MIDI from {} Hz to {} Hz?",
                    midi.sample_rate, self.status.sample_rate
                );
                task.sample_rate_warning = Some(SampleRateWarning {
                    source_rate: midi.sample_rate,
                    target_rate: self.status.sample_rate,
                    affected_media: "the selected loop MIDI and cycle timing".to_owned(),
                });
            }
        } else {
            self.pending_io = Some(PendingIo::PrepareLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            });
        }
        Ok(())
    }

    fn export_loop_audio_now(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        format: LoopAudioExportFormat,
        selected_channels: &[u32],
    ) -> Result<(), String> {
        let task_id = self
            .io_task
            .as_ref()
            .map(|task| task.id)
            .ok_or_else(|| "loop audio export has no task".to_owned())?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture loop: {error}"))?;
        let content = capture
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted loop content".to_owned())?;
        let audio = LoopAudio {
            sample_rate: capture.sample_rate,
            channels: selected_channels
                .iter()
                .map(|index| {
                    let channel = content
                        .audio
                        .get(*index as usize)
                        .ok_or_else(|| "selected audio channel is unavailable".to_owned())?;
                    Ok(LoopAudioChannel {
                        label: format!("audio {}", index + 1),
                        role: "direct".to_owned(),
                        samples: channel.samples.clone(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        let (bytes, extension, mime_type) = match format {
            LoopAudioExportFormat::Exact => (
                encode_loop_audio(&audio).map_err(|error| error.to_string())?,
                "shoop-audio",
                "application/x-shoop-audio",
            ),
            LoopAudioExportFormat::FloatWav => (
                encode_float_wav(&audio).map_err(|error| error.to_string())?,
                "wav",
                "audio/wav",
            ),
        };
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(ApplicationFileOutput {
                task_id,
                suggested_name: format!("{}.{}", safe_file_stem(&model.name), extension),
                mime_type: mime_type.to_owned(),
                bytes: Arc::from(bytes),
            });
        self.finish_io(IoTaskStatus::Completed, "Loop audio ready to save");
        Ok(())
    }

    #[allow(dead_code)]
    fn import_loop_audio_now(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        name: String,
        bytes: &[u8],
        update_loop_length: bool,
    ) -> Result<(), String> {
        let mut audio = if name.to_ascii_lowercase().ends_with(".wav") {
            decode_wav(bytes)
        } else {
            decode_loop_audio(bytes)
        }
        .map_err(|error| error.to_string())?;
        if audio.sample_rate != self.status.sample_rate {
            let source = audio.sample_rate;
            audio = resample_loop_audio(&audio, self.status.sample_rate)
                .map_err(|error| error.to_string())?;
            self.notifications.push(AppNotification {
                level: NotificationLevel::Warning,
                message: format!(
                    "Resampled loop audio from {source} Hz to {} Hz",
                    self.status.sample_rate
                ),
            });
        }
        if audio.channels.is_empty() {
            return Err("audio file contains no channels".to_owned());
        }
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let mut capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture target loop: {error}"))?;
        let target = capture
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted target loop".to_owned())?;
        for (index, channel) in target.audio.iter_mut().enumerate() {
            channel.samples = audio.channels[index % audio.channels.len()].samples.clone();
        }
        if update_loop_length {
            target.length = target
                .audio
                .iter()
                .map(|channel| channel.samples.len() as u32)
                .max()
                .unwrap_or(0);
        }
        let replacement = backend
            .replace_session(&capture)
            .map_err(|error| format!("could not commit loop audio: {error}"))?;
        self.remap_backend_entities(&capture, &replacement)?;
        if let Some(model) = self.loops.get_mut(&loop_id) {
            model.length =
                target_length_for_source(&capture, loop_id.raw()).unwrap_or(model.length);
            model.state.empty = false;
            model.audio_data = None;
        }
        self.finish_io(IoTaskStatus::Completed, "Loop audio imported");
        Ok(())
    }

    fn export_loop_midi_now(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        standard: bool,
    ) -> Result<(), String> {
        let task_id = self
            .io_task
            .as_ref()
            .map(|task| task.id)
            .ok_or_else(|| "loop MIDI export has no task".to_owned())?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture loop: {error}"))?;
        let content = capture
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .and_then(|loop_| loop_.midi.first())
            .ok_or_else(|| "loop has no MIDI channel".to_owned())?;
        let midi = ExactMidi {
            sample_rate: capture.sample_rate,
            length_frames: u64::from(content.length),
            start_state: content.start_state.clone(),
            events: content
                .events
                .iter()
                .enumerate()
                .map(|(order, event)| ExactMidiEvent {
                    frame: u64::from(event.time),
                    order: order as u32,
                    data: event.data.clone(),
                })
                .collect(),
        };
        let (bytes, extension, mime) = if standard {
            let encoded = encode_standard_midi(&midi).map_err(|error| error.to_string())?;
            self.notifications.push(AppNotification {
                level: NotificationLevel::Warning,
                message: format!(
                    "Standard MIDI export timing error is at most {:.3} samples",
                    encoded.max_quantization_error_frames
                ),
            });
            (encoded.bytes, "mid", "audio/midi")
        } else {
            (
                encode_exact_midi(&midi).map_err(|error| error.to_string())?,
                "shoop-midi",
                "application/x-shoop-midi",
            )
        };
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(ApplicationFileOutput {
                task_id,
                suggested_name: format!("{}.{}", safe_file_stem(&model.name), extension),
                mime_type: mime.to_owned(),
                bytes: Arc::from(bytes),
            });
        self.finish_io(IoTaskStatus::Completed, "Loop MIDI ready to save");
        Ok(())
    }

    #[allow(dead_code)]
    fn import_loop_midi_now(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        name: String,
        bytes: &[u8],
        update_loop_length: bool,
    ) -> Result<(), String> {
        let mut midi = if name.to_ascii_lowercase().ends_with(".mid") {
            decode_standard_midi(bytes, self.status.sample_rate)
        } else {
            decode_exact_midi(bytes)
        }
        .map_err(|error| error.to_string())?;
        if midi.sample_rate != self.status.sample_rate {
            let source = midi.sample_rate;
            midi = resample_exact_midi(&midi, self.status.sample_rate)
                .map_err(|error| error.to_string())?;
            self.notifications.push(AppNotification {
                level: NotificationLevel::Warning,
                message: format!(
                    "Resampled loop MIDI from {source} Hz to {} Hz",
                    self.status.sample_rate
                ),
            });
        }
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let mut capture = backend
            .capture_session()
            .map_err(|error| format!("could not capture target loop: {error}"))?;
        let target = capture
            .tracks
            .iter_mut()
            .flat_map(|track| &mut track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted target loop".to_owned())?;
        let channel = target
            .midi
            .first_mut()
            .ok_or_else(|| "target loop has no MIDI channel".to_owned())?;
        *channel = BackendMidiContent {
            length: u32::try_from(midi.length_frames)
                .map_err(|_| "MIDI duration exceeds engine range".to_owned())?,
            start_state: midi.start_state,
            events: midi
                .events
                .into_iter()
                .map(|event| {
                    Ok(BackendMidiEvent {
                        time: u32::try_from(event.frame)
                            .map_err(|_| "MIDI event exceeds engine range".to_owned())?,
                        data: event.data,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
            start_offset: channel.start_offset,
            preplay: channel.preplay,
        };
        if update_loop_length {
            target.length = channel.length;
        }
        let replacement = backend
            .replace_session(&capture)
            .map_err(|error| format!("could not commit loop MIDI: {error}"))?;
        self.remap_backend_entities(&capture, &replacement)?;
        if let Some(model) = self.loops.get_mut(&loop_id) {
            model.length =
                target_length_for_source(&capture, loop_id.raw()).unwrap_or(model.length);
            model.state.empty = false;
        }
        self.finish_io(IoTaskStatus::Completed, "Loop MIDI imported");
        Ok(())
    }

    fn remap_backend_entities(
        &mut self,
        source: &BackendSessionData,
        replacement: &BackendSessionReplacement,
    ) -> Result<(), String> {
        let old_track_sources = self
            .tracks
            .iter()
            .map(|track| (track.id, track.backend_id.raw()))
            .collect::<BTreeMap<_, _>>();
        let old_loop_sources = self
            .loops
            .values()
            .map(|loop_| (loop_.id, loop_.backend_id.raw()))
            .collect::<BTreeMap<_, _>>();
        let old_port_sources = self
            .connection_ports
            .values()
            .map(|port| (port.id, port.backend_id.raw()))
            .collect::<BTreeMap<_, _>>();
        for track in &mut self.tracks {
            let source_id = old_track_sources[&track.id];
            track.backend_id = replacement
                .tracks
                .get(&source_id)
                .ok_or_else(|| "replacement omitted track mapping".to_owned())?
                .track_id;
        }
        for loop_ in self.loops.values_mut() {
            let source_id = old_loop_sources[&loop_.id];
            loop_.backend_id = *replacement
                .loops
                .get(&source_id)
                .ok_or_else(|| "replacement omitted loop mapping".to_owned())?;
        }
        for port in self.connection_ports.values_mut() {
            let source_id = old_port_sources[&port.id];
            port.backend_id = *replacement
                .ports
                .get(&source_id)
                .ok_or_else(|| "replacement omitted port mapping".to_owned())?;
            port.candidates.clear();
        }
        let _ = source;
        Ok(())
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

fn safe_file_stem(name: &str) -> String {
    let stem = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if stem.is_empty() {
        "loop".to_owned()
    } else {
        stem
    }
}

fn target_length_for_source(source: &BackendSessionData, loop_source_id: u64) -> Option<u32> {
    source
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .find(|loop_| loop_.source_id == loop_source_id)
        .map(|loop_| loop_.length)
}

fn io_pending_error(message: &str) -> bool {
    message.contains("session capture pending") || message.contains("session replacement pending")
}

fn direct_topology(track: &TrackDocument) -> Result<(u32, bool), String> {
    if track.fx_chain.is_some() {
        return Err(format!(
            "track {} requires unsupported FX/Carla topology",
            track.id
        ));
    }
    match track.topology {
        TrackTopologyDocument::Direct {
            audio_channels,
            midi,
        } => Ok((audio_channels, midi)),
        _ => Err(format!(
            "track {} requires an unsupported deferred topology",
            track.id
        )),
    }
}

fn session_bundle_to_backend(bundle: &SessionBundle) -> Result<BackendSessionData, String> {
    if !bundle.document.buses.is_empty()
        || !bundle.document.global_ports.is_empty()
        || !bundle.document.fx_states.is_empty()
        || !bundle.document.scripts.is_empty()
        || !bundle.document.midi_control.bindings.is_empty()
        || !bundle.document.settings.is_empty()
    {
        return Err("session requires a feature not yet available in the egui runtime".to_owned());
    }
    let mut tracks = Vec::new();
    for track in bundle
        .document
        .track_groups
        .iter()
        .flat_map(|group| &group.tracks)
    {
        let (audio_channels, midi) = direct_topology(track)?;
        let state = BackendTrackState {
            audio_channels,
            midi,
            output_gain_db: track.controls.output_gain_db,
            output_balance: track.controls.output_balance,
            output_muted: track.controls.output_muted,
            input_gain_db: track.controls.input_gain_db,
            input_balance: track.controls.input_balance,
            input_monitoring: track.controls.input_monitoring,
            ..Default::default()
        };
        let ports = track
            .ports
            .iter()
            .map(|port| BackendSessionPort {
                source_id: port.id,
                descriptor: BackendPortDescriptor {
                    id: BackendPortId::from_raw(port.id),
                    name: port.name.clone(),
                    data_type: backend_data_type(port.data_type),
                    direction: backend_port_direction(port.direction),
                    role: backend_port_role(port.role),
                },
                external_connections: port.external_connections.clone(),
            })
            .collect();
        let mut loops = Vec::with_capacity(track.loops.len());
        for loop_ in &track.loops {
            if loop_.composite.is_some() {
                return Err(format!(
                    "loop {} requires unsupported composite topology",
                    loop_.id
                ));
            }
            let mut audio = Vec::new();
            let mut midi_channels = Vec::new();
            for channel in &loop_.channels {
                match channel.data_type {
                    DataTypeDocument::Audio => {
                        let samples = match &channel.media_id {
                            Some(id) => match bundle.media.get(id) {
                                Some(MediaPayload::Audio(audio)) => audio.samples.clone(),
                                _ => return Err(format!("audio media {id} is unavailable")),
                            },
                            None => Vec::new(),
                        };
                        audio.push(BackendAudioContent {
                            samples,
                            gain: channel.gain,
                            start_offset: i32::try_from(channel.start_offset_frames)
                                .map_err(|_| "audio offset exceeds engine range".to_owned())?,
                            preplay: u32::try_from(channel.preplay_frames)
                                .map_err(|_| "audio preplay exceeds engine range".to_owned())?,
                        });
                    }
                    DataTypeDocument::Midi => {
                        let exact = match &channel.media_id {
                            Some(id) => match bundle.media.get(id) {
                                Some(MediaPayload::Midi(midi)) => Some(midi),
                                _ => return Err(format!("MIDI media {id} is unavailable")),
                            },
                            None => None,
                        };
                        midi_channels.push(BackendMidiContent {
                            length: u32::try_from(channel.data_length_frames)
                                .map_err(|_| "MIDI duration exceeds engine range".to_owned())?,
                            start_state: exact
                                .map(|midi| midi.start_state.clone())
                                .unwrap_or_default(),
                            events: exact
                                .map(|midi| {
                                    midi.events
                                        .iter()
                                        .map(|event| {
                                            Ok(BackendMidiEvent {
                                                time: u32::try_from(event.frame).map_err(|_| {
                                                    "MIDI event exceeds engine range".to_owned()
                                                })?,
                                                data: event.data.clone(),
                                            })
                                        })
                                        .collect::<Result<Vec<_>, String>>()
                                })
                                .transpose()?
                                .unwrap_or_default(),
                            start_offset: i32::try_from(channel.start_offset_frames)
                                .map_err(|_| "MIDI offset exceeds engine range".to_owned())?,
                            preplay: u32::try_from(channel.preplay_frames)
                                .map_err(|_| "MIDI preplay exceeds engine range".to_owned())?,
                        });
                    }
                }
            }
            if audio.len() != audio_channels as usize || midi_channels.len() != usize::from(midi) {
                return Err(format!("loop {} channel shape is invalid", loop_.id));
            }
            loops.push(BackendLoopContent {
                source_id: loop_.id,
                length: u32::try_from(loop_.length_frames)
                    .map_err(|_| "loop length exceeds engine range".to_owned())?,
                gain: loop_.gain,
                balance: loop_.balance,
                audio,
                midi: midi_channels,
            });
        }
        tracks.push(BackendSessionTrack {
            source_id: track.id,
            port_name_base: track.port_name_base.clone(),
            state,
            loops,
            ports,
            carla_state: None,
        });
    }
    Ok(BackendSessionData {
        sample_rate: bundle.document.sample_rate,
        tracks,
    })
}

fn session_data_type(value: PortDataType) -> DataTypeDocument {
    match value {
        PortDataType::Audio => DataTypeDocument::Audio,
        PortDataType::Midi => DataTypeDocument::Midi,
    }
}

fn app_data_type(value: DataTypeDocument) -> PortDataType {
    match value {
        DataTypeDocument::Audio => PortDataType::Audio,
        DataTypeDocument::Midi => PortDataType::Midi,
    }
}

fn backend_data_type(value: DataTypeDocument) -> BackendPortDataType {
    match value {
        DataTypeDocument::Audio => BackendPortDataType::Audio,
        DataTypeDocument::Midi => BackendPortDataType::Midi,
    }
}

fn session_port_direction(value: PortDirection) -> PortDirectionDocument {
    match value {
        PortDirection::Input => PortDirectionDocument::Input,
        PortDirection::Output => PortDirectionDocument::Output,
    }
}

fn app_port_direction(value: PortDirectionDocument) -> PortDirection {
    match value {
        PortDirectionDocument::Input => PortDirection::Input,
        PortDirectionDocument::Output => PortDirection::Output,
    }
}

fn backend_port_direction(value: PortDirectionDocument) -> BackendPortDirection {
    match value {
        PortDirectionDocument::Input => BackendPortDirection::Input,
        PortDirectionDocument::Output => BackendPortDirection::Output,
    }
}

fn session_port_role(value: PortRole) -> PortRoleDocument {
    match value {
        PortRole::AudioInput => PortRoleDocument::AudioInput,
        PortRole::AudioOutput => PortRoleDocument::AudioOutput,
        PortRole::AudioSend => PortRoleDocument::AudioSend,
        PortRole::AudioReturn => PortRoleDocument::AudioReturn,
        PortRole::MidiInput => PortRoleDocument::MidiInput,
        PortRole::MidiOutput => PortRoleDocument::MidiOutput,
        PortRole::MidiSend => PortRoleDocument::MidiSend,
    }
}

fn app_port_role(value: PortRoleDocument) -> PortRole {
    match value {
        PortRoleDocument::AudioInput => PortRole::AudioInput,
        PortRoleDocument::AudioOutput => PortRole::AudioOutput,
        PortRoleDocument::AudioSend => PortRole::AudioSend,
        PortRoleDocument::AudioReturn => PortRole::AudioReturn,
        PortRoleDocument::MidiInput => PortRole::MidiInput,
        PortRoleDocument::MidiOutput => PortRole::MidiOutput,
        PortRoleDocument::MidiSend => PortRole::MidiSend,
        PortRoleDocument::Internal => PortRole::AudioInput,
    }
}

fn backend_port_role(value: PortRoleDocument) -> BackendPortRole {
    match value {
        PortRoleDocument::AudioInput => BackendPortRole::AudioInput,
        PortRoleDocument::AudioOutput => BackendPortRole::AudioOutput,
        PortRoleDocument::AudioSend => BackendPortRole::AudioSend,
        PortRoleDocument::AudioReturn => BackendPortRole::AudioReturn,
        PortRoleDocument::MidiInput => BackendPortRole::MidiInput,
        PortRoleDocument::MidiOutput => BackendPortRole::MidiOutput,
        PortRoleDocument::MidiSend => BackendPortRole::MidiSend,
        PortRoleDocument::Internal => BackendPortRole::AudioInput,
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
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
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
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
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
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
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
    fn cooperative_session_round_trip_warns_before_resampling_and_rejects_old_files() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Persistent".to_owned(),
                audio_channels: 12,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let persistent_track = runtime
            .snapshot()
            .tracks
            .iter()
            .find(|track| track.name == "Persistent")
            .unwrap()
            .id;
        runtime
            .dispatch(AppIntent::Track {
                track_id: persistent_track,
                action: TrackAction::OutputGainChanged(-6.0),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)))
            .unwrap();
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
            if runtime
                .snapshot()
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
            {
                break;
            }
        }
        let output = runtime
            .take_file_output()
            .expect("session output should be available");
        assert!(output.suggested_name.ends_with(".shoop"));
        let saved = decode_session(&output.bytes).unwrap();
        assert_eq!(saved.document.track_groups[1].tracks[0].name, "Persistent");
        assert_eq!(
            match saved.document.track_groups[1].tracks[0].topology {
                TrackTopologyDocument::Direct { audio_channels, .. } => audio_channels,
                _ => 0,
            },
            12
        );

        let resampled = resample_session(&saved, 32_000).unwrap();
        let bytes = encode_session(&resampled, "test").unwrap();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "different-rate.shoop".to_owned(),
                bytes: Arc::from(bytes),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let warning = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(warning.status, IoTaskStatus::AwaitingSampleRateConfirmation);
        assert_eq!(
            warning.sample_rate_warning.as_ref().unwrap().source_rate,
            32_000
        );
        runtime
            .dispatch(AppIntent::ConfirmSampleRateConversion {
                task_id: warning.id,
                accept: true,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
            if runtime
                .snapshot()
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
            {
                break;
            }
        }
        let loaded = runtime.snapshot();
        assert!(loaded.global_controls.solo);
        assert!(loaded.tracks.iter().any(|track| {
            track.name == "Persistent" && (track.controls.output_gain_db + 6.0).abs() < 0.001
        }));

        let before = loaded.tracks.len();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "old.shl".to_owned(),
                bytes: Arc::from(&b"old qml archive"[..]),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(runtime.snapshot().tracks.len(), before);
        assert!(runtime
            .snapshot()
            .notifications
            .iter()
            .any(|notification| notification.message.contains("unsupported file format")));
    }

    #[test]
    fn loop_audio_and_midi_io_map_channels_and_warn_before_resampling() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Media".to_owned(),
                audio_channels: 3,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loop_id = runtime.snapshot().tracks[1].loops[0].id;
        let audio = LoopAudio {
            sample_rate: 32_000,
            channels: vec![
                LoopAudioChannel {
                    label: "a".to_owned(),
                    role: "direct".to_owned(),
                    samples: vec![0.1; 256],
                },
                LoopAudioChannel {
                    label: "b".to_owned(),
                    role: "direct".to_owned(),
                    samples: vec![0.5; 256],
                },
            ],
        };
        runtime
            .dispatch(AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "input.shoop-audio".to_owned(),
                bytes: Arc::from(encode_loop_audio(&audio).unwrap()),
                update_loop_length: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let warning = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(warning.status, IoTaskStatus::AwaitingSampleRateConfirmation);
        runtime
            .dispatch(AppIntent::ConfirmSampleRateConversion {
                task_id: warning.id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let mapping_task = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(mapping_task.status, IoTaskStatus::AwaitingChannelMapping);
        let mut mapping = mapping_task
            .audio_channel_mapping
            .as_ref()
            .unwrap()
            .default_mapping
            .clone();
        mapping.copy_from_slice(&[1, 0, 1]);
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelMapping {
                task_id: mapping_task.id,
                source_for_destination: mapping,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );
        runtime
            .dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::Exact,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let selection_task = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(
            selection_task.status,
            IoTaskStatus::AwaitingChannelSelection
        );
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: selection_task.id,
                channels: vec![1, 0],
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let exported_audio = decode_loop_audio(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(exported_audio.channels.len(), 2);
        assert_eq!(exported_audio.channels[0].label, "audio 2");
        assert_eq!(exported_audio.channels[1].label, "audio 1");
        assert_ne!(
            exported_audio.channels[0].samples,
            exported_audio.channels[1].samples
        );
        runtime
            .dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::FloatWav,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let selection_task = runtime.snapshot().io_task.clone().unwrap();
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: selection_task.id,
                channels: vec![1, 0],
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let output = runtime.take_file_output().unwrap();
        assert!(output.suggested_name.ends_with(".wav"));
        let wav_audio = decode_wav(&output.bytes).unwrap();
        assert_eq!(wav_audio.channels.len(), exported_audio.channels.len());
        for (wav, exact) in wav_audio.channels.iter().zip(&exported_audio.channels) {
            assert_eq!(wav.role, exact.role);
            assert_eq!(wav.samples, exact.samples);
        }

        let midi = ExactMidi {
            sample_rate: 32_000,
            length_frames: 100,
            start_state: vec![vec![0xB0, 7, 100]],
            events: vec![ExactMidiEvent {
                frame: 50,
                order: 0,
                data: vec![0x90, 60, 100],
            }],
        };
        runtime
            .dispatch(AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "input.shoop-midi".to_owned(),
                bytes: Arc::from(encode_exact_midi(&midi).unwrap()),
                update_loop_length: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let warning = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(warning.status, IoTaskStatus::AwaitingSampleRateConfirmation);
        runtime
            .dispatch(AppIntent::ConfirmSampleRateConversion {
                task_id: warning.id,
                accept: true,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        runtime
            .dispatch(AppIntent::RequestLoopMidiExport {
                loop_id,
                standard: false,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let exported_midi = decode_exact_midi(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(exported_midi.sample_rate, 48_000);
        assert_eq!(exported_midi.length_frames, 150);
        assert_eq!(exported_midi.events[0].frame, 75);
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
