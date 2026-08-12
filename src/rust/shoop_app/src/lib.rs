use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::TryRecvError;
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use shoop_app_api::{
    AppIntent, AppNotification, AppSnapshot, ApplicationPortOwner, ApplicationPortState,
    AudioChannelMappingState, AudioChannelSelectionState, AudioDriverConfig,
    AudioDriverRuntimeState, AudioDriverState, AudioDriverSwitchState, AudioDriverSwitchStatus,
    ChannelId, ClickSoundDescriptor, ClickTrackKind, ClickTrackPreviewStatus, ClickTrackRequest,
    ClickTrackState, ConfirmedConnectionState, ConnectionErrorKind, ConnectionErrorState,
    ConnectionPolicy, ConnectionViewState, DirectTrackSpec, GlobalControlAction, HostPortId,
    HostPortState, IoTaskKind, IoTaskState, IoTaskStatus, KeyEvent, KeyEventType, LoopAction,
    LoopAudioExportFormat, LoopDetailsState, LoopId, LoopMode, LoopState, MidiEventState,
    MidiSequenceChannelState, NotificationLevel, PendingConnectionState, PianoAction, PortDataType,
    PortDirection, PortId, PortRole, SampleRateWarning, ScriptDialogButtonId, ScriptDialogId,
    ScriptId, ScriptKind, ScriptMidiRuleDirection, ScriptingState, StatusState, TaskId,
    TrackAction, TrackControlState, TrackId, TrackPortOwnerKind, TrackProcessorDescriptor,
    TrackSpec, TrackSpecTopology, TrackState, TrackTopology, WaveformChannelState,
};
use shoop_backend::{
    Backend, BackendAudioChannelUpdate, BackendAudioContent, BackendChannelMode,
    BackendConnectionSnapshot, BackendGrabRequest, BackendLoopContent, BackendLoopContentUpdate,
    BackendLoopId, BackendLoopMode, BackendMidiChannelUpdate, BackendMidiContent, BackendMidiData,
    BackendMidiEvent, BackendPortDataType, BackendPortDescriptor, BackendPortDirection,
    BackendPortId, BackendPortRole, BackendSessionData, BackendSessionPort,
    BackendSessionReplacement, BackendSessionTrack, BackendSnapshot,
    BackendTinySynthFxMidiCcAssignment, BackendTinySynthFxParameter, BackendTrackControl,
    BackendTrackFxControl, BackendTrackId, BackendTrackState, BackendTrackTopology,
    DirectTrackRequest, TrackRequest,
};
#[cfg(not(target_arch = "wasm32"))]
use shoop_scripting::NativeMidiService;
use shoop_scripting::{
    ControlLoop, ControlOperation, ControlSnapshot, ControlTrack, NullMidiService, ScriptKeyEvent,
    ScriptLoopEvent, ScriptManager, SessionScriptSource,
};
use shoop_session::{
    click_sound_ids, decode_exact_midi, decode_loop_audio, decode_session, decode_standard_midi,
    decode_wav, encode_exact_midi, encode_float_wav, encode_loop_audio, encode_session,
    encode_standard_midi, generate_audio_click_track, generate_midi_click_track,
    resample_exact_midi, resample_loop_audio, resample_session, AudioClickTrackSpec, AudioPayload,
    ChannelDocument, ChannelModeDocument, ClickTrackTimingSpec, CompositeDocument,
    CompositeEventDocument, CompositeKindDocument, ConnectabilityDocument, DataTypeDocument,
    ExactMidi, ExactMidiEvent, FxChainDocument, FxChainTypeDocument, FxStateDocument,
    GlobalControlsDocument, LoopAudio, LoopAudioChannel, LoopDocument, MediaPayload,
    MidiClickTrackSpec, MidiControlDocument, PortDirectionDocument, PortDocument, PortRoleDocument,
    RecordingActionDocument, ScriptDocument, SessionBundle, SessionDocument,
    TinySynthFxMidiCcAssignmentDocument, TinySynthFxParameterDocument, TrackControlsDocument,
    TrackDocument, TrackGroupDocument, TrackTopologyDocument, MAX_CLICK_TRACK_CLICKS,
    MAX_CLICK_TRACK_FRAMES,
};

const COMMAND_CAPACITY: usize = 1024;
const MAX_COOPERATIVE_COMMANDS_PER_TICK: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(2);
const PREVIEW_OUTPUT_CAPACITY: usize = 1;

static NEXT_INTENT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug)]
pub struct ApplicationFileOutput {
    pub task_id: TaskId,
    pub suggested_name: String,
    pub mime_type: String,
    pub bytes: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct ApplicationAudioPreview {
    pub request_id: u64,
    pub sample_rate: u32,
    pub samples: Arc<[f32]>,
}

#[derive(Clone)]
pub struct ApplicationHandle {
    sender: SyncSender<ApplicationMessage>,
    snapshot: Arc<RwLock<Arc<AppSnapshot>>>,
    saturated_connection: Arc<Mutex<Option<(PortId, String)>>>,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
    preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
}

impl ApplicationHandle {
    pub fn dispatch(&self, intent: AppIntent) -> Result<(), DispatchError> {
        let queued = QueuedIntent::new(intent);
        let span = tracing::debug_span!(
            "frontend.app.intent_dispatch",
            intent_id = queued.id,
            intent = queued.intent.kind(),
            outcome = tracing::field::Empty
        );
        let _entered = span.enter();
        match self.sender.try_send(ApplicationMessage::Intent(queued)) {
            Ok(()) => {
                span.record("outcome", "queued");
                Ok(())
            }
            Err(TrySendError::Full(ApplicationMessage::Intent(QueuedIntent {
                intent:
                    AppIntent::SetPortConnected {
                        port_id,
                        host_port_id,
                        ..
                    },
                ..
            }))) => {
                span.record("outcome", "full");
                *self
                    .saturated_connection
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) =
                    Some((port_id, host_port_id.to_string()));
                Err(DispatchError::Full)
            }
            Err(error) => {
                let error = DispatchError::from(error);
                span.record(
                    "outcome",
                    match error {
                        DispatchError::Full => "full",
                        DispatchError::Disconnected => "disconnected",
                    },
                );
                Err(error)
            }
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

    pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
        self.preview_outputs
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupScript {
    pub name: String,
    pub source: String,
    pub kind: ScriptKind,
    pub enabled: bool,
}

pub struct ApplicationRuntime {
    handle: ApplicationHandle,
    join: Option<JoinHandle<()>>,
    startup_script_ids: Vec<Option<ScriptId>>,
}

impl ApplicationRuntime {
    pub fn start(backend: Box<dyn Backend + Send>) -> Result<Self> {
        Self::start_with_scripts(backend, Vec::new())
    }

    pub fn start_with_scripts(
        mut backend: Box<dyn Backend + Send>,
        startup_scripts: Vec<StartupScript>,
    ) -> Result<Self> {
        let _span = tracing::info_span!(
            "frontend.app.runtime_start",
            startup_script_count = startup_scripts.len()
        )
        .entered();
        let file_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let preview_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let snapshot = Arc::new(RwLock::new(Arc::new(AppSnapshot::default())));
        let (sender, receiver) = mpsc::sync_channel(COMMAND_CAPACITY);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let saturated_connection = Arc::new(Mutex::new(None));
        let handle = ApplicationHandle {
            sender,
            snapshot: Arc::clone(&snapshot),
            saturated_connection: Arc::clone(&saturated_connection),
            file_outputs: Arc::clone(&file_outputs),
            preview_outputs: Arc::clone(&preview_outputs),
        };
        let actor_snapshot = Arc::clone(&snapshot);
        let actor_saturated_connection = Arc::clone(&saturated_connection);
        let join = thread::Builder::new()
            .name("shoop-application".to_owned())
            .spawn(move || {
                let _worker_span = tracing::info_span!("worker.application").entered();
                match ApplicationModel::initialize(
                    &mut *backend,
                    file_outputs,
                    preview_outputs,
                    true,
                ) {
                    Ok(mut model) => {
                        let startup_script_ids = model.install_startup_scripts(startup_scripts);
                        *actor_snapshot
                            .write()
                            .unwrap_or_else(|error| error.into_inner()) =
                            Arc::new(model.snapshot());
                        if ready_sender.send(Ok(startup_script_ids)).is_ok() {
                            run_actor(
                                model,
                                backend,
                                receiver,
                                actor_snapshot,
                                actor_saturated_connection,
                            );
                        }
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error.to_string()));
                    }
                }
            })?;
        match ready_receiver.recv() {
            Ok(Ok(startup_script_ids)) => Ok(Self {
                handle,
                join: Some(join),
                startup_script_ids,
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(anyhow!(error))
            }
            Err(error) => {
                let _ = join.join();
                Err(anyhow!("application actor failed during startup: {error}"))
            }
        }
    }

    pub fn handle(&self) -> ApplicationHandle {
        self.handle.clone()
    }

    pub fn startup_script_ids(&self) -> &[Option<ScriptId>] {
        &self.startup_script_ids
    }
}

impl Drop for ApplicationRuntime {
    fn drop(&mut self) {
        let _span = tracing::info_span!("frontend.app.runtime_shutdown").entered();
        let _ = self.handle.sender.send(ApplicationMessage::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct CooperativeApplicationRuntime {
    model: ApplicationModel,
    backend: Box<dyn Backend>,
    commands: VecDeque<QueuedIntent>,
    snapshot: Arc<AppSnapshot>,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
    preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
}

impl CooperativeApplicationRuntime {
    pub fn start(backend: Box<dyn Backend>) -> Result<Self> {
        Self::start_with_scripts(backend, Vec::new())
    }

    pub fn start_with_scripts(
        backend: Box<dyn Backend>,
        startup_scripts: Vec<StartupScript>,
    ) -> Result<Self> {
        Self::start_with_scripts_and_midi(backend, startup_scripts, Box::new(NullMidiService))
    }

    pub fn start_with_scripts_and_midi(
        mut backend: Box<dyn Backend>,
        startup_scripts: Vec<StartupScript>,
        midi: Box<dyn shoop_scripting::MidiControlService>,
    ) -> Result<Self> {
        let file_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let preview_outputs = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(
            &mut *backend,
            Arc::clone(&file_outputs),
            Arc::clone(&preview_outputs),
            false,
        )?;
        model.script_manager = ScriptManager::new_with_midi(midi);
        model.install_startup_scripts(startup_scripts);
        model.script_last_snapshot = model.script_control_snapshot();
        Self::from_model(model, backend, file_outputs, preview_outputs)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_with_midi(
        backend: Box<dyn Backend>,
        midi: Box<dyn shoop_scripting::MidiControlService>,
    ) -> Result<Self> {
        Self::start_with_scripts_and_midi(backend, Vec::new(), midi)
    }

    fn from_model(
        model: ApplicationModel,
        backend: Box<dyn Backend>,
        file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
        preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
    ) -> Result<Self> {
        let snapshot = Arc::new(model.snapshot());
        Ok(Self {
            model,
            backend,
            commands: VecDeque::with_capacity(COMMAND_CAPACITY),
            snapshot,
            file_outputs,
            preview_outputs,
        })
    }

    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), DispatchError> {
        let queued = QueuedIntent::new(intent);
        let span = tracing::debug_span!(
            "frontend.app.intent_dispatch",
            intent_id = queued.id,
            intent = queued.intent.kind(),
            outcome = tracing::field::Empty
        );
        let _entered = span.enter();
        if self.commands.len() >= COMMAND_CAPACITY {
            span.record("outcome", "full");
            if let AppIntent::SetPortConnected {
                port_id,
                host_port_id,
                ..
            } = queued.intent
            {
                self.model
                    .report_connection_saturation(port_id, host_port_id.to_string());
                self.snapshot = Arc::new(self.model.snapshot());
            }
            return Err(DispatchError::Full);
        }
        self.commands.push_back(queued);
        span.record("outcome", "queued");
        Ok(())
    }

    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        Arc::clone(&self.snapshot)
    }

    pub fn tick(&mut self, elapsed: Duration) {
        for _ in 0..MAX_COOPERATIVE_COMMANDS_PER_TICK {
            let Some(queued) = self.commands.pop_front() else {
                break;
            };
            handle_queued_intent(&mut self.model, &mut *self.backend, queued);
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

    pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
        self.preview_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front()
    }
}

struct QueuedIntent {
    id: u64,
    intent: AppIntent,
}

impl QueuedIntent {
    fn new(intent: AppIntent) -> Self {
        Self {
            id: NEXT_INTENT_ID.fetch_add(1, Ordering::Relaxed),
            intent,
        }
    }
}

enum ApplicationMessage {
    Intent(QueuedIntent),
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
            Ok(ApplicationMessage::Intent(queued)) => {
                handle_queued_intent(&mut model, &mut *backend, queued)
            }
            Ok(ApplicationMessage::Shutdown) => {
                model.handle_intent(&mut *backend, AppIntent::Piano(PianoAction::ReleaseAll));
                break;
            }
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

fn handle_queued_intent(
    model: &mut ApplicationModel,
    backend: &mut dyn Backend,
    queued: QueuedIntent,
) {
    let _span = tracing::debug_span!(
        "frontend.app.intent_handle",
        intent_id = queued.id,
        intent = queued.intent.kind()
    )
    .entered();
    model.handle_intent(backend, queued.intent);
}

fn update_application(
    model: &mut ApplicationModel,
    backend: &mut dyn Backend,
    elapsed: Duration,
    publish: impl FnOnce(Arc<AppSnapshot>),
) {
    let span = tracing::trace_span!(
        "frontend.app.update",
        revision = model.revision,
        elapsed_us = elapsed.as_micros() as u64,
        outcome = tracing::field::Empty
    );
    let _entered = span.enter();
    {
        let _span = tracing::trace_span!("frontend.app.backend_advance").entered();
        backend.advance(elapsed);
    }
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
    if let Err(error) = model.advance_script_compositions(backend, elapsed) {
        model.notify_error(error);
    }
    if let Err(error) = model.refresh_selected_media(backend) {
        model.notify_error(error);
    }
    model.advance_io(backend);
    if let Err(error) = model.advance_scripting(backend, elapsed) {
        model.notify_error(error);
    }
    model.revision = model.revision.wrapping_add(1);
    {
        let _span = tracing::trace_span!(
            "frontend.app.snapshot_publish",
            revision = model.revision,
            track_count = model.tracks.len(),
            loop_count = model.loops.len()
        )
        .entered();
        publish(Arc::new(model.snapshot()));
    }
    span.record("outcome", "published");
}

struct ApplicationModel {
    revision: u64,
    next_track_id: u64,
    next_loop_id: u64,
    next_port_id: u64,
    tracks: Vec<TrackModel>,
    loops: BTreeMap<LoopId, LoopModel>,
    connection_ports: BTreeMap<PortId, ConnectionPortModel>,
    host_ports: BTreeMap<String, HostPortState>,
    confirmed_connections: BTreeSet<(PortId, String)>,
    pending_connections: BTreeMap<(PortId, String), PendingConnection>,
    connection_errors: Vec<ConnectionErrorState>,
    connection_revision: u64,
    connection_backend_available: bool,
    connection_view: Arc<ConnectionViewState>,
    scripting_view: Arc<ScriptingState>,
    track_processors: Arc<[TrackProcessorDescriptor]>,
    script_manager: ScriptManager,
    script_last_snapshot: ControlSnapshot,
    script_composition_playback: BTreeMap<LoopId, ScriptCompositionPlayback>,
    script_composition_frame_remainder: u128,
    active_piano_notes: BTreeMap<u8, BTreeSet<TrackId>>,
    global: shoop_app_api::GlobalControlState,
    status: StatusState,
    last_callback_budget_overruns: u32,
    audio_drivers: AudioDriverRuntimeState,
    click_track: ClickTrackState,
    next_preview_request_id: u64,
    notifications: Vec<AppNotification>,
    next_task_id: u64,
    next_audio_switch_id: u64,
    pending_audio_switch: Option<PendingAudioSwitch>,
    io_task: Option<IoTaskState>,
    pending_io: Option<PendingIo>,
    session_encoding: Option<Receiver<Result<Vec<u8>, String>>>,
    #[cfg(not(target_arch = "wasm32"))]
    background_session_encoding: bool,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
    preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
}

struct TrackModel {
    id: TrackId,
    backend_id: BackendTrackId,
    name: String,
    port_name_base: String,
    is_sync: bool,
    audio_channels: u32,
    topology: TrackTopology,
    fx: Option<shoop_app_api::TrackFxState>,
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

#[derive(Clone)]
struct RecordedFxState {
    processor_type: shoop_app_api::TrackProcessorTypeId,
    state: String,
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
    midi_data: Option<Vec<MidiSequenceChannelState>>,
    script_composition: Vec<Vec<LoopId>>,
    recorded_fx_state: Option<RecordedFxState>,
}

fn midi_detail_channels(model: &LoopModel, data: BackendMidiData) -> Vec<MidiSequenceChannelState> {
    data.channels
        .into_iter()
        .enumerate()
        .map(|(index, channel)| {
            let kind = match channel.mode {
                BackendChannelMode::Direct => "MIDI",
                BackendChannelMode::Dry => "Dry MIDI",
                BackendChannelMode::Wet => "Wet MIDI",
            };
            MidiSequenceChannelState {
                id: ChannelId::from_raw(
                    model.id.raw().wrapping_shl(16) | 0x8000 | index as u64 + 1,
                ),
                label: format!("{kind} {}", index + 1),
                content_revision: channel.content_revision,
                events: channel
                    .events
                    .into_iter()
                    .map(|event| MidiEventState {
                        frame: event.time,
                        data: Arc::from(event.data),
                    })
                    .collect(),
                start_offset: i64::from(channel.start_offset),
                loop_length: u64::from(model.length),
                played_sample: matches!(
                    model.state.mode,
                    LoopMode::Playing | LoopMode::PlayingDryThroughWet
                )
                .then_some(i64::from(model.position)),
            }
        })
        .collect()
}

struct ScriptCompositionPlayback {
    section: usize,
    remaining_frames: u64,
    mode: LoopMode,
}

struct PendingAudioSwitch {
    target: shoop_app_api::ResolvedAudioDriverConfig,
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
    PrepareGeneratedClickAudio {
        loop_id: LoopId,
        audio: LoopAudio,
    },
    PrepareGeneratedClickMidi {
        loop_id: LoopId,
        midi: ExactMidi,
    },
    CommitLoopImport {
        loop_id: LoopId,
        update: BackendLoopContentUpdate,
        message: String,
    },
}

fn piano_failures(failures: Vec<String>) -> Result<(), String> {
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "could not inject piano MIDI into {} track(s): {}",
            failures.len(),
            failures.join("; ")
        ))
    }
}

impl ApplicationModel {
    fn initialize(
        backend: &mut dyn Backend,
        file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
        preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
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
            midi_data: None,
            script_composition: Vec::new(),
            recorded_fx_state: None,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let script_manager = ScriptManager::new_with_midi(Box::new(NativeMidiService::new()));
        #[cfg(target_arch = "wasm32")]
        let script_manager = ScriptManager::new_with_midi(Box::new(NullMidiService));
        let model = Self {
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
                topology: TrackTopology::Direct,
                fx: None,
                loops: vec![loop_id],
                port_ids,
                controls: Default::default(),
            }],
            loops: BTreeMap::from([(loop_id, loop_model)]),
            connection_ports,
            host_ports: BTreeMap::new(),
            confirmed_connections: BTreeSet::new(),
            pending_connections: BTreeMap::new(),
            connection_errors: Vec::new(),
            connection_revision: 1,
            connection_backend_available: false,
            connection_view: Arc::new(ConnectionViewState::default()),
            scripting_view: Arc::new(ScriptingState {
                supported: true,
                ..Default::default()
            }),
            track_processors: backend
                .track_processor_catalog()
                .unwrap_or_else(|_| Arc::from([])),
            script_manager,
            script_last_snapshot: ControlSnapshot::default(),
            script_composition_playback: BTreeMap::new(),
            script_composition_frame_remainder: 0,
            active_piano_notes: BTreeMap::new(),
            global: Default::default(),
            status: Default::default(),
            last_callback_budget_overruns: 0,
            audio_drivers: backend.audio_driver_state().unwrap_or_default(),
            click_track: ClickTrackState {
                sounds: click_sound_ids()
                    .map(|id| ClickSoundDescriptor {
                        id: id.to_owned(),
                        name: id.to_owned(),
                    })
                    .collect::<Vec<_>>()
                    .into(),
                max_click_count: MAX_CLICK_TRACK_CLICKS,
                max_output_frames: MAX_CLICK_TRACK_FRAMES,
                ..Default::default()
            },
            next_preview_request_id: 1,
            notifications: Vec::new(),
            next_task_id: 1,
            next_audio_switch_id: 1,
            pending_audio_switch: None,
            io_task: None,
            pending_io: None,
            session_encoding: None,
            #[cfg(not(target_arch = "wasm32"))]
            background_session_encoding,
            file_outputs,
            preview_outputs,
        };
        let mut model = model;
        model.script_last_snapshot = model.script_control_snapshot();
        Ok(model)
    }

    fn install_startup_scripts(&mut self, scripts: Vec<StartupScript>) -> Vec<Option<ScriptId>> {
        let mut ids = Vec::with_capacity(scripts.len());
        for script in scripts {
            match self
                .script_manager
                .add(script.name, script.source, script.kind, script.enabled)
            {
                Ok(id) => ids.push(Some(id)),
                Err(error) => {
                    ids.push(None);
                    self.notify_error(error.to_string());
                }
            }
        }
        self.refresh_scripting_view();
        ids
    }

    fn handle_intent(&mut self, backend: &mut dyn Backend, intent: AppIntent) {
        let kind = intent.kind();
        let span = tracing::debug_span!(
            "frontend.app.intent_apply",
            intent = kind,
            revision = self.revision,
            outcome = tracing::field::Empty
        );
        let _entered = span.enter();
        let result = match intent {
            AppIntent::Loop {
                track_id,
                loop_id,
                action,
            } => self.handle_loop_action(backend, track_id, loop_id, action),
            AppIntent::Global(action) => self.handle_global_action(backend, action),
            AppIntent::Piano(action) => self.handle_piano_action(backend, action),
            AppIntent::Track { track_id, action } => {
                self.handle_track_action(backend, track_id, action)
            }
            AppIntent::AddTrack(spec) => self.add_track(backend, spec),
            AppIntent::AddTrackWithTopology(spec) => self.add_track_spec(backend, spec),
            AppIntent::AddLoop { track_id } => self.add_aligned_loop_row(backend, track_id),
            AppIntent::KeyEvent(event) => self.handle_script_key_event(backend, event),
            AppIntent::AddScriptSource {
                name,
                source,
                kind,
                enabled,
            } => self.add_script_source(backend, name, source, kind, enabled),
            AppIntent::AddEphemeralScript { name, source } => {
                self.add_ephemeral_script(backend, name, source)
            }
            AppIntent::SetScriptEnabled { script_id, enabled } => {
                self.set_script_enabled(backend, script_id, enabled)
            }
            AppIntent::RestartScript { script_id } => self.restart_script(backend, script_id),
            AppIntent::ReplaceScriptSource { script_id, source } => {
                self.replace_script_source(backend, script_id, source)
            }
            AppIntent::StopScript { script_id } => self.stop_script(script_id),
            AppIntent::ForgetScript { script_id } => self.forget_script(script_id),
            AppIntent::InvokeScriptDialogButton {
                script_id,
                dialog_id,
                button_id,
            } => self.invoke_script_dialog_button(backend, script_id, dialog_id, button_id),
            AppIntent::SetPortConnected {
                port_id,
                host_port_id,
                connected,
            } => self.set_port_connected(backend, port_id, host_port_id.to_string(), connected),
            AppIntent::RefreshAudioDriverDiscovery { config } => backend
                .refresh_audio_driver_discovery(&config)
                .map(|runtime| {
                    let switch = self.audio_drivers.switch.clone();
                    self.audio_drivers = runtime;
                    self.audio_drivers.switch = switch;
                })
                .map_err(|error| format!("could not refresh audio devices: {error}")),
            AppIntent::RequestAudioDriverSwitch { config } => {
                self.request_audio_driver_switch(backend, config)
            }
            AppIntent::ConfirmAudioDriverSwitch { request_id, accept } => {
                self.confirm_audio_driver_switch(backend, request_id, accept)
            }
            AppIntent::CompleteAudioDriverSwitchPersistence {
                request_id,
                success,
                message,
            } => self.complete_audio_driver_persistence(request_id, success, message),
            AppIntent::ResetXruns => {
                self.status.xruns = 0;
                Ok(())
            }
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
            AppIntent::PreviewClickTrack { loop_id, request } => {
                self.preview_click_track(loop_id, request)
            }
            AppIntent::CompleteClickTrackPreview {
                request_id,
                success,
                message,
            } => self.complete_click_track_preview(request_id, success, message),
            AppIntent::GenerateClickTrack { loop_id, request } => {
                self.begin_generate_click_track(loop_id, request)
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
            span.record("outcome", "error");
            tracing::warn!(intent = kind, error = %error, "frontend.app.intent_failed");
            self.notify_error(error);
        } else {
            span.record("outcome", "ok");
        }
    }

    fn handle_piano_action(
        &mut self,
        backend: &mut dyn Backend,
        action: PianoAction,
    ) -> Result<(), String> {
        match action {
            PianoAction::Press(note) => {
                let note = note.value();
                if self.active_piano_notes.contains_key(&note) {
                    return Ok(());
                }
                let destinations = self
                    .tracks
                    .iter()
                    .filter(|track| self.piano_track_is_eligible(track))
                    .map(|track| (track.id, track.backend_id))
                    .collect::<Vec<_>>();
                let mut recipients = BTreeSet::new();
                let mut failures = Vec::new();
                let event = BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, note, 100],
                };
                for (track_id, backend_id) in destinations {
                    match backend.inject_midi_input(backend_id, std::slice::from_ref(&event)) {
                        Ok(()) => {
                            recipients.insert(track_id);
                        }
                        Err(error) => failures.push(format!("{track_id}: {error}")),
                    }
                }
                if !recipients.is_empty() {
                    self.active_piano_notes.insert(note, recipients);
                }
                piano_failures(failures)
            }
            PianoAction::Release(note) => {
                let note = note.value();
                let recipients = self.active_piano_notes.remove(&note).unwrap_or_default();
                self.release_piano_note(backend, note, recipients)
            }
            PianoAction::ReleaseAll => {
                let active = std::mem::take(&mut self.active_piano_notes);
                let mut failures = Vec::new();
                for (note, recipients) in active {
                    if let Err(error) = self.release_piano_note(backend, note, recipients) {
                        failures.push(error);
                    }
                }
                piano_failures(failures)
            }
        }
    }

    fn piano_track_is_eligible(&self, track: &TrackModel) -> bool {
        track.controls.input_monitoring
            && track.port_ids.iter().any(|port_id| {
                self.connection_ports.get(port_id).is_some_and(|port| {
                    port.track_id == track.id
                        && port.data_type == PortDataType::Midi
                        && port.direction == PortDirection::Input
                        && port.role == PortRole::MidiInput
                })
            })
    }

    fn release_piano_note(
        &self,
        backend: &mut dyn Backend,
        note: u8,
        recipients: BTreeSet<TrackId>,
    ) -> Result<(), String> {
        let event = BackendMidiEvent {
            time: 0,
            data: vec![0x80, note, 0],
        };
        let mut failures = Vec::new();
        for track_id in recipients {
            let Some(track) = self.tracks.iter().find(|track| track.id == track_id) else {
                continue;
            };
            if let Err(error) =
                backend.inject_midi_input(track.backend_id, std::slice::from_ref(&event))
            {
                failures.push(format!("{track_id}: {error}"));
            }
        }
        piano_failures(failures)
    }

    fn add_script_source(
        &mut self,
        backend: &mut dyn Backend,
        name: String,
        source: Arc<str>,
        kind: ScriptKind,
        enabled: bool,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .add(name, source.to_string(), kind, enabled)
            .map(|_| ())
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn add_ephemeral_script(
        &mut self,
        backend: &mut dyn Backend,
        name: String,
        source: Arc<str>,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .add_ephemeral(name, source.to_string())
            .map(|_| ())
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn set_script_enabled(
        &mut self,
        backend: &mut dyn Backend,
        id: ScriptId,
        enabled: bool,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .set_enabled(id, enabled)
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn restart_script(&mut self, backend: &mut dyn Backend, id: ScriptId) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .start(id)
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn replace_script_source(
        &mut self,
        backend: &mut dyn Backend,
        id: ScriptId,
        source: Arc<str>,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .replace_user_source(id, source.to_string())
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn stop_script(&mut self, id: ScriptId) -> Result<(), String> {
        let result = self
            .script_manager
            .stop(id)
            .map_err(|error| error.to_string());
        self.refresh_scripting_view();
        result
    }

    fn forget_script(&mut self, id: ScriptId) -> Result<(), String> {
        let result = self
            .script_manager
            .forget(id)
            .map_err(|error| error.to_string());
        self.refresh_scripting_view();
        result
    }

    fn invoke_script_dialog_button(
        &mut self,
        backend: &mut dyn Backend,
        script_id: ScriptId,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let callback_result = self
            .script_manager
            .invoke_dialog_button(script_id, dialog_id, button_id)
            .map_err(|error| error.to_string());
        let operation_result = self.apply_script_operations(backend);
        self.refresh_scripting_view();
        callback_result.and(operation_result)
    }

    fn handle_script_key_event(
        &mut self,
        backend: &mut dyn Backend,
        event: KeyEvent,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        self.script_manager.dispatch_key_event(ScriptKeyEvent {
            event_type: match event.event_type {
                KeyEventType::Pressed => 0,
                KeyEventType::Released => 1,
            },
            key: event.key,
            modifiers: event.modifiers,
        });
        let result = self.apply_script_operations(backend);
        self.refresh_scripting_view();
        result
    }

    fn advance_scripting(
        &mut self,
        backend: &mut dyn Backend,
        elapsed: Duration,
    ) -> Result<(), String> {
        let current = self.script_control_snapshot();
        self.script_manager.set_control_snapshot(current.clone());
        for loop_ in &current.loops {
            let previous = self
                .script_last_snapshot
                .loops
                .iter()
                .find(|candidate| candidate.id == loop_.id);
            let mut event_types = Vec::new();
            match previous {
                Some(previous) => {
                    if previous.mode != loop_.mode {
                        event_types.push(0);
                    }
                    if previous.length != loop_.length {
                        event_types.push(1);
                    }
                    if previous.selected != loop_.selected {
                        event_types.push(2);
                    }
                    if previous.targeted != loop_.targeted {
                        event_types.push(3);
                    }
                    if previous.coords != loop_.coords {
                        event_types.push(4);
                    }
                }
                None => event_types.extend([0, 1, 2, 3, 4]),
            }
            for event_type in event_types {
                self.script_manager.dispatch_loop_event(&ScriptLoopEvent {
                    coords: loop_.coords,
                    event_type,
                    mode: loop_.mode,
                    length: loop_.length,
                    selected: loop_.selected,
                    targeted: loop_.targeted,
                });
            }
        }
        if current.apply_n_cycles != self.script_last_snapshot.apply_n_cycles
            || current.solo != self.script_last_snapshot.solo
            || current.sync_active != self.script_last_snapshot.sync_active
            || current.play_after_record != self.script_last_snapshot.play_after_record
            || current.auto_mute_other_track_inputs
                != self.script_last_snapshot.auto_mute_other_track_inputs
            || current.default_recording_action
                != self.script_last_snapshot.default_recording_action
        {
            self.script_manager.dispatch_global_event();
        }
        self.script_manager.advance_timers(elapsed);
        self.script_manager.advance_midi(elapsed);
        self.script_last_snapshot = current;
        let result = self.apply_script_operations(backend);
        self.refresh_scripting_view();
        result
    }

    fn prepare_script_invocation(&mut self) {
        let snapshot = self.script_control_snapshot();
        self.script_manager.set_control_snapshot(snapshot);
    }

    fn script_control_snapshot(&self) -> ControlSnapshot {
        let mut tracks = Vec::with_capacity(self.tracks.len());
        let mut loops = Vec::with_capacity(self.loops.len());
        let mut main_index = 0_i64;
        for track in &self.tracks {
            let index = if track.is_sync {
                -1
            } else {
                let index = main_index;
                main_index += 1;
                index
            };
            tracks.push(ControlTrack {
                id: track.id,
                index,
                output_gain_db: track.controls.output_gain_db,
                output_balance: track.controls.output_balance,
                output_muted: track.controls.output_muted,
                input_gain_db: track.controls.input_gain_db,
                input_muted: !track.controls.input_monitoring,
            });
            for (row, id) in track.loops.iter().enumerate() {
                let Some(model) = self.loops.get(id) else {
                    continue;
                };
                loops.push(ControlLoop {
                    id: model.id,
                    coords: [index, row as i64],
                    mode: model.state.mode,
                    next_mode: model.state.next_transition_delay.and_then(|_| {
                        (model.state.next_mode != LoopMode::Unknown)
                            .then_some(model.state.next_mode)
                    }),
                    next_mode_delay: model.state.next_transition_delay,
                    length: model.length,
                    gain: model.state.gain,
                    balance: model.state.balance,
                    selected: model.state.selected,
                    targeted: model.state.targeted,
                });
            }
        }
        ControlSnapshot {
            loops,
            tracks,
            apply_n_cycles: self.global.apply_n_cycles,
            solo: self.global.solo,
            sync_active: self.global.sync,
            play_after_record: self.global.play_after_record,
            auto_mute_other_track_inputs: self.global.auto_mute_other_track_inputs,
            default_recording_action: self.global.default_recording_action,
        }
    }

    fn apply_script_operations(&mut self, backend: &mut dyn Backend) -> Result<(), String> {
        for operation in self.script_manager.take_control_operations() {
            self.apply_script_operation(backend, operation)?;
        }
        Ok(())
    }

    fn apply_script_operation(
        &mut self,
        backend: &mut dyn Backend,
        operation: ControlOperation,
    ) -> Result<(), String> {
        match operation {
            ControlOperation::Transition {
                loops,
                mode,
                cycles_delay,
                align_to_sync_at,
            } => {
                for id in loops {
                    let backend_id = self
                        .loops
                        .get(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?
                        .backend_id;
                    backend
                        .transition_loop_aligned(
                            backend_id,
                            backend_loop_mode(mode),
                            cycles_delay,
                            align_to_sync_at,
                        )
                        .map_err(|error| format!("could not transition loop {id}: {error}"))?;
                }
                Ok(())
            }
            ControlOperation::Trigger { loops, mode } => {
                self.script_trigger_loops(backend, &loops, mode)
            }
            ControlOperation::Grab { loops } => {
                for id in loops {
                    self.grab_targets(backend, id)?;
                }
                Ok(())
            }
            ControlOperation::RecordN {
                loops,
                n_cycles,
                cycles_delay,
            } => {
                for id in loops {
                    let backend_id = self
                        .loops
                        .get(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?
                        .backend_id;
                    backend
                        .transition_loop(backend_id, BackendLoopMode::Recording, Some(cycles_delay))
                        .map_err(|error| format!("could not record loop {id}: {error}"))?;
                    let finish = if self.global.play_after_record {
                        BackendLoopMode::Playing
                    } else {
                        BackendLoopMode::Stopped
                    };
                    backend
                        .transition_loop(
                            backend_id,
                            finish,
                            Some(cycles_delay.saturating_add(n_cycles)),
                        )
                        .map_err(|error| {
                            format!("could not schedule recording end for loop {id}: {error}")
                        })?;
                }
                Ok(())
            }
            ControlOperation::RecordWithTargeted { loops } => {
                let target = self
                    .loops
                    .values()
                    .find(|model| model.state.targeted)
                    .map(|model| model.backend_id)
                    .ok_or_else(|| "cannot record with targeted: no loop is targeted".to_owned())?;
                for id in loops {
                    let backend_id = self
                        .loops
                        .get(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?
                        .backend_id;
                    backend
                        .set_loop_sync_source(backend_id, Some(target))
                        .map_err(|error| format!("could not target-sync loop {id}: {error}"))?;
                    backend
                        .transition_loop(backend_id, BackendLoopMode::Recording, Some(0))
                        .map_err(|error| format!("could not record loop {id}: {error}"))?;
                }
                Ok(())
            }
            ControlOperation::SetLoopGain { loops, gain } => {
                for id in loops {
                    let model = self
                        .loops
                        .get_mut(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?;
                    backend
                        .set_loop_gain(model.backend_id, gain)
                        .map_err(|error| format!("could not set loop gain: {error}"))?;
                    model.state.gain = gain;
                }
                Ok(())
            }
            ControlOperation::SetLoopBalance { loops, balance } => {
                for id in loops {
                    let model = self
                        .loops
                        .get_mut(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?;
                    backend
                        .set_loop_balance(model.backend_id, balance)
                        .map_err(|error| format!("could not set loop balance: {error}"))?;
                    model.state.balance = balance;
                }
                Ok(())
            }
            ControlOperation::SetLoopSelection {
                loops,
                selected,
                clear_others,
            } => {
                if clear_others {
                    for model in self.loops.values_mut() {
                        model.state.selected = false;
                    }
                }
                for id in loops {
                    if let Some(model) = self.loops.get_mut(&id) {
                        model.state.selected = selected;
                    }
                }
                self.refresh_selected_media(backend)
            }
            ControlOperation::SetTarget { target } => {
                for model in self.loops.values_mut() {
                    model.state.targeted = Some(model.id) == target;
                }
                Ok(())
            }
            ControlOperation::ClearLoops { loops } => {
                for id in loops {
                    self.script_composition_playback.remove(&id);
                    let model = self
                        .loops
                        .get_mut(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?;
                    backend
                        .clear_loop(model.backend_id)
                        .map_err(|error| format!("could not clear loop {id}: {error}"))?;
                    model.length = 0;
                    model.state.empty = true;
                    model.state.composite_kind = shoop_app_api::CompositeKind::None;
                    model.audio_data = None;
                    model.midi_data = None;
                    model.script_composition.clear();
                }
                Ok(())
            }
            ControlOperation::AdoptRingbuffers {
                loops,
                reverse_cycle_start,
                cycles_length,
                go_to_cycle,
                go_to_mode,
            } => {
                let requests = loops
                    .iter()
                    .map(|id| {
                        self.loops
                            .get(id)
                            .map(|model| BackendGrabRequest {
                                loop_id: model.backend_id,
                                reverse_start_cycle: Some(reverse_cycle_start as i32),
                                cycles_length: Some(cycles_length as i32),
                                go_to_cycle: Some(go_to_cycle as i32),
                                go_to_mode: backend_loop_mode(go_to_mode),
                            })
                            .ok_or_else(|| format!("stale or unknown loop {id}"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                backend
                    .grab_loops(&requests)
                    .map_err(|error| format!("could not adopt ringbuffers: {error}"))
            }
            ControlOperation::ComposeAddToEnd {
                target,
                add,
                parallel,
            } => {
                if add.is_empty() {
                    return Ok(());
                }
                if add.iter().any(|source| *source == target) {
                    return Err("a loop cannot be composed into itself".to_owned());
                }
                for source in &add {
                    if !self.loops.contains_key(source) {
                        return Err(format!("stale or unknown composition source {source}"));
                    }
                }
                let target_loop = self
                    .loops
                    .get_mut(&target)
                    .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
                if parallel && !target_loop.script_composition.is_empty() {
                    target_loop
                        .script_composition
                        .last_mut()
                        .unwrap()
                        .extend(add);
                } else if parallel {
                    target_loop.script_composition.push(add);
                } else {
                    target_loop
                        .script_composition
                        .extend(add.into_iter().map(|source| vec![source]));
                }
                let sections = target_loop.script_composition.clone();
                let length = sections
                    .iter()
                    .map(|section| {
                        section
                            .iter()
                            .filter_map(|source| self.loops.get(source))
                            .map(|source| source.length)
                            .max()
                            .unwrap_or(0)
                    })
                    .sum();
                let target_loop = self.loops.get_mut(&target).unwrap();
                target_loop.length = length;
                target_loop.state.empty = false;
                target_loop.state.composite_kind = shoop_app_api::CompositeKind::Regular;
                Ok(())
            }
            ControlOperation::SetRepeatSync { loops, active } => {
                let sync = active.then(|| self.sync_backend_loop()).flatten();
                for id in loops {
                    let backend_id = self
                        .loops
                        .get(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?
                        .backend_id;
                    backend
                        .set_loop_sync_source(backend_id, sync)
                        .map_err(|error| {
                            format!("could not update repeat sync for loop {id}: {error}")
                        })?;
                }
                Ok(())
            }
            ControlOperation::SetTrackGain { tracks, gain_db } => self.apply_script_track_action(
                backend,
                tracks,
                TrackAction::OutputGainChanged(gain_db),
            ),
            ControlOperation::SetTrackBalance { tracks, balance } => self
                .apply_script_track_action(
                    backend,
                    tracks,
                    TrackAction::OutputBalanceChanged(balance),
                ),
            ControlOperation::SetTrackMuted { tracks, muted } => self.apply_script_track_action(
                backend,
                tracks,
                TrackAction::OutputMuteChanged(muted),
            ),
            ControlOperation::SetTrackInputGain { tracks, gain_db } => self
                .apply_script_track_action(backend, tracks, TrackAction::InputGainChanged(gain_db)),
            ControlOperation::SetTrackInputMuted {
                tracks,
                muted,
                respect_auto_mute,
            } => self.handle_track_input_monitoring(backend, &tracks, !muted, respect_auto_mute),
            ControlOperation::SetApplyNCycles(value) => {
                self.handle_global_action(backend, GlobalControlAction::SetApplyNCycles(value))
            }
            ControlOperation::SetSolo(value) => {
                self.handle_global_action(backend, GlobalControlAction::SetSolo(value))
            }
            ControlOperation::SetSyncActive(value) => {
                self.handle_global_action(backend, GlobalControlAction::SetSync(value))
            }
            ControlOperation::SetPlayAfterRecord(value) => {
                self.handle_global_action(backend, GlobalControlAction::SetPlayAfterRecord(value))
            }
            ControlOperation::SetAutoMuteOtherTrackInputs(value) => self.handle_global_action(
                backend,
                GlobalControlAction::SetAutoMuteOtherTrackInputs(value),
            ),
            ControlOperation::SetDefaultRecordingAction(value) => self.handle_global_action(
                backend,
                GlobalControlAction::SetDefaultRecordingAction(value),
            ),
        }
    }

    fn apply_script_track_action(
        &mut self,
        backend: &mut dyn Backend,
        tracks: Vec<TrackId>,
        action: TrackAction,
    ) -> Result<(), String> {
        for id in tracks {
            self.handle_track_action(backend, id, action.clone())?;
        }
        Ok(())
    }

    fn handle_track_input_monitoring(
        &mut self,
        backend: &mut dyn Backend,
        tracks: &[TrackId],
        enabled: bool,
        respect_auto_mute: bool,
    ) -> Result<(), String> {
        let targets = tracks.iter().copied().collect::<BTreeSet<_>>();
        if targets.is_empty() {
            return Ok(());
        }
        for target in &targets {
            if !self.tracks.iter().any(|track| track.id == *target) {
                return Err(format!("stale or unknown track {target}"));
            }
        }
        if enabled && respect_auto_mute && self.global.auto_mute_other_track_inputs {
            for track in self
                .tracks
                .iter()
                .filter(|track| !targets.contains(&track.id))
            {
                backend
                    .set_track_control(
                        track.backend_id,
                        BackendTrackControl::InputMonitoring(false),
                    )
                    .map_err(|error| format!("could not update track {}: {error}", track.id))?;
            }
        }
        for track in self
            .tracks
            .iter()
            .filter(|track| targets.contains(&track.id))
        {
            backend
                .set_track_control(
                    track.backend_id,
                    BackendTrackControl::InputMonitoring(enabled),
                )
                .map_err(|error| format!("could not update track {}: {error}", track.id))?;
        }
        Ok(())
    }

    fn script_trigger_loops(
        &mut self,
        backend: &mut dyn Backend,
        loops: &[LoopId],
        mode: LoopMode,
    ) -> Result<(), String> {
        let delay = self.global.sync.then_some(self.target_delay());
        let backend_mode = backend_loop_mode(mode);
        let sync_length = self.sync_length().max(1);
        let mut expanded = Vec::new();
        for id in loops {
            let composition = self
                .loops
                .get(id)
                .ok_or_else(|| format!("stale or unknown loop {id}"))?
                .script_composition
                .clone();
            if composition.is_empty() {
                expanded.push((*id, delay));
                continue;
            }
            if mode == LoopMode::Stopped {
                self.script_composition_playback.remove(id);
                for source in composition.iter().flatten() {
                    expanded.push((*source, None));
                }
                continue;
            }
            if !self.global.sync
                && matches!(mode, LoopMode::Playing | LoopMode::PlayingDryThroughWet)
            {
                self.script_composition_playback.remove(id);
                let all_sources = composition
                    .iter()
                    .flatten()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                let first_sources = composition[0]
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>();
                for source in all_sources.difference(&first_sources) {
                    let backend_id = self
                        .loops
                        .get(source)
                        .ok_or_else(|| format!("stale composition source {source}"))?
                        .backend_id;
                    backend
                        .transition_loop(backend_id, BackendLoopMode::Stopped, None)
                        .map_err(|error| {
                            format!("could not stop inactive composition source {source}: {error}")
                        })?;
                }
                for source in &composition[0] {
                    expanded.push((*source, None));
                }
                if composition.len() > 1 {
                    self.script_composition_playback.insert(
                        *id,
                        ScriptCompositionPlayback {
                            section: 0,
                            remaining_frames: self
                                .script_composition_section_length(&composition[0])
                                .max(1),
                            mode,
                        },
                    );
                }
                continue;
            }
            let mut section_delay = delay.unwrap_or(0);
            for section in &composition {
                for source in section {
                    expanded.push((*source, self.global.sync.then_some(section_delay)));
                }
                let section_length = self
                    .script_composition_section_length(section)
                    .try_into()
                    .unwrap_or(u32::MAX);
                section_delay =
                    section_delay.saturating_add(section_length.max(1).div_ceil(sync_length));
            }
        }
        for (id, loop_delay) in expanded.iter().copied() {
            let model = self
                .loops
                .get(&id)
                .ok_or_else(|| format!("stale or unknown loop {id}"))?;
            backend
                .transition_loop(model.backend_id, backend_mode, loop_delay)
                .map_err(|error| format!("could not trigger loop {id}: {error}"))?;
            if mode == LoopMode::Recording && self.global.apply_n_cycles > 0 {
                let finish = if self.global.play_after_record {
                    BackendLoopMode::Playing
                } else {
                    BackendLoopMode::Stopped
                };
                backend
                    .transition_loop(
                        model.backend_id,
                        finish,
                        Some(
                            loop_delay
                                .unwrap_or(0)
                                .saturating_add(self.global.apply_n_cycles),
                        ),
                    )
                    .map_err(|error| {
                        format!("could not schedule recording end for {id}: {error}")
                    })?;
            }
        }
        if self.global.solo
            && matches!(
                mode,
                LoopMode::Playing | LoopMode::PlayingDryThroughWet | LoopMode::Recording
            )
        {
            let expanded_ids = expanded.iter().map(|(id, _)| *id).collect::<Vec<_>>();
            let target_tracks: Vec<_> = expanded_ids
                .iter()
                .filter_map(|id| self.loops.get(id).map(|model| model.track_id))
                .collect();
            for model in self.loops.values() {
                if target_tracks.contains(&model.track_id) && !expanded_ids.contains(&model.id) {
                    backend
                        .transition_loop(model.backend_id, BackendLoopMode::Stopped, delay)
                        .map_err(|error| {
                            format!("could not solo-stop loop {}: {error}", model.id)
                        })?;
                }
            }
        }
        Ok(())
    }

    fn script_composition_section_length(&self, section: &[LoopId]) -> u64 {
        section
            .iter()
            .filter_map(|source| self.loops.get(source))
            .map(|source| u64::from(source.length))
            .max()
            .unwrap_or(0)
    }

    fn advance_script_compositions(
        &mut self,
        backend: &mut dyn Backend,
        elapsed: Duration,
    ) -> Result<(), String> {
        if self.script_composition_playback.is_empty() {
            self.script_composition_frame_remainder = 0;
            return Ok(());
        }
        let scaled = elapsed
            .as_nanos()
            .saturating_mul(u128::from(self.status.sample_rate))
            .saturating_add(self.script_composition_frame_remainder);
        let frames = (scaled / 1_000_000_000).min(u128::from(u64::MAX)) as u64;
        self.script_composition_frame_remainder = scaled % 1_000_000_000;
        if frames == 0 {
            return Ok(());
        }

        let targets = self
            .script_composition_playback
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for target in targets {
            let Some(composition) = self
                .loops
                .get(&target)
                .map(|model| model.script_composition.clone())
            else {
                self.script_composition_playback.remove(&target);
                continue;
            };
            let Some(playback) = self.script_composition_playback.get(&target) else {
                continue;
            };
            if composition.len() < 2 || playback.section >= composition.len() {
                self.script_composition_playback.remove(&target);
                continue;
            }
            if frames < playback.remaining_frames {
                self.script_composition_playback
                    .get_mut(&target)
                    .unwrap()
                    .remaining_frames -= frames;
                continue;
            }

            // Advance at most one section per control pump. Catch-up bursts would collapse
            // multiple serial sections into one backend callback after an actor stall.
            let previous_section = playback.section;
            let next_section = (previous_section + 1) % composition.len();
            let mode = playback.mode;
            let next_length = self
                .script_composition_section_length(&composition[next_section])
                .max(1);
            let next_sources = composition[next_section]
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>();
            for source in composition[previous_section]
                .iter()
                .filter(|source| !next_sources.contains(source))
            {
                let backend_id = self
                    .loops
                    .get(source)
                    .ok_or_else(|| format!("stale composition source {source}"))?
                    .backend_id;
                backend
                    .transition_loop(backend_id, BackendLoopMode::Stopped, None)
                    .map_err(|error| {
                        format!("could not stop completed composition source {source}: {error}")
                    })?;
            }
            for source in &composition[next_section] {
                let backend_id = self
                    .loops
                    .get(source)
                    .ok_or_else(|| format!("stale composition source {source}"))?
                    .backend_id;
                backend
                    .transition_loop(backend_id, backend_loop_mode(mode), None)
                    .map_err(|error| {
                        format!("could not start composition source {source}: {error}")
                    })?;
            }
            let playback = self.script_composition_playback.get_mut(&target).unwrap();
            playback.section = next_section;
            playback.remaining_frames = next_length;
        }
        Ok(())
    }

    fn refresh_scripting_view(&mut self) {
        self.scripting_view = Arc::new(ScriptingState {
            supported: true,
            scripts: self.script_manager.states().into(),
            dialogs: self.script_manager.dialogs().into(),
            ..Default::default()
        });
        if !self.connection_view.loading {
            self.rebuild_connection_view();
        }
    }

    fn request_audio_driver_switch(
        &mut self,
        backend: &mut dyn Backend,
        config: AudioDriverConfig,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        if matches!(
            self.audio_drivers.switch.status,
            AudioDriverSwitchStatus::AwaitingConfirmation
                | AudioDriverSwitchStatus::Switching
                | AudioDriverSwitchStatus::Resampling
                | AudioDriverSwitchStatus::Restoring
        ) {
            return Err("another audio-driver switch is active".to_owned());
        }
        let source = self
            .audio_drivers
            .active
            .clone()
            .ok_or_else(|| "the active audio driver is unavailable".to_owned())?;
        let target = backend
            .preflight_audio_driver(&config)
            .map_err(|error| format!("could not prepare audio driver: {error}"))?;
        if source.configured == target.configured
            && source.sample_rate == target.sample_rate
            && source.buffer_size == target.buffer_size
        {
            return Err("the requested audio-driver configuration is already active".to_owned());
        }
        let request_id = self.next_audio_switch_id;
        self.next_audio_switch_id = self.next_audio_switch_id.saturating_add(1);
        let rate_message = if source.sample_rate == target.sample_rate {
            format!("Sample rate remains {} Hz.", source.sample_rate)
        } else {
            format!(
                "Sample rate changes from {} Hz to {} Hz. All loop audio, MIDI timing, lengths, offsets, preplay, ring-buffer durations, and cycle timing will be resampled.",
                source.sample_rate, target.sample_rate
            )
        };
        self.pending_audio_switch = Some(PendingAudioSwitch {
            target: target.clone(),
        });
        self.audio_drivers.switch = AudioDriverSwitchState {
            request_id,
            status: AudioDriverSwitchStatus::AwaitingConfirmation,
            source: Some(source.clone()),
            target: Some(target.clone()),
            message: format!(
                "Switch {} ({}) to {} ({})? Audio processing and current transport activity will be interrupted. {rate_message}",
                source.configured.kind().label(),
                source.instance_name,
                target.configured.kind().label(),
                target.instance_name,
            ),
            persistence_retry_available: false,
        };
        Ok(())
    }

    fn confirm_audio_driver_switch(
        &mut self,
        backend: &mut dyn Backend,
        request_id: u64,
        accept: bool,
    ) -> Result<(), String> {
        if self.audio_drivers.switch.request_id != request_id
            || self.audio_drivers.switch.status != AudioDriverSwitchStatus::AwaitingConfirmation
        {
            return Err(format!("stale audio-driver switch request {request_id}"));
        }
        let Some(pending) = self.pending_audio_switch.take() else {
            return Err("audio-driver switch preparation is missing".to_owned());
        };
        if !accept {
            self.audio_drivers.switch = AudioDriverSwitchState {
                request_id,
                status: AudioDriverSwitchStatus::Completed,
                message: "Audio-driver switch cancelled; runtime and settings are unchanged."
                    .to_owned(),
                ..Default::default()
            };
            return Ok(());
        }
        if let Err(error) = self.ensure_io_idle() {
            self.pending_audio_switch = Some(pending);
            return Err(error);
        }
        let source = self.audio_drivers.switch.source.clone();
        if let Err(error) = self.handle_piano_action(backend, PianoAction::ReleaseAll) {
            self.notify_error(error);
        }
        self.audio_drivers.switch.status = AudioDriverSwitchStatus::Switching;
        self.audio_drivers.switch.message = "Capturing the current session".to_owned();
        let capture = match backend.capture_session() {
            Ok(capture) => capture,
            Err(error) => {
                let message = format!("could not capture session before driver switch: {error}");
                self.fail_audio_driver_switch(request_id, source, pending.target, &message);
                return Err(message);
            }
        };
        let source_bundle = match self.session_bundle_from_backend(&capture) {
            Ok(bundle) => bundle,
            Err(error) => {
                self.fail_audio_driver_switch(request_id, source, pending.target, &error);
                return Err(error);
            }
        };
        let target_rate = pending.target.sample_rate;
        let bundle = if source_bundle.document.sample_rate == target_rate {
            source_bundle.clone()
        } else {
            self.audio_drivers.switch.status = AudioDriverSwitchStatus::Resampling;
            self.audio_drivers.switch.message = format!(
                "Resampling all loop contents from {} Hz to {target_rate} Hz",
                source_bundle.document.sample_rate
            );
            match resample_session(&source_bundle, target_rate) {
                Ok(bundle) => bundle,
                Err(error) => {
                    let message = format!("could not resample session: {error}");
                    self.fail_audio_driver_switch(request_id, source, pending.target, &message);
                    return Err(message);
                }
            }
        };
        let backend_data = match session_bundle_to_backend(&bundle, &self.track_processors) {
            Ok(data) => data,
            Err(error) => {
                self.fail_audio_driver_switch(request_id, source, pending.target, &error);
                return Err(error);
            }
        };
        self.audio_drivers.switch.status = AudioDriverSwitchStatus::Switching;
        self.audio_drivers.switch.message =
            "Starting the target driver and restoring session".to_owned();
        let replacement = match backend.switch_audio_driver(
            &pending.target.configured,
            target_rate,
            &backend_data,
        ) {
            Ok(replacement) => replacement,
            Err(error) => {
                if error
                    .to_string()
                    .contains("resolved target sample rate changed")
                {
                    if let Ok(target) = backend.preflight_audio_driver(&pending.target.configured) {
                        let source = backend
                            .audio_driver_state()
                            .ok()
                            .and_then(|runtime| runtime.active);
                        let rate_message = source.as_ref().map_or_else(
                            || format!("Target sample rate is now {} Hz.", target.sample_rate),
                            |source| {
                                format!(
                                    "Sample rate is now resolved as {} Hz → {} Hz. All loop contents will be resampled after confirmation.",
                                    source.sample_rate, target.sample_rate
                                )
                            },
                        );
                        self.pending_audio_switch = Some(PendingAudioSwitch {
                            target: target.clone(),
                        });
                        self.audio_drivers.switch = AudioDriverSwitchState {
                            request_id,
                            status: AudioDriverSwitchStatus::AwaitingConfirmation,
                            source,
                            target: Some(target),
                            message: format!(
                                "The target sample rate changed during preparation. Confirm again. {rate_message}"
                            ),
                            persistence_retry_available: false,
                        };
                        return Ok(());
                    }
                }
                let message = format!("audio-driver switch failed: {error}");
                let runtime = backend.audio_driver_state().unwrap_or_default();
                let fatal = runtime.active.is_none();
                self.audio_drivers = runtime;
                self.audio_drivers.switch = AudioDriverSwitchState {
                    request_id,
                    status: if fatal {
                        AudioDriverSwitchStatus::Fatal
                    } else {
                        AudioDriverSwitchStatus::Failed
                    },
                    source,
                    target: Some(pending.target),
                    message: message.clone(),
                    persistence_retry_available: false,
                };
                return Err(message);
            }
        };
        if let Err(error) = self.apply_loaded_session(backend, &bundle, &replacement) {
            self.audio_drivers.switch.status = AudioDriverSwitchStatus::Restoring;
            self.audio_drivers.switch.message = "Restoring the prior audio driver".to_owned();
            let rollback = source.as_ref().ok_or_else(|| {
                "could not restore switched session because source driver state is missing"
                    .to_owned()
            });
            let rollback = rollback.and_then(|source| {
                backend
                    .switch_audio_driver(&source.configured, source.sample_rate, &capture)
                    .map_err(|rollback_error| rollback_error.to_string())
                    .and_then(|mapping| {
                        self.apply_loaded_session(backend, &source_bundle, &mapping)
                    })
            });
            let message = match rollback {
                Ok(()) => format!(
                    "could not remap switched session: {error}; the prior driver was restored"
                ),
                Err(rollback_error) => {
                    let message = format!(
                        "could not remap switched session: {error}; restoring the prior driver failed: {rollback_error}"
                    );
                    self.audio_drivers.switch = AudioDriverSwitchState {
                        request_id,
                        status: AudioDriverSwitchStatus::Fatal,
                        source,
                        target: Some(pending.target),
                        message: message.clone(),
                        persistence_retry_available: false,
                    };
                    return Err(message);
                }
            };
            self.fail_audio_driver_switch(request_id, source, pending.target, &message);
            return Err(message);
        }
        let runtime = backend.audio_driver_state().unwrap_or_default();
        self.audio_drivers = runtime;
        self.audio_drivers.switch = AudioDriverSwitchState {
            request_id,
            status: AudioDriverSwitchStatus::Persisting,
            source,
            target: Some(pending.target),
            message: "Audio driver switched; saving the preferred configuration".to_owned(),
            persistence_retry_available: false,
        };
        Ok(())
    }

    fn complete_audio_driver_persistence(
        &mut self,
        request_id: u64,
        success: bool,
        message: String,
    ) -> Result<(), String> {
        if self.audio_drivers.switch.request_id != request_id
            || !matches!(
                self.audio_drivers.switch.status,
                AudioDriverSwitchStatus::Persisting | AudioDriverSwitchStatus::Failed
            )
        {
            return Err(format!(
                "stale audio-driver persistence result {request_id}"
            ));
        }
        self.audio_drivers.switch.status = if success {
            AudioDriverSwitchStatus::Completed
        } else {
            AudioDriverSwitchStatus::Failed
        };
        self.audio_drivers.switch.message = message;
        self.audio_drivers.switch.persistence_retry_available = !success;
        Ok(())
    }

    fn fail_audio_driver_switch(
        &mut self,
        request_id: u64,
        source: Option<shoop_app_api::ResolvedAudioDriverConfig>,
        target: shoop_app_api::ResolvedAudioDriverConfig,
        message: &str,
    ) {
        self.audio_drivers.switch = AudioDriverSwitchState {
            request_id,
            status: AudioDriverSwitchStatus::Failed,
            source,
            target: Some(target),
            message: message.to_owned(),
            persistence_retry_available: false,
        };
    }

    fn preview_click_track(
        &mut self,
        loop_id: LoopId,
        request: ClickTrackRequest,
    ) -> Result<(), String> {
        self.validate_click_track_target(loop_id, ClickTrackKind::Audio)?;
        if request.kind != ClickTrackKind::Audio {
            return Err("MIDI click tracks cannot be previewed as audio".to_owned());
        }
        let request_id = self.next_preview_request_id;
        self.next_preview_request_id = self.next_preview_request_id.wrapping_add(1).max(1);
        let result = generate_audio_click_track(
            &AudioClickTrackSpec {
                timing: click_timing_spec(&request),
                primary_sound: request.primary_sound_id,
                secondary_sound: request.secondary_sound_id,
                secondary_clicks_per_primary: request.secondary_clicks_per_primary,
            },
            self.status.sample_rate,
        )
        .map_err(|error| error.to_string());
        let audio = match result {
            Ok(audio) => audio,
            Err(message) => {
                self.click_track.preview_request_id = request_id;
                self.click_track.preview_status = ClickTrackPreviewStatus::Failed;
                self.click_track.preview_message.clone_from(&message);
                return Err(message);
            }
        };
        let samples = audio
            .channels
            .into_iter()
            .next()
            .map(|channel| channel.samples)
            .ok_or_else(|| "generated preview contains no audio".to_owned())?;
        let mut outputs = self
            .preview_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        while outputs.len() >= PREVIEW_OUTPUT_CAPACITY {
            outputs.pop_front();
        }
        outputs.push_back(ApplicationAudioPreview {
            request_id,
            sample_rate: audio.sample_rate,
            samples: samples.into(),
        });
        self.click_track.preview_request_id = request_id;
        self.click_track.preview_status = ClickTrackPreviewStatus::Queued;
        self.click_track.preview_message = "Click preview queued".to_owned();
        Ok(())
    }

    fn complete_click_track_preview(
        &mut self,
        request_id: u64,
        success: bool,
        message: String,
    ) -> Result<(), String> {
        if request_id != self.click_track.preview_request_id {
            return Ok(());
        }
        self.click_track.preview_status = if success {
            ClickTrackPreviewStatus::Completed
        } else {
            ClickTrackPreviewStatus::Failed
        };
        self.click_track.preview_message = message.clone();
        if success {
            Ok(())
        } else {
            Err(message)
        }
    }

    fn begin_generate_click_track(
        &mut self,
        loop_id: LoopId,
        request: ClickTrackRequest,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        self.validate_click_track_target(loop_id, request.kind)?;
        if self.loops.values().any(|loop_| {
            matches!(
                loop_.state.mode,
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            )
        }) {
            return Err("click generation is unavailable while recording or replacing".to_owned());
        }
        self.start_io_task(IoTaskKind::GenerateClickTrack, "Generating click track");
        let pending = match request.kind {
            ClickTrackKind::Audio => generate_audio_click_track(
                &AudioClickTrackSpec {
                    timing: click_timing_spec(&request),
                    primary_sound: request.primary_sound_id,
                    secondary_sound: request.secondary_sound_id,
                    secondary_clicks_per_primary: request.secondary_clicks_per_primary,
                },
                self.status.sample_rate,
            )
            .map(|audio| PendingIo::PrepareGeneratedClickAudio { loop_id, audio }),
            ClickTrackKind::Midi => generate_midi_click_track(
                MidiClickTrackSpec {
                    timing: click_timing_spec(&request),
                    note: request.midi_note,
                    channel: 0,
                    velocity: 127,
                    note_length_seconds: request.midi_note_length_seconds,
                },
                self.status.sample_rate,
            )
            .map(|midi| PendingIo::PrepareGeneratedClickMidi { loop_id, midi }),
        };
        match pending {
            Ok(pending) => {
                self.pending_io = Some(pending);
                self.set_io_progress(0.35, "Preparing generated click media");
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                self.finish_io(IoTaskStatus::Failed, &message);
                Err(message)
            }
        }
    }

    fn validate_click_track_target(
        &self,
        loop_id: LoopId,
        kind: ClickTrackKind,
    ) -> Result<(), String> {
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        if model.state.composite_kind != shoop_app_api::CompositeKind::None {
            return Err("click tracks require a primitive loop".to_owned());
        }
        let supported = match kind {
            ClickTrackKind::Audio => model.state.has_audio,
            ClickTrackKind::Midi => model.state.has_midi,
        };
        if supported {
            Ok(())
        } else {
            Err(format!(
                "target loop has no {} channels",
                match kind {
                    ClickTrackKind::Audio => "audio",
                    ClickTrackKind::Midi => "MIDI",
                }
            ))
        }
    }

    fn begin_save_session(&mut self) -> Result<(), String> {
        self.ensure_io_idle()?;
        if self.loops.values().any(|loop_| {
            matches!(
                loop_.state.mode,
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            )
        }) {
            return Err(
                "session save is waiting for active recording/replacement content to settle"
                    .to_owned(),
            );
        }
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
        let bundle = match decode_session(bytes) {
            Ok(bundle) => bundle,
            Err(error) => {
                let message = error.to_string();
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        };
        if let Err(error) =
            ScriptManager::validate_session_scripts(&session_script_sources(&bundle))
        {
            let message = error.to_string();
            self.finish_io(IoTaskStatus::Failed, &message);
            return Err(message);
        }
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
        let backend_data = match session_bundle_to_backend(&bundle, &self.track_processors) {
            Ok(backend_data) => backend_data,
            Err(message) => {
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        };
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
        let result = match pending {
            PendingIo::AwaitingSessionLoad { name, bundle } => {
                resample_session(&bundle, self.status.sample_rate)
                    .map_err(|error| error.to_string())
                    .and_then(|bundle| {
                        let backend_data =
                            session_bundle_to_backend(&bundle, &self.track_processors)?;
                        self.pending_io = Some(PendingIo::CommitSessionLoad {
                            name,
                            bundle,
                            backend_data,
                        });
                        self.set_io_progress(0.7, "Staging session");
                        Ok(())
                    })
            }
            PendingIo::AwaitingLoopAudioImport {
                loop_id,
                audio,
                update_loop_length,
            } => resample_loop_audio(&audio, self.status.sample_rate)
                .map_err(|error| error.to_string())
                .and_then(|audio| {
                    self.begin_audio_channel_mapping(loop_id, audio, update_loop_length)
                }),
            PendingIo::AwaitingLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            } => resample_exact_midi(&midi, self.status.sample_rate)
                .map_err(|error| error.to_string())
                .map(|midi| {
                    self.pending_io = Some(PendingIo::PrepareLoopMidiImport {
                        loop_id,
                        midi,
                        update_loop_length,
                    });
                }),
            other => {
                self.pending_io = Some(other);
                return Err("I/O task is not awaiting sample-rate confirmation".to_owned());
            }
        };
        if let Err(message) = &result {
            self.finish_io(IoTaskStatus::Failed, message);
        }
        result
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
        let track = self
            .tracks
            .iter()
            .find(|track| track.id == loop_model.track_id)
            .ok_or_else(|| "target loop track is unavailable".to_owned())?;
        let destination_channels = audio_channel_labels(&track.topology, track.audio_channels);
        let destinations = destination_channels.len();
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
                destination_channels,
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
            } => match self.prepare_loop_audio_import(loop_id, &audio, update_loop_length) {
                Ok(update) => {
                    self.pending_io = Some(PendingIo::CommitLoopImport {
                        loop_id,
                        update,
                        message: "Loop audio imported".to_owned(),
                    });
                    self.set_io_progress(0.75, "Committing loop audio");
                }
                Err(error) => self.fail_io(error),
            },
            PendingIo::PrepareLoopMidiImport {
                loop_id,
                midi,
                update_loop_length,
            } => match self.prepare_loop_midi_import(loop_id, &midi, update_loop_length) {
                Ok(update) => {
                    self.pending_io = Some(PendingIo::CommitLoopImport {
                        loop_id,
                        update,
                        message: "Loop MIDI imported".to_owned(),
                    });
                    self.set_io_progress(0.75, "Committing loop MIDI");
                }
                Err(error) => self.fail_io(error),
            },
            PendingIo::PrepareGeneratedClickAudio { loop_id, audio } => {
                match self.prepare_generated_click_audio(loop_id, &audio) {
                    Ok(update) => {
                        self.pending_io = Some(PendingIo::CommitLoopImport {
                            loop_id,
                            update,
                            message: "Audio click track generated".to_owned(),
                        });
                        self.set_io_progress(0.75, "Committing generated click track");
                    }
                    Err(error) => self.fail_io(error),
                }
            }
            PendingIo::PrepareGeneratedClickMidi { loop_id, midi } => {
                match self.prepare_generated_click_midi(loop_id, &midi) {
                    Ok(update) => {
                        self.pending_io = Some(PendingIo::CommitLoopImport {
                            loop_id,
                            update,
                            message: "MIDI click track generated".to_owned(),
                        });
                        self.set_io_progress(0.75, "Committing generated click track");
                    }
                    Err(error) => self.fail_io(error),
                }
            }
            PendingIo::CommitLoopImport {
                loop_id,
                update,
                message,
            } => {
                let backend_loop = self.loops.get(&loop_id).map(|model| model.backend_id);
                match backend_loop
                    .ok_or_else(|| anyhow!("stale loop {loop_id}"))
                    .and_then(|backend_loop| backend.replace_loop_content(backend_loop, &update))
                {
                    Ok(()) => {
                        if let Some(model) = self.loops.get_mut(&loop_id) {
                            if let Some(length) = update.length {
                                model.length = length;
                            }
                            model.state.mode = LoopMode::Stopped;
                            model.state.next_mode = LoopMode::Unknown;
                            model.state.next_transition_delay = None;
                            model.state.empty = false;
                            model.audio_data = None;
                            model.midi_data = None;
                        }
                        self.finish_io(IoTaskStatus::Completed, &message);
                    }
                    Err(error) if io_pending_error(&error.to_string()) => {
                        self.pending_io = Some(PendingIo::CommitLoopImport {
                            loop_id,
                            update,
                            message,
                        });
                    }
                    Err(error) => self.fail_io(format!("could not commit loop import: {error}")),
                }
            }
        }
    }

    fn add_track(
        &mut self,
        backend: &mut dyn Backend,
        spec: DirectTrackSpec,
    ) -> Result<(), String> {
        self.add_track_spec(backend, spec.into())
    }

    fn add_track_spec(&mut self, backend: &mut dyn Backend, spec: TrackSpec) -> Result<(), String> {
        spec.validate(&self.track_processors)
            .map_err(|error| format!("invalid track: {error:?}"))?;
        let (backend_topology, topology, loop_audio_channels) = match &spec.topology {
            TrackSpecTopology::Direct {
                audio_channels,
                midi,
            } => (
                BackendTrackTopology::Direct {
                    audio_channels: *audio_channels,
                    midi: *midi,
                },
                TrackTopology::Direct,
                *audio_channels,
            ),
            TrackSpecTopology::DryWet {
                dry_audio_channels,
                wet_audio_channels,
                dry_midi,
                processor_type,
            } => {
                let backend_topology =
                    if processor_type.as_str() == shoop_app_api::TrackProcessorTypeId::EXTERNAL {
                        BackendTrackTopology::DryWetExternal {
                            dry_audio_channels: *dry_audio_channels,
                            wet_audio_channels: *wet_audio_channels,
                            dry_midi: *dry_midi,
                        }
                    } else {
                        BackendTrackTopology::DryWetProcessor {
                            processor_type: processor_type.as_str().to_owned(),
                            dry_audio_channels: *dry_audio_channels,
                            wet_audio_channels: *wet_audio_channels,
                            dry_midi: *dry_midi,
                        }
                    };
                (
                    backend_topology,
                    TrackTopology::DryWet {
                        dry_audio_channels: *dry_audio_channels,
                        wet_audio_channels: *wet_audio_channels,
                        dry_midi: *dry_midi,
                        processor_type: processor_type.clone(),
                    },
                    dry_audio_channels.saturating_add(*wet_audio_channels),
                )
            }
        };
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
            .create_track(TrackRequest {
                port_name_base: port_name_base.clone(),
                topology: backend_topology,
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
                loop_audio_channels,
            ));
        }
        self.tracks.push(TrackModel {
            id: track_id,
            backend_id: created.track_id,
            name: spec.name,
            port_name_base,
            is_sync: false,
            audio_channels: loop_audio_channels,
            topology,
            fx: None,
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
        if let TrackAction::InputMonitoringChanged {
            enabled,
            respect_auto_mute,
        } = &action
        {
            return self.handle_track_input_monitoring(
                backend,
                &[track_id],
                *enabled,
                *respect_auto_mute,
            );
        }
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
            TrackAction::InputMonitoringChanged { .. } => unreachable!(),
            TrackAction::FxActiveChanged(value) => {
                return backend
                    .set_track_fx_control(track.backend_id, BackendTrackFxControl::SetActive(value))
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"));
            }
            TrackAction::FxVisibilityChanged(value) => {
                return backend
                    .set_track_fx_control(
                        track.backend_id,
                        BackendTrackFxControl::SetVisible(value),
                    )
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"));
            }
            TrackAction::FxToggleOrRecover => {
                return backend
                    .set_track_fx_control(track.backend_id, BackendTrackFxControl::ToggleOrRecover)
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"));
            }
            TrackAction::FxRestoreState(state) => {
                return backend
                    .set_track_fx_control(
                        track.backend_id,
                        BackendTrackFxControl::RestoreState(state),
                    )
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"));
            }
            TrackAction::FxClearLogs => {
                return backend
                    .set_track_fx_control(track.backend_id, BackendTrackFxControl::ClearLogs)
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"));
            }
            TrackAction::TinySynthFx(control) => {
                return backend
                    .set_track_fx_control(
                        track.backend_id,
                        BackendTrackFxControl::TinySynthFx(control),
                    )
                    .map_err(|error| {
                        format!("could not update Tiny Synth/FX track {track_id}: {error}")
                    });
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
                midi_data: None,
                script_composition: Vec::new(),
                recorded_fx_state: None,
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
                self.refresh_selected_media(backend)?;
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
                self.refresh_selected_media(backend)?;
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
            LoopAction::RestoreRecordedFxState => {
                let recorded = loop_model
                    .recorded_fx_state
                    .clone()
                    .ok_or_else(|| format!("loop {loop_id} has no recorded FX state"))?;
                let track = self
                    .tracks
                    .iter()
                    .find(|track| track.id == track_id)
                    .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
                let TrackTopology::DryWet { processor_type, .. } = &track.topology else {
                    return Err(format!("track {track_id} has no compatible processor"));
                };
                if *processor_type != recorded.processor_type {
                    return Err("recorded FX state belongs to another processor".to_owned());
                }
                backend
                    .set_track_fx_control(
                        track.backend_id,
                        BackendTrackFxControl::RestoreState(recorded.state),
                    )
                    .map_err(|error| format!("could not restore recorded FX state: {error}"))
            }
        }
    }

    fn capture_recording_fx_states(
        &mut self,
        backend: &mut dyn Backend,
        loop_ids: &[LoopId],
    ) -> Result<(), String> {
        let mut captured = Vec::new();
        for loop_id in loop_ids {
            let track_id = self
                .loops
                .get(loop_id)
                .ok_or_else(|| format!("stale loop {loop_id}"))?
                .track_id;
            let track = self
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .ok_or_else(|| format!("stale track {track_id}"))?;
            let TrackTopology::DryWet { processor_type, .. } = &track.topology else {
                continue;
            };
            if processor_type.as_str() == shoop_app_api::TrackProcessorTypeId::EXTERNAL {
                continue;
            }
            let state = backend
                .track_fx_state_string(track.backend_id)
                .map_err(|error| format!("could not capture processor state: {error}"))?
                .ok_or_else(|| "processed track returned no processor state".to_owned())?;
            captured.push((
                *loop_id,
                RecordedFxState {
                    processor_type: processor_type.clone(),
                    state,
                },
            ));
        }
        for (loop_id, state) in captured {
            let model = self.loops.get_mut(&loop_id).expect("loop was checked");
            model.recorded_fx_state = Some(state);
            model.state.has_recorded_fx_state = true;
        }
        Ok(())
    }

    fn refresh_selected_media(&mut self, backend: &mut dyn Backend) -> Result<(), String> {
        let selected: Vec<_> = self
            .loops
            .values()
            .filter(|model| model.state.selected)
            .map(|model| {
                (
                    model.id,
                    model.backend_id,
                    model.state.has_audio,
                    model.state.has_midi,
                )
            })
            .collect();
        for model in self.loops.values_mut() {
            if !model.state.selected {
                model.audio_data = None;
                model.midi_data = None;
            }
        }
        let [(id, backend_id, has_audio, has_midi)] = selected.as_slice() else {
            return Ok(());
        };
        let (needs_audio, needs_midi) = self
            .loops
            .get(id)
            .map(|model| (model.audio_data.is_none(), model.midi_data.is_none()))
            .unwrap_or_default();
        if needs_audio {
            if *has_audio {
                let data = backend
                    .loop_audio_data(*backend_id)
                    .map_err(|error| format!("could not fetch selected loop audio: {error}"))?;
                if let (Some(model), Some(data)) = (self.loops.get_mut(id), data) {
                    model.audio_data = Some(data);
                }
            } else if let Some(model) = self.loops.get_mut(id) {
                model.audio_data = Some(Vec::new());
            }
        }
        if needs_midi {
            if *has_midi {
                let data = backend
                    .loop_midi_data(*backend_id)
                    .map_err(|error| format!("could not fetch selected loop MIDI: {error}"))?;
                if let Some(data) = data {
                    let Some(model) = self.loops.get(id) else {
                        return Ok(());
                    };
                    let channels = midi_detail_channels(model, data);
                    if let Some(model) = self.loops.get_mut(id) {
                        model.midi_data = Some(channels);
                    }
                }
            } else if let Some(model) = self.loops.get_mut(id) {
                model.midi_data = Some(Vec::new());
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
        let ids = self.action_target_ids(initiating_loop);
        self.capture_recording_fx_states(backend, &ids)?;
        for id in ids {
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
        self.capture_recording_fx_states(backend, &ids)?;
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
        for id in &ids {
            if let Some(model) = self.loops.get_mut(id) {
                model.audio_data = None;
                model.midi_data = None;
            }
        }

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
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            let ids = targets.iter().map(|(id, _, _)| *id).collect::<Vec<_>>();
            self.capture_recording_fx_states(backend, &ids)?;
        }
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
        if self.connection_view.application_ports.iter().any(|port| {
            port.id == port_id && port.connection_policy == ConnectionPolicy::OwnerManaged
        }) {
            let message = format!("connection policy is managed by the port owner: {port_id}");
            self.push_connection_error(ConnectionErrorState {
                port_id: Some(port_id),
                external_port: Some(external_port),
                kind: ConnectionErrorKind::Incompatible,
                message: message.clone(),
            });
            return Err(message);
        }
        let Some(port) = self.connection_ports.get(&port_id) else {
            let message = format!("stale or unknown application port {port_id}");
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
                    model.midi_data = None;
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
            GlobalControlAction::SetAutoMuteOtherTrackInputs(value) => {
                self.global.auto_mute_other_track_inputs = value;
            }
            GlobalControlAction::SetApplyNCycles(value) => self.global.apply_n_cycles = value,
        }
        Ok(())
    }

    fn apply_backend_snapshot(&mut self, snapshot: BackendSnapshot) {
        let _span = tracing::trace_span!(
            "frontend.app.backend_snapshot_apply",
            callback_count = snapshot.status.callback_count,
            processed_frames = snapshot.status.processed_frames,
            track_count = snapshot.tracks.len(),
            loop_count = snapshot.loops.len()
        )
        .entered();
        let switch = self.audio_drivers.switch.clone();
        self.audio_drivers = snapshot.audio_drivers.clone();
        if switch.status != AudioDriverSwitchStatus::Idle {
            self.audio_drivers.switch = switch;
        }
        self.status.dsp_load_percent = snapshot.status.dsp_load_percent;
        let callback_xruns =
            if snapshot.status.callback_budget_overruns >= self.last_callback_budget_overruns {
                snapshot
                    .status
                    .callback_budget_overruns
                    .saturating_sub(self.last_callback_budget_overruns)
            } else {
                snapshot.status.callback_budget_overruns
            };
        self.last_callback_budget_overruns = snapshot.status.callback_budget_overruns;
        self.status.xruns = self
            .status
            .xruns
            .saturating_add(snapshot.status.xruns)
            .saturating_add(callback_xruns);
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
        self.status.render_discontinuities = snapshot.status.render_discontinuities;
        self.status.memory_growths = snapshot.status.memory_growths;
        let render_memory_growths =
            if snapshot.status.render_memory_growths >= self.status.render_memory_growths {
                snapshot
                    .status
                    .render_memory_growths
                    .saturating_sub(self.status.render_memory_growths)
            } else {
                snapshot.status.render_memory_growths
            };
        self.status.render_memory_growths = snapshot.status.render_memory_growths;
        if render_memory_growths > 0 {
            let callback_label = if render_memory_growths == 1 {
                "callback"
            } else {
                "callbacks"
            };
            self.notify_warning(format!(
                "Audio recovered after memory grew during {render_memory_growths} render {callback_label}; timing may have been disrupted"
            ));
        }
        self.status.command_overflows = snapshot.status.command_overflows;
        self.status.storage_low_channels = snapshot.status.storage_low_channels;
        self.status.storage_exhaustions = snapshot.status.storage_exhaustions;
        for track in &mut self.tracks {
            let Some(backend_state) = snapshot.tracks.get(&track.backend_id) else {
                continue;
            };
            let (input_audio_channels, output_audio_channels, input_midi, output_midi) =
                match &backend_state.topology {
                    BackendTrackTopology::Direct {
                        audio_channels,
                        midi,
                    } => (*audio_channels, *audio_channels, *midi, *midi),
                    BackendTrackTopology::DryWetExternal {
                        dry_audio_channels,
                        wet_audio_channels,
                        dry_midi,
                    }
                    | BackendTrackTopology::DryWetProcessor {
                        dry_audio_channels,
                        wet_audio_channels,
                        dry_midi,
                        ..
                    } => (*dry_audio_channels, *wet_audio_channels, *dry_midi, false),
                };
            track.fx.clone_from(&backend_state.fx);
            let controls = &mut track.controls;
            controls.has_output = output_audio_channels > 0 || output_midi;
            controls.has_output_audio = output_audio_channels > 0;
            controls.output_stereo = output_audio_channels == 2;
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
            controls.has_input = input_audio_channels > 0 || input_midi;
            controls.has_input_audio = input_audio_channels > 0;
            controls.input_stereo = input_audio_channels == 2;
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
            controls.latest_input_midi_message =
                backend_state.latest_input_midi_message.map(|message| {
                    shoop_app_api::LatestMidiMessage {
                        bytes: message.bytes,
                        len: message.len,
                    }
                });
            controls.clamp();
        }
        let track_capabilities = self
            .tracks
            .iter()
            .filter_map(|track| {
                snapshot.tracks.get(&track.backend_id).map(|state| {
                    (
                        track.id,
                        (track.audio_channels > 0, state.topology.has_midi()),
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        for model in self.loops.values_mut() {
            let Some(backend_state) = snapshot.loops.get(&model.backend_id) else {
                continue;
            };
            if model.script_composition.is_empty() {
                model.length = backend_state.length;
                model.position = backend_state.position;
            }
            let was_changing = matches!(
                model.state.mode,
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            );
            model.state.mode = app_loop_mode(backend_state.mode);
            let is_changing = matches!(
                model.state.mode,
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            );
            if was_changing && !is_changing {
                model.audio_data = None;
                model.midi_data = None;
            }
            model.state.next_mode = backend_state
                .next_mode
                .map(app_loop_mode)
                .unwrap_or(model.state.mode);
            model.state.next_transition_delay = backend_state.next_transition_delay;
            model.state.empty = model.length == 0;
            model.state.position = if model.length == 0 {
                0.0
            } else {
                model.position as f32 / model.length as f32
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
        let composites = self
            .loops
            .values()
            .filter(|model| !model.script_composition.is_empty())
            .map(|model| (model.id, model.script_composition.clone()))
            .collect::<Vec<_>>();
        for (target, sections) in composites {
            let length = sections
                .iter()
                .map(|section| {
                    section
                        .iter()
                        .filter_map(|source| self.loops.get(source))
                        .map(|source| source.length)
                        .max()
                        .unwrap_or(0)
                })
                .sum();
            let active_section = self
                .script_composition_playback
                .get(&target)
                .map(|playback| playback.section)
                .unwrap_or(0);
            let section_offset = sections
                .iter()
                .take(active_section)
                .map(|section| {
                    section
                        .iter()
                        .filter_map(|source| self.loops.get(source))
                        .map(|source| source.length)
                        .max()
                        .unwrap_or(0)
                })
                .sum::<u32>();
            let source_state = sections
                .get(active_section)
                .and_then(|section| section.first())
                .and_then(|source| self.loops.get(source))
                .map(|source| {
                    (
                        source.state.mode,
                        source.state.next_mode,
                        source.state.next_transition_delay,
                        section_offset.saturating_add(source.position),
                    )
                });
            if let Some(model) = self.loops.get_mut(&target) {
                model.length = length;
                model.state.empty = false;
                if let Some((mode, next_mode, next_transition_delay, position)) = source_state {
                    model.state.mode = mode;
                    model.state.next_mode = next_mode;
                    model.state.next_transition_delay = next_transition_delay;
                    model.position = position.min(length);
                    model.state.position = if length == 0 {
                        0.0
                    } else {
                        model.position as f32 / length as f32
                    };
                }
            }
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
        self.host_ports = snapshot
            .host_ports
            .values()
            .map(|host| {
                (
                    host.id.clone(),
                    HostPortState {
                        id: HostPortId::new(host.id.clone()),
                        name: host.name.clone(),
                        data_type: map_port_data_type(host.data_type),
                        direction: map_port_direction(host.direction),
                    },
                )
            })
            .collect();
        self.confirmed_connections = snapshot
            .confirmed_links
            .iter()
            .filter_map(|link| {
                self.connection_ports
                    .values()
                    .find(|port| port.backend_id == link.application_port_id)
                    .map(|port| (port.id, link.host_port_id.clone()))
            })
            .collect();
        for port in self.connection_ports.values_mut() {
            let backend_present = snapshot.application_ports.contains_key(&port.backend_id);
            port.candidates = if backend_present {
                self.host_ports
                    .iter()
                    .filter(|(_, host)| {
                        host.data_type == port.data_type && host.direction != port.direction
                    })
                    .map(|(host_id, _)| {
                        (
                            host_id.clone(),
                            (
                                true,
                                self.confirmed_connections
                                    .contains(&(port.id, host_id.clone())),
                            ),
                        )
                    })
                    .collect()
            } else {
                BTreeMap::new()
            };
        }
        let pending_keys: Vec<_> = self.pending_connections.keys().cloned().collect();
        for (port_id, host_port_id) in pending_keys {
            let key = (port_id, host_port_id.clone());
            let host_present = self.host_ports.contains_key(&host_port_id);
            let connected = self
                .confirmed_connections
                .contains(&(port_id, host_port_id.clone()));
            let desired = self.pending_connections[&key].desired_connected;
            if host_present && connected == desired {
                self.pending_connections.remove(&key);
            } else if !host_present {
                self.pending_connections.remove(&key);
                self.push_connection_error(ConnectionErrorState {
                    port_id: Some(port_id),
                    external_port: Some(host_port_id.clone()),
                    kind: ConnectionErrorKind::EndpointUnavailable,
                    message: format!("host endpoint disappeared: {host_port_id}"),
                });
            }
        }
        self.rebuild_connection_view();
    }

    fn rebuild_connection_view(&mut self) {
        let mut application_ports: Vec<ApplicationPortState> = self
            .connection_ports
            .values()
            .map(|port| {
                let is_sync = self
                    .tracks
                    .iter()
                    .find(|track| track.id == port.track_id)
                    .is_some_and(|track| track.is_sync);
                ApplicationPortState {
                    id: port.id,
                    owner: ApplicationPortOwner::Track {
                        track_id: port.track_id,
                        kind: if is_sync {
                            TrackPortOwnerKind::Sync
                        } else {
                            TrackPortOwnerKind::Main
                        },
                    },
                    name: port.name.clone(),
                    data_type: port.data_type,
                    direction: port.direction,
                    role: port.role,
                    connection_policy: ConnectionPolicy::UserManaged,
                }
            })
            .collect();
        let mut normalized_hosts = self.host_ports.clone();
        let mut normalized_links: BTreeSet<(PortId, HostPortId)> = self
            .confirmed_connections
            .iter()
            .map(|(port_id, host_id)| (*port_id, HostPortId::new(host_id.clone())))
            .collect();
        for script in self.scripting_view.scripts.iter() {
            for (registration, rule) in script.midi.rule_states.iter().enumerate() {
                let registration = u32::try_from(registration).unwrap_or(u32::MAX);
                let (direction, role, host_direction, host_kind) = match rule.direction {
                    ScriptMidiRuleDirection::Input => (
                        PortDirection::Input,
                        PortRole::MidiInput,
                        PortDirection::Output,
                        "source",
                    ),
                    ScriptMidiRuleDirection::Output => (
                        PortDirection::Output,
                        PortRole::MidiOutput,
                        PortDirection::Input,
                        "sink",
                    ),
                };
                let port_id = script_connection_port_id(script.id, registration);
                application_ports.push(ApplicationPortState {
                    id: port_id,
                    owner: ApplicationPortOwner::LuaControl {
                        script_id: script.id,
                        registration,
                    },
                    name: format!(
                        "{}: MIDI {} {}",
                        script.name,
                        host_kind,
                        registration.saturating_add(1)
                    ),
                    data_type: PortDataType::Midi,
                    direction,
                    role,
                    connection_policy: ConnectionPolicy::OwnerManaged,
                });
                for endpoint in rule.endpoints.iter() {
                    let host_id = script_midi_host_id(host_direction, &endpoint.id);
                    normalized_hosts
                        .entry(host_id.to_string())
                        .or_insert_with(|| HostPortState {
                            id: host_id.clone(),
                            name: endpoint.name.clone(),
                            data_type: PortDataType::Midi,
                            direction: host_direction,
                        });
                    if endpoint.connected {
                        normalized_links.insert((port_id, host_id));
                    }
                }
            }
        }
        application_ports.sort_by(|left, right| {
            (&left.owner, &left.name, left.id).cmp(&(&right.owner, &right.name, right.id))
        });
        let application_ports: Arc<[ApplicationPortState]> = application_ports.into();
        let host_ports: Arc<[HostPortState]> =
            normalized_hosts.into_values().collect::<Vec<_>>().into();
        let confirmed_links: Arc<[ConfirmedConnectionState]> = normalized_links
            .into_iter()
            .map(
                |(application_port_id, host_port_id)| ConfirmedConnectionState {
                    application_port_id,
                    host_port_id,
                },
            )
            .collect::<Vec<_>>()
            .into();
        let pending_links: Arc<[PendingConnectionState]> = self
            .pending_connections
            .iter()
            .map(
                |((application_port_id, host_port_id), pending)| PendingConnectionState {
                    application_port_id: *application_port_id,
                    host_port_id: HostPortId::new(host_port_id.clone()),
                    desired_connected: pending.desired_connected,
                },
            )
            .collect::<Vec<_>>()
            .into();
        let errors: Arc<[ConnectionErrorState]> = self.connection_errors.clone().into();
        let changed = self.connection_view.loading
            || self.connection_view.backend_available != self.connection_backend_available
            || self.connection_view.application_ports.as_ref() != application_ports.as_ref()
            || self.connection_view.host_ports.as_ref() != host_ports.as_ref()
            || self.connection_view.confirmed_links.as_ref() != confirmed_links.as_ref()
            || self.connection_view.pending_links.as_ref() != pending_links.as_ref()
            || self.connection_view.errors.as_ref() != errors.as_ref();
        if changed {
            self.connection_revision = self.connection_revision.wrapping_add(1);
            self.connection_view = Arc::new(ConnectionViewState {
                revision: self.connection_revision,
                loading: false,
                backend_available: self.connection_backend_available,
                application_ports,
                host_ports,
                confirmed_links,
                pending_links,
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
        let mut next_fx_state_id = 1_u64;
        let mut fx_states = Vec::new();
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
            let port_ids_for_role = |role| {
                ports
                    .iter()
                    .filter(|port| port.role == role)
                    .map(|port| port.id)
                    .collect::<Vec<_>>()
            };
            let audio_inputs = port_ids_for_role(PortRoleDocument::AudioInput);
            let audio_sends = port_ids_for_role(PortRoleDocument::AudioSend);
            let audio_returns = port_ids_for_role(PortRoleDocument::AudioReturn);
            let audio_outputs = port_ids_for_role(PortRoleDocument::AudioOutput);
            let midi_inputs = port_ids_for_role(PortRoleDocument::MidiInput);
            let midi_sends = port_ids_for_role(PortRoleDocument::MidiSend);
            let midi_outputs = port_ids_for_role(PortRoleDocument::MidiOutput);
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
                let recorded_fx_state_id = model
                    .recorded_fx_state
                    .as_ref()
                    .filter(|_| {
                        content
                            .audio
                            .iter()
                            .any(|channel| channel.mode == BackendChannelMode::Wet)
                    })
                    .map(|recorded| {
                        let id = next_fx_state_id;
                        next_fx_state_id = next_fx_state_id.saturating_add(1);
                        fx_states.push(FxStateDocument {
                            id,
                            chain_type: fx_chain_type_for_processor(&recorded.processor_type)?,
                            internal_state: recorded.state.clone(),
                        });
                        Ok::<u64, String>(id)
                    })
                    .transpose()?;
                let mut direct_audio_index = 0_usize;
                let mut dry_audio_index = 0_usize;
                let mut wet_audio_index = 0_usize;
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
                    let (mode, connected_port_ids, fx_state_id) = match audio.mode {
                        BackendChannelMode::Direct => {
                            let mut ids = Vec::new();
                            ids.extend(audio_inputs.get(direct_audio_index));
                            ids.extend(audio_outputs.get(direct_audio_index));
                            direct_audio_index += 1;
                            (
                                ChannelModeDocument::Direct,
                                ids.into_iter().copied().collect(),
                                None,
                            )
                        }
                        BackendChannelMode::Dry => {
                            let mut ids = Vec::new();
                            ids.extend(audio_inputs.get(dry_audio_index));
                            ids.extend(audio_sends.get(dry_audio_index));
                            dry_audio_index += 1;
                            (
                                ChannelModeDocument::Dry,
                                ids.into_iter().copied().collect(),
                                None,
                            )
                        }
                        BackendChannelMode::Wet => {
                            let mut ids = Vec::new();
                            ids.extend(audio_returns.get(wet_audio_index));
                            ids.extend(audio_outputs.get(wet_audio_index));
                            wet_audio_index += 1;
                            (
                                ChannelModeDocument::Wet,
                                ids.into_iter().copied().collect(),
                                recorded_fx_state_id,
                            )
                        }
                    };
                    channels.push(ChannelDocument {
                        id: channel_id,
                        mode,
                        data_type: DataTypeDocument::Audio,
                        data_length_frames,
                        start_offset_frames: i64::from(audio.start_offset),
                        preplay_frames: u64::from(audio.preplay),
                        gain: audio.gain,
                        connected_port_ids,
                        media_id: (data_length_frames > 0).then_some(media_id),
                        recording_started_at: None,
                        recording_fx_state_id: fx_state_id,
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
                    let (mode, connected_port_ids) = match midi.mode {
                        BackendChannelMode::Direct => (
                            ChannelModeDocument::Direct,
                            midi_inputs.iter().chain(&midi_outputs).copied().collect(),
                        ),
                        BackendChannelMode::Dry => (
                            ChannelModeDocument::Dry,
                            midi_inputs.iter().chain(&midi_sends).copied().collect(),
                        ),
                        BackendChannelMode::Wet => {
                            return Err("wet MIDI channels are unsupported".to_owned());
                        }
                    };
                    channels.push(ChannelDocument {
                        id: channel_id,
                        mode,
                        data_type: DataTypeDocument::Midi,
                        data_length_frames: u64::from(midi.length),
                        start_offset_frames: i64::from(midi.start_offset),
                        preplay_frames: u64::from(midi.preplay),
                        gain: 1.0,
                        connected_port_ids,
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
                    length_frames: u64::from(model.length),
                    is_sync: model.state.sync,
                    gain: content.gain,
                    balance: content.balance,
                    channels: if model.script_composition.is_empty() {
                        channels
                    } else {
                        Vec::new()
                    },
                    composite: (!model.script_composition.is_empty()).then(|| CompositeDocument {
                        kind: CompositeKindDocument::Regular,
                        playlists: vec![model
                            .script_composition
                            .iter()
                            .map(|section| {
                                section
                                    .iter()
                                    .map(|source| CompositeEventDocument {
                                        delay_frames: 0,
                                        loop_id: source.raw(),
                                        mode: None,
                                        n_cycles: None,
                                    })
                                    .collect()
                            })
                            .collect()],
                    }),
                });
            }
            let (topology, fx_chain) = match &captured.topology {
                BackendTrackTopology::Direct {
                    audio_channels,
                    midi,
                } => (
                    TrackTopologyDocument::Direct {
                        audio_channels: *audio_channels,
                        midi: *midi,
                    },
                    None,
                ),
                BackendTrackTopology::DryWetExternal {
                    dry_audio_channels,
                    wet_audio_channels,
                    dry_midi,
                } => (
                    TrackTopologyDocument::DryWetExternal {
                        dry_audio_channels: *dry_audio_channels,
                        wet_audio_channels: *wet_audio_channels,
                        dry_midi: *dry_midi,
                    },
                    None,
                ),
                BackendTrackTopology::DryWetProcessor {
                    processor_type,
                    dry_audio_channels,
                    wet_audio_channels,
                    dry_midi,
                } => {
                    let chain_type = fx_chain_type_for_processor(
                        &shoop_app_api::TrackProcessorTypeId::new(processor_type.clone()),
                    )?;
                    let internal_state = captured.processor_state.clone().ok_or_else(|| {
                        format!("processed track {} has no captured state", track.id)
                    })?;
                    let topology = if chain_type == FxChainTypeDocument::TinySynthFx {
                        if dry_audio_channels != wet_audio_channels || !dry_midi {
                            return Err(format!(
                                "Tiny Synth/FX track {} has an invalid channel shape",
                                track.id
                            ));
                        }
                        TrackTopologyDocument::TinySynthFx {
                            audio_channels: *dry_audio_channels,
                        }
                    } else {
                        TrackTopologyDocument::Carla {
                            chain_type,
                            audio_channels: *wet_audio_channels,
                            midi: *dry_midi,
                            dry_audio_channels: Some(*dry_audio_channels),
                            wet_audio_channels: Some(*wet_audio_channels),
                        }
                    };
                    (
                        topology,
                        Some(FxChainDocument {
                            id: track.id.raw(),
                            title: track.name.clone(),
                            chain_type,
                            ports: Vec::new(),
                            internal_state,
                            midi_cc_assignments: captured
                                .tiny_synth_midi_cc_assignments
                                .iter()
                                .copied()
                                .map(document_midi_cc_assignment)
                                .collect(),
                        }),
                    )
                }
            };
            let document = TrackDocument {
                id: track.id.raw(),
                name: track.name.clone(),
                port_name_base: track.port_name_base.clone(),
                is_sync: track.is_sync,
                width: None,
                topology,
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
                fx_chain,
            };
            if track.is_sync {
                sync_tracks.push(document);
            } else {
                main_tracks.push(document);
            }
        }
        let document = SessionDocument {
            sample_rate: capture.sample_rate,
            connection_model_version: shoop_session::CONNECTION_MODEL_VERSION,
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
                auto_mute_other_track_inputs: self.global.auto_mute_other_track_inputs,
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
            fx_states,
            scripts: self.session_script_documents(),
            midi_control: MidiControlDocument::default(),
            settings: Vec::new(),
        };
        Ok(SessionBundle { document, media })
    }

    fn session_script_documents(&self) -> Vec<ScriptDocument> {
        self.script_manager
            .session_scripts()
            .into_iter()
            .map(|script| ScriptDocument {
                id: script.document_id,
                name: script.name,
                source: script.source,
                enabled: script.enabled,
            })
            .collect()
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
            let (backend_topology, topology, audio_channels, _) =
                runtime_track_topology(track_document, &self.track_processors)?;
            let output_audio_channels = backend_topology.wet_audio_channels();
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
                let empty = loop_document.composite.is_none()
                    && loop_document
                        .channels
                        .iter()
                        .all(|channel| channel.data_length_frames == 0);
                let recorded_fx_state = loop_document
                    .channels
                    .iter()
                    .filter(|channel| channel.mode == ChannelModeDocument::Wet)
                    .filter_map(|channel| channel.recording_fx_state_id)
                    .next()
                    .map(|state_id| {
                        let state = bundle
                            .document
                            .fx_states
                            .iter()
                            .find(|state| state.id == state_id)
                            .ok_or_else(|| format!("missing recorded FX state {state_id}"))?;
                        let processor_type = processor_for_fx_chain_type(state.chain_type);
                        Ok::<RecordedFxState, String>(RecordedFxState {
                            processor_type,
                            state: state.internal_state.clone(),
                        })
                    })
                    .transpose()?;
                let script_composition = loop_document
                    .composite
                    .as_ref()
                    .and_then(|composite| composite.playlists.first())
                    .map(|playlist| {
                        playlist
                            .iter()
                            .map(|section| {
                                section
                                    .iter()
                                    .map(|event| LoopId::from_raw(event.loop_id))
                                    .collect()
                            })
                            .collect()
                    })
                    .unwrap_or_default();
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
                            stereo: output_audio_channels == 2,
                            play_after_record: bundle.document.global.play_after_record,
                            composite_kind: match loop_document
                                .composite
                                .as_ref()
                                .map(|composite| composite.kind)
                            {
                                Some(CompositeKindDocument::Regular) => {
                                    shoop_app_api::CompositeKind::Regular
                                }
                                Some(CompositeKindDocument::Script) => {
                                    shoop_app_api::CompositeKind::Script
                                }
                                None => shoop_app_api::CompositeKind::None,
                            },
                            has_recorded_fx_state: recorded_fx_state.is_some(),
                            ..Default::default()
                        },
                        length: u32::try_from(loop_document.length_frames)
                            .map_err(|_| "loop length exceeds engine range".to_owned())?,
                        position: 0,
                        audio_data: None,
                        midi_data: None,
                        script_composition,
                        recorded_fx_state,
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
                topology,
                fx: None,
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
        self.script_composition_playback.clear();
        self.script_composition_frame_remainder = 0;
        self.active_piano_notes.clear();
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
        self.global.auto_mute_other_track_inputs =
            bundle.document.global.auto_mute_other_track_inputs;
        self.global.apply_n_cycles = bundle.document.global.apply_n_cycles;
        self.script_manager
            .replace_session_scripts(&session_script_sources(bundle))
            .map_err(|error| error.to_string())?;
        self.refresh_scripting_view();
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
                    topology: track.topology.clone(),
                    fx: track.fx.clone(),
                    loops: track
                        .loops
                        .iter()
                        .filter_map(|id| self.loops.get(id))
                        .map(|model| {
                            let mut state = model.state.clone();
                            state.name.clone_from(&model.name);
                            state.length_frames = u64::from(model.length);
                            state
                        })
                        .collect(),
                    controls: track.controls.clone(),
                    port_ids: Arc::clone(&track.port_ids),
                })
                .collect(),
            track_processors: Arc::clone(&self.track_processors),
            global_controls: self.global.clone(),
            status: self.status.clone(),
            audio_drivers: self.audio_drivers.clone(),
            details: self.details_snapshot(),
            connections: Arc::clone(&self.connection_view),
            scripting: Arc::clone(&self.scripting_view),
            click_track: self.click_track.clone(),
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
        let labels = self
            .tracks
            .iter()
            .find(|track| track.id == model.track_id)
            .map(|track| audio_channel_labels(&track.topology, track.audio_channels))
            .unwrap_or_default();
        let channels = model
            .audio_data
            .as_ref()
            .map(|channels| {
                channels
                    .iter()
                    .enumerate()
                    .map(|(index, samples)| WaveformChannelState {
                        id: ChannelId::from_raw((model.id.raw() << 8) | index as u64 + 1),
                        label: labels
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("Audio {}", index + 1)),
                        samples: Arc::clone(samples),
                        start_offset: 0,
                        loop_length: model.length as u64,
                        played_sample: matches!(model.state.mode, LoopMode::Playing)
                            .then_some(model.position as i64),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let midi_channels = model
            .midi_data
            .as_ref()
            .map(|channels| {
                channels
                    .iter()
                    .cloned()
                    .map(|mut channel| {
                        channel.loop_length = u64::from(model.length);
                        channel.played_sample = matches!(
                            model.state.mode,
                            LoopMode::Playing | LoopMode::PlayingDryThroughWet
                        )
                        .then_some(i64::from(model.position));
                        channel
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(LoopDetailsState {
            generation: self.revision,
            loop_id: model.id,
            title: model.name.clone(),
            loading: model.state.has_audio && model.audio_data.is_none(),
            channels,
            midi_loading: model.state.has_midi && model.midi_data.is_none(),
            midi_channels,
        })
    }

    fn prepare_loop_audio_import(
        &self,
        loop_id: LoopId,
        audio: &LoopAudio,
        update_loop_length: bool,
    ) -> Result<BackendLoopContentUpdate, String> {
        if audio.channels.is_empty() {
            return Err("audio file contains no channels".to_owned());
        }
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let target_channels = self
            .tracks
            .iter()
            .find(|track| track.id == model.track_id)
            .map(|track| track.audio_channels as usize)
            .ok_or_else(|| "target loop track is unavailable".to_owned())?;
        if audio.channels.len() != target_channels {
            return Err(format!(
                "mapped audio has {} channels but the target loop has {target_channels}",
                audio.channels.len()
            ));
        }
        let length = if update_loop_length {
            Some(
                audio
                    .channels
                    .iter()
                    .try_fold(0_u32, |longest, channel| {
                        u32::try_from(channel.samples.len()).map(|length| longest.max(length))
                    })
                    .map_err(|_| "audio duration exceeds engine range".to_owned())?,
            )
        } else {
            None
        };
        Ok(BackendLoopContentUpdate {
            audio: audio
                .channels
                .iter()
                .enumerate()
                .map(|(channel, source)| BackendAudioChannelUpdate {
                    channel,
                    samples: source.samples.clone(),
                    start_offset: None,
                    preplay: None,
                })
                .collect(),
            midi: Vec::new(),
            length,
        })
    }

    fn prepare_loop_midi_import(
        &self,
        loop_id: LoopId,
        midi: &ExactMidi,
        update_loop_length: bool,
    ) -> Result<BackendLoopContentUpdate, String> {
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        if !model.state.has_midi {
            return Err("target loop has no MIDI channel".to_owned());
        }
        let length = u32::try_from(midi.length_frames)
            .map_err(|_| "MIDI duration exceeds engine range".to_owned())?;
        Ok(BackendLoopContentUpdate {
            audio: Vec::new(),
            midi: vec![BackendMidiChannelUpdate {
                channel: 0,
                length,
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
                start_offset: None,
                preplay: None,
            }],
            length: update_loop_length.then_some(length),
        })
    }

    fn prepare_generated_click_audio(
        &self,
        loop_id: LoopId,
        audio: &LoopAudio,
    ) -> Result<BackendLoopContentUpdate, String> {
        let samples = audio
            .channels
            .first()
            .ok_or_else(|| "generated audio contains no channels".to_owned())?
            .samples
            .clone();
        let length = u32::try_from(samples.len())
            .map_err(|_| "generated audio exceeds engine range".to_owned())?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let target_channels = self
            .tracks
            .iter()
            .find(|track| track.id == model.track_id)
            .map(|track| track.audio_channels as usize)
            .ok_or_else(|| "target loop track is unavailable".to_owned())?;
        if target_channels == 0 {
            return Err("target loop has no audio channels".to_owned());
        }
        Ok(BackendLoopContentUpdate {
            audio: (0..target_channels)
                .map(|channel| BackendAudioChannelUpdate {
                    channel,
                    samples: samples.clone(),
                    start_offset: Some(0),
                    preplay: Some(0),
                })
                .collect(),
            midi: Vec::new(),
            length: Some(length),
        })
    }

    fn prepare_generated_click_midi(
        &self,
        loop_id: LoopId,
        midi: &ExactMidi,
    ) -> Result<BackendLoopContentUpdate, String> {
        let length = u32::try_from(midi.length_frames)
            .map_err(|_| "generated MIDI exceeds engine range".to_owned())?;
        let events = midi
            .events
            .iter()
            .map(|event| {
                Ok(BackendMidiEvent {
                    time: u32::try_from(event.frame)
                        .map_err(|_| "generated MIDI event exceeds engine range".to_owned())?,
                    data: event.data.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        if !model.state.has_midi {
            return Err("target loop has no MIDI channels".to_owned());
        }
        Ok(BackendLoopContentUpdate {
            audio: Vec::new(),
            midi: vec![BackendMidiChannelUpdate {
                channel: 0,
                length,
                start_state: midi.start_state.clone(),
                events,
                start_offset: Some(0),
                preplay: Some(0),
            }],
            length: Some(length),
        })
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
        let track = self
            .tracks
            .iter()
            .find(|track| track.id == loop_model.track_id)
            .ok_or_else(|| "loop track is unavailable".to_owned())?;
        let channels = track.audio_channels;
        let available_channels = audio_channel_labels(&track.topology, channels);
        if channels == 0 {
            return Err("loop has no audio channels".to_owned());
        }
        self.start_io_task(IoTaskKind::ExportLoopAudio, "Select loop audio channels");
        if let Some(task) = &mut self.io_task {
            task.status = IoTaskStatus::AwaitingChannelSelection;
            task.progress = 0.2;
            task.audio_channel_selection = Some(AudioChannelSelectionState {
                available_channels,
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
        let audio = match if name.to_ascii_lowercase().ends_with(".wav") {
            decode_wav(bytes)
        } else {
            decode_loop_audio(bytes)
        } {
            Ok(audio) => audio,
            Err(error) => {
                let message = error.to_string();
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        };
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
        } else if let Err(message) =
            self.begin_audio_channel_mapping(loop_id, audio, update_loop_length)
        {
            self.finish_io(IoTaskStatus::Failed, &message);
            return Err(message);
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
        let midi = match if name.to_ascii_lowercase().ends_with(".mid") {
            decode_standard_midi(bytes, self.status.sample_rate)
        } else {
            decode_exact_midi(bytes)
        } {
            Ok(midi) => midi,
            Err(error) => {
                let message = error.to_string();
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        };
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
        let labels = self
            .tracks
            .iter()
            .find(|track| track.id == model.track_id)
            .map(|track| audio_channel_labels(&track.topology, track.audio_channels))
            .unwrap_or_default();
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
                        label: labels
                            .get(*index as usize)
                            .cloned()
                            .unwrap_or_else(|| format!("Audio {}", index + 1)),
                        role: match channel.mode {
                            BackendChannelMode::Direct => "direct",
                            BackendChannelMode::Dry => "dry",
                            BackendChannelMode::Wet => "wet",
                        }
                        .to_owned(),
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

    fn notify_warning(&mut self, message: String) {
        self.notify(NotificationLevel::Warning, message);
    }

    fn notify_error(&mut self, message: String) {
        self.notify(NotificationLevel::Error, message);
    }

    fn notify(&mut self, level: NotificationLevel, message: String) {
        self.notifications.push(AppNotification { level, message });
        const MAX_NOTIFICATIONS: usize = 32;
        if self.notifications.len() > MAX_NOTIFICATIONS {
            self.notifications
                .drain(..self.notifications.len() - MAX_NOTIFICATIONS);
        }
    }
}

fn click_timing_spec(request: &ClickTrackRequest) -> ClickTrackTimingSpec {
    ClickTrackTimingSpec {
        bpm: request.bpm,
        click_count: request.click_count,
        odd_click_delay_percent: request.odd_click_delay_percent,
    }
}

fn audio_channel_labels(topology: &TrackTopology, direct_audio_channels: u32) -> Vec<String> {
    match topology {
        TrackTopology::Direct => (0..direct_audio_channels)
            .map(|index| format!("Direct {}", index + 1))
            .collect(),
        TrackTopology::DryWet {
            dry_audio_channels,
            wet_audio_channels,
            ..
        } => (0..*dry_audio_channels)
            .map(|index| format!("Dry {}", index + 1))
            .chain((0..*wet_audio_channels).map(|index| format!("Wet {}", index + 1)))
            .collect(),
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

fn io_pending_error(message: &str) -> bool {
    message.contains("session capture pending")
        || message.contains("session replacement pending")
        || message.contains("loop content replacement pending")
}

fn document_midi_cc_assignment(
    assignment: BackendTinySynthFxMidiCcAssignment,
) -> TinySynthFxMidiCcAssignmentDocument {
    let parameter = match assignment.parameter {
        BackendTinySynthFxParameter::MasterGain => TinySynthFxParameterDocument::MasterGain,
        BackendTinySynthFxParameter::ReverbAmount => TinySynthFxParameterDocument::ReverbAmount,
        BackendTinySynthFxParameter::DistortionDrive => {
            TinySynthFxParameterDocument::DistortionDrive
        }
        BackendTinySynthFxParameter::CompressorAmount => {
            TinySynthFxParameterDocument::CompressorAmount
        }
        BackendTinySynthFxParameter::EqLow => TinySynthFxParameterDocument::EqLow,
        BackendTinySynthFxParameter::EqMid => TinySynthFxParameterDocument::EqMid,
        BackendTinySynthFxParameter::EqHigh => TinySynthFxParameterDocument::EqHigh,
    };
    TinySynthFxMidiCcAssignmentDocument {
        parameter,
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn backend_midi_cc_assignment(
    assignment: TinySynthFxMidiCcAssignmentDocument,
) -> BackendTinySynthFxMidiCcAssignment {
    let parameter = match assignment.parameter {
        TinySynthFxParameterDocument::MasterGain => BackendTinySynthFxParameter::MasterGain,
        TinySynthFxParameterDocument::ReverbAmount => BackendTinySynthFxParameter::ReverbAmount,
        TinySynthFxParameterDocument::DistortionDrive => {
            BackendTinySynthFxParameter::DistortionDrive
        }
        TinySynthFxParameterDocument::CompressorAmount => {
            BackendTinySynthFxParameter::CompressorAmount
        }
        TinySynthFxParameterDocument::EqLow => BackendTinySynthFxParameter::EqLow,
        TinySynthFxParameterDocument::EqMid => BackendTinySynthFxParameter::EqMid,
        TinySynthFxParameterDocument::EqHigh => BackendTinySynthFxParameter::EqHigh,
    };
    BackendTinySynthFxMidiCcAssignment {
        parameter,
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn fx_chain_type_for_processor(
    processor: &shoop_app_api::TrackProcessorTypeId,
) -> Result<FxChainTypeDocument, String> {
    match processor.as_str() {
        shoop_app_api::TrackProcessorTypeId::CARLA_RACK => Ok(FxChainTypeDocument::CarlaRack),
        shoop_app_api::TrackProcessorTypeId::CARLA_PATCHBAY => {
            Ok(FxChainTypeDocument::CarlaPatchbay)
        }
        shoop_app_api::TrackProcessorTypeId::CARLA_PATCHBAY_16X => {
            Ok(FxChainTypeDocument::CarlaPatchbay16x)
        }
        shoop_app_api::TrackProcessorTypeId::TINY_SYNTH_FX => Ok(FxChainTypeDocument::TinySynthFx),
        "test_2x2x1" => Ok(FxChainTypeDocument::Test),
        value => Err(format!(
            "processor {value} has no session FX-chain representation"
        )),
    }
}

fn processor_for_fx_chain_type(
    chain_type: FxChainTypeDocument,
) -> shoop_app_api::TrackProcessorTypeId {
    shoop_app_api::TrackProcessorTypeId::new(match chain_type {
        FxChainTypeDocument::CarlaRack => shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
        FxChainTypeDocument::CarlaPatchbay => shoop_app_api::TrackProcessorTypeId::CARLA_PATCHBAY,
        FxChainTypeDocument::CarlaPatchbay16x => {
            shoop_app_api::TrackProcessorTypeId::CARLA_PATCHBAY_16X
        }
        FxChainTypeDocument::TinySynthFx => shoop_app_api::TrackProcessorTypeId::TINY_SYNTH_FX,
        FxChainTypeDocument::Test => "test_2x2x1",
    })
}

fn runtime_track_topology(
    track: &TrackDocument,
    processors: &[TrackProcessorDescriptor],
) -> Result<(BackendTrackTopology, TrackTopology, u32, bool), String> {
    match &track.topology {
        TrackTopologyDocument::Direct {
            audio_channels,
            midi,
        } => {
            if track.fx_chain.is_some() {
                return Err(format!("direct track {} contains an FX chain", track.id));
            }
            Ok((
                BackendTrackTopology::Direct {
                    audio_channels: *audio_channels,
                    midi: *midi,
                },
                TrackTopology::Direct,
                *audio_channels,
                *midi,
            ))
        }
        TrackTopologyDocument::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } => {
            let processor = shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::EXTERNAL,
            );
            validate_loaded_processor(
                track.id,
                &processor,
                *dry_audio_channels,
                *wet_audio_channels,
                *dry_midi,
                processors,
            )?;
            Ok((
                BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: *dry_audio_channels,
                    wet_audio_channels: *wet_audio_channels,
                    dry_midi: *dry_midi,
                },
                TrackTopology::DryWet {
                    dry_audio_channels: *dry_audio_channels,
                    wet_audio_channels: *wet_audio_channels,
                    dry_midi: *dry_midi,
                    processor_type: processor,
                },
                dry_audio_channels.saturating_add(*wet_audio_channels),
                *dry_midi,
            ))
        }
        TrackTopologyDocument::Carla {
            chain_type,
            audio_channels,
            midi,
            dry_audio_channels,
            wet_audio_channels,
        } => {
            let dry_audio_channels = dry_audio_channels.unwrap_or(*audio_channels);
            let wet_audio_channels = wet_audio_channels.unwrap_or(*audio_channels);
            let processor = processor_for_fx_chain_type(*chain_type);
            validate_loaded_processor(
                track.id,
                &processor,
                dry_audio_channels,
                wet_audio_channels,
                *midi,
                processors,
            )?;
            Ok((
                BackendTrackTopology::DryWetProcessor {
                    processor_type: processor.as_str().to_owned(),
                    dry_audio_channels,
                    wet_audio_channels,
                    dry_midi: *midi,
                },
                TrackTopology::DryWet {
                    dry_audio_channels,
                    wet_audio_channels,
                    dry_midi: *midi,
                    processor_type: processor,
                },
                dry_audio_channels.saturating_add(wet_audio_channels),
                *midi,
            ))
        }
        TrackTopologyDocument::TinySynthFx { audio_channels } => {
            let processor = shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::TINY_SYNTH_FX,
            );
            validate_loaded_processor(
                track.id,
                &processor,
                *audio_channels,
                *audio_channels,
                true,
                processors,
            )?;
            Ok((
                BackendTrackTopology::DryWetProcessor {
                    processor_type: processor.as_str().to_owned(),
                    dry_audio_channels: *audio_channels,
                    wet_audio_channels: *audio_channels,
                    dry_midi: true,
                },
                TrackTopology::DryWet {
                    dry_audio_channels: *audio_channels,
                    wet_audio_channels: *audio_channels,
                    dry_midi: true,
                    processor_type: processor,
                },
                audio_channels.saturating_mul(2),
                true,
            ))
        }
        TrackTopologyDocument::Trigger => Err(format!(
            "track {} requires unsupported trigger topology",
            track.id
        )),
    }
}

fn validate_loaded_processor(
    track_id: u64,
    processor: &shoop_app_api::TrackProcessorTypeId,
    dry_audio_channels: u32,
    wet_audio_channels: u32,
    dry_midi: bool,
    processors: &[TrackProcessorDescriptor],
) -> Result<(), String> {
    let descriptor = processors
        .iter()
        .find(|descriptor| descriptor.id == *processor)
        .filter(|descriptor| descriptor.available)
        .ok_or_else(|| format!("track {track_id} requires unavailable processor {processor}"))?;
    if !descriptor
        .constraints
        .accepts(dry_audio_channels, wet_audio_channels, dry_midi)
    {
        return Err(format!(
            "track {track_id} processor {processor} does not support its channel shape"
        ));
    }
    Ok(())
}

fn session_script_sources(bundle: &SessionBundle) -> Vec<SessionScriptSource> {
    bundle
        .document
        .scripts
        .iter()
        .map(|script| SessionScriptSource {
            document_id: script.id,
            name: script.name.clone(),
            source: script.source.clone(),
            enabled: script.enabled,
        })
        .collect()
}

fn session_bundle_to_backend(
    bundle: &SessionBundle,
    processors: &[TrackProcessorDescriptor],
) -> Result<BackendSessionData, String> {
    if !bundle.document.buses.is_empty()
        || !bundle.document.global_ports.is_empty()
        || !bundle.document.midi_control.bindings.is_empty()
        || !bundle.document.settings.is_empty()
    {
        return Err(
            "session requires a feature not yet available in the application runtime".to_owned(),
        );
    }
    let mut tracks = Vec::new();
    for track in bundle
        .document
        .track_groups
        .iter()
        .flat_map(|group| &group.tracks)
    {
        let (topology, _, _, midi) = runtime_track_topology(track, processors)?;
        let output_audio_channels = topology.wet_audio_channels();
        let state = BackendTrackState {
            topology: topology.clone(),
            audio_channels: output_audio_channels,
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
        let expected_audio_modes = match &topology {
            BackendTrackTopology::Direct { audio_channels, .. } => {
                vec![BackendChannelMode::Direct; *audio_channels as usize]
            }
            BackendTrackTopology::DryWetExternal {
                dry_audio_channels,
                wet_audio_channels,
                ..
            }
            | BackendTrackTopology::DryWetProcessor {
                dry_audio_channels,
                wet_audio_channels,
                ..
            } => (0..*dry_audio_channels)
                .map(|_| BackendChannelMode::Dry)
                .chain((0..*wet_audio_channels).map(|_| BackendChannelMode::Wet))
                .collect(),
        };
        let expected_midi_modes = if midi {
            vec![if matches!(topology, BackendTrackTopology::Direct { .. }) {
                BackendChannelMode::Direct
            } else {
                BackendChannelMode::Dry
            }]
        } else {
            Vec::new()
        };
        let mut loops = Vec::with_capacity(track.loops.len());
        for loop_ in &track.loops {
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
                            mode: backend_channel_mode(channel.mode)?,
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
                            mode: backend_channel_mode(channel.mode)?,
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
            if loop_.composite.is_some() {
                audio = expected_audio_modes
                    .iter()
                    .copied()
                    .map(|mode| BackendAudioContent {
                        mode,
                        samples: Vec::new(),
                        gain: 1.0,
                        start_offset: 0,
                        preplay: 0,
                    })
                    .collect();
                midi_channels = expected_midi_modes
                    .iter()
                    .copied()
                    .map(|mode| BackendMidiContent {
                        mode,
                        length: 0,
                        start_state: Vec::new(),
                        events: Vec::new(),
                        start_offset: 0,
                        preplay: 0,
                    })
                    .collect();
            }
            if audio.iter().map(|channel| channel.mode).collect::<Vec<_>>() != expected_audio_modes
                || midi_channels
                    .iter()
                    .map(|channel| channel.mode)
                    .collect::<Vec<_>>()
                    != expected_midi_modes
            {
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
            topology,
            state,
            loops,
            ports,
            processor_state: track
                .fx_chain
                .as_ref()
                .map(|chain| chain.internal_state.clone()),
            tiny_synth_midi_cc_assignments: track
                .fx_chain
                .as_ref()
                .into_iter()
                .flat_map(|chain| chain.midi_cc_assignments.iter().copied())
                .map(backend_midi_cc_assignment)
                .collect(),
        });
    }
    Ok(BackendSessionData {
        sample_rate: bundle.document.sample_rate,
        tracks,
        use_legacy_browser_default_routes: cfg!(target_arch = "wasm32")
            && bundle.document.connection_model_version == 0,
    })
}

fn backend_channel_mode(value: ChannelModeDocument) -> Result<BackendChannelMode, String> {
    match value {
        ChannelModeDocument::Direct => Ok(BackendChannelMode::Direct),
        ChannelModeDocument::Dry => Ok(BackendChannelMode::Dry),
        ChannelModeDocument::Wet => Ok(BackendChannelMode::Wet),
        ChannelModeDocument::Disabled => Err("disabled loop channels are unsupported".to_owned()),
    }
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

fn script_connection_port_id(script_id: ScriptId, registration: u32) -> PortId {
    const SCRIPT_PORT_NAMESPACE: u64 = 1 << 63;
    const SCRIPT_ID_MASK: u64 = 0x7fff_ffff;
    PortId::from_raw(
        SCRIPT_PORT_NAMESPACE
            | ((script_id.raw() & SCRIPT_ID_MASK) << 32)
            | u64::from(registration.saturating_add(1)),
    )
}

fn script_midi_host_id(direction: PortDirection, endpoint: &str) -> HostPortId {
    let direction = match direction {
        PortDirection::Input => shoop_scripting::MidiEndpointDirection::Input,
        PortDirection::Output => shoop_scripting::MidiEndpointDirection::Output,
    };
    HostPortId::new(shoop_scripting::midi_endpoint_host_id(direction, endpoint))
}

fn map_port_data_type(value: BackendPortDataType) -> PortDataType {
    match value {
        BackendPortDataType::Audio => PortDataType::Audio,
        BackendPortDataType::Midi => PortDataType::Midi,
    }
}

fn map_port_direction(value: BackendPortDirection) -> PortDirection {
    match value {
        BackendPortDirection::Input => PortDirection::Input,
        BackendPortDirection::Output => PortDirection::Output,
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
    fn application_publishes_backend_processor_capabilities() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new("future_browser_fx"),
            label: "Future browser FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: Some(2),
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Unsupported,
            },
            features: shoop_app_api::TrackProcessorFeatures {
                state: true,
                external_ui: false,
                embedded_ui: false,
                recovery: false,
                logs: false,
            },
            editor: None,
        };
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![descriptor.clone()]);
        let runtime = ApplicationRuntime::start(Box::new(backend)).unwrap();
        assert_eq!(
            runtime.handle().snapshot().track_processors.as_ref(),
            &[descriptor]
        );
    }

    #[test]
    fn application_adds_external_dry_wet_tracks_from_processor_capabilities() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::EXTERNAL,
            ),
            label: "External".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: None,
                max_wet_audio_channels: None,
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
            },
            features: shoop_app_api::TrackProcessorFeatures::default(),
            editor: None,
        };
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![descriptor]);
        let runtime = ApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime
            .handle()
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "Outboard".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::EXTERNAL,
                    ),
                },
            }))
            .unwrap();
        let snapshot = wait_for(&runtime.handle(), |snapshot| snapshot.tracks.len() == 2);
        let track = &snapshot.tracks[1];
        assert_eq!(
            track.topology,
            TrackTopology::DryWet {
                dry_audio_channels: 2,
                wet_audio_channels: 1,
                dry_midi: true,
                processor_type: shoop_app_api::TrackProcessorTypeId::new(
                    shoop_app_api::TrackProcessorTypeId::EXTERNAL,
                ),
            }
        );
        assert_eq!(track.loops.len(), 8);
        assert!(track.controls.has_input);
        assert!(track.controls.input_stereo);
        assert!(track.controls.has_output);
        assert!(!track.controls.output_stereo);
        assert_eq!(track.port_ids.len(), 8);
        runtime
            .handle()
            .dispatch(AppIntent::AddLoop { track_id: track.id })
            .unwrap();
        let snapshot = wait_for(&runtime.handle(), |snapshot| {
            snapshot.tracks[1].loops.len() == 9
        });
        assert_eq!(snapshot.tracks[1].topology, track.topology);
        assert!(snapshot.tracks[1].loops[8].has_audio);
        runtime
            .handle()
            .dispatch(AppIntent::RequestSaveSession)
            .unwrap();
        let _ = wait_for(&runtime.handle(), |snapshot| {
            snapshot
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Completed)
        });
        let output = runtime.handle().take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        let saved_track = &saved.document.track_groups[1].tracks[0];
        assert_eq!(
            saved_track.topology,
            TrackTopologyDocument::DryWetExternal {
                dry_audio_channels: 2,
                wet_audio_channels: 1,
                dry_midi: true,
            }
        );
        assert_eq!(
            saved_track
                .ports
                .iter()
                .map(|port| port.role)
                .collect::<Vec<_>>(),
            vec![
                PortRoleDocument::AudioInput,
                PortRoleDocument::AudioSend,
                PortRoleDocument::AudioInput,
                PortRoleDocument::AudioSend,
                PortRoleDocument::AudioReturn,
                PortRoleDocument::AudioOutput,
                PortRoleDocument::MidiInput,
                PortRoleDocument::MidiSend,
            ]
        );
    }

    #[test]
    fn tiny_synth_fx_round_trips_controls_and_recorded_state() {
        let backend = shoop_backend::EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "Tiny".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::TINY_SYNTH_FX,
                    ),
                },
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = runtime.snapshot().tracks[1].clone();
        for control in [
            shoop_app_api::TinySynthFxControl::SelectPreset("pad".to_owned()),
            shoop_app_api::TinySynthFxControl::SetMasterGainDb(-12.0),
            shoop_app_api::TinySynthFxControl::SetReverbEnabled(true),
            shoop_app_api::TinySynthFxControl::SetReverbAmount(0.4),
            shoop_app_api::TinySynthFxControl::SetDistortionEnabled(true),
            shoop_app_api::TinySynthFxControl::SetDistortionDrive(7.0),
            shoop_app_api::TinySynthFxControl::SetCompressorEnabled(true),
            shoop_app_api::TinySynthFxControl::SetCompressorAmount(0.6),
            shoop_app_api::TinySynthFxControl::SetEqEnabled(true),
            shoop_app_api::TinySynthFxControl::SetEqLowDb(3.0),
            shoop_app_api::TinySynthFxControl::SetEqMidDb(-2.0),
            shoop_app_api::TinySynthFxControl::SetEqHighDb(1.5),
            shoop_app_api::TinySynthFxControl::AssignMidiCc(
                shoop_app_api::TinySynthFxMidiCcAssignment {
                    parameter: shoop_app_api::TinySynthFxParameter::EqHigh,
                    channel: 6,
                    controller: 74,
                },
            ),
        ] {
            runtime
                .dispatch(AppIntent::Track {
                    track_id: track.id,
                    action: TrackAction::TinySynthFx(control),
                })
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let loop_id = track.loops[0].id;
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks[1].loops[0].has_recorded_fx_state);
        runtime
            .dispatch(AppIntent::Track {
                track_id: track.id,
                action: TrackAction::FxVisibilityChanged(true),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks[1].fx.as_ref().unwrap().visible);

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let output = runtime.take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        let saved_track = &saved.document.track_groups[1].tracks[0];
        assert_eq!(
            saved_track.topology,
            TrackTopologyDocument::TinySynthFx { audio_channels: 2 }
        );
        assert_eq!(
            saved_track.fx_chain.as_ref().unwrap().chain_type,
            FxChainTypeDocument::TinySynthFx
        );
        let current_state = &saved_track.fx_chain.as_ref().unwrap().internal_state;
        assert!(!current_state.is_empty());
        assert_eq!(
            saved_track.fx_chain.as_ref().unwrap().midi_cc_assignments,
            [TinySynthFxMidiCcAssignmentDocument {
                parameter: TinySynthFxParameterDocument::EqHigh,
                channel: 6,
                controller: 74,
            }]
        );
        let take_id = saved_track.loops[0]
            .channels
            .iter()
            .find(|channel| channel.mode == ChannelModeDocument::Wet)
            .unwrap()
            .recording_fx_state_id
            .unwrap();
        assert_eq!(saved.document.fx_states.len(), 1);
        assert_eq!(saved.document.fx_states[0].id, take_id);
        assert_eq!(&saved.document.fx_states[0].internal_state, current_state);

        drop(runtime);
        let backend = shoop_backend::EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "tiny.shoop".to_owned(),
                bytes: output.bytes,
            })
            .unwrap();
        for _ in 0..20 {
            runtime.tick(Duration::ZERO);
            if runtime.snapshot().io_task.as_ref().is_some_and(|task| {
                task.status == IoTaskStatus::Completed || task.status == IoTaskStatus::Failed
            }) {
                break;
            }
        }
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let loaded = runtime.snapshot();
        assert_eq!(loaded.tracks[1].topology, track.topology);
        assert!(!loaded.tracks[1].fx.as_ref().unwrap().visible);
        let Some(shoop_app_api::TrackProcessorEditorState::TinySynthFx(editor)) = loaded.tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing Tiny Synth/FX editor state");
        };
        assert_eq!(editor.selected_preset_id.as_deref(), Some("pad"));
        assert_eq!(editor.master_gain_db, -12.0);
        assert!(editor.reverb_enabled);
        assert_eq!(editor.reverb_amount, 0.4);
        assert!(editor.distortion_enabled);
        assert_eq!(editor.distortion_drive, 7.0);
        assert!(editor.compressor_enabled);
        assert_eq!(editor.compressor_amount, 0.6);
        assert!(editor.eq_enabled);
        assert_eq!(editor.eq_low_db, 3.0);
        assert_eq!(editor.eq_mid_db, -2.0);
        assert_eq!(editor.eq_high_db, 1.5);
        assert_eq!(
            editor.midi_cc_assignments.as_ref(),
            [shoop_app_api::TinySynthFxMidiCcAssignment {
                parameter: shoop_app_api::TinySynthFxParameter::EqHigh,
                channel: 6,
                controller: 74,
            }]
        );
        assert!(loaded.tracks[1].loops[0].has_recorded_fx_state);
        let loaded_track_id = loaded.tracks[1].id;
        let loaded_loop_id = loaded.tracks[1].loops[0].id;

        runtime
            .dispatch(AppIntent::Track {
                track_id: loaded_track_id,
                action: TrackAction::TinySynthFx(
                    shoop_app_api::TinySynthFxControl::SetMasterGainDb(-18.0),
                ),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: loaded_track_id,
                loop_id: loaded_loop_id,
                action: LoopAction::RestoreRecordedFxState,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let restored = runtime.snapshot();
        let Some(shoop_app_api::TrackProcessorEditorState::TinySynthFx(restored_editor)) = restored
            .tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing restored Tiny Synth/FX editor state");
        };
        assert_eq!(restored_editor.master_gain_db, -12.0);
        assert_eq!(restored_editor.selected_preset_id.as_deref(), Some("pad"));
        assert_eq!(
            restored_editor.midi_cc_assignments.as_ref(),
            [shoop_app_api::TinySynthFxMidiCcAssignment {
                parameter: shoop_app_api::TinySynthFxParameter::EqHigh,
                channel: 6,
                controller: 74,
            }]
        );

        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 44_100,
                    buffer_size: 256,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let switched = runtime.snapshot();
        assert_eq!(switched.status.sample_rate, 44_100);
        assert_eq!(switched.tracks[1].topology, track.topology);
        assert!(switched.tracks[1].loops[0].has_recorded_fx_state);
        let Some(shoop_app_api::TrackProcessorEditorState::TinySynthFx(switched_editor)) = switched
            .tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing Tiny Synth/FX state after sample-rate switch");
        };
        assert_eq!(switched_editor.master_gain_db, -12.0);
        assert_eq!(switched_editor.selected_preset_id.as_deref(), Some("pad"));
        assert!(switched_editor.reverb_enabled);
        assert_eq!(switched_editor.reverb_amount, 0.4);
        assert!(switched_editor.distortion_enabled);
        assert_eq!(switched_editor.distortion_drive, 7.0);
        assert!(switched_editor.compressor_enabled);
        assert_eq!(switched_editor.compressor_amount, 0.6);
        assert_eq!(
            switched_editor.midi_cc_assignments.as_ref(),
            [shoop_app_api::TinySynthFxMidiCcAssignment {
                parameter: shoop_app_api::TinySynthFxParameter::EqHigh,
                channel: 6,
                controller: 74,
            }]
        );
        assert!(switched_editor.eq_enabled);
        assert_eq!(switched_editor.eq_low_db, 3.0);
        assert_eq!(switched_editor.eq_mid_db, -2.0);
        assert_eq!(switched_editor.eq_high_db, 1.5);
    }

    #[test]
    fn processed_track_session_round_trip_preserves_roles_state_and_recorded_take() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
            ),
            label: "Carla Rack".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: Some(2),
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
            },
            features: shoop_app_api::TrackProcessorFeatures {
                state: true,
                external_ui: true,
                embedded_ui: false,
                recovery: true,
                logs: true,
            },
            editor: None,
        };
        let exact_state = "opaque\0state\nユニコード";
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![descriptor]);
        backend.set_default_fx_state_string(exact_state);
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "Processed".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
                    ),
                },
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = runtime.snapshot().tracks[1].clone();
        let loop_id = track.loops[0].id;
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks[1].loops[0].has_recorded_fx_state);

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let output = runtime.take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        let saved_track = &saved.document.track_groups[1].tracks[0];
        assert_eq!(
            saved_track.topology,
            TrackTopologyDocument::Carla {
                chain_type: FxChainTypeDocument::CarlaRack,
                audio_channels: 1,
                midi: true,
                dry_audio_channels: Some(2),
                wet_audio_channels: Some(1),
            }
        );
        assert_eq!(
            saved_track.fx_chain.as_ref().unwrap().internal_state,
            exact_state
        );
        assert_eq!(
            saved_track.loops[0]
                .channels
                .iter()
                .map(|channel| channel.mode)
                .collect::<Vec<_>>(),
            vec![
                ChannelModeDocument::Dry,
                ChannelModeDocument::Dry,
                ChannelModeDocument::Wet,
                ChannelModeDocument::Dry,
            ]
        );
        let take_id = saved_track.loops[0]
            .channels
            .iter()
            .find(|channel| channel.mode == ChannelModeDocument::Wet)
            .unwrap()
            .recording_fx_state_id
            .unwrap();
        assert_eq!(saved.document.fx_states.len(), 1);
        assert_eq!(saved.document.fx_states[0].id, take_id);
        assert_eq!(saved.document.fx_states[0].internal_state, exact_state);
        assert!(!saved
            .document
            .settings
            .iter()
            .any(|setting| setting.key == "carla.hosting_mode"));

        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "processed.shoop".to_owned(),
                bytes: output.bytes,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loaded = runtime.snapshot();
        assert_eq!(loaded.tracks.len(), 2);
        assert_eq!(loaded.tracks[1].topology, track.topology);
        assert!(loaded.tracks[1].loops[0].has_recorded_fx_state);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: loaded.tracks[1].id,
                loop_id: loaded.tracks[1].loops[0].id,
                action: LoopAction::RestoreRecordedFxState,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(
            !runtime.snapshot().notifications.iter().any(|notification| {
                notification.level == NotificationLevel::Error
                    && notification.message.contains("recorded FX")
            })
        );
    }

    #[test]
    fn failed_recorded_fx_restore_leaves_the_processed_track_usable() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
            ),
            label: "Carla Rack".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: Some(2),
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
            },
            features: shoop_app_api::TrackProcessorFeatures {
                state: true,
                external_ui: true,
                embedded_ui: false,
                recovery: true,
                logs: true,
            },
            editor: None,
        };
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![descriptor]);
        backend.set_fail_fx_state_restore(true);
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "Restore failure".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
                    ),
                },
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = runtime.snapshot().tracks[1].clone();
        let loop_id = track.loops[0].id;
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Recording
        );
        assert!(runtime.snapshot().notifications.iter().any(|notification| {
            notification
                .message
                .contains("active recording/replacement content to settle")
        }));
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id,
                action: LoopAction::RestoreRecordedFxState,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let after = runtime.snapshot();
        assert_eq!(after.tracks[1].id, track.id);
        assert_eq!(after.tracks[1].loops[0].id, loop_id);
        assert!(after.tracks[1].loops[0].has_recorded_fx_state);
        assert!(after.notifications.iter().any(|notification| {
            notification.level == NotificationLevel::Error
                && notification
                    .message
                    .contains("injected processor state restore failure")
        }));
    }

    #[test]
    fn actor_starts_embedded_production_keyboard_script_without_checkout_files() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
                kind: ScriptKind::Bundled,
                enabled: true,
            }],
        )
        .unwrap();
        let snapshot = runtime.handle().snapshot();
        assert_eq!(snapshot.scripting.scripts.len(), 1);
        assert_eq!(
            snapshot.scripting.scripts[0].lifecycle,
            shoop_app_api::ScriptLifecycle::Listening
        );
        assert!(snapshot.scripting.scripts[0].latest_error.is_none());
    }

    #[test]
    fn startup_script_ids_preserve_source_order_across_rejection_and_duplicate_names() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    source: "local =".to_owned(),
                    kind: ScriptKind::User,
                    enabled: true,
                },
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    source: "print('second')".to_owned(),
                    kind: ScriptKind::User,
                    enabled: true,
                },
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    source: "print('third')".to_owned(),
                    kind: ScriptKind::User,
                    enabled: false,
                },
            ],
        )
        .unwrap();
        let ids = runtime.startup_script_ids();
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0], None);
        assert!(ids[1].is_some());
        assert!(ids[2].is_some());
        assert_ne!(ids[1], ids[2]);
        let snapshot = runtime.handle().snapshot();
        assert_eq!(snapshot.scripting.scripts.len(), 2);
        assert_eq!(snapshot.scripting.scripts[0].id, ids[1].unwrap());
        assert_eq!(snapshot.scripting.scripts[1].id, ids[2].unwrap());
    }

    #[test]
    fn cooperative_startup_runs_embedded_keyboard_on_the_application_owner() {
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
                kind: ScriptKind::Bundled,
                enabled: true,
            }],
        )
        .unwrap();
        assert!(runtime.snapshot().scripting.supported);
        assert_eq!(
            runtime.snapshot().scripting.scripts[0].lifecycle,
            shoop_app_api::ScriptLifecycle::Listening
        );
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Browser track".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::KeyEvent(KeyEvent {
                event_type: KeyEventType::Pressed,
                key: 16_777_236,
                modifiers: 0,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
    }

    #[test]
    fn lua_control_ports_are_owner_managed_stable_and_visible_without_midi_hosts() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 2).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start_with_midi(
            Box::new(backend),
            Box::new(shoop_scripting::NullMidiService),
        )
        .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        let snapshot = runtime.snapshot();
        let script_id = snapshot.scripting.scripts[0].id;
        let control_ports: Vec<_> = snapshot
            .connections
            .application_ports
            .iter()
            .filter(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::LuaControl {
                        script_id: owner, ..
                    } if owner == script_id
                )
            })
            .collect();
        assert_eq!(control_ports.len(), 2);
        assert!(control_ports.iter().all(|port| {
            port.data_type == PortDataType::Midi
                && port.connection_policy == ConnectionPolicy::OwnerManaged
        }));
        assert!(!snapshot
            .connections
            .host_ports
            .iter()
            .any(|host| host.data_type == PortDataType::Midi));
        let stable_ids: Vec<_> = control_ports.iter().map(|port| port.id).collect();

        runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id: stable_ids[0],
                host_port_id: HostPortId::new("invented:midi"),
                connected: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().connections.errors.iter().any(|error| {
            error.port_id == Some(stable_ids[0]) && error.kind == ConnectionErrorKind::Incompatible
        }));

        runtime
            .dispatch(AppIntent::StopScript { script_id })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(!runtime
            .snapshot()
            .connections
            .application_ports
            .iter()
            .any(|port| matches!(
                port.owner,
                ApplicationPortOwner::LuaControl {
                    script_id: owner, ..
                } if owner == script_id
            )));
        runtime
            .dispatch(AppIntent::RestartScript { script_id })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        let restarted_ids: Vec<_> = runtime
            .snapshot()
            .connections
            .application_ports
            .iter()
            .filter_map(|port| match port.owner {
                ApplicationPortOwner::LuaControl {
                    script_id: owner, ..
                } if owner == script_id => Some(port.id),
                _ => None,
            })
            .collect();
        assert_eq!(restarted_ids, stable_ids);
    }

    #[test]
    fn midi_callback_can_request_script_dialog_opening() {
        let (midi, midi_control) = shoop_scripting::FakeMidiService::new();
        midi_control.set_endpoints(vec![shoop_scripting::MidiEndpoint {
            id: "controller-source".to_owned(),
            name: "Controller".to_owned(),
            direction: shoop_scripting::MidiEndpointDirection::Output,
        }]);
        let mut runtime = CooperativeApplicationRuntime::start_with_midi(
            Box::new(FakeBackend::default()),
            Box::new(midi),
        )
        .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "midi-dialog.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 0)
local c=require('shoop_control')
local d=require('shoop_dialog')
d.simple('MIDI',{d.rich_text('Received')})
c.auto_open_device_specific_midi_control_input('Controller', function() d.open('MIDI') end)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        assert_eq!(runtime.snapshot().scripting.dialogs[0].open_request, 0);
        midi_control.push_input("controller-source", vec![0x90, 60, 100]);
        runtime.tick(Duration::from_millis(1));
        assert_eq!(runtime.snapshot().scripting.dialogs[0].open_request, 1);
    }

    #[test]
    fn web_midi_track_and_control_views_share_canonical_host_rows() {
        let backend = FakeBackend::default();
        let backend_control = backend.connection_control();
        backend_control.add_external_port(
            "webmidi:source:test-input",
            BackendPortDirection::Output,
            BackendPortDataType::Midi,
        );
        backend_control.add_external_port(
            "webmidi:sink:test-output",
            BackendPortDirection::Input,
            BackendPortDataType::Midi,
        );
        let (midi, midi_control) = shoop_scripting::FakeMidiService::new();
        midi_control.set_endpoints(vec![
            shoop_scripting::MidiEndpoint {
                id: "webmidi:source:test-input".to_owned(),
                name: "APC MINI MIDI".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Output,
            },
            shoop_scripting::MidiEndpoint {
                id: "webmidi:sink:test-output".to_owned(),
                name: "APC MINI MIDI".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Input,
            },
        ]);
        let mut runtime =
            CooperativeApplicationRuntime::start_with_midi(Box::new(backend), Box::new(midi))
                .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        let snapshot = runtime.snapshot();
        let midi_hosts = snapshot
            .connections
            .host_ports
            .iter()
            .filter(|host| {
                host.data_type == PortDataType::Midi && host.id.as_str().starts_with("webmidi:")
            })
            .collect::<Vec<_>>();
        assert_eq!(midi_hosts.len(), 2);
        assert_eq!(
            midi_hosts
                .iter()
                .map(|host| host.id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["webmidi:sink:test-output", "webmidi:source:test-input",])
        );
        let script_id = snapshot.scripting.scripts[0].id;
        let control_ports = snapshot
            .connections
            .application_ports
            .iter()
            .filter(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::LuaControl {
                        script_id: owner, ..
                    } if owner == script_id
                )
            })
            .map(|port| port.id)
            .collect::<BTreeSet<_>>();
        assert_eq!(control_ports.len(), 2);
        assert_eq!(
            snapshot
                .connections
                .confirmed_links
                .iter()
                .filter(|link| control_ports.contains(&link.application_port_id))
                .map(|link| link.host_port_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["webmidi:sink:test-output", "webmidi:source:test-input",])
        );
    }

    #[test]
    fn actor_owns_script_lifecycle_and_publishes_plain_states() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        assert!(handle.snapshot().scripting.supported);
        handle
            .dispatch(AppIntent::AddScriptSource {
                name: "user.lua".to_owned(),
                source: Arc::from("-- User docs\nshoop_announce_api_version(1, 0)\nprint('ready')"),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        let added = wait_for(&handle, |snapshot| snapshot.scripting.scripts.len() == 1);
        let script = &added.scripting.scripts[0];
        assert_eq!(script.name, "user.lua");
        assert_eq!(script.kind, ScriptKind::User);
        assert_eq!(script.documentation.as_deref(), Some("User docs\n"));
        assert_eq!(script.lifecycle, shoop_app_api::ScriptLifecycle::Finished);
        let script_id = script.id;

        handle
            .dispatch(AppIntent::StopScript { script_id })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot.scripting.scripts[0].lifecycle == shoop_app_api::ScriptLifecycle::Inactive
        });
        handle
            .dispatch(AppIntent::RestartScript { script_id })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot.scripting.scripts[0].lifecycle == shoop_app_api::ScriptLifecycle::Finished
        });
        handle
            .dispatch(AppIntent::SetScriptEnabled {
                script_id,
                enabled: false,
            })
            .unwrap();
        wait_for(&handle, |snapshot| {
            !snapshot.scripting.scripts[0].enabled
                && snapshot.scripting.scripts[0].lifecycle
                    == shoop_app_api::ScriptLifecycle::Inactive
        });
        handle
            .dispatch(AppIntent::ForgetScript { script_id })
            .unwrap();
        wait_for(&handle, |snapshot| snapshot.scripting.scripts.is_empty());
    }

    #[test]
    fn threaded_actor_routes_script_dialog_callbacks_and_teardown() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "actor-dialog.lua".to_owned(),
                source: r#"
shoop_announce_api_version(1, 0)
local c=require('shoop_control')
local d=require('shoop_dialog')
d.simple('Actor dialog',{d.button('Apply',function() c.set_solo(true) end)})
d.open('Actor dialog')
"#
                .to_owned(),
                kind: ScriptKind::User,
                enabled: true,
            }],
        )
        .unwrap();
        let handle = runtime.handle();
        let initial = handle.snapshot();
        assert_eq!(initial.scripting.dialogs.len(), 1);
        assert_eq!(initial.scripting.dialogs[0].open_request, 1);
        let dialog = &initial.scripting.dialogs[0];
        let shoop_app_api::ScriptDialogKind::Simple(content) = &dialog.kind else {
            panic!("expected simple dialog");
        };
        let shoop_app_api::ScriptDialogElement::Button {
            id: Some(button_id),
            ..
        } = &content.elements[0]
        else {
            panic!("expected callback button");
        };
        handle
            .dispatch(AppIntent::InvokeScriptDialogButton {
                script_id: dialog.owner_script_id,
                dialog_id: dialog.id,
                button_id: *button_id,
            })
            .unwrap();
        wait_for(&handle, |snapshot| snapshot.global_controls.solo);
        handle
            .dispatch(AppIntent::StopScript {
                script_id: dialog.owner_script_id,
            })
            .unwrap();
        wait_for(&handle, |snapshot| snapshot.scripting.dialogs.is_empty());
    }

    #[test]
    fn incompatible_script_version_is_published_as_error_without_side_effects() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "future.lua".to_owned(),
                source: Arc::from(
                    "shoop_announce_api_version(1, 2); __shoop_control.set_solo(true)",
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        assert!(!snapshot.global_controls.solo);
        assert!(snapshot.scripting.dialogs.is_empty());
        assert_eq!(
            snapshot.scripting.scripts[0].lifecycle,
            shoop_app_api::ScriptLifecycle::Error
        );
        let error = snapshot.scripting.scripts[0]
            .latest_error
            .as_deref()
            .unwrap();
        assert!(error.contains("script requests 1.2, host supports 1.1"));
    }

    #[test]
    fn auto_mute_policy_change_dispatches_lua_global_event() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "global-listener.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 1)
local c = require('shoop_control')
c.register_global_event_cb(function() c.set_solo(true) end)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(!runtime.snapshot().global_controls.solo);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetAutoMuteOtherTrackInputs(true),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().global_controls.solo);
    }

    #[test]
    fn lua_respecting_input_unmute_applies_global_policy_through_application() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        for name in ["first", "second"] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: 1,
                    midi: false,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let first = runtime.snapshot().tracks[1].id;
        runtime
            .dispatch(AppIntent::Track {
                track_id: first,
                action: TrackAction::InputMonitoringChanged {
                    enabled: true,
                    respect_auto_mute: false,
                },
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks[1].controls.input_monitoring);

        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "exclusive-input.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 1)
local c = require('shoop_control')
c.set_auto_mute_other_track_inputs(true)
c.track_set_input_muted(1, false, true)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        assert!(snapshot.global_controls.auto_mute_other_track_inputs);
        assert!(!snapshot.tracks[0].controls.input_monitoring);
        assert!(!snapshot.tracks[1].controls.input_monitoring);
        assert!(snapshot.tracks[2].controls.input_monitoring);
    }

    #[test]
    fn lua_control_batches_use_authoritative_application_and_backend_paths() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::AddScriptSource {
                name: "control.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 0)
local c = require('shoop_control')
c.set_solo(true)
c.loop_select({-1, 0}, true)
c.loop_set_gain({-1, 0}, 0.25)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.global_controls.solo
                && snapshot.tracks[0].loops[0].selected
                && (snapshot.tracks[0].loops[0].gain - 0.25).abs() < f32::EPSILON
        });
        assert_eq!(snapshot.scripting.scripts.len(), 1);
        assert_eq!(
            snapshot.scripting.scripts[0].lifecycle,
            shoop_app_api::ScriptLifecycle::Finished
        );
    }

    #[test]
    fn script_keyboard_events_timers_and_committed_loop_events_are_dispatched() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::AddScriptSource {
                name: "events.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 0)
local c = require('shoop_control')
c.register_keyboard_event_cb(function(event)
    if event.type == c.constants.KeyEventType_Pressed and event.key == c.constants.Key_Space then
        c.set_solo(true)
    end
end)
c.register_loop_event_cb(function(event)
    if event.type == c.constants.LoopEventType_SelectedChanged and event.selected then
        print_info('selected once')
        c.set_apply_n_cycles(7)
    end
end)
c.register_one_shot_timer_cb(1, function() c.set_sync_active(false) end)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot
                .scripting
                .scripts
                .first()
                .is_some_and(|script| script.lifecycle == shoop_app_api::ScriptLifecycle::Listening)
                && !snapshot.global_controls.sync
        });
        handle
            .dispatch(AppIntent::KeyEvent(KeyEvent {
                event_type: KeyEventType::Pressed,
                key: 32,
                modifiers: 0,
            }))
            .unwrap();
        wait_for(&handle, |snapshot| snapshot.global_controls.solo);
        let snapshot = handle.snapshot();
        handle
            .dispatch(AppIntent::Loop {
                track_id: snapshot.tracks[0].id,
                loop_id: snapshot.tracks[0].loops[0].id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        wait_for(&handle, |snapshot| {
            snapshot.tracks[0].loops[0].selected
                && snapshot.global_controls.apply_n_cycles == 7
                && snapshot.scripting.scripts[0].logs.len() == 1
        });
        thread::sleep(Duration::from_millis(50));
        assert_eq!(handle.snapshot().scripting.scripts[0].logs.len(), 1);
    }

    #[test]
    fn script_dialogs_publish_open_in_callbacks_invoke_exact_buttons_and_teardown() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "owner.lua".to_owned(),
                source: Arc::from(
                    r#"
shoop_announce_api_version(1, 0)
local c = require('shoop_control')
local d = require('shoop_dialog')
d.simple('Main', {
    d.rich_text('Ready', {strong=true}),
    d.button('Apply', function() c.set_solo(true); d.open('Other') end),
})
d.simple('Other', {d.rich_text('More')})
d.open('Main')
c.register_one_shot_timer_cb(1, function() d.open('Other') end)
"#,
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let initial = runtime.snapshot();
        assert_eq!(
            initial.scripting.api_version,
            shoop_app_api::LUA_API_VERSION
        );
        assert_eq!(initial.scripting.dialogs.len(), 2);
        assert_eq!(initial.scripting.dialogs[0].name, "Main");
        assert_eq!(initial.scripting.dialogs[0].open_request, 1);
        assert_eq!(initial.scripting.dialogs[1].open_request, 0);
        let owner = initial.scripting.dialogs[0].owner_script_id;
        let dialog_id = initial.scripting.dialogs[0].id;
        let shoop_app_api::ScriptDialogKind::Simple(content) = &initial.scripting.dialogs[0].kind
        else {
            panic!("expected simple dialog");
        };
        let shoop_app_api::ScriptDialogElement::Button {
            id: Some(button_id),
            ..
        } = &content.elements[1]
        else {
            panic!("expected callback button");
        };

        runtime.tick(Duration::from_millis(1));
        assert_eq!(runtime.snapshot().scripting.dialogs[1].open_request, 1);
        runtime
            .dispatch(AppIntent::InvokeScriptDialogButton {
                script_id: owner,
                dialog_id,
                button_id: *button_id,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().global_controls.solo);
        assert_eq!(runtime.snapshot().scripting.dialogs[1].open_request, 2);

        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "other.lua".to_owned(),
                source: Arc::from(
                    "shoop_announce_api_version(1, 0); local d=require('shoop_dialog'); d.simple('Main',{d.rich_text('independent')})",
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(runtime.snapshot().scripting.dialogs.len(), 3);
        assert_ne!(
            runtime.snapshot().scripting.dialogs[0].id,
            runtime.snapshot().scripting.dialogs[2].id
        );

        runtime
            .dispatch(AppIntent::RestartScript { script_id: owner })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let restarted = runtime.snapshot();
        assert_eq!(restarted.scripting.dialogs.len(), 3);
        assert_ne!(restarted.scripting.dialogs[0].id, dialog_id);
        runtime
            .dispatch(AppIntent::InvokeScriptDialogButton {
                script_id: owner,
                dialog_id,
                button_id: *button_id,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().notifications.iter().any(|notification| {
            notification
                .message
                .contains("stale or unknown script dialog button")
        }));

        runtime
            .dispatch(AppIntent::StopScript { script_id: owner })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let stopped = runtime.snapshot();
        assert_eq!(stopped.scripting.dialogs.len(), 1);
        assert_eq!(stopped.scripting.dialogs[0].owner_script_name, "other.lua");
    }

    #[test]
    fn production_keyboard_script_handles_navigation_modes_numbers_targets_and_releases() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        for name in ["A", "B"] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: 2,
                    midi: false,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let first_track = runtime
            .snapshot()
            .tracks
            .iter()
            .find(|track| !track.is_sync)
            .unwrap()
            .id;
        for _ in 0..2 {
            runtime
                .dispatch(AppIntent::AddLoop {
                    track_id: first_track,
                })
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let sync = runtime
            .snapshot()
            .tracks
            .iter()
            .find(|track| track.is_sync)
            .unwrap()
            .clone();
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync.id,
                loop_id: sync.loops[0].id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(100));
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync.id,
                loop_id: sync.loops[0].id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(0)))
            .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "keyboard.lua".to_owned(),
                source: Arc::from(shoop_scripting::KEYBOARD_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let key = |runtime: &mut CooperativeApplicationRuntime,
                   key: i64,
                   modifiers: i64,
                   event_type: KeyEventType| {
            runtime
                .dispatch(AppIntent::KeyEvent(KeyEvent {
                    event_type,
                    key,
                    modifiers,
                }))
                .unwrap();
            runtime.tick(Duration::ZERO);
            runtime.tick(Duration::ZERO);
        };
        let press = |runtime: &mut CooperativeApplicationRuntime, key_value, modifiers| {
            key(runtime, key_value, modifiers, KeyEventType::Pressed)
        };
        press(&mut runtime, 16_777_236, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        press(&mut runtime, 16_777_236, 0);
        assert!(runtime.snapshot().tracks[2].loops[0].selected);
        press(&mut runtime, 16_777_234, 67_108_864);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        assert!(runtime.snapshot().tracks[2].loops[0].selected);
        press(&mut runtime, 16_777_236, 134_217_728);
        assert_eq!(
            runtime
                .snapshot()
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .filter(|loop_| loop_.selected)
                .count(),
            1
        );

        press(&mut runtime, 49, 0);
        press(&mut runtime, 50, 0);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 12);
        key(&mut runtime, 49, 0, KeyEventType::Released);
        key(&mut runtime, 50, 0, KeyEventType::Released);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(0)))
            .unwrap();
        runtime.tick(Duration::ZERO);

        press(&mut runtime, 84, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .any(|loop_| loop_.targeted));
        press(&mut runtime, 85, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| !loop_.targeted));
        press(&mut runtime, 82, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .any(|loop_| loop_.mode == LoopMode::Recording));
        press(&mut runtime, 83, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter(|loop_| loop_.selected)
            .all(|loop_| loop_.mode == LoopMode::Stopped));
        press(&mut runtime, 80, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter(|loop_| loop_.selected)
            .all(|loop_| loop_.mode == LoopMode::Playing));
        press(&mut runtime, 16_777_216, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| !loop_.selected));
        press(&mut runtime, 83, 0);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| loop_.mode == LoopMode::Stopped));

        press(&mut runtime, 16_777_236, 0);
        press(&mut runtime, 46, 0);
        assert_ne!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        key(&mut runtime, 46, 0, KeyEventType::Released);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        press(&mut runtime, 67, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].empty);
        press(&mut runtime, 32, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 83, 0);
        press(&mut runtime, 76, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::PlayingDryThroughWet
        );
        press(&mut runtime, 77, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::RecordingDryIntoWet
        );
        press(&mut runtime, 83, 0);
        press(&mut runtime, 71, 0);
        press(&mut runtime, 80, 0);
        press(&mut runtime, 78, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[1].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 79, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[1].mode,
            LoopMode::Playing
        );
        assert_eq!(
            runtime.snapshot().tracks[1].loops[2].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 84, 0);
        press(&mut runtime, 16_777_237, 0);
        press(&mut runtime, 87, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[1].mode,
            LoopMode::Recording
        );
        assert!(runtime.snapshot().scripting.scripts[0]
            .latest_error
            .is_none());
    }

    #[test]
    fn script_snapshot_distinguishes_current_and_planned_modes_for_loops_and_composites() {
        let backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Track".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loop_id = runtime.model.tracks[1].loops[0];
        let backend_id = runtime.model.loops[&loop_id].backend_id;

        runtime.backend.set_loop_length(backend_id, 480).unwrap();
        runtime
            .backend
            .transition_loop(backend_id, BackendLoopMode::Playing, None)
            .unwrap();
        runtime.tick(Duration::ZERO);
        let stable = runtime
            .model
            .script_control_snapshot()
            .loops
            .into_iter()
            .find(|loop_| loop_.id == loop_id)
            .unwrap();
        assert_eq!(stable.mode, LoopMode::Playing);
        assert_eq!(stable.next_mode, None);
        assert_eq!(stable.next_mode_delay, None);

        let composite_id = runtime.model.tracks[1].loops[1];
        runtime
            .model
            .apply_script_operation(
                &mut *runtime.backend,
                ControlOperation::ComposeAddToEnd {
                    target: composite_id,
                    add: vec![loop_id],
                    parallel: false,
                },
            )
            .unwrap();
        runtime
            .backend
            .transition_loop(backend_id, BackendLoopMode::Recording, Some(2))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let planned = runtime
            .model
            .script_control_snapshot()
            .loops
            .into_iter()
            .find(|loop_| loop_.id == loop_id)
            .unwrap();
        assert_eq!(planned.mode, LoopMode::Playing);
        assert_eq!(planned.next_mode, Some(LoopMode::Recording));
        assert_eq!(planned.next_mode_delay, Some(2));
        let composite = runtime
            .model
            .script_control_snapshot()
            .loops
            .into_iter()
            .find(|loop_| loop_.id == composite_id)
            .unwrap();
        assert_eq!(composite.mode, LoopMode::Playing);
        assert_eq!(composite.next_mode, Some(LoopMode::Recording));
        assert_eq!(composite.next_mode_delay, Some(2));
    }

    #[test]
    fn production_keyboard_plays_manual_recording_on_next_sync_cycle() {
        let backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(backend),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
                kind: ScriptKind::Bundled,
                enabled: true,
            }],
        )
        .unwrap();
        let sync_backend_id = runtime
            .model
            .loops
            .get(&runtime.model.tracks[0].loops[0])
            .unwrap()
            .backend_id;
        runtime
            .backend
            .set_loop_length(sync_backend_id, 480)
            .unwrap();
        runtime
            .backend
            .transition_loop(sync_backend_id, BackendLoopMode::Playing, None)
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Track".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = runtime.snapshot().tracks[1].clone();
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().global_controls.play_after_record);

        let space = |runtime: &mut CooperativeApplicationRuntime| {
            for event_type in [KeyEventType::Pressed, KeyEventType::Released] {
                runtime
                    .dispatch(AppIntent::KeyEvent(KeyEvent {
                        event_type,
                        key: 32,
                        modifiers: 0,
                    }))
                    .unwrap();
                runtime.tick(Duration::ZERO);
                runtime.tick(Duration::ZERO);
            }
        };
        space(&mut runtime);
        runtime.tick(Duration::from_millis(11));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Recording
        );

        space(&mut runtime);
        runtime.tick(Duration::from_millis(10));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
    }

    #[test]
    fn regular_script_composition_plays_serial_sections_and_wraps_without_sync() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        for name in ["Target", "Source A", "Source B"] {
            model
                .add_track(
                    &mut backend,
                    DirectTrackSpec {
                        name: name.to_owned(),
                        audio_channels: 1,
                        midi: false,
                    },
                )
                .unwrap();
        }
        model.global.sync = false;
        let target = model.tracks[1].loops[0];
        let source_a = model.tracks[2].loops[0];
        let source_b = model.tracks[3].loops[0];
        model.loops.get_mut(&source_a).unwrap().length = 480;
        model.loops.get_mut(&source_b).unwrap().length = 480;
        model
            .apply_script_operation(
                &mut backend,
                ControlOperation::ComposeAddToEnd {
                    target,
                    add: vec![source_a, source_b],
                    parallel: false,
                },
            )
            .unwrap();
        let before = backend.operations().len();
        model
            .script_trigger_loops(&mut backend, &[target], LoopMode::Playing)
            .unwrap();
        let source_a_backend = model.loops[&source_a].backend_id;
        let source_b_backend = model.loops[&source_b].backend_id;
        assert_eq!(
            &backend.operations()[before..],
            [
                shoop_backend::FakeOperation::Transition(
                    source_b_backend,
                    BackendLoopMode::Stopped,
                    None,
                ),
                shoop_backend::FakeOperation::Transition(
                    source_a_backend,
                    BackendLoopMode::Playing,
                    None,
                ),
            ]
        );

        update_application(&mut model, &mut backend, Duration::from_millis(10), |_| {});
        assert!(backend.operations().ends_with(&[
            shoop_backend::FakeOperation::Transition(
                source_a_backend,
                BackendLoopMode::Stopped,
                None,
            ),
            shoop_backend::FakeOperation::Transition(
                source_b_backend,
                BackendLoopMode::Playing,
                None,
            ),
        ]));
        update_application(&mut model, &mut backend, Duration::ZERO, |_| {});
        assert_eq!(model.loops[&target].state.mode, LoopMode::Playing);
        update_application(&mut model, &mut backend, Duration::from_millis(10), |_| {});
        assert!(backend.operations().ends_with(&[
            shoop_backend::FakeOperation::Transition(
                source_b_backend,
                BackendLoopMode::Stopped,
                None,
            ),
            shoop_backend::FakeOperation::Transition(
                source_a_backend,
                BackendLoopMode::Playing,
                None,
            ),
        ]));
        model
            .script_trigger_loops(&mut backend, &[target], LoopMode::Stopped)
            .unwrap();
        assert!(!model.script_composition_playback.contains_key(&target));
    }

    #[test]
    fn script_composition_append_and_parallel_execute_on_engine_backend() {
        let backend = shoop_backend::EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        for name in ["Target", "Source A", "Source B"] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: 1,
                    midi: false,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let tracks = runtime
            .snapshot()
            .tracks
            .iter()
            .filter(|track| !track.is_sync)
            .cloned()
            .collect::<Vec<_>>();
        for track in &tracks[1..] {
            runtime
                .dispatch(AppIntent::Loop {
                    track_id: track.id,
                    loop_id: track.loops[0].id,
                    action: LoopAction::RecordClicked,
                })
                .unwrap();
        }
        runtime.tick(Duration::from_millis(20));
        for track in &tracks[1..] {
            runtime
                .dispatch(AppIntent::Loop {
                    track_id: track.id,
                    loop_id: track.loops[0].id,
                    action: LoopAction::StopClicked,
                })
                .unwrap();
        }
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "composition.lua".to_owned(),
                source: Arc::from(
                    "shoop_announce_api_version(1, 0); local c=require('shoop_control'); c.loop_compose_add_to_end({0,0},{1,0},false); c.loop_compose_add_to_end({0,0},{2,0},true); c.loop_trigger({0,0},c.constants.LoopMode_Playing)",
                ),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::from_millis(20));
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.tracks[1].loops[0].composite_kind,
            shoop_app_api::CompositeKind::Regular
        );
        assert_eq!(snapshot.tracks[2].loops[0].mode, LoopMode::Playing);
        assert_eq!(snapshot.tracks[3].loops[0].mode, LoopMode::Playing);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..20 {
            runtime.tick(Duration::from_millis(1));
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
            .unwrap_or_else(|| panic!("save did not complete: {:?}", runtime.snapshot().io_task));
        let saved = decode_session(&output.bytes).unwrap();
        let composite = saved.document.track_groups[1].tracks[0].loops[0]
            .composite
            .as_ref()
            .unwrap();
        assert_eq!(composite.kind, CompositeKindDocument::Regular);
        assert_eq!(composite.playlists[0][0].len(), 2);
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "composition.shoop".to_owned(),
                bytes: output.bytes,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].composite_kind,
            shoop_app_api::CompositeKind::Regular
        );
    }

    #[test]
    fn unchanged_apc_script_drives_authoritative_state_and_bounded_led_output() {
        let (midi, midi_control) = shoop_scripting::FakeMidiService::new();
        midi_control.set_endpoints(vec![
            shoop_scripting::MidiEndpoint {
                id: "apc-source".to_owned(),
                name: "AKAI APC MINI MIDI controller".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Output,
            },
            shoop_scripting::MidiEndpoint {
                id: "apc-sink".to_owned(),
                name: "AKAI APC MINI MIDI controller".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Input,
            },
        ]);
        let mut runtime = CooperativeApplicationRuntime::start_with_midi(
            Box::new(FakeBackend::default()),
            Box::new(midi),
        )
        .unwrap();
        for index in 0..8 {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: format!("Track {index}"),
                    audio_channels: 2,
                    midi: false,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let first_track = runtime
            .snapshot()
            .tracks
            .iter()
            .find(|track| !track.is_sync)
            .unwrap()
            .id;
        for _ in 1..8 {
            runtime
                .dispatch(AppIntent::AddLoop {
                    track_id: first_track,
                })
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(0)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        assert_eq!(
            runtime.snapshot().scripting.scripts[0].lifecycle,
            shoop_app_api::ScriptLifecycle::Listening
        );
        assert_eq!(runtime.snapshot().scripting.scripts[0].midi.connections, 2);
        let connected_snapshot = runtime.snapshot();
        let script_id = connected_snapshot.scripting.scripts[0].id;
        let control_ports: Vec<_> = connected_snapshot
            .connections
            .application_ports
            .iter()
            .filter(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::LuaControl {
                        script_id: owner, ..
                    } if owner == script_id
                )
            })
            .collect();
        assert_eq!(control_ports.len(), 2);
        assert!(control_ports
            .iter()
            .all(|port| port.connection_policy == ConnectionPolicy::OwnerManaged));
        assert_eq!(
            connected_snapshot
                .connections
                .confirmed_links
                .iter()
                .filter(|link| control_ports
                    .iter()
                    .any(|port| port.id == link.application_port_id))
                .count(),
            2
        );
        assert!(connected_snapshot
            .connections
            .host_ports
            .iter()
            .any(|host| host.name == "AKAI APC MINI MIDI controller"));
        runtime.tick(Duration::from_millis(1_000));
        let mut reset = midi_control.take_sent();
        assert!(reset.len() <= 1, "positive MIDI rate limit emitted a burst");
        for _ in 0..67 {
            runtime.tick(Duration::from_millis(1));
            let batch = midi_control.take_sent();
            assert!(batch.len() <= 1, "positive MIDI rate limit emitted a burst");
            reset.extend(batch);
        }
        assert!(reset.len() >= 67);
        assert!(reset.iter().all(|(_, message)| message.len() == 3));

        let send_note = |runtime: &mut CooperativeApplicationRuntime,
                         midi: &shoop_scripting::FakeMidiControl,
                         note: u8,
                         pressed: bool| {
            midi.push_input(
                "apc-source",
                vec![if pressed { 0x90 } else { 0x80 }, note, 0x7f],
            );
            runtime.tick(Duration::from_millis(2));
            runtime.tick(Duration::ZERO);
        };
        let grid_note = 56;
        send_note(&mut runtime, &midi_control, grid_note, true);
        let after_default = runtime.snapshot();
        assert_eq!(
            after_default.tracks[1].loops[0].mode,
            LoopMode::Recording,
            "script={:?} notifications={:?}",
            after_default.scripting.scripts[0],
            after_default.notifications
        );
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 82, false);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );

        send_note(&mut runtime, &midi_control, 86, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 86, false);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 86, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 86, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(runtime.snapshot().tracks[1].loops[0].targeted);

        send_note(&mut runtime, &midi_control, 71, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 71, false);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 1);

        send_note(&mut runtime, &midi_control, 68, true);
        midi_control.push_input("apc-source", vec![0xb0, 48, 127]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 68, false);
        assert_eq!(runtime.snapshot().tracks[1].controls.output_gain_db, 20.0);
        assert!(runtime.snapshot().tracks[1].controls.output_muted);
        send_note(&mut runtime, &midi_control, 69, true);
        midi_control.push_input("apc-source", vec![0xb0, 48, 127]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 69, false);
        assert_eq!(runtime.snapshot().tracks[1].controls.output_balance, 1.0);
        assert!(runtime.snapshot().tracks[1].controls.input_monitoring);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(0)))
            .unwrap();
        runtime.tick(Duration::ZERO);

        send_note(&mut runtime, &midi_control, 84, true);
        send_note(&mut runtime, &midi_control, 57, true);
        send_note(&mut runtime, &midi_control, 84, false);
        assert_eq!(
            runtime.snapshot().tracks[2].loops[0].mode,
            LoopMode::Recording
        );
        let mut saw_recording_led = midi_control
            .take_sent()
            .iter()
            .any(|(_, message)| message == &[0x90, 57, 3]);
        for _ in 0..1_024 {
            if saw_recording_led {
                break;
            }
            runtime.tick(Duration::from_millis(1));
            let batch = midi_control.take_sent();
            assert!(batch.len() <= 1, "positive MIDI rate limit emitted a burst");
            saw_recording_led = batch.iter().any(|(_, message)| message == &[0x90, 57, 3]);
        }
        assert!(saw_recording_led);
        send_note(&mut runtime, &midi_control, 84, true);
        send_note(&mut runtime, &midi_control, 70, true);
        send_note(&mut runtime, &midi_control, 59, true);
        send_note(&mut runtime, &midi_control, 70, false);
        send_note(&mut runtime, &midi_control, 84, false);
        assert_eq!(
            runtime.snapshot().tracks[4].loops[0].mode,
            LoopMode::RecordingDryIntoWet
        );
        send_note(&mut runtime, &midi_control, 89, true);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| loop_.mode == LoopMode::Stopped));
        send_note(&mut runtime, &midi_control, 86, true);
        send_note(&mut runtime, &midi_control, 89, true);
        send_note(&mut runtime, &midi_control, 86, false);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| !loop_.selected));
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 85, true);
        send_note(&mut runtime, &midi_control, 58, true);
        send_note(&mut runtime, &midi_control, 85, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert_eq!(
            runtime.snapshot().global_controls.default_recording_action,
            shoop_app_api::DefaultRecordingAction::Grab
        );

        send_note(&mut runtime, &midi_control, 87, true);
        assert!(runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 87, false);
        assert!(!runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 87, true);
        send_note(&mut runtime, &midi_control, 87, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(runtime.snapshot().global_controls.sync);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);

        send_note(&mut runtime, &midi_control, 83, true);
        assert!(runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 83, false);
        assert!(!runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 83, true);
        send_note(&mut runtime, &midi_control, 83, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(runtime.snapshot().global_controls.solo);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetDefaultRecordingAction(
                    shoop_app_api::DefaultRecordingAction::Record,
                ),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        send_note(&mut runtime, &midi_control, 88, true);
        assert_eq!(
            runtime.snapshot().tracks[0].loops[0].mode,
            LoopMode::Recording
        );
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 89, true);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(runtime
            .snapshot()
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| loop_.empty));

        send_note(&mut runtime, &midi_control, 71, true);
        send_note(&mut runtime, &midi_control, 7, true);
        send_note(&mut runtime, &midi_control, 71, false);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 0);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 70, true);
        send_note(&mut runtime, &midi_control, 56, true);
        send_note(&mut runtime, &midi_control, 57, true);
        send_note(&mut runtime, &midi_control, 58, true);
        send_note(&mut runtime, &midi_control, 58, false);
        send_note(&mut runtime, &midi_control, 57, false);
        send_note(&mut runtime, &midi_control, 70, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].composite_kind,
            shoop_app_api::CompositeKind::Regular
        );
        let target_id = runtime.snapshot().tracks[1].loops[0].id;
        let composed_sources = &runtime.model.loops[&target_id].script_composition[0];
        assert_eq!(composed_sources.len(), 2);
        assert_eq!(
            composed_sources[0],
            runtime.snapshot().tracks[2].loops[0].id
        );
        assert_eq!(
            composed_sources[1],
            runtime.snapshot().tracks[3].loops[0].id
        );

        // Re-enter composition mode and release each source before pressing the next one,
        // proving the production controller's regular serial append path as well.
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 70, true);
        send_note(&mut runtime, &midi_control, 59, true);
        send_note(&mut runtime, &midi_control, 59, false);
        send_note(&mut runtime, &midi_control, 60, true);
        send_note(&mut runtime, &midi_control, 60, false);
        send_note(&mut runtime, &midi_control, 61, true);
        send_note(&mut runtime, &midi_control, 61, false);
        send_note(&mut runtime, &midi_control, 70, false);
        send_note(&mut runtime, &midi_control, 98, false);
        let serial_target = runtime.snapshot().tracks[4].loops[0].id;
        let serial_sections = &runtime.model.loops[&serial_target].script_composition;
        assert_eq!(serial_sections.len(), 2);
        assert_eq!(
            serial_sections[0],
            [runtime.snapshot().tracks[5].loops[0].id]
        );
        assert_eq!(
            serial_sections[1],
            [runtime.snapshot().tracks[6].loops[0].id]
        );
        assert!(runtime.snapshot().scripting.scripts[0]
            .latest_error
            .is_none());
        send_note(&mut runtime, &midi_control, 56, true);
        let composition_play = runtime.snapshot();
        assert_eq!(
            composition_play.tracks[2].loops[0].mode,
            LoopMode::Recording
        );

        assert!(!midi_control.take_sent().is_empty());
        midi_control.set_endpoints(Vec::new());
        runtime.tick(Duration::from_millis(500));
        assert_eq!(midi_control.active_connections(), 0);
        midi_control.set_endpoints(vec![
            shoop_scripting::MidiEndpoint {
                id: "apc-source".to_owned(),
                name: "AKAI APC MINI MIDI controller".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Output,
            },
            shoop_scripting::MidiEndpoint {
                id: "apc-sink".to_owned(),
                name: "AKAI APC MINI MIDI controller".to_owned(),
                direction: shoop_scripting::MidiEndpointDirection::Input,
            },
        ]);
        runtime.tick(Duration::from_millis(500));
        assert_eq!(midi_control.active_connections(), 2);
        let mut reconnect_reset = midi_control.take_sent();
        assert!(
            reconnect_reset.len() <= 1,
            "positive MIDI rate limit emitted a burst"
        );
        runtime.tick(Duration::from_millis(1_000));
        let batch = midi_control.take_sent();
        assert!(batch.len() <= 1, "positive MIDI rate limit emitted a burst");
        reconnect_reset.extend(batch);
        for _ in 0..67 {
            runtime.tick(Duration::from_millis(1));
            let batch = midi_control.take_sent();
            assert!(batch.len() <= 1, "positive MIDI rate limit emitted a burst");
            reconnect_reset.extend(batch);
        }
        assert!(reconnect_reset.len() >= 67);
        let script_id = runtime.snapshot().scripting.scripts[0].id;
        runtime
            .dispatch(AppIntent::StopScript { script_id })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(midi_control.active_connections(), 0);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unchanged_apc_script_uses_native_virtual_midi_when_available() {
        use midir::os::unix::{VirtualInput, VirtualOutput};

        let input = match midir::MidiInput::new("Shoop APC test sink") {
            Ok(input) => input,
            Err(error) => {
                eprintln!("SKIP native APC MIDI test: {error}");
                return;
            }
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        let sink = match input.create_virtual(
            "APC MINI MIDI shoop test sink",
            move |_, message, _| {
                let _ = sender.send(message.to_vec());
            },
            (),
        ) {
            Ok(sink) => sink,
            Err(error) => {
                eprintln!("SKIP native APC MIDI test: {error}");
                return;
            }
        };
        let output = match midir::MidiOutput::new("Shoop APC test source") {
            Ok(output) => output,
            Err(error) => {
                eprintln!("SKIP native APC MIDI test: {error}");
                return;
            }
        };
        let mut source = match output.create_virtual("APC MINI MIDI shoop test source") {
            Ok(source) => source,
            Err(error) => {
                eprintln!("SKIP native APC MIDI test: {error}");
                return;
            }
        };
        let mut runtime = CooperativeApplicationRuntime::start_with_midi(
            Box::new(FakeBackend::default()),
            Box::new(NativeMidiService::new()),
        )
        .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(shoop_scripting::AKAI_APC_MINI_MK1_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(500));
        if runtime.snapshot().scripting.scripts[0].midi.connections != 2 {
            eprintln!("SKIP native APC MIDI test: virtual endpoints were not discoverable");
            return;
        }
        source.send(&[0x90, 83, 0x7f]).unwrap();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(5));
            runtime.tick(Duration::from_millis(5));
            if runtime.snapshot().global_controls.solo {
                break;
            }
        }
        assert!(runtime.snapshot().global_controls.solo);
        source.send(&[0x80, 83, 0]).unwrap();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(5));
            runtime.tick(Duration::from_millis(5));
            if !runtime.snapshot().global_controls.solo {
                break;
            }
        }
        assert!(!runtime.snapshot().global_controls.solo);
        runtime.tick(Duration::from_millis(1_000));
        assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());
        drop(sink);
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
            .application_ports
            .iter()
            .all(|port| matches!(
                port.owner,
                ApplicationPortOwner::Track { track_id, .. }
                    if track_id == snapshot.tracks[0].id
            )));
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
    fn auto_mute_other_track_inputs_is_respected_per_monitoring_request() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        for name in ["first", "second", "third"] {
            model.handle_intent(
                &mut backend,
                AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: 1,
                    midi: false,
                }),
            );
        }
        model.apply_backend_snapshot(backend.poll().unwrap());
        let first = model.tracks[1].id;
        let second = model.tracks[2].id;
        let third = model.tracks[3].id;
        let monitoring = |model: &ApplicationModel| {
            model
                .tracks
                .iter()
                .map(|track| track.controls.input_monitoring)
                .collect::<Vec<_>>()
        };

        model
            .handle_track_input_monitoring(&mut backend, &[first, second], true, false)
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, true, true, false]);

        model
            .handle_global_action(
                &mut backend,
                GlobalControlAction::SetAutoMuteOtherTrackInputs(true),
            )
            .unwrap();
        assert_eq!(monitoring(&model), [false, true, true, false]);

        model
            .handle_track_action(
                &mut backend,
                third,
                TrackAction::InputMonitoringChanged {
                    enabled: true,
                    respect_auto_mute: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, false, false, true]);

        model
            .handle_track_input_monitoring(&mut backend, &[first], true, false)
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, true, false, true]);

        model
            .handle_track_input_monitoring(&mut backend, &[first], false, true)
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, false, false, true]);

        model
            .handle_track_input_monitoring(&mut backend, &[first, second], true, true)
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, true, true, false]);

        model
            .handle_global_action(
                &mut backend,
                GlobalControlAction::SetAutoMuteOtherTrackInputs(false),
            )
            .unwrap();
        assert_eq!(monitoring(&model), [false, true, true, false]);
        model
            .handle_track_input_monitoring(&mut backend, &[third], true, true)
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(monitoring(&model), [false, true, true, true]);
    }

    #[test]
    fn piano_fanout_tracks_original_monitored_midi_recipients() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::EXTERNAL,
            ),
            label: "External".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: None,
                max_wet_audio_channels: None,
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
            },
            features: shoop_app_api::TrackProcessorFeatures::default(),
            editor: None,
        }]);
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        for (name, audio_channels, midi) in [
            ("first", 0, true),
            ("second", 0, true),
            ("later", 0, true),
            ("audio", 1, false),
        ] {
            model.handle_intent(
                &mut backend,
                AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels,
                    midi,
                }),
            );
        }
        model.handle_intent(
            &mut backend,
            AppIntent::AddTrackWithTopology(TrackSpec {
                name: "processed".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 0,
                    wet_audio_channels: 0,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::EXTERNAL,
                    ),
                },
            }),
        );
        let first = model.tracks[1].id;
        let second = model.tracks[2].id;
        let later = model.tracks[3].id;
        let audio = model.tracks[4].id;
        let processed = model.tracks[5].id;
        for track_id in [first, second, audio, processed] {
            model.handle_intent(
                &mut backend,
                AppIntent::Track {
                    track_id,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                },
            );
        }
        model.apply_backend_snapshot(backend.poll().unwrap());
        let operation_start = backend.operations().len();
        let note = shoop_app_api::MidiNote::new(60).unwrap();
        assert!(model
            .handle_piano_action(&mut backend, PianoAction::Press(note))
            .is_ok());
        assert!(model
            .handle_piano_action(&mut backend, PianoAction::Press(note))
            .is_ok());
        let pressed = backend.operations()[operation_start..]
            .iter()
            .filter_map(|operation| match operation {
                shoop_backend::FakeOperation::InjectMidiInput(track, events) => {
                    Some((*track, events[0].data.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            pressed,
            vec![
                (model.tracks[1].backend_id, vec![0x90, 60, 100]),
                (model.tracks[2].backend_id, vec![0x90, 60, 100]),
                (model.tracks[5].backend_id, vec![0x90, 60, 100]),
            ]
        );

        for (track_id, monitoring) in [(first, false), (later, true)] {
            model.handle_intent(
                &mut backend,
                AppIntent::Track {
                    track_id,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: monitoring,
                        respect_auto_mute: false,
                    },
                },
            );
        }
        model.apply_backend_snapshot(backend.poll().unwrap());
        let release_start = backend.operations().len();
        assert!(model
            .handle_piano_action(&mut backend, PianoAction::Release(note))
            .is_ok());
        assert!(model
            .handle_piano_action(&mut backend, PianoAction::Release(note))
            .is_ok());
        let released = backend.operations()[release_start..]
            .iter()
            .filter_map(|operation| match operation {
                shoop_backend::FakeOperation::InjectMidiInput(track, events) => {
                    Some((*track, events[0].data.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            released,
            vec![
                (model.tracks[1].backend_id, vec![0x80, 60, 0]),
                (model.tracks[2].backend_id, vec![0x80, 60, 0]),
                (model.tracks[5].backend_id, vec![0x80, 60, 0]),
            ]
        );
    }

    #[test]
    fn piano_partial_failure_keeps_successful_recipients_releasable() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        for name in ["fails", "works"] {
            model.handle_intent(
                &mut backend,
                AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: 0,
                    midi: true,
                }),
            );
            let track_id = model.tracks.last().unwrap().id;
            model.handle_intent(
                &mut backend,
                AppIntent::Track {
                    track_id,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                },
            );
        }
        model.apply_backend_snapshot(backend.poll().unwrap());
        let failed_backend_id = model.tracks[1].backend_id;
        let successful_backend_id = model.tracks[2].backend_id;
        backend.fail_midi_input_for(failed_backend_id);
        let note = shoop_app_api::MidiNote::new(64).unwrap();
        let error = model
            .handle_piano_action(&mut backend, PianoAction::Press(note))
            .unwrap_err();
        assert!(error.contains("1 track(s)"));
        let release_start = backend.operations().len();
        assert!(model
            .handle_piano_action(&mut backend, PianoAction::ReleaseAll)
            .is_ok());
        assert!(backend.operations()[release_start..].contains(
            &shoop_backend::FakeOperation::InjectMidiInput(
                successful_backend_id,
                vec![BackendMidiEvent {
                    time: 0,
                    data: vec![0x80, 64, 0],
                }],
            )
        ));
        assert!(!model.active_piano_notes.contains_key(&64));
    }

    #[test]
    fn engine_backed_piano_fanout_records_into_each_monitored_midi_track() {
        let backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        for (name, midi) in [("first", true), ("second", true), ("audio", false)] {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: name.to_owned(),
                    audio_channels: u32::from(!midi),
                    midi,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let tracks = runtime.snapshot().tracks[1..].to_vec();
        for track in &tracks {
            runtime
                .dispatch(AppIntent::Track {
                    track_id: track.id,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                })
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let midi_loops = runtime.model.tracks[1..3]
            .iter()
            .map(|track| runtime.model.loops[&track.loops[0]].backend_id)
            .collect::<Vec<_>>();
        for loop_id in &midi_loops {
            runtime
                .backend
                .transition_loop(*loop_id, BackendLoopMode::Recording, None)
                .unwrap();
        }
        let note = shoop_app_api::MidiNote::new(67).unwrap();
        runtime
            .dispatch(AppIntent::Piano(PianoAction::Press(note)))
            .unwrap();
        runtime.tick(Duration::from_millis(4));
        runtime
            .dispatch(AppIntent::Piano(PianoAction::Release(note)))
            .unwrap();
        runtime.tick(Duration::from_millis(4));
        for loop_id in &midi_loops {
            runtime
                .backend
                .transition_loop(*loop_id, BackendLoopMode::Stopped, None)
                .unwrap();
        }
        let captured = runtime.backend.capture_session().unwrap();
        for track in &captured.tracks[1..3] {
            let events = &track.loops[0].midi[0].events;
            assert!(events.iter().any(|event| event.data == [0x90, 67, 100]));
            assert!(events.iter().any(|event| event.data == [0x80, 67, 0]));
        }
        assert!(captured.tracks[3].loops[0].midi.is_empty());
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
        let details = snapshot.details.as_ref().unwrap();
        assert_eq!(details.channels.len(), 2);
        assert_eq!(details.midi_channels.len(), 1);
        assert!(!details.loading);
        assert!(!details.midi_loading);

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
    fn midi_only_selection_publishes_immutable_midi_details() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "MIDI only".to_owned(),
                    audio_channels: 0,
                    midi: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let track_id = model.tracks[1].id;
        let loop_id = model.tracks[1].loops[0];
        let backend_loop = model.loops[&loop_id].backend_id;
        backend
            .replace_loop_content(
                backend_loop,
                &BackendLoopContentUpdate {
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 24,
                        start_state: Vec::new(),
                        events: vec![
                            BackendMidiEvent {
                                time: 2,
                                data: vec![0x90, 67, 99],
                            },
                            BackendMidiEvent {
                                time: 18,
                                data: vec![0x80, 67, 0],
                            },
                        ],
                        start_offset: Some(-3),
                        preplay: Some(4),
                    }],
                    length: Some(24),
                    ..Default::default()
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .handle_loop_action(
                &mut backend,
                track_id,
                loop_id,
                LoopAction::IconClicked(SelectionModifiers::default()),
            )
            .unwrap();

        let details = model.details_snapshot().unwrap();
        assert!(details.channels.is_empty());
        assert!(!details.loading);
        assert!(!details.midi_loading);
        assert_eq!(details.midi_channels.len(), 1);
        let channel = &details.midi_channels[0];
        assert_eq!(channel.start_offset, -3);
        assert_eq!(channel.loop_length, 24);
        assert_eq!(channel.events.len(), 2);
        assert_eq!(channel.events[0].data.as_ref(), [0x90, 67, 99]);
        assert_eq!(
            backend.capture_session().unwrap().tracks[1].loops[0].midi[0]
                .events
                .len(),
            2
        );
        let original_events = Arc::clone(&channel.events);
        model
            .apply_script_operation(
                &mut backend,
                ControlOperation::ClearLoops {
                    loops: vec![loop_id],
                },
            )
            .unwrap();
        model.refresh_selected_media(&mut backend).unwrap();
        let cleared = model.details_snapshot().unwrap();
        assert!(cleared.midi_channels[0].events.is_empty());
        assert!(!Arc::ptr_eq(
            &cleared.midi_channels[0].events,
            &original_events
        ));
    }

    #[test]
    fn target_delay_is_derived_from_target_and_sync_lengths() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
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
    fn expanded_loop_actions_route_modes_grab_and_balance() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
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
            !snapshot.connections.loading && !snapshot.connections.application_ports.is_empty()
        });
        assert!(initial.connections.backend_available);
        assert!(initial.connections.application_ports.iter().all(|port| {
            matches!(
                port.owner,
                ApplicationPortOwner::Track { track_id, .. }
                    if track_id == initial.tracks[0].id
            ) && initial.tracks[0].port_ids.contains(&port.id)
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
                    .application_ports
                    .iter()
                    .filter(|port| {
                        matches!(
                            port.owner,
                            ApplicationPortOwner::Track { track_id, .. }
                                if track_id == snapshot.tracks[1].id
                        )
                    })
                    .count()
                    == 6
        });
        let track = &snapshot.tracks[1];
        assert_eq!(track.port_ids.len(), 6);
        let input = snapshot
            .connections
            .application_ports
            .iter()
            .find(|port| {
                matches!(
                    port.owner,
                    ApplicationPortOwner::Track { track_id, .. } if track_id == track.id
                ) && port.role == PortRole::AudioInput
            })
            .unwrap();
        let input_id = input.id;
        let input_name = input.name.clone();
        assert!(snapshot
            .connections
            .host_ports
            .iter()
            .any(|host| host.id.as_str() == "system:capture_1"));
        assert!(snapshot
            .connections
            .host_ports
            .iter()
            .any(|host| host.id.as_str() == "system:playback_1"));
        assert_ne!(
            input.direction,
            snapshot
                .connections
                .host_ports
                .iter()
                .find(|host| host.id.as_str() == "system:capture_1")
                .unwrap()
                .direction
        );

        control.defer_mutations(true);
        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: input_id,
                host_port_id: HostPortId::new("system:capture_1"),
                connected: true,
            })
            .unwrap();
        let pending = wait_for(&handle, |snapshot| {
            snapshot.connections.pending_links.iter().any(|link| {
                link.application_port_id == input_id
                    && link.host_port_id.as_str() == "system:capture_1"
                    && link.desired_connected
            })
        });
        let held_revision = pending.connections.revision;
        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: input_id,
                host_port_id: HostPortId::new("system:capture_1"),
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
        assert!(!failed.connections.confirmed_links.iter().any(|link| {
            link.application_port_id == input_id && link.host_port_id.as_str() == "system:capture_1"
        }));

        control.defer_mutations(false);
        control.add_external_port(
            "new-client:audio_source",
            BackendPortDirection::Output,
            BackendPortDataType::Audio,
        );
        wait_for(&handle, |snapshot| {
            snapshot
                .connections
                .host_ports
                .iter()
                .any(|host| host.id.as_str() == "new-client:audio_source")
        });
        let backend_port = control.port_id_by_name(&input_name).unwrap();
        control.externally_set_connected(backend_port, "new-client:audio_source", true);
        wait_for(&handle, |snapshot| {
            snapshot.connections.confirmed_links.iter().any(|link| {
                link.application_port_id == input_id
                    && link.host_port_id.as_str() == "new-client:audio_source"
            })
        });
        control.remove_external_port("new-client:audio_source");
        wait_for(&handle, |snapshot| {
            !snapshot
                .connections
                .host_ports
                .iter()
                .any(|host| host.id.as_str() == "new-client:audio_source")
        });

        handle
            .dispatch(AppIntent::SetPortConnected {
                port_id: PortId::from_raw(999_999),
                host_port_id: HostPortId::new("system:capture_1"),
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
            .application_ports
            .iter()
            .find(|port| port.role == PortRole::AudioInput)
            .unwrap();
        let port_id = port.id;
        runtime
            .dispatch(AppIntent::SetPortConnected {
                port_id,
                host_port_id: HostPortId::new("system:capture_1"),
                connected: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime
            .snapshot()
            .connections
            .pending_links
            .iter()
            .any(|link| {
                link.application_port_id == port_id
                    && link.host_port_id.as_str() == "system:capture_1"
                    && link.desired_connected
            }));
        runtime.tick(CONNECTION_TIMEOUT);
        let timed_out = runtime.snapshot();
        assert!(timed_out.connections.errors.iter().any(|error| {
            error.port_id == Some(port_id) && error.kind == ConnectionErrorKind::TimedOut
        }));
        assert!(!timed_out.connections.confirmed_links.iter().any(|link| {
            link.application_port_id == port_id && link.host_port_id.as_str() == "system:capture_1"
        }));
        assert!(!timed_out.connections.pending_links.iter().any(|link| {
            link.application_port_id == port_id && link.host_port_id.as_str() == "system:capture_1"
        }));
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
            &first.connections.application_ports,
            &second.connections.application_ports
        ));
    }

    #[test]
    fn audio_driver_switch_requires_confirmation_and_resamples_transactionally() {
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                source: shoop_scripting::KEYBOARD_SCRIPT.to_owned(),
                kind: ScriptKind::Bundled,
                enabled: true,
            }],
        )
        .unwrap();
        runtime.tick(Duration::ZERO);
        let initial = runtime.snapshot();
        let track_id = initial.tracks[0].id;
        runtime
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::NameChanged("preserved sync track".to_owned()),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Track {
                track_id,
                action: TrackAction::OutputGainChanged(-7.5),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSolo(true)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let before = runtime.snapshot();
        let track_ids = before
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>();
        let script = before.scripting.scripts[0].clone();
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let warning = runtime.snapshot();
        assert_eq!(
            warning.audio_drivers.switch.status,
            AudioDriverSwitchStatus::AwaitingConfirmation
        );
        assert!(warning
            .audio_drivers
            .switch
            .message
            .contains("48000 Hz to 32000 Hz"));
        assert!(warning
            .audio_drivers
            .switch
            .message
            .contains("All loop audio, MIDI timing"));
        assert_eq!(warning.status.sample_rate, 48_000);
        let request_id = warning.audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let switched = runtime.snapshot();
        assert_eq!(
            switched.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Persisting
        );
        assert_eq!(
            switched.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
        assert_eq!(
            switched
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            track_ids
        );
        assert_eq!(switched.tracks[0].name, "preserved sync track");
        assert_eq!(switched.tracks[0].controls.output_gain_db, -7.5);
        assert!(switched.global_controls.solo);
        assert_eq!(switched.scripting.scripts[0].id, script.id);
        assert_eq!(switched.scripting.scripts[0].name, script.name);
        assert_eq!(switched.scripting.scripts[0].lifecycle, script.lifecycle);
        assert!(switched
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .all(|loop_| loop_.mode == LoopMode::Stopped));
        runtime
            .dispatch(AppIntent::CompleteAudioDriverSwitchPersistence {
                request_id,
                success: true,
                message: "saved".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().audio_drivers.switch.status,
            AudioDriverSwitchStatus::Completed
        );
    }

    #[test]
    fn engine_driver_switch_scales_recorded_loop_length_with_existing_resampler() {
        let backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        let sync = &runtime.snapshot().tracks[0];
        let track_id = sync.id;
        let loop_id = sync.loops[0].id;
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(10));
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(Default::default()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().details.as_ref().unwrap().channels[0].loop_length,
            480
        );
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 24_000,
                    buffer_size: 64,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::ZERO);
        let switched = runtime.snapshot();
        assert_eq!(switched.status.sample_rate, 24_000);
        assert_eq!(
            switched.details.as_ref().unwrap().channels[0].loop_length,
            240
        );
        assert_eq!(
            switched
                .audio_drivers
                .switch
                .target
                .as_ref()
                .unwrap()
                .sample_rate,
            24_000
        );
    }

    #[test]
    fn persistence_failure_keeps_new_driver_active_and_enables_save_retry() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::CompleteAudioDriverSwitchPersistence {
                request_id,
                success: false,
                message: "save failed".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let failed = runtime.snapshot();
        assert_eq!(failed.status.sample_rate, 32_000);
        assert_eq!(
            failed.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert!(failed.audio_drivers.switch.persistence_retry_available);
        assert_eq!(
            failed.audio_drivers.active.as_ref().unwrap().sample_rate,
            32_000
        );
    }

    #[test]
    fn cancelling_audio_driver_switch_leaves_runtime_unchanged() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 44_100,
                    buffer_size: 256,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: false,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let cancelled = runtime.snapshot();
        assert_eq!(cancelled.status.sample_rate, 48_000);
        assert_eq!(
            cancelled
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .configured
                .kind(),
            shoop_app_api::AudioDriverKind::Dummy
        );
        assert!(cancelled.audio_drivers.switch.message.contains("cancelled"));
    }

    #[test]
    fn active_io_task_rejects_audio_driver_preflight_without_mutation() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 32_000,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Idle
        );
        assert_eq!(snapshot.status.sample_rate, 48_000);
        assert!(snapshot
            .notifications
            .iter()
            .any(|notification| notification.message.contains("I/O task is active")));
    }

    #[test]
    fn failed_audio_driver_switch_restores_prior_runtime_and_reports_failure() {
        let backend = FakeBackend::default();
        let control = backend.audio_driver_control();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Cpal(shoop_app_api::CpalAudioDriverConfig {
                    sample_rate: 48_000,
                    buffer_size: 128,
                    ..Default::default()
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        control.fail_next_switch("injected target failure");
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let failed = runtime.snapshot();
        assert_eq!(
            failed.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert!(failed
            .audio_drivers
            .switch
            .message
            .contains("injected target failure"));
        assert_eq!(failed.status.sample_rate, 48_000);
        assert_eq!(
            failed
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .configured
                .kind(),
            shoop_app_api::AudioDriverKind::Dummy
        );
    }

    #[test]
    fn remap_failure_rolls_back_the_committed_target_driver() {
        let backend = FakeBackend::default();
        let control = backend.audio_driver_control();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Cpal(shoop_app_api::CpalAudioDriverConfig {
                    sample_rate: 48_000,
                    buffer_size: 128,
                    ..Default::default()
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        control.corrupt_next_replacement_mapping();
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let failed = runtime.snapshot();
        assert_eq!(
            failed.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert!(failed
            .audio_drivers
            .switch
            .message
            .contains("prior driver was restored"));
        assert_eq!(
            failed
                .audio_drivers
                .active
                .as_ref()
                .unwrap()
                .configured
                .kind(),
            shoop_app_api::AudioDriverKind::Dummy
        );
        assert_eq!(failed.tracks.len(), 1);
    }

    #[test]
    fn rollback_failure_publishes_fatal_backend_state_with_both_errors() {
        let backend = FakeBackend::default();
        let control = backend.audio_driver_control();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Cpal(shoop_app_api::CpalAudioDriverConfig {
                    sample_rate: 48_000,
                    buffer_size: 128,
                    ..Default::default()
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        control.corrupt_next_replacement_mapping();
        control.fail_switch_after(1, "injected rollback failure");
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let fatal = runtime.snapshot();
        assert_eq!(
            fatal.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Fatal
        );
        assert!(fatal
            .audio_drivers
            .switch
            .message
            .contains("injected rollback failure"));
        assert!(fatal
            .audio_drivers
            .switch
            .message
            .contains("could not remap switched session"));
    }

    #[test]
    fn changed_commit_time_rate_requires_a_second_confirmation() {
        let backend = FakeBackend::default();
        let control = backend.audio_driver_control();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 44_100,
                    buffer_size: 128,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        control.set_preflight_sample_rate_override(Some(32_000));
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let reconfirm = runtime.snapshot();
        assert_eq!(
            reconfirm.audio_drivers.switch.status,
            AudioDriverSwitchStatus::AwaitingConfirmation
        );
        assert_eq!(
            reconfirm
                .audio_drivers
                .switch
                .target
                .as_ref()
                .unwrap()
                .sample_rate,
            32_000
        );
        assert!(reconfirm
            .audio_drivers
            .switch
            .message
            .contains("Confirm again"));
    }

    #[test]
    fn recording_blocks_confirmed_audio_driver_switch_without_stopping_recording() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        let sync = &runtime.snapshot().tracks[0];
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync.id,
                loop_id: sync.loops[0].id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 44_100,
                    buffer_size: 256,
                }),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let request_id = runtime.snapshot().audio_drivers.switch.request_id;
        runtime
            .dispatch(AppIntent::ConfirmAudioDriverSwitch {
                request_id,
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let failed = runtime.snapshot();
        assert_eq!(
            failed.audio_drivers.switch.status,
            AudioDriverSwitchStatus::Failed
        );
        assert_eq!(failed.tracks[0].loops[0].mode, LoopMode::Recording);
        assert_eq!(failed.status.sample_rate, 48_000);
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
    fn session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "machine.lua".to_owned(),
                source: Arc::from("shoop_announce_api_version(1, 0); print('machine')"),
                kind: ScriptKind::User,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddEphemeralScript {
                name: "run-once.lua".to_owned(),
                source: Arc::from("shoop_announce_api_version(1, 0); print('run once')"),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let mut document = SessionDocument::empty(48_000);
        document.scripts.push(ScriptDocument {
            id: 77,
            name: "session.lua".to_owned(),
            source: "shoop_announce_api_version(1, 0); require('shoop_control').set_solo(true)"
                .to_owned(),
            enabled: true,
        });
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "scripts.shoop".to_owned(),
                bytes: Arc::from(
                    encode_session(&SessionBundle::new(document.clone()), "test").unwrap(),
                ),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loaded = runtime.snapshot();
        assert_eq!(loaded.scripting.scripts.len(), 3);
        assert!(loaded
            .scripting
            .scripts
            .iter()
            .any(|script| { script.name == "machine.lua" && script.kind == ScriptKind::User }));
        assert!(loaded
            .scripting
            .scripts
            .iter()
            .any(|script| { script.name == "session.lua" && script.kind == ScriptKind::Session }));
        assert!(loaded.scripting.scripts.iter().any(|script| {
            script.name == "run-once.lua" && script.kind == ScriptKind::Ephemeral
        }));
        assert!(loaded.global_controls.solo);

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let output = runtime.take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        assert_eq!(saved.document.scripts, document.scripts);

        let before = runtime.snapshot().scripting.clone();
        let mut cancelled = document.clone();
        cancelled.sample_rate = 32_000;
        cancelled.scripts[0].source =
            "shoop_announce_api_version(1, 0); require('shoop_control').set_solo(false)".to_owned();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "cancelled.shoop".to_owned(),
                bytes: Arc::from(encode_session(&SessionBundle::new(cancelled), "test").unwrap()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let task = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(task.status, IoTaskStatus::AwaitingSampleRateConfirmation);
        runtime
            .dispatch(AppIntent::ConfirmSampleRateConversion {
                task_id: task.id,
                accept: false,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(runtime.snapshot().scripting, before);
        assert!(runtime.snapshot().global_controls.solo);

        let mut invalid = SessionDocument::empty(48_000);
        invalid.scripts.push(ScriptDocument {
            id: 88,
            name: "invalid.lua".to_owned(),
            source: "function(".to_owned(),
            enabled: true,
        });
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "invalid.shoop".to_owned(),
                bytes: Arc::from(encode_session(&SessionBundle::new(invalid), "test").unwrap()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Failed
        );
        assert_eq!(runtime.snapshot().scripting, before);

        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "empty.shoop".to_owned(),
                bytes: Arc::from(
                    encode_session(&SessionBundle::new(SessionDocument::empty(48_000)), "test")
                        .unwrap(),
                ),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let scripts = &runtime.snapshot().scripting.scripts;
        assert_eq!(scripts.len(), 2);
        assert_eq!(scripts[0].name, "machine.lua");
        assert_eq!(scripts[1].name, "run-once.lua");
        assert_eq!(scripts[1].kind, ScriptKind::Ephemeral);
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
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        let loop_id = runtime
            .snapshot()
            .tracks
            .iter()
            .find(|track| track.id == persistent_track)
            .unwrap()
            .loops[0]
            .id;
        runtime
            .dispatch(AppIntent::Loop {
                track_id: persistent_track,
                loop_id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().notifications.iter().any(|notification| {
            notification
                .message
                .contains("active recording/replacement")
        }));
        runtime
            .dispatch(AppIntent::Loop {
                track_id: persistent_track,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
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
                task_id: TaskId::from_raw(warning.id.raw() + 1),
                accept: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::AwaitingSampleRateConfirmation
        );
        runtime
            .dispatch(AppIntent::ConfirmSampleRateConversion {
                task_id: warning.id,
                accept: false,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Cancelled
        );
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "different-rate.shoop".to_owned(),
                bytes: Arc::from(encode_session(&resampled, "test").unwrap()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let warning = runtime.snapshot().io_task.clone().unwrap();
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
                bytes: Arc::from(&b"unsupported predecessor archive"[..]),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(runtime.snapshot().tracks.len(), before);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Failed
        );
        assert!(runtime
            .snapshot()
            .notifications
            .iter()
            .any(|notification| notification.message.contains("unsupported file format")));
    }

    #[test]
    fn unsupported_deferred_topology_is_rejected_without_replacing_the_session() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Retained".to_owned(),
                audio_channels: 2,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let before = runtime.snapshot();
        let before_ids = before
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>();

        let mut document = SessionDocument::empty(48_000);
        document.track_groups.push(TrackGroupDocument {
            name: "main".to_owned(),
            tracks: vec![TrackDocument {
                id: 42,
                name: "Deferred trigger".to_owned(),
                port_name_base: "deferred_trigger".to_owned(),
                is_sync: false,
                width: None,
                topology: TrackTopologyDocument::Trigger,
                controls: TrackControlsDocument::default(),
                loops: Vec::new(),
                ports: Vec::new(),
                fx_chain: None,
            }],
        });
        let bytes = encode_session(&SessionBundle::new(document), "capability-test").unwrap();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "deferred.shoop".to_owned(),
                bytes: Arc::from(bytes),
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
            if runtime
                .snapshot()
                .io_task
                .as_ref()
                .is_some_and(|task| task.status == IoTaskStatus::Failed)
            {
                break;
            }
        }

        let after = runtime.snapshot();
        assert_eq!(
            after
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            before_ids
        );
        assert_eq!(after.io_task.as_ref().unwrap().status, IoTaskStatus::Failed);
        assert!(after.notifications.iter().any(|notification| {
            notification
                .message
                .contains("unsupported trigger topology")
        }));
    }

    #[test]
    fn invalid_loop_media_inputs_finish_tasks_as_failed() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Media errors".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loop_id = runtime.snapshot().tracks[1].loops[0].id;

        for (name, bytes) in [
            ("broken.shoop-audio", Arc::from(&b"not audio"[..])),
            (
                "empty.shoop-audio",
                Arc::from(
                    encode_loop_audio(&LoopAudio {
                        sample_rate: 48_000,
                        channels: Vec::new(),
                    })
                    .unwrap(),
                ),
            ),
        ] {
            runtime
                .dispatch(AppIntent::ImportLoopAudioBytes {
                    loop_id,
                    name: name.to_owned(),
                    bytes,
                    update_loop_length: true,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
            assert_eq!(
                runtime.snapshot().io_task.as_ref().unwrap().status,
                IoTaskStatus::Failed
            );
        }

        runtime
            .dispatch(AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "broken.shoop-midi".to_owned(),
                bytes: Arc::from(&b"not midi"[..]),
                update_loop_length: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Failed
        );
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
        let track_id = runtime.snapshot().tracks[1].id;
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
        for (source_channels, mapping) in [(3_u32, vec![2, 1, 0]), (4, vec![3, 1, 0])] {
            let audio = LoopAudio {
                sample_rate: 48_000,
                channels: (0..source_channels)
                    .map(|index| LoopAudioChannel {
                        label: format!("source {}", index + 1),
                        role: "direct".to_owned(),
                        samples: vec![(index + 1) as f32 / 10.0; 256],
                    })
                    .collect(),
            };
            runtime
                .dispatch(AppIntent::ImportLoopAudioBytes {
                    loop_id,
                    name: "mapping.shoop-audio".to_owned(),
                    bytes: Arc::from(encode_loop_audio(&audio).unwrap()),
                    update_loop_length: true,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
            let task = runtime.snapshot().io_task.clone().unwrap();
            assert_eq!(
                task.audio_channel_mapping
                    .as_ref()
                    .unwrap()
                    .source_channels
                    .len(),
                source_channels as usize
            );
            runtime
                .dispatch(AppIntent::ConfirmAudioChannelMapping {
                    task_id: task.id,
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
        }
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
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
        assert_eq!(exported_audio.channels[0].label, "Direct 2");
        assert_eq!(exported_audio.channels[1].label, "Direct 1");
        assert_ne!(
            exported_audio.channels[0].samples,
            exported_audio.channels[1].samples
        );
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
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
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        runtime
            .dispatch(AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "roundtrip.wav".to_owned(),
                bytes: output.bytes,
                update_loop_length: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let mapping_task = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(mapping_task.status, IoTaskStatus::AwaitingChannelMapping);
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelMapping {
                task_id: mapping_task.id,
                source_for_destination: mapping_task.audio_channel_mapping.unwrap().default_mapping,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );

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
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
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
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        runtime
            .dispatch(AppIntent::RequestLoopMidiExport {
                loop_id,
                standard: true,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let standard_output = runtime.take_file_output().unwrap();
        assert!(standard_output.suggested_name.ends_with(".mid"));
        let standard_midi = decode_standard_midi(&standard_output.bytes, 48_000).unwrap();
        assert!(standard_midi
            .events
            .iter()
            .any(|event| event.frame == 75 && event.data == [0x90, 60, 100]));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        runtime
            .dispatch(AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "roundtrip.mid".to_owned(),
                bytes: standard_output.bytes,
                update_loop_length: true,
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
            .dispatch(AppIntent::RequestLoopMidiExport {
                loop_id,
                standard: false,
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let roundtrip = decode_exact_midi(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert!(roundtrip
            .events
            .iter()
            .any(|event| event.frame == 75 && event.data == [0x90, 60, 100]));
    }

    #[test]
    fn dry_wet_media_io_maps_and_exports_role_order_without_flattening() {
        let mut backend = FakeBackend::default();
        backend.set_track_processor_catalog(vec![shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::EXTERNAL,
            ),
            label: "External".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: None,
                max_wet_audio_channels: None,
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
            },
            features: shoop_app_api::TrackProcessorFeatures::default(),
            editor: None,
        }]);
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "Role media".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::EXTERNAL,
                    ),
                },
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loop_id = runtime.snapshot().tracks[1].loops[0].id;
        let input = LoopAudio {
            sample_rate: 48_000,
            channels: (0..3)
                .map(|index| LoopAudioChannel {
                    label: format!("source {index}"),
                    role: "source".to_owned(),
                    samples: vec![index as f32 + 0.25; 32],
                })
                .collect(),
        };
        runtime
            .dispatch(AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "roles.shoop-audio".to_owned(),
                bytes: Arc::from(encode_loop_audio(&input).unwrap()),
                update_loop_length: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let mapping = runtime.snapshot().io_task.clone().unwrap();
        let destinations = &mapping
            .audio_channel_mapping
            .as_ref()
            .unwrap()
            .destination_channels;
        assert_eq!(destinations, &["Dry 1", "Dry 2", "Wet 1"]);
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelMapping {
                task_id: mapping.id,
                source_for_destination: vec![2, 0, 2],
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        runtime
            .dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::Exact,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let selection = runtime.snapshot().io_task.clone().unwrap();
        assert_eq!(
            selection
                .audio_channel_selection
                .as_ref()
                .unwrap()
                .available_channels,
            vec!["Dry 1", "Dry 2", "Wet 1"]
        );
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: selection.id,
                channels: vec![2, 0],
            })
            .unwrap();
        for _ in 0..10 {
            runtime.tick(Duration::ZERO);
        }
        let exported = decode_loop_audio(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(
            exported
                .channels
                .iter()
                .map(|channel| (channel.label.as_str(), channel.role.as_str()))
                .collect::<Vec<_>>(),
            vec![("Wet 1", "wet"), ("Dry 1", "dry")]
        );
        assert_eq!(exported.channels[0].samples, input.channels[2].samples);
        assert_eq!(exported.channels[1].samples, input.channels[2].samples);

        for (channel, expected_label, expected_role) in [(0, "Dry 1", "dry"), (2, "Wet 1", "wet")] {
            runtime
                .dispatch(AppIntent::RequestLoopAudioExport {
                    loop_id,
                    format: LoopAudioExportFormat::Exact,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
            let task_id = runtime.snapshot().io_task.as_ref().unwrap().id;
            runtime
                .dispatch(AppIntent::ConfirmAudioChannelSelection {
                    task_id,
                    channels: vec![channel],
                })
                .unwrap();
            for _ in 0..10 {
                runtime.tick(Duration::ZERO);
            }
            let exported = decode_loop_audio(&runtime.take_file_output().unwrap().bytes).unwrap();
            assert_eq!(exported.channels.len(), 1);
            assert_eq!(exported.channels[0].label, expected_label);
            assert_eq!(exported.channels[0].role, expected_role);
        }
    }

    #[test]
    fn click_generation_and_preview_preserve_opposite_media_and_stable_identity() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Click target".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let before = runtime.snapshot();
        let track_id = before.tracks[1].id;
        let loop_id = before.tracks[1].loops[0].id;
        let mut audio_request = ClickTrackRequest::default();
        audio_request.bpm = 600.0;
        audio_request.click_count = 2;
        runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id,
                request: audio_request.clone(),
            })
            .unwrap();
        for _ in 0..6 {
            runtime.tick(Duration::ZERO);
        }
        let generated_audio_state = runtime.snapshot();
        assert_eq!(generated_audio_state.tracks[1].id, track_id);
        assert_eq!(generated_audio_state.tracks[1].loops[0].id, loop_id);
        assert_eq!(
            generated_audio_state.tracks[1].loops[0].length_frames,
            9_600
        );
        assert_eq!(
            generated_audio_state.io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );

        runtime
            .dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::Exact,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let task = runtime.snapshot().io_task.clone().unwrap();
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: task.id,
                channels: vec![0, 1],
            })
            .unwrap();
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let audio_before_midi =
            decode_loop_audio(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(audio_before_midi.channels.len(), 2);
        assert_eq!(
            audio_before_midi.channels[0].samples,
            audio_before_midi.channels[1].samples
        );
        assert!(audio_before_midi.channels[0]
            .samples
            .iter()
            .any(|sample| *sample != 0.0));

        let mut midi_request = audio_request.clone();
        midi_request.kind = ClickTrackKind::Midi;
        midi_request.midi_note = 65;
        runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id,
                request: midi_request,
            })
            .unwrap();
        for _ in 0..6 {
            runtime.tick(Duration::ZERO);
        }
        runtime
            .dispatch(AppIntent::RequestLoopMidiExport {
                loop_id,
                standard: false,
            })
            .unwrap();
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let midi = decode_exact_midi(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(midi.length_frames, 9_600);
        assert_eq!(midi.events[0].data, vec![0x90, 65, 127]);

        runtime
            .dispatch(AppIntent::RequestLoopAudioExport {
                loop_id,
                format: LoopAudioExportFormat::Exact,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let task = runtime.snapshot().io_task.clone().unwrap();
        runtime
            .dispatch(AppIntent::ConfirmAudioChannelSelection {
                task_id: task.id,
                channels: vec![0, 1],
            })
            .unwrap();
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let audio_after_midi =
            decode_loop_audio(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(audio_after_midi, audio_before_midi);

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..12 {
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
        let session = runtime.take_file_output().unwrap();
        let decoded = decode_session(&session.bytes).unwrap();
        let saved_loop = decoded.document.track_groups[1].tracks[0]
            .loops
            .iter()
            .find(|saved| saved.id == loop_id.raw())
            .unwrap();
        assert_eq!(saved_loop.length_frames, 9_600);
        assert_eq!(
            saved_loop
                .channels
                .iter()
                .filter(|channel| channel.media_id.is_some())
                .count(),
            3
        );
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: session.suggested_name,
                bytes: session.bytes,
            })
            .unwrap();
        for _ in 0..12 {
            runtime.tick(Duration::ZERO);
            if runtime.snapshot().io_task.as_ref().is_some_and(|task| {
                task.kind == IoTaskKind::LoadSession && task.status == IoTaskStatus::Completed
            }) {
                break;
            }
        }
        assert_eq!(runtime.snapshot().tracks[1].loops[0].id, loop_id);
        assert_eq!(runtime.snapshot().tracks[1].loops[0].length_frames, 9_600);

        let state_before_preview = runtime.snapshot();
        runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: audio_request.clone(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let first_preview = runtime.take_audio_preview().unwrap();
        assert_eq!(first_preview.sample_rate, 48_000);
        assert_eq!(first_preview.samples.len(), 9_600);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].length_frames,
            state_before_preview.tracks[1].loops[0].length_frames
        );
        runtime
            .dispatch(AppIntent::PreviewClickTrack {
                loop_id,
                request: audio_request,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let second_preview = runtime.take_audio_preview().unwrap();
        runtime
            .dispatch(AppIntent::CompleteClickTrackPreview {
                request_id: first_preview.request_id,
                success: false,
                message: "stale failure".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().click_track.preview_request_id,
            second_preview.request_id
        );
        assert_eq!(
            runtime.snapshot().click_track.preview_status,
            ClickTrackPreviewStatus::Queued
        );
    }

    #[test]
    fn engine_backed_click_updates_preserve_running_sync_and_follower_alignment() {
        let backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        let sync_track = runtime.snapshot().tracks[0].id;
        let sync_loop = runtime.snapshot().tracks[0].loops[0].id;
        let request = ClickTrackRequest {
            bpm: 600.0,
            click_count: 2,
            ..Default::default()
        };
        runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id: sync_loop,
                request: request.clone(),
            })
            .unwrap();
        for _ in 0..8 {
            runtime.tick(Duration::ZERO);
        }
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync_track,
                loop_id: sync_loop,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(5));
        assert_eq!(
            runtime.snapshot().tracks[0].loops[0].mode,
            LoopMode::Playing
        );

        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Mixed click target".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track_id = runtime.snapshot().tracks[1].id;
        let loop_id = runtime.snapshot().tracks[1].loops[0].id;
        let mut midi_request = request;
        midi_request.kind = ClickTrackKind::Midi;
        runtime
            .dispatch(AppIntent::GenerateClickTrack {
                loop_id,
                request: midi_request,
            })
            .unwrap();
        for _ in 0..8 {
            runtime.tick(Duration::ZERO);
        }
        let snapshot = runtime.snapshot();
        assert_eq!(
            snapshot.io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );
        assert_eq!(snapshot.tracks[0].id, sync_track);
        assert_eq!(snapshot.tracks[0].loops[0].id, sync_loop);
        assert_eq!(snapshot.tracks[0].loops[0].mode, LoopMode::Playing);
        assert_eq!(snapshot.tracks[1].id, track_id);
        assert_eq!(snapshot.tracks[1].loops[0].id, loop_id);
        assert_eq!(snapshot.tracks[1].loops[0].mode, LoopMode::Stopped);
        assert!(snapshot.tracks[1].loops[0].has_audio);
        assert!(snapshot.tracks[1].loops[0].has_midi);

        runtime
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let follower = &runtime.snapshot().tracks[1].loops[0];
        assert_eq!(follower.mode, LoopMode::Stopped);
        assert_eq!(follower.next_mode, LoopMode::Playing);
    }

    #[test]
    fn callback_overruns_are_accumulated_as_resettable_xruns() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();

        let mut snapshot = BackendSnapshot::default();
        snapshot.status.xruns = 2;
        snapshot.status.callback_budget_overruns = 3;
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.xruns, 5);

        snapshot.status.xruns = 0;
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.xruns, 5);

        snapshot.status.callback_budget_overruns = 5;
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.xruns, 7);

        model.handle_intent(&mut backend, AppIntent::ResetXruns);
        assert_eq!(model.status.xruns, 0);
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.xruns, 0);

        snapshot.status.callback_budget_overruns = 1;
        model.apply_backend_snapshot(snapshot);
        assert_eq!(model.status.xruns, 1);
    }

    #[test]
    fn render_memory_growth_is_a_recoverable_warning() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();

        let mut snapshot = BackendSnapshot::default();
        snapshot.status.render_memory_growths = 1;
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.render_memory_growths, 1);
        assert_eq!(model.notifications.len(), 1);
        assert_eq!(model.notifications[0].level, NotificationLevel::Warning);
        assert!(model.notifications[0].message.contains("Audio recovered"));

        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.notifications.len(), 1);

        snapshot.status.render_memory_growths = 3;
        model.apply_backend_snapshot(snapshot);
        assert_eq!(model.status.render_memory_growths, 3);
        assert_eq!(model.notifications.len(), 2);
        assert!(model.notifications[1].message.contains('2'));
    }

    #[test]
    fn generated_click_update_resets_target_offsets_and_preserves_other_media() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Mixed".to_owned(),
                    audio_channels: 2,
                    midi: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let loop_id = model.tracks[1].loops[0];
        let backend_loop_id = model.loops[&loop_id].backend_id;
        backend
            .replace_loop_content(
                backend_loop_id,
                &BackendLoopContentUpdate {
                    audio: (0..2)
                        .map(|channel| BackendAudioChannelUpdate {
                            channel,
                            samples: vec![channel as f32 + 0.25; 128],
                            start_offset: Some(11),
                            preplay: Some(12),
                        })
                        .collect(),
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 128,
                        start_state: vec![vec![0xB0, 7, 99]],
                        events: vec![BackendMidiEvent {
                            time: 64,
                            data: vec![0x90, 70, 88],
                        }],
                        start_offset: Some(13),
                        preplay: Some(14),
                    }],
                    length: Some(128),
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());

        let mut request = ClickTrackRequest::default();
        request.bpm = 600.0;
        request.click_count = 2;
        model
            .begin_generate_click_track(loop_id, request.clone())
            .unwrap();
        model.advance_io(&mut backend);
        model.advance_io(&mut backend);
        let audio_result = backend.capture_session().unwrap();
        let target = audio_result
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == model.loops[&loop_id].backend_id.raw())
            .unwrap();
        assert_eq!(target.length, 9_600);
        assert_eq!(target.audio.len(), 2);
        assert_eq!(target.audio[0].samples, target.audio[1].samples);
        assert!(target
            .audio
            .iter()
            .all(|channel| channel.start_offset == 0 && channel.preplay == 0));
        assert_eq!(target.midi[0].events[0].data, vec![0x90, 70, 88]);
        assert_eq!(target.midi[0].start_offset, 13);
        let preserved_audio = target.audio.clone();

        request.kind = ClickTrackKind::Midi;
        model.begin_generate_click_track(loop_id, request).unwrap();
        model.advance_io(&mut backend);
        model.advance_io(&mut backend);
        let midi_result = backend.capture_session().unwrap();
        let target = midi_result
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == model.loops[&loop_id].backend_id.raw())
            .unwrap();
        assert_eq!(target.audio, preserved_audio);
        assert_eq!(target.midi[0].events[0].data, vec![0x90, 64, 127]);
        assert_eq!(target.midi[0].start_offset, 0);
        assert_eq!(target.midi[0].preplay, 0);

        let sync_id = model.tracks[0].loops[0];
        model
            .begin_generate_click_track(sync_id, ClickTrackRequest::default())
            .unwrap();
        model.advance_io(&mut backend);
        model.advance_io(&mut backend);
        assert_eq!(model.loops[&sync_id].length, 115_200);
    }

    #[test]
    fn click_generation_rejects_conflicts_and_failed_update_keeps_content() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Failure target".to_owned(),
                    audio_channels: 1,
                    midi: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let loop_id = model.tracks[1].loops[0];
        let before = backend.capture_session().unwrap();
        model
            .begin_generate_click_track(loop_id, ClickTrackRequest::default())
            .unwrap();
        model.advance_io(&mut backend);
        backend.fail_next_loop_content_replace("injected click replacement failure");
        model.advance_io(&mut backend);
        assert_eq!(model.io_task.as_ref().unwrap().status, IoTaskStatus::Failed);
        assert_eq!(backend.capture_session().unwrap(), before);
        assert_eq!(model.loops[&loop_id].length, 0);

        model.loops.get_mut(&loop_id).unwrap().state.mode = LoopMode::Recording;
        assert!(model
            .begin_generate_click_track(loop_id, ClickTrackRequest::default())
            .unwrap_err()
            .contains("recording"));
        model.loops.get_mut(&loop_id).unwrap().state.mode = LoopMode::Stopped;
        model.pending_io = Some(PendingIo::SaveSession);
        assert!(model
            .begin_generate_click_track(loop_id, ClickTrackRequest::default())
            .unwrap_err()
            .contains("another I/O task"));
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
                host_port_id: HostPortId::new("device:port"),
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
