#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

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
    AppIntent, AppSnapshot, ApplicationPortOwner, ApplicationPortState, AudioChannelMappingState,
    AudioChannelSelectionState, AudioDriverConfig, AudioDriverRuntimeState, AudioDriverState,
    AudioDriverSwitchState, AudioDriverSwitchStatus, ChannelId, ClickSoundDescriptor,
    ClickTrackKind, ClickTrackPreviewStatus, ClickTrackRequest, ClickTrackState,
    CompositeDetailsState, CompositeEventDetailsState, CompositeEventId,
    CompositeTrackDetailsState, ConfirmedConnectionState, ConnectionErrorKind,
    ConnectionErrorState, ConnectionPolicy, ConnectionViewState, CueOutputSelection,
    DefaultRecordingAction, DirectTrackSpec, GlobalControlAction, HostPortId, HostPortState,
    IoTaskKind, IoTaskState, IoTaskStatus, KeyEvent, KeyEventType, LatencyRangeSelectionState,
    LoopAction, LoopAudioExportFormat, LoopDetailsState, LoopId, LoopMidiExportFormat, LoopMode,
    LoopState, MidiEventState, MidiSequenceChannelState, PendingConnectionState, PianoAction,
    PortDataType, PortDirection, PortId, PortRole, SampleRateWarning, ScriptDialogButtonId,
    ScriptDialogId, ScriptId, ScriptKind, ScriptMidiRuleDirection, ScriptingState, StatusState,
    StructuralState, TakeLatencyProvenanceState, TaskId, TrackAction, TrackControlState, TrackId,
    TrackPortOwnerKind, TrackProcessorDescriptor, TrackSpec, TrackSpecTopology, TrackState,
    TrackTopology, WaveformChannelState,
};
use shoop_backend::{
    Backend, BackendAsyncResult, BackendAudioChannelUpdate, BackendAudioContent, BackendAudioData,
    BackendChannelMode, BackendCompositeConfig, BackendCompositeEntry, BackendCompositeId,
    BackendCompositeKind, BackendCompositeTarget, BackendConnectionSnapshot, BackendGrabRequest,
    BackendLatencyCertainty, BackendLoopContent, BackendLoopContentUpdate, BackendLoopId,
    BackendLoopMode, BackendMidiChannelUpdate, BackendMidiContent, BackendMidiData,
    BackendMidiEvent, BackendMutationDetail, BackendOperationProgress,
    BackendOxiSynthMidiCcAssignment, BackendOxiSynthParameter, BackendPortDataType,
    BackendPortDescriptor, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendSessionData, BackendSessionPort, BackendSessionReplacement, BackendSessionTrack,
    BackendSnapshot, BackendTakeLatencySnapshot, BackendTrackControl, BackendTrackFxControl,
    BackendTrackId, BackendTrackState, BackendTrackTopology, DirectTrackRequest, TrackRequest,
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
    CompositeKindDocument, CompositeLoopInstanceDocument, ConnectabilityDocument,
    CueOutputSelectionDocument, DataTypeDocument, ExactMidi, ExactMidiEvent, FxChainDocument,
    FxChainTypeDocument, FxStateDocument, GlobalControlsDocument, LatencyCertaintyDocument,
    LatencyComponentDocument, LatencyComponentPolicyDocument, LatencyObservationDocument,
    LatencyRangeSelectionDocument, LatencyValueDocument, LoopAudio, LoopAudioChannel, LoopDocument,
    MediaPayload, MidiClickTrackSpec, MidiControlDocument, OxiSynthMidiCcAssignmentDocument,
    OxiSynthParameterDocument, PortDirectionDocument, PortDocument, PortRoleDocument,
    RecordingActionDocument, ScriptDocument, SessionBundle, SessionDocument, TakeLatencyDocument,
    TrackControlsDocument, TrackDocument, TrackGroupDocument, TrackLatencyPolicyDocument,
    TrackTopologyDocument, MAX_CLICK_TRACK_CLICKS, MAX_CLICK_TRACK_FRAMES,
};

const COMMAND_CAPACITY: usize = 1024;
const MAX_COOPERATIVE_COMMANDS_PER_TICK: usize = 64;
const POLL_INTERVAL: Duration = Duration::from_millis(16);
#[cfg(test)]
const TEST_KEYBOARD_SCRIPT: &str = unsafe {
    std::str::from_utf8_unchecked(include_bytes!(
        "../../../../resources/builtins/keyboard.lua"
    ))
};
#[cfg(test)]
const TEST_APC_MINI_SCRIPT: &str = unsafe {
    std::str::from_utf8_unchecked(include_bytes!(
        "../../../../resources/builtins/akai_apc_mini_mk1.lua"
    ))
};

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
    pub identity: Option<String>,
    pub source: String,
    pub source_path: Option<String>,
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
        Ok(snapshot) => {
            model.clear_periodic_failure("backend.poll");
            model.apply_backend_snapshot(snapshot);
        }
        Err(error) => {
            model.connection_backend_available = false;
            let message = format!("backend poll failed: {error}");
            if model.report_periodic_failure("backend.poll", message.clone()) {
                model.push_connection_error(ConnectionErrorState {
                    port_id: None,
                    external_port: None,
                    kind: ConnectionErrorKind::BackendUnavailable,
                    message,
                });
            }
        }
    }
    let composite_result = model.refresh_backend_composite_configs(backend);
    model.report_periodic_result("backend.composite_configs", composite_result);
    let composition_result = model.advance_script_compositions(backend, elapsed);
    model.report_periodic_result("scripting.compositions", composition_result);
    let selected_media_result = model.refresh_selected_media(backend);
    model.report_periodic_result("backend.selected_media", selected_media_result);
    model.advance_io(backend);
    #[cfg(not(target_arch = "wasm32"))]
    model.advance_script_conversions();
    let scripting_result = model.advance_scripting(backend, elapsed);
    model.report_periodic_result("scripting.advance", scripting_result);
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
    desired_track_controls: BTreeMap<(BackendTrackId, TrackControlKey), BackendTrackControl>,
    desired_fx_controls: BTreeMap<(BackendTrackId, FxControlKey), BackendTrackFxControl>,
    desired_loop_controls: BTreeMap<(BackendLoopId, LoopControlKey), f32>,
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
    active_periodic_failures: BTreeSet<&'static str>,
    next_task_id: u64,
    next_audio_switch_id: u64,
    pending_audio_switch: Option<PendingAudioSwitch>,
    io_task: Option<IoTaskState>,
    pending_io: Option<PendingIo>,
    session_encoding: Option<Receiver<Result<Vec<u8>, String>>>,
    #[cfg(not(target_arch = "wasm32"))]
    background_session_encoding: bool,
    #[cfg(not(target_arch = "wasm32"))]
    next_script_conversion_request: u64,
    #[cfg(not(target_arch = "wasm32"))]
    pending_script_conversions: BTreeMap<ScriptId, PendingScriptConversion>,
    file_outputs: Arc<Mutex<VecDeque<ApplicationFileOutput>>>,
    preview_outputs: Arc<Mutex<VecDeque<ApplicationAudioPreview>>>,
}

#[cfg(not(target_arch = "wasm32"))]
struct PendingScriptConversion {
    request_id: u64,
    expected_source: String,
    expected_generation: u64,
    receiver: Receiver<Result<Arc<shoop_scripting::ScriptResourceBundle>, String>>,
}

struct TrackModel {
    id: TrackId,
    backend_id: BackendTrackId,
    name: String,
    structural_state: StructuralState,
    port_name_base: String,
    is_sync: bool,
    audio_channels: u32,
    topology: TrackTopology,
    fx: Option<shoop_app_api::TrackFxState>,
    loops: Vec<LoopId>,
    port_ids: Arc<[PortId]>,
    controls: TrackControlState,
    latency_policy: shoop_app_api::TrackLatencyPolicyState,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum TrackControlKey {
    OutputGain,
    OutputBalance,
    OutputMute,
    InputGain,
    InputBalance,
    InputMonitoring,
}

fn track_control_key(control: BackendTrackControl) -> TrackControlKey {
    match control {
        BackendTrackControl::OutputGainDb(_) => TrackControlKey::OutputGain,
        BackendTrackControl::OutputBalance(_) => TrackControlKey::OutputBalance,
        BackendTrackControl::OutputMute(_) => TrackControlKey::OutputMute,
        BackendTrackControl::InputGainDb(_) => TrackControlKey::InputGain,
        BackendTrackControl::InputBalance(_) => TrackControlKey::InputBalance,
        BackendTrackControl::InputMonitoring(_) => TrackControlKey::InputMonitoring,
    }
}

fn track_control_matches(state: &BackendTrackState, control: BackendTrackControl) -> bool {
    match control {
        BackendTrackControl::OutputGainDb(value) => {
            (state.output_gain_db - value).abs() <= f32::EPSILON
        }
        BackendTrackControl::OutputBalance(value) => {
            (state.output_balance - value).abs() <= f32::EPSILON
        }
        BackendTrackControl::OutputMute(value) => state.output_muted == value,
        BackendTrackControl::InputGainDb(value) => {
            (state.input_gain_db - value).abs() <= f32::EPSILON
        }
        BackendTrackControl::InputBalance(value) => {
            (state.input_balance - value).abs() <= f32::EPSILON
        }
        BackendTrackControl::InputMonitoring(value) => state.input_monitoring == value,
    }
}

fn apply_track_control(state: &mut TrackControlState, control: BackendTrackControl) {
    match control {
        BackendTrackControl::OutputGainDb(value) => state.output_gain_db = value,
        BackendTrackControl::OutputBalance(value) => state.output_balance = value,
        BackendTrackControl::OutputMute(value) => state.output_muted = value,
        BackendTrackControl::InputGainDb(value) => state.input_gain_db = value,
        BackendTrackControl::InputBalance(value) => state.input_balance = value,
        BackendTrackControl::InputMonitoring(value) => state.input_monitoring = value,
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum FxControlKey {
    Active,
    Visible,
    OxiPreset,
    OxiReverbSend,
    OxiChorusSend,
    OxiMidiAssignments,
}

fn apply_fx_control(fx: &mut shoop_app_api::TrackFxState, control: &BackendTrackFxControl) {
    match control {
        BackendTrackFxControl::SetActive(value) => fx.active = *value,
        BackendTrackFxControl::SetVisible(value) => fx.visible = *value,
        BackendTrackFxControl::OxiSynth(control) => {
            let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) =
                fx.editor.as_mut()
            else {
                return;
            };
            match control {
                shoop_app_api::OxiSynthControl::SelectPreset(id) => {
                    editor.selected_preset_id.clone_from(id);
                }
                shoop_app_api::OxiSynthControl::SetReverbSend(value) => {
                    editor.reverb_send = *value;
                }
                shoop_app_api::OxiSynthControl::SetChorusSend(value) => {
                    editor.chorus_send = *value;
                }
                shoop_app_api::OxiSynthControl::AssignMidiCc(assignment) => {
                    let mut assignments = editor.midi_cc_assignments.to_vec();
                    assignments.retain(|current| {
                        current.parameter != assignment.parameter
                            && (current.channel, current.controller)
                                != (assignment.channel, assignment.controller)
                    });
                    assignments.push(*assignment);
                    assignments.sort_by_key(|assignment| assignment.parameter);
                    editor.midi_cc_assignments = assignments.into();
                }
                shoop_app_api::OxiSynthControl::RemoveMidiCc(parameter) => {
                    editor.midi_cc_assignments = editor
                        .midi_cc_assignments
                        .iter()
                        .copied()
                        .filter(|assignment| assignment.parameter != *parameter)
                        .collect::<Vec<_>>()
                        .into();
                }
                shoop_app_api::OxiSynthControl::ClearMidiCcAssignments => {
                    editor.midi_cc_assignments = Arc::from([]);
                }
                shoop_app_api::OxiSynthControl::Panic => {}
            }
        }
        BackendTrackFxControl::ToggleOrRecover
        | BackendTrackFxControl::RestoreState(_)
        | BackendTrackFxControl::ClearLogs => {}
    }
}

fn fx_control_matches(
    fx: Option<&shoop_app_api::TrackFxState>,
    control: &BackendTrackFxControl,
) -> bool {
    let Some(fx) = fx else {
        return false;
    };
    let mut effective = fx.clone();
    apply_fx_control(&mut effective, control);
    effective == *fx
}

fn fx_control_key(control: &BackendTrackFxControl) -> Option<FxControlKey> {
    Some(match control {
        BackendTrackFxControl::SetActive(_) => FxControlKey::Active,
        BackendTrackFxControl::SetVisible(_) => FxControlKey::Visible,
        BackendTrackFxControl::OxiSynth(control) => match control {
            shoop_app_api::OxiSynthControl::SelectPreset(_) => FxControlKey::OxiPreset,
            shoop_app_api::OxiSynthControl::SetReverbSend(_) => FxControlKey::OxiReverbSend,
            shoop_app_api::OxiSynthControl::SetChorusSend(_) => FxControlKey::OxiChorusSend,
            shoop_app_api::OxiSynthControl::AssignMidiCc(_)
            | shoop_app_api::OxiSynthControl::RemoveMidiCc(_)
            | shoop_app_api::OxiSynthControl::ClearMidiCcAssignments => {
                FxControlKey::OxiMidiAssignments
            }
            shoop_app_api::OxiSynthControl::Panic => return None,
        },
        BackendTrackFxControl::ToggleOrRecover
        | BackendTrackFxControl::RestoreState(_)
        | BackendTrackFxControl::ClearLogs => return None,
    })
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum LoopControlKey {
    Gain,
    Balance,
}

struct ConnectionPortModel {
    id: PortId,
    backend_id: BackendPortId,
    owner: ApplicationPortOwner,
    name: String,
    data_type: PortDataType,
    direction: PortDirection,
    role: PortRole,
    capture_latency: shoop_app_api::LatencyObservationState,
    playback_latency: shoop_app_api::LatencyObservationState,
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

#[derive(Clone)]
struct LoopModel {
    id: LoopId,
    backend_id: BackendLoopId,
    track_id: TrackId,
    name: String,
    state: LoopState,
    length: u32,
    position: u32,
    audio_data: Option<BackendAudioData>,
    midi_data: Option<Vec<MidiSequenceChannelState>>,
    script_composition: Vec<Vec<LoopId>>,
    composite: Option<CompositeDocument>,
    backend_composite: Option<BackendCompositeId>,
    backend_composite_signature: Vec<(LoopId, u32)>,
    repeat_sync: bool,
    recorded_fx_state: Option<RecordedFxState>,
}

fn composite_with_appended_sources(
    existing: Option<&CompositeDocument>,
    add: &[LoopId],
    start_cycle: u64,
    serial_step_cycles: &[u64],
) -> Result<CompositeDocument, String> {
    let mut composite = existing.cloned().unwrap_or(CompositeDocument {
        kind: CompositeKindDocument::Regular,
        instances: Vec::new(),
    });
    composite.kind = CompositeKindDocument::Regular;
    for event in &mut composite.instances {
        event.mode = None;
    }
    let mut next_id = composite
        .instances
        .iter()
        .map(|instance| instance.instance_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "composite instance ID space is exhausted".to_owned())?;
    let mut position = start_cycle;
    for (source, duration) in add.iter().zip(serial_step_cycles) {
        composite.instances.push(CompositeLoopInstanceDocument {
            instance_id: next_id,
            start_cycle: position,
            loop_id: source.raw(),
            mode: None,
            n_cycles: None,
        });
        next_id = next_id
            .checked_add(1)
            .ok_or_else(|| "composite instance ID space is exhausted".to_owned())?;
        position = position
            .checked_add(*duration)
            .ok_or_else(|| "composite schedule position overflows".to_owned())?;
    }
    Ok(composite)
}

fn composite_with_source_at(
    existing: &CompositeDocument,
    source: LoopId,
    start_iteration: u64,
) -> Result<CompositeDocument, String> {
    let mut composite = existing.clone();
    let mode = (composite.kind == CompositeKindDocument::Script).then(|| "playing".to_owned());
    let instance_id = composite
        .instances
        .iter()
        .map(|instance| instance.instance_id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| "composite instance ID space is exhausted".to_owned())?;
    composite.instances.push(CompositeLoopInstanceDocument {
        instance_id,
        start_cycle: start_iteration,
        loop_id: source.raw(),
        mode,
        n_cycles: None,
    });
    Ok(composite)
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
            let latency = app_backend_take_latency(&channel.latency);
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
                preplay_samples: u64::from(channel.preplay),
                loop_length: u64::from(model.length),
                played_sample: matches!(
                    model.state.mode,
                    LoopMode::Playing | LoopMode::PlayingDryThroughWet
                )
                .then_some(i64::from(model.position)),
                latency,
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

enum BackendIoStepError {
    Pending(BackendOperationProgress),
    Failed(String),
}

impl From<String> for BackendIoStepError {
    fn from(value: String) -> Self {
        Self::Failed(value)
    }
}

fn backend_progress_fraction(progress: BackendOperationProgress) -> f32 {
    progress
        .total
        .filter(|total| *total > 0)
        .map(|total| progress.completed as f32 / total as f32)
        .unwrap_or(0.0)
        .clamp(0.0, 1.0)
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
        format: LoopMidiExportFormat,
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
    CaptureLoopDuplicate {
        source: LoopModel,
        target: LoopId,
    },
    CommitLoopDuplicate {
        source: LoopModel,
        target: LoopId,
        update: BackendLoopContentUpdate,
        gain: f32,
        balance: f32,
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
            TrackPortOwnerKind::Sync,
            &created.ports,
            &mut next_port_id,
            &mut connection_ports,
        );
        let global_ports = backend.poll()?.connections.application_ports;
        for descriptor in global_ports
            .values()
            .filter(|port| port.owner == BackendPortOwner::GlobalFxControl)
        {
            let id = PortId::from_raw(next_port_id);
            next_port_id = next_port_id.saturating_add(1);
            connection_ports.insert(
                id,
                ConnectionPortModel {
                    id,
                    backend_id: descriptor.id,
                    owner: ApplicationPortOwner::GlobalFxControl,
                    name: descriptor.name.clone(),
                    data_type: PortDataType::Midi,
                    direction: PortDirection::Input,
                    role: PortRole::MidiInput,
                    capture_latency: Default::default(),
                    playback_latency: Default::default(),
                    candidates: BTreeMap::new(),
                },
            );
        }
        let loop_model = LoopModel {
            id: loop_id,
            backend_id: backend_loop,
            track_id,
            name: "sync loop".to_owned(),
            state: LoopState {
                id: loop_id,
                name: "sync loop".to_owned(),
                structural_state: StructuralState::Creating,
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
            composite: None,
            backend_composite: None,
            backend_composite_signature: Vec::new(),
            repeat_sync: false,
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
                structural_state: StructuralState::Creating,
                port_name_base: "sync_loop".to_owned(),
                is_sync: true,
                audio_channels: 1,
                topology: TrackTopology::Direct,
                fx: None,
                loops: vec![loop_id],
                port_ids,
                controls: Default::default(),
                latency_policy: Default::default(),
            }],
            loops: BTreeMap::from([(loop_id, loop_model)]),
            connection_ports,
            host_ports: BTreeMap::new(),
            confirmed_connections: BTreeSet::new(),
            pending_connections: BTreeMap::new(),
            desired_track_controls: BTreeMap::new(),
            desired_fx_controls: BTreeMap::new(),
            desired_loop_controls: BTreeMap::new(),
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
            active_periodic_failures: BTreeSet::new(),
            next_task_id: 1,
            next_audio_switch_id: 1,
            pending_audio_switch: None,
            io_task: None,
            pending_io: None,
            session_encoding: None,
            #[cfg(not(target_arch = "wasm32"))]
            background_session_encoding,
            #[cfg(not(target_arch = "wasm32"))]
            next_script_conversion_request: 1,
            #[cfg(not(target_arch = "wasm32"))]
            pending_script_conversions: BTreeMap::new(),
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
            let result = if let Some(identity) = script.identity {
                self.script_manager.add_catalog_script(
                    identity,
                    script.source,
                    script.kind,
                    script.enabled,
                    script.source_path,
                    None,
                )
            } else {
                self.script_manager.add_with_source_path(
                    script.name,
                    script.source,
                    script.kind,
                    script.enabled,
                    script.source_path,
                )
            };
            match result {
                Ok(id) => ids.push(Some(id)),
                Err(error) => {
                    ids.push(None);
                    self.report_error(error.to_string());
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
            AppIntent::SetTrackLatencyPolicy {
                track_id,
                mut policy,
            } => {
                let backend_policy = backend_track_latency_policy(&policy, &self.connection_ports);
                let track = self
                    .tracks
                    .iter_mut()
                    .find(|track| track.id == track_id)
                    .ok_or_else(|| format!("stale track {track_id}"));
                match (track, backend_policy) {
                    (Err(error), _) | (_, Err(error)) => Err(error),
                    (Ok(track), Ok(backend_policy)) => {
                        policy.pending = true;
                        policy.error = None;
                        track.latency_policy = policy.clone();
                        match backend.set_track_latency_policy(track.backend_id, &backend_policy) {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                track.latency_policy.pending = false;
                                track.latency_policy.error = Some(error.to_string());
                                Err(format!("could not set track latency policy: {error}"))
                            }
                        }
                    }
                }
            }
            AppIntent::SetTakeLatencyPolicy {
                loop_id,
                capture_alignment_frames,
            } => {
                let model = self
                    .loops
                    .get_mut(&loop_id)
                    .ok_or_else(|| format!("stale loop {loop_id}"));
                match model {
                    Err(error) => Err(error),
                    Ok(model) => {
                        let previous = model.state.latency.capture_alignment_frames;
                        model.state.latency.capture_alignment_frames = capture_alignment_frames;
                        model.state.latency.error = None;
                        match backend
                            .set_take_latency_policy(model.backend_id, capture_alignment_frames)
                        {
                            Ok(()) => Ok(()),
                            Err(error) => {
                                model.state.latency.capture_alignment_frames = previous;
                                model.state.latency.error = Some(error.to_string());
                                Err(format!("could not set take latency policy: {error}"))
                            }
                        }
                    }
                }
            }
            AppIntent::ConsolidateTakeLatency { loop_id } => {
                let model = self
                    .loops
                    .get_mut(&loop_id)
                    .ok_or_else(|| format!("stale loop {loop_id}"));
                match model {
                    Err(error) => Err(error),
                    Ok(model) => match backend.consolidate_take_latency(model.backend_id) {
                        Ok(()) => {
                            model.state.latency.capture_alignment_frames = 0;
                            model.state.latency.variable_history = false;
                            model.state.latency.history_revisions = 0;
                            model.state.latency.error = None;
                            model.audio_data = None;
                            model.midi_data = None;
                            Ok(())
                        }
                        Err(error) => {
                            model.state.latency.error = Some(error.to_string());
                            Err(format!("could not consolidate take latency: {error}"))
                        }
                    },
                }
            }
            AppIntent::SetLoopSmoothingMs(milliseconds) => backend
                .set_loop_smoothing_ms(milliseconds)
                .map_err(|error| error.to_string()),
            AppIntent::SetLoopTimeline {
                loop_id,
                start_offset,
                preplay_samples,
                loop_length,
            } => {
                self.set_loop_timeline(backend, loop_id, start_offset, preplay_samples, loop_length)
            }
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
            AppIntent::AddTrackWithLatencyPolicy { spec, mut policy } => {
                self.add_track_spec(backend, spec).and_then(|()| {
                    let track = self
                        .tracks
                        .last_mut()
                        .ok_or_else(|| "new track was not published".to_owned())?;
                    policy.pending = true;
                    policy.error = None;
                    let backend_policy =
                        backend_track_latency_policy(&policy, &self.connection_ports)?;
                    backend
                        .set_track_latency_policy(track.backend_id, &backend_policy)
                        .map_err(|error| {
                            format!("could not set new track latency policy: {error}")
                        })?;
                    track.latency_policy = policy;
                    Ok(())
                })
            }
            AppIntent::AddLoop { track_id } => self.add_aligned_loop_row(backend, track_id),
            AppIntent::ComposeLoopSerial {
                target_loop_id,
                source_loop_id,
            } => self.compose_loop_serial(backend, target_loop_id, source_loop_id),
            AppIntent::ComposeLoopAt {
                target_loop_id,
                source_loop_id,
                start_iteration,
            } => self.compose_loop_at(backend, target_loop_id, source_loop_id, start_iteration),
            AppIntent::DeleteCompositeEvents {
                target_loop_id,
                events,
            } => self.delete_composite_events(backend, target_loop_id, &events),
            AppIntent::RelocateCompositeEvents {
                target_loop_id,
                events,
                start_iteration,
                duplicate,
            } => self.relocate_composite_events(
                backend,
                target_loop_id,
                &events,
                start_iteration,
                duplicate,
            ),
            AppIntent::SetCompositeLoopCycles {
                target_loop_id,
                event,
                n_cycles,
            } => self.set_composite_loop_cycles(backend, target_loop_id, event, n_cycles),
            AppIntent::SetCompositeKind {
                target_loop_id,
                kind,
            } => self.set_composite_kind(backend, target_loop_id, kind),
            AppIntent::SetCompositeEventMode {
                target_loop_id,
                event,
                mode,
            } => self.set_composite_event_mode(backend, target_loop_id, event, mode),
            AppIntent::KeyEvent(event) => self.handle_script_key_event(backend, event),
            AppIntent::AddScriptSource {
                name,
                source,
                kind,
                enabled,
            } => self.add_script_source(backend, name, source, kind, enabled, None),
            AppIntent::AddScriptFileSource {
                name,
                source,
                source_path,
                kind,
                enabled,
            } => self.add_script_source(backend, name, source, kind, enabled, Some(source_path)),
            AppIntent::AddEphemeralScript {
                name,
                source,
                source_path,
            } => self.add_ephemeral_script(backend, name, source, source_path),
            AppIntent::ReconcileCatalogScripts {
                scripts,
                preserve_identities,
            } => self.reconcile_catalog_scripts(backend, &scripts, &preserve_identities),
            AppIntent::SetScriptEnabled { script_id, enabled } => {
                self.set_script_enabled(backend, script_id, enabled)
            }
            AppIntent::RestartScript { script_id } => self.restart_script(backend, script_id),
            AppIntent::ReplaceScriptSource { script_id, source } => {
                self.replace_script_source(backend, script_id, source)
            }
            AppIntent::StopScript { script_id } => self.stop_script(script_id),
            AppIntent::ForgetScript { script_id } => self.forget_script(script_id),
            AppIntent::ExportScript { script_id } => self.export_script(script_id),
            AppIntent::ConvertScriptKind { script_id, kind } => {
                self.convert_script_kind(script_id, kind)
            }
            AppIntent::RemoveSessionScript { script_id } => self.remove_session_script(script_id),
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
            AppIntent::RequestNewSession => self.begin_new_session(),
            AppIntent::RequestSaveSession => self.begin_save_session(),
            AppIntent::RequestLoadSessionPicker
            | AppIntent::RequestLoadSessionUrl
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
            AppIntent::FailIoTask { task_id, message } => {
                if self.io_task.as_ref().is_some_and(|task| task.id == task_id) {
                    tracing::error!(task_id = task_id.raw(), error = %message, "frontend.app.io_task_failed");
                    self.finish_io(IoTaskStatus::Failed, &message);
                } else {
                    tracing::warn!(task_id = task_id.raw(), error = %message, "frontend.app.stale_io_task_failure");
                }
                Ok(())
            }
            AppIntent::FailIoWorkflow { kind, message } => {
                if self.pending_io.is_some() {
                    tracing::warn!(?kind, error = %message, "frontend.app.io_workflow_failure_ignored_while_busy");
                } else {
                    let task_id = self.start_io_task(kind, &message);
                    tracing::error!(task_id = task_id.raw(), ?kind, error = %message, "frontend.app.io_workflow_failed");
                    self.finish_io(IoTaskStatus::Failed, &message);
                }
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
                manual_offset_frames,
            } => self.import_loop_audio(
                backend,
                loop_id,
                name,
                &bytes,
                update_loop_length,
                manual_offset_frames,
            ),
            AppIntent::RequestLoopMidiExport { loop_id, format } => {
                self.export_loop_midi(backend, loop_id, format)
            }
            AppIntent::ImportLoopMidiBytes {
                loop_id,
                name,
                bytes,
                update_loop_length,
                manual_offset_frames,
            } => self.import_loop_midi(
                backend,
                loop_id,
                name,
                &bytes,
                update_loop_length,
                manual_offset_frames,
            ),
        };
        if let Err(error) = result {
            span.record("outcome", "error");
            tracing::warn!(intent = kind, error = %error, "frontend.app.intent_failed");
        } else {
            span.record("outcome", "ok");
        }
    }

    fn set_loop_timeline(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
        start_offset: Option<i64>,
        preplay_samples: Option<u64>,
        loop_length: Option<u64>,
    ) -> Result<(), String> {
        let backend_id = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?
            .backend_id;
        let start_offset = start_offset
            .map(i32::try_from)
            .transpose()
            .map_err(|_| "loop start is outside the supported frame range".to_owned())?;
        let preplay = preplay_samples
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "preplay is outside the supported frame range".to_owned())?;
        let length = loop_length
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "loop length is outside the supported frame range".to_owned())?;
        backend
            .set_loop_timing(backend_id, start_offset, preplay, length)
            .map_err(|error| format!("could not update loop timing: {error}"))?;

        let model = self.loops.get_mut(&loop_id).expect("loop was checked");
        if let Some(length) = length {
            model.length = length;
        }
        if let Some(channels) = model.audio_data.as_mut() {
            for channel in &mut channels.channels {
                if let Some(offset) = start_offset {
                    channel.start_offset = offset;
                }
                if let Some(samples) = preplay {
                    channel.preplay = samples;
                }
            }
        }
        if let Some(channels) = model.midi_data.as_mut() {
            for channel in channels {
                if let Some(offset) = start_offset {
                    channel.start_offset = i64::from(offset);
                }
                if let Some(samples) = preplay {
                    channel.preplay_samples = u64::from(samples);
                }
                if let Some(length) = length {
                    channel.loop_length = u64::from(length);
                }
            }
        }
        self.revision = self.revision.saturating_add(1);
        Ok(())
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
                    .filter(|track| self.track_accepts_live_midi(track))
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

    fn handle_midi_panic(&mut self, backend: &mut dyn Backend) -> Result<(), String> {
        self.active_piano_notes.clear();
        let destinations = self
            .tracks
            .iter()
            .filter(|track| self.track_accepts_live_midi(track))
            .map(|track| (track.id, track.backend_id))
            .collect::<Vec<_>>();
        let events = (0..16)
            .map(|channel| BackendMidiEvent {
                time: 0,
                data: vec![0xb0 | channel, 120, 0],
            })
            .collect::<Vec<_>>();
        let mut failures = Vec::new();
        for (track_id, backend_id) in destinations {
            if let Err(error) = backend.inject_midi_input(backend_id, &events) {
                failures.push(format!("{track_id}: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "could not send MIDI panic to {} track(s): {}",
                failures.len(),
                failures.join("; ")
            ))
        }
    }

    fn track_accepts_live_midi(&self, track: &TrackModel) -> bool {
        track.controls.input_monitoring
            && track.port_ids.iter().any(|port_id| {
                self.connection_ports.get(port_id).is_some_and(|port| {
                    matches!(
                        port.owner,
                        ApplicationPortOwner::Track { track_id, .. } if track_id == track.id
                    ) && port.data_type == PortDataType::Midi
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
        source_path: Option<String>,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .add_with_source_path(name, source.to_string(), kind, enabled, source_path)
            .map(|_| ())
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn reconcile_catalog_scripts(
        &mut self,
        backend: &mut dyn Backend,
        scripts: &[shoop_app_api::CatalogScriptSource],
        preserve_identities: &[String],
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .reconcile_catalog_scripts(scripts, preserve_identities)
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
        source_path: Option<String>,
    ) -> Result<(), String> {
        self.prepare_script_invocation();
        let result = self
            .script_manager
            .add_ephemeral_with_source_path(name, source.to_string(), source_path)
            .map(|_| ())
            .map_err(|error| error.to_string())
            .and_then(|()| self.apply_script_operations(backend));
        self.refresh_scripting_view();
        result
    }

    fn export_script(&mut self, id: ScriptId) -> Result<(), String> {
        let (name, source) = self
            .script_manager
            .source(id)
            .map_err(|error| error.to_string())?;
        let task_id = TaskId::from_raw(self.next_task_id);
        self.next_task_id = self.next_task_id.saturating_add(1);
        self.file_outputs
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push_back(ApplicationFileOutput {
                task_id,
                suggested_name: name.to_owned(),
                mime_type: "text/x-lua".to_owned(),
                bytes: Arc::from(source.as_bytes()),
            });
        Ok(())
    }

    fn convert_script_kind(&mut self, id: ScriptId, kind: ScriptKind) -> Result<(), String> {
        if kind == ScriptKind::Session {
            let (source, source_path, bundle, generation) = self
                .script_manager
                .conversion_source(id)
                .map_err(|error| error.to_string())?;
            if let Some(bundle) = bundle {
                let result = self
                    .script_manager
                    .commit_session_bundle(id, &source, generation, bundle)
                    .map_err(|error| error.to_string());
                self.refresh_scripting_view();
                return result;
            }
            #[cfg(target_arch = "wasm32")]
            let _ = source_path;
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(source_path) = source_path {
                let request_id = self.next_script_conversion_request;
                self.next_script_conversion_request =
                    self.next_script_conversion_request.saturating_add(1);
                let expected_source = source.clone();
                let source_bytes = Arc::<[u8]>::from(source.as_bytes());
                let (sender, receiver) = mpsc::channel();
                thread::Builder::new()
                    .name("shoop-script-bundle-scan".to_owned())
                    .spawn(move || {
                        let result = shoop_script_resources::capture_filesystem_bundle(
                            std::path::Path::new(&source_path),
                            source_bytes,
                            shoop_script_resources::ResourceLimits::default(),
                        )
                        .map(Arc::new)
                        .map_err(|error| error.to_string());
                        let _ = sender.send(result);
                    })
                    .map_err(|error| format!("could not start script resource scan: {error}"))?;
                self.pending_script_conversions.insert(
                    id,
                    PendingScriptConversion {
                        request_id,
                        expected_source,
                        expected_generation: generation,
                        receiver,
                    },
                );
                return Ok(());
            }
        }
        let result = self
            .script_manager
            .convert_kind(id, kind)
            .map_err(|error| error.to_string());
        self.refresh_scripting_view();
        result
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn advance_script_conversions(&mut self) {
        let completed = self
            .pending_script_conversions
            .iter()
            .filter_map(|(script_id, pending)| match pending.receiver.try_recv() {
                Ok(result) => Some((*script_id, pending.request_id, result)),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => Some((
                    *script_id,
                    pending.request_id,
                    Err("script resource scan worker stopped unexpectedly".to_owned()),
                )),
            })
            .collect::<Vec<_>>();
        for (script_id, request_id, result) in completed {
            let Some(pending) = self.pending_script_conversions.remove(&script_id) else {
                continue;
            };
            if pending.request_id != request_id {
                continue;
            }
            match result {
                Ok(bundle) => {
                    if let Err(error) = self.script_manager.commit_session_bundle(
                        script_id,
                        &pending.expected_source,
                        pending.expected_generation,
                        bundle,
                    ) {
                        self.report_error(format!("could not include script in session: {error}"));
                    }
                }
                Err(error) => {
                    self.report_error(format!("could not include script in session: {error}"))
                }
            }
            self.refresh_scripting_view();
        }
    }

    fn remove_session_script(&mut self, id: ScriptId) -> Result<(), String> {
        let result = self
            .script_manager
            .remove_session_script(id)
            .map_err(|error| error.to_string());
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
                input_balance: track.controls.input_balance,
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
                    let model = self
                        .loops
                        .get(&id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?;
                    if let Some(composite_id) = model.backend_composite {
                        backend
                            .transition_composite_loop(
                                composite_id,
                                backend_loop_mode(mode),
                                cycles_delay,
                                align_to_sync_at.map(i64::from),
                            )
                            .map_err(|error| format!("could not transition loop {id}: {error}"))?;
                    } else {
                        backend
                            .transition_loop_aligned(
                                model.backend_id,
                                backend_loop_mode(mode),
                                cycles_delay,
                                align_to_sync_at,
                            )
                            .map_err(|error| format!("could not transition loop {id}: {error}"))?;
                    }
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
                    if let Some(composite_id) = model.backend_composite {
                        backend
                            .remove_composite_loop(composite_id)
                            .map_err(|error| format!("could not remove composite {id}: {error}"))?;
                    }
                    backend
                        .clear_loop(model.backend_id)
                        .map_err(|error| format!("could not clear loop {id}: {error}"))?;
                    model.length = 0;
                    model.state.empty = true;
                    model.state.composite_kind = shoop_app_api::CompositeKind::None;
                    model.audio_data = None;
                    model.midi_data = None;
                    model.script_composition.clear();
                    model.composite = None;
                    model.backend_composite = None;
                    model.backend_composite_signature.clear();
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
                    .get(&target)
                    .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
                for source in &add {
                    if *source == target
                        || self.composite_references(*source, target, &mut BTreeSet::new())
                    {
                        return Err(format!(
                            "adding loop {source} to composite {target} would create a cycle"
                        ));
                    }
                }
                let mut sections = target_loop.script_composition.clone();
                if parallel && !sections.is_empty() {
                    sections.last_mut().unwrap().extend(add.iter().copied());
                } else if parallel {
                    sections.push(add.clone());
                } else {
                    sections.extend(add.iter().copied().map(|source| vec![source]));
                }
                let sync_length = u64::from(self.sync_length()).max(1);
                let details = target_loop
                    .composite
                    .as_ref()
                    .map(|composite| self.composite_details_snapshot(composite));
                let start_cycle = if parallel {
                    details
                        .as_ref()
                        .and_then(|details| {
                            details.events.iter().map(|event| event.start_frame).max()
                        })
                        .unwrap_or(0)
                        / sync_length
                } else {
                    details
                        .as_ref()
                        .map(|details| details.timeline_length_frames / sync_length)
                        .unwrap_or(0)
                };
                let mut durations = add
                    .iter()
                    .map(|source| {
                        u64::from(self.loops[source].length)
                            .div_ceil(sync_length)
                            .max(1)
                    })
                    .collect::<Vec<_>>();
                if parallel {
                    durations.fill(0);
                }
                let composite = composite_with_appended_sources(
                    target_loop.composite.as_ref(),
                    &add,
                    start_cycle,
                    &durations,
                )?;
                let previous_backend_composite = target_loop.backend_composite;
                let backend_composite = match self.backend_composite_config(&composite)? {
                    Some(config) => match previous_backend_composite {
                        Some(id) => {
                            backend
                                .configure_composite_loop(id, &config)
                                .map_err(|error| {
                                    format!("could not configure composite loop: {error}")
                                })?;
                            Some(id)
                        }
                        None => self.create_and_configure_backend_composite(backend, &composite)?,
                    },
                    None => {
                        if let Some(id) = previous_backend_composite {
                            backend.remove_composite_loop(id).map_err(|error| {
                                format!("could not remove composite loop: {error}")
                            })?;
                        }
                        None
                    }
                };
                let signature = self.composite_length_signature(&composite);
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
                target_loop.script_composition = sections;
                target_loop.length = length;
                target_loop.state.empty = false;
                target_loop.state.composite_kind = shoop_app_api::CompositeKind::Regular;
                target_loop.composite = Some(composite);
                target_loop.backend_composite = backend_composite;
                target_loop.backend_composite_signature = signature;
                Ok(())
            }
            ControlOperation::SetRepeatSync { loops, active } => {
                let sync = active.then(|| self.sync_backend_loop()).flatten();
                for id in &loops {
                    let backend_id = self
                        .loops
                        .get(id)
                        .ok_or_else(|| format!("stale or unknown loop {id}"))?
                        .backend_id;
                    backend
                        .set_loop_sync_source(backend_id, sync)
                        .map_err(|error| {
                            format!("could not update repeat sync for loop {id}: {error}")
                        })?;
                }
                for id in loops {
                    self.loops.get_mut(&id).unwrap().repeat_sync = active;
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
            ControlOperation::SetTrackInputBalance { tracks, balance } => self
                .apply_script_track_action(
                    backend,
                    tracks,
                    TrackAction::InputBalanceChanged(balance),
                ),
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
        let mut updates = Vec::new();
        if enabled && respect_auto_mute && self.global.auto_mute_other_track_inputs {
            updates.extend(
                self.tracks
                    .iter()
                    .filter(|track| !targets.contains(&track.id))
                    .map(|track| (track.id, track.backend_id, false)),
            );
        }
        updates.extend(
            self.tracks
                .iter()
                .filter(|track| targets.contains(&track.id))
                .map(|track| (track.id, track.backend_id, enabled)),
        );
        for (track_id, backend_id, value) in updates {
            let control = BackendTrackControl::InputMonitoring(value);
            backend
                .set_track_control(backend_id, control)
                .map_err(|error| format!("could not update track {track_id}: {error}"))?;
            self.desired_track_controls
                .insert((backend_id, TrackControlKey::InputMonitoring), control);
            if let Some(track) = self.tracks.iter_mut().find(|track| track.id == track_id) {
                apply_track_control(&mut track.controls, control);
            }
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
        let mut backend_composite_targets = Vec::new();
        for id in loops {
            let model = self
                .loops
                .get(id)
                .ok_or_else(|| format!("stale or unknown loop {id}"))?;
            if let Some(composite_id) = model.backend_composite {
                backend
                    .set_composite_play_after_record(composite_id, self.global.play_after_record)
                    .map_err(|error| format!("could not set composite option: {error}"))?;
                backend
                    .transition_composite_loop(composite_id, backend_mode, delay, None)
                    .map_err(|error| format!("could not trigger loop {id}: {error}"))?;
                backend_composite_targets.push(*id);
                continue;
            }
            let composition = model.script_composition.clone();
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
            let mut expanded_ids = expanded.iter().map(|(id, _)| *id).collect::<Vec<_>>();
            expanded_ids.extend(backend_composite_targets);
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
            self.report_error(error);
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
            let rollback_data = session_bundle_to_backend(&source_bundle, &self.track_processors);
            let rollback = source.as_ref().ok_or_else(|| {
                "could not restore switched session because source driver state is missing"
                    .to_owned()
            });
            let rollback = rollback.and_then(|source| {
                let rollback_data = rollback_data?;
                backend
                    .switch_audio_driver(&source.configured, source.sample_rate, &rollback_data)
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

    fn begin_new_session(&mut self) -> Result<(), String> {
        self.ensure_io_idle()?;
        let task_id = self.start_io_task(IoTaskKind::LoadSession, "Creating new session");
        let document = new_session_document(self.status.sample_rate);
        self.begin_session_load(
            "new session".to_owned(),
            SessionBundle::new(document),
            task_id,
        )
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
        self.begin_session_load(name, bundle, task_id)
    }

    fn begin_session_load(
        &mut self,
        name: String,
        bundle: SessionBundle,
        task_id: TaskId,
    ) -> Result<(), String> {
        let session_scripts = match session_script_sources(&bundle) {
            Ok(scripts) => scripts,
            Err(message) => {
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        };
        if let Err(error) = ScriptManager::validate_session_scripts(&session_scripts) {
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
        self.report_error(message);
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
            PendingIo::SaveSession => match backend.capture_session_async() {
                Ok(BackendAsyncResult::Ready(capture)) => {
                    match self.session_bundle_from_backend(&capture) {
                        Ok(bundle) => self.start_session_encoding(bundle),
                        Err(error) => self.fail_io(error),
                    }
                }
                Ok(BackendAsyncResult::Pending(progress)) => {
                    self.pending_io = Some(PendingIo::SaveSession);
                    self.set_io_progress(
                        backend_progress_fraction(progress) * 0.5,
                        "Capturing session",
                    );
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
            } => match backend.replace_session_async(&backend_data) {
                Ok(BackendAsyncResult::Ready(replacement)) => {
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
                Ok(BackendAsyncResult::Pending(progress)) => {
                    self.pending_io = Some(PendingIo::CommitSessionLoad {
                        name,
                        bundle,
                        backend_data,
                    });
                    self.set_io_progress(
                        0.75 + backend_progress_fraction(progress) * 0.2,
                        "Replacing backend session",
                    );
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
                    match error {
                        BackendIoStepError::Pending(progress) => {
                            self.pending_io = Some(PendingIo::ExportLoopAudio {
                                loop_id,
                                format,
                                channels,
                            });
                            self.set_io_progress(
                                backend_progress_fraction(progress) * 0.75,
                                "Capturing loop audio",
                            );
                        }
                        BackendIoStepError::Failed(error) => self.fail_io(error),
                    }
                }
            }
            PendingIo::ExportLoopMidi { loop_id, format } => {
                if let Err(error) = self.export_loop_midi_now(backend, loop_id, format) {
                    match error {
                        BackendIoStepError::Pending(progress) => {
                            self.pending_io = Some(PendingIo::ExportLoopMidi { loop_id, format });
                            self.set_io_progress(
                                backend_progress_fraction(progress) * 0.75,
                                "Capturing loop MIDI",
                            );
                        }
                        BackendIoStepError::Failed(error) => self.fail_io(error),
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
                let result = backend_loop
                    .ok_or_else(|| anyhow!("stale loop {loop_id}"))
                    .and_then(|backend_loop| {
                        backend.replace_loop_content_async(backend_loop, &update)
                    });
                match result {
                    Ok(BackendAsyncResult::Ready(())) => {
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
                    Ok(BackendAsyncResult::Pending(progress)) => {
                        self.pending_io = Some(PendingIo::CommitLoopImport {
                            loop_id,
                            update,
                            message,
                        });
                        self.set_io_progress(
                            0.75 + backend_progress_fraction(progress) * 0.2,
                            "Replacing loop content",
                        );
                    }
                    Err(error) => self.fail_io(format!("could not commit loop import: {error}")),
                }
            }
            PendingIo::CaptureLoopDuplicate { source, target } => {
                match backend.capture_session_async() {
                    Ok(BackendAsyncResult::Ready(capture)) => {
                        let content = capture
                            .tracks
                            .into_iter()
                            .flat_map(|track| track.loops)
                            .find(|content| content.source_id == source.backend_id.raw());
                        match content {
                            Some(content) => {
                                let gain = content.gain;
                                let balance = content.balance;
                                let update = BackendLoopContentUpdate {
                                    audio: content
                                        .audio
                                        .into_iter()
                                        .enumerate()
                                        .map(|(channel, content)| BackendAudioChannelUpdate {
                                            channel,
                                            samples: content.samples,
                                            start_offset: Some(content.start_offset),
                                            preplay: Some(content.preplay),
                                            latency: Some(content.latency),
                                        })
                                        .collect(),
                                    midi: content
                                        .midi
                                        .into_iter()
                                        .enumerate()
                                        .map(|(channel, content)| BackendMidiChannelUpdate {
                                            channel,
                                            length: content.length,
                                            start_state: content.start_state,
                                            events: content.events,
                                            start_offset: Some(content.start_offset),
                                            preplay: Some(content.preplay),
                                            latency: Some(content.latency),
                                        })
                                        .collect(),
                                    length: Some(content.length),
                                };
                                if update.audio.is_empty() && update.midi.is_empty() {
                                    let result = self
                                        .loops
                                        .get(&target)
                                        .ok_or_else(|| format!("stale loop {target}"))
                                        .and_then(|model| {
                                            backend.clear_loop(model.backend_id).map_err(|error| {
                                                format!(
                                                    "could not clear duplicate target {target}: {error}"
                                                )
                                            })
                                        })
                                        .and_then(|()| {
                                            self.finish_primitive_loop_duplicate(
                                                backend, source, target, gain, balance,
                                            )
                                        });
                                    if let Err(error) = result {
                                        self.report_error(error);
                                    }
                                } else {
                                    self.pending_io = Some(PendingIo::CommitLoopDuplicate {
                                        source,
                                        target,
                                        update,
                                        gain,
                                        balance,
                                    });
                                }
                            }
                            None => self.report_error(format!(
                                "backend content for loop {} is unavailable",
                                source.id
                            )),
                        }
                    }
                    Ok(BackendAsyncResult::Pending(_)) => {
                        self.pending_io = Some(PendingIo::CaptureLoopDuplicate { source, target });
                    }
                    Err(error) => {
                        self.report_error(format!("could not capture loop {}: {error}", source.id))
                    }
                }
            }
            PendingIo::CommitLoopDuplicate {
                source,
                target,
                update,
                gain,
                balance,
            } => {
                let target_backend = self.loops.get(&target).map(|model| model.backend_id);
                let result = target_backend
                    .ok_or_else(|| anyhow!("stale loop {target}"))
                    .and_then(|target_backend| {
                        backend.replace_loop_content_async(target_backend, &update)
                    });
                match result {
                    Ok(BackendAsyncResult::Ready(())) => {
                        match self
                            .finish_primitive_loop_duplicate(backend, source, target, gain, balance)
                        {
                            Ok(()) => {}
                            Err(error) => self.report_error(error),
                        }
                    }
                    Ok(BackendAsyncResult::Pending(_)) => {
                        self.pending_io = Some(PendingIo::CommitLoopDuplicate {
                            source,
                            target,
                            update,
                            gain,
                            balance,
                        });
                    }
                    Err(error) => {
                        self.report_error(format!("could not duplicate loop content: {error}"))
                    }
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
        let sync_backend = self.global.sync.then(|| self.sync_backend_loop()).flatten();
        for backend_loop in &created.loops {
            backend
                .set_loop_sync_source(*backend_loop, sync_backend)
                .map_err(|error| format!("could not synchronize new loop: {error}"))?;
        }
        let port_ids = register_backend_ports(
            track_id,
            TrackPortOwnerKind::Main,
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
            structural_state: StructuralState::Creating,
            port_name_base,
            is_sync: false,
            audio_channels: loop_audio_channels,
            topology,
            fx: None,
            loops: loop_ids,
            port_ids,
            controls: Default::default(),
            latency_policy: Default::default(),
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
            if let Some(sync) = self.global.sync.then(|| self.sync_backend_loop()).flatten() {
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

    fn remove_track_model(&mut self, index: usize) {
        let track = self.tracks.remove(index);
        let backend_loop_ids = track
            .loops
            .iter()
            .filter_map(|loop_id| self.loops.get(loop_id).map(|model| model.backend_id))
            .collect::<BTreeSet<_>>();
        for loop_id in &track.loops {
            self.script_composition_playback.remove(loop_id);
            self.loops.remove(loop_id);
        }
        for port_id in track.port_ids.iter() {
            self.connection_ports.remove(port_id);
        }
        self.desired_track_controls
            .retain(|(backend_id, _), _| *backend_id != track.backend_id);
        self.desired_fx_controls
            .retain(|(backend_id, _), _| *backend_id != track.backend_id);
        self.desired_loop_controls
            .retain(|(backend_id, _), _| !backend_loop_ids.contains(backend_id));
    }

    fn remove_track(&mut self, backend: &mut dyn Backend, track_id: TrackId) -> Result<(), String> {
        let index = self
            .tracks
            .iter()
            .position(|track| track.id == track_id && !track.is_sync)
            .ok_or_else(|| format!("stale, unknown, or sync track {track_id}"))?;
        let loop_ids = self.tracks[index]
            .loops
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let referenced = self.loops.values().any(|model| {
            !loop_ids.contains(&model.id)
                && model.composite.as_ref().is_some_and(|composite| {
                    composite
                        .instances
                        .iter()
                        .any(|event| loop_ids.contains(&LoopId::from_raw(event.loop_id)))
                })
        });
        if referenced {
            return Err(format!(
                "cannot remove track {track_id} while its loops are used by a composite"
            ));
        }
        for loop_id in &loop_ids {
            if let Some(composite_id) = self.loops[loop_id].backend_composite {
                backend
                    .remove_composite_loop(composite_id)
                    .map_err(|error| format!("could not remove track composite: {error}"))?;
            }
        }
        let backend_track_id = self.tracks[index].backend_id;
        backend
            .remove_track(backend_track_id)
            .map_err(|error| format!("could not remove track {track_id}: {error}"))?;
        self.tracks[index].structural_state = StructuralState::Removing;
        for loop_id in loop_ids {
            if let Some(model) = self.loops.get_mut(&loop_id) {
                model.state.structural_state = StructuralState::Removing;
            }
        }
        Ok(())
    }

    fn move_track_before(
        &mut self,
        source: TrackId,
        target: Option<TrackId>,
    ) -> Result<(), String> {
        let source_index = self
            .tracks
            .iter()
            .position(|track| track.id == source && !track.is_sync)
            .ok_or_else(|| format!("stale, unknown, or sync track {source}"))?;
        let target_index = target
            .map(|target| {
                self.tracks
                    .iter()
                    .position(|track| track.id == target && !track.is_sync)
                    .ok_or_else(|| format!("stale, unknown, or sync track {target}"))
            })
            .transpose()?;
        if target == Some(source) {
            return Ok(());
        }
        let track = self.tracks.remove(source_index);
        let target_index = target_index
            .map(|index| index - usize::from(source_index < index))
            .unwrap_or(self.tracks.len());
        self.tracks.insert(target_index, track);
        Ok(())
    }

    fn handle_track_action(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        action: TrackAction,
    ) -> Result<(), String> {
        if action == TrackAction::Remove {
            return self.remove_track(backend, track_id);
        }
        if let TrackAction::MoveBefore(target) = &action {
            return self.move_track_before(track_id, *target);
        }
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
        if matches!(&action, TrackAction::OutputBalanceChanged(_)) && !track.controls.output_stereo
        {
            return Ok(());
        }
        let backend_action = match action {
            TrackAction::Remove | TrackAction::MoveBefore(_) => unreachable!(),
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
                let control = BackendTrackFxControl::SetActive(value);
                backend
                    .set_track_fx_control(track.backend_id, control.clone())
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"))?;
                self.desired_fx_controls
                    .insert((track.backend_id, FxControlKey::Active), control.clone());
                if let Some(fx) = track.fx.as_mut() {
                    apply_fx_control(fx, &control);
                }
                return Ok(());
            }
            TrackAction::FxVisibilityChanged(value) => {
                let control = BackendTrackFxControl::SetVisible(value);
                backend
                    .set_track_fx_control(track.backend_id, control.clone())
                    .map_err(|error| format!("could not update track FX {track_id}: {error}"))?;
                self.desired_fx_controls
                    .insert((track.backend_id, FxControlKey::Visible), control.clone());
                if let Some(fx) = track.fx.as_mut() {
                    apply_fx_control(fx, &control);
                }
                return Ok(());
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
            TrackAction::OxiSynth(oxisynth) => {
                let control = BackendTrackFxControl::OxiSynth(oxisynth);
                backend
                    .set_track_fx_control(track.backend_id, control.clone())
                    .map_err(|error| {
                        format!("could not update OxiSynth track {track_id}: {error}")
                    })?;
                if let Some(key) = fx_control_key(&control) {
                    self.desired_fx_controls
                        .insert((track.backend_id, key), control.clone());
                    if let Some(fx) = track.fx.as_mut() {
                        apply_fx_control(fx, &control);
                    }
                }
                return Ok(());
            }
        };
        backend
            .set_track_control(track.backend_id, backend_action)
            .map_err(|error| format!("could not update track {track_id}: {error}"))?;
        self.desired_track_controls.insert(
            (track.backend_id, track_control_key(backend_action)),
            backend_action,
        );
        apply_track_control(&mut track.controls, backend_action);
        track.controls.clamp();
        Ok(())
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
                    structural_state: StructuralState::Creating,
                    show_gain: audio_channels > 0,
                    has_audio: audio_channels > 0,
                    stereo: audio_channels == 2,
                    empty: true,
                    ..Default::default()
                },
                length: 0,
                position: 0,
                audio_data: None,
                midi_data: None,
                script_composition: Vec::new(),
                composite: None,
                backend_composite: None,
                backend_composite_signature: Vec::new(),
                repeat_sync: self.global.sync,
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
        if loop_model.track_id != track_id
            && !matches!(
                action,
                LoopAction::ComposeIntoEnd(_) | LoopAction::ComposeIntoParallel(_)
            )
        {
            return Err(format!(
                "loop {loop_id} does not belong to track {track_id}"
            ));
        }
        match action {
            LoopAction::NameChanged(name) => {
                let model = self.loops.get_mut(&loop_id).expect("loop was checked");
                model.name.clone_from(&name);
                model.state.name = name;
                Ok(())
            }
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
            LoopAction::DefaultClicked => self.default_loop_action(backend, loop_id),
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
                self.desired_loop_controls
                    .insert((loop_model.backend_id, LoopControlKey::Gain), value);
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
                self.desired_loop_controls
                    .insert((loop_model.backend_id, LoopControlKey::Balance), value);
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
            LoopAction::ConvertToComposite => {
                if loop_model.composite.is_some() {
                    return Err(format!("loop {loop_id} is already a composite"));
                }
                let composite = CompositeDocument {
                    kind: CompositeKindDocument::Regular,
                    instances: Vec::new(),
                };
                let signature = self.composite_length_signature(&composite);
                let backend_composite =
                    self.create_and_configure_backend_composite(backend, &composite)?;
                if let Err(error) = backend.clear_loop(loop_model.backend_id) {
                    if let Some(id) = backend_composite {
                        let _ = backend.remove_composite_loop(id);
                    }
                    return Err(format!("could not clear loop {loop_id}: {error}"));
                }
                self.script_composition_playback.remove(&loop_id);
                for candidate in self.loops.values_mut() {
                    candidate.state.selected = false;
                }
                let model = self.loops.get_mut(&loop_id).expect("loop was checked");
                model.length = 0;
                model.position = 0;
                model.audio_data = None;
                model.midi_data = None;
                model.script_composition.clear();
                model.composite = Some(composite);
                model.backend_composite = backend_composite;
                model.backend_composite_signature = signature;
                model.recorded_fx_state = None;
                model.state.empty = true;
                model.state.composite_kind = shoop_app_api::CompositeKind::Regular;
                model.state.has_recorded_fx_state = false;
                model.state.selected = true;
                Ok(())
            }
            LoopAction::Duplicate => self.duplicate_loop_below(backend, track_id, loop_id),
            LoopAction::DuplicateTo(target) => {
                self.duplicate_loop_into(backend, track_id, loop_id, target)
            }
            LoopAction::ComposeIntoEnd(target) => {
                self.compose_loop_into(backend, track_id, loop_id, target, false)
            }
            LoopAction::ComposeIntoParallel(target) => {
                self.compose_loop_into(backend, track_id, loop_id, target, true)
            }
            LoopAction::SwapWith(target) => self.swap_loops(track_id, loop_id, target),
            LoopAction::MoveBefore(target) => self.move_loop_before(track_id, loop_id, target),
        }
    }

    fn duplicate_loop_below(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        source: LoopId,
    ) -> Result<(), String> {
        let track_index = self
            .tracks
            .iter()
            .position(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        let source_index = self.tracks[track_index]
            .loops
            .iter()
            .position(|id| *id == source)
            .ok_or_else(|| format!("loop {source} does not belong to track {track_id}"))?;
        let target = self.tracks[track_index].loops[source_index + 1..]
            .iter()
            .copied()
            .find(|id| {
                self.loops
                    .get(id)
                    .is_some_and(|model| model.state.empty && model.composite.is_none())
            });
        let target = match target {
            Some(target) => target,
            None => {
                self.add_aligned_loop_row(backend, track_id)?;
                *self.tracks[track_index]
                    .loops
                    .last()
                    .expect("adding a row adds a loop to its target track")
            }
        };
        self.duplicate_loop_into(backend, track_id, source, target)
    }

    fn duplicate_loop_into(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        source: LoopId,
        target: LoopId,
    ) -> Result<(), String> {
        if source == target {
            return Err("a loop cannot be duplicated onto itself".to_owned());
        }
        let track = self
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        if !track.loops.contains(&source) || !track.loops.contains(&target) {
            return Err(format!(
                "loops {source} and {target} must belong to track {track_id}"
            ));
        }
        if self.loops[&source].composite.is_some()
            && self.composite_references(source, target, &mut BTreeSet::new())
        {
            return Err(format!(
                "duplicating loop {source} onto {target} would create a cycle"
            ));
        }

        let source_model = self.loops[&source].clone();
        let target_backend = self.loops[&target].backend_id;
        let source_name = self.loops[&source].name.clone();
        let source_state = self.loops[&source].state.clone();
        let source_length = self.loops[&source].length;
        let source_audio_data = self.loops[&source].audio_data.clone();
        let source_midi_data = self.loops[&source].midi_data.clone();
        let source_script_composition = self.loops[&source].script_composition.clone();
        let source_composite = self.loops[&source].composite.clone();
        let source_repeat_sync = self.loops[&source].repeat_sync;
        let source_recorded_fx_state = self.loops[&source].recorded_fx_state.clone();
        let previous_backend_composite = self.loops[&target].backend_composite;

        let backend_composite = if let Some(composite) = &source_composite {
            let created = self.create_and_configure_backend_composite(backend, composite)?;
            if let Err(error) = backend.clear_loop(target_backend) {
                if let Some(id) = created {
                    let _ = backend.remove_composite_loop(id);
                }
                return Err(format!(
                    "could not clear duplicate target {target}: {error}"
                ));
            }
            created
        } else {
            self.ensure_io_idle()?;
            self.pending_io = Some(PendingIo::CaptureLoopDuplicate {
                source: source_model,
                target,
            });
            // Preserve immediate completion for in-process backends while remote
            // backends retain the operation and continue it on later ticks.
            self.advance_io(backend);
            self.advance_io(backend);
            return Ok(());
        };
        if let Some(id) = previous_backend_composite {
            backend
                .remove_composite_loop(id)
                .map_err(|error| format!("could not replace duplicate target: {error}"))?;
        }

        self.script_composition_playback.remove(&target);
        let signature = source_composite
            .as_ref()
            .map(|composite| self.composite_length_signature(composite))
            .unwrap_or_default();
        let model = self.loops.get_mut(&target).expect("target was checked");
        model.name = source_name.clone();
        model.state = source_state;
        model.state.id = target;
        model.state.name = source_name;
        model.state.mode = LoopMode::Stopped;
        model.state.position = 0.0;
        model.state.next_mode = LoopMode::Unknown;
        model.state.next_transition_delay = None;
        model.state.selected = false;
        model.state.targeted = false;
        model.length = source_length;
        model.position = 0;
        model.audio_data = source_audio_data;
        model.midi_data = source_midi_data;
        model.script_composition = source_script_composition;
        model.composite = source_composite;
        model.backend_composite = backend_composite;
        model.backend_composite_signature = signature;
        model.repeat_sync = source_repeat_sync;
        model.recorded_fx_state = source_recorded_fx_state;
        Ok(())
    }

    fn finish_primitive_loop_duplicate(
        &mut self,
        backend: &mut dyn Backend,
        source: LoopModel,
        target: LoopId,
        gain: f32,
        balance: f32,
    ) -> Result<(), String> {
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale loop {target}"))?;
        let target_backend = target_model.backend_id;
        let previous_backend_composite = target_model.backend_composite;
        backend
            .set_loop_gain(target_backend, gain)
            .map_err(|error| format!("could not duplicate loop gain: {error}"))?;
        backend
            .set_loop_balance(target_backend, balance)
            .map_err(|error| format!("could not duplicate loop balance: {error}"))?;
        if let Some(id) = previous_backend_composite {
            backend
                .remove_composite_loop(id)
                .map_err(|error| format!("could not replace duplicate target: {error}"))?;
        }

        self.script_composition_playback.remove(&target);
        let model = self.loops.get_mut(&target).expect("target was checked");
        model.name = source.name.clone();
        model.state = source.state;
        model.state.id = target;
        model.state.name = source.name;
        model.state.mode = LoopMode::Stopped;
        model.state.position = 0.0;
        model.state.next_mode = LoopMode::Unknown;
        model.state.next_transition_delay = None;
        model.state.selected = false;
        model.state.targeted = false;
        model.length = source.length;
        model.position = 0;
        model.audio_data = source.audio_data;
        model.midi_data = source.midi_data;
        model.script_composition = source.script_composition;
        model.composite = None;
        model.backend_composite = None;
        model.backend_composite_signature.clear();
        model.repeat_sync = source.repeat_sync;
        model.recorded_fx_state = source.recorded_fx_state;
        Ok(())
    }

    fn move_loop_before(
        &mut self,
        track_id: TrackId,
        source: LoopId,
        target: Option<LoopId>,
    ) -> Result<(), String> {
        let track = self
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        let source_index = track
            .loops
            .iter()
            .position(|id| *id == source)
            .ok_or_else(|| format!("loop {source} does not belong to track {track_id}"))?;
        let target_index = target
            .map(|target| {
                track
                    .loops
                    .iter()
                    .position(|id| *id == target)
                    .ok_or_else(|| format!("loop {target} does not belong to track {track_id}"))
            })
            .transpose()?;
        if target == Some(source) {
            return Ok(());
        }
        track.loops.remove(source_index);
        let target_index = target_index
            .map(|index| index - usize::from(source_index < index))
            .unwrap_or(track.loops.len());
        track.loops.insert(target_index, source);
        Ok(())
    }

    fn swap_loops(
        &mut self,
        track_id: TrackId,
        source: LoopId,
        target: LoopId,
    ) -> Result<(), String> {
        if source == target {
            return Err("a loop cannot be swapped with itself".to_owned());
        }
        let track = self
            .tracks
            .iter_mut()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        let source_index = track
            .loops
            .iter()
            .position(|id| *id == source)
            .ok_or_else(|| format!("loop {source} does not belong to track {track_id}"))?;
        let target_index = track
            .loops
            .iter()
            .position(|id| *id == target)
            .ok_or_else(|| format!("loop {target} does not belong to track {track_id}"))?;
        track.loops.swap(source_index, target_index);
        Ok(())
    }

    fn composite_length_signature(&self, composite: &CompositeDocument) -> Vec<(LoopId, u32)> {
        let mut ids = composite
            .instances
            .iter()
            .map(|event| LoopId::from_raw(event.loop_id))
            .collect::<BTreeSet<_>>();
        if let Some(sync) = self
            .tracks
            .iter()
            .find(|track| track.is_sync)
            .and_then(|track| track.loops.first())
        {
            ids.insert(*sync);
        }
        ids.into_iter()
            .filter_map(|id| self.loops.get(&id).map(|model| (id, model.length)))
            .collect()
    }

    fn refresh_backend_composite_configs(
        &mut self,
        backend: &mut dyn Backend,
    ) -> Result<(), String> {
        let stale = self
            .loops
            .values()
            .filter_map(|model| {
                let composite_id = model.backend_composite?;
                let composite = model.composite.as_ref()?;
                let recording = matches!(
                    model.state.mode,
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ) || model.state.next_transition_delay == Some(0)
                    && matches!(
                        model.state.next_mode,
                        LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                    );
                let signature = self.composite_length_signature(composite);
                (!recording && signature != model.backend_composite_signature).then_some((
                    model.id,
                    composite_id,
                    composite.clone(),
                    signature,
                ))
            })
            .collect::<Vec<_>>();
        for (id, composite_id, composite, signature) in stale {
            let Some(config) = self.backend_composite_config(&composite)? else {
                continue;
            };
            backend
                .configure_composite_loop(composite_id, &config)
                .map_err(|error| format!("could not refresh composite {id}: {error}"))?;
            self.loops.get_mut(&id).unwrap().backend_composite_signature = signature;
        }
        Ok(())
    }

    fn backend_composite_config(
        &self,
        composite: &CompositeDocument,
    ) -> Result<Option<BackendCompositeConfig>, String> {
        let Some(sync_source) = self.sync_backend_loop() else {
            return Ok(None);
        };
        let entries = composite
            .instances
            .iter()
            .map(|event| {
                let source_id = LoopId::from_raw(event.loop_id);
                let source = self
                    .loops
                    .get(&source_id)
                    .ok_or_else(|| format!("stale composition source {source_id}"))?;
                let target = match source.backend_composite {
                    Some(id) => BackendCompositeTarget::Composite(id),
                    None if source.composite.is_none() => {
                        BackendCompositeTarget::Loop(source.backend_id)
                    }
                    None => return Ok(None),
                };
                let delay = i64::try_from(event.start_cycle)
                    .map_err(|_| "composite delay exceeds engine range".to_owned())?;
                let mode = match event.mode.as_deref() {
                    None => None,
                    Some("stopped") => Some(BackendLoopMode::Stopped),
                    Some("playing") => Some(BackendLoopMode::Playing),
                    Some("recording") => Some(BackendLoopMode::Recording),
                    Some("replacing") => Some(BackendLoopMode::Replacing),
                    Some("playing_dry_through_wet") => Some(BackendLoopMode::PlayingDryThroughWet),
                    Some("recording_dry_into_wet") => Some(BackendLoopMode::RecordingDryIntoWet),
                    Some(mode) => {
                        return Err(format!("unsupported composite mode {mode}"));
                    }
                };
                Ok(Some(BackendCompositeEntry {
                    target,
                    delay,
                    n_cycles: event.n_cycles.map(i64::from),
                    mode,
                }))
            })
            .collect::<Result<Option<Vec<_>>, String>>()?;
        let Some(entries) = entries else {
            return Ok(None);
        };
        let kind = match composite.kind {
            CompositeKindDocument::Regular => BackendCompositeKind::Regular,
            CompositeKindDocument::Script => BackendCompositeKind::Script,
        };
        Ok(Some(BackendCompositeConfig {
            kind,
            sync_source,
            timelines: (!entries.is_empty())
                .then_some(vec![entries])
                .into_iter()
                .collect(),
        }))
    }

    fn create_and_configure_backend_composite(
        &self,
        backend: &mut dyn Backend,
        composite: &CompositeDocument,
    ) -> Result<Option<BackendCompositeId>, String> {
        if !backend.supports_composite_loops() {
            return Ok(None);
        }
        let Some(config) = self.backend_composite_config(composite)? else {
            return Ok(None);
        };
        let id = backend
            .create_composite_loop()
            .map_err(|error| format!("could not create composite loop: {error}"))?;
        if let Err(error) = backend.configure_composite_loop(id, &config) {
            let _ = backend.remove_composite_loop(id);
            return Err(format!("could not configure composite loop: {error}"));
        }
        Ok(Some(id))
    }

    fn restore_backend_composites(&mut self, backend: &mut dyn Backend) -> Result<(), String> {
        let mut pending = self
            .loops
            .values()
            .filter(|model| model.composite.is_some())
            .map(|model| model.id)
            .collect::<BTreeSet<_>>();
        while !pending.is_empty() {
            let ready = pending.iter().copied().find(|id| {
                self.loops[id]
                    .composite
                    .as_ref()
                    .into_iter()
                    .flat_map(|composite| &composite.instances)
                    .map(|event| LoopId::from_raw(event.loop_id))
                    .filter(|child| {
                        self.loops
                            .get(child)
                            .is_some_and(|model| model.composite.is_some())
                    })
                    .all(|child| !pending.contains(&child))
            });
            let id = ready.ok_or_else(|| "composite dependency cycle".to_owned())?;
            let composite = self.loops[&id].composite.clone().unwrap();
            let backend_composite =
                self.create_and_configure_backend_composite(backend, &composite)?;
            let signature = self.composite_length_signature(&composite);
            let model = self.loops.get_mut(&id).unwrap();
            model.backend_composite = backend_composite;
            model.backend_composite_signature = signature;
            pending.remove(&id);
        }
        Ok(())
    }

    fn compose_loop_into(
        &mut self,
        backend: &mut dyn Backend,
        track_id: TrackId,
        source: LoopId,
        target: LoopId,
        parallel: bool,
    ) -> Result<(), String> {
        let track = self
            .tracks
            .iter()
            .find(|track| track.id == track_id)
            .ok_or_else(|| format!("stale or unknown track {track_id}"))?;
        if track.is_sync {
            return Err("the sync loop cannot be a composition target".to_owned());
        }
        if !track.loops.contains(&target) {
            return Err(format!("loop {target} does not belong to track {track_id}"));
        }
        if !self.loops.contains_key(&source) {
            return Err(format!("stale or unknown composition source {source}"));
        }
        if source == target || self.composite_references(source, target, &mut BTreeSet::new()) {
            return Err(format!(
                "adding loop {source} to composite {target} would create a cycle"
            ));
        }
        if self.loops[&target].composite.is_none() {
            self.handle_loop_action(backend, track_id, target, LoopAction::ConvertToComposite)?;
        }
        self.apply_script_operation(
            backend,
            ControlOperation::ComposeAddToEnd {
                target,
                add: vec![source],
                parallel,
            },
        )
    }

    fn compose_loop_serial(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        source: LoopId,
    ) -> Result<(), String> {
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
        if target_model.composite.is_none() {
            return Err(format!("composition target {target} is not a composite"));
        }
        if !self.loops.contains_key(&source) {
            return Err(format!("stale or unknown composition source {source}"));
        }
        if source == target || self.composite_references(source, target, &mut BTreeSet::new()) {
            return Err(format!(
                "adding loop {source} to composite {target} would create a cycle"
            ));
        }
        self.apply_script_operation(
            backend,
            ControlOperation::ComposeAddToEnd {
                target,
                add: vec![source],
                parallel: false,
            },
        )
    }

    fn compose_loop_at(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        source: LoopId,
        start_iteration: u64,
    ) -> Result<(), String> {
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
        let existing = target_model
            .composite
            .as_ref()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        if !self.loops.contains_key(&source) {
            return Err(format!("stale or unknown composition source {source}"));
        }
        if source == target || self.composite_references(source, target, &mut BTreeSet::new()) {
            return Err(format!(
                "adding loop {source} to composite {target} would create a cycle"
            ));
        }
        if !backend.supports_composite_loops() {
            return Err("positioned composite schedules require backend support".to_owned());
        }

        let composite = composite_with_source_at(existing, source, start_iteration)?;
        let previous_backend_composite = target_model.backend_composite;
        let config = self.backend_composite_config(&composite)?.ok_or_else(|| {
            "positioned composite schedule is not backend-configurable".to_owned()
        })?;
        let backend_composite = match previous_backend_composite {
            Some(id) => {
                backend
                    .configure_composite_loop(id, &config)
                    .map_err(|error| format!("could not configure composite loop: {error}"))?;
                id
            }
            None => self
                .create_and_configure_backend_composite(backend, &composite)?
                .ok_or_else(|| "could not create positioned composite schedule".to_owned())?,
        };
        let signature = self.composite_length_signature(&composite);
        let length = self
            .composite_details_snapshot(&composite)
            .timeline_length_frames
            .try_into()
            .unwrap_or(u32::MAX);
        let mut sections = target_model.script_composition.clone();
        sections.push(vec![source]);
        let target_model = self.loops.get_mut(&target).unwrap();
        target_model.script_composition = sections;
        target_model.length = length;
        target_model.state.empty = false;
        target_model.state.composite_kind = match composite.kind {
            CompositeKindDocument::Regular => shoop_app_api::CompositeKind::Regular,
            CompositeKindDocument::Script => shoop_app_api::CompositeKind::Script,
        };
        target_model.composite = Some(composite);
        target_model.backend_composite = Some(backend_composite);
        target_model.backend_composite_signature = signature;
        Ok(())
    }

    fn delete_composite_events(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        events: &[CompositeEventId],
    ) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
        let existing = target_model
            .composite
            .as_ref()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        let selected = events.iter().copied().collect::<BTreeSet<_>>();
        let composite = self.composite_without_events_preserving_positions(existing, &selected)?;
        let previous_backend_composite = target_model.backend_composite;
        let backend_composite = match self.backend_composite_config(&composite)? {
            Some(config) => match previous_backend_composite {
                Some(id) => {
                    backend
                        .configure_composite_loop(id, &config)
                        .map_err(|error| format!("could not configure composite loop: {error}"))?;
                    Some(id)
                }
                None => self.create_and_configure_backend_composite(backend, &composite)?,
            },
            None => {
                if let Some(id) = previous_backend_composite {
                    backend
                        .remove_composite_loop(id)
                        .map_err(|error| format!("could not remove composite loop: {error}"))?;
                }
                None
            }
        };
        let signature = self.composite_length_signature(&composite);
        let length = self
            .composite_details_snapshot(&composite)
            .timeline_length_frames
            .try_into()
            .unwrap_or(u32::MAX);
        let sections = composite
            .instances
            .iter()
            .map(|instance| vec![LoopId::from_raw(instance.loop_id)])
            .collect();
        let empty = composite.instances.is_empty();
        let composite_kind = match composite.kind {
            CompositeKindDocument::Regular => shoop_app_api::CompositeKind::Regular,
            CompositeKindDocument::Script => shoop_app_api::CompositeKind::Script,
        };
        self.script_composition_playback.remove(&target);
        let target_model = self.loops.get_mut(&target).unwrap();
        target_model.script_composition = sections;
        target_model.length = length;
        target_model.state.empty = empty;
        target_model.state.composite_kind = composite_kind;
        target_model.composite = Some(composite);
        target_model.backend_composite = backend_composite;
        target_model.backend_composite_signature = signature;
        Ok(())
    }

    fn relocate_composite_events(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        events: &[CompositeEventId],
        start_iteration: u64,
        duplicate: bool,
    ) -> Result<(), String> {
        if events.is_empty() {
            return Ok(());
        }
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
        let existing = target_model
            .composite
            .as_ref()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        let details = self.composite_details_snapshot(existing);
        let selected = events.iter().copied().collect::<BTreeSet<_>>();
        let selected_details = details
            .events
            .iter()
            .filter(|event| {
                selected.contains(&CompositeEventId {
                    instance_id: event.instance_id,
                })
            })
            .collect::<Vec<_>>();
        if selected_details.len() != selected.len() {
            return Err("composite relocation references a stale event".to_owned());
        }
        let cycle_length = details.cycle_length_frames.max(1);
        let origin = selected_details
            .iter()
            .map(|event| event.start_frame / cycle_length)
            .min()
            .unwrap_or(0);
        let mut composite = if duplicate {
            existing.clone()
        } else {
            self.composite_without_events_preserving_positions(existing, &selected)?
        };
        let mut next_id = composite
            .instances
            .iter()
            .map(|instance| instance.instance_id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "composite instance ID space is exhausted".to_owned())?;
        for event in selected_details {
            let document = existing
                .instances
                .iter()
                .find(|instance| instance.instance_id == event.instance_id)
                .unwrap();
            let relative = event.start_frame / cycle_length - origin;
            let mut document = document.clone();
            document.start_cycle = start_iteration
                .checked_add(relative)
                .ok_or_else(|| "composite relocation position overflow".to_owned())?;
            if duplicate {
                document.instance_id = next_id;
                next_id = next_id
                    .checked_add(1)
                    .ok_or_else(|| "composite instance ID space is exhausted".to_owned())?;
            }
            composite.instances.push(document);
        }

        let config = self
            .backend_composite_config(&composite)?
            .ok_or_else(|| "relocated composite is not backend-configurable".to_owned())?;
        let backend_composite = match target_model.backend_composite {
            Some(id) => {
                backend
                    .configure_composite_loop(id, &config)
                    .map_err(|error| format!("could not configure composite loop: {error}"))?;
                id
            }
            None => self
                .create_and_configure_backend_composite(backend, &composite)?
                .ok_or_else(|| "could not create relocated composite schedule".to_owned())?,
        };
        let signature = self.composite_length_signature(&composite);
        let length = self
            .composite_details_snapshot(&composite)
            .timeline_length_frames
            .try_into()
            .unwrap_or(u32::MAX);
        let target_model = self.loops.get_mut(&target).unwrap();
        target_model.length = length;
        target_model.state.empty = false;
        target_model.composite = Some(composite);
        target_model.backend_composite = Some(backend_composite);
        target_model.backend_composite_signature = signature;
        Ok(())
    }

    fn set_composite_loop_cycles(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        event_id: CompositeEventId,
        n_cycles: Option<u32>,
    ) -> Result<(), String> {
        if n_cycles == Some(0) {
            return Err("forced composite length must be at least one cycle".to_owned());
        }
        let target_model = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?;
        let mut composite = target_model
            .composite
            .clone()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        let event = composite
            .instances
            .iter_mut()
            .find(|event| event.instance_id == event_id.instance_id)
            .ok_or_else(|| "stale or unknown composite event".to_owned())?;
        event.n_cycles = n_cycles;

        let backend_composite = match target_model.backend_composite {
            Some(id) => {
                let config = self
                    .backend_composite_config(&composite)?
                    .ok_or_else(|| "composite is not backend-configurable".to_owned())?;
                backend
                    .configure_composite_loop(id, &config)
                    .map_err(|error| format!("could not configure composite loop: {error}"))?;
                Some(id)
            }
            None => self.create_and_configure_backend_composite(backend, &composite)?,
        };
        let signature = self.composite_length_signature(&composite);
        let length = self
            .composite_details_snapshot(&composite)
            .timeline_length_frames
            .try_into()
            .unwrap_or(u32::MAX);
        let target_model = self.loops.get_mut(&target).unwrap();
        target_model.length = length;
        target_model.composite = Some(composite);
        target_model.backend_composite = backend_composite;
        target_model.backend_composite_signature = signature;
        Ok(())
    }

    fn set_composite_kind(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        kind: shoop_app_api::CompositeKind,
    ) -> Result<(), String> {
        let document_kind = match kind {
            shoop_app_api::CompositeKind::Regular => CompositeKindDocument::Regular,
            shoop_app_api::CompositeKind::Script => CompositeKindDocument::Script,
            shoop_app_api::CompositeKind::None => {
                return Err("a composite cannot be changed to a primitive loop".to_owned());
            }
        };
        let mut composite = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?
            .composite
            .clone()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        composite.kind = document_kind;
        for event in &mut composite.instances {
            event.mode = match document_kind {
                CompositeKindDocument::Regular => None,
                CompositeKindDocument::Script => Some("playing".to_owned()),
            };
        }
        self.commit_composite_editor_change(backend, target, composite)
    }

    fn set_composite_event_mode(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        event_id: CompositeEventId,
        mode: LoopMode,
    ) -> Result<(), String> {
        let mode = match mode {
            LoopMode::Stopped => "stopped",
            LoopMode::Playing => "playing",
            LoopMode::Recording => "recording",
            LoopMode::Replacing => "replacing",
            LoopMode::PlayingDryThroughWet => "playing_dry_through_wet",
            LoopMode::RecordingDryIntoWet => "recording_dry_into_wet",
            LoopMode::Unknown => return Err("unknown is not a script event mode".to_owned()),
        };
        let mut composite = self
            .loops
            .get(&target)
            .ok_or_else(|| format!("stale or unknown composition target {target}"))?
            .composite
            .clone()
            .ok_or_else(|| format!("composition target {target} is not a composite"))?;
        if composite.kind != CompositeKindDocument::Script {
            return Err("event modes can only be edited in script composites".to_owned());
        }
        let event = composite
            .instances
            .iter_mut()
            .find(|event| event.instance_id == event_id.instance_id)
            .ok_or_else(|| "stale or unknown composite event".to_owned())?;
        event.mode = Some(mode.to_owned());
        self.commit_composite_editor_change(backend, target, composite)
    }

    fn commit_composite_editor_change(
        &mut self,
        backend: &mut dyn Backend,
        target: LoopId,
        composite: CompositeDocument,
    ) -> Result<(), String> {
        let target_model = self.loops.get(&target).unwrap();
        let previous_backend_composite = target_model.backend_composite;
        let has_events = !composite.instances.is_empty();
        let config = (has_events && backend.supports_composite_loops())
            .then(|| self.backend_composite_config(&composite))
            .transpose()?
            .flatten();
        let backend_composite = match (previous_backend_composite, config) {
            (Some(id), Some(config)) => {
                backend
                    .configure_composite_loop(id, &config)
                    .map_err(|error| format!("could not configure composite loop: {error}"))?;
                Some(id)
            }
            (None, Some(config)) => {
                let id = backend
                    .create_composite_loop()
                    .map_err(|error| format!("could not create composite loop: {error}"))?;
                if let Err(error) = backend.configure_composite_loop(id, &config) {
                    let _ = backend.remove_composite_loop(id);
                    return Err(format!("could not configure composite loop: {error}"));
                }
                Some(id)
            }
            (Some(id), None) => {
                backend
                    .remove_composite_loop(id)
                    .map_err(|error| format!("could not remove composite loop: {error}"))?;
                None
            }
            (None, None) => None,
        };
        let signature = self.composite_length_signature(&composite);
        let length = self
            .composite_details_snapshot(&composite)
            .timeline_length_frames
            .try_into()
            .unwrap_or(u32::MAX);
        let kind = match composite.kind {
            CompositeKindDocument::Regular => shoop_app_api::CompositeKind::Regular,
            CompositeKindDocument::Script => shoop_app_api::CompositeKind::Script,
        };
        let target_model = self.loops.get_mut(&target).unwrap();
        target_model.length = length;
        target_model.state.composite_kind = kind;
        target_model.composite = Some(composite);
        target_model.backend_composite = backend_composite;
        target_model.backend_composite_signature = signature;
        Ok(())
    }

    fn composite_without_events_preserving_positions(
        &self,
        existing: &CompositeDocument,
        selected: &BTreeSet<CompositeEventId>,
    ) -> Result<CompositeDocument, String> {
        let mut composite = existing.clone();
        let mut found = BTreeSet::new();
        composite.instances.retain(|instance| {
            let id = CompositeEventId {
                instance_id: instance.instance_id,
            };
            if selected.contains(&id) {
                found.insert(id);
                false
            } else {
                true
            }
        });
        if found != *selected {
            return Err("composite deletion references a stale event".to_owned());
        }
        Ok(composite)
    }

    fn composite_references(
        &self,
        composite_id: LoopId,
        searched: LoopId,
        visited: &mut BTreeSet<LoopId>,
    ) -> bool {
        if !visited.insert(composite_id) {
            return false;
        }
        self.loops
            .get(&composite_id)
            .and_then(|model| model.composite.as_ref())
            .is_some_and(|composite| {
                composite.instances.iter().any(|event| {
                    let child = LoopId::from_raw(event.loop_id);
                    child == searched || self.composite_references(child, searched, visited)
                })
            })
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
            if matches!(
                processor_type.as_str(),
                shoop_app_api::TrackProcessorTypeId::EXTERNAL
                    | shoop_app_api::TrackProcessorTypeId::OXISYNTH
            ) {
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
                    .loop_audio_data_with_metadata(*backend_id)
                    .map_err(|error| format!("could not fetch selected loop audio: {error}"))?;
                if let (Some(model), Some(data)) = (self.loops.get_mut(id), data) {
                    model.audio_data = Some(data);
                }
            } else if let Some(model) = self.loops.get_mut(id) {
                model.audio_data = Some(BackendAudioData::default());
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

    fn default_loop_action(
        &mut self,
        backend: &mut dyn Backend,
        loop_id: LoopId,
    ) -> Result<(), String> {
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale or unknown loop {loop_id}"))?;
        let has_planned_transition = model.state.next_transition_delay.is_some()
            && !matches!(model.state.next_mode, LoopMode::Unknown | LoopMode::Stopped);
        let mode = if has_planned_transition {
            BackendLoopMode::Stopped
        } else if model.state.mode == LoopMode::Recording {
            BackendLoopMode::Playing
        } else if model.length == 0 && model.state.mode == LoopMode::Stopped {
            if self.global.default_recording_action == DefaultRecordingAction::Grab {
                return self.grab_single_target(backend, loop_id);
            }
            BackendLoopMode::Recording
        } else if model.length > 0 && model.state.mode == LoopMode::Stopped {
            BackendLoopMode::Playing
        } else {
            BackendLoopMode::Stopped
        };
        self.transition_single_target(backend, loop_id, mode)
    }

    fn grab_targets(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
    ) -> Result<(), String> {
        self.grab_targets_inner(backend, initiating_loop, true)
    }

    fn grab_single_target(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
    ) -> Result<(), String> {
        self.grab_targets_inner(backend, initiating_loop, false)
    }

    fn grab_targets_inner(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
        include_selection: bool,
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
        let ids = if self.global.sync && include_selection {
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
        self.transition_targets_inner(backend, initiating_loop, mode, true)
    }

    fn transition_single_target(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
        mode: BackendLoopMode,
    ) -> Result<(), String> {
        self.transition_targets_inner(backend, initiating_loop, mode, false)
    }

    fn transition_targets_inner(
        &mut self,
        backend: &mut dyn Backend,
        initiating_loop: LoopId,
        mode: BackendLoopMode,
        include_selection: bool,
    ) -> Result<(), String> {
        let initiating_selected = include_selection
            && self
                .loops
                .get(&initiating_loop)
                .is_some_and(|model| model.state.selected);
        let targets: Vec<_> = self
            .loops
            .values()
            .filter(|model| {
                model.id == initiating_loop || initiating_selected && model.state.selected
            })
            .map(|model| {
                (
                    model.id,
                    model.track_id,
                    model.backend_id,
                    model.backend_composite,
                )
            })
            .collect();
        let delay = self.global.sync.then_some(self.target_delay());
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            let ids = targets.iter().map(|(id, _, _, _)| *id).collect::<Vec<_>>();
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
            let track_ids: Vec<_> = targets
                .iter()
                .map(|(_, track_id, _, _)| *track_id)
                .collect();
            let selected_ids: Vec<_> = targets.iter().map(|(id, _, _, _)| *id).collect();
            let others: Vec<_> = self
                .loops
                .values()
                .filter(|model| {
                    track_ids.contains(&model.track_id) && !selected_ids.contains(&model.id)
                })
                .map(|model| (model.id, model.backend_id, model.backend_composite))
                .collect();
            for (id, backend_id, composite_id) in others {
                if let Some(composite_id) = composite_id {
                    backend
                        .transition_composite_loop(
                            composite_id,
                            BackendLoopMode::Stopped,
                            delay,
                            None,
                        )
                        .map_err(|error| format!("could not solo-stop loop {id}: {error}"))?;
                } else {
                    backend
                        .transition_loop(backend_id, BackendLoopMode::Stopped, delay)
                        .map_err(|error| format!("could not solo-stop loop {id}: {error}"))?;
                }
            }
        }
        for (id, _, backend_id, composite_id) in targets {
            if let Some(composite_id) = composite_id {
                backend
                    .set_composite_play_after_record(composite_id, self.global.play_after_record)
                    .map_err(|error| format!("could not set composite option: {error}"))?;
                backend
                    .transition_composite_loop(composite_id, mode, delay, None)
                    .map_err(|error| format!("could not transition loop {id}: {error}"))?;
            } else {
                backend
                    .transition_loop(backend_id, mode, delay)
                    .map_err(|error| format!("could not transition loop {id}: {error}"))?;
            }
            if mode == BackendLoopMode::Recording && self.global.apply_n_cycles > 0 {
                let after = delay
                    .unwrap_or(0)
                    .saturating_add(self.global.apply_n_cycles);
                let next = if self.global.play_after_record {
                    BackendLoopMode::Playing
                } else {
                    BackendLoopMode::Stopped
                };
                if let Some(composite_id) = composite_id {
                    backend
                        .transition_composite_loop(composite_id, next, Some(after), None)
                        .map_err(|error| {
                            format!("could not schedule recording end for {id}: {error}")
                        })?;
                } else {
                    backend
                        .transition_loop(backend_id, next, Some(after))
                        .map_err(|error| {
                            format!("could not schedule recording end for {id}: {error}")
                        })?;
                }
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
            GlobalControlAction::MidiPanic => self.handle_midi_panic(backend)?,
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
            GlobalControlAction::SetSync(value) => {
                let sync = value.then(|| self.sync_backend_loop()).flatten();
                let targets = self
                    .loops
                    .values()
                    .filter(|model| {
                        self.tracks
                            .iter()
                            .find(|track| track.id == model.track_id)
                            .is_some_and(|track| !track.is_sync)
                    })
                    .map(|model| (model.id, model.backend_id, model.repeat_sync))
                    .collect::<Vec<_>>();
                let mut applied: Vec<(LoopId, BackendLoopId, bool)> = Vec::new();
                for (id, backend_id, previous) in &targets {
                    if let Err(error) = backend.set_loop_sync_source(*backend_id, sync) {
                        for (_, applied_backend, applied_previous) in applied.into_iter().rev() {
                            let previous_source =
                                applied_previous.then(|| self.sync_backend_loop()).flatten();
                            let _ = backend.set_loop_sync_source(applied_backend, previous_source);
                        }
                        return Err(format!(
                            "could not update loop {id} synchronization: {error}"
                        ));
                    }
                    applied.push((*id, *backend_id, *previous));
                }
                for (id, _, _) in targets {
                    self.loops.get_mut(&id).unwrap().repeat_sync = value;
                }
                self.global.sync = value;
            }
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
        let mut rejected_track_creations = BTreeSet::new();
        for failure in &snapshot.mutation_failures {
            match failure.detail.as_ref() {
                Some(BackendMutationDetail::TrackCreation) => {
                    if let Some(entity) = failure.entity {
                        rejected_track_creations.insert(BackendTrackId::from_raw(entity));
                    }
                }
                Some(BackendMutationDetail::TrackRemoval) => {
                    if let Some(entity) = failure.entity {
                        let backend_id = BackendTrackId::from_raw(entity);
                        if let Some(track) = self
                            .tracks
                            .iter_mut()
                            .find(|track| track.backend_id == backend_id)
                        {
                            track.structural_state = StructuralState::Confirmed;
                            for loop_id in &track.loops {
                                if let Some(model) = self.loops.get_mut(loop_id) {
                                    model.state.structural_state = StructuralState::Confirmed;
                                }
                            }
                        }
                    }
                }
                Some(BackendMutationDetail::LoopCreation { loop_id }) => {
                    if let Some((id, track_id)) = self.loops.iter().find_map(|(id, model)| {
                        (model.backend_id == *loop_id).then_some((*id, model.track_id))
                    }) {
                        self.loops.remove(&id);
                        if let Some(track) =
                            self.tracks.iter_mut().find(|track| track.id == track_id)
                        {
                            track.loops.retain(|candidate| *candidate != id);
                        }
                    }
                }
                _ => {}
            }
            match failure.detail.as_ref() {
                Some(
                    BackendMutationDetail::TrackCreation
                    | BackendMutationDetail::TrackRemoval
                    | BackendMutationDetail::LoopCreation { .. },
                ) => {}
                Some(BackendMutationDetail::TrackControl(rejected)) => {
                    if let Some(entity) = failure.entity {
                        let key = (
                            BackendTrackId::from_raw(entity),
                            track_control_key(*rejected),
                        );
                        if self.desired_track_controls.get(&key) == Some(rejected) {
                            self.desired_track_controls.remove(&key);
                        }
                    }
                }
                Some(BackendMutationDetail::LoopGain(rejected)) => {
                    if let Some(entity) = failure.entity {
                        let key = (BackendLoopId::from_raw(entity), LoopControlKey::Gain);
                        if self.desired_loop_controls.get(&key) == Some(rejected) {
                            self.desired_loop_controls.remove(&key);
                        }
                    }
                }
                Some(BackendMutationDetail::LoopBalance(rejected)) => {
                    if let Some(entity) = failure.entity {
                        let key = (BackendLoopId::from_raw(entity), LoopControlKey::Balance);
                        if self.desired_loop_controls.get(&key) == Some(rejected) {
                            self.desired_loop_controls.remove(&key);
                        }
                    }
                }
                Some(BackendMutationDetail::TrackFxControl(rejected)) => {
                    if let (Some(entity), Some(control_key)) =
                        (failure.entity, fx_control_key(rejected))
                    {
                        let key = (BackendTrackId::from_raw(entity), control_key);
                        if self.desired_fx_controls.get(&key) == Some(rejected) {
                            self.desired_fx_controls.remove(&key);
                        }
                    }
                }
                None => {}
            }
            self.report_error(format!(
                "Backend rejected {:?} mutation (driver generation {}, sequence {}): {}",
                failure.kind, failure.driver_generation, failure.sequence, failure.message
            ));
        }
        let rejected_indices = self
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                rejected_track_creations
                    .contains(&track.backend_id)
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        for index in rejected_indices.into_iter().rev() {
            self.remove_track_model(index);
        }
        let confirmed_removal_indices = self
            .tracks
            .iter()
            .enumerate()
            .filter_map(|(index, track)| {
                (track.structural_state == StructuralState::Removing
                    && !snapshot.tracks.contains_key(&track.backend_id))
                .then_some(index)
            })
            .collect::<Vec<_>>();
        for index in confirmed_removal_indices.into_iter().rev() {
            self.remove_track_model(index);
        }
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
            self.report_warning(format!(
                "Audio recovered after memory grew during {render_memory_growths} render {callback_label}; timing may have been disrupted"
            ));
        }
        self.status.command_overflows = snapshot.status.command_overflows;
        self.status.storage_low_channels = snapshot.status.storage_low_channels;
        self.status.storage_exhaustions = snapshot.status.storage_exhaustions;
        self.status.backend_capture_latency = snapshot.status.backend_capture_latency;
        self.status.backend_playback_latency = snapshot.status.backend_playback_latency;
        self.status.latency_diagnostics = snapshot.status.latency_diagnostics;
        let diagnostics = self.status.latency_diagnostics;
        self.status.latency_diagnostic_summary = format!(
            "unresolved={} changes={} margins={} deferred={} finalization={} ambiguity={} providers={}",
            diagnostics.unresolved_recipes,
            diagnostics.observation_changes,
            diagnostics.insufficient_margins,
            diagnostics.deferred_transitions,
            diagnostics.finalization_overruns,
            diagnostics.path_ambiguities,
            diagnostics.provider_failures,
        );
        self.desired_track_controls
            .retain(|(backend_id, _), desired| {
                !snapshot
                    .tracks
                    .get(backend_id)
                    .is_some_and(|state| track_control_matches(state, *desired))
            });
        self.desired_fx_controls.retain(|(backend_id, _), desired| {
            !snapshot
                .tracks
                .get(backend_id)
                .is_some_and(|state| fx_control_matches(state.fx.as_ref(), desired))
        });
        self.desired_loop_controls
            .retain(|(backend_id, key), desired| {
                !snapshot
                    .loops
                    .get(backend_id)
                    .is_some_and(|state| match key {
                        LoopControlKey::Gain => (state.gain - *desired).abs() <= f32::EPSILON,
                        LoopControlKey::Balance => (state.balance - *desired).abs() <= f32::EPSILON,
                    })
            });
        let backend_to_application_ports = self
            .connection_ports
            .values()
            .map(|port| (port.backend_id.raw(), port.id))
            .collect::<BTreeMap<_, _>>();
        for track in &mut self.tracks {
            let Some(backend_state) = snapshot.tracks.get(&track.backend_id) else {
                continue;
            };
            if track.structural_state == StructuralState::Creating {
                track.structural_state = StructuralState::Confirmed;
            }
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
            track.latency_policy = application_track_latency_policy(
                &backend_state.latency_policy,
                &backend_to_application_ports,
            );
            if let Some(fx) = track.fx.as_mut() {
                for ((backend_id, _), desired) in &self.desired_fx_controls {
                    if *backend_id == track.backend_id {
                        apply_fx_control(fx, desired);
                    }
                }
            }
            let controls = &mut track.controls;
            controls.has_output = output_audio_channels > 0 || output_midi;
            controls.has_output_audio = output_audio_channels > 0;
            controls.output_stereo = output_audio_channels == 2;
            controls.output_gain_db = backend_state.output_gain_db;
            controls.output_balance = backend_state.output_balance;
            controls.output_muted = backend_state.output_muted;
            (controls.output_peak_left_db, controls.output_peak_right_db) =
                display_peaks(&backend_state.output_peaks, controls.output_stereo);
            controls.output_midi_activity = backend_state.output_midi_activity;
            controls.has_input = input_audio_channels > 0 || input_midi;
            controls.has_input_audio = input_audio_channels > 0;
            controls.input_stereo = input_audio_channels == 2;
            controls.input_gain_db = backend_state.input_gain_db;
            controls.input_balance = backend_state.input_balance;
            controls.input_monitoring = backend_state.input_monitoring;
            (controls.input_peak_left_db, controls.input_peak_right_db) =
                display_peaks(&backend_state.input_peaks, controls.input_stereo);
            controls.input_midi_activity = backend_state.input_midi_activity;
            controls.latest_input_midi_message =
                backend_state.latest_input_midi_message.map(|message| {
                    shoop_app_api::LatestMidiMessage {
                        bytes: message.bytes,
                        len: message.len,
                    }
                });
            for ((backend_id, _), desired) in &self.desired_track_controls {
                if *backend_id == track.backend_id {
                    apply_track_control(controls, *desired);
                }
            }
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
            if model.state.structural_state == StructuralState::Creating {
                model.state.structural_state = StructuralState::Confirmed;
            }
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
            model.state.gain = self
                .desired_loop_controls
                .get(&(model.backend_id, LoopControlKey::Gain))
                .copied()
                .unwrap_or(backend_state.gain);
            model.state.balance = self
                .desired_loop_controls
                .get(&(model.backend_id, LoopControlKey::Balance))
                .copied()
                .unwrap_or(backend_state.balance);
            (model.state.peak_left_db, model.state.peak_right_db) =
                display_peaks(&backend_state.audio_peaks, model.state.stereo);
            model.state.midi_activity = backend_state.midi_activity;
            let local_latency_error = model.state.latency.error.take();
            model.state.latency.clone_from(&backend_state.latency);
            if local_latency_error.is_some() {
                model.state.latency.error = local_latency_error;
            }
        }
        for track in &mut self.tracks {
            track.controls.output_midi_activity = combined_output_midi_activity(
                track.controls.output_midi_activity,
                track.loops.iter().filter_map(|loop_id| {
                    self.loops
                        .get(loop_id)
                        .map(|loop_| loop_.state.midi_activity)
                }),
            );
        }
        let app_loop_by_backend = self
            .loops
            .values()
            .map(|model| (model.backend_id, model.id))
            .collect::<BTreeMap<_, _>>();
        let app_composite_by_backend = self
            .loops
            .values()
            .filter_map(|model| model.backend_composite.map(|id| (id, model.id)))
            .collect::<BTreeMap<_, _>>();
        for model in self.loops.values_mut() {
            let Some(composite_id) = model.backend_composite else {
                continue;
            };
            let Some(state) = snapshot.composites.get(&composite_id) else {
                continue;
            };
            model.state.mode = app_loop_mode(state.mode);
            model.state.next_mode = state
                .next_mode
                .map(app_loop_mode)
                .unwrap_or(model.state.mode);
            model.state.next_transition_delay = state.next_transition_delay;
            model.state.composite_iteration = Some(state.iteration);
            model.state.composite_cycle_count = state.cycle_count;
            model.state.active_composite_children = state
                .active_children
                .iter()
                .filter_map(|child| match child.target {
                    BackendCompositeTarget::Loop(id) => app_loop_by_backend.get(&id).copied(),
                    BackendCompositeTarget::Composite(id) => {
                        app_composite_by_backend.get(&id).copied()
                    }
                })
                .collect::<Vec<_>>()
                .into();
            model.length = u32::try_from(state.length).unwrap_or(u32::MAX);
            model.position = u32::try_from(state.position).unwrap_or(u32::MAX);
            model.state.empty = model
                .composite
                .as_ref()
                .is_none_or(|composite| composite.instances.is_empty());
            model.state.position = if model.length == 0 {
                0.0
            } else {
                model.position as f32 / model.length as f32
            };
        }
        let composites = self
            .loops
            .values()
            .filter(|model| {
                model.backend_composite.is_none() && !model.script_composition.is_empty()
            })
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
            let Some(active_section) = self
                .script_composition_playback
                .get(&target)
                .map(|playback| playback.section)
            else {
                if let Some(model) = self.loops.get_mut(&target) {
                    model.state.mode = LoopMode::Stopped;
                    model.state.next_mode = LoopMode::Stopped;
                    model.state.next_transition_delay = None;
                    model.state.composite_iteration = None;
                    model.state.composite_cycle_count = 0;
                    model.state.active_composite_children = Arc::from([]);
                    model.position = 0;
                    model.state.position = 0.0;
                }
                continue;
            };
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
        for port in self.connection_ports.values_mut() {
            if let Some(latency) = snapshot.port_latency.get(&port.backend_id) {
                port.capture_latency = latency.capture;
                port.playback_latency = latency.playback;
            }
        }
        self.apply_connection_snapshot(snapshot.connections);
    }

    fn apply_connection_snapshot(&mut self, snapshot: BackendConnectionSnapshot) {
        self.connection_backend_available = snapshot.available;
        for descriptor in snapshot
            .application_ports
            .values()
            .filter(|port| port.owner == BackendPortOwner::GlobalFxControl)
        {
            if self
                .connection_ports
                .values()
                .all(|port| port.backend_id != descriptor.id)
            {
                let id = PortId::from_raw(self.next_port_id);
                self.next_port_id = self.next_port_id.saturating_add(1);
                self.connection_ports.insert(
                    id,
                    ConnectionPortModel {
                        id,
                        backend_id: descriptor.id,
                        owner: ApplicationPortOwner::GlobalFxControl,
                        name: descriptor.name.clone(),
                        data_type: PortDataType::Midi,
                        direction: PortDirection::Input,
                        role: PortRole::MidiInput,
                        capture_latency: Default::default(),
                        playback_latency: Default::default(),
                        candidates: BTreeMap::new(),
                    },
                );
            }
        }
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
            .map(|port| ApplicationPortState {
                id: port.id,
                owner: port.owner.clone(),
                name: port.name.clone(),
                data_type: port.data_type,
                direction: port.direction,
                role: port.role,
                connection_policy: ConnectionPolicy::UserManaged,
                capture_latency: port.capture_latency,
                playback_latency: port.playback_latency,
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
                    capture_latency: Default::default(),
                    playback_latency: Default::default(),
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
        self.report_error(message);
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
                        latency: document_take_latency(&audio.latency, capture.sample_rate),
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
                                latency: document_take_latency(&midi.latency, capture.sample_rate),
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
                        latency: document_take_latency(&midi.latency, capture.sample_rate),
                    });
                }
                loops.push(LoopDocument {
                    id: loop_id.raw(),
                    name: model.name.clone(),
                    length_frames: u64::from(model.length),
                    is_sync: model.state.sync,
                    gain: content.gain,
                    balance: content.balance,
                    channels: if model.composite.is_none() {
                        channels
                    } else {
                        Vec::new()
                    },
                    composite: model.composite.clone(),
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
                    let topology = if chain_type == FxChainTypeDocument::OxiSynth {
                        if *dry_audio_channels != 2 || *wet_audio_channels != 2 || !dry_midi {
                            return Err(format!(
                                "OxiSynth track {} has an invalid channel shape",
                                track.id
                            ));
                        }
                        TrackTopologyDocument::OxiSynth
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
                                .oxisynth_midi_cc_assignments
                                .iter()
                                .copied()
                                .map(document_oxisynth_midi_cc_assignment)
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
                latency_policy: document_track_latency_policy(&track.latency_policy),
            };
            if track.is_sync {
                sync_tracks.push(document);
            } else {
                main_tracks.push(document);
            }
        }
        let global_ports = capture
            .global_ports
            .iter()
            .map(|captured| {
                let app_port = self
                    .connection_ports
                    .values()
                    .find(|port| port.backend_id == captured.descriptor.id)
                    .ok_or_else(|| "backend omitted global port mapping".to_owned())?;
                Ok(PortDocument {
                    id: app_port.id.raw(),
                    name: "Global FX Control MIDI In".to_owned(),
                    data_type: DataTypeDocument::Midi,
                    direction: PortDirectionDocument::Input,
                    role: PortRoleDocument::MidiInput,
                    input_connectability: vec![ConnectabilityDocument::External],
                    output_connectability: vec![ConnectabilityDocument::Internal],
                    gain: 1.0,
                    muted: false,
                    passthrough_muted: true,
                    internal_connections: Vec::new(),
                    external_connections: captured.external_connections.clone(),
                    ringbuffer_frames: 0,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let (script_documents, scripts) = self.session_script_documents();
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
            global_ports,
            fx_states,
            scripts: script_documents,
            midi_control: MidiControlDocument::default(),
            settings: Vec::new(),
        };
        Ok(SessionBundle {
            document,
            media,
            scripts,
        })
    }

    fn session_script_documents(
        &self,
    ) -> (
        Vec<ScriptDocument>,
        BTreeMap<u64, Arc<shoop_scripting::ScriptResourceBundle>>,
    ) {
        let scripts = self.script_manager.session_scripts();
        let documents = scripts
            .iter()
            .map(|script| ScriptDocument {
                id: script.document_id,
                name: script.name.clone(),
                entrypoint: script.bundle.entrypoint.to_string(),
                enabled: script.enabled,
            })
            .collect();
        let resources = scripts
            .into_iter()
            .map(|script| (script.document_id, script.bundle))
            .collect();
        (documents, resources)
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
                        owner: ApplicationPortOwner::Track {
                            track_id: TrackId::from_raw(track_document.id),
                            kind: if track_document.is_sync {
                                TrackPortOwnerKind::Sync
                            } else {
                                TrackPortOwnerKind::Main
                            },
                        },
                        name: document.name.clone(),
                        data_type: app_data_type(document.data_type),
                        direction: app_port_direction(document.direction),
                        role: app_port_role(document.role),
                        capture_latency: Default::default(),
                        playback_latency: Default::default(),
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
                        .set_loop_sync_source(
                            *backend_loop,
                            bundle.document.global.sync.then_some(sync_source).flatten(),
                        )
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
                let latency = loop_document
                    .channels
                    .first()
                    .map(|channel| app_take_latency_document(&channel.latency))
                    .transpose()?
                    .unwrap_or_default();
                let composite = loop_document.composite.clone();
                let script_composition = composite
                    .as_ref()
                    .map(|composite| {
                        composite
                            .instances
                            .iter()
                            .map(|instance| vec![LoopId::from_raw(instance.loop_id)])
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
                            latency,
                            ..Default::default()
                        },
                        length: u32::try_from(loop_document.length_frames)
                            .map_err(|_| "loop length exceeds engine range".to_owned())?,
                        position: 0,
                        audio_data: None,
                        midi_data: None,
                        script_composition,
                        composite,
                        backend_composite: None,
                        backend_composite_signature: Vec::new(),
                        repeat_sync: bundle.document.global.sync,
                        recorded_fx_state,
                    },
                );
            }
            tracks.push(TrackModel {
                id: TrackId::from_raw(track_document.id),
                backend_id: created.track_id,
                name: track_document.name.clone(),
                structural_state: StructuralState::Confirmed,
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
                latency_policy: app_track_latency_policy(&track_document.latency_policy)?,
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
        for document in &bundle.document.global_ports {
            let backend_id = replacement
                .global_ports
                .get(&document.id)
                .copied()
                .ok_or_else(|| "backend omitted loaded global FX control port".to_owned())?;
            connection_ports.insert(
                PortId::from_raw(document.id),
                ConnectionPortModel {
                    id: PortId::from_raw(document.id),
                    backend_id,
                    owner: ApplicationPortOwner::GlobalFxControl,
                    name: document.name.clone(),
                    data_type: PortDataType::Midi,
                    direction: PortDirection::Input,
                    role: PortRole::MidiInput,
                    capture_latency: Default::default(),
                    playback_latency: Default::default(),
                    candidates: BTreeMap::new(),
                },
            );
        }
        if bundle.document.global_ports.is_empty() {
            let (source_id, backend_id) = replacement
                .global_ports
                .iter()
                .next()
                .ok_or_else(|| "backend omitted migrated global FX control port".to_owned())?;
            connection_ports.insert(
                PortId::from_raw(*source_id),
                ConnectionPortModel {
                    id: PortId::from_raw(*source_id),
                    backend_id: *backend_id,
                    owner: ApplicationPortOwner::GlobalFxControl,
                    name: "Global FX Control MIDI In".to_owned(),
                    data_type: PortDataType::Midi,
                    direction: PortDirection::Input,
                    role: PortRole::MidiInput,
                    capture_latency: Default::default(),
                    playback_latency: Default::default(),
                    candidates: BTreeMap::new(),
                },
            );
        }
        self.next_port_id = connection_ports
            .keys()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.tracks = tracks;
        self.loops = loops;
        self.restore_backend_composites(backend)?;
        self.script_composition_playback.clear();
        self.script_composition_frame_remainder = 0;
        self.active_piano_notes.clear();
        self.connection_ports = connection_ports;
        self.pending_connections.clear();
        self.desired_track_controls.clear();
        self.desired_fx_controls.clear();
        self.desired_loop_controls.clear();
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
            .replace_session_scripts(&session_script_sources(bundle)?)
            .map_err(|error| error.to_string())?;
        self.refresh_scripting_view();
        Ok(())
    }

    fn snapshot(&self) -> AppSnapshot {
        let selected_composite_references = self
            .loops
            .values()
            .filter(|model| model.state.selected)
            .filter_map(|model| {
                model.composite.as_ref().map(|composite| {
                    (
                        model.state.composite_kind,
                        composite
                            .instances
                            .iter()
                            .map(|event| LoopId::from_raw(event.loop_id)),
                    )
                })
            })
            .flat_map(|(kind, references)| references.map(move |loop_id| (loop_id, kind)))
            .collect::<BTreeMap<_, _>>();
        AppSnapshot {
            revision: self.revision,
            tracks: self
                .tracks
                .iter()
                .map(|track| TrackState {
                    id: track.id,
                    name: track.name.clone(),
                    structural_state: track.structural_state,
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
                            state.selected_composite_kind = selected_composite_references
                                .get(&model.id)
                                .copied()
                                .unwrap_or_default();
                            state
                        })
                        .collect(),
                    controls: track.controls.clone(),
                    latency_policy: track.latency_policy.clone(),
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
                    .channels
                    .iter()
                    .enumerate()
                    .map(|(index, channel)| WaveformChannelState {
                        id: ChannelId::from_raw((model.id.raw() << 8) | index as u64 + 1),
                        label: labels
                            .get(index)
                            .cloned()
                            .unwrap_or_else(|| format!("Audio {}", index + 1)),
                        samples: Arc::clone(&channel.samples),
                        start_offset: i64::from(channel.start_offset),
                        preplay_samples: u64::from(channel.preplay),
                        loop_length: model.length as u64,
                        played_sample: matches!(model.state.mode, LoopMode::Playing)
                            .then_some(model.position as i64),
                        latency: app_backend_take_latency(&channel.latency),
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
        let composite = model.composite.as_ref().map(|composite| {
            let mut details = self.composite_details_snapshot(composite);
            details.played_frame = matches!(
                model.state.mode,
                LoopMode::Playing | LoopMode::PlayingDryThroughWet
            )
            .then_some(u64::from(model.position));
            details
        });
        Some(LoopDetailsState {
            generation: self.revision,
            loop_id: model.id,
            title: model.name.clone(),
            loading: composite.is_none() && model.state.has_audio && model.audio_data.is_none(),
            channels,
            midi_loading: composite.is_none() && model.state.has_midi && model.midi_data.is_none(),
            midi_channels,
            sync_loop_length: u64::from(self.sync_length()),
            composite,
        })
    }

    fn composite_details_snapshot(&self, composite: &CompositeDocument) -> CompositeDetailsState {
        let cycle_length_frames = u64::from(self.sync_length()).max(1);
        let tracks = self
            .tracks
            .iter()
            .filter(|track| !track.is_sync)
            .map(|track| CompositeTrackDetailsState {
                id: track.id,
                name: track.name.clone(),
            })
            .collect();
        let mut events = Vec::new();
        let mut timeline_length_frames = 0_u64;
        for event in &composite.instances {
            let Some(source) = self.loops.get(&LoopId::from_raw(event.loop_id)) else {
                continue;
            };
            let source_duration = u64::from(source.length);
            let natural_duration = if cycle_length_frames > 0 {
                source_duration
                    .div_ceil(cycle_length_frames)
                    .max(1)
                    .saturating_mul(cycle_length_frames)
            } else {
                source_duration.max(1)
            };
            let duration = event
                .n_cycles
                .filter(|cycles| *cycles > 0)
                .map(|cycles| {
                    let cycle = if cycle_length_frames > 0 {
                        cycle_length_frames
                    } else {
                        natural_duration
                    };
                    cycle.saturating_mul(u64::from(cycles))
                })
                .unwrap_or(natural_duration);
            let start_frame = event.start_cycle.saturating_mul(cycle_length_frames);
            let end_frame = start_frame.saturating_add(duration);
            timeline_length_frames = timeline_length_frames.max(end_frame);
            events.push(CompositeEventDetailsState {
                loop_id: source.id,
                loop_name: source.name.clone(),
                track_id: source.track_id,
                start_frame,
                end_frame,
                instance_id: event.instance_id,
                mode: event.mode.clone(),
                forced_n_cycles: event.n_cycles,
                loop_mode: source.state.mode,
            });
        }
        CompositeDetailsState {
            kind: match composite.kind {
                CompositeKindDocument::Regular => shoop_app_api::CompositeKind::Regular,
                CompositeKindDocument::Script => shoop_app_api::CompositeKind::Script,
            },
            cycle_length_frames,
            timeline_length_frames,
            played_frame: None,
            tracks,
            events,
        }
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
                .map(|(channel, source)| {
                    Ok(BackendAudioChannelUpdate {
                        channel,
                        samples: source.samples.clone(),
                        start_offset: None,
                        preplay: None,
                        latency: Some(backend_take_latency(&source.latency, audio.sample_rate)?),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
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
                latency: Some(backend_take_latency(&midi.latency, midi.sample_rate)?),
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
                    latency: Some(BackendTakeLatencySnapshot::default()),
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
                latency: Some(BackendTakeLatencySnapshot::default()),
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
        manual_offset_frames: Option<i32>,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        let task_id = self.start_io_task(IoTaskKind::ImportLoopAudio, "Loading loop audio");
        let mut audio = match if name.to_ascii_lowercase().ends_with(".wav") {
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
        if let Some(offset) = manual_offset_frames {
            let result = audio.channels.iter_mut().try_for_each(|channel| {
                apply_manual_import_offset(
                    &mut channel.latency,
                    offset,
                    channel.samples.len() as u64,
                    audio.sample_rate,
                )
            });
            if let Err(message) = result {
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        }
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
        format: LoopMidiExportFormat,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        self.start_io_task(IoTaskKind::ExportLoopMidi, "Exporting loop MIDI");
        self.pending_io = Some(PendingIo::ExportLoopMidi { loop_id, format });
        Ok(())
    }

    fn import_loop_midi(
        &mut self,
        _backend: &mut dyn Backend,
        loop_id: LoopId,
        name: String,
        bytes: &[u8],
        update_loop_length: bool,
        manual_offset_frames: Option<i32>,
    ) -> Result<(), String> {
        self.ensure_io_idle()?;
        self.start_io_task(IoTaskKind::ImportLoopMidi, "Loading loop MIDI");
        let mut midi = match if name.to_ascii_lowercase().ends_with(".mid") {
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
        if let Some(offset) = manual_offset_frames {
            if let Err(message) = apply_manual_import_offset(
                &mut midi.latency,
                offset,
                midi.length_frames,
                midi.sample_rate,
            ) {
                self.finish_io(IoTaskStatus::Failed, &message);
                return Err(message);
            }
        }
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
    ) -> Result<(), BackendIoStepError> {
        let task_id = self
            .io_task
            .as_ref()
            .map(|task| task.id)
            .ok_or_else(|| "loop audio export has no task".to_owned())?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let capture = match backend.capture_session_async().map_err(|error| {
            BackendIoStepError::Failed(format!("could not capture loop: {error}"))
        })? {
            BackendAsyncResult::Ready(capture) => capture,
            BackendAsyncResult::Pending(progress) => {
                return Err(BackendIoStepError::Pending(progress));
            }
        };
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
        let raw_export = matches!(
            format,
            LoopAudioExportFormat::RawExact | LoopAudioExportFormat::RawFloatWav
        );
        let audio = LoopAudio {
            sample_rate: capture.sample_rate,
            channels: selected_channels
                .iter()
                .map(|index| {
                    let channel = content
                        .audio
                        .get(*index as usize)
                        .ok_or_else(|| "selected audio channel is unavailable".to_owned())?;
                    let mapping = shoop_latency::ScalarFrameMapping::new(
                        channel.latency.capture_alignment_frames,
                    )
                    .map_err(|error| error.to_string())?;
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
                        samples: if raw_export {
                            channel.samples.clone()
                        } else {
                            (0..content.length as usize)
                                .map(|logical| {
                                    mapping
                                        .raw_media_frame(
                                            logical as i64,
                                            i64::from(channel.start_offset),
                                        )
                                        .ok()
                                        .and_then(|raw| usize::try_from(raw).ok())
                                        .and_then(|raw| channel.samples.get(raw).copied())
                                        .unwrap_or(0.0)
                                })
                                .collect()
                        },
                        latency: if raw_export {
                            document_take_latency(&channel.latency, capture.sample_rate)
                        } else {
                            TakeLatencyDocument::default()
                        },
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        let (bytes, extension, mime_type) = match format {
            LoopAudioExportFormat::Exact | LoopAudioExportFormat::RawExact => (
                encode_loop_audio(&audio).map_err(|error| error.to_string())?,
                "shoop-audio",
                "application/x-shoop-audio",
            ),
            LoopAudioExportFormat::FloatWav | LoopAudioExportFormat::RawFloatWav => (
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
        format: LoopMidiExportFormat,
    ) -> Result<(), BackendIoStepError> {
        let task_id = self
            .io_task
            .as_ref()
            .map(|task| task.id)
            .ok_or_else(|| "loop MIDI export has no task".to_owned())?;
        let model = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| format!("stale loop {loop_id}"))?;
        let capture = match backend.capture_session_async().map_err(|error| {
            BackendIoStepError::Failed(format!("could not capture loop: {error}"))
        })? {
            BackendAsyncResult::Ready(capture) => capture,
            BackendAsyncResult::Pending(progress) => {
                return Err(BackendIoStepError::Pending(progress));
            }
        };
        let loop_content = capture
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == model.backend_id.raw())
            .ok_or_else(|| "backend omitted loop content".to_owned())?;
        let content = loop_content
            .midi
            .first()
            .ok_or_else(|| "loop has no MIDI channel".to_owned())?;
        let mut midi = ExactMidi {
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
            latency: document_take_latency(&content.latency, capture.sample_rate),
        };
        let raw = matches!(
            format,
            LoopMidiExportFormat::Exact | LoopMidiExportFormat::RawStandard
        );
        if !raw {
            let mapping =
                shoop_latency::ScalarFrameMapping::new(content.latency.capture_alignment_frames)
                    .map_err(|error| error.to_string())?;
            let selected_start = mapping
                .raw_media_frame(0, i64::from(content.start_offset))
                .map_err(|error| error.to_string())?;
            midi.start_state = logical_midi_start_state(content, selected_start);
            midi.events = midi
                .events
                .into_iter()
                .filter_map(|mut event| {
                    let logical = mapping
                        .logical_media_frame(
                            i64::try_from(event.frame).ok()?,
                            i64::from(content.start_offset),
                        )
                        .ok()?;
                    if logical < 0 || logical >= i64::from(loop_content.length) {
                        return None;
                    }
                    event.frame = logical as u64;
                    Some(event)
                })
                .collect();
            midi.length_frames = u64::from(loop_content.length);
            midi.latency = TakeLatencyDocument::default();
        }
        let (bytes, extension, mime) = match format {
            LoopMidiExportFormat::Exact => (
                encode_exact_midi(&midi).map_err(|error| error.to_string())?,
                "shoop-midi",
                "application/x-shoop-midi",
            ),
            LoopMidiExportFormat::Standard | LoopMidiExportFormat::RawStandard => {
                let encoded = encode_standard_midi(&midi).map_err(|error| error.to_string())?;
                tracing::warn!(
                    max_error_samples = encoded.max_quantization_error_frames,
                    raw,
                    "frontend.app.standard_midi_quantized"
                );
                (encoded.bytes, "mid", "audio/midi")
            }
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

    fn report_warning(&self, message: String) {
        tracing::warn!(diagnostic = %message, "frontend.app.warning");
    }

    fn report_error(&self, message: String) {
        tracing::error!(error = %message, "frontend.app.operation_failed");
    }

    fn report_periodic_result(
        &mut self,
        operation: &'static str,
        result: std::result::Result<(), String>,
    ) {
        match result {
            Ok(()) => self.clear_periodic_failure(operation),
            Err(error) => {
                self.report_periodic_failure(operation, error);
            }
        }
    }

    fn report_periodic_failure(&mut self, operation: &'static str, message: String) -> bool {
        if !self.active_periodic_failures.insert(operation) {
            return false;
        }
        tracing::error!(operation, error = %message, "frontend.app.periodic_operation_failed");
        true
    }

    fn clear_periodic_failure(&mut self, operation: &'static str) {
        if self.active_periodic_failures.remove(operation) {
            tracing::info!(operation, "frontend.app.periodic_operation_recovered");
        }
    }
}

fn new_session_document(sample_rate: u32) -> SessionDocument {
    let mut document = SessionDocument::empty(sample_rate);
    document.track_groups.push(TrackGroupDocument {
        name: "sync".to_owned(),
        tracks: vec![TrackDocument {
            id: 1,
            name: "Sync".to_owned(),
            port_name_base: "sync_loop".to_owned(),
            is_sync: true,
            width: None,
            topology: TrackTopologyDocument::Direct {
                audio_channels: 1,
                midi: false,
            },
            controls: TrackControlsDocument::default(),
            loops: vec![LoopDocument {
                id: 1,
                name: "sync loop".to_owned(),
                length_frames: 0,
                is_sync: true,
                gain: 1.0,
                balance: 0.0,
                channels: vec![ChannelDocument {
                    id: 1,
                    mode: ChannelModeDocument::Direct,
                    data_type: DataTypeDocument::Audio,
                    data_length_frames: 0,
                    start_offset_frames: 0,
                    preplay_frames: 0,
                    gain: 1.0,
                    connected_port_ids: vec![1, 2],
                    media_id: None,
                    recording_started_at: None,
                    recording_fx_state_id: None,
                    latency: TakeLatencyDocument::default(),
                }],
                composite: None,
            }],
            ports: vec![
                PortDocument {
                    id: 1,
                    name: "sync_loop_direct_in".to_owned(),
                    data_type: DataTypeDocument::Audio,
                    direction: PortDirectionDocument::Input,
                    role: PortRoleDocument::AudioInput,
                    input_connectability: vec![ConnectabilityDocument::External],
                    output_connectability: vec![ConnectabilityDocument::Internal],
                    gain: 1.0,
                    muted: false,
                    passthrough_muted: false,
                    internal_connections: Vec::new(),
                    external_connections: Vec::new(),
                    ringbuffer_frames: 0,
                },
                PortDocument {
                    id: 2,
                    name: "sync_loop_direct_out".to_owned(),
                    data_type: DataTypeDocument::Audio,
                    direction: PortDirectionDocument::Output,
                    role: PortRoleDocument::AudioOutput,
                    input_connectability: vec![ConnectabilityDocument::Internal],
                    output_connectability: vec![ConnectabilityDocument::External],
                    gain: 1.0,
                    muted: false,
                    passthrough_muted: false,
                    internal_connections: Vec::new(),
                    external_connections: Vec::new(),
                    ringbuffer_frames: 0,
                },
            ],
            fx_chain: None,
            latency_policy: TrackLatencyPolicyDocument::default(),
        }],
    });
    document
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

fn logical_midi_start_state(content: &BackendMidiContent, selected_start: i64) -> Vec<Vec<u8>> {
    let mut messages = content.start_state.clone();
    if selected_start > 0 {
        messages.extend(
            content
                .events
                .iter()
                .filter(|event| i64::from(event.time) < selected_start)
                .map(|event| event.data.clone()),
        );
    }

    // Canonicalize the messages into the state a fresh receiver needs at the
    // logical boundary. This avoids replaying transient notes from retained
    // preroll while preserving the latest controllers and active notes.
    let mut state = BTreeMap::<(u8, u8, u8), Vec<u8>>::new();
    let mut passthrough = Vec::new();
    for message in messages {
        let Some(&status) = message.first() else {
            continue;
        };
        let kind = status & 0xF0;
        let channel = status & 0x0F;
        match kind {
            0x80 if message.len() >= 2 => {
                state.remove(&(4, channel, message[1]));
            }
            0x90 if message.len() >= 3 => {
                let key = (4, channel, message[1]);
                if message[2] == 0 {
                    state.remove(&key);
                } else {
                    state.insert(key, message);
                }
            }
            0xB0 if message.len() >= 3 => {
                if matches!(message[1], 120 | 123) {
                    state.retain(|(group, existing_channel, _), _| {
                        *group != 4 || *existing_channel != channel
                    });
                } else {
                    state.insert((0, channel, message[1]), message);
                }
            }
            0xC0 if message.len() >= 2 => {
                state.insert((1, channel, 0), message);
            }
            0xD0 if message.len() >= 2 => {
                state.insert((2, channel, 0), message);
            }
            0xE0 if message.len() >= 3 => {
                state.insert((3, channel, 0), message);
            }
            _ => passthrough.push(message),
        }
    }
    passthrough.extend(state.into_values());
    passthrough
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

fn document_oxisynth_midi_cc_assignment(
    assignment: BackendOxiSynthMidiCcAssignment,
) -> OxiSynthMidiCcAssignmentDocument {
    let parameter = match assignment.parameter {
        BackendOxiSynthParameter::ReverbSend => OxiSynthParameterDocument::ReverbSend,
        BackendOxiSynthParameter::ChorusSend => OxiSynthParameterDocument::ChorusSend,
    };
    OxiSynthMidiCcAssignmentDocument {
        parameter,
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn backend_oxisynth_midi_cc_assignment(
    assignment: OxiSynthMidiCcAssignmentDocument,
) -> BackendOxiSynthMidiCcAssignment {
    let parameter = match assignment.parameter {
        OxiSynthParameterDocument::ReverbSend => BackendOxiSynthParameter::ReverbSend,
        OxiSynthParameterDocument::ChorusSend => BackendOxiSynthParameter::ChorusSend,
    };
    BackendOxiSynthMidiCcAssignment {
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
        shoop_app_api::TrackProcessorTypeId::OXISYNTH => Ok(FxChainTypeDocument::OxiSynth),
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
        FxChainTypeDocument::OxiSynth => shoop_app_api::TrackProcessorTypeId::OXISYNTH,
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
        TrackTopologyDocument::OxiSynth => {
            let processor = shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::OXISYNTH,
            );
            validate_loaded_processor(track.id, &processor, 2, 2, true, processors)?;
            Ok((
                BackendTrackTopology::DryWetProcessor {
                    processor_type: processor.as_str().to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                TrackTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                    processor_type: processor,
                },
                2,
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

fn session_script_sources(bundle: &SessionBundle) -> Result<Vec<SessionScriptSource>, String> {
    bundle
        .document
        .scripts
        .iter()
        .map(|script| {
            let resources = bundle.scripts.get(&script.id).ok_or_else(|| {
                format!("session script {} resource bundle is missing", script.id)
            })?;
            let source = std::str::from_utf8(&resources.entrypoint_resource().bytes)
                .map_err(|error| {
                    format!("session script {} source is not UTF-8: {error}", script.id)
                })?
                .to_owned();
            Ok(SessionScriptSource {
                document_id: script.id,
                name: script.name.clone(),
                source,
                bundle: Arc::clone(resources),
                enabled: script.enabled,
            })
        })
        .collect()
}

fn valid_global_fx_port_document(port: &PortDocument) -> bool {
    port.name == "Global FX Control MIDI In"
        && port.data_type == DataTypeDocument::Midi
        && port.direction == PortDirectionDocument::Input
        && port.role == PortRoleDocument::MidiInput
        && port.input_connectability == [ConnectabilityDocument::External]
        && port.output_connectability == [ConnectabilityDocument::Internal]
        && port.gain == 1.0
        && !port.muted
        && port.passthrough_muted
        && port.internal_connections.is_empty()
        && port.ringbuffer_frames == 0
}

fn session_bundle_to_backend(
    bundle: &SessionBundle,
    processors: &[TrackProcessorDescriptor],
) -> Result<BackendSessionData, String> {
    if !bundle.document.buses.is_empty()
        || !bundle.document.midi_control.bindings.is_empty()
        || !bundle.document.settings.is_empty()
    {
        return Err(
            "session requires a feature not yet available in the application runtime".to_owned(),
        );
    }
    let used_ids = bundle
        .document
        .track_groups
        .iter()
        .flat_map(|group| &group.tracks)
        .flat_map(|track| track.ports.iter().map(|port| port.id))
        .collect::<BTreeSet<_>>();
    let global_document = match bundle.document.global_ports.as_slice() {
        [] => None,
        [port] if valid_global_fx_port_document(port) => Some(port.clone()),
        _ => return Err("session global FX control port is invalid".to_owned()),
    };
    let global_id = global_document
        .as_ref()
        .map(|port| port.id)
        .unwrap_or_else(|| {
            let mut id = 1_u64;
            while used_ids.contains(&id) {
                id = id.saturating_add(1);
            }
            id
        });
    if used_ids.contains(&global_id) {
        return Err("session global FX control port ID conflicts with a track port".to_owned());
    }
    let global_ports = vec![BackendSessionPort {
        source_id: global_id,
        descriptor: BackendPortDescriptor {
            id: BackendPortId::from_raw(global_id),
            owner: BackendPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Input,
            role: BackendPortRole::MidiInput,
        },
        external_connections: global_document
            .map(|port| port.external_connections)
            .unwrap_or_default(),
    }];
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
            latency_policy: app_track_latency_policy(&track.latency_policy)?,
            ..Default::default()
        };
        let ports = track
            .ports
            .iter()
            .map(|port| BackendSessionPort {
                source_id: port.id,
                descriptor: BackendPortDescriptor {
                    id: BackendPortId::from_raw(port.id),
                    owner: BackendPortOwner::Track,
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
                            latency: backend_take_latency(
                                &channel.latency,
                                bundle.document.sample_rate,
                            )?,
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
                            latency: backend_take_latency(
                                &channel.latency,
                                bundle.document.sample_rate,
                            )?,
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
                        latency: Default::default(),
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
                        latency: Default::default(),
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
            oxisynth_midi_cc_assignments: track
                .fx_chain
                .as_ref()
                .into_iter()
                .flat_map(|chain| chain.midi_cc_assignments.iter().copied())
                .map(backend_oxisynth_midi_cc_assignment)
                .collect(),
        });
    }
    Ok(BackendSessionData {
        sample_rate: bundle.document.sample_rate,
        tracks,
        global_ports,
        use_legacy_browser_default_routes: false,
    })
}

fn document_take_latency(
    latency: &BackendTakeLatencySnapshot,
    session_sample_rate: u32,
) -> TakeLatencyDocument {
    TakeLatencyDocument {
        capture_alignment_frames: i64::from(latency.capture_alignment_frames),
        retained_before_frames: u64::from(latency.retained_before_frames),
        retained_after_frames: u64::from(latency.retained_after_frames),
        observation: LatencyObservationDocument {
            minimum_frames: latency.observation_min_frames.map(u64::from),
            maximum_frames: latency.observation_max_frames.map(u64::from),
            certainty: match latency.certainty {
                BackendLatencyCertainty::Exact => LatencyCertaintyDocument::Exact,
                BackendLatencyCertainty::Range => LatencyCertaintyDocument::Range,
                BackendLatencyCertainty::Estimated => LatencyCertaintyDocument::Estimated,
                BackendLatencyCertainty::ManualOnly => LatencyCertaintyDocument::ManualOnly,
                BackendLatencyCertainty::Unknown => LatencyCertaintyDocument::Unknown,
            },
            sample_rate: if latency.observation_min_frames.is_some()
                && latency.observation_sample_rate == 0
            {
                session_sample_rate
            } else {
                latency.observation_sample_rate
            },
            revision: latency.observation_revision,
        },
        variable_history: latency.variable_history,
        history_revisions: latency.history_revisions,
        changed_during_operation: latency.changed_during_operation,
        incomplete: latency.incomplete,
        applied_during_render: latency.applied_during_render,
    }
}

fn app_backend_take_latency(latency: &BackendTakeLatencySnapshot) -> TakeLatencyProvenanceState {
    TakeLatencyProvenanceState {
        capture_alignment_frames: latency.capture_alignment_frames,
        retained_before_frames: latency.retained_before_frames,
        retained_after_frames: latency.retained_after_frames,
        render_advance_frames: 0,
        certainty: match latency.certainty {
            BackendLatencyCertainty::Exact => shoop_app_api::LatencyCertaintyState::Exact,
            BackendLatencyCertainty::Range => shoop_app_api::LatencyCertaintyState::Range,
            BackendLatencyCertainty::Estimated => shoop_app_api::LatencyCertaintyState::Estimated,
            BackendLatencyCertainty::ManualOnly => shoop_app_api::LatencyCertaintyState::ManualOnly,
            BackendLatencyCertainty::Unknown => shoop_app_api::LatencyCertaintyState::Unknown,
        },
        observation_min_frames: latency.observation_min_frames,
        observation_max_frames: latency.observation_max_frames,
        observation_sample_rate: latency.observation_sample_rate,
        observation_revision: latency.observation_revision,
        variable_history: latency.variable_history,
        history_revisions: latency.history_revisions,
        changed_during_operation: latency.changed_during_operation,
        incomplete: latency.incomplete,
        deferred_mode: None,
        finalizing: false,
        error: None,
    }
}

fn app_take_latency_document(
    latency: &TakeLatencyDocument,
) -> Result<TakeLatencyProvenanceState, String> {
    Ok(TakeLatencyProvenanceState {
        capture_alignment_frames: i32::try_from(latency.capture_alignment_frames)
            .map_err(|_| "take latency alignment exceeds engine range".to_owned())?,
        retained_before_frames: u32::try_from(latency.retained_before_frames)
            .map_err(|_| "retained-before margin exceeds engine range".to_owned())?,
        retained_after_frames: u32::try_from(latency.retained_after_frames)
            .map_err(|_| "retained-after margin exceeds engine range".to_owned())?,
        render_advance_frames: 0,
        certainty: match latency.observation.certainty {
            LatencyCertaintyDocument::Exact => shoop_app_api::LatencyCertaintyState::Exact,
            LatencyCertaintyDocument::Range => shoop_app_api::LatencyCertaintyState::Range,
            LatencyCertaintyDocument::Estimated => shoop_app_api::LatencyCertaintyState::Estimated,
            LatencyCertaintyDocument::ManualOnly => {
                shoop_app_api::LatencyCertaintyState::ManualOnly
            }
            LatencyCertaintyDocument::Unknown => shoop_app_api::LatencyCertaintyState::Unknown,
        },
        observation_min_frames: latency
            .observation
            .minimum_frames
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "take latency minimum exceeds engine range".to_owned())?,
        observation_max_frames: latency
            .observation
            .maximum_frames
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "take latency maximum exceeds engine range".to_owned())?,
        observation_sample_rate: latency.observation.sample_rate,
        observation_revision: latency.observation.revision,
        variable_history: latency.variable_history,
        history_revisions: latency.history_revisions,
        changed_during_operation: latency.changed_during_operation,
        incomplete: latency.incomplete,
        deferred_mode: None,
        finalizing: false,
        error: None,
    })
}

fn apply_manual_import_offset(
    latency: &mut TakeLatencyDocument,
    offset_frames: i32,
    raw_length_frames: u64,
    sample_rate: u32,
) -> Result<(), String> {
    if offset_frames.unsigned_abs() > shoop_latency::MAX_COMPENSATION_FRAMES {
        return Err(format!(
            "manual import offset exceeds the {} frame compensation limit",
            shoop_latency::MAX_COMPENSATION_FRAMES
        ));
    }
    let adjusted = latency
        .capture_alignment_frames
        .checked_add(i64::from(offset_frames))
        .ok_or_else(|| "manual import offset overflows take alignment".to_owned())?;
    if adjusted.unsigned_abs() > u64::from(shoop_latency::MAX_COMPENSATION_FRAMES) {
        return Err(format!(
            "adjusted import offset exceeds the {} frame compensation limit",
            shoop_latency::MAX_COMPENSATION_FRAMES
        ));
    }
    latency.capture_alignment_frames = adjusted;
    if latency.observation == LatencyObservationDocument::default() {
        latency.observation.certainty = LatencyCertaintyDocument::ManualOnly;
        latency.observation.sample_rate = sample_rate;
        latency.observation.revision = 1;
    }
    let _ = raw_length_frames;
    Ok(())
}

fn backend_take_latency(
    latency: &TakeLatencyDocument,
    sample_rate: u32,
) -> Result<BackendTakeLatencySnapshot, String> {
    Ok(BackendTakeLatencySnapshot {
        capture_alignment_frames: i32::try_from(latency.capture_alignment_frames)
            .map_err(|_| "take latency alignment exceeds engine range".to_owned())?,
        retained_before_frames: u32::try_from(latency.retained_before_frames)
            .map_err(|_| "retained-before margin exceeds engine range".to_owned())?,
        retained_after_frames: u32::try_from(latency.retained_after_frames)
            .map_err(|_| "retained-after margin exceeds engine range".to_owned())?,
        observation_min_frames: latency
            .observation
            .minimum_frames
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "take latency minimum exceeds engine range".to_owned())?,
        observation_max_frames: latency
            .observation
            .maximum_frames
            .map(u32::try_from)
            .transpose()
            .map_err(|_| "take latency maximum exceeds engine range".to_owned())?,
        certainty: match latency.observation.certainty {
            LatencyCertaintyDocument::Exact => BackendLatencyCertainty::Exact,
            LatencyCertaintyDocument::Range => BackendLatencyCertainty::Range,
            LatencyCertaintyDocument::Estimated => BackendLatencyCertainty::Estimated,
            LatencyCertaintyDocument::ManualOnly => BackendLatencyCertainty::ManualOnly,
            LatencyCertaintyDocument::Unknown => BackendLatencyCertainty::Unknown,
        },
        observation_sample_rate: if latency.observation.minimum_frames.is_some() {
            sample_rate
        } else {
            latency.observation.sample_rate
        },
        observation_revision: latency.observation.revision,
        variable_history: latency.variable_history,
        history_revisions: latency.history_revisions,
        changed_during_operation: latency.changed_during_operation,
        incomplete: latency.incomplete,
        applied_during_render: latency.applied_during_render,
    })
}

fn backend_track_latency_policy(
    policy: &shoop_app_api::TrackLatencyPolicyState,
    ports: &BTreeMap<PortId, ConnectionPortModel>,
) -> Result<shoop_app_api::TrackLatencyPolicyState, String> {
    let mut translated = policy.clone();
    translated.cue_output = policy
        .cue_output
        .as_ref()
        .map(|selection| match selection {
            CueOutputSelection::ApplicationPort(port_id) => ports
                .get(port_id)
                .map(|port| {
                    CueOutputSelection::ApplicationPort(PortId::from_raw(port.backend_id.raw()))
                })
                .ok_or_else(|| format!("stale cue application port {port_id}")),
            CueOutputSelection::HostPort(host_port_id) => {
                Ok(CueOutputSelection::HostPort(host_port_id.clone()))
            }
        })
        .transpose()?;
    Ok(translated)
}

fn application_track_latency_policy(
    policy: &shoop_app_api::TrackLatencyPolicyState,
    backend_to_application: &BTreeMap<u64, PortId>,
) -> shoop_app_api::TrackLatencyPolicyState {
    let mut translated = policy.clone();
    translated.cue_output = policy
        .cue_output
        .as_ref()
        .and_then(|selection| match selection {
            CueOutputSelection::ApplicationPort(port_id) => backend_to_application
                .get(&port_id.raw())
                .copied()
                .map(CueOutputSelection::ApplicationPort),
            CueOutputSelection::HostPort(host_port_id) => {
                Some(CueOutputSelection::HostPort(host_port_id.clone()))
            }
        });
    translated
}

fn document_track_latency_policy(
    policy: &shoop_app_api::TrackLatencyPolicyState,
) -> TrackLatencyPolicyDocument {
    TrackLatencyPolicyDocument {
        cue_followed: policy.cue_followed,
        cue_output: policy.cue_output.as_ref().map(|selection| match selection {
            CueOutputSelection::ApplicationPort(port_id) => {
                CueOutputSelectionDocument::ApplicationPort {
                    port_id: port_id.raw(),
                }
            }
            CueOutputSelection::HostPort(host_port_id) => CueOutputSelectionDocument::HostPort {
                host_port_id: host_port_id.to_string(),
            },
        }),
        revision: policy.revision,
        components: policy
            .components
            .iter()
            .map(|component| LatencyComponentPolicyDocument {
                component: match component.kind {
                    shoop_app_api::LatencyComponentKind::ExternalCapture => {
                        LatencyComponentDocument::ExternalCapture
                    }
                    shoop_app_api::LatencyComponentKind::Processor => {
                        LatencyComponentDocument::Processor
                    }
                    shoop_app_api::LatencyComponentKind::CuePlayback => {
                        LatencyComponentDocument::CuePlayback
                    }
                    shoop_app_api::LatencyComponentKind::BackendBuffering => {
                        LatencyComponentDocument::BackendBuffering
                    }
                    shoop_app_api::LatencyComponentKind::Manual => LatencyComponentDocument::Manual,
                },
                enabled: component.enabled,
                value: match component.value_mode {
                    shoop_app_api::LatencyValueMode::Automatic => LatencyValueDocument::Automatic,
                    shoop_app_api::LatencyValueMode::Manual(frames) => {
                        LatencyValueDocument::Manual {
                            frames: u64::from(frames),
                        }
                    }
                    shoop_app_api::LatencyValueMode::AutomaticPlusTrim(frames) => {
                        LatencyValueDocument::AutomaticPlusTrim {
                            frames: i64::from(frames),
                        }
                    }
                },
                range_selection: match component.range_selection {
                    LatencyRangeSelectionState::Minimum => LatencyRangeSelectionDocument::Minimum,
                    LatencyRangeSelectionState::Midpoint => LatencyRangeSelectionDocument::Midpoint,
                    LatencyRangeSelectionState::Maximum => LatencyRangeSelectionDocument::Maximum,
                },
            })
            .collect(),
    }
}

fn app_track_latency_policy(
    policy: &TrackLatencyPolicyDocument,
) -> Result<shoop_app_api::TrackLatencyPolicyState, String> {
    Ok(shoop_app_api::TrackLatencyPolicyState {
        cue_followed: policy.cue_followed,
        cue_output: policy.cue_output.as_ref().map(|selection| match selection {
            CueOutputSelectionDocument::ApplicationPort { port_id } => {
                CueOutputSelection::ApplicationPort(PortId::from_raw(*port_id))
            }
            CueOutputSelectionDocument::HostPort { host_port_id } => {
                CueOutputSelection::HostPort(HostPortId::new(host_port_id.clone()))
            }
        }),
        revision: policy.revision,
        pending: false,
        error: None,
        components: policy
            .components
            .iter()
            .map(|component| {
                Ok(shoop_app_api::LatencyComponentPolicyState {
                    kind: match component.component {
                        LatencyComponentDocument::ExternalCapture => {
                            shoop_app_api::LatencyComponentKind::ExternalCapture
                        }
                        LatencyComponentDocument::Processor => {
                            shoop_app_api::LatencyComponentKind::Processor
                        }
                        LatencyComponentDocument::CuePlayback => {
                            shoop_app_api::LatencyComponentKind::CuePlayback
                        }
                        LatencyComponentDocument::BackendBuffering => {
                            shoop_app_api::LatencyComponentKind::BackendBuffering
                        }
                        LatencyComponentDocument::Manual => {
                            shoop_app_api::LatencyComponentKind::Manual
                        }
                    },
                    enabled: component.enabled,
                    value_mode: match component.value {
                        LatencyValueDocument::Automatic => {
                            shoop_app_api::LatencyValueMode::Automatic
                        }
                        LatencyValueDocument::Manual { frames } => {
                            shoop_app_api::LatencyValueMode::Manual(u32::try_from(frames).map_err(
                                |_| "manual latency exceeds application range".to_owned(),
                            )?)
                        }
                        LatencyValueDocument::AutomaticPlusTrim { frames } => {
                            shoop_app_api::LatencyValueMode::AutomaticPlusTrim(
                                i32::try_from(frames).map_err(|_| {
                                    "latency trim exceeds application range".to_owned()
                                })?,
                            )
                        }
                    },
                    range_selection: match component.range_selection {
                        LatencyRangeSelectionDocument::Minimum => {
                            LatencyRangeSelectionState::Minimum
                        }
                        LatencyRangeSelectionDocument::Midpoint => {
                            LatencyRangeSelectionState::Midpoint
                        }
                        LatencyRangeSelectionDocument::Maximum => {
                            LatencyRangeSelectionState::Maximum
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>, String>>()?
            .into(),
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
    kind: TrackPortOwnerKind,
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
                owner: match descriptor.owner {
                    BackendPortOwner::Track => ApplicationPortOwner::Track { track_id, kind },
                    BackendPortOwner::GlobalFxControl => ApplicationPortOwner::GlobalFxControl,
                },
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
                capture_latency: Default::default(),
                playback_latency: Default::default(),
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

fn combined_output_midi_activity(
    port_activity: bool,
    loop_activity: impl IntoIterator<Item = bool>,
) -> bool {
    port_activity || loop_activity.into_iter().any(|active| active)
}

fn display_peaks(peaks: &[f32], stereo: bool) -> (f32, f32) {
    if stereo {
        let left = peaks.first().copied().unwrap_or(-200.0);
        let right = peaks.get(1).copied().unwrap_or(left);
        (left, right)
    } else {
        let peak = peaks.iter().copied().fold(-200.0, f32::max);
        (peak, peak)
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
    #[cfg(not(target_arch = "wasm32"))]
    use std::time::Instant;

    use shoop_app_api::{SelectionModifiers, TrackAction};
    use shoop_backend::{
        BackendPortDataType, BackendPortDirection, EngineBackend, FakeBackend, LocalDummyBackend,
    };

    use super::*;

    fn cooperative_start_with_midi(
        backend: Box<dyn Backend>,
        midi: Box<dyn shoop_scripting::MidiControlService>,
    ) -> Result<CooperativeApplicationRuntime> {
        CooperativeApplicationRuntime::start_with_scripts_and_midi(backend, Vec::new(), midi)
    }

    #[shoop_wasm_test_support::shoop_test]
    fn track_output_midi_activity_includes_port_and_loop_playback() {
        assert!(combined_output_midi_activity(false, [false, true]));
        assert!(combined_output_midi_activity(true, [false, false]));
        assert!(!combined_output_midi_activity(false, [false, false]));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn display_peaks_preserves_stereo_and_uses_the_loudest_other_channel_count() {
        assert_eq!(display_peaks(&[-12.0, -6.0], true), (-12.0, -6.0));
        assert_eq!(display_peaks(&[-18.0, -3.0, -9.0], false), (-3.0, -3.0));
        assert_eq!(display_peaks(&[], false), (-200.0, -200.0));
    }

    #[cfg(not(target_arch = "wasm32"))]
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

    #[shoop_wasm_test_support::shoop_test]
    fn desired_control_helpers_cover_every_control_shape() {
        let mut backend_track = BackendTrackState::default();
        let mut controls = TrackControlState::default();
        for control in [
            BackendTrackControl::OutputGainDb(-3.0),
            BackendTrackControl::OutputBalance(0.25),
            BackendTrackControl::OutputMute(true),
            BackendTrackControl::InputGainDb(-6.0),
            BackendTrackControl::InputBalance(-0.5),
            BackendTrackControl::InputMonitoring(true),
        ] {
            let _ = track_control_key(control);
            assert!(!track_control_matches(&backend_track, control));
            apply_track_control(&mut controls, control);
            match control {
                BackendTrackControl::OutputGainDb(value) => backend_track.output_gain_db = value,
                BackendTrackControl::OutputBalance(value) => backend_track.output_balance = value,
                BackendTrackControl::OutputMute(value) => backend_track.output_muted = value,
                BackendTrackControl::InputGainDb(value) => backend_track.input_gain_db = value,
                BackendTrackControl::InputBalance(value) => backend_track.input_balance = value,
                BackendTrackControl::InputMonitoring(value) => {
                    backend_track.input_monitoring = value
                }
            }
            assert!(track_control_matches(&backend_track, control));
        }

        let mut oxisynth = shoop_app_api::TrackFxState {
            processor_type: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::OXISYNTH,
            ),
            active: true,
            visible: true,
            lifecycle: shoop_app_api::FxLifecycle::Running,
            generation: 0,
            crash_summary: None,
            logs: Arc::from([]),
            latency: Default::default(),
            latency_provider: Default::default(),
            editor: Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(
                shoop_app_api::OxiSynthState {
                    selected_preset_id: "0:0".to_owned(),
                    reverb_send: 0.0,
                    chorus_send: 0.0,
                    midi_cc_assignments: Arc::from([]),
                },
            )),
        };
        let select = BackendTrackFxControl::OxiSynth(shoop_app_api::OxiSynthControl::SelectPreset(
            "0:40".to_owned(),
        ));
        assert_eq!(fx_control_key(&select), Some(FxControlKey::OxiPreset));
        assert!(!fx_control_matches(Some(&oxisynth), &select));
        apply_fx_control(&mut oxisynth, &select);
        assert!(fx_control_matches(Some(&oxisynth), &select));
        assert_eq!(
            fx_control_key(&BackendTrackFxControl::OxiSynth(
                shoop_app_api::OxiSynthControl::Panic,
            )),
            None
        );

        let progress = |completed, total| BackendOperationProgress {
            key: 1,
            kind: shoop_backend::BackendOperationKind::SessionCapture,
            completed,
            total,
        };
        assert_eq!(backend_progress_fraction(progress(1, None)), 0.0);
        assert_eq!(backend_progress_fraction(progress(1, Some(0))), 0.0);
        assert_eq!(backend_progress_fraction(progress(1, Some(2))), 0.5);
        assert_eq!(backend_progress_fraction(progress(3, Some(2))), 1.0);
        let error = BackendIoStepError::from("typed failure".to_owned());
        assert!(matches!(error, BackendIoStepError::Failed(message) if message == "typed failure"));
    }

    fn engine_model_with_regular_composite() -> (
        LocalDummyBackend,
        ApplicationModel,
        TrackId,
        LoopId,
        [LoopId; 3],
    ) {
        let mut backend = EngineBackend::new_dummy(1_000, 1).unwrap();
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
                    name: "Composite".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let track_id = model.tracks[1].id;
        let target = model.tracks[1].loops[0];
        let sources = [
            model.tracks[1].loops[1],
            model.tracks[1].loops[2],
            model.tracks[1].loops[3],
        ];
        for source in sources {
            backend
                .set_loop_length(model.loops[&source].backend_id, 4)
                .unwrap();
        }
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .handle_loop_action(
                &mut backend,
                track_id,
                target,
                LoopAction::ConvertToComposite,
            )
            .unwrap();
        for source in sources {
            model
                .compose_loop_serial(&mut backend, target, source)
                .unwrap();
        }
        (backend, model, track_id, target, sources)
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn new_session_replaces_existing_tracks_and_restores_sync_track() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Recorded track".to_owned(),
                audio_channels: 2,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().tracks.len() > 1);

        runtime.dispatch(AppIntent::RequestNewSession).unwrap();
        runtime.tick(Duration::ZERO);

        let snapshot = runtime.snapshot();
        assert_eq!(snapshot.tracks.len(), 1);
        assert!(snapshot.tracks[0].is_sync);
        assert_eq!(snapshot.tracks[0].loops.len(), 1);
        assert!(snapshot.tracks[0].loops[0].sync);
        assert_eq!(
            snapshot.io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn removing_and_recreating_a_track_reuses_its_port_names_without_stale_resources() {
        let mut backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        let spec = DirectTrackSpec {
            name: "Reusable Track".to_owned(),
            audio_channels: 2,
            midi: true,
        };
        model.add_track(&mut backend, spec.clone()).unwrap();
        let track = model.tracks.last().unwrap();
        let track_id = track.id;
        let backend_track_id = track.backend_id;
        let port_name_base = track.port_name_base.clone();
        let old_loop_ids = track
            .loops
            .iter()
            .map(|id| model.loops[id].backend_id)
            .collect::<Vec<_>>();
        let old_port_ids = track
            .port_ids
            .iter()
            .map(|id| model.connection_ports[id].backend_id)
            .collect::<Vec<_>>();
        let before = backend.poll().unwrap();
        let old_port_names = old_port_ids
            .iter()
            .map(|id| before.connections.application_ports[id].name.clone())
            .collect::<Vec<_>>();

        model
            .handle_track_action(&mut backend, track_id, TrackAction::Remove)
            .unwrap();
        let removed = backend.poll().unwrap();
        assert!(!removed.tracks.contains_key(&backend_track_id));
        assert!(old_loop_ids
            .iter()
            .all(|loop_id| !removed.loops.contains_key(loop_id)));
        assert!(old_port_ids
            .iter()
            .all(|port_id| !removed.connections.application_ports.contains_key(port_id)));
        assert_eq!(
            model
                .tracks
                .iter()
                .find(|track| track.id == track_id)
                .map(|track| track.structural_state),
            Some(StructuralState::Removing)
        );
        model.apply_backend_snapshot(removed);
        assert!(!model.tracks.iter().any(|track| track.id == track_id));

        model.add_track(&mut backend, spec).unwrap();
        let recreated = model.tracks.last().unwrap();
        assert_eq!(recreated.port_name_base, port_name_base);
        let new_port_ids = recreated
            .port_ids
            .iter()
            .map(|id| model.connection_ports[id].backend_id)
            .collect::<Vec<_>>();
        assert!(new_port_ids
            .iter()
            .all(|port_id| !old_port_ids.contains(port_id)));
        let snapshot = backend.poll().unwrap();
        let new_port_names = new_port_ids
            .iter()
            .map(|id| snapshot.connections.application_ports[id].name.clone())
            .collect::<Vec<_>>();
        assert_eq!(new_port_names, old_port_names);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn loop_name_change_is_published() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let initial = runtime.handle().snapshot();
        let track_id = initial.tracks[0].id;
        let loop_id = initial.tracks[0].loops[0].id;
        runtime
            .handle()
            .dispatch(AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::NameChanged("Count-in".to_owned()),
            })
            .unwrap();
        let snapshot = wait_for(&runtime.handle(), |snapshot| {
            snapshot.tracks[0].loops[0].name == "Count-in"
        });
        assert_eq!(snapshot.tracks[0].loops[0].name, "Count-in");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_and_track_move_actions_reorder_by_stable_insertion_target() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        for index in 1..=3 {
            model
                .add_track(
                    &mut backend,
                    DirectTrackSpec {
                        name: format!("Track {index}"),
                        audio_channels: 1,
                        midi: false,
                    },
                )
                .unwrap();
        }

        let sync = model.tracks[0].id;
        let first_track = model.tracks[1].id;
        let second_track = model.tracks[2].id;
        let third_track = model.tracks[3].id;
        model
            .handle_track_action(
                &mut backend,
                first_track,
                TrackAction::MoveBefore(Some(third_track)),
            )
            .unwrap();
        assert_eq!(
            model
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            [sync, second_track, first_track, third_track]
        );
        model
            .handle_track_action(&mut backend, first_track, TrackAction::MoveBefore(None))
            .unwrap();
        assert_eq!(
            model
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            [sync, second_track, third_track, first_track]
        );

        let track_index = model
            .tracks
            .iter()
            .position(|track| track.id == second_track)
            .unwrap();
        let original = model.tracks[track_index].loops.clone();
        model
            .handle_loop_action(
                &mut backend,
                second_track,
                original[0],
                LoopAction::MoveBefore(Some(original[2])),
            )
            .unwrap();
        let mut expected = original.clone();
        let moved = expected.remove(0);
        expected.insert(1, moved);
        assert_eq!(model.tracks[track_index].loops, expected);
        model
            .handle_loop_action(
                &mut backend,
                second_track,
                original[0],
                LoopAction::MoveBefore(None),
            )
            .unwrap();
        let mut expected = original[1..].to_vec();
        expected.push(original[0]);
        assert_eq!(model.tracks[track_index].loops, expected);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn duplicate_uses_the_first_empty_slot_below_and_drop_actions_target_and_swap() {
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
                    midi: true,
                },
            )
            .unwrap();
        let track_id = model.tracks[1].id;
        let source = model.tracks[1].loops[1];
        let occupied = model.tracks[1].loops[2];
        let duplicate_target = model.tracks[1].loops[3];
        let source_backend = model.loops[&source].backend_id;
        let occupied_backend = model.loops[&occupied].backend_id;
        let duplicate_backend = model.loops[&duplicate_target].backend_id;
        let source_update = BackendLoopContentUpdate {
            audio: vec![BackendAudioChannelUpdate {
                channel: 0,
                samples: vec![0.25, -0.5, 0.75, -1.0],
                start_offset: Some(-2),
                preplay: Some(3),
                latency: Some(BackendTakeLatencySnapshot {
                    capture_alignment_frames: 2,
                    observation_min_frames: Some(2),
                    observation_max_frames: Some(2),
                    certainty: BackendLatencyCertainty::Exact,
                    observation_sample_rate: 48_000,
                    observation_revision: 5,
                    ..Default::default()
                }),
            }],
            midi: vec![BackendMidiChannelUpdate {
                channel: 0,
                length: 4,
                start_state: vec![vec![0xB0, 7, 99]],
                events: vec![BackendMidiEvent {
                    time: 2,
                    data: vec![0x90, 64, 100],
                }],
                start_offset: Some(-1),
                preplay: Some(2),
                latency: Some(BackendTakeLatencySnapshot {
                    capture_alignment_frames: 1,
                    observation_min_frames: Some(1),
                    observation_max_frames: Some(1),
                    certainty: BackendLatencyCertainty::Exact,
                    observation_sample_rate: 48_000,
                    observation_revision: 6,
                    ..Default::default()
                }),
            }],
            length: Some(4),
        };
        backend
            .replace_loop_content(source_backend, &source_update)
            .unwrap();
        backend.set_loop_gain(source_backend, 0.42).unwrap();
        backend.set_loop_balance(source_backend, -0.25).unwrap();
        backend
            .replace_loop_content(
                occupied_backend,
                &BackendLoopContentUpdate {
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![1.0, 1.0],
                        start_offset: Some(0),
                        preplay: Some(0),
                        latency: None,
                    }],
                    midi: Vec::new(),
                    length: Some(2),
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .handle_loop_action(
                &mut backend,
                track_id,
                source,
                LoopAction::NameChanged("Source".to_owned()),
            )
            .unwrap();

        backend.delay_next_async_loop_copy();
        model
            .handle_loop_action(&mut backend, track_id, source, LoopAction::Duplicate)
            .unwrap();
        model.advance_io(&mut backend);
        model.advance_io(&mut backend);
        assert!(model.loops[&model.tracks[1].loops[0]].state.empty);
        assert_eq!(model.loops[&occupied].length, 2);
        assert_eq!(model.loops[&duplicate_target].name, "Source");
        assert_eq!(model.loops[&duplicate_target].length, 4);
        let captured = backend.capture_session().unwrap();
        let track = captured
            .tracks
            .iter()
            .find(|track| {
                track
                    .loops
                    .iter()
                    .any(|loop_| loop_.source_id == source_backend.raw())
            })
            .unwrap();
        let source_content = track
            .loops
            .iter()
            .find(|loop_| loop_.source_id == source_backend.raw())
            .unwrap();
        let duplicate_content = track
            .loops
            .iter()
            .find(|loop_| loop_.source_id == duplicate_backend.raw())
            .unwrap();
        assert_eq!(duplicate_content.length, source_content.length);
        assert_eq!(duplicate_content.gain, source_content.gain);
        assert_eq!(duplicate_content.balance, source_content.balance);
        assert_eq!(duplicate_content.audio, source_content.audio);
        assert_eq!(duplicate_content.midi, source_content.midi);

        model
            .handle_loop_action(
                &mut backend,
                track_id,
                source,
                LoopAction::DuplicateTo(occupied),
            )
            .unwrap();
        assert_eq!(model.loops[&occupied].name, "Source");
        assert_eq!(model.loops[&occupied].length, 4);

        let before = model.tracks[1].loops.clone();
        model
            .handle_loop_action(
                &mut backend,
                track_id,
                source,
                LoopAction::SwapWith(duplicate_target),
            )
            .unwrap();
        assert_eq!(model.tracks[1].loops[1], duplicate_target);
        assert_eq!(model.tracks[1].loops[3], source);
        assert_eq!(model.tracks[1].loops[0], before[0]);
        assert_eq!(model.tracks[1].loops[2], before[2]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn application_publishes_backend_processor_capabilities() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new("future_browser_fx"),
            label: "Future browser FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: None,
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn application_adds_external_dry_wet_tracks_from_processor_capabilities() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::EXTERNAL,
            ),
            label: "External".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: None,
                min_wet_audio_channels: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_session_round_trip_preserves_selected_preset() {
        let backend = shoop_backend::EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithTopology(TrackSpec {
                name: "OxiSynth".to_owned(),
                topology: TrackSpecTopology::DryWet {
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                    processor_type: shoop_app_api::TrackProcessorTypeId::new(
                        shoop_app_api::TrackProcessorTypeId::OXISYNTH,
                    ),
                },
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = runtime.snapshot().tracks[1].clone();
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) =
            track.fx.as_ref().and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:0");
        runtime
            .dispatch(AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OxiSynth(shoop_app_api::OxiSynthControl::SelectPreset(
                    "0:40".to_owned(),
                )),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OxiSynth(shoop_app_api::OxiSynthControl::SelectPreset(
                    "0:41".to_owned(),
                )),
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OxiSynth(shoop_app_api::OxiSynthControl::SelectPreset(
                    "0:40".to_owned(),
                )),
            })
            .unwrap();
        for control in [
            shoop_app_api::OxiSynthControl::SetReverbSend(0.25),
            shoop_app_api::OxiSynthControl::SetChorusSend(0.5),
            shoop_app_api::OxiSynthControl::AssignMidiCc(shoop_app_api::OxiSynthMidiCcAssignment {
                parameter: shoop_app_api::OxiSynthParameter::ReverbSend,
                channel: 3,
                controller: 74,
            }),
        ] {
            runtime
                .dispatch(AppIntent::Track {
                    track_id: track.id,
                    action: TrackAction::OxiSynth(control),
                })
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let selected = runtime.snapshot();
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) = selected.tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing selected OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(editor.reverb_send, 0.25);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(editor.midi_cc_assignments.len(), 1);
        runtime
            .dispatch(AppIntent::Track {
                track_id: track.id,
                action: TrackAction::OxiSynth(shoop_app_api::OxiSynthControl::SelectPreset(
                    "1:0".to_owned(),
                )),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let rejected = runtime.snapshot();
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) = rejected.tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing OxiSynth editor state after rejection");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action: LoopAction::RecordClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(!runtime.snapshot().tracks[1].loops[0].has_recorded_fx_state);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track.id,
                loop_id: track.loops[0].id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..3 {
            runtime.tick(Duration::ZERO);
        }
        let output = runtime.take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        let saved_track = &saved.document.track_groups[1].tracks[0];
        assert_eq!(saved_track.topology, TrackTopologyDocument::OxiSynth);
        let chain = saved_track.fx_chain.as_ref().unwrap();
        assert_eq!(chain.chain_type, FxChainTypeDocument::OxiSynth);
        assert_eq!(
            chain.internal_state,
            "shoop-oxisynth:2:timgm6mb:0:40:3e800000:3f000000"
        );
        assert_eq!(
            chain.midi_cc_assignments,
            [OxiSynthMidiCcAssignmentDocument {
                parameter: OxiSynthParameterDocument::ReverbSend,
                channel: 3,
                controller: 74,
            }]
        );
        assert!(saved
            .document
            .fx_states
            .iter()
            .all(|state| state.id != chain.id));

        let mut malformed = saved.clone();
        malformed.document.track_groups[1].tracks[0]
            .fx_chain
            .as_mut()
            .unwrap()
            .internal_state = "malformed".to_owned();
        let malformed = encode_session(&malformed, "malformed-oxisynth").unwrap();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "malformed-oxisynth.shoop".to_owned(),
                bytes: malformed.into(),
            })
            .unwrap();
        for _ in 0..4 {
            runtime.tick(Duration::ZERO);
        }
        let after_failed_load = runtime.snapshot();
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) = after_failed_load
            .tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("OxiSynth state disappeared after failed load");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(editor.reverb_send, 0.25);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(editor.midi_cc_assignments.len(), 1);

        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "oxisynth.shoop".to_owned(),
                bytes: output.bytes,
            })
            .unwrap();
        for _ in 0..4 {
            runtime.tick(Duration::ZERO);
        }
        let loaded = runtime.snapshot();
        assert_eq!(loaded.tracks[1].topology, track.topology);
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) = loaded.tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing loaded OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(editor.reverb_send, 0.25);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(editor.midi_cc_assignments.len(), 1);

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
        let Some(shoop_app_api::TrackProcessorEditorState::OxiSynth(editor)) = switched.tracks[1]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing OxiSynth state after sample-rate switch");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(editor.reverb_send, 0.25);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(editor.midi_cc_assignments.len(), 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processed_track_session_round_trip_preserves_roles_state_and_recorded_take() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
            ),
            label: "Carla Rack".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: None,
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
        assert!(runtime.snapshot().tracks[1].loops[0].has_recorded_fx_state);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn failed_recorded_fx_restore_leaves_the_processed_track_usable() {
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new(
                shoop_app_api::TrackProcessorTypeId::CARLA_RACK,
            ),
            label: "Carla Rack".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(2),
                min_wet_audio_channels: None,
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
        assert!(runtime.snapshot().io_task.is_none());
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
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn actor_starts_embedded_production_keyboard_script_without_checkout_files() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                identity: None,
                source: TEST_KEYBOARD_SCRIPT.to_owned(),
                source_path: None,
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn startup_script_ids_preserve_source_order_across_rejection_and_duplicate_names() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    identity: None,
                    source: "local =".to_owned(),
                    source_path: None,
                    kind: ScriptKind::User,
                    enabled: true,
                },
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    identity: None,
                    source: "print('second')".to_owned(),
                    source_path: None,
                    kind: ScriptKind::User,
                    enabled: true,
                },
                StartupScript {
                    name: "duplicate.lua".to_owned(),
                    identity: None,
                    source: "print('third')".to_owned(),
                    source_path: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn cooperative_startup_runs_embedded_keyboard_on_the_application_owner() {
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                identity: None,
                source: TEST_KEYBOARD_SCRIPT.to_owned(),
                source_path: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn lua_control_ports_are_owner_managed_stable_and_visible_without_midi_hosts() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 2).unwrap();
        let mut runtime = cooperative_start_with_midi(
            Box::new(backend),
            Box::new(shoop_scripting::NullMidiService),
        )
        .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(TEST_APC_MINI_SCRIPT),
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

    #[shoop_wasm_test_support::shoop_test]
    fn midi_callback_can_request_script_dialog_opening() {
        let (midi, midi_control) = shoop_scripting::FakeMidiService::new();
        midi_control.set_endpoints(vec![shoop_scripting::MidiEndpoint {
            id: "controller-source".to_owned(),
            name: "Controller".to_owned(),
            direction: shoop_scripting::MidiEndpointDirection::Output,
        }]);
        let mut runtime =
            cooperative_start_with_midi(Box::new(FakeBackend::default()), Box::new(midi)).unwrap();
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

    #[shoop_wasm_test_support::shoop_test]
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
        let mut runtime = cooperative_start_with_midi(Box::new(backend), Box::new(midi)).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(TEST_APC_MINI_SCRIPT),
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn threaded_actor_routes_script_dialog_callbacks_and_teardown() {
        let runtime = ApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "actor-dialog.lua".to_owned(),
                identity: None,
                source: r#"
shoop_announce_api_version(1, 0)
local c=require('shoop_control')
local d=require('shoop_dialog')
d.simple('Actor dialog',{d.button('Apply',function() c.set_solo(true) end)})
d.open('Actor dialog')
"#
                .to_owned(),
                source_path: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn incompatible_script_version_is_published_without_side_effects() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "future.lua".to_owned(),
                source: Arc::from(
                    "shoop_announce_api_version(1, 5); __shoop_control.set_solo(true)",
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
            shoop_app_api::ScriptLifecycle::Incompatible
        );
        let error = snapshot.scripting.scripts[0]
            .latest_error
            .as_deref()
            .unwrap();
        assert!(error.contains("script requests 1.5, host supports 1.4"));
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        assert_eq!(runtime.snapshot().scripting.dialogs.len(), 3);

        runtime
            .dispatch(AppIntent::StopScript { script_id: owner })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let stopped = runtime.snapshot();
        assert_eq!(stopped.scripting.dialogs.len(), 1);
        assert_eq!(stopped.scripting.dialogs[0].owner_script_name, "other.lua");
    }

    #[shoop_wasm_test_support::shoop_test]
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
                source: Arc::from(TEST_KEYBOARD_SCRIPT),
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
        press(&mut runtime, 73, 0);
        assert!(runtime.snapshot().tracks[1].controls.input_monitoring);
        press(&mut runtime, 16_777_236, 0);
        assert!(runtime.snapshot().tracks[2].loops[0].selected);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetAutoMuteOtherTrackInputs(true),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        press(&mut runtime, 73, 0);
        assert!(!runtime.snapshot().tracks[1].controls.input_monitoring);
        assert!(runtime.snapshot().tracks[2].controls.input_monitoring);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetAutoMuteOtherTrackInputs(false),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
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
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 1);
        press(&mut runtime, 50, 0);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 12);
        key(&mut runtime, 49, 0, KeyEventType::Released);
        key(&mut runtime, 50, 0, KeyEventType::Released);
        press(&mut runtime, 51, 0);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 3);
        key(&mut runtime, 51, 0, KeyEventType::Released);
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
        press(&mut runtime, 16_777_216, 0);
        press(&mut runtime, 82, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        press(&mut runtime, 80, 0);
        press(&mut runtime, 16_777_216, 0);
        press(&mut runtime, 80, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        press(&mut runtime, 76, 0);
        press(&mut runtime, 16_777_216, 0);
        press(&mut runtime, 76, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
        press(&mut runtime, 77, 0);
        press(&mut runtime, 16_777_216, 0);
        press(&mut runtime, 77, 0);
        assert!(runtime.snapshot().tracks[1].loops[0].selected);
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
        let after_grab = runtime.snapshot();
        assert!(after_grab.tracks[1].loops[0].empty);
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
        press(&mut runtime, 16_777_216, 0);
        press(&mut runtime, 78, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[3].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 79, 0);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[3].mode,
            LoopMode::Playing
        );
        assert_eq!(
            runtime.snapshot().tracks[1].loops[2].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 16_777_236, 0);
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

    #[shoop_wasm_test_support::shoop_test]
    fn production_keyboard_selects_sync_fallback_and_ctrl_momentary_toggle() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "keyboard.lua".to_owned(),
                source: Arc::from(TEST_KEYBOARD_SCRIPT),
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
        key(&mut runtime, 16_777_236, 0, KeyEventType::Pressed);
        assert!(runtime.snapshot().tracks[0].loops[0].selected);

        key(&mut runtime, 16_777_249, 67_108_864, KeyEventType::Pressed);
        assert!(runtime.snapshot().global_controls.sync);
        key(&mut runtime, 16_777_249, 0, KeyEventType::Released);
        assert!(!runtime.snapshot().global_controls.sync);
        assert!(runtime.snapshot().scripting.scripts[0]
            .latest_error
            .is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn production_keyboard_default_action_covers_stopped_cancel_and_grab_policy() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Track".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "keyboard.lua".to_owned(),
                source: Arc::from(TEST_KEYBOARD_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);

        let press = |runtime: &mut CooperativeApplicationRuntime, key: i64| {
            runtime
                .dispatch(AppIntent::KeyEvent(KeyEvent {
                    event_type: KeyEventType::Pressed,
                    key,
                    modifiers: 0,
                }))
                .unwrap();
            runtime.tick(Duration::ZERO);
            runtime.tick(Duration::ZERO);
        };
        press(&mut runtime, 16_777_236);
        let loop_id = runtime.model.tracks[1].loops[0];
        let backend_id = runtime.model.loops[&loop_id].backend_id;
        runtime.backend.set_loop_length(backend_id, 100).unwrap();
        runtime.tick(Duration::ZERO);

        press(&mut runtime, 32);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        press(&mut runtime, 32);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        press(&mut runtime, 46);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Playing
        );
        runtime
            .dispatch(AppIntent::KeyEvent(KeyEvent {
                event_type: KeyEventType::Released,
                key: 46,
                modifiers: 0,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );

        runtime
            .backend
            .transition_loop(backend_id, BackendLoopMode::Recording, Some(2))
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].next_mode,
            LoopMode::Recording
        );
        press(&mut runtime, 32);
        assert_ne!(
            runtime.snapshot().tracks[1].loops[0].next_mode,
            LoopMode::Recording
        );

        press(&mut runtime, 87);
        assert!(!runtime.snapshot().tracks[1].loops[0].targeted);
        press(&mut runtime, 16_777_216);
        press(&mut runtime, 87);
        assert_ne!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Recording
        );
        press(&mut runtime, 16_777_236);
        press(&mut runtime, 67);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetDefaultRecordingAction(
                    shoop_app_api::DefaultRecordingAction::Grab,
                ),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        press(&mut runtime, 32);
        assert!(runtime.snapshot().tracks[1].loops[0].empty);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_snapshot_keeps_child_and_composite_transition_state_independent() {
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
        assert_eq!(composite.mode, LoopMode::Stopped);
        assert_eq!(composite.next_mode, None);
        assert_eq!(composite.next_mode_delay, None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn production_keyboard_plays_manual_recording_on_next_sync_cycle() {
        let backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(backend),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                identity: None,
                source: TEST_KEYBOARD_SCRIPT.to_owned(),
                source_path: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn composite_details_preserve_qml_schedule_semantics_and_canonical_session_data() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        for name in ["Target", "Rhythm", "Melody"] {
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
        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 50;
        let target = model.tracks[1].loops[0];
        let rhythm_a = model.tracks[2].loops[0];
        let rhythm_b = model.tracks[2].loops[1];
        let melody = model.tracks[3].loops[0];
        model.loops.get_mut(&rhythm_a).unwrap().length = 100;
        model.loops.get_mut(&rhythm_b).unwrap().length = 40;
        model.loops.get_mut(&melody).unwrap().length = 60;
        let composite = CompositeDocument {
            kind: CompositeKindDocument::Script,
            instances: vec![
                CompositeLoopInstanceDocument {
                    instance_id: 1,
                    start_cycle: 1,
                    loop_id: rhythm_a.raw(),
                    mode: Some("playing".to_owned()),
                    n_cycles: None,
                },
                CompositeLoopInstanceDocument {
                    instance_id: 2,
                    start_cycle: 2,
                    loop_id: rhythm_b.raw(),
                    mode: Some("recording".to_owned()),
                    n_cycles: Some(2),
                },
                CompositeLoopInstanceDocument {
                    instance_id: 3,
                    start_cycle: 5,
                    loop_id: melody.raw(),
                    mode: Some("playing_dry_through_wet".to_owned()),
                    n_cycles: None,
                },
                CompositeLoopInstanceDocument {
                    instance_id: 4,
                    start_cycle: 3,
                    loop_id: rhythm_b.raw(),
                    mode: Some("playing".to_owned()),
                    n_cycles: None,
                },
            ],
        };
        {
            let target = model.loops.get_mut(&target).unwrap();
            target.state.selected = true;
            target.state.empty = false;
            target.state.composite_kind = shoop_app_api::CompositeKind::Script;
            target.script_composition = vec![vec![rhythm_a, rhythm_b], vec![melody]];
            target.composite = Some(composite.clone());
        }

        let details = model.details_snapshot().unwrap();
        assert!(!details.loading);
        assert!(!details.midi_loading);
        assert!(details.channels.is_empty());
        assert!(details.midi_channels.is_empty());
        let details = details.composite.unwrap();
        assert_eq!(details.kind, shoop_app_api::CompositeKind::Script);
        let snapshot = model.snapshot();
        let loop_state = |id| {
            snapshot
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .find(|loop_| loop_.id == id)
                .unwrap()
        };
        assert_eq!(
            loop_state(rhythm_a).selected_composite_kind,
            shoop_app_api::CompositeKind::Script
        );
        assert_eq!(
            loop_state(rhythm_b).selected_composite_kind,
            shoop_app_api::CompositeKind::Script
        );
        assert_eq!(
            loop_state(melody).selected_composite_kind,
            shoop_app_api::CompositeKind::Script
        );
        assert_eq!(
            loop_state(target).selected_composite_kind,
            shoop_app_api::CompositeKind::None
        );
        assert_eq!(details.cycle_length_frames, 50);
        assert_eq!(
            details
                .tracks
                .iter()
                .map(|track| track.name.as_str())
                .collect::<Vec<_>>(),
            ["Target", "Rhythm", "Melody"]
        );
        assert_eq!(
            details
                .events
                .iter()
                .map(|event| (
                    event.loop_id,
                    event.start_frame,
                    event.end_frame,
                    event.instance_id,
                ))
                .collect::<Vec<_>>(),
            [
                (rhythm_a, 50, 150, 1),
                (rhythm_b, 100, 200, 2),
                (melody, 250, 350, 3),
                (rhythm_b, 150, 200, 4),
            ]
        );
        assert_eq!(details.timeline_length_frames, 350);
        assert_eq!(details.events[1].forced_n_cycles, Some(2));
        assert_eq!(details.events[1].mode.as_deref(), Some("recording"));

        let before_force = model.loops[&target].composite.clone().unwrap();
        model
            .loops
            .get_mut(&target)
            .unwrap()
            .composite
            .as_mut()
            .unwrap()
            .instances
            .push(CompositeLoopInstanceDocument {
                instance_id: 5,
                start_cycle: 0,
                loop_id: rhythm_a.raw(),
                mode: Some("playing".to_owned()),
                n_cycles: None,
            });
        model
            .set_composite_loop_cycles(
                &mut backend,
                target,
                CompositeEventId { instance_id: 1 },
                Some(4),
            )
            .unwrap();
        assert_eq!(
            model.loops[&target].composite.as_ref().unwrap().instances[0].n_cycles,
            Some(4)
        );
        assert_eq!(
            model.loops[&target].composite.as_ref().unwrap().instances[4].n_cycles,
            None
        );
        assert_eq!(model.loops[&target].length, 350);
        model
            .set_composite_loop_cycles(
                &mut backend,
                target,
                CompositeEventId { instance_id: 1 },
                None,
            )
            .unwrap();
        assert_eq!(
            model.loops[&target].composite.as_ref().unwrap().instances[0].n_cycles,
            None
        );
        assert_eq!(
            model.loops[&target].composite.as_ref().unwrap().instances[4].n_cycles,
            None
        );
        assert_eq!(model.loops[&target].length, 350);
        assert!(model
            .set_composite_loop_cycles(
                &mut backend,
                target,
                CompositeEventId { instance_id: 1 },
                Some(0),
            )
            .unwrap_err()
            .contains("at least one cycle"));
        model.loops.get_mut(&target).unwrap().composite = Some(before_force);

        let capture = backend.capture_session().unwrap();
        let saved = model.session_bundle_from_backend(&capture).unwrap();
        let saved_composite = saved
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == target.raw())
            .unwrap()
            .composite
            .as_ref()
            .unwrap();
        assert_eq!(saved_composite, &composite);

        model
            .apply_script_operation(
                &mut backend,
                ControlOperation::ComposeAddToEnd {
                    target,
                    add: vec![melody],
                    parallel: false,
                },
            )
            .unwrap();
        let updated = model.loops[&target].composite.as_ref().unwrap();
        assert_eq!(updated.kind, CompositeKindDocument::Regular);
        assert_eq!(updated.instances.len(), 5);
        assert_eq!(updated.instances[0].start_cycle, 1);
        assert_eq!(updated.instances[3].start_cycle, 3);
        assert_eq!(updated.instances.last().unwrap().loop_id, melody.raw());
        assert!(updated.instances.iter().all(|event| event.mode.is_none()));

        model.loops.get_mut(&target).unwrap().composite = Some(CompositeDocument {
            kind: CompositeKindDocument::Regular,
            instances: Vec::new(),
        });
        let empty = model.details_snapshot().unwrap().composite.unwrap();
        assert_eq!(empty.kind, shoop_app_api::CompositeKind::Regular);
        assert!(empty.events.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn compose_into_actions_convert_the_target_and_schedule_serial_or_parallel() {
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
        let track = model.tracks[1].id;
        let target = model.tracks[1].loops[0];
        let source = model.tracks[1].loops[1];
        let parallel_source = model.tracks[1].loops[2];
        let end_source = model.tracks[1].loops[3];

        model
            .handle_loop_action(
                &mut backend,
                track,
                source,
                LoopAction::ComposeIntoEnd(target),
            )
            .unwrap();
        let target_model = &model.loops[&target];
        assert_eq!(
            target_model.state.composite_kind,
            shoop_app_api::CompositeKind::Regular
        );
        assert_eq!(target_model.script_composition, [vec![source]]);
        assert_eq!(
            target_model.composite.as_ref().unwrap().instances[0].loop_id,
            source.raw()
        );

        model
            .handle_loop_action(
                &mut backend,
                track,
                parallel_source,
                LoopAction::ComposeIntoParallel(target),
            )
            .unwrap();
        let target_model = &model.loops[&target];
        assert_eq!(
            target_model.script_composition,
            [vec![source, parallel_source]]
        );
        assert_eq!(
            target_model
                .composite
                .as_ref()
                .unwrap()
                .instances
                .iter()
                .map(|event| (event.loop_id, event.start_cycle))
                .collect::<Vec<_>>(),
            [(source.raw(), 0), (parallel_source.raw(), 0)]
        );

        model
            .handle_loop_action(
                &mut backend,
                track,
                end_source,
                LoopAction::ComposeIntoEnd(target),
            )
            .unwrap();
        let target_model = &model.loops[&target];
        assert_eq!(
            target_model.script_composition,
            [vec![source, parallel_source], vec![end_source]]
        );
        assert_eq!(
            target_model
                .composite
                .as_ref()
                .unwrap()
                .instances
                .iter()
                .map(|event| event.loop_id)
                .collect::<Vec<_>>(),
            [source.raw(), parallel_source.raw(), end_source.raw()]
        );

        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Other track".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let cross_track_source = model.tracks[2].loops[0];
        model
            .handle_loop_action(
                &mut backend,
                track,
                cross_track_source,
                LoopAction::ComposeIntoEnd(target),
            )
            .unwrap();
        assert_eq!(
            model.loops[&target].script_composition,
            [
                vec![source, parallel_source],
                vec![end_source],
                vec![cross_track_source]
            ]
        );

        let sync_track = model.tracks[0].id;
        let sync_loop = model.tracks[0].loops[0];
        let error = model
            .handle_loop_action(
                &mut backend,
                sync_track,
                cross_track_source,
                LoopAction::ComposeIntoEnd(sync_loop),
            )
            .unwrap_err();
        assert!(error.contains("sync loop cannot be a composition target"));
        assert!(model.loops[&sync_loop].composite.is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn gui_conversion_and_serial_composition_are_authoritative_and_cycle_safe() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        for name in ["Target", "Source", "Nested"] {
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
        let target_track = model.tracks[1].id;
        let target = model.tracks[1].loops[0];
        let source = model.tracks[2].loops[0];
        let nested = model.tracks[3].loops[0];
        let target_backend = model.loops[&target].backend_id;
        {
            let model = model.loops.get_mut(&target).unwrap();
            model.length = 480;
            model.audio_data = Some(BackendAudioData {
                channels: vec![shoop_backend::BackendAudioChannelData {
                    samples: Arc::from([0.25, -0.25]),
                    ..Default::default()
                }],
            });
            model.state.empty = false;
            model.state.selected = false;
        }
        model.loops.get_mut(&source).unwrap().state.selected = true;
        assert!(model
            .compose_loop_serial(&mut backend, target, source)
            .unwrap_err()
            .contains("is not a composite"));
        assert!(model.loops[&target].composite.is_none());

        model
            .handle_loop_action(
                &mut backend,
                target_track,
                target,
                LoopAction::ConvertToComposite,
            )
            .unwrap();
        assert!(backend
            .operations()
            .contains(&shoop_backend::FakeOperation::Clear(target_backend)));
        let converted = &model.loops[&target];
        assert_eq!(converted.length, 0);
        assert!(converted.audio_data.is_none());
        assert!(converted.script_composition.is_empty());
        assert!(converted.state.selected);
        assert!(!model.loops[&source].state.selected);
        assert_eq!(
            converted.composite.as_ref().unwrap(),
            &CompositeDocument {
                kind: CompositeKindDocument::Regular,
                instances: Vec::new(),
            }
        );
        assert_eq!(
            model.details_snapshot().unwrap().composite.unwrap().kind,
            shoop_app_api::CompositeKind::Regular
        );
        let converted_before = model.loops[&target].composite.clone();
        assert!(model
            .handle_loop_action(
                &mut backend,
                target_track,
                target,
                LoopAction::ConvertToComposite,
            )
            .unwrap_err()
            .contains("already a composite"));
        assert_eq!(model.loops[&target].composite, converted_before);

        model
            .compose_loop_serial(&mut backend, target, source)
            .unwrap();
        let composed = &model.loops[&target];
        assert_eq!(composed.script_composition, [vec![source]]);
        assert_eq!(
            composed.composite.as_ref().unwrap().instances[0].loop_id,
            source.raw()
        );

        backend.enable_composite_loops();
        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 100;
        model.loops.get_mut(&source).unwrap().length = 100;
        backend
            .set_loop_length(model.loops[&sync].backend_id, 100)
            .unwrap();
        backend
            .set_loop_length(model.loops[&source].backend_id, 100)
            .unwrap();
        model
            .compose_loop_at(&mut backend, target, source, 3)
            .unwrap();
        let positioned = &model.loops[&target];
        assert_eq!(positioned.composite.as_ref().unwrap().instances.len(), 2);
        assert_eq!(
            positioned.composite.as_ref().unwrap().instances[1].start_cycle,
            3
        );
        assert_eq!(positioned.length, 400);
        assert!(positioned.backend_composite.is_some());
        assert_eq!(
            model
                .details_snapshot()
                .unwrap()
                .composite
                .unwrap()
                .events
                .iter()
                .map(|event| event.start_frame)
                .collect::<Vec<_>>(),
            [0, 300]
        );
        let before = positioned.composite.clone();
        assert!(model
            .compose_loop_serial(&mut backend, target, target)
            .unwrap_err()
            .contains("would create a cycle"));
        assert!(model
            .compose_loop_serial(&mut backend, target, LoopId::from_raw(u64::MAX))
            .unwrap_err()
            .contains("stale or unknown composition source"));
        assert_eq!(model.loops[&target].composite, before);

        model.loops.get_mut(&nested).unwrap().composite = Some(CompositeDocument {
            kind: CompositeKindDocument::Regular,
            instances: vec![CompositeLoopInstanceDocument {
                instance_id: 1,
                start_cycle: 0,
                loop_id: target.raw(),
                mode: None,
                n_cycles: None,
            }],
        });
        assert!(model
            .compose_loop_serial(&mut backend, target, nested)
            .unwrap_err()
            .contains("would create a cycle"));
        assert_eq!(model.loops[&target].composite, before);

        let capture = backend.capture_session().unwrap();
        let saved = model.session_bundle_from_backend(&capture).unwrap();
        let saved_target = saved
            .document
            .track_groups
            .iter()
            .flat_map(|group| &group.tracks)
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.id == target.raw())
            .unwrap();
        assert!(saved_target.channels.is_empty());
        assert_eq!(saved_target.composite, before);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn failed_backend_composite_reconfiguration_does_not_commit_application_schedule() {
        let mut backend = FakeBackend::default();
        backend.enable_composite_loops();
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
                    name: "Transactional".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let track = model.tracks[1].id;
        let target = model.tracks[1].loops[0];
        let source = model.tracks[1].loops[1];
        model
            .handle_loop_action(&mut backend, track, target, LoopAction::ConvertToComposite)
            .unwrap();
        let before_composite = model.loops[&target].composite.clone();
        let before_sections = model.loops[&target].script_composition.clone();
        let before_backend = model.loops[&target].backend_composite;
        let before_operations = backend.operations().len();

        backend.fail_next_composite_configuration("injected composite configuration failure");
        assert!(model
            .compose_loop_serial(&mut backend, target, source)
            .unwrap_err()
            .contains("injected composite configuration failure"));

        assert_eq!(model.loops[&target].composite, before_composite);
        assert_eq!(model.loops[&target].script_composition, before_sections);
        assert_eq!(model.loops[&target].backend_composite, before_backend);
        assert_eq!(backend.operations().len(), before_operations);

        let sync = model.tracks[0].loops[0];
        model.loops.get_mut(&sync).unwrap().length = 1;
        model.loops.get_mut(&source).unwrap().length = 1;
        backend
            .set_loop_length(model.loops[&sync].backend_id, 1)
            .unwrap();
        backend
            .set_loop_length(model.loops[&source].backend_id, 1)
            .unwrap();
        model
            .compose_loop_serial(&mut backend, target, source)
            .unwrap();
        let before_composite = model.loops[&target].composite.clone();
        let before_sections = model.loops[&target].script_composition.clone();
        let before_backend = model.loops[&target].backend_composite;
        backend.fail_next_composite_configuration("injected composite deletion failure");
        assert!(model
            .delete_composite_events(&mut backend, target, &[CompositeEventId { instance_id: 1 }],)
            .unwrap_err()
            .contains("injected composite deletion failure"));
        assert_eq!(model.loops[&target].composite, before_composite);
        assert_eq!(model.loops[&target].script_composition, before_sections);
        assert_eq!(model.loops[&target].backend_composite, before_backend);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn composite_event_deletion_leaves_other_instance_positions_unchanged() {
        let (mut backend, mut model, _, target, sources) = engine_model_with_regular_composite();
        let before =
            model.composite_details_snapshot(model.loops[&target].composite.as_ref().unwrap());
        let last_start = before
            .events
            .iter()
            .find(|event| event.loop_id == sources[2])
            .unwrap()
            .start_frame;

        model
            .delete_composite_events(
                &mut backend,
                target,
                &[
                    CompositeEventId { instance_id: 1 },
                    CompositeEventId { instance_id: 2 },
                ],
            )
            .unwrap();

        let composite = model.loops[&target].composite.as_ref().unwrap();
        assert_eq!(composite.instances.len(), 1);
        let after = model.composite_details_snapshot(composite);
        assert_eq!(after.events.len(), 1);
        assert_eq!(after.events[0].loop_id, sources[2]);
        assert_eq!(after.events[0].start_frame, last_start);
        assert_eq!(
            model.loops[&target].length,
            before.timeline_length_frames as u32
        );

        let unchanged = model.loops[&target].composite.clone();
        assert!(model
            .delete_composite_events(
                &mut backend,
                target,
                &[CompositeEventId { instance_id: 99 }],
            )
            .unwrap_err()
            .contains("stale event"));
        assert_eq!(model.loops[&target].composite, unchanged);

        model
            .delete_composite_events(&mut backend, target, &[CompositeEventId { instance_id: 3 }])
            .unwrap();
        assert!(model.loops[&target].composite.is_some());
        assert!(model.loops[&target].state.empty);
        assert_eq!(model.loops[&target].length, 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn composite_event_groups_move_or_duplicate_while_preserving_relative_positions() {
        for duplicate in [false, true] {
            let (mut backend, mut model, _, target, _) = engine_model_with_regular_composite();
            let before =
                model.composite_details_snapshot(model.loops[&target].composite.as_ref().unwrap());
            let selected = [0_usize, 2]
                .into_iter()
                .map(|index| CompositeEventId {
                    instance_id: before.events[index].instance_id,
                })
                .collect::<Vec<_>>();
            let cycle_length = before.cycle_length_frames;
            let relative = before.events[2].start_frame - before.events[0].start_frame;

            model
                .relocate_composite_events(&mut backend, target, &selected, 10, duplicate)
                .unwrap();

            let after =
                model.composite_details_snapshot(model.loops[&target].composite.as_ref().unwrap());
            assert_eq!(after.events.len(), if duplicate { 5 } else { 3 });
            let moved = &after.events[after.events.len() - 2..];
            assert_eq!(moved[0].start_frame, 10 * cycle_length);
            assert_eq!(moved[1].start_frame - moved[0].start_frame, relative);
            if duplicate {
                assert!(moved.iter().all(|event| selected
                    .iter()
                    .all(|id| id.instance_id != event.instance_id)));
            } else {
                assert_eq!(
                    moved
                        .iter()
                        .map(|event| event.instance_id)
                        .collect::<Vec<_>>(),
                    selected
                        .iter()
                        .map(|event| event.instance_id)
                        .collect::<Vec<_>>()
                );
            }
            assert!(after.events.iter().any(|event| {
                event.loop_id == before.events[1].loop_id
                    && event.start_frame == before.events[1].start_frame
            }));
            assert_eq!(
                after
                    .events
                    .iter()
                    .filter(|event| event.loop_id == before.events[0].loop_id)
                    .count(),
                if duplicate { 2 } else { 1 }
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rich_composite_survives_session_load_and_save_without_projection_loss() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Sources".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..20 {
            runtime.tick(Duration::ZERO);
            if runtime.take_file_output().is_some_and(|output| {
                let mut bundle = decode_session(&output.bytes).unwrap();
                let track = &mut bundle.document.track_groups[1].tracks[0];
                let target = track.loops[0].id;
                let source_a = track.loops[1].id;
                let source_b = track.loops[2].id;
                track.loops[0].channels.clear();
                track.loops[0].length_frames = 350;
                track.loops[1].length_frames = 100;
                track.loops[1].channels[0].latency = TakeLatencyDocument {
                    capture_alignment_frames: -2,
                    observation: LatencyObservationDocument {
                        certainty: LatencyCertaintyDocument::ManualOnly,
                        sample_rate: 48_000,
                        revision: 4,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let source_latency = track.loops[1].channels[0].latency.clone();
                track.loops[2].length_frames = 200;
                let composite = CompositeDocument {
                    kind: CompositeKindDocument::Script,
                    instances: vec![
                        CompositeLoopInstanceDocument {
                            instance_id: 1,
                            start_cycle: 1,
                            loop_id: source_a,
                            mode: Some("recording".to_owned()),
                            n_cycles: Some(3),
                        },
                        CompositeLoopInstanceDocument {
                            instance_id: 2,
                            start_cycle: 6,
                            loop_id: source_b,
                            mode: Some("playing".to_owned()),
                            n_cycles: None,
                        },
                        CompositeLoopInstanceDocument {
                            instance_id: 3,
                            start_cycle: 3,
                            loop_id: source_b,
                            mode: Some("playing_dry_through_wet".to_owned()),
                            n_cycles: Some(2),
                        },
                    ],
                };
                track.loops[0].composite = Some(composite.clone());
                bundle.document.selected_loop_ids = vec![target];
                runtime
                    .dispatch(AppIntent::LoadSessionBytes {
                        name: "rich-composite.shoop".to_owned(),
                        bytes: Arc::from(encode_session(&bundle, "test").unwrap()),
                    })
                    .unwrap();
                for _ in 0..20 {
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
                let details = loaded.details.as_ref().unwrap().composite.as_ref().unwrap();
                assert_eq!(details.kind, shoop_app_api::CompositeKind::Script);
                assert_eq!(details.events.len(), 3);
                assert_eq!(details.events[0].start_frame, 1);
                assert_eq!(details.events[0].mode.as_deref(), Some("recording"));
                assert_eq!(details.events[0].forced_n_cycles, Some(3));
                assert_eq!(details.events[2].instance_id, 3);

                runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
                for _ in 0..20 {
                    runtime.tick(Duration::ZERO);
                    if let Some(output) = runtime.take_file_output() {
                        let saved = decode_session(&output.bytes).unwrap();
                        let saved_composite = saved.document.track_groups[1].tracks[0].loops[0]
                            .composite
                            .as_ref()
                            .unwrap();
                        assert_eq!(saved_composite, &composite);
                        let saved_source = saved.document.track_groups[1].tracks[0]
                            .loops
                            .iter()
                            .find(|loop_| loop_.id == source_a)
                            .unwrap();
                        assert_eq!(saved_source.channels[0].latency, source_latency);
                        return true;
                    }
                }
                panic!("rich composite save did not complete");
            }) {
                return;
            }
        }
        panic!("baseline session save did not complete");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn latest_global_or_script_repeat_sync_policy_applies_to_existing_and_new_loops() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        model
            .handle_global_action(&mut backend, GlobalControlAction::SetSync(false))
            .unwrap();
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Unsynced".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let loop_id = model.tracks[1].loops[0];
        assert!(!model.loops[&loop_id].repeat_sync);
        assert_eq!(
            backend.loop_sync_source(model.loops[&loop_id].backend_id),
            None
        );

        model
            .apply_script_operation(
                &mut backend,
                ControlOperation::SetRepeatSync {
                    loops: vec![loop_id],
                    active: true,
                },
            )
            .unwrap();
        assert!(model.loops[&loop_id].repeat_sync);
        assert!(backend
            .loop_sync_source(model.loops[&loop_id].backend_id)
            .is_some());
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert!(model.loops[&loop_id].repeat_sync);

        model
            .handle_global_action(&mut backend, GlobalControlAction::SetSync(false))
            .unwrap();
        assert!(!model.loops[&loop_id].repeat_sync);
        model
            .handle_global_action(&mut backend, GlobalControlAction::SetSync(true))
            .unwrap();
        assert!(model.loops[&loop_id].repeat_sync);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabling_sync_makes_a_primitive_loop_repeat_at_its_own_boundary() {
        let mut backend = EngineBackend::new_dummy(1_000, 1).unwrap();
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
        let track_id = model.tracks[1].id;
        let loop_id = model.tracks[1].loops[0];
        backend
            .set_loop_length(model.loops[&sync].backend_id, 10)
            .unwrap();
        backend
            .set_loop_length(model.loops[&loop_id].backend_id, 4)
            .unwrap();
        backend
            .transition_loop(
                model.loops[&sync].backend_id,
                BackendLoopMode::Playing,
                None,
            )
            .unwrap();
        backend.advance(Duration::from_millis(1));
        model
            .handle_global_action(&mut backend, GlobalControlAction::SetSync(false))
            .unwrap();
        model
            .handle_loop_action(&mut backend, track_id, loop_id, LoopAction::PlayClicked)
            .unwrap();

        backend.advance(Duration::from_millis(4));
        let state = backend.poll().unwrap().loops[&model.loops[&loop_id].backend_id].clone();

        assert_eq!(state.mode, BackendLoopMode::Playing);
        assert_eq!(state.position, 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn independently_playing_a_child_does_not_advance_its_composite() {
        let (mut backend, mut model, _, target, sources) = engine_model_with_regular_composite();
        backend
            .transition_loop(
                model.loops[&sources[0]].backend_id,
                BackendLoopMode::Playing,
                None,
            )
            .unwrap();
        backend.advance(Duration::from_millis(2));
        model.apply_backend_snapshot(backend.poll().unwrap());

        assert_eq!(
            (
                model.loops[&target].state.mode,
                model.loops[&target].position
            ),
            (LoopMode::Stopped, 0)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn gui_play_on_a_regular_composite_starts_the_composite_and_first_child() {
        let (mut backend, mut model, track_id, target, sources) =
            engine_model_with_regular_composite();
        model
            .handle_global_action(&mut backend, GlobalControlAction::SetSync(false))
            .unwrap();
        model
            .handle_loop_action(&mut backend, track_id, target, LoopAction::PlayClicked)
            .unwrap();
        backend.advance(Duration::from_millis(1));
        model.apply_backend_snapshot(backend.poll().unwrap());

        assert_eq!(
            (
                model.loops[&target].state.mode,
                model.loops[&sources[0]].state.mode,
            ),
            (LoopMode::Playing, LoopMode::Playing)
        );
        assert!(sources[1..]
            .iter()
            .all(|source| model.loops[source].state.mode == LoopMode::Stopped));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn public_intents_disable_repeat_sync_for_an_independently_wrapping_loop() {
        let backend = EngineBackend::new_dummy(1_000, 1).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Unsynced".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        let sync_track = snapshot.tracks[0].id;
        let sync = snapshot.tracks[0].loops[0].id;
        let track = snapshot.tracks[1].id;
        let loop_id = snapshot.tracks[1].loops[0].id;
        for (track_id, id, frames) in [(sync_track, sync, 10), (track, loop_id, 4)] {
            runtime
                .dispatch(AppIntent::Loop {
                    track_id,
                    loop_id: id,
                    action: LoopAction::RecordClicked,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
            runtime.tick(Duration::from_millis(frames));
            runtime
                .dispatch(AppIntent::Loop {
                    track_id,
                    loop_id: id,
                    action: LoopAction::StopClicked,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
        }
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(true)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync_track,
                loop_id: sync,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        assert_eq!(
            runtime.snapshot().tracks[0].loops[0].mode,
            LoopMode::Playing
        );
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(4));
        let loop_ = &runtime.snapshot().tracks[1].loops[0];
        assert_eq!(loop_.mode, LoopMode::Playing);
        assert_eq!(loop_.position, 0.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn public_intents_drive_three_section_engine_composite_and_isolate_child_control() {
        let backend = EngineBackend::new_dummy(1_000, 1).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Composite".to_owned(),
                audio_channels: 1,
                midi: false,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        let sync_track = snapshot.tracks[0].id;
        let sync = snapshot.tracks[0].loops[0].id;
        let track = snapshot.tracks[1].id;
        let target = snapshot.tracks[1].loops[0].id;
        let sources = [
            snapshot.tracks[1].loops[1].id,
            snapshot.tracks[1].loops[2].id,
            snapshot.tracks[1].loops[3].id,
        ];
        let mut record_for = |track_id, loop_id, frames| {
            runtime
                .dispatch(AppIntent::Loop {
                    track_id,
                    loop_id,
                    action: LoopAction::RecordClicked,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
            runtime.tick(Duration::from_millis(frames));
            runtime
                .dispatch(AppIntent::Loop {
                    track_id,
                    loop_id,
                    action: LoopAction::StopClicked,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
        };
        record_for(sync_track, sync, 1);
        for source in sources {
            record_for(track, source, 4);
        }
        runtime
            .dispatch(AppIntent::Loop {
                track_id: sync_track,
                loop_id: sync,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: target,
                action: LoopAction::ConvertToComposite,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        for source in sources {
            runtime
                .dispatch(AppIntent::ComposeLoopSerial {
                    target_loop_id: target,
                    source_loop_id: source,
                })
                .unwrap();
            runtime.tick(Duration::ZERO);
        }

        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: sources[2],
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        let isolated = runtime.snapshot();
        let parent = isolated.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        let child = isolated.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == sources[2])
            .unwrap();
        assert_eq!(child.mode, LoopMode::Playing);
        assert!(child.position > 0.0);
        assert_eq!(parent.mode, LoopMode::Stopped);
        assert_eq!(parent.position, 0.0);
        assert!(parent.active_composite_children.is_empty());
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: sources[2],
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: target,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let started = runtime.snapshot();
        let parent = started.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.mode, LoopMode::Playing);
        assert_eq!(parent.active_composite_children.as_ref(), [sources[0]]);

        runtime.tick(Duration::from_millis(4));
        let second = runtime.snapshot();
        let parent = second.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.active_composite_children.as_ref(), [sources[1]]);
        assert_eq!(parent.composite_iteration, Some(4));
        assert!((parent.position - 1.0 / 3.0).abs() < f32::EPSILON);
        runtime.tick(Duration::from_millis(4));
        let third = runtime.snapshot();
        let parent = third.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.active_composite_children.as_ref(), [sources[2]]);
        runtime.tick(Duration::from_millis(4));
        let wrapped = runtime.snapshot();
        let parent = wrapped.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.active_composite_children.as_ref(), [sources[0]]);
        assert_eq!(parent.composite_cycle_count, 1);

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        let output = loop {
            runtime.tick(Duration::from_millis(1));
            if let Some(output) = runtime.take_file_output() {
                break output;
            }
        };
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "composite-round-trip.shoop".to_owned(),
                bytes: output.bytes,
            })
            .unwrap();
        for _ in 0..20 {
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
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: target,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loaded = runtime.snapshot();
        let parent = loaded.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.mode, LoopMode::Playing);
        assert_eq!(parent.active_composite_children.as_ref(), [sources[0]]);

        runtime
            .dispatch(AppIntent::RequestAudioDriverSwitch {
                config: AudioDriverConfig::Dummy(shoop_app_api::DummyAudioDriverConfig {
                    sample_rate: 1_000,
                    buffer_size: 2,
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
        assert_eq!(
            runtime.snapshot().audio_drivers.switch.status,
            AudioDriverSwitchStatus::Persisting
        );
        runtime
            .dispatch(AppIntent::Loop {
                track_id: track,
                loop_id: target,
                action: LoopAction::PlayClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let replaced = runtime.snapshot();
        let parent = replaced.tracks[1]
            .loops
            .iter()
            .find(|loop_| loop_.id == target)
            .unwrap();
        assert_eq!(parent.mode, LoopMode::Playing);
        assert_eq!(parent.active_composite_children.as_ref(), [sources[0]]);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        assert_eq!(composite.instances.len(), 2);
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

    #[shoop_wasm_test_support::shoop_test]
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
        let mut runtime =
            cooperative_start_with_midi(Box::new(FakeBackend::default()), Box::new(midi)).unwrap();
        for index in 0..8 {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: format!("Track {index}"),
                    audio_channels: if index == 0 { 1 } else { 2 },
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
                source: Arc::from(TEST_APC_MINI_SCRIPT),
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
        assert!(reset.iter().any(|(_, message)| message == &[0x90, 83, 0]));
        assert!(reset.iter().any(|(_, message)| message == &[0x90, 87, 1]));

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
        let mut global_feedback = Vec::new();
        for action in [
            GlobalControlAction::SetSolo(true),
            GlobalControlAction::SetSolo(false),
            GlobalControlAction::SetSync(true),
            GlobalControlAction::SetSync(false),
        ] {
            runtime.dispatch(AppIntent::Global(action)).unwrap();
            runtime.tick(Duration::ZERO);
            runtime.tick(Duration::from_millis(2));
            global_feedback.extend(midi_control.take_sent());
        }
        for _ in 0..10 {
            runtime.tick(Duration::from_millis(1));
            global_feedback.extend(midi_control.take_sent());
        }
        for expected in [[0x90, 83, 1], [0x90, 83, 0], [0x90, 87, 0], [0x90, 87, 1]] {
            assert!(
                global_feedback
                    .iter()
                    .any(|(_, message)| message == &expected),
                "missing {expected:?} in {global_feedback:?}"
            );
        }

        for note in [98, 86, 84, 85, 82, 70, 71, 68, 69] {
            send_note(&mut runtime, &midi_control, note, true);
            send_note(&mut runtime, &midi_control, note, false);
            let feedback = midi_control.take_sent();
            assert!(feedback
                .iter()
                .any(|(_, message)| message == &[0x90, note, 1]));
            assert!(feedback
                .iter()
                .any(|(_, message)| message == &[0x90, note, 0]));
        }

        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(7)))
            .unwrap();
        runtime.tick(Duration::ZERO);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 71, true);
        send_note(&mut runtime, &midi_control, 71, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 7);
        let mut debug_reset = midi_control.take_sent();
        for _ in 0..70 {
            runtime.tick(Duration::from_millis(1));
            debug_reset.extend(midi_control.take_sent());
        }
        assert!(debug_reset.len() >= 67);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetApplyNCycles(0)))
            .unwrap();
        runtime.tick(Duration::ZERO);

        midi_control.push_input("apc-source", vec![0xb0, 48, 127]);
        runtime.tick(Duration::from_millis(2));
        assert_eq!(runtime.snapshot().tracks[1].controls.output_gain_db, 0.0);

        let grid_note = 56;
        send_note(&mut runtime, &midi_control, grid_note, true);
        let after_default = runtime.snapshot();
        assert_eq!(
            after_default.tracks[1].loops[0].mode,
            LoopMode::Recording,
            "script={:?}",
            after_default.scripting.scripts[0]
        );
        send_note(&mut runtime, &midi_control, 70, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 70, false);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::PlayingDryThroughWet
        );
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 82, false);
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0].mode,
            LoopMode::Stopped
        );
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 82, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(runtime.snapshot().tracks[1].loops[0].empty);
        send_note(&mut runtime, &midi_control, 85, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 85, false);
        assert!(runtime.snapshot().tracks[1].loops[0].empty);
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 82, false);

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
        send_note(&mut runtime, &midi_control, 71, true);
        send_note(&mut runtime, &midi_control, 7, true);
        send_note(&mut runtime, &midi_control, 71, false);
        assert_eq!(runtime.snapshot().global_controls.apply_n_cycles, 0);

        send_note(&mut runtime, &midi_control, 68, true);
        midi_control.push_input("apc-source", vec![0xb0, 48, 127]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 68, false);
        assert_eq!(runtime.snapshot().tracks[1].controls.output_gain_db, 20.0);
        assert!(runtime.snapshot().tracks[1].controls.output_muted);
        send_note(&mut runtime, &midi_control, 69, true);
        midi_control.push_input("apc-source", vec![0xb0, 48, 127]);
        midi_control.push_input("apc-source", vec![0xb0, 49, 127]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, grid_note, true);
        send_note(&mut runtime, &midi_control, 69, false);
        assert!(!runtime.snapshot().tracks[1].controls.output_stereo);
        assert_eq!(runtime.snapshot().tracks[1].controls.output_balance, 0.0);
        assert!(runtime.snapshot().tracks[2].controls.output_stereo);
        assert_eq!(runtime.snapshot().tracks[2].controls.output_balance, 1.0);
        assert!(runtime.snapshot().tracks[1].controls.input_monitoring);

        send_note(&mut runtime, &midi_control, 68, true);
        midi_control.push_input("apc-source", vec![0xb0, 56, 127]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, 7, true);
        send_note(&mut runtime, &midi_control, 68, false);
        assert_eq!(runtime.snapshot().tracks[0].controls.output_gain_db, 20.0);
        assert!(runtime.snapshot().tracks[0].controls.output_muted);
        send_note(&mut runtime, &midi_control, 69, true);
        midi_control.push_input("apc-source", vec![0xb0, 56, 0]);
        runtime.tick(Duration::from_millis(2));
        send_note(&mut runtime, &midi_control, 7, true);
        send_note(&mut runtime, &midi_control, 69, false);
        assert!(!runtime.snapshot().tracks[0].controls.output_stereo);
        assert_eq!(runtime.snapshot().tracks[0].controls.output_balance, 0.0);
        assert!(runtime.snapshot().tracks[0].controls.input_monitoring);

        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetAutoMuteOtherTrackInputs(true),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        send_note(&mut runtime, &midi_control, 69, true);
        send_note(&mut runtime, &midi_control, 57, true);
        send_note(&mut runtime, &midi_control, 69, false);
        assert!(!runtime.snapshot().tracks[1].controls.input_monitoring);
        assert!(runtime.snapshot().tracks[2].controls.input_monitoring);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetAutoMuteOtherTrackInputs(false),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
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
        assert!(!runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 87, false);
        assert!(runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 87, true);
        send_note(&mut runtime, &midi_control, 87, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(!runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 87, true);
        runtime.tick(Duration::from_millis(250));
        assert!(runtime.snapshot().global_controls.sync);
        send_note(&mut runtime, &midi_control, 87, false);
        assert!(!runtime.snapshot().global_controls.sync);

        send_note(&mut runtime, &midi_control, 83, true);
        assert!(!runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 83, false);
        assert!(runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 83, true);
        send_note(&mut runtime, &midi_control, 83, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(!runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 83, true);
        runtime.tick(Duration::from_millis(250));
        assert!(runtime.snapshot().global_controls.solo);
        send_note(&mut runtime, &midi_control, 83, false);
        assert!(!runtime.snapshot().global_controls.solo);
        runtime
            .dispatch(AppIntent::Global(
                GlobalControlAction::SetDefaultRecordingAction(
                    shoop_app_api::DefaultRecordingAction::Record,
                ),
            ))
            .unwrap();
        runtime.tick(Duration::ZERO);
        send_note(&mut runtime, &midi_control, 7, true);
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

        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 70, true);
        send_note(&mut runtime, &midi_control, 56, true);
        send_note(&mut runtime, &midi_control, 56, false);
        send_note(&mut runtime, &midi_control, 59, true);
        send_note(&mut runtime, &midi_control, 59, false);
        send_note(&mut runtime, &midi_control, 70, false);
        send_note(&mut runtime, &midi_control, 98, false);
        let extended_sections = &runtime.model.loops[&target_id].script_composition;
        assert_eq!(extended_sections.len(), 2);
        assert_eq!(
            extended_sections[0],
            [
                runtime.snapshot().tracks[2].loops[0].id,
                runtime.snapshot().tracks[3].loops[0].id,
            ]
        );
        assert_eq!(
            extended_sections[1],
            [runtime.snapshot().tracks[4].loops[0].id]
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
        send_note(&mut runtime, &midi_control, 7, true);
        send_note(&mut runtime, &midi_control, 7, false);
        send_note(&mut runtime, &midi_control, 70, false);
        send_note(&mut runtime, &midi_control, 98, false);
        let serial_target = runtime.snapshot().tracks[4].loops[0].id;
        let serial_sections = &runtime.model.loops[&serial_target].script_composition;
        assert_eq!(serial_sections.len(), 3);
        assert_eq!(
            serial_sections[0],
            [runtime.snapshot().tracks[5].loops[0].id]
        );
        assert_eq!(
            serial_sections[1],
            [runtime.snapshot().tracks[6].loops[0].id]
        );
        assert_eq!(
            serial_sections[2],
            [runtime.snapshot().tracks[0].loops[0].id]
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

    #[shoop_wasm_test_support::shoop_test]
    fn production_apc_reset_restores_authoritative_loop_colors() {
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
        let mut runtime =
            cooperative_start_with_midi(Box::new(FakeBackend::default()), Box::new(midi)).unwrap();
        for index in 0..2 {
            runtime
                .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                    name: format!("Track {index}"),
                    audio_channels: 2,
                    midi: false,
                }))
                .unwrap();
        }
        runtime.tick(Duration::ZERO);
        let initial_loop_id = runtime.model.tracks[1].loops[2];
        let initial_backend_id = runtime.model.loops[&initial_loop_id].backend_id;
        runtime
            .backend
            .set_loop_length(initial_backend_id, 100)
            .unwrap();
        runtime
            .backend
            .transition_loop(initial_backend_id, BackendLoopMode::Recording, None)
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::Global(GlobalControlAction::SetSync(false)))
            .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(TEST_APC_MINI_SCRIPT),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::from_millis(1));
        runtime.tick(Duration::from_millis(1_000));
        let mut initial_reset = midi_control.take_sent();
        for _ in 0..70 {
            runtime.tick(Duration::from_millis(1));
            initial_reset.extend(midi_control.take_sent());
        }
        assert!(initial_reset
            .iter()
            .any(|(_, message)| message == &[0x90, 40, 3]));

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
        let collect = |runtime: &mut CooperativeApplicationRuntime,
                       midi: &shoop_scripting::FakeMidiControl| {
            let mut messages = midi.take_sent();
            for _ in 0..20 {
                runtime.tick(Duration::from_millis(1));
                messages.extend(midi.take_sent());
            }
            messages
        };

        send_note(&mut runtime, &midi_control, 56, true);
        assert!(collect(&mut runtime, &midi_control)
            .iter()
            .any(|(_, message)| message == &[0x90, 56, 3]));
        runtime.tick(Duration::from_millis(100));
        send_note(&mut runtime, &midi_control, 56, true);
        assert!(collect(&mut runtime, &midi_control)
            .iter()
            .any(|(_, message)| message == &[0x90, 56, 1]));
        let loop_id = runtime.model.tracks[1].loops[0];
        let backend_id = runtime.model.loops[&loop_id].backend_id;
        runtime.backend.set_loop_length(backend_id, 100).unwrap();
        runtime.tick(Duration::ZERO);
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, 56, true);
        send_note(&mut runtime, &midi_control, 82, false);
        assert!(collect(&mut runtime, &midi_control)
            .iter()
            .any(|(_, message)| message == &[0x90, 56, 5]));
        send_note(&mut runtime, &midi_control, 98, true);
        send_note(&mut runtime, &midi_control, 82, true);
        send_note(&mut runtime, &midi_control, 56, true);
        send_note(&mut runtime, &midi_control, 82, false);
        send_note(&mut runtime, &midi_control, 98, false);
        assert!(collect(&mut runtime, &midi_control)
            .iter()
            .any(|(_, message)| message == &[0x90, 56, 0]));

        send_note(&mut runtime, &midi_control, 84, true);
        send_note(&mut runtime, &midi_control, 41, true);
        send_note(&mut runtime, &midi_control, 84, false);
        assert!(collect(&mut runtime, &midi_control)
            .iter()
            .any(|(_, message)| message == &[0x90, 41, 3]));
        midi_control.set_endpoints(Vec::new());
        runtime.tick(Duration::from_millis(500));
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
        runtime.tick(Duration::from_millis(1_000));
        let mut reset = midi_control.take_sent();
        for _ in 0..70 {
            runtime.tick(Duration::from_millis(1));
            reset.extend(midi_control.take_sent());
        }
        assert!(reset.iter().any(|(_, message)| message == &[0x90, 41, 3]));
        assert!(!reset.iter().any(|(_, message)| message == &[0x90, 41, 0]));
    }

    #[cfg(target_os = "linux")]
    #[shoop_wasm_test_support::shoop_test]
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
        let mut runtime = cooperative_start_with_midi(
            Box::new(FakeBackend::default()),
            Box::new(NativeMidiService::new()),
        )
        .unwrap();
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "akai_apc_mini_mk1.lua".to_owned(),
                source: Arc::from(TEST_APC_MINI_SCRIPT),
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
        }
        assert!(!runtime.snapshot().global_controls.solo);
        source.send(&[0x80, 83, 0]).unwrap();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(5));
            runtime.tick(Duration::from_millis(5));
            if runtime.snapshot().global_controls.solo {
                break;
            }
        }
        assert!(runtime.snapshot().global_controls.solo);
        source.send(&[0x90, 83, 0x7f]).unwrap();
        for _ in 0..60 {
            std::thread::sleep(Duration::from_millis(5));
            runtime.tick(Duration::from_millis(5));
            if !runtime.snapshot().global_controls.solo {
                break;
            }
        }
        assert!(!runtime.snapshot().global_controls.solo);
        source.send(&[0x80, 83, 0]).unwrap();
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(5));
            runtime.tick(Duration::from_millis(5));
            if runtime.snapshot().global_controls.solo {
                break;
            }
        }
        assert!(runtime.snapshot().global_controls.solo);
        runtime.tick(Duration::from_millis(1_000));
        assert!(receiver.recv_timeout(Duration::from_secs(1)).is_ok());
        drop(sink);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn actor_rejects_stale_and_mismatched_ids_without_state_changes() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        let initial = handle.snapshot();
        handle
            .dispatch(AppIntent::Loop {
                track_id: TrackId::from_raw(900),
                loop_id: LoopId::from_raw(901),
                action: LoopAction::IconClicked(SelectionModifiers::default()),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.revision > initial.revision);
        assert_eq!(snapshot.tracks.len(), initial.tracks.len());
        assert_eq!(snapshot.tracks[0].id, initial.tracks[0].id);

        handle
            .dispatch(AppIntent::Track {
                track_id: TrackId::from_raw(902),
                action: TrackAction::NameChanged("nope".to_owned()),
            })
            .unwrap();
        let after_track = wait_for(&handle, |candidate| candidate.revision > snapshot.revision);
        assert_eq!(after_track.tracks.len(), initial.tracks.len());
        assert_eq!(after_track.tracks[0].id, initial.tracks[0].id);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
        let snapshot = wait_for(&handle, |snapshot| snapshot.revision > 1);
        assert_eq!(snapshot.tracks.len(), 1);
        assert!(snapshot.connections.application_ports.iter().all(|port| {
            port.owner == ApplicationPortOwner::GlobalFxControl
                || matches!(
                    port.owner,
                    ApplicationPortOwner::Track { track_id, .. }
                        if track_id == snapshot.tracks[0].id
                )
        }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn new_track_latency_defaults_reconcile_and_latch_on_the_next_operation() {
        let backend = EngineBackend::new_dummy(48_000, 64).unwrap();
        let mut runtime = CooperativeApplicationRuntime::start(Box::new(backend)).unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::AddTrackWithLatencyPolicy {
                spec: TrackSpec {
                    name: "Default latency policy".to_owned(),
                    topology: TrackSpecTopology::Direct {
                        audio_channels: 1,
                        midi: false,
                    },
                },
                policy: shoop_app_api::TrackLatencyPolicyState {
                    components: Arc::from([shoop_app_api::LatencyComponentPolicyState {
                        kind: shoop_app_api::LatencyComponentKind::ExternalCapture,
                        enabled: true,
                        value_mode: shoop_app_api::LatencyValueMode::Manual(4),
                        range_selection: LatencyRangeSelectionState::Maximum,
                    }]),
                    revision: 1,
                    ..Default::default()
                },
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let track = &runtime.snapshot().tracks[1];
        assert!(!track.latency_policy.pending);
        assert_eq!(track.latency_policy.revision, 1);
        assert!(runtime.snapshot().connections.application_ports.iter().any(|port| {
            matches!(port.owner, ApplicationPortOwner::Track { track_id, .. } if track_id == track.id)
                && port.capture_latency.certainty == shoop_app_api::LatencyCertaintyState::Unknown
        }));
        let (track_id, loop_id) = (track.id, track.loops[0].id);
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
        runtime.tick(Duration::from_millis(2));
        assert_eq!(
            runtime.snapshot().tracks[1].loops[0]
                .latency
                .capture_alignment_frames,
            4
        );
        assert!(runtime
            .snapshot()
            .status
            .latency_diagnostic_summary
            .contains("unresolved="));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn latency_intents_reconcile_optimistic_state_and_report_rejections() {
        let runtime = ApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let handle = runtime.handle();
        handle
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Latency".to_owned(),
                audio_channels: 1,
                midi: true,
            }))
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| snapshot.tracks.len() == 2);
        let track_id = snapshot.tracks[1].id;
        let loop_id = snapshot.tracks[1].loops[0].id;
        let policy = shoop_app_api::TrackLatencyPolicyState {
            cue_followed: true,
            cue_output: None,
            components: Arc::from([shoop_app_api::LatencyComponentPolicyState {
                kind: shoop_app_api::LatencyComponentKind::ExternalCapture,
                enabled: true,
                value_mode: shoop_app_api::LatencyValueMode::Manual(5),
                range_selection: LatencyRangeSelectionState::Maximum,
            }]),
            revision: 4,
            pending: false,
            error: None,
        };
        handle
            .dispatch(AppIntent::SetTrackLatencyPolicy {
                track_id,
                policy: policy.clone(),
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].latency_policy.revision == 4
                && !snapshot.tracks[1].latency_policy.pending
        });
        assert_eq!(
            snapshot.tracks[1].latency_policy.components,
            policy.components
        );

        handle
            .dispatch(AppIntent::SetTakeLatencyPolicy {
                loop_id,
                capture_alignment_frames: 9,
            })
            .unwrap();
        let snapshot = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].loops[0].latency.capture_alignment_frames == 9
        });
        assert_eq!(
            snapshot.tracks[1].loops[0].latency.capture_alignment_frames,
            9
        );

        handle
            .dispatch(AppIntent::SetTakeLatencyPolicy {
                loop_id,
                capture_alignment_frames: shoop_latency::MAX_COMPENSATION_FRAMES as i32 + 1,
            })
            .unwrap();
        let rejected = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].loops[0].latency.error.is_some()
        });
        assert_eq!(
            rejected.tracks[1].loops[0].latency.capture_alignment_frames,
            9
        );

        handle
            .dispatch(AppIntent::ConsolidateTakeLatency { loop_id })
            .unwrap();
        let consolidated = wait_for(&handle, |snapshot| {
            snapshot.tracks[1].loops[0].latency.capture_alignment_frames == 0
        });
        assert_eq!(
            consolidated.tracks[1].loops[0]
                .latency
                .capture_alignment_frames,
            0
        );
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn midi_panic_fans_out_all_channels_and_continues_after_track_failure() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        for (name, audio_channels, midi) in [
            ("fails", 0, true),
            ("works", 0, true),
            ("inactive", 0, true),
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
        for track_index in [1, 2, 4] {
            let track_id = model.tracks[track_index].id;
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
        let failed = model.tracks[1].backend_id;
        let successful = model.tracks[2].backend_id;
        backend.fail_midi_input_for(failed);
        model
            .active_piano_notes
            .insert(60, BTreeSet::from([model.tracks[1].id]));

        let operation_start = backend.operations().len();
        let error = model
            .handle_global_action(&mut backend, GlobalControlAction::MidiPanic)
            .unwrap_err();
        assert!(error.contains("could not send MIDI panic to 1 track(s)"));
        assert!(model.active_piano_notes.is_empty());
        let injections = backend.operations()[operation_start..]
            .iter()
            .filter_map(|operation| match operation {
                shoop_backend::FakeOperation::InjectMidiInput(track, events) => {
                    Some((*track, events.clone()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(injections.len(), 1);
        assert_eq!(injections[0].0, successful);
        assert_eq!(
            injections[0]
                .1
                .iter()
                .map(|event| event.data.clone())
                .collect::<Vec<_>>(),
            (0..16)
                .map(|channel| vec![0xb0 | channel, 120, 0])
                .collect::<Vec<_>>()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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
                min_dry_audio_channels: None,
                max_dry_audio_channels: None,
                min_wet_audio_channels: None,
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
                        latency: None,
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
        assert_eq!(channel.preplay_samples, 4);
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
        model.handle_intent(
            &mut backend,
            AppIntent::SetLoopTimeline {
                loop_id,
                start_offset: Some(-8),
                preplay_samples: Some(6),
                loop_length: Some(30),
            },
        );
        let edited = model.details_snapshot().unwrap();
        assert_eq!(edited.midi_channels[0].start_offset, -8);
        assert_eq!(edited.midi_channels[0].preplay_samples, 6);
        assert_eq!(edited.midi_channels[0].loop_length, 30);
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

    #[shoop_wasm_test_support::shoop_test]
    fn default_click_targets_only_the_touched_loop_without_changing_selection() {
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
        let touched = model.tracks[1].loops[0];
        let other = model.tracks[1].loops[1];
        for id in [touched, other] {
            let loop_model = model.loops.get_mut(&id).unwrap();
            loop_model.state.selected = true;
            loop_model.state.mode = LoopMode::Stopped;
            loop_model.length = 0;
        }
        let operation_start = backend.operations().len();

        model
            .handle_loop_action(&mut backend, track_id, touched, LoopAction::DefaultClicked)
            .unwrap();

        assert!(model.loops[&touched].state.selected);
        assert!(model.loops[&other].state.selected);
        let transitions = backend.operations()[operation_start..]
            .iter()
            .filter_map(|operation| match operation {
                shoop_backend::FakeOperation::Transition(id, mode, delay) => {
                    Some((*id, *mode, *delay))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            transitions,
            [(
                model.loops[&touched].backend_id,
                BackendLoopMode::Recording,
                Some(0),
            )]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
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
            port.owner == ApplicationPortOwner::GlobalFxControl
                || (matches!(
                    port.owner,
                    ApplicationPortOwner::Track { track_id, .. }
                        if track_id == initial.tracks[0].id
                ) && initial.tracks[0].port_ids.contains(&port.id))
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn audio_driver_switch_requires_confirmation_and_resamples_transactionally() {
        let mut runtime = CooperativeApplicationRuntime::start_with_scripts(
            Box::new(FakeBackend::default()),
            vec![StartupScript {
                name: "keyboard.lua".to_owned(),
                identity: None,
                source: TEST_KEYBOARD_SCRIPT.to_owned(),
                source_path: None,
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

    #[shoop_wasm_test_support::shoop_test]
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
            .dispatch(AppIntent::SetTakeLatencyPolicy {
                loop_id,
                capture_alignment_frames: 6,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().tracks[0].loops[0]
                .latency
                .capture_alignment_frames,
            6
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
            switched.tracks[0].loops[0].latency.capture_alignment_frames,
            3
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        assert!(snapshot.io_task.is_some());
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
            AudioDriverSwitchStatus::Failed,
            "{}",
            failed.audio_drivers.switch.message
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
                source_path: None,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let session_source =
            "shoop_announce_api_version(1, 0); require('shoop_control').set_solo(true)";
        let mut document = SessionDocument::empty(48_000);
        document.scripts.push(ScriptDocument {
            id: 77,
            name: "session.lua".to_owned(),
            entrypoint: "main.lua".to_owned(),
            enabled: true,
        });
        let mut session_bundle = SessionBundle::new(document.clone());
        session_bundle.scripts.insert(
            77,
            Arc::new(
                shoop_scripting::ScriptResourceBundle::source_only(
                    "main.lua",
                    Arc::<[u8]>::from(session_source.as_bytes()),
                )
                .unwrap(),
            ),
        );
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "scripts.shoop".to_owned(),
                bytes: Arc::from(encode_session(&session_bundle, "test").unwrap()),
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
        let mut cancelled = session_bundle.clone();
        cancelled.document.sample_rate = 32_000;
        cancelled.scripts.insert(
            77,
            Arc::new(
                shoop_scripting::ScriptResourceBundle::source_only(
                    "main.lua",
                    Arc::<[u8]>::from(
                        &b"shoop_announce_api_version(1, 0); require('shoop_control').set_solo(false)"[..],
                    ),
                )
                .unwrap(),
            ),
        );
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "cancelled.shoop".to_owned(),
                bytes: Arc::from(encode_session(&cancelled, "test").unwrap()),
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
            entrypoint: "main.lua".to_owned(),
            enabled: true,
        });
        let mut invalid = SessionBundle::new(invalid);
        invalid.scripts.insert(
            88,
            Arc::new(
                shoop_scripting::ScriptResourceBundle::source_only(
                    "main.lua",
                    Arc::<[u8]>::from(&b"function("[..]),
                )
                .unwrap(),
            ),
        );
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "invalid.shoop".to_owned(),
                bytes: Arc::from(encode_session(&invalid, "test").unwrap()),
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

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn filesystem_script_conversion_captures_resources_off_actor_and_survives_source_removal() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temporary.path().join("help/images")).unwrap();
        let script_path = temporary.path().join("portable.lua");
        let markdown = "portable\n\n![icon](images/icon.png)";
        let source = "shoop_announce_api_version(1, 0); local f=require('shoop_file'); local loaded=f.load('help/readme.md'); local d=require('shoop_dialog'); d.simple('Help', {d.markdown_file('help/readme.md')})";
        std::fs::write(&script_path, source).unwrap();
        std::fs::write(temporary.path().join("help/readme.md"), markdown).unwrap();
        std::fs::write(
            temporary.path().join("help/images/icon.png"),
            include_bytes!("../../../../resources/logo-small.png"),
        )
        .unwrap();

        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddEphemeralScript {
                name: "portable.lua".to_owned(),
                source: Arc::from(source),
                source_path: Some(script_path.to_string_lossy().into_owned()),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let script_id = runtime.snapshot().scripting.scripts[0].id;
        runtime
            .dispatch(AppIntent::ConvertScriptKind {
                script_id,
                kind: ScriptKind::Session,
            })
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            runtime.tick(Duration::from_millis(1));
            if runtime.snapshot().scripting.scripts[0].kind == ScriptKind::Session {
                break;
            }
            assert!(Instant::now() < deadline, "script conversion timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        std::fs::remove_dir_all(temporary.path()).unwrap();

        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let output = runtime.take_file_output().unwrap();
        let saved = decode_session(&output.bytes).unwrap();
        let script = &saved.document.scripts[0];
        let resources = &saved.scripts[&script.id];
        assert_eq!(
            resources.entrypoint_resource().bytes.as_ref(),
            source.as_bytes()
        );
        assert_eq!(
            resources
                .get(
                    &shoop_script_resources::NormalizedRelativePath::parse("help/readme.md")
                        .unwrap()
                )
                .unwrap()
                .bytes
                .as_ref(),
            markdown.as_bytes()
        );
        assert!(resources
            .get(
                &shoop_script_resources::NormalizedRelativePath::parse("help/images/icon.png")
                    .unwrap()
            )
            .is_some());

        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "portable.shoop".to_owned(),
                bytes: Arc::clone(&output.bytes),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let snapshot = runtime.snapshot();
        let dialog = snapshot
            .scripting
            .dialogs
            .iter()
            .find(|dialog| dialog.name == "Help")
            .unwrap_or_else(|| panic!("missing Help dialog: {:?}", snapshot.scripting.scripts));
        let shoop_app_api::ScriptDialogKind::Simple(content) = &dialog.kind else {
            panic!("expected simple dialog")
        };
        let shoop_app_api::ScriptDialogElement::Markdown {
            resource_base_uri: Some(base_uri),
            ..
        } = &content.elements[0]
        else {
            panic!("expected provider-backed Markdown")
        };
        let image =
            shoop_script_resources::read_resource_uri(&format!("{base_uri}images/icon.png"))
                .unwrap()
                .unwrap();
        assert_eq!(
            image.as_ref(),
            include_bytes!("../../../../resources/logo-small.png")
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn scripts_export_exact_source_and_convert_session_ownership() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        let source = "shoop_announce_api_version(1, 5)\nprint('future')";
        runtime
            .dispatch(AppIntent::AddScriptSource {
                name: "future.lua".to_owned(),
                source: Arc::from(source),
                kind: ScriptKind::Bundled,
                enabled: true,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let script = &runtime.snapshot().scripting.scripts[0];
        let script_id = script.id;
        assert_eq!(
            script.lifecycle,
            shoop_app_api::ScriptLifecycle::Incompatible
        );

        runtime
            .dispatch(AppIntent::ExportScript { script_id })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let output = runtime.take_file_output().unwrap();
        assert_eq!(output.suggested_name, "future.lua");
        assert_eq!(output.mime_type, "text/x-lua");
        assert_eq!(&*output.bytes, source.as_bytes());

        runtime
            .dispatch(AppIntent::ConvertScriptKind {
                script_id,
                kind: ScriptKind::Session,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().scripting.scripts[0].kind,
            ScriptKind::Session
        );
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let saved = decode_session(&runtime.take_file_output().unwrap().bytes).unwrap();
        assert_eq!(saved.document.scripts.len(), 1);
        assert_eq!(
            saved.scripts[&saved.document.scripts[0].id]
                .entrypoint_resource()
                .bytes
                .as_ref(),
            source.as_bytes()
        );

        runtime
            .dispatch(AppIntent::ConvertScriptKind {
                script_id,
                kind: ScriptKind::Ephemeral,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().scripting.scripts[0].kind,
            ScriptKind::Ephemeral
        );
        runtime
            .dispatch(AppIntent::ConvertScriptKind {
                script_id,
                kind: ScriptKind::Session,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::RemoveSessionScript { script_id })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert!(runtime.snapshot().scripting.scripts.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
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
        assert!(runtime.snapshot().io_task.is_none());
        runtime
            .dispatch(AppIntent::Loop {
                track_id: persistent_track,
                loop_id,
                action: LoopAction::StopClicked,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        runtime
            .dispatch(AppIntent::SetTrackLatencyPolicy {
                track_id: persistent_track,
                policy: shoop_app_api::TrackLatencyPolicyState {
                    cue_followed: true,
                    cue_output: None,
                    components: Arc::from([shoop_app_api::LatencyComponentPolicyState {
                        kind: shoop_app_api::LatencyComponentKind::Manual,
                        enabled: true,
                        value_mode: shoop_app_api::LatencyValueMode::Manual(12),
                        range_selection: LatencyRangeSelectionState::Maximum,
                    }]),
                    revision: 7,
                    pending: false,
                    error: None,
                },
            })
            .unwrap();
        runtime
            .dispatch(AppIntent::SetTakeLatencyPolicy {
                loop_id,
                capture_alignment_frames: 6,
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
        assert_eq!(
            saved.document.track_groups[1].tracks[0]
                .latency_policy
                .components[0]
                .value,
            LatencyValueDocument::Manual { frames: 12 }
        );
        assert!(saved.document.track_groups[1].tracks[0].loops[0]
            .channels
            .iter()
            .all(|channel| channel.latency.capture_alignment_frames == 6));

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
        runtime.tick(Duration::ZERO);
        let loaded = runtime.snapshot();
        assert!(loaded.global_controls.solo);
        assert!(loaded.tracks.iter().any(|track| {
            track.name == "Persistent" && (track.controls.output_gain_db + 6.0).abs() < 0.001
        }));
        let persistent = loaded
            .tracks
            .iter()
            .find(|track| track.name == "Persistent")
            .unwrap();
        assert_eq!(
            persistent.latency_policy.components[0].value_mode,
            shoop_app_api::LatencyValueMode::Manual(12)
        );
        assert_eq!(
            persistent.loops[0].latency.capture_alignment_frames,
            6,
            "loaded loop latencies: {:?}",
            persistent
                .loops
                .iter()
                .map(|loop_| loop_.latency.capture_alignment_frames)
                .collect::<Vec<_>>()
        );

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
            .io_task
            .as_ref()
            .unwrap()
            .message
            .contains("unsupported file format"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn global_fx_port_round_trips_legacy_migrates_and_malformed_load_is_transactional() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.tick(Duration::ZERO);
        let global = runtime
            .snapshot()
            .connections
            .application_ports
            .iter()
            .find(|port| port.owner == ApplicationPortOwner::GlobalFxControl)
            .unwrap()
            .clone();
        assert_eq!(global.name, "Global FX Control MIDI In");
        assert_eq!(global.connection_policy, ConnectionPolicy::UserManaged);
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        for _ in 0..12 {
            runtime.tick(Duration::ZERO);
            if runtime.snapshot().io_task.as_ref().is_some_and(|task| {
                task.kind == IoTaskKind::SaveSession && task.status == IoTaskStatus::Completed
            }) {
                break;
            }
        }
        let saved = runtime.take_file_output().unwrap();
        let bundle = decode_session(&saved.bytes).unwrap();
        assert_eq!(bundle.document.global_ports.len(), 1);
        let document = &bundle.document.global_ports[0];
        assert_eq!(document.id, global.id.raw());
        assert_eq!(document.ringbuffer_frames, 0);
        assert!(document.internal_connections.is_empty());

        let mut legacy = bundle.clone();
        legacy.document.global_ports.clear();
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "legacy.shoop".to_owned(),
                bytes: Arc::from(encode_session(&legacy, "legacy").unwrap()),
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
        runtime.tick(Duration::ZERO);
        let migrated = runtime.snapshot();
        assert_eq!(
            migrated
                .connections
                .application_ports
                .iter()
                .filter(|port| port.owner == ApplicationPortOwner::GlobalFxControl)
                .count(),
            1,
            "task={:?}, ports={:?}",
            migrated.io_task,
            migrated.connections.application_ports
        );
        assert!(!migrated.connections.confirmed_links.iter().any(|link| {
            migrated
                .connections
                .application_ports
                .iter()
                .find(|port| port.id == link.application_port_id)
                .is_some_and(|port| port.owner == ApplicationPortOwner::GlobalFxControl)
        }));

        let before_tracks = migrated
            .tracks
            .iter()
            .map(|track| track.id)
            .collect::<Vec<_>>();
        let mut malformed = bundle;
        malformed
            .document
            .global_ports
            .push(malformed.document.global_ports[0].clone());
        runtime
            .dispatch(AppIntent::LoadSessionBytes {
                name: "malformed.shoop".to_owned(),
                bytes: Arc::from(encode_session(&malformed, "malformed").unwrap()),
            })
            .unwrap();
        for _ in 0..12 {
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
            before_tracks
        );
        assert_eq!(after.io_task.as_ref().unwrap().status, IoTaskStatus::Failed);
    }

    #[shoop_wasm_test_support::shoop_test]
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
                latency_policy: TrackLatencyPolicyDocument::default(),
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
        assert!(after
            .io_task
            .as_ref()
            .unwrap()
            .message
            .contains("unsupported trigger topology"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn logical_and_raw_audio_exports_preserve_take_timing_without_mutation() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model =
            ApplicationModel::initialize(&mut backend, Arc::clone(&files), previews, false)
                .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Export latency".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let loop_id = model.tracks[1].loops[0];
        let backend_loop = model.loops[&loop_id].backend_id;
        let latency = BackendTakeLatencySnapshot {
            capture_alignment_frames: 2,
            observation_min_frames: Some(2),
            observation_max_frames: Some(2),
            certainty: BackendLatencyCertainty::Exact,
            observation_sample_rate: 48_000,
            observation_revision: 1,
            ..Default::default()
        };
        backend
            .replace_loop_content(
                backend_loop,
                &BackendLoopContentUpdate {
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![9.0, 9.0, 1.0, 2.0, 3.0],
                        start_offset: Some(0),
                        preplay: Some(0),
                        latency: Some(latency.clone()),
                    }],
                    length: Some(3),
                    ..Default::default()
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());

        fn export(
            model: &mut ApplicationModel,
            backend: &mut FakeBackend,
            loop_id: LoopId,
            format: LoopAudioExportFormat,
        ) -> LoopAudio {
            model.handle_intent(
                backend,
                AppIntent::RequestLoopAudioExport { loop_id, format },
            );
            let task = model.io_task.clone().unwrap();
            model.handle_intent(
                backend,
                AppIntent::ConfirmAudioChannelSelection {
                    task_id: task.id,
                    channels: vec![0],
                },
            );
            for _ in 0..4 {
                model.advance_io(backend);
            }
            decode_loop_audio(
                &model
                    .file_outputs
                    .lock()
                    .unwrap()
                    .pop_front()
                    .unwrap()
                    .bytes,
            )
            .unwrap()
        }

        let logical = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopAudioExportFormat::Exact,
        );
        assert_eq!(logical.channels[0].samples, vec![1.0, 2.0, 3.0]);
        assert_eq!(logical.channels[0].latency, TakeLatencyDocument::default());
        let raw = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopAudioExportFormat::RawExact,
        );
        assert_eq!(raw.channels[0].samples, vec![9.0, 9.0, 1.0, 2.0, 3.0]);
        assert_eq!(raw.channels[0].latency.capture_alignment_frames, 2);
        assert_eq!(
            backend.loop_audio_data(backend_loop).unwrap().unwrap()[0].as_ref(),
            [9.0, 9.0, 1.0, 2.0, 3.0]
        );

        model.handle_intent(&mut backend, AppIntent::RequestSaveSession);
        for _ in 0..1_000 {
            model.advance_io(&mut backend);
            if !model.file_outputs.lock().unwrap().is_empty() {
                break;
            }
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::yield_now();
        }
        let saved = model.file_outputs.lock().unwrap().pop_front().unwrap();
        model.handle_intent(
            &mut backend,
            AppIntent::LoadSessionBytes {
                name: "latency-roundtrip.shoop".to_owned(),
                bytes: saved.bytes,
            },
        );
        for _ in 0..12 {
            model.advance_io(&mut backend);
        }
        assert_eq!(
            model.io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );
        let reloaded_logical = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopAudioExportFormat::Exact,
        );
        let reloaded_raw = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopAudioExportFormat::RawExact,
        );
        assert_eq!(reloaded_logical, logical);
        assert_eq!(reloaded_raw, raw);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn logical_and_raw_midi_exports_preserve_state_and_provenance() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model =
            ApplicationModel::initialize(&mut backend, Arc::clone(&files), previews, false)
                .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "MIDI export latency".to_owned(),
                    audio_channels: 0,
                    midi: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let loop_id = model.tracks[1].loops[0];
        let backend_loop = model.loops[&loop_id].backend_id;
        let latency = BackendTakeLatencySnapshot {
            capture_alignment_frames: 9_600,
            observation_min_frames: Some(9_600),
            observation_max_frames: Some(9_600),
            certainty: BackendLatencyCertainty::Exact,
            observation_sample_rate: 48_000,
            observation_revision: 1,
            ..Default::default()
        };
        let raw_events = vec![
            BackendMidiEvent {
                time: 4_800,
                data: vec![0xB0, 7, 20],
            },
            BackendMidiEvent {
                time: 9_600,
                data: vec![0x90, 60, 100],
            },
            BackendMidiEvent {
                time: 19_200,
                data: vec![0x80, 60, 64],
            },
        ];
        backend
            .replace_loop_content(
                backend_loop,
                &BackendLoopContentUpdate {
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 28_800,
                        start_state: vec![vec![0xB0, 7, 10]],
                        events: raw_events.clone(),
                        start_offset: Some(0),
                        preplay: Some(0),
                        latency: Some(latency),
                    }],
                    length: Some(19_200),
                    ..Default::default()
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());

        fn export(
            model: &mut ApplicationModel,
            backend: &mut FakeBackend,
            loop_id: LoopId,
            format: LoopMidiExportFormat,
        ) -> ExactMidi {
            model.handle_intent(
                backend,
                AppIntent::RequestLoopMidiExport { loop_id, format },
            );
            for _ in 0..4 {
                model.advance_io(backend);
            }
            let bytes = model
                .file_outputs
                .lock()
                .unwrap()
                .pop_front()
                .unwrap()
                .bytes;
            match format {
                LoopMidiExportFormat::Exact => decode_exact_midi(&bytes).unwrap(),
                LoopMidiExportFormat::Standard | LoopMidiExportFormat::RawStandard => {
                    decode_standard_midi(&bytes, 48_000).unwrap()
                }
            }
        }

        let logical = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopMidiExportFormat::Standard,
        );
        assert_eq!(logical.length_frames, 19_200);
        assert!(logical.start_state.is_empty());
        assert_eq!(logical.events[0].data, vec![0xB0, 7, 20]);
        assert_eq!(
            logical
                .events
                .iter()
                .map(|event| event.frame)
                .collect::<Vec<_>>(),
            vec![0, 0, 9_600]
        );
        assert_eq!(logical.latency, TakeLatencyDocument::default());
        let raw = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopMidiExportFormat::Exact,
        );
        assert_eq!(raw.length_frames, 28_800);
        assert_eq!(raw.start_state, vec![vec![0xB0, 7, 10]]);
        assert_eq!(raw.events.len(), 3);
        assert_eq!(raw.latency.capture_alignment_frames, 9_600);
        let raw_standard = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopMidiExportFormat::RawStandard,
        );
        assert_eq!(raw_standard.length_frames, 28_800);
        assert_eq!(
            raw_standard
                .events
                .iter()
                .map(|event| event.frame)
                .collect::<Vec<_>>(),
            vec![0, 4_800, 9_600, 19_200]
        );
        let unchanged = backend.loop_midi_data(backend_loop).unwrap().unwrap();
        assert_eq!(unchanged.channels[0].events, raw_events);

        model.handle_intent(&mut backend, AppIntent::RequestSaveSession);
        for _ in 0..1_000 {
            model.advance_io(&mut backend);
            if !model.file_outputs.lock().unwrap().is_empty() {
                break;
            }
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::yield_now();
        }
        let saved = model.file_outputs.lock().unwrap().pop_front().unwrap();
        model.handle_intent(
            &mut backend,
            AppIntent::LoadSessionBytes {
                name: "midi-latency-roundtrip.shoop".to_owned(),
                bytes: saved.bytes,
            },
        );
        for _ in 0..12 {
            model.advance_io(&mut backend);
        }
        assert_eq!(
            model.io_task.as_ref().unwrap().status,
            IoTaskStatus::Completed
        );
        let reloaded_logical = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopMidiExportFormat::Standard,
        );
        let reloaded_raw = export(
            &mut model,
            &mut backend,
            loop_id,
            LoopMidiExportFormat::Exact,
        );
        assert_eq!(reloaded_logical, logical);
        assert_eq!(reloaded_raw, raw);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn standard_imports_default_to_unknown_zero_provenance_and_accept_manual_offsets() {
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Import provenance".to_owned(),
                    audio_channels: 1,
                    midi: true,
                },
            )
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let loop_id = model.tracks[1].loops[0];
        let backend_loop = model.loops[&loop_id].backend_id;
        let audio = LoopAudio {
            sample_rate: 48_000,
            channels: vec![LoopAudioChannel {
                label: "input".to_owned(),
                role: "direct".to_owned(),
                samples: vec![1.0, 2.0, 3.0, 4.0],
                latency: TakeLatencyDocument::default(),
            }],
        };
        let wav = encode_float_wav(&audio).unwrap();

        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "plain.wav".to_owned(),
                bytes: Arc::from(wav.clone()),
                update_loop_length: true,
                manual_offset_frames: None,
            },
        );
        let task = model.io_task.clone().unwrap();
        model.handle_intent(
            &mut backend,
            AppIntent::ConfirmAudioChannelMapping {
                task_id: task.id,
                source_for_destination: vec![0],
            },
        );
        for _ in 0..4 {
            model.advance_io(&mut backend);
        }
        let captured = backend.capture_session().unwrap();
        let content = captured
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == backend_loop.raw())
            .unwrap();
        assert_eq!(content.audio[0].latency.capture_alignment_frames, 0);
        assert_eq!(
            content.audio[0].latency.certainty,
            BackendLatencyCertainty::Unknown
        );

        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "manual.wav".to_owned(),
                bytes: Arc::from(wav),
                update_loop_length: true,
                manual_offset_frames: Some(2),
            },
        );
        let task = model.io_task.clone().unwrap();
        model.handle_intent(
            &mut backend,
            AppIntent::ConfirmAudioChannelMapping {
                task_id: task.id,
                source_for_destination: vec![0],
            },
        );
        for _ in 0..4 {
            model.advance_io(&mut backend);
        }

        let midi = ExactMidi {
            sample_rate: 48_000,
            length_frames: 9_600,
            start_state: vec![vec![0xB0, 7, 100]],
            events: vec![ExactMidiEvent {
                frame: 4_800,
                order: 0,
                data: vec![0x90, 60, 100],
            }],
            latency: TakeLatencyDocument::default(),
        };
        let standard = encode_standard_midi(&midi).unwrap().bytes;
        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "manual.mid".to_owned(),
                bytes: Arc::from(standard),
                update_loop_length: true,
                manual_offset_frames: Some(-3),
            },
        );
        for _ in 0..4 {
            model.advance_io(&mut backend);
        }
        let captured = backend.capture_session().unwrap();
        let content = captured
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == backend_loop.raw())
            .unwrap();
        assert_eq!(content.audio[0].latency.capture_alignment_frames, 2);
        assert_eq!(
            content.audio[0].latency.certainty,
            BackendLatencyCertainty::ManualOnly
        );
        assert_eq!(content.midi[0].latency.capture_alignment_frames, -3);
        assert_eq!(
            content.midi[0].latency.certainty,
            BackendLatencyCertainty::ManualOnly
        );

        let empty_audio = LoopAudio {
            sample_rate: 48_000,
            channels: vec![LoopAudioChannel {
                label: "empty".to_owned(),
                role: "direct".to_owned(),
                samples: Vec::new(),
                latency: TakeLatencyDocument::default(),
            }],
        };
        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "empty.wav".to_owned(),
                bytes: Arc::from(encode_float_wav(&empty_audio).unwrap()),
                update_loop_length: true,
                manual_offset_frames: Some(7),
            },
        );
        let task = model.io_task.clone().unwrap();
        model.handle_intent(
            &mut backend,
            AppIntent::ConfirmAudioChannelMapping {
                task_id: task.id,
                source_for_destination: vec![0],
            },
        );
        for _ in 0..4 {
            model.advance_io(&mut backend);
        }
        let empty_midi = ExactMidi {
            sample_rate: 48_000,
            length_frames: 0,
            start_state: Vec::new(),
            events: Vec::new(),
            latency: TakeLatencyDocument::default(),
        };
        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "empty.mid".to_owned(),
                bytes: Arc::from(encode_standard_midi(&empty_midi).unwrap().bytes),
                update_loop_length: true,
                manual_offset_frames: Some(-5),
            },
        );
        for _ in 0..4 {
            model.advance_io(&mut backend);
        }
        let empty_capture = backend.capture_session().unwrap();
        let empty_content = empty_capture
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == backend_loop.raw())
            .unwrap();
        assert!(empty_content.audio[0].samples.is_empty());
        assert_eq!(empty_content.audio[0].latency.capture_alignment_frames, 7);
        assert!(empty_content.midi[0].events.is_empty());
        assert_eq!(empty_content.midi[0].latency.capture_alignment_frames, -5);

        let before_rejection = empty_capture;
        model.handle_intent(
            &mut backend,
            AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "overflow.wav".to_owned(),
                bytes: Arc::from(encode_float_wav(&audio).unwrap()),
                update_loop_length: true,
                manual_offset_frames: Some(shoop_latency::MAX_COMPENSATION_FRAMES as i32 + 1),
            },
        );
        assert_eq!(model.io_task.as_ref().unwrap().status, IoTaskStatus::Failed);
        assert_eq!(backend.capture_session().unwrap(), before_rejection);
    }

    #[shoop_wasm_test_support::shoop_test]
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
                    manual_offset_frames: None,
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
                manual_offset_frames: None,
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().status,
            IoTaskStatus::Failed
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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
                    latency: TakeLatencyDocument::default(),
                },
                LoopAudioChannel {
                    label: "b".to_owned(),
                    role: "direct".to_owned(),
                    samples: vec![0.5; 256],
                    latency: TakeLatencyDocument::default(),
                },
            ],
        };
        runtime
            .dispatch(AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "input.shoop-audio".to_owned(),
                bytes: Arc::from(encode_loop_audio(&audio).unwrap()),
                update_loop_length: true,
                manual_offset_frames: None,
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
                        latency: TakeLatencyDocument::default(),
                    })
                    .collect(),
            };
            runtime
                .dispatch(AppIntent::ImportLoopAudioBytes {
                    loop_id,
                    name: "mapping.shoop-audio".to_owned(),
                    bytes: Arc::from(encode_loop_audio(&audio).unwrap()),
                    update_loop_length: true,
                    manual_offset_frames: None,
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
                manual_offset_frames: None,
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
            latency: TakeLatencyDocument::default(),
        };
        runtime
            .dispatch(AppIntent::ImportLoopMidiBytes {
                loop_id,
                name: "input.shoop-midi".to_owned(),
                bytes: Arc::from(encode_exact_midi(&midi).unwrap()),
                update_loop_length: true,
                manual_offset_frames: None,
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
                format: LoopMidiExportFormat::Exact,
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
                format: LoopMidiExportFormat::Standard,
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
                manual_offset_frames: None,
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
                format: LoopMidiExportFormat::Exact,
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

    #[shoop_wasm_test_support::shoop_test]
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
                min_dry_audio_channels: None,
                max_dry_audio_channels: None,
                min_wet_audio_channels: None,
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
                    latency: TakeLatencyDocument::default(),
                })
                .collect(),
        };
        runtime
            .dispatch(AppIntent::ImportLoopAudioBytes {
                loop_id,
                name: "roles.shoop-audio".to_owned(),
                bytes: Arc::from(encode_loop_audio(&input).unwrap()),
                update_loop_length: true,
                manual_offset_frames: None,
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

    #[shoop_wasm_test_support::shoop_test]
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
                format: LoopMidiExportFormat::Exact,
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn periodic_failures_are_reported_once_until_recovery() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();

        assert!(model.report_periodic_failure("test.operation", "first failure".to_owned()));
        assert!(!model.report_periodic_failure("test.operation", "repeated failure".to_owned()));
        assert!(model.active_periodic_failures.contains("test.operation"));

        model.clear_periodic_failure("test.operation");
        assert!(model.report_periodic_failure("test.operation", "new failure".to_owned()));
        assert!(model.active_periodic_failures.contains("test.operation"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn io_task_failures_update_only_the_matching_task() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime.dispatch(AppIntent::RequestSaveSession).unwrap();
        runtime.tick(Duration::ZERO);
        let task_id = runtime.snapshot().io_task.as_ref().unwrap().id;

        runtime
            .dispatch(AppIntent::FailIoTask {
                task_id,
                message: "injected save failure".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let failed = runtime.snapshot();
        assert_eq!(
            failed.io_task.as_ref().unwrap().status,
            IoTaskStatus::Failed
        );
        assert_eq!(
            failed.io_task.as_ref().unwrap().message,
            "injected save failure"
        );

        runtime
            .dispatch(AppIntent::FailIoTask {
                task_id: TaskId::from_raw(task_id.raw().wrapping_add(1)),
                message: "stale failure".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        assert_eq!(
            runtime.snapshot().io_task.as_ref().unwrap().message,
            "injected save failure"
        );

        runtime
            .dispatch(AppIntent::FailIoWorkflow {
                kind: IoTaskKind::LoadSession,
                message: "injected read failure".to_owned(),
            })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let workflow_failure = runtime.snapshot();
        let task = workflow_failure.io_task.as_ref().unwrap();
        assert_eq!(task.kind, IoTaskKind::LoadSession);
        assert_eq!(task.status, IoTaskStatus::Failed);
        assert_eq!(task.message, "injected read failure");
    }

    #[shoop_wasm_test_support::shoop_test]
    fn render_memory_growth_is_a_recoverable_warning() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();

        let mut snapshot = BackendSnapshot::default();
        snapshot.status.render_memory_growths = 1;
        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.render_memory_growths, 1);

        model.apply_backend_snapshot(snapshot.clone());
        assert_eq!(model.status.render_memory_growths, 1);

        snapshot.status.render_memory_growths = 3;
        model.apply_backend_snapshot(snapshot);
        assert_eq!(model.status.render_memory_growths, 3);
    }

    #[shoop_wasm_test_support::shoop_test]
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
                            latency: None,
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
                        latency: None,
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
        assert!(target.audio.iter().all(|channel| {
            channel.start_offset == 0
                && channel.preplay == 0
                && channel.latency == BackendTakeLatencySnapshot::default()
        }));
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
        assert_eq!(
            target.midi[0].latency,
            BackendTakeLatencySnapshot::default()
        );

        let sync_id = model.tracks[0].loops[0];
        model
            .begin_generate_click_track(sync_id, ClickTrackRequest::default())
            .unwrap();
        model.advance_io(&mut backend);
        model.advance_io(&mut backend);
        assert_eq!(model.loops[&sync_id].length, 115_200);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn structural_creation_and_removal_are_provisional_and_rejection_recovers() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Provisional".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let track_id = model.tracks.last().unwrap().id;
        let backend_id = model.tracks.last().unwrap().backend_id;
        assert_eq!(
            model.tracks.last().unwrap().structural_state,
            StructuralState::Creating
        );
        let confirmed = backend.poll().unwrap();
        model.apply_backend_snapshot(confirmed.clone());
        assert_eq!(
            model.tracks.last().unwrap().structural_state,
            StructuralState::Confirmed
        );

        model
            .handle_track_action(&mut backend, track_id, TrackAction::Remove)
            .unwrap();
        assert_eq!(
            model.tracks.last().unwrap().structural_state,
            StructuralState::Removing
        );
        assert!(model.tracks.last().unwrap().loops.iter().all(|loop_id| {
            model.loops[loop_id].state.structural_state == StructuralState::Removing
        }));
        let mut rejected = confirmed;
        rejected
            .mutation_failures
            .push(shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 2,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackStructure,
                entity: Some(backend_id.raw()),
                detail: Some(BackendMutationDetail::TrackRemoval),
                message: "removal rejected".to_owned(),
            });
        model.apply_backend_snapshot(rejected);
        assert_eq!(
            model.tracks.last().unwrap().structural_state,
            StructuralState::Confirmed
        );
        assert!(model.tracks.last().unwrap().loops.iter().all(|loop_id| {
            model.loops[loop_id].state.structural_state == StructuralState::Confirmed
        }));

        let stale_index = model
            .tracks
            .iter()
            .position(|track| track.id == track_id)
            .unwrap();
        model.remove_track_model(stale_index);
        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Loop rejection".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let loop_track_id = model.tracks.last().unwrap().id;
        model.apply_backend_snapshot(backend.poll().unwrap());
        let previous_loop_count = model.tracks.last().unwrap().loops.len();
        model
            .add_aligned_loop_row(&mut backend, loop_track_id)
            .unwrap();
        let added_loop_id = *model.tracks.last().unwrap().loops.last().unwrap();
        let added_backend_loop = model.loops[&added_loop_id].backend_id;
        let mut loop_rejected = backend.poll().unwrap();
        loop_rejected.loops.remove(&added_backend_loop);
        loop_rejected
            .mutation_failures
            .push(shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 3,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackStructure,
                entity: Some(backend_id.raw()),
                detail: Some(BackendMutationDetail::LoopCreation {
                    loop_id: added_backend_loop,
                }),
                message: "loop creation rejected".to_owned(),
            });
        model.apply_backend_snapshot(loop_rejected);
        assert_eq!(
            model.tracks.last().unwrap().loops.len(),
            previous_loop_count
        );
        assert!(!model.loops.contains_key(&added_loop_id));

        model
            .add_track(
                &mut backend,
                DirectTrackSpec {
                    name: "Rejected creation".to_owned(),
                    audio_channels: 1,
                    midi: false,
                },
            )
            .unwrap();
        let rejected_track_id = model.tracks.last().unwrap().id;
        let rejected_backend_track = model.tracks.last().unwrap().backend_id;
        let rejected_backend_loops = model
            .tracks
            .last()
            .unwrap()
            .loops
            .iter()
            .map(|loop_id| model.loops[loop_id].backend_id)
            .collect::<Vec<_>>();
        let mut creation_rejected = backend.poll().unwrap();
        creation_rejected.tracks.remove(&rejected_backend_track);
        for loop_id in rejected_backend_loops {
            creation_rejected.loops.remove(&loop_id);
        }
        creation_rejected
            .mutation_failures
            .push(shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 4,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackStructure,
                entity: Some(rejected_backend_track.raw()),
                detail: Some(BackendMutationDetail::TrackCreation),
                message: "track creation rejected".to_owned(),
            });
        model.apply_backend_snapshot(creation_rejected);
        assert!(model
            .tracks
            .iter()
            .all(|track| track.id != rejected_track_id));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn desired_controls_survive_stale_snapshots_ignore_stale_rejection_and_converge() {
        let mut backend = FakeBackend::default();
        let files = Arc::new(Mutex::new(VecDeque::new()));
        let previews = Arc::new(Mutex::new(VecDeque::new()));
        let mut model = ApplicationModel::initialize(&mut backend, files, previews, false).unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        let track_id = model.tracks[0].id;
        let backend_track = model.tracks[0].backend_id;
        let loop_id = model.tracks[0].loops[0];
        let backend_loop = model.loops[&loop_id].backend_id;

        model.desired_track_controls.insert(
            (backend_track, TrackControlKey::OutputGain),
            BackendTrackControl::OutputGainDb(-6.0),
        );
        model
            .desired_loop_controls
            .insert((backend_loop, LoopControlKey::Gain), 0.7);
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(model.tracks[0].controls.output_gain_db, -6.0);
        assert_eq!(model.loops[&loop_id].state.gain, 0.7);

        model.desired_track_controls.insert(
            (backend_track, TrackControlKey::OutputGain),
            BackendTrackControl::OutputGainDb(-12.0),
        );
        let mut stale_rejection = backend.poll().unwrap();
        stale_rejection
            .mutation_failures
            .push(shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 9,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackControl,
                entity: Some(backend_track.raw()),
                detail: Some(BackendMutationDetail::TrackControl(
                    BackendTrackControl::OutputGainDb(-6.0),
                )),
                message: "older gain rejected".to_owned(),
            });
        model.apply_backend_snapshot(stale_rejection);
        assert_eq!(model.tracks[0].controls.output_gain_db, -12.0);

        let mut latest_rejection = backend.poll().unwrap();
        latest_rejection
            .mutation_failures
            .push(shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 10,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackControl,
                entity: Some(backend_track.raw()),
                detail: Some(BackendMutationDetail::TrackControl(
                    BackendTrackControl::OutputGainDb(-12.0),
                )),
                message: "latest gain rejected".to_owned(),
            });
        model.apply_backend_snapshot(latest_rejection);
        assert_eq!(model.tracks[0].id, track_id);
        assert_eq!(model.tracks[0].controls.output_gain_db, 0.0);
        assert!(!model
            .desired_track_controls
            .contains_key(&(backend_track, TrackControlKey::OutputGain)));

        let fx_rejected = BackendTrackFxControl::SetVisible(true);
        model
            .desired_loop_controls
            .insert((backend_loop, LoopControlKey::Gain), 0.5);
        model
            .desired_loop_controls
            .insert((backend_loop, LoopControlKey::Balance), -0.25);
        model
            .desired_fx_controls
            .insert((backend_track, FxControlKey::Visible), fx_rejected.clone());
        let mut control_rejections = backend.poll().unwrap();
        control_rejections.mutation_failures.extend([
            shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 11,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::LoopControl,
                entity: Some(backend_loop.raw()),
                detail: Some(BackendMutationDetail::LoopGain(0.5)),
                message: "loop gain rejected".to_owned(),
            },
            shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 12,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::LoopControl,
                entity: Some(backend_loop.raw()),
                detail: Some(BackendMutationDetail::LoopBalance(-0.25)),
                message: "loop balance rejected".to_owned(),
            },
            shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 13,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::TrackFxControl,
                entity: Some(backend_track.raw()),
                detail: Some(BackendMutationDetail::TrackFxControl(fx_rejected)),
                message: "FX visibility rejected".to_owned(),
            },
            shoop_backend::BackendMutationFailure {
                driver_generation: 1,
                sequence: 14,
                operation_key: None,
                kind: shoop_backend::BackendMutationKind::SessionTransfer,
                entity: None,
                detail: None,
                message: "uncorrelated operation rejected".to_owned(),
            },
        ]);
        model.apply_backend_snapshot(control_rejections);
        assert!(!model
            .desired_loop_controls
            .contains_key(&(backend_loop, LoopControlKey::Gain)));
        assert!(!model
            .desired_loop_controls
            .contains_key(&(backend_loop, LoopControlKey::Balance)));
        assert!(!model
            .desired_fx_controls
            .contains_key(&(backend_track, FxControlKey::Visible)));

        model.desired_track_controls.insert(
            (backend_track, TrackControlKey::OutputGain),
            BackendTrackControl::OutputGainDb(-3.0),
        );
        backend
            .set_track_control(backend_track, BackendTrackControl::OutputGainDb(-3.0))
            .unwrap();
        model.apply_backend_snapshot(backend.poll().unwrap());
        assert_eq!(model.tracks[0].controls.output_gain_db, -3.0);
        assert!(!model
            .desired_track_controls
            .contains_key(&(backend_track, TrackControlKey::OutputGain)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_smoothing_intent_routes_to_backend() {
        let mut backend = FakeBackend::default();
        let mut model = ApplicationModel::initialize(
            &mut backend,
            Arc::new(Mutex::new(VecDeque::new())),
            Arc::new(Mutex::new(VecDeque::new())),
            false,
        )
        .unwrap();
        model.handle_intent(&mut backend, AppIntent::SetLoopSmoothingMs(23));
        assert_eq!(backend.loop_smoothing_ms(), Some(23));
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn successful_consolidation_invalidates_cached_waveform_and_midi_details() {
        let mut runtime =
            CooperativeApplicationRuntime::start(Box::new(FakeBackend::default())).unwrap();
        runtime
            .dispatch(AppIntent::AddTrack(DirectTrackSpec {
                name: "Cached latency".to_owned(),
                audio_channels: 1,
                midi: true,
            }))
            .unwrap();
        runtime.tick(Duration::ZERO);
        let loop_id = runtime.snapshot().tracks[1].loops[0].id;
        let model = runtime.model.loops.get_mut(&loop_id).unwrap();
        model.audio_data = Some(BackendAudioData {
            channels: vec![shoop_backend::BackendAudioChannelData {
                samples: Arc::from([1.0]),
                start_offset: 0,
                preplay: 0,
                latency: Default::default(),
            }],
        });
        model.midi_data = Some(Vec::new());

        runtime
            .dispatch(AppIntent::ConsolidateTakeLatency { loop_id })
            .unwrap();
        runtime.tick(Duration::ZERO);
        let model = runtime.model.loops.get(&loop_id).unwrap();
        assert!(model.audio_data.is_none());
        assert!(model.midi_data.is_none());
    }
}
