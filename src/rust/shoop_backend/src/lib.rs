#[cfg(all(feature = "native-drivers", not(target_arch = "wasm32")))]
mod native;
#[cfg(all(feature = "native-drivers", not(target_arch = "wasm32")))]
pub use native::NativeBackend;
#[cfg(all(feature = "native-fx", not(target_arch = "wasm32")))]
pub use native::{
    configure_carla_hosting_mode, configured_carla_hosting_mode, run_carla_worker_if_requested,
};
pub use shoop_app_api::{
    TinySynthFxControl, TinySynthFxState, TrackProcessorEditorState, TrackProcessorTypeId,
};

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverDescriptor, AudioDriverKind, AudioDriverRuntimeState,
    DummyAudioDriverConfig, FxLifecycle, ResolvedAudioDriverConfig, TrackFxState,
    TrackProcessorDescriptor,
};
use shoop_engine::dummy_midi_port::DummyMidiPort;
use shoop_engine::dummy_port::{DummyAudioPort, DummyExternalConnections, PortId};
use shoop_engine::external_audio_port::ExternalAudioPort;
use shoop_engine::external_midi_port::ExternalMidiPort;
use shoop_engine::internal_audio_port::InternalAudioPort;
use shoop_engine::session::{Port, Session};
use shoop_engine::{
    ChannelMode, LoopMode, MidiStorage, PortDataType as EnginePortDataType, PortDirection,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BackendLoopId(u64);

impl BackendLoopId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BackendTrackId(u64);

impl BackendTrackId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BackendPortId(u64);

impl BackendPortId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BackendPortDataType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BackendPortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BackendPortRole {
    AudioInput,
    AudioOutput,
    AudioSend,
    AudioReturn,
    MidiInput,
    MidiOutput,
    MidiSend,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendPortDescriptor {
    pub id: BackendPortId,
    pub name: String,
    pub data_type: BackendPortDataType,
    pub direction: BackendPortDirection,
    pub role: BackendPortRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendHostPortDescriptor {
    pub id: String,
    pub name: String,
    pub data_type: BackendPortDataType,
    pub direction: BackendPortDirection,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct BackendConfirmedLink {
    pub application_port_id: BackendPortId,
    pub host_port_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendConnectionFailure {
    pub port_id: BackendPortId,
    pub external_port: String,
    pub desired_connected: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendConnectionSnapshot {
    pub revision: u64,
    pub available: bool,
    /// Normalized application-owned ports, keyed by stable backend identity.
    pub application_ports: BTreeMap<BackendPortId, BackendPortDescriptor>,
    /// One normalized host inventory. An empty inventory is valid.
    pub host_ports: BTreeMap<String, BackendHostPortDescriptor>,
    /// Backend-confirmed links only; requested state is tracked by the application.
    pub confirmed_links: BTreeSet<BackendConfirmedLink>,
    pub failures: Vec<BackendConnectionFailure>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendTrackTopology {
    Direct {
        audio_channels: u32,
        midi: bool,
    },
    DryWetExternal {
        dry_audio_channels: u32,
        wet_audio_channels: u32,
        dry_midi: bool,
    },
    DryWetProcessor {
        processor_type: String,
        dry_audio_channels: u32,
        wet_audio_channels: u32,
        dry_midi: bool,
    },
}

impl Default for BackendTrackTopology {
    fn default() -> Self {
        Self::Direct {
            audio_channels: 0,
            midi: false,
        }
    }
}

impl BackendTrackTopology {
    pub const fn dry_audio_channels(&self) -> u32 {
        match self {
            Self::Direct { audio_channels, .. } => *audio_channels,
            Self::DryWetExternal {
                dry_audio_channels, ..
            }
            | Self::DryWetProcessor {
                dry_audio_channels, ..
            } => *dry_audio_channels,
        }
    }

    pub const fn wet_audio_channels(&self) -> u32 {
        match self {
            Self::Direct { audio_channels, .. } => *audio_channels,
            Self::DryWetExternal {
                wet_audio_channels, ..
            }
            | Self::DryWetProcessor {
                wet_audio_channels, ..
            } => *wet_audio_channels,
        }
    }

    pub const fn has_midi(&self) -> bool {
        match self {
            Self::Direct { midi, .. } => *midi,
            Self::DryWetExternal { dry_midi, .. } | Self::DryWetProcessor { dry_midi, .. } => {
                *dry_midi
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackRequest {
    pub port_name_base: String,
    pub topology: BackendTrackTopology,
    pub initial_loops: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectTrackRequest {
    pub port_name_base: String,
    pub audio_channels: u32,
    pub midi: bool,
    pub initial_loops: usize,
}

impl From<DirectTrackRequest> for TrackRequest {
    fn from(value: DirectTrackRequest) -> Self {
        Self {
            port_name_base: value.port_name_base,
            topology: BackendTrackTopology::Direct {
                audio_channels: value.audio_channels,
                midi: value.midi,
            },
            initial_loops: value.initial_loops,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendTrackCreation {
    pub track_id: BackendTrackId,
    pub loops: Vec<BackendLoopId>,
    pub ports: Vec<BackendPortDescriptor>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackendTrackControl {
    OutputGainDb(f32),
    OutputBalance(f32),
    OutputMute(bool),
    InputGainDb(f32),
    InputBalance(f32),
    InputMonitoring(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendTrackFxControl {
    SetActive(bool),
    SetVisible(bool),
    ToggleOrRecover,
    RestoreState(String),
    ClearLogs,
    TinySynthFx(TinySynthFxControl),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendTrackState {
    pub topology: BackendTrackTopology,
    #[serde(skip)]
    pub fx: Option<TrackFxState>,
    pub audio_channels: u32,
    pub midi: bool,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub input_monitoring: bool,
    pub input_peaks: Vec<f32>,
    pub output_peaks: Vec<f32>,
    pub input_midi_activity: bool,
    pub output_midi_activity: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendDriverState {
    #[default]
    Dummy,
    AwaitingGesture,
    RequestingPermission,
    Starting,
    Running,
    Suspended,
    Denied,
    Unsupported,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackendStatus {
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
    pub driver_state: BackendDriverState,
    pub callback_count: u64,
    pub processed_frames: u64,
    pub input_peak: f32,
    pub output_peak: f32,
    pub callback_budget_overruns: u32,
    pub render_discontinuities: u32,
    pub memory_growths: u32,
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendLoopMode {
    #[default]
    Unknown,
    Stopped,
    Playing,
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DryWetProcessorMapping {
    pub dry_audio: Vec<(u32, u32)>,
    pub wet_audio: Vec<(u32, u32)>,
    pub dry_midi: bool,
}

pub fn dry_wet_processor_mapping(
    dry_audio_channels: u32,
    wet_audio_channels: u32,
    dry_midi: bool,
    processor_audio_inputs: u32,
    processor_audio_outputs: u32,
    processor_has_midi_input: bool,
) -> DryWetProcessorMapping {
    DryWetProcessorMapping {
        dry_audio: (0..dry_audio_channels.min(processor_audio_inputs))
            .map(|index| (index, index))
            .collect(),
        wet_audio: (0..wet_audio_channels.min(processor_audio_outputs))
            .map(|index| (index, index))
            .collect(),
        dry_midi: dry_midi && processor_has_midi_input,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DryWetRoutingState {
    pub dry_input_passthrough_muted: bool,
    pub wet_output_passthrough_muted: bool,
    pub processor_active: bool,
    pub force_monitoring_off: bool,
}

pub fn dry_wet_routing_state(
    monitoring: bool,
    current_modes: &[BackendLoopMode],
    next_cycle_modes: &[BackendLoopMode],
) -> DryWetRoutingState {
    let recording = current_modes.iter().any(|mode| {
        matches!(
            mode,
            BackendLoopMode::Recording | BackendLoopMode::Replacing
        )
    });
    let pre_recording = next_cycle_modes.iter().any(|mode| {
        matches!(
            mode,
            BackendLoopMode::Recording | BackendLoopMode::Replacing
        )
    });
    let playing_dry = current_modes.contains(&BackendLoopMode::PlayingDryThroughWet);
    let rerecording = current_modes.contains(&BackendLoopMode::RecordingDryIntoWet);
    let pre_rerecording = next_cycle_modes.contains(&BackendLoopMode::RecordingDryIntoWet);
    DryWetRoutingState {
        dry_input_passthrough_muted: (!monitoring && !(recording || pre_recording))
            || rerecording
            || pre_rerecording,
        wet_output_passthrough_muted: !(monitoring
            || playing_dry
            || rerecording
            || pre_rerecording),
        processor_active: monitoring
            || recording
            || pre_recording
            || playing_dry
            || rerecording
            || pre_rerecording,
        force_monitoring_off: rerecording || pre_rerecording,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendLoopState {
    pub mode: BackendLoopMode,
    pub length: u32,
    pub position: u32,
    pub next_mode: Option<BackendLoopMode>,
    pub next_transition_delay: Option<u32>,
    pub stereo: bool,
    pub gain: f32,
    pub balance: f32,
    pub audio_peaks: Vec<f32>,
    pub midi_activity: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BackendGrabRequest {
    pub loop_id: BackendLoopId,
    pub reverse_start_cycle: Option<i32>,
    pub cycles_length: Option<i32>,
    pub go_to_cycle: Option<i32>,
    pub go_to_mode: BackendLoopMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendChannelMode {
    Direct,
    Dry,
    Wet,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendAudioContent {
    pub mode: BackendChannelMode,
    pub samples: Vec<f32>,
    pub gain: f32,
    pub start_offset: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendMidiEvent {
    pub time: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendMidiContent {
    pub mode: BackendChannelMode,
    pub length: u32,
    pub start_state: Vec<Vec<u8>>,
    pub events: Vec<BackendMidiEvent>,
    pub start_offset: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendLoopContent {
    pub source_id: u64,
    pub length: u32,
    pub gain: f32,
    pub balance: f32,
    pub audio: Vec<BackendAudioContent>,
    pub midi: Vec<BackendMidiContent>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionPort {
    pub source_id: u64,
    pub descriptor: BackendPortDescriptor,
    pub external_connections: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionTrack {
    pub source_id: u64,
    pub port_name_base: String,
    pub topology: BackendTrackTopology,
    pub state: BackendTrackState,
    pub loops: Vec<BackendLoopContent>,
    pub ports: Vec<BackendSessionPort>,
    pub processor_state: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionData {
    pub sample_rate: u32,
    pub tracks: Vec<BackendSessionTrack>,
    #[serde(default)]
    pub use_legacy_browser_default_routes: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionReplacement {
    pub tracks: BTreeMap<u64, BackendTrackCreation>,
    pub loops: BTreeMap<u64, BackendLoopId>,
    pub ports: BTreeMap<u64, BackendPortId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendAudioDataChunk {
    pub content_revision: u64,
    pub channel: usize,
    pub channel_count: usize,
    pub offset: usize,
    pub total_samples: usize,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendSnapshot {
    pub status: BackendStatus,
    pub audio_drivers: AudioDriverRuntimeState,
    pub tracks: BTreeMap<BackendTrackId, BackendTrackState>,
    pub loops: BTreeMap<BackendLoopId, BackendLoopState>,
    pub connections: BackendConnectionSnapshot,
}

pub trait Backend {
    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(Arc::from([]))
    }

    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        Ok(AudioDriverRuntimeState::default())
    }
    fn refresh_audio_driver_discovery(
        &mut self,
        _config: &AudioDriverConfig,
    ) -> Result<AudioDriverRuntimeState> {
        self.audio_driver_state()
    }
    fn preflight_audio_driver(
        &mut self,
        _config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        Err(anyhow!("audio-driver switching is unavailable"))
    }
    fn switch_audio_driver(
        &mut self,
        _config: &AudioDriverConfig,
        _confirmed_sample_rate: u32,
        _session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        Err(anyhow!("audio-driver switching is unavailable"))
    }
    fn create_loop(&mut self) -> Result<BackendLoopId>;
    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels,
                midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetExternal { .. } => {
                Err(anyhow!("External dry/wet topology is unavailable"))
            }
            BackendTrackTopology::DryWetProcessor { .. } => {
                Err(anyhow!("requested track processor is unavailable"))
            }
        }
    }
    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation>;
    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId>;
    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()>;
    fn inject_midi_input(
        &mut self,
        _track_id: BackendTrackId,
        _events: &[BackendMidiEvent],
    ) -> Result<()> {
        Err(anyhow!("MIDI input injection is unavailable"))
    }
    fn set_track_fx_control(
        &mut self,
        _track_id: BackendTrackId,
        _control: BackendTrackFxControl,
    ) -> Result<()> {
        Err(anyhow!("track FX controls are unavailable"))
    }
    fn track_fx_state_string(&mut self, _track_id: BackendTrackId) -> Result<Option<String>> {
        Ok(None)
    }
    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()>;
    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()>;
    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()>;
    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>>;
    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        let channels = self.loop_audio_data(loop_id)?.unwrap_or_default();
        let samples = channels
            .get(channel)
            .cloned()
            .unwrap_or_else(|| Arc::from([]));
        let end = offset.saturating_add(max_samples).min(samples.len());
        Ok(BackendAudioDataChunk {
            content_revision: 0,
            channel,
            channel_count: channels.len(),
            offset,
            total_samples: samples.len(),
            samples: if offset < end {
                samples[offset..end].to_vec()
            } else {
                Vec::new()
            },
        })
    }
    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()>;
    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()>;
    fn transition_loop_aligned(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    ) -> Result<()> {
        if align_to_sync_at.is_some() {
            return Err(anyhow!("aligned loop transitions are unavailable"));
        }
        self.transition_loop(loop_id, mode, cycles_delay)
    }
    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()>;
    fn capture_session(&mut self) -> Result<BackendSessionData> {
        Err(anyhow!("session capture is unavailable"))
    }
    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let _ = session;
        Err(anyhow!("session replacement is unavailable"))
    }
    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let _ = (port_id, external_port, connected);
        Err(anyhow!("external connection management is unavailable"))
    }
    fn advance(&mut self, elapsed: Duration);
    fn poll(&mut self) -> Result<BackendSnapshot>;
    fn wait_idle(&mut self);
}

const NANOSECONDS_PER_SECOND: u128 = 1_000_000_000;
const MAX_CYCLES_PER_ADVANCE: u32 = 8;
const MIDI_INPUT_INJECTION_CAPACITY: usize = 128;

fn validate_midi_input_events(events: &[BackendMidiEvent]) -> Result<()> {
    if events.len() > MIDI_INPUT_INJECTION_CAPACITY {
        return Err(anyhow!("MIDI input injection exceeds capacity"));
    }
    if events
        .iter()
        .any(|event| event.time != 0 || event.data.is_empty() || event.data.len() > 4)
    {
        return Err(anyhow!("invalid MIDI input injection event"));
    }
    Ok(())
}
pub const MAX_WEB_AUDIO_QUANTUM: u32 = 2048;
pub const RECORDING_CAPACITY_SECONDS: u32 = 10;
pub const WEB_MIDI_OUTPUT_QUEUE_CAPACITY: usize = 1024;

pub fn tiny_synth_fx_descriptor() -> TrackProcessorDescriptor {
    TrackProcessorDescriptor {
        id: TrackProcessorTypeId::new(TrackProcessorTypeId::TINY_SYNTH_FX),
        label: "Tiny Synth/FX".to_owned(),
        available: true,
        unavailable_reason: None,
        constraints: shoop_app_api::TrackProcessorConstraints {
            max_dry_audio_channels: None,
            max_wet_audio_channels: None,
            matching_audio_channels: true,
            midi: shoop_app_api::TrackProcessorMidiPolicy::Required,
        },
        features: shoop_app_api::TrackProcessorFeatures {
            state: true,
            external_ui: false,
            embedded_ui: true,
            recovery: false,
            logs: false,
        },
        editor: Some(shoop_app_api::TrackProcessorEditorDescriptor::TinySynthFx {
            presets: shoop_engine::tiny_synth_fx::available_presets()
                .map(|(id, name)| shoop_app_api::TrackProcessorPresetDescriptor {
                    id: id.to_owned(),
                    name: name.to_owned(),
                })
                .collect::<Vec<_>>()
                .into(),
        }),
    }
}

pub fn encode_tiny_synth_fx_state(sample_rate: f32, state: &TinySynthFxState) -> Result<String> {
    let mut control = shoop_engine::tiny_synth_fx::TinySynthFxControlState::new(sample_rate)?;
    if let Some(preset) = &state.selected_preset_id {
        control.select_preset(preset)?;
    }
    control.set_master_gain_db(state.master_gain_db)?;
    control.set_reverb_enabled(state.reverb_enabled);
    control.set_reverb_amount(state.reverb_amount)?;
    control.set_distortion_enabled(state.distortion_enabled);
    control.set_distortion_drive(state.distortion_drive)?;
    Ok(control.encode())
}

pub fn default_tiny_synth_fx_state() -> TrackFxState {
    let control = shoop_engine::tiny_synth_fx::TinySynthFxControlState::new(48_000.0)
        .expect("fixed Tiny Synth/FX defaults are valid");
    let editor = control.editor_state();
    TrackFxState {
        processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::TINY_SYNTH_FX),
        active: false,
        visible: false,
        lifecycle: FxLifecycle::Running,
        generation: 0,
        crash_summary: None,
        logs: Arc::from([]),
        editor: Some(TrackProcessorEditorState::TinySynthFx(TinySynthFxState {
            selected_preset_id: editor.selected_preset_id,
            master_gain_db: editor.master_gain_db,
            reverb_enabled: editor.reverb_enabled,
            reverb_amount: editor.reverb_amount,
            distortion_enabled: editor.distortion_enabled,
            distortion_drive: editor.distortion_drive,
        })),
    }
}
const RECORDING_CHUNK_SIZE: usize = 4096;
const WEB_AUDIO_CAPTURE_PORTS: [&str; 2] = ["webaudio:capture_1", "webaudio:capture_2"];
const WEB_AUDIO_DESTINATION_PORTS: [&str; 2] = ["webaudio:destination_1", "webaudio:destination_2"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendWebMidiOutputEvent {
    pub application_port_id: BackendPortId,
    pub host_port_id: String,
    pub frame: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EngineBackendMode {
    Dummy,
    Physical,
}

pub struct EngineBackend {
    session: Session,
    sample_rate: u32,
    buffer_size: u32,
    elapsed_frame_numerator: u128,
    processed_frames: u64,
    xruns: u32,
    loops: BTreeMap<BackendLoopId, usize>,
    loop_channels: BTreeMap<BackendLoopId, EngineLoopChannels>,
    tracks: BTreeMap<BackendTrackId, EngineTrack>,
    next_loop_id: u64,
    next_track_id: u64,
    next_port_id: u64,
    next_backend_port_id: u64,
    connection_revision: u64,
    connection_ports: BTreeMap<BackendPortId, EngineConnectionPort>,
    external_connections: DummyExternalConnections,
    web_midi_hosts: BTreeMap<String, BackendHostPortDescriptor>,
    desired_web_midi_connections: BTreeSet<(BackendPortId, String)>,
    mode: EngineBackendMode,
    callback_count: u64,
    input_peak: f32,
    output_peak: f32,
    last_quantum: u32,
    route_scratch: Vec<f32>,
    web_midi_output: VecDeque<(BackendPortId, shoop_engine::MidiStorageElem)>,
    web_midi_output_pending: VecDeque<BackendWebMidiOutputEvent>,
    web_midi_output_dropped: u32,
    web_midi_input_refused: u32,
}

struct EngineLoopChannels {
    audio: Vec<usize>,
    audio_modes: Vec<BackendChannelMode>,
    midi: Vec<usize>,
    midi_modes: Vec<BackendChannelMode>,
    gain: f32,
    balance: f32,
}

struct EngineConnectionPort {
    descriptor: BackendPortDescriptor,
    registry_id: PortId,
}

struct EngineTrack {
    port_name_base: String,
    topology: BackendTrackTopology,
    audio_inputs: Vec<usize>,
    audio_outputs: Vec<usize>,
    audio_sends: Vec<usize>,
    audio_returns: Vec<usize>,
    midi_input: Option<usize>,
    midi_output: Option<usize>,
    midi_input_port: Option<BackendPortId>,
    midi_output_port: Option<BackendPortId>,
    loops: Vec<BackendLoopId>,
    ports: Vec<BackendPortId>,
    output_gain_db: f32,
    output_balance: f32,
    output_muted: bool,
    input_gain_db: f32,
    input_balance: f32,
    input_monitoring: bool,
    fx: Option<EngineTinyFx>,
}

struct EngineTinyFx {
    control: shoop_engine::tiny_synth_fx::TinySynthFxControlState,
    active: bool,
    visible: bool,
}

impl EngineBackend {
    pub fn new_dummy(sample_rate: u32, buffer_size: u32) -> Result<Self> {
        if sample_rate == 0 || buffer_size == 0 {
            return Err(anyhow!(
                "dummy sample rate and buffer size must be non-zero"
            ));
        }
        let mut session = Session::default();
        session.set_sample_rate(sample_rate);
        session.set_buffer_size(buffer_size);
        Ok(Self {
            session,
            sample_rate,
            buffer_size,
            elapsed_frame_numerator: 0,
            processed_frames: 0,
            xruns: 0,
            loops: BTreeMap::new(),
            loop_channels: BTreeMap::new(),
            tracks: BTreeMap::new(),
            next_loop_id: 1,
            next_track_id: 1,
            next_port_id: 1,
            next_backend_port_id: 1,
            connection_revision: 1,
            connection_ports: BTreeMap::new(),
            external_connections: representative_external_connections(),
            web_midi_hosts: BTreeMap::new(),
            desired_web_midi_connections: BTreeSet::new(),
            mode: EngineBackendMode::Dummy,
            callback_count: 0,
            input_peak: 0.0,
            output_peak: 0.0,
            last_quantum: buffer_size,
            route_scratch: vec![0.0; buffer_size as usize],
            web_midi_output: VecDeque::with_capacity(WEB_MIDI_OUTPUT_QUEUE_CAPACITY),
            web_midi_output_pending: VecDeque::with_capacity(WEB_MIDI_OUTPUT_QUEUE_CAPACITY),
            web_midi_output_dropped: 0,
            web_midi_input_refused: 0,
        })
    }

    pub fn new_web_audio(sample_rate: u32, max_quantum: u32) -> Result<Self> {
        if sample_rate == 0 || max_quantum == 0 || max_quantum > MAX_WEB_AUDIO_QUANTUM {
            return Err(anyhow!(
                "Web Audio sample rate must be non-zero and quantum must be in 1..={MAX_WEB_AUDIO_QUANTUM}"
            ));
        }
        let mut backend = Self::new_dummy(sample_rate, max_quantum)?;
        backend.mode = EngineBackendMode::Physical;
        backend.external_connections.remove_all_mock_ports();
        Ok(backend)
    }

    pub fn configure_web_audio_channels(
        &mut self,
        input_channels: u32,
        output_channels: u32,
    ) -> Result<()> {
        if self.mode != EngineBackendMode::Physical {
            return Err(anyhow!(
                "device channels supplied to a non-physical backend"
            ));
        }
        if input_channels > 2 || output_channels > 2 {
            return Err(anyhow!(
                "Web Audio channel count exceeds the protocol limit"
            ));
        }
        let audio_hosts = self
            .external_connections
            .mock_ports()
            .iter()
            .filter(|port| port.data_type == EnginePortDataType::Audio)
            .map(|port| port.name.clone())
            .collect::<Vec<_>>();
        for host in audio_hosts {
            self.external_connections.remove_mock_port(&host);
        }
        for host in WEB_AUDIO_CAPTURE_PORTS.iter().take(input_channels as usize) {
            self.external_connections.add_mock_port(
                *host,
                PortDirection::Output,
                EnginePortDataType::Audio,
            );
        }
        for host in WEB_AUDIO_DESTINATION_PORTS
            .iter()
            .take(output_channels as usize)
        {
            self.external_connections.add_mock_port(
                *host,
                PortDirection::Input,
                EnginePortDataType::Audio,
            );
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        Ok(())
    }

    pub fn configure_web_midi_endpoints(
        &mut self,
        endpoints: Vec<BackendHostPortDescriptor>,
    ) -> Result<()> {
        if self.mode != EngineBackendMode::Physical {
            return Err(anyhow!(
                "Web MIDI endpoints supplied to a non-physical backend"
            ));
        }
        let replacement = endpoints
            .into_iter()
            .map(|endpoint| {
                if endpoint.data_type != BackendPortDataType::Midi {
                    return Err(anyhow!("Web MIDI endpoint has a non-MIDI data type"));
                }
                Ok((endpoint.id.clone(), endpoint))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        if replacement == self.web_midi_hosts {
            return Ok(());
        }
        let removed = self
            .web_midi_hosts
            .keys()
            .filter(|id| !replacement.contains_key(*id))
            .cloned()
            .collect::<Vec<_>>();
        for id in removed {
            self.external_connections.remove_mock_port(&id);
        }
        for endpoint in replacement.values() {
            self.external_connections.add_mock_port(
                endpoint.id.clone(),
                engine_direction(endpoint.direction),
                EnginePortDataType::Midi,
            );
        }
        self.web_midi_hosts = replacement;
        let desired = self
            .desired_web_midi_connections
            .iter()
            .filter(|(_, host_id)| self.web_midi_hosts.contains_key(host_id))
            .cloned()
            .collect::<Vec<_>>();
        for (application_port_id, host_id) in desired {
            let Some(local) = self.connection_ports.get(&application_port_id) else {
                continue;
            };
            if let Some(host) = self.web_midi_hosts.get(&host_id) {
                if host.direction == opposite_backend_direction(local.descriptor.direction) {
                    self.external_connections
                        .connect(local.registry_id, &host_id)?;
                }
            }
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        Ok(())
    }

    pub fn stage_web_midi_input(&mut self, host_port_id: &str, data: &[u8]) -> Result<usize> {
        let Some(host) = self.web_midi_hosts.get(host_port_id) else {
            self.web_midi_input_refused = self.web_midi_input_refused.saturating_add(1);
            return Ok(0);
        };
        if host.direction != BackendPortDirection::Output {
            return Err(anyhow!("Web MIDI endpoint is not an input source"));
        }
        let destinations = self
            .tracks
            .values()
            .filter_map(|track| {
                Some((
                    track.midi_input?,
                    track.midi_input_port?,
                    self.connection_ports
                        .get(&track.midi_input_port?)?
                        .registry_id,
                ))
            })
            .filter(|(_, _, registry_id)| {
                self.external_connections
                    .is_connected(*registry_id, host_port_id)
            })
            .map(|(session_port, application_port_id, _)| (session_port, application_port_id))
            .collect::<Vec<_>>();
        let mut staged = 0;
        for (session_port, _) in destinations {
            let accepted = self
                .session
                .port_mut(session_port)
                .and_then(Port::as_external_midi_mut)
                .ok_or_else(|| anyhow!("missing physical MIDI input port"))?
                .push_incoming(0, data);
            if accepted {
                staged += 1;
            } else {
                self.web_midi_input_refused = self.web_midi_input_refused.saturating_add(1);
            }
        }
        Ok(staged)
    }

    pub fn drain_web_midi_output(
        &mut self,
        max_events: usize,
    ) -> (Vec<BackendWebMidiOutputEvent>, u32, u32) {
        while self.web_midi_output_pending.len() < max_events {
            let Some((application_port_id, event)) = self.web_midi_output.pop_front() else {
                break;
            };
            let Some(local) = self.connection_ports.get(&application_port_id) else {
                continue;
            };
            for host_port_id in self.external_connections.connections_for(local.registry_id) {
                if !self
                    .web_midi_hosts
                    .get(&host_port_id)
                    .is_some_and(|host| host.direction == BackendPortDirection::Input)
                {
                    continue;
                }
                if self.web_midi_output_pending.len() >= WEB_MIDI_OUTPUT_QUEUE_CAPACITY {
                    self.web_midi_output_dropped = self.web_midi_output_dropped.saturating_add(1);
                    continue;
                }
                self.web_midi_output_pending
                    .push_back(BackendWebMidiOutputEvent {
                        application_port_id,
                        host_port_id,
                        frame: event.time,
                        data: event.data().to_vec(),
                    });
            }
        }
        let count = max_events.min(self.web_midi_output_pending.len());
        let events = self.web_midi_output_pending.drain(..count).collect();
        (
            events,
            std::mem::take(&mut self.web_midi_output_dropped),
            std::mem::take(&mut self.web_midi_input_refused),
        )
    }

    pub fn web_midi_input_refused(&self) -> u32 {
        self.web_midi_input_refused
    }

    pub fn process_audio_quantum(
        &mut self,
        input: &[f32],
        input_channels: usize,
        output: &mut [f32],
        output_channels: usize,
        n_frames: usize,
    ) -> Result<()> {
        if self.mode != EngineBackendMode::Physical {
            return Err(anyhow!("audio quantum supplied to a non-physical backend"));
        }
        if n_frames == 0
            || n_frames > self.buffer_size as usize
            || input_channels.saturating_mul(n_frames) > input.len()
            || output_channels.saturating_mul(n_frames) > output.len()
        {
            return Err(anyhow!("invalid Web Audio channel or quantum shape"));
        }

        self.input_peak = 0.0;
        for track in self.tracks.values() {
            for (channel, session_port) in track.audio_inputs.iter().enumerate() {
                let backend_port_id = track.ports[channel * 2];
                let registry_id = self.connection_ports[&backend_port_id].registry_id;
                let mut source_count = 0;
                self.route_scratch[..n_frames].fill(0.0);
                for source in 0..input_channels {
                    let host = WEB_AUDIO_CAPTURE_PORTS[source];
                    if self.external_connections.is_connected(registry_id, host) {
                        source_count += 1;
                        for (mixed, sample) in self.route_scratch[..n_frames]
                            .iter_mut()
                            .zip(&input[source * n_frames..(source + 1) * n_frames])
                        {
                            *mixed += *sample;
                        }
                    }
                }
                let samples = if source_count == 0 {
                    &[][..]
                } else {
                    &self.route_scratch[..n_frames]
                };
                self.input_peak = samples
                    .iter()
                    .fold(self.input_peak, |peak, sample| peak.max(sample.abs()));
                self.session
                    .port_mut(*session_port)
                    .and_then(Port::as_external_mut)
                    .ok_or_else(|| anyhow!("missing physical audio input port"))?
                    .stage_input(samples);
            }
        }

        self.session.process(n_frames);
        for track in self.tracks.values() {
            let (Some(session_port), Some(application_port_id)) =
                (track.midi_output, track.midi_output_port)
            else {
                continue;
            };
            let events = self
                .session
                .port(session_port)
                .and_then(Port::as_external_midi)
                .ok_or_else(|| anyhow!("missing physical MIDI output port"))?
                .outgoing();
            for event in events {
                if self.web_midi_output.len() >= WEB_MIDI_OUTPUT_QUEUE_CAPACITY {
                    self.web_midi_output_dropped = self.web_midi_output_dropped.saturating_add(1);
                } else {
                    self.web_midi_output
                        .push_back((application_port_id, *event));
                }
            }
        }
        output[..output_channels * n_frames].fill(0.0);
        self.output_peak = 0.0;
        for track in self.tracks.values() {
            for (channel, session_port) in track.audio_outputs.iter().enumerate() {
                let backend_port_id = track.ports[channel * 2 + 1];
                let registry_id = self.connection_ports[&backend_port_id].registry_id;
                let samples = self
                    .session
                    .port(*session_port)
                    .and_then(Port::as_external)
                    .ok_or_else(|| anyhow!("missing physical audio output port"))?
                    .output(n_frames);
                for destination in 0..output_channels {
                    let host = WEB_AUDIO_DESTINATION_PORTS[destination];
                    if self.external_connections.is_connected(registry_id, host) {
                        for (target, sample) in output
                            [destination * n_frames..(destination + 1) * n_frames]
                            .iter_mut()
                            .zip(samples)
                        {
                            *target += *sample;
                        }
                    }
                }
            }
        }
        for sample in &mut output[..output_channels * n_frames] {
            self.output_peak = self.output_peak.max(sample.abs());
            *sample = sample.clamp(-1.0, 1.0);
        }
        self.callback_count = self.callback_count.saturating_add(1);
        self.processed_frames = self.processed_frames.saturating_add(n_frames as u64);
        self.last_quantum = n_frames as u32;
        Ok(())
    }

    pub fn advance_frames(&mut self, mut frames: u32) {
        while frames > 0 {
            let chunk = frames.min(self.buffer_size);
            self.session.process(chunk as usize);
            self.processed_frames = self.processed_frames.saturating_add(chunk as u64);
            frames -= chunk;
        }
    }

    pub fn processed_frames(&self) -> u64 {
        self.processed_frames
    }

    fn audio_driver_runtime_state(&self) -> AudioDriverRuntimeState {
        let (supported, configured, kind, instance_name) = match self.mode {
            EngineBackendMode::Dummy => (
                true,
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: self.sample_rate,
                    buffer_size: self.buffer_size,
                }),
                AudioDriverKind::Dummy,
                "dummy".to_owned(),
            ),
            EngineBackendMode::Physical => (
                false,
                AudioDriverConfig::WebAudio,
                AudioDriverKind::WebAudio,
                "Web Audio".to_owned(),
            ),
        };
        AudioDriverRuntimeState {
            supported,
            catalog: Arc::from([AudioDriverDescriptor {
                kind,
                available: true,
                ..Default::default()
            }]),
            active: Some(ResolvedAudioDriverConfig {
                configured,
                sample_rate: self.sample_rate,
                buffer_size: self.buffer_size,
                instance_name,
            }),
            ..Default::default()
        }
    }

    fn next_port_id(&mut self) -> PortId {
        let id = PortId(self.next_port_id);
        self.next_port_id = self.next_port_id.saturating_add(1);
        id
    }

    fn register_connection_port(
        &mut self,
        registry_id: PortId,
        name: String,
        data_type: BackendPortDataType,
        direction: BackendPortDirection,
        role: BackendPortRole,
    ) -> BackendPortDescriptor {
        let id = BackendPortId::from_raw(self.next_backend_port_id);
        self.next_backend_port_id = self.next_backend_port_id.saturating_add(1);
        let descriptor = BackendPortDescriptor {
            id,
            name: name.clone(),
            data_type,
            direction,
            role,
        };
        self.connection_ports.insert(
            id,
            EngineConnectionPort {
                descriptor: descriptor.clone(),
                registry_id,
            },
        );
        if self.mode == EngineBackendMode::Dummy {
            self.external_connections.add_mock_port(
                format!("shoop:{name}"),
                engine_direction(direction),
                engine_data_type(data_type),
            );
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        descriptor
    }

    pub fn add_external_mock_port(
        &mut self,
        name: impl Into<String>,
        direction: BackendPortDirection,
        data_type: BackendPortDataType,
    ) {
        self.external_connections.add_mock_port(
            name,
            engine_direction(direction),
            engine_data_type(data_type),
        );
        self.connection_revision = self.connection_revision.wrapping_add(1);
    }

    pub fn remove_external_mock_port(&mut self, name: &str) {
        self.external_connections.remove_mock_port(name);
        self.connection_revision = self.connection_revision.wrapping_add(1);
    }

    pub fn remove_all_external_mock_ports(&mut self) {
        self.external_connections.remove_all_mock_ports();
        self.connection_revision = self.connection_revision.wrapping_add(1);
    }

    pub fn externally_set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        self.set_port_connected(port_id, external_port, connected)
    }

    fn connection_snapshot(&self) -> BackendConnectionSnapshot {
        let application_ports = self
            .connection_ports
            .iter()
            .map(|(id, local)| (*id, local.descriptor.clone()))
            .collect();
        let host_ports = self
            .external_connections
            .mock_ports()
            .iter()
            .filter(|port| !port.name.starts_with("shoop:"))
            .map(|port| {
                (
                    port.name.clone(),
                    self.web_midi_hosts.get(&port.name).cloned().unwrap_or(
                        BackendHostPortDescriptor {
                            id: port.name.clone(),
                            name: port.name.clone(),
                            data_type: backend_data_type(port.data_type),
                            direction: backend_direction(port.direction),
                        },
                    ),
                )
            })
            .collect();
        let confirmed_links = self
            .connection_ports
            .iter()
            .flat_map(|(id, local)| {
                self.external_connections
                    .connections_for(local.registry_id)
                    .into_iter()
                    .filter(|host_port_id| !host_port_id.starts_with("shoop:"))
                    .map(|host_port_id| BackendConfirmedLink {
                        application_port_id: *id,
                        host_port_id,
                    })
            })
            .collect();
        BackendConnectionSnapshot {
            revision: self.connection_revision,
            available: true,
            application_ports,
            host_ports,
            confirmed_links,
            failures: Vec::new(),
        }
    }

    fn engine_loop_index(&self, id: BackendLoopId) -> Result<usize> {
        self.loops
            .get(&id)
            .copied()
            .ok_or_else(|| anyhow!("unknown backend loop {id:?}"))
    }

    fn create_track_loop(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let (audio_routes, midi_route) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
            match track.topology {
                BackendTrackTopology::Direct { .. } => (
                    track
                        .audio_inputs
                        .iter()
                        .copied()
                        .zip(track.audio_outputs.iter().copied())
                        .map(|(input, output)| (input, output, BackendChannelMode::Direct))
                        .collect::<Vec<_>>(),
                    track
                        .midi_input
                        .zip(track.midi_output)
                        .map(|(input, output)| (input, output, BackendChannelMode::Direct)),
                ),
                BackendTrackTopology::DryWetProcessor { .. } => (
                    track
                        .audio_inputs
                        .iter()
                        .copied()
                        .zip(track.audio_sends.iter().copied())
                        .map(|(input, output)| (input, output, BackendChannelMode::Dry))
                        .chain(
                            track
                                .audio_returns
                                .iter()
                                .copied()
                                .zip(track.audio_outputs.iter().copied())
                                .map(|(input, output)| (input, output, BackendChannelMode::Wet)),
                        )
                        .collect::<Vec<_>>(),
                    track
                        .midi_input
                        .zip(track.midi_output)
                        .map(|(input, output)| (input, output, BackendChannelMode::Dry)),
                ),
                BackendTrackTopology::DryWetExternal { .. } => {
                    return Err(anyhow!(
                        "External topology is unavailable in the engine backend"
                    ));
                }
            }
        };
        let loop_id = self.create_loop()?;
        let engine_loop = self.engine_loop_index(loop_id)?;
        let mut audio = Vec::with_capacity(audio_routes.len());
        let mut audio_modes = Vec::with_capacity(audio_routes.len());
        for (input, output, mode) in audio_routes {
            let engine_mode = match mode {
                BackendChannelMode::Direct => ChannelMode::Direct,
                BackendChannelMode::Dry => ChannelMode::Dry,
                BackendChannelMode::Wet => ChannelMode::Wet,
            };
            let channel = if self.mode == EngineBackendMode::Physical {
                self.session.add_audio_channel_with_bounded_capacity(
                    engine_loop,
                    RECORDING_CHUNK_SIZE,
                    self.sample_rate as usize * RECORDING_CAPACITY_SECONDS as usize,
                    engine_mode,
                )?
            } else {
                self.session
                    .add_audio_channel(engine_loop, 64, engine_mode)?
            };
            self.session.connect_channel_input(channel, input)?;
            self.session.connect_channel_output(channel, output)?;
            audio.push(channel);
            audio_modes.push(mode);
        }
        let mut midi = Vec::new();
        let mut midi_modes = Vec::new();
        if let Some((input, output, mode)) = midi_route {
            let engine_mode = match mode {
                BackendChannelMode::Direct => ChannelMode::Direct,
                BackendChannelMode::Dry => ChannelMode::Dry,
                BackendChannelMode::Wet => ChannelMode::Wet,
            };
            let channel = self
                .session
                .add_midi_channel(engine_loop, 1024, engine_mode)?;
            self.session.connect_channel_input(channel, input)?;
            self.session.connect_channel_output(channel, output)?;
            midi.push(channel);
            midi_modes.push(mode);
        }
        self.loop_channels.insert(
            loop_id,
            EngineLoopChannels {
                audio,
                audio_modes,
                midi,
                midi_modes,
                gain: 1.0,
                balance: 0.0,
            },
        );
        self.tracks
            .get_mut(&track_id)
            .expect("track was validated before loop construction")
            .loops
            .push(loop_id);
        Ok(loop_id)
    }

    fn create_tiny_synth_fx_track(
        &mut self,
        request: TrackRequest,
    ) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetProcessor {
            processor_type,
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected processed track topology"));
        };
        if processor_type != TrackProcessorTypeId::TINY_SYNTH_FX {
            return Err(anyhow!("requested track processor is unavailable"));
        }
        if dry_audio_channels != wet_audio_channels || !dry_midi {
            return Err(anyhow!(
                "Tiny Synth/FX requires matched audio channels and one MIDI input"
            ));
        }
        let channel_count = dry_audio_channels as usize;
        let capture_samples = self.sample_rate as usize * RECORDING_CAPACITY_SECONDS as usize;
        let capture_block_size = capture_samples.div_ceil(32).max(self.buffer_size as usize);
        let mut audio_inputs = Vec::with_capacity(channel_count);
        let mut audio_outputs = Vec::with_capacity(channel_count);
        let mut audio_sends = Vec::with_capacity(channel_count);
        let mut audio_returns = Vec::with_capacity(channel_count);
        let mut ports = Vec::with_capacity(channel_count.saturating_mul(2).saturating_add(1));
        for index in 0..dry_audio_channels {
            let input_name = format!("{}_audio_dry_in_{}", request.port_name_base, index + 1);
            let output_name = format!("{}_audio_wet_out_{}", request.port_name_base, index + 1);
            let input_registry_id = self.next_port_id();
            let output_registry_id = self.next_port_id();
            let (input, output) = if self.mode == EngineBackendMode::Physical {
                let mut input = ExternalAudioPort::new(
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                let input = self.session.add_port(Port::External(input));
                let output = self.session.add_port(Port::External(ExternalAudioPort::new(
                    output_name.clone(),
                    PortDirection::Output,
                    self.buffer_size as usize,
                )));
                (input, output)
            } else {
                let mut input = DummyAudioPort::new(
                    input_registry_id,
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                let input = self.session.add_port(Port::Dummy(input));
                let output = self.session.add_port(Port::Dummy(DummyAudioPort::new(
                    output_registry_id,
                    output_name.clone(),
                    PortDirection::Output,
                    1,
                )));
                (input, output)
            };
            let send = self.session.add_port(Port::Internal(InternalAudioPort::new(
                format!("{}:audio_in_{index}", request.port_name_base),
                self.buffer_size as usize,
                shoop_engine::PortConnectability::INTERNAL,
                shoop_engine::PortConnectability::INTERNAL,
                0,
            )));
            let receive = self.session.add_port(Port::Internal(InternalAudioPort::new(
                format!("{}:audio_out_{index}", request.port_name_base),
                self.buffer_size as usize,
                shoop_engine::PortConnectability::INTERNAL,
                shoop_engine::PortConnectability::INTERNAL,
                0,
            )));
            self.session.connect_ports_internal(input, send)?;
            self.session.connect_ports_internal(receive, output)?;
            ports.push(self.register_connection_port(
                input_registry_id,
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            ports.push(self.register_connection_port(
                output_registry_id,
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
            audio_inputs.push(input);
            audio_outputs.push(output);
            audio_sends.push(send);
            audio_returns.push(receive);
        }
        let midi_name = format!("{}_dry_midi_in", request.port_name_base);
        let midi_registry_id = self.next_port_id();
        let midi_input = if self.mode == EngineBackendMode::Physical {
            let mut input = ExternalMidiPort::new(midi_name.clone(), PortDirection::Input);
            input.midi_mut().set_passthrough_muted(true);
            input
                .midi_mut()
                .set_ringbuffer_n_samples(capture_samples.min(u32::MAX as usize) as u32);
            self.session.add_port(Port::ExternalMidi(input))
        } else {
            let mut input =
                DummyMidiPort::new(midi_registry_id, midi_name.clone(), PortDirection::Input);
            input.midi_mut().set_passthrough_muted(true);
            input
                .midi_mut()
                .set_ringbuffer_n_samples(capture_samples.min(u32::MAX as usize) as u32);
            self.session.add_port(Port::DummyMidi(input))
        };
        let midi_target = self
            .session
            .add_port(Port::ExternalMidi(ExternalMidiPort::new(
                format!("{}:midi_in_0", request.port_name_base),
                PortDirection::Output,
            )));
        self.session
            .connect_ports_internal(midi_input, midi_target)?;
        let midi_descriptor = self.register_connection_port(
            midi_registry_id,
            midi_name,
            BackendPortDataType::Midi,
            BackendPortDirection::Input,
            BackendPortRole::MidiInput,
        );
        let midi_input_port = Some(midi_descriptor.id);
        ports.push(midi_descriptor);

        if self.mode == EngineBackendMode::Physical {
            let input_channels = WEB_AUDIO_CAPTURE_PORTS
                .iter()
                .filter(|host| {
                    self.external_connections
                        .mock_ports()
                        .iter()
                        .any(|port| port.name == **host)
                })
                .count();
            let output_channels = WEB_AUDIO_DESTINATION_PORTS
                .iter()
                .filter(|host| {
                    self.external_connections
                        .mock_ports()
                        .iter()
                        .any(|port| port.name == **host)
                })
                .count();
            for channel in 0..channel_count {
                let input_registry = self.connection_ports[&ports[channel * 2].id].registry_id;
                if input_channels > 0 {
                    self.external_connections.connect(
                        input_registry,
                        WEB_AUDIO_CAPTURE_PORTS[channel.min(input_channels - 1)],
                    )?;
                }
                let output_registry = self.connection_ports[&ports[channel * 2 + 1].id].registry_id;
                if channel_count == 1 {
                    for host in WEB_AUDIO_DESTINATION_PORTS.iter().take(output_channels) {
                        self.external_connections.connect(output_registry, host)?;
                    }
                } else if output_channels > 0 {
                    self.external_connections.connect(
                        output_registry,
                        WEB_AUDIO_DESTINATION_PORTS[channel.min(output_channels - 1)],
                    )?;
                }
            }
            self.connection_revision = self.connection_revision.wrapping_add(1);
        }

        let control =
            shoop_engine::tiny_synth_fx::TinySynthFxControlState::new(self.sample_rate as f32)?;
        let processor = control.prepare_processor(
            self.sample_rate as f32,
            channel_count,
            self.buffer_size as usize,
        )?;
        let _ = self
            .session
            .set_tiny_synth_fx_processor(request.port_name_base.clone(), processor);
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            EngineTrack {
                port_name_base: request.port_name_base,
                topology: request.topology,
                audio_inputs,
                audio_outputs,
                audio_sends,
                audio_returns,
                midi_input: Some(midi_input),
                midi_output: Some(midi_target),
                midi_input_port,
                midi_output_port: None,
                loops: Vec::new(),
                ports: ports.iter().map(|port| port.id).collect(),
                output_gain_db: 0.0,
                output_balance: 0.0,
                output_muted: false,
                input_gain_db: 0.0,
                input_balance: 0.0,
                input_monitoring: false,
                fx: Some(EngineTinyFx {
                    control,
                    active: false,
                    visible: false,
                }),
            },
        );
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.create_track_loop(track_id)?);
        }
        self.apply_graph_changes()?;
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn apply_engine_track_routing(&mut self, track_id: BackendTrackId) -> Result<()> {
        let (topology, monitoring, loops, title) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
            (
                track.topology.clone(),
                track.input_monitoring,
                track.loops.clone(),
                track.port_name_base.clone(),
            )
        };
        let routing = match topology {
            BackendTrackTopology::Direct { .. } => DryWetRoutingState {
                dry_input_passthrough_muted: !monitoring,
                wet_output_passthrough_muted: true,
                processor_active: false,
                force_monitoring_off: false,
            },
            BackendTrackTopology::DryWetProcessor { .. } => {
                let mut current = Vec::with_capacity(loops.len());
                let mut next = Vec::with_capacity(loops.len());
                for loop_id in loops {
                    let engine_loop = self.engine_loop_index(loop_id)?;
                    let state = self
                        .session
                        .loop_(engine_loop)
                        .ok_or_else(|| anyhow!("missing engine loop"))?;
                    current.push(from_engine_mode(state.mode()));
                    if let Some((mode, delay)) = state.first_planned_transition() {
                        if delay == 1 {
                            next.push(from_engine_mode(mode));
                        }
                    }
                }
                dry_wet_routing_state(monitoring, &current, &next)
            }
            BackendTrackTopology::DryWetExternal { .. } => {
                return Err(anyhow!(
                    "External topology is unavailable in the engine backend"
                ));
            }
        };
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        if routing.force_monitoring_off {
            track.input_monitoring = false;
        }
        for port in &track.audio_inputs {
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing audio input port"))?
                .set_passthrough_muted(routing.dry_input_passthrough_muted);
        }
        if let Some(port) = track.midi_input {
            self.session
                .port_mut(port)
                .and_then(Port::midi_mut)
                .ok_or_else(|| anyhow!("missing MIDI input port"))?
                .set_passthrough_muted(routing.dry_input_passthrough_muted);
        }
        for port in &track.audio_returns {
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing processor output port"))?
                .set_passthrough_muted(routing.wet_output_passthrough_muted);
        }
        if let Some(fx) = track.fx.as_mut() {
            fx.active = routing.processor_active;
            self.session
                .set_tiny_synth_fx_active(&title, routing.processor_active);
        }
        Ok(())
    }

    fn capture_session_data(&self) -> Result<BackendSessionData> {
        let connections = self.connection_snapshot();
        let mut tracks = Vec::with_capacity(self.tracks.len());
        for (track_id, track) in &self.tracks {
            let state = BackendTrackState {
                topology: track.topology.clone(),
                fx: track.fx.as_ref().map(engine_tiny_fx_state),
                audio_channels: track.audio_outputs.len() as u32,
                midi: track.midi_input.is_some(),
                output_gain_db: track.output_gain_db,
                output_balance: track.output_balance,
                output_muted: track.output_muted,
                input_gain_db: track.input_gain_db,
                input_balance: track.input_balance,
                input_monitoring: track.input_monitoring,
                ..Default::default()
            };
            let mut loops = Vec::with_capacity(track.loops.len());
            for loop_id in &track.loops {
                let engine_loop = self.engine_loop_index(*loop_id)?;
                let loop_state = self
                    .session
                    .loop_(engine_loop)
                    .ok_or_else(|| anyhow!("missing engine loop"))?;
                if matches!(
                    loop_state.mode(),
                    LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
                ) {
                    return Err(anyhow!("loop content is changing"));
                }
                let channels = self
                    .loop_channels
                    .get(loop_id)
                    .ok_or_else(|| anyhow!("missing loop channels"))?;
                let audio = channels
                    .audio
                    .iter()
                    .zip(&channels.audio_modes)
                    .map(|(channel, mode)| {
                        let channel = self
                            .session
                            .audio_channel(*channel)
                            .ok_or_else(|| anyhow!("missing audio channel"))?;
                        Ok(BackendAudioContent {
                            mode: *mode,
                            samples: channel.data(),
                            gain: channel.gain(),
                            start_offset: channel.start_offset(),
                            preplay: channel.pre_play_samples(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let midi = channels
                    .midi
                    .iter()
                    .zip(&channels.midi_modes)
                    .map(|(channel, mode)| {
                        let channel = self
                            .session
                            .midi_channel(*channel)
                            .ok_or_else(|| anyhow!("missing MIDI channel"))?;
                        Ok(BackendMidiContent {
                            mode: *mode,
                            length: channel.length(),
                            start_state: channel.recording_start_state_messages(),
                            events: channel
                                .contents()
                                .into_iter()
                                .map(|event| BackendMidiEvent {
                                    time: event.time,
                                    data: event.data().to_vec(),
                                })
                                .collect(),
                            start_offset: channel.start_offset(),
                            preplay: channel.pre_play_samples(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                loops.push(BackendLoopContent {
                    source_id: loop_id.raw(),
                    length: loop_state.length(),
                    gain: channels.gain,
                    balance: channels.balance,
                    audio,
                    midi,
                });
            }
            let ports = track
                .ports
                .iter()
                .map(|port_id| {
                    let descriptor = connections
                        .application_ports
                        .get(port_id)
                        .ok_or_else(|| anyhow!("missing application connection port"))?;
                    let mut external_connections = connections
                        .confirmed_links
                        .iter()
                        .filter(|link| link.application_port_id == *port_id)
                        .map(|link| link.host_port_id.clone())
                        .collect::<BTreeSet<_>>();
                    if self.mode == EngineBackendMode::Physical
                        && descriptor.data_type == BackendPortDataType::Midi
                    {
                        external_connections.extend(
                            self.desired_web_midi_connections
                                .iter()
                                .filter(|(desired_port, _)| desired_port == port_id)
                                .map(|(_, host_id)| host_id.clone()),
                        );
                    }
                    Ok(BackendSessionPort {
                        source_id: port_id.raw(),
                        descriptor: descriptor.clone(),
                        external_connections: external_connections.into_iter().collect(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            tracks.push(BackendSessionTrack {
                source_id: track_id.raw(),
                port_name_base: track.port_name_base.clone(),
                topology: track.topology.clone(),
                state,
                loops,
                ports,
                processor_state: track.fx.as_ref().map(|fx| fx.control.encode()),
            });
        }
        Ok(BackendSessionData {
            sample_rate: self.sample_rate,
            tracks,
            use_legacy_browser_default_routes: false,
        })
    }

    fn build_replacement(
        &self,
        data: &BackendSessionData,
    ) -> Result<(Self, BackendSessionReplacement)> {
        if data.sample_rate != self.sample_rate {
            return Err(anyhow!(
                "prepared session sample rate {} does not match backend {}",
                data.sample_rate,
                self.sample_rate
            ));
        }
        if data.tracks.iter().any(|track| {
            let processor_state_valid = match &track.topology {
                BackendTrackTopology::Direct { .. } => track.processor_state.is_none(),
                BackendTrackTopology::DryWetProcessor { processor_type, .. }
                    if processor_type == TrackProcessorTypeId::TINY_SYNTH_FX =>
                {
                    track.processor_state.is_some()
                }
                _ => false,
            };
            track.state.topology != track.topology || !processor_state_valid
        }) {
            return Err(anyhow!(
                "session requires a track processor unavailable in this backend"
            ));
        }
        let mut staged = match self.mode {
            EngineBackendMode::Dummy => Self::new_dummy(self.sample_rate, self.buffer_size)?,
            EngineBackendMode::Physical => Self::new_web_audio(self.sample_rate, self.buffer_size)?,
        };
        staged.external_connections = DummyExternalConnections::default();
        for descriptor in self.external_connections.mock_ports() {
            staged.external_connections.add_mock_port(
                descriptor.name.clone(),
                descriptor.direction,
                descriptor.data_type,
            );
        }
        staged.web_midi_hosts = self.web_midi_hosts.clone();
        let mut replacement = BackendSessionReplacement::default();
        for source_track in &data.tracks {
            let created = staged.create_track(TrackRequest {
                port_name_base: source_track.port_name_base.clone(),
                topology: source_track.topology.clone(),
                initial_loops: source_track.loops.len(),
            })?;
            if let Some(state) = &source_track.processor_state {
                staged.set_track_fx_control(
                    created.track_id,
                    BackendTrackFxControl::RestoreState(state.clone()),
                )?;
            }
            for control in [
                BackendTrackControl::OutputGainDb(source_track.state.output_gain_db),
                BackendTrackControl::OutputBalance(source_track.state.output_balance),
                BackendTrackControl::OutputMute(source_track.state.output_muted),
                BackendTrackControl::InputGainDb(source_track.state.input_gain_db),
                BackendTrackControl::InputBalance(source_track.state.input_balance),
                BackendTrackControl::InputMonitoring(source_track.state.input_monitoring),
            ] {
                staged.set_track_control(created.track_id, control)?;
            }
            if created.loops.len() != source_track.loops.len()
                || created.ports.len() != source_track.ports.len()
            {
                return Err(anyhow!("prepared session topology shape changed"));
            }
            for (source_loop, loop_id) in source_track.loops.iter().zip(&created.loops) {
                let engine_loop = staged.engine_loop_index(*loop_id)?;
                let channels = staged
                    .loop_channels
                    .get(loop_id)
                    .ok_or_else(|| anyhow!("missing staged loop channels"))?;
                if channels.audio.len() != source_loop.audio.len()
                    || channels.midi.len() != source_loop.midi.len()
                {
                    return Err(anyhow!("prepared loop channel shape changed"));
                }
                let audio_indices = channels.audio.clone();
                let midi_indices = channels.midi.clone();
                for (index, content) in audio_indices.iter().zip(&source_loop.audio) {
                    let channel = staged
                        .session
                        .audio_channel_mut(*index)
                        .ok_or_else(|| anyhow!("missing staged audio channel"))?;
                    channel.load_data(&content.samples);
                    channel.set_gain(content.gain);
                    channel.set_start_offset(content.start_offset);
                    channel.set_pre_play_samples(content.preplay);
                }
                for (index, content) in midi_indices.iter().zip(&source_loop.midi) {
                    let events = content
                        .events
                        .iter()
                        .map(|event| {
                            shoop_engine::MidiStorageElem::new(event.time, &event.data)
                                .ok_or_else(|| anyhow!("invalid MIDI event"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let channel = staged
                        .session
                        .midi_channel_mut(*index)
                        .ok_or_else(|| anyhow!("missing staged MIDI channel"))?;
                    channel.set_contents(&events, content.length, Some(&content.start_state));
                    channel.set_start_offset(content.start_offset);
                    channel.set_pre_play_samples(content.preplay);
                }
                staged
                    .session
                    .loop_mut(engine_loop)
                    .ok_or_else(|| anyhow!("missing staged loop"))?
                    .set_length(source_loop.length);
                staged.set_loop_gain(*loop_id, source_loop.gain)?;
                staged.set_loop_balance(*loop_id, source_loop.balance)?;
                replacement.loops.insert(source_loop.source_id, *loop_id);
            }
            for (source_port, created_port) in source_track.ports.iter().zip(&created.ports) {
                replacement
                    .ports
                    .insert(source_port.source_id, created_port.id);
                if !(staged.mode == EngineBackendMode::Physical
                    && data.use_legacy_browser_default_routes)
                {
                    let registry_id = staged
                        .connection_ports
                        .get(&created_port.id)
                        .ok_or_else(|| anyhow!("missing staged connection port"))?
                        .registry_id;
                    for default_connection in
                        staged.external_connections.connections_for(registry_id)
                    {
                        staged.set_port_connected(created_port.id, &default_connection, false)?;
                    }
                    for external in &source_port.external_connections {
                        staged.set_port_connected(created_port.id, external, true)?;
                    }
                }
            }
            replacement
                .tracks
                .insert(source_track.source_id, created.clone());
        }
        staged.apply_graph_changes()?;
        Ok((staged, replacement))
    }

    fn apply_graph_changes(&mut self) -> Result<()> {
        self.session
            .apply_graph_changes()
            .map_err(|error| anyhow!("could not apply dummy engine graph: {error}"))
    }
}

fn representative_external_connections() -> DummyExternalConnections {
    let mut connections = DummyExternalConnections::default();
    for (name, direction, data_type) in [
        (
            "system:capture_1",
            PortDirection::Output,
            EnginePortDataType::Audio,
        ),
        (
            "system:capture_2",
            PortDirection::Output,
            EnginePortDataType::Audio,
        ),
        (
            "system:playback_1",
            PortDirection::Input,
            EnginePortDataType::Audio,
        ),
        (
            "system:playback_2",
            PortDirection::Input,
            EnginePortDataType::Audio,
        ),
        (
            "controller:midi_out",
            PortDirection::Output,
            EnginePortDataType::Midi,
        ),
        (
            "synth:midi_in",
            PortDirection::Input,
            EnginePortDataType::Midi,
        ),
    ] {
        connections.add_mock_port(name, direction, data_type);
    }
    connections
}

fn engine_direction(direction: BackendPortDirection) -> PortDirection {
    match direction {
        BackendPortDirection::Input => PortDirection::Input,
        BackendPortDirection::Output => PortDirection::Output,
    }
}

fn opposite_backend_direction(direction: BackendPortDirection) -> BackendPortDirection {
    match direction {
        BackendPortDirection::Input => BackendPortDirection::Output,
        BackendPortDirection::Output => BackendPortDirection::Input,
    }
}

fn engine_data_type(data_type: BackendPortDataType) -> EnginePortDataType {
    match data_type {
        BackendPortDataType::Audio => EnginePortDataType::Audio,
        BackendPortDataType::Midi => EnginePortDataType::Midi,
    }
}

fn backend_direction(direction: PortDirection) -> BackendPortDirection {
    match direction {
        PortDirection::Input => BackendPortDirection::Input,
        PortDirection::Output => BackendPortDirection::Output,
        PortDirection::Any => unreachable!("host descriptors have a concrete direction"),
    }
}

fn backend_data_type(data_type: EnginePortDataType) -> BackendPortDataType {
    match data_type {
        EnginePortDataType::Audio => BackendPortDataType::Audio,
        EnginePortDataType::Midi => BackendPortDataType::Midi,
        EnginePortDataType::Any => unreachable!("host descriptors have a concrete data type"),
    }
}

fn balance_factors(balance: f32) -> (f32, f32) {
    let balance = balance.clamp(-1.0, 1.0);
    if balance < 0.0 {
        (1.0, 1.0 + balance)
    } else {
        (1.0 - balance, 1.0)
    }
}

fn grab_window(
    request: &BackendGrabRequest,
    cycle_len: u32,
    sync_pos: u32,
    data_len: usize,
) -> (usize, usize, usize) {
    let cycles = request.cycles_length.unwrap_or(1).max(1) as u32;
    let go_cycle = request.go_to_cycle.unwrap_or(0).max(0) as u32;
    let wanted = if cycle_len > 0 {
        if request.reverse_start_cycle == Some(0) {
            sync_pos
        } else if request.go_to_mode == BackendLoopMode::Recording {
            go_cycle.saturating_mul(cycle_len).saturating_add(sync_pos)
        } else {
            cycles.saturating_mul(cycle_len)
        }
    } else {
        data_len.min(u32::MAX as usize) as u32
    } as usize;
    let end = if cycle_len > 0 {
        if let Some(reverse) = request.reverse_start_cycle {
            if reverse == 0 {
                data_len
            } else {
                let before = (reverse.max(0) as u32).saturating_sub(cycles);
                data_len.saturating_sub(
                    sync_pos.saturating_add(before.saturating_mul(cycle_len)) as usize
                )
            }
        } else if request.go_to_mode == BackendLoopMode::Recording {
            data_len
        } else {
            data_len.saturating_sub(
                sync_pos.saturating_add(go_cycle.saturating_mul(cycle_len)) as usize,
            )
        }
    } else {
        data_len
    };
    (wanted, end.saturating_sub(wanted), end)
}

fn apply_loop_gain_balance(session: &mut Session, channels: &EngineLoopChannels) -> Result<()> {
    let (left, right) = balance_factors(channels.balance);
    let stereo = channels.audio.len() == 2;
    for (index, channel) in channels.audio.iter().enumerate() {
        let factor = if stereo {
            if index == 0 {
                left
            } else {
                right
            }
        } else {
            1.0
        };
        session
            .audio_channel_mut(*channel)
            .ok_or_else(|| anyhow!("missing audio loop channel"))?
            .set_gain(channels.gain * factor);
    }
    Ok(())
}

fn db_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude > 0.0 {
        20.0 * amplitude.log10()
    } else {
        -200.0
    }
}

fn engine_tiny_fx_state(fx: &EngineTinyFx) -> TrackFxState {
    let editor = fx.control.editor_state();
    TrackFxState {
        processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::TINY_SYNTH_FX),
        active: fx.active,
        visible: fx.visible,
        lifecycle: FxLifecycle::Running,
        generation: 0,
        crash_summary: None,
        logs: Arc::from([]),
        editor: Some(TrackProcessorEditorState::TinySynthFx(TinySynthFxState {
            selected_preset_id: editor.selected_preset_id,
            master_gain_db: editor.master_gain_db,
            reverb_enabled: editor.reverb_enabled,
            reverb_amount: editor.reverb_amount,
            distortion_enabled: editor.distortion_enabled,
            distortion_drive: editor.distortion_drive,
        })),
    }
}

impl Backend for EngineBackend {
    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(vec![tiny_synth_fx_descriptor()].into())
    }

    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        Ok(self.audio_driver_runtime_state())
    }

    fn preflight_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        let AudioDriverConfig::Dummy(config) = config else {
            return Err(anyhow!("this backend supports only dummy-driver switching"));
        };
        if self.mode != EngineBackendMode::Dummy {
            return Err(anyhow!("Web Audio is selected automatically"));
        }
        if config.sample_rate == 0 || config.buffer_size == 0 {
            return Err(anyhow!(
                "dummy sample rate and buffer size must be non-zero"
            ));
        }
        Ok(ResolvedAudioDriverConfig {
            configured: AudioDriverConfig::Dummy(config.clone()),
            sample_rate: config.sample_rate,
            buffer_size: config.buffer_size,
            instance_name: "dummy".to_owned(),
        })
    }

    fn switch_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
        confirmed_sample_rate: u32,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let resolved = self.preflight_audio_driver(config)?;
        if resolved.sample_rate != confirmed_sample_rate {
            return Err(anyhow!(
                "resolved target sample rate changed from {confirmed_sample_rate} to {}",
                resolved.sample_rate
            ));
        }
        if session.sample_rate != resolved.sample_rate {
            return Err(anyhow!(
                "prepared session sample rate {} does not match target {}",
                session.sample_rate,
                resolved.sample_rate
            ));
        }
        let mut target = EngineBackend::new_dummy(resolved.sample_rate, resolved.buffer_size)?;
        target.external_connections = self.external_connections.clone();
        let (mut replacement, mapping) = target.build_replacement(session)?;
        replacement.elapsed_frame_numerator = self.elapsed_frame_numerator;
        replacement.processed_frames = self.processed_frames;
        replacement.xruns = self.xruns;
        *self = replacement;
        Ok(mapping)
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match &request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels: *audio_channels,
                midi: *midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetProcessor { processor_type, .. }
                if processor_type == TrackProcessorTypeId::TINY_SYNTH_FX =>
            {
                self.create_tiny_synth_fx_track(request)
            }
            _ => Err(anyhow!("requested track processor is unavailable")),
        }
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let engine_loop = self.session.create_loop();
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(id, engine_loop);
        Ok(id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let audio_channels = usize::try_from(request.audio_channels)
            .map_err(|_| anyhow!("direct track audio channel count does not fit this target"))?;
        let port_capacity = audio_channels
            .checked_mul(2)
            .and_then(|count| count.checked_add(2))
            .ok_or_else(|| anyhow!("direct track audio channel count is too large"))?;
        let mut audio_inputs = Vec::with_capacity(audio_channels);
        let mut audio_outputs = Vec::with_capacity(audio_channels);
        let mut ports = Vec::with_capacity(port_capacity);
        let capture_samples = self.sample_rate as usize * RECORDING_CAPACITY_SECONDS as usize;
        let capture_block_size = capture_samples.div_ceil(32).max(self.buffer_size as usize);
        for index in 0..request.audio_channels {
            let suffix = if request.audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            let input_name = format!("{}_direct_in{suffix}", request.port_name_base);
            let output_name = format!("{}_direct_out{suffix}", request.port_name_base);
            let input_registry_id = self.next_port_id();
            let output_registry_id = self.next_port_id();
            let (input, output) = if self.mode == EngineBackendMode::Physical {
                let mut input = ExternalAudioPort::new(
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                let input = self.session.add_port(Port::External(input));
                let output = self.session.add_port(Port::External(ExternalAudioPort::new(
                    output_name.clone(),
                    PortDirection::Output,
                    self.buffer_size as usize,
                )));
                (input, output)
            } else {
                let mut input = DummyAudioPort::new(
                    input_registry_id,
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                let input = self.session.add_port(Port::Dummy(input));
                let output = self.session.add_port(Port::Dummy(DummyAudioPort::new(
                    output_registry_id,
                    output_name.clone(),
                    PortDirection::Output,
                    1,
                )));
                (input, output)
            };
            ports.push(self.register_connection_port(
                input_registry_id,
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            ports.push(self.register_connection_port(
                output_registry_id,
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
            self.session.connect_ports_internal(input, output)?;
            audio_inputs.push(input);
            audio_outputs.push(output);
        }
        let (midi_input, midi_output, midi_input_port, midi_output_port) = if request.midi {
            let input_name = format!("{}_direct_midi_in", request.port_name_base);
            let output_name = format!("{}_direct_midi_out", request.port_name_base);
            let input_registry_id = self.next_port_id();
            let output_registry_id = self.next_port_id();
            let (input, output) = if self.mode == EngineBackendMode::Physical {
                let mut input = ExternalMidiPort::new(input_name.clone(), PortDirection::Input);
                input.midi_mut().set_passthrough_muted(true);
                input
                    .midi_mut()
                    .set_ringbuffer_n_samples(capture_samples.min(u32::MAX as usize) as u32);
                let input = self.session.add_port(Port::ExternalMidi(input));
                let output = self
                    .session
                    .add_port(Port::ExternalMidi(ExternalMidiPort::new(
                        output_name.clone(),
                        PortDirection::Output,
                    )));
                (input, output)
            } else {
                let mut input =
                    DummyMidiPort::new(input_registry_id, input_name.clone(), PortDirection::Input);
                input.midi_mut().set_passthrough_muted(true);
                input
                    .midi_mut()
                    .set_ringbuffer_n_samples(capture_samples.min(u32::MAX as usize) as u32);
                let input = self.session.add_port(Port::DummyMidi(input));
                let output = self.session.add_port(Port::DummyMidi(DummyMidiPort::new(
                    output_registry_id,
                    output_name.clone(),
                    PortDirection::Output,
                )));
                (input, output)
            };
            let input_port = self.register_connection_port(
                input_registry_id,
                input_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
            );
            let output_port = self.register_connection_port(
                output_registry_id,
                output_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Output,
                BackendPortRole::MidiOutput,
            );
            ports.push(input_port.clone());
            ports.push(output_port.clone());
            self.session.connect_ports_internal(input, output)?;
            (
                Some(input),
                Some(output),
                Some(input_port.id),
                Some(output_port.id),
            )
        } else {
            (None, None, None, None)
        };
        if self.mode == EngineBackendMode::Physical {
            let input_channels = WEB_AUDIO_CAPTURE_PORTS
                .iter()
                .take_while(|host| {
                    self.external_connections
                        .mock_ports()
                        .iter()
                        .any(|p| p.name == **host)
                })
                .count();
            let output_channels = WEB_AUDIO_DESTINATION_PORTS
                .iter()
                .take_while(|host| {
                    self.external_connections
                        .mock_ports()
                        .iter()
                        .any(|p| p.name == **host)
                })
                .count();
            for channel in 0..audio_channels {
                let input_registry = self.connection_ports[&ports[channel * 2].id].registry_id;
                if input_channels > 0 {
                    self.external_connections.connect(
                        input_registry,
                        WEB_AUDIO_CAPTURE_PORTS[channel.min(input_channels - 1)],
                    )?;
                }
                let output_registry = self.connection_ports[&ports[channel * 2 + 1].id].registry_id;
                if audio_channels == 1 {
                    for host in WEB_AUDIO_DESTINATION_PORTS.iter().take(output_channels) {
                        self.external_connections.connect(output_registry, host)?;
                    }
                } else if output_channels > 0 {
                    self.external_connections.connect(
                        output_registry,
                        WEB_AUDIO_DESTINATION_PORTS[channel.min(output_channels - 1)],
                    )?;
                }
            }
            self.connection_revision = self.connection_revision.wrapping_add(1);
        }
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            EngineTrack {
                port_name_base: request.port_name_base,
                topology: BackendTrackTopology::Direct {
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                },
                audio_inputs,
                audio_outputs,
                audio_sends: Vec::new(),
                audio_returns: Vec::new(),
                midi_input,
                midi_output,
                midi_input_port,
                midi_output_port,
                loops: Vec::new(),
                ports: ports.iter().map(|port| port.id).collect(),
                output_gain_db: 0.0,
                output_balance: 0.0,
                output_muted: false,
                input_gain_db: 0.0,
                input_balance: 0.0,
                input_monitoring: false,
                fx: None,
            },
        );
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.create_track_loop(track_id)?);
        }
        self.apply_graph_changes()?;
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let loop_id = self.create_track_loop(track_id)?;
        self.apply_graph_changes()?;
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        match control {
            BackendTrackControl::OutputGainDb(value) => track.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => {
                track.output_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::OutputMute(value) => {
                track.output_muted = value;
                for port in &track.audio_outputs {
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .ok_or_else(|| anyhow!("missing audio output port"))?
                        .set_muted(value);
                }
                if matches!(track.topology, BackendTrackTopology::Direct { .. }) {
                    if let Some(port) = track.midi_output {
                        self.session
                            .port_mut(port)
                            .and_then(Port::midi_mut)
                            .ok_or_else(|| anyhow!("missing MIDI output port"))?
                            .set_muted(value);
                    }
                }
            }
            BackendTrackControl::InputGainDb(value) => track.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => {
                track.input_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::InputMonitoring(value) => {
                track.input_monitoring = value;
                for port in &track.audio_inputs {
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .ok_or_else(|| anyhow!("missing audio input port"))?
                        .set_passthrough_muted(!value);
                }
                if let Some(port) = track.midi_input {
                    self.session
                        .port_mut(port)
                        .and_then(Port::midi_mut)
                        .ok_or_else(|| anyhow!("missing MIDI input port"))?
                        .set_passthrough_muted(!value);
                }
            }
        }
        let (left, right) = balance_factors(track.output_balance);
        let base = db_gain(track.output_gain_db);
        for (index, port) in track.audio_outputs.iter().enumerate() {
            let factor = if track.audio_outputs.len() == 2 {
                if index == 0 {
                    left
                } else {
                    right
                }
            } else {
                1.0
            };
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing audio output port"))?
                .set_gain(base * factor);
        }
        let (left, right) = balance_factors(track.input_balance);
        let base = db_gain(track.input_gain_db);
        for (index, port) in track.audio_inputs.iter().enumerate() {
            let factor = if track.audio_inputs.len() == 2 {
                if index == 0 {
                    left
                } else {
                    right
                }
            } else {
                1.0
            };
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing audio input port"))?
                .set_gain(base * factor);
        }
        Ok(())
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        validate_midi_input_events(events)?;
        let port = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?
            .midi_input
            .ok_or_else(|| anyhow!("backend track has no MIDI input {track_id:?}"))?;
        let input = self
            .session
            .port_mut(port)
            .ok_or_else(|| anyhow!("missing MIDI input port"))?;
        for event in events {
            let accepted = match input {
                Port::DummyMidi(input) => input.queue_msg_next_cycle(0, &event.data),
                Port::ExternalMidi(input) => input.push_incoming(0, &event.data),
                _ => return Err(anyhow!("track MIDI input has an incompatible port type")),
            };
            if !accepted {
                return Err(anyhow!("MIDI input injection staging is full"));
            }
        }
        Ok(())
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        let fx = track
            .fx
            .as_mut()
            .ok_or_else(|| anyhow!("track has no processor"))?;
        let title = track.port_name_base.clone();
        match control {
            BackendTrackFxControl::SetActive(active) => {
                fx.active = active;
                self.session.set_tiny_synth_fx_active(&title, active);
            }
            BackendTrackFxControl::SetVisible(visible) => fx.visible = visible,
            BackendTrackFxControl::ToggleOrRecover => fx.visible = !fx.visible,
            BackendTrackFxControl::RestoreState(state) => {
                let replacement =
                    shoop_engine::tiny_synth_fx::TinySynthFxControlState::from_encoded(
                        self.sample_rate as f32,
                        &state,
                    )?;
                let processor = replacement.prepare_processor(
                    self.sample_rate as f32,
                    track.audio_inputs.len(),
                    self.buffer_size as usize,
                )?;
                let displaced = self.session.set_tiny_synth_fx_processor(title, processor);
                drop(displaced);
                fx.control = replacement;
            }
            BackendTrackFxControl::ClearLogs => {}
            BackendTrackFxControl::TinySynthFx(control) => match control {
                TinySynthFxControl::SelectPreset(id) => {
                    fx.control.select_preset(&id)?;
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.select_preset(&id);
                    }
                }
                TinySynthFxControl::SetMasterGainDb(value) => {
                    fx.control.set_master_gain_db(value)?;
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.set_master_gain_db(value);
                    }
                }
                TinySynthFxControl::SetReverbEnabled(value) => {
                    fx.control.set_reverb_enabled(value);
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.set_reverb_enabled(value);
                    }
                }
                TinySynthFxControl::SetReverbAmount(value) => {
                    fx.control.set_reverb_amount(value)?;
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.set_reverb_amount(value);
                    }
                }
                TinySynthFxControl::SetDistortionEnabled(value) => {
                    fx.control.set_distortion_enabled(value);
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.set_distortion_enabled(value);
                    }
                }
                TinySynthFxControl::SetDistortionDrive(value) => {
                    fx.control.set_distortion_drive(value)?;
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.set_distortion_drive(value);
                    }
                }
                TinySynthFxControl::Panic => {
                    if let Some(processor) = self.session.tiny_synth_fx_processor_mut(&title) {
                        processor.panic();
                    }
                }
            },
        }
        Ok(())
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        let track = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        Ok(track.fx.as_ref().map(|fx| fx.control.encode()))
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let channels = self
            .loop_channels
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        channels.gain = gain.clamp(0.0, 1.0);
        apply_loop_gain_balance(&mut self.session, channels)
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        let channels = self
            .loop_channels
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        channels.balance = balance.clamp(-1.0, 1.0);
        apply_loop_gain_balance(&mut self.session, channels)
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        let mut audio_requests = Vec::with_capacity(requests.len());
        let mut midi_captures = Vec::new();
        for request in requests {
            let engine_loop = self.engine_loop_index(request.loop_id)?;
            audio_requests.push(shoop_engine::session::AudioRingbufferAdoption {
                loop_idx: engine_loop,
                reverse_start_cycle: request.reverse_start_cycle,
                cycles_length: request.cycles_length,
                go_to_cycle: request.go_to_cycle,
                go_to_mode: to_engine_mode(request.go_to_mode),
            });
            let Some(channels) = self.loop_channels.get(&request.loop_id) else {
                return Err(anyhow!(
                    "unknown backend loop channels {:?}",
                    request.loop_id
                ));
            };
            if channels.midi.is_empty() {
                continue;
            }
            let input = self
                .tracks
                .values()
                .find(|track| track.loops.contains(&request.loop_id))
                .and_then(|track| track.midi_input)
                .ok_or_else(|| anyhow!("missing MIDI input for loop {:?}", request.loop_id))?;
            let port = self
                .session
                .port(input)
                .and_then(Port::midi)
                .ok_or_else(|| anyhow!("missing MIDI input port"))?;
            let mut captured = MidiStorage::with_capacity_elems(1024);
            port.snapshot_ringbuffer_into(&mut captured);
            let sync = self
                .session
                .loop_(engine_loop)
                .and_then(|loop_| loop_.sync_source());
            let cycle_len = sync.map(|state| state.length).unwrap_or(0);
            let sync_pos = sync.map(|state| state.position).unwrap_or(0);
            let data_len = port.ringbuffer_n_samples() as usize;
            let (wanted, start, end) = grab_window(request, cycle_len, sync_pos, data_len);
            let messages = captured
                .iter()
                .filter(|message| {
                    let time = message.time as usize;
                    time >= start && time < end
                })
                .map(|message| message.at_time(message.time.saturating_sub(start as u32)))
                .collect::<Vec<_>>();
            for channel in &channels.midi {
                midi_captures.push((*channel, messages.clone(), wanted as u32));
            }
        }
        self.session.adopt_audio_ringbuffers(&audio_requests)?;
        for (channel, messages, length) in midi_captures {
            self.session
                .midi_channel_mut(channel)
                .ok_or_else(|| anyhow!("missing MIDI loop channel"))?
                .set_contents(&messages, length, None);
        }
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        channels
            .audio
            .iter()
            .map(|channel| {
                self.session
                    .audio_channel(*channel)
                    .map(|channel| Arc::from(channel.data()))
                    .ok_or_else(|| anyhow!("missing audio loop channel"))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }

    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        let channel_count = channels.audio.len();
        let Some(index) = channels.audio.get(channel) else {
            return Ok(BackendAudioDataChunk {
                channel,
                channel_count,
                offset,
                ..Default::default()
            });
        };
        let channel_ref = self
            .session
            .audio_channel(*index)
            .ok_or_else(|| anyhow!("missing audio loop channel"))?;
        let revision = u64::from(channel_ref.data_seq_nr());
        let total_samples = channel_ref.length();
        let samples = channel_ref.data_range(offset, max_samples);
        if u64::from(channel_ref.data_seq_nr()) != revision {
            return Err(anyhow!("audio content changed during chunk capture"));
        }
        Ok(BackendAudioDataChunk {
            content_revision: revision,
            channel,
            channel_count,
            offset,
            total_samples,
            samples,
        })
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        let target = self.engine_loop_index(loop_id)?;
        let source = source.map(|id| self.engine_loop_index(id)).transpose()?;
        self.session.set_loop_sync_source(target, source)?;
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        if let Some(delay) = cycles_delay {
            self.session
                .loop_mut(engine_loop)
                .ok_or_else(|| anyhow!("missing engine loop"))?
                .plan_transition(to_engine_mode(mode), Some(delay), None);
        } else {
            self.session
                .set_loop_mode(engine_loop, to_engine_mode(mode))?;
        }
        Ok(())
    }

    fn transition_loop_aligned(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    ) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        self.session
            .loop_mut(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .plan_transition(to_engine_mode(mode), cycles_delay, align_to_sync_at);
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        self.session
            .loop_mut(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .clear(0);
        Ok(())
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        self.capture_session_data()
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let (mut replacement, mapping) = self.build_replacement(session)?;
        replacement.elapsed_frame_numerator = self.elapsed_frame_numerator;
        replacement.processed_frames = self.processed_frames;
        replacement.xruns = self.xruns;
        replacement.callback_count = self.callback_count;
        replacement.input_peak = self.input_peak;
        replacement.output_peak = self.output_peak;
        replacement.last_quantum = self.last_quantum;
        *self = replacement;
        Ok(mapping)
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let local = self
            .connection_ports
            .get(&port_id)
            .ok_or_else(|| anyhow!("unknown backend port {port_id:?}"))?;
        let local_direction = local.descriptor.direction;
        let local_data_type = local.descriptor.data_type;
        let registry_id = local.registry_id;
        let is_web_midi = self.mode == EngineBackendMode::Physical
            && local_data_type == BackendPortDataType::Midi
            && external_port.starts_with("webmidi:");
        let candidate = self
            .external_connections
            .mock_ports()
            .iter()
            .find(|candidate| candidate.name == external_port);
        let Some(candidate) = candidate else {
            if is_web_midi {
                let compatible_prefix = match local_direction {
                    BackendPortDirection::Input => "webmidi:source:",
                    BackendPortDirection::Output => "webmidi:sink:",
                };
                if !external_port.starts_with(compatible_prefix) {
                    return Err(anyhow!("external port is incompatible: {external_port}"));
                }
                if connected {
                    self.desired_web_midi_connections
                        .insert((port_id, external_port.to_owned()));
                } else {
                    self.desired_web_midi_connections
                        .remove(&(port_id, external_port.to_owned()));
                }
                self.connection_revision = self.connection_revision.wrapping_add(1);
                return Ok(());
            }
            return Err(anyhow!("external port disappeared: {external_port}"));
        };
        if candidate.direction != engine_direction(opposite_backend_direction(local_direction))
            || candidate.data_type != engine_data_type(local_data_type)
        {
            return Err(anyhow!("external port is incompatible: {external_port}"));
        }
        if connected {
            self.external_connections
                .connect(registry_id, external_port)?;
            if is_web_midi {
                self.desired_web_midi_connections
                    .insert((port_id, external_port.to_owned()));
            }
        } else {
            self.external_connections
                .disconnect(registry_id, external_port)?;
            if is_web_midi {
                self.desired_web_midi_connections
                    .remove(&(port_id, external_port.to_owned()));
            }
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        Ok(())
    }

    fn advance(&mut self, elapsed: Duration) {
        if self.mode == EngineBackendMode::Physical {
            return;
        }
        self.elapsed_frame_numerator = self
            .elapsed_frame_numerator
            .saturating_add(elapsed.as_nanos().saturating_mul(self.sample_rate as u128));
        let due = self.elapsed_frame_numerator / NANOSECONDS_PER_SECOND;
        let max_frames = self.buffer_size.saturating_mul(MAX_CYCLES_PER_ADVANCE) as u128;
        let processed = due.min(max_frames) as u32;
        self.elapsed_frame_numerator -= processed as u128 * NANOSECONDS_PER_SECOND;
        if due > max_frames {
            self.elapsed_frame_numerator = 0;
            self.xruns = self.xruns.saturating_add(1);
        }
        self.advance_frames(processed);
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let track_ids = self.tracks.keys().copied().collect::<Vec<_>>();
        for track_id in track_ids {
            self.apply_engine_track_routing(track_id)?;
        }
        let mut tracks = BTreeMap::new();
        for (id, track) in &self.tracks {
            let input_peaks = track
                .audio_inputs
                .iter()
                .map(|port| {
                    self.session
                        .port(*port)
                        .and_then(Port::audio)
                        .map(|port| amplitude_db(port.input_peak()))
                        .unwrap_or(-200.0)
                })
                .collect();
            let output_peaks = track
                .audio_outputs
                .iter()
                .map(|port| {
                    self.session
                        .port(*port)
                        .and_then(Port::audio)
                        .map(|port| amplitude_db(port.output_peak()))
                        .unwrap_or(-200.0)
                })
                .collect();
            let input_midi_activity = track.midi_input.is_some_and(|port| {
                self.session
                    .port(port)
                    .and_then(Port::midi)
                    .is_some_and(|port| port.n_input_events() > 0 || port.n_notes_active() > 0)
            });
            let output_midi_activity = track.midi_output.is_some_and(|port| {
                self.session
                    .port(port)
                    .and_then(Port::midi)
                    .is_some_and(|port| port.n_output_events() > 0 || port.n_notes_active() > 0)
            });
            tracks.insert(
                *id,
                BackendTrackState {
                    topology: track.topology.clone(),
                    fx: track.fx.as_ref().map(engine_tiny_fx_state),
                    audio_channels: track.audio_outputs.len() as u32,
                    midi: track.midi_input.is_some(),
                    output_gain_db: track.output_gain_db,
                    output_balance: track.output_balance,
                    output_muted: track.output_muted,
                    input_gain_db: track.input_gain_db,
                    input_balance: track.input_balance,
                    input_monitoring: track.input_monitoring,
                    input_peaks,
                    output_peaks,
                    input_midi_activity,
                    output_midi_activity,
                },
            );
        }
        let mut loops = BTreeMap::new();
        for (id, engine_loop) in &self.loops {
            let Some(state) = self.session.loop_(*engine_loop) else {
                continue;
            };
            let channels = self.loop_channels.get(id);
            let audio: Vec<_> = channels
                .into_iter()
                .flat_map(|channels| &channels.audio)
                .filter_map(|channel| self.session.audio_channel(*channel))
                .collect();
            let midi_activity = channels
                .into_iter()
                .flat_map(|channels| &channels.midi)
                .filter_map(|channel| self.session.midi_channel(*channel))
                .any(|channel| channel.n_events_triggered() > 0 || channel.n_notes_active() > 0);
            loops.insert(
                *id,
                BackendLoopState {
                    mode: from_engine_mode(state.mode()),
                    length: state.length(),
                    position: state.position(),
                    next_mode: state
                        .first_planned_transition()
                        .map(|(mode, _)| from_engine_mode(mode)),
                    next_transition_delay: state.first_planned_transition().map(|(_, delay)| delay),
                    stereo: channels.is_some_and(|channels| {
                        channels
                            .audio_modes
                            .iter()
                            .filter(|mode| {
                                matches!(mode, BackendChannelMode::Direct | BackendChannelMode::Wet)
                            })
                            .count()
                            == 2
                    }),
                    gain: channels.map(|channels| channels.gain).unwrap_or(1.0),
                    balance: channels.map(|channels| channels.balance).unwrap_or(0.0),
                    audio_peaks: audio
                        .iter()
                        .map(|channel| amplitude_db(channel.output_peak()))
                        .collect(),
                    midi_activity,
                },
            );
        }
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: 0.0,
                xruns: self.xruns,
                buffer_size: if self.mode == EngineBackendMode::Physical {
                    self.last_quantum
                } else {
                    self.buffer_size
                },
                sample_rate: self.sample_rate,
                driver_state: if self.mode == EngineBackendMode::Physical {
                    BackendDriverState::Running
                } else {
                    BackendDriverState::Dummy
                },
                callback_count: self.callback_count,
                processed_frames: self.processed_frames,
                input_peak: self.input_peak,
                output_peak: self.output_peak,
                callback_budget_overruns: 0,
                render_discontinuities: 0,
                memory_growths: 0,
                command_overflows: 0,
                storage_low_channels: self
                    .loop_channels
                    .values()
                    .flat_map(|channels| &channels.audio)
                    .filter_map(|channel| self.session.audio_channel(*channel))
                    .filter(|channel| {
                        channel
                            .storage_remaining()
                            .is_some_and(|remaining| remaining <= self.sample_rate as usize)
                    })
                    .count()
                    .min(u32::MAX as usize) as u32,
                storage_exhaustions: self
                    .loop_channels
                    .values()
                    .flat_map(|channels| &channels.audio)
                    .filter_map(|channel| self.session.audio_channel(*channel))
                    .map(|channel| channel.storage_exhaustions())
                    .sum(),
            },
            audio_drivers: self.audio_driver_runtime_state(),
            tracks,
            loops,
            connections: self.connection_snapshot(),
        })
    }

    fn wait_idle(&mut self) {
        let _ = self.apply_graph_changes();
    }
}

fn from_engine_mode(mode: LoopMode) -> BackendLoopMode {
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

fn to_engine_mode(mode: BackendLoopMode) -> LoopMode {
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

#[derive(Clone, Debug)]
pub struct FakeConnectionControl {
    state: Arc<Mutex<FakeConnectionState>>,
}

#[derive(Debug)]
struct FakeConnectionState {
    revision: u64,
    available: bool,
    ports: BTreeMap<BackendPortId, BackendPortDescriptor>,
    external_ports: BTreeMap<String, (BackendPortDirection, BackendPortDataType)>,
    connected: BTreeSet<(BackendPortId, String)>,
    pending: Vec<(BackendPortId, String, bool)>,
    failures: Vec<BackendConnectionFailure>,
    defer_mutations: bool,
    fail_next: Option<String>,
}

impl Default for FakeConnectionState {
    fn default() -> Self {
        let mut external_ports = BTreeMap::new();
        for (name, direction, data_type) in [
            (
                "system:capture_1",
                BackendPortDirection::Output,
                BackendPortDataType::Audio,
            ),
            (
                "system:capture_2",
                BackendPortDirection::Output,
                BackendPortDataType::Audio,
            ),
            (
                "system:playback_1",
                BackendPortDirection::Input,
                BackendPortDataType::Audio,
            ),
            (
                "system:playback_2",
                BackendPortDirection::Input,
                BackendPortDataType::Audio,
            ),
            (
                "controller:midi_out",
                BackendPortDirection::Output,
                BackendPortDataType::Midi,
            ),
            (
                "synth:midi_in",
                BackendPortDirection::Input,
                BackendPortDataType::Midi,
            ),
        ] {
            external_ports.insert(name.to_owned(), (direction, data_type));
        }
        Self {
            revision: 1,
            available: true,
            ports: BTreeMap::new(),
            external_ports,
            connected: BTreeSet::new(),
            pending: Vec::new(),
            failures: Vec::new(),
            defer_mutations: false,
            fail_next: None,
        }
    }
}

impl FakeConnectionControl {
    fn with_state<T>(&self, apply: impl FnOnce(&mut FakeConnectionState) -> T) -> T {
        apply(&mut self.state.lock().unwrap_or_else(|error| error.into_inner()))
    }

    pub fn add_external_port(
        &self,
        name: impl Into<String>,
        direction: BackendPortDirection,
        data_type: BackendPortDataType,
    ) {
        self.with_state(|state| {
            state
                .external_ports
                .insert(name.into(), (direction, data_type));
            state.revision = state.revision.wrapping_add(1);
        });
    }

    pub fn remove_external_port(&self, name: &str) {
        self.with_state(|state| {
            state.external_ports.remove(name);
            state.connected.retain(|(_, endpoint)| endpoint != name);
            state.pending.retain(|(_, endpoint, _)| endpoint != name);
            state.revision = state.revision.wrapping_add(1);
        });
    }

    pub fn externally_set_connected(
        &self,
        port_id: BackendPortId,
        external_port: impl Into<String>,
        connected: bool,
    ) {
        self.with_state(|state| {
            apply_fake_connection(state, port_id, external_port.into(), connected);
        });
    }

    pub fn set_available(&self, available: bool) {
        self.with_state(|state| {
            state.available = available;
            state.revision = state.revision.wrapping_add(1);
        });
    }

    pub fn defer_mutations(&self, defer: bool) {
        self.with_state(|state| state.defer_mutations = defer);
    }

    pub fn fail_next_mutation(&self, message: impl Into<String>) {
        self.with_state(|state| state.fail_next = Some(message.into()));
    }

    pub fn complete_pending(&self, succeed: bool) {
        self.with_state(|state| {
            for (port_id, external_port, connected) in std::mem::take(&mut state.pending) {
                if succeed {
                    apply_fake_connection(state, port_id, external_port, connected);
                } else {
                    state.failures.push(BackendConnectionFailure {
                        port_id,
                        external_port,
                        desired_connected: connected,
                        message: "injected deferred connection failure".to_owned(),
                    });
                    state.revision = state.revision.wrapping_add(1);
                }
            }
        });
    }

    pub fn pending_len(&self) -> usize {
        self.with_state(|state| state.pending.len())
    }

    pub fn port_id_by_name(&self, name: &str) -> Option<BackendPortId> {
        self.with_state(|state| {
            state
                .ports
                .values()
                .find(|port| port.name == name)
                .map(|port| port.id)
        })
    }
}

fn empty_audio_content(mode: BackendChannelMode) -> BackendAudioContent {
    BackendAudioContent {
        mode,
        samples: Vec::new(),
        gain: 1.0,
        start_offset: 0,
        preplay: 0,
    }
}

fn empty_midi_content(mode: BackendChannelMode) -> BackendMidiContent {
    BackendMidiContent {
        mode,
        length: 0,
        start_state: Vec::new(),
        events: Vec::new(),
        start_offset: 0,
        preplay: 0,
    }
}

fn apply_fake_connection(
    state: &mut FakeConnectionState,
    port_id: BackendPortId,
    external_port: String,
    connected: bool,
) {
    let key = (port_id, external_port);
    if connected {
        state.connected.insert(key);
    } else {
        state.connected.remove(&key);
    }
    state.revision = state.revision.wrapping_add(1);
}

#[derive(Clone, Debug, Default)]
pub struct FakeAudioDriverControl {
    state: Arc<Mutex<FakeAudioDriverControlState>>,
}

#[derive(Debug, Default)]
struct FakeAudioDriverControlState {
    fail_next_switch: Option<String>,
    fail_switch_after: Option<(usize, String)>,
    corrupt_next_replacement_mapping: bool,
    preflight_sample_rate_override: Option<u32>,
}

impl FakeAudioDriverControl {
    pub fn fail_next_switch(&self, message: impl Into<String>) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .fail_next_switch = Some(message.into());
    }

    pub fn fail_switch_after(&self, successful_switches: usize, message: impl Into<String>) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .fail_switch_after = Some((successful_switches, message.into()));
    }

    pub fn corrupt_next_replacement_mapping(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .corrupt_next_replacement_mapping = true;
    }

    pub fn set_preflight_sample_rate_override(&self, sample_rate: Option<u32>) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .preflight_sample_rate_override = sample_rate;
    }
}

#[derive(Debug)]
pub struct FakeBackend {
    status: BackendStatus,
    active_audio_driver: ResolvedAudioDriverConfig,
    tracks: BTreeMap<BackendTrackId, FakeTrack>,
    loops: BTreeMap<BackendLoopId, BackendLoopState>,
    sync_sources: BTreeMap<BackendLoopId, Option<BackendLoopId>>,
    next_loop_id: u64,
    next_track_id: u64,
    next_port_id: u64,
    fail_track_creation_after: Option<usize>,
    fail_next_session_replace: Option<String>,
    failed_midi_input_tracks: BTreeSet<BackendTrackId>,
    processor_catalog: Arc<[TrackProcessorDescriptor]>,
    default_fx_state_string: String,
    fail_fx_state_restore: bool,
    audio_driver_control: FakeAudioDriverControl,
    operations: Vec<FakeOperation>,
    connections: FakeConnectionControl,
    loop_content: BTreeMap<BackendLoopId, BackendLoopContent>,
}

#[derive(Debug)]
struct FakeTrack {
    port_name_base: String,
    state: BackendTrackState,
    loops: Vec<BackendLoopId>,
    ports: Vec<BackendPortId>,
    fx_state_string: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FakeOperation {
    CreateLoop(BackendLoopId),
    CreateTrack(BackendTrackId),
    AddTrackLoop(BackendTrackId, BackendLoopId),
    SetTrackControl(BackendTrackId, BackendTrackControl),
    SetLoopGain(BackendLoopId, f32),
    SetLoopBalance(BackendLoopId, f32),
    GrabLoops(Vec<BackendGrabRequest>),
    SetSyncSource(BackendLoopId, Option<BackendLoopId>),
    Transition(BackendLoopId, BackendLoopMode, Option<u32>),
    Clear(BackendLoopId),
    InjectMidiInput(BackendTrackId, Vec<BackendMidiEvent>),
    SetPortConnected(BackendPortId, String, bool),
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self {
            status: BackendStatus {
                buffer_size: 256,
                sample_rate: 48_000,
                ..Default::default()
            },
            active_audio_driver: ResolvedAudioDriverConfig {
                configured: AudioDriverConfig::default(),
                sample_rate: 48_000,
                buffer_size: 256,
                instance_name: "fake dummy".to_owned(),
            },
            tracks: BTreeMap::new(),
            loops: BTreeMap::new(),
            sync_sources: BTreeMap::new(),
            next_loop_id: 1,
            next_track_id: 1,
            next_port_id: 1,
            fail_track_creation_after: None,
            fail_next_session_replace: None,
            failed_midi_input_tracks: BTreeSet::new(),
            processor_catalog: Arc::from([]),
            default_fx_state_string: "{}".to_owned(),
            fail_fx_state_restore: false,
            audio_driver_control: FakeAudioDriverControl::default(),
            operations: Vec::new(),
            connections: FakeConnectionControl {
                state: Arc::new(Mutex::new(FakeConnectionState::default())),
            },
            loop_content: BTreeMap::new(),
        }
    }
}

impl FakeBackend {
    pub fn operations(&self) -> &[FakeOperation] {
        &self.operations
    }

    pub fn fail_track_creation_after(&mut self, successful_creations: usize) {
        self.fail_track_creation_after = Some(successful_creations);
    }

    pub fn fail_midi_input_for(&mut self, track_id: BackendTrackId) {
        self.failed_midi_input_tracks.insert(track_id);
    }

    pub fn set_track_processor_catalog(&mut self, catalog: Vec<TrackProcessorDescriptor>) {
        self.processor_catalog = Arc::from(catalog);
    }

    pub fn set_default_fx_state_string(&mut self, state: impl Into<String>) {
        self.default_fx_state_string = state.into();
    }

    pub fn set_fail_fx_state_restore(&mut self, fail: bool) {
        self.fail_fx_state_restore = fail;
    }

    pub fn set_track_fx_state(
        &mut self,
        track_id: BackendTrackId,
        state: TrackFxState,
        state_string: impl Into<String>,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?;
        track.state.fx = Some(state);
        track.fx_state_string = Some(state_string.into());
        Ok(())
    }

    pub fn fail_next_driver_switch(&mut self, message: impl Into<String>) {
        self.audio_driver_control.fail_next_switch(message);
    }

    pub fn fail_next_session_replace(&mut self, message: impl Into<String>) {
        self.fail_next_session_replace = Some(message.into());
    }

    pub fn set_preflight_sample_rate_override(&mut self, sample_rate: Option<u32>) {
        self.audio_driver_control
            .set_preflight_sample_rate_override(sample_rate);
    }

    pub fn audio_driver_control(&self) -> FakeAudioDriverControl {
        self.audio_driver_control.clone()
    }

    pub fn connection_control(&self) -> FakeConnectionControl {
        self.connections.clone()
    }

    fn next_port_descriptor(
        &mut self,
        name: String,
        data_type: BackendPortDataType,
        direction: BackendPortDirection,
        role: BackendPortRole,
    ) -> BackendPortDescriptor {
        let descriptor = BackendPortDescriptor {
            id: BackendPortId::from_raw(self.next_port_id),
            name,
            data_type,
            direction,
            role,
        };
        self.next_port_id = self.next_port_id.saturating_add(1);
        self.connections.with_state(|state| {
            state.ports.insert(descriptor.id, descriptor.clone());
            state.revision = state.revision.wrapping_add(1);
        });
        descriptor
    }

    fn connection_snapshot(&self) -> BackendConnectionSnapshot {
        self.connections.with_state(|state| {
            let application_ports = state.ports.clone();
            let host_ports = state
                .external_ports
                .iter()
                .map(|(id, (direction, data_type))| {
                    (
                        id.clone(),
                        BackendHostPortDescriptor {
                            id: id.clone(),
                            name: id.clone(),
                            data_type: *data_type,
                            direction: *direction,
                        },
                    )
                })
                .collect();
            let confirmed_links = state
                .connected
                .iter()
                .map(|(application_port_id, host_port_id)| BackendConfirmedLink {
                    application_port_id: *application_port_id,
                    host_port_id: host_port_id.clone(),
                })
                .collect();
            BackendConnectionSnapshot {
                revision: state.revision,
                available: state.available,
                application_ports,
                host_ports,
                confirmed_links,
                failures: std::mem::take(&mut state.failures),
            }
        })
    }

    fn create_external_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected External dry/wet topology"));
        };
        if let Some(remaining) = self.fail_track_creation_after.as_mut() {
            if *remaining == 0 {
                self.fail_track_creation_after = None;
                return Err(anyhow!("injected track creation failure"));
            }
            *remaining -= 1;
        }
        let mut ports = Vec::new();
        for index in 0..dry_audio_channels {
            ports.push(self.next_port_descriptor(
                format!("{}_audio_dry_in_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            ports.push(self.next_port_descriptor(
                format!("{}_audio_dry_send_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioSend,
            ));
        }
        for index in 0..wet_audio_channels {
            ports.push(self.next_port_descriptor(
                format!("{}_audio_wet_return_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioReturn,
            ));
            ports.push(self.next_port_descriptor(
                format!("{}_audio_wet_out_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
        }
        if dry_midi {
            ports.push(self.next_port_descriptor(
                format!("{}_dry_midi_in", request.port_name_base),
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
            ));
            ports.push(self.next_port_descriptor(
                format!("{}_dry_midi_send", request.port_name_base),
                BackendPortDataType::Midi,
                BackendPortDirection::Output,
                BackendPortRole::MidiSend,
            ));
        }
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            FakeTrack {
                port_name_base: request.port_name_base,
                state: BackendTrackState {
                    topology: request.topology,
                    audio_channels: wet_audio_channels,
                    midi: dry_midi,
                    ..Default::default()
                },
                loops: Vec::new(),
                ports: ports.iter().map(|port| port.id).collect(),
                fx_state_string: None,
            },
        );
        self.operations.push(FakeOperation::CreateTrack(track_id));
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.add_loop_to_track(track_id)?);
        }
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn create_processed_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetProcessor {
            processor_type,
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected processed dry/wet topology"));
        };
        if let Some(remaining) = self.fail_track_creation_after.as_mut() {
            if *remaining == 0 {
                self.fail_track_creation_after = None;
                return Err(anyhow!("injected track creation failure"));
            }
            *remaining -= 1;
        }
        let mut ports = Vec::new();
        for index in 0..dry_audio_channels {
            ports.push(self.next_port_descriptor(
                format!("{}_audio_dry_in_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
        }
        for index in 0..wet_audio_channels {
            ports.push(self.next_port_descriptor(
                format!("{}_audio_wet_out_{}", request.port_name_base, index + 1),
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
        }
        if dry_midi {
            ports.push(self.next_port_descriptor(
                format!("{}_dry_midi_in", request.port_name_base),
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
            ));
        }
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            FakeTrack {
                port_name_base: request.port_name_base,
                state: BackendTrackState {
                    topology: request.topology,
                    fx: Some(TrackFxState {
                        processor_type: TrackProcessorTypeId::new(processor_type),
                        active: false,
                        visible: false,
                        lifecycle: FxLifecycle::Running,
                        generation: 1,
                        crash_summary: None,
                        logs: Arc::from([]),
                        editor: None,
                    }),
                    audio_channels: wet_audio_channels,
                    midi: dry_midi,
                    ..Default::default()
                },
                loops: Vec::new(),
                ports: ports.iter().map(|port| port.id).collect(),
                fx_state_string: Some(self.default_fx_state_string.clone()),
            },
        );
        self.operations.push(FakeOperation::CreateTrack(track_id));
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.add_loop_to_track(track_id)?);
        }
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn require_loop(&self, id: BackendLoopId) -> Result<()> {
        self.loops
            .contains_key(&id)
            .then_some(())
            .ok_or_else(|| anyhow!("unknown fake loop {id:?}"))
    }
}

impl Backend for FakeBackend {
    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(Arc::clone(&self.processor_catalog))
    }

    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        Ok(AudioDriverRuntimeState {
            supported: true,
            catalog: Arc::from([
                AudioDriverDescriptor {
                    kind: AudioDriverKind::Dummy,
                    available: true,
                    ..Default::default()
                },
                AudioDriverDescriptor {
                    kind: AudioDriverKind::Jack,
                    available: true,
                    ..Default::default()
                },
                AudioDriverDescriptor {
                    kind: AudioDriverKind::Cpal,
                    available: true,
                    hosts: vec!["default".to_owned(), "test".to_owned()],
                    input_devices: vec!["default".to_owned(), "input".to_owned()],
                    output_devices: vec!["default".to_owned(), "output".to_owned()],
                    midi_inputs: vec!["midi in".to_owned()],
                    midi_outputs: vec!["midi out".to_owned()],
                    ..Default::default()
                },
            ]),
            active: Some(self.active_audio_driver.clone()),
            ..Default::default()
        })
    }

    fn preflight_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        let (sample_rate, buffer_size, instance_name) = match config {
            AudioDriverConfig::Dummy(config) => {
                if config.sample_rate == 0 || config.buffer_size == 0 {
                    return Err(anyhow!(
                        "dummy sample rate and buffer size must be non-zero"
                    ));
                }
                (
                    config.sample_rate,
                    config.buffer_size,
                    "fake dummy".to_owned(),
                )
            }
            AudioDriverConfig::Jack(config) => (
                self.status.sample_rate,
                self.status.buffer_size,
                config.client_name.clone(),
            ),
            AudioDriverConfig::Cpal(config) => (
                if config.sample_rate == 0 {
                    self.status.sample_rate
                } else {
                    config.sample_rate
                },
                if config.buffer_size == 0 {
                    self.status.buffer_size
                } else {
                    config.buffer_size
                },
                config.output_device.clone(),
            ),
            AudioDriverConfig::WebAudio => {
                return Err(anyhow!("Web Audio is selected automatically"));
            }
        };
        Ok(ResolvedAudioDriverConfig {
            configured: config.clone(),
            sample_rate: self
                .audio_driver_control
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .preflight_sample_rate_override
                .unwrap_or(sample_rate),
            buffer_size,
            instance_name,
        })
    }

    fn switch_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
        confirmed_sample_rate: u32,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let resolved = self.preflight_audio_driver(config)?;
        let failure = {
            let mut control = self
                .audio_driver_control
                .state
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            if let Some(message) = control.fail_next_switch.take() {
                Some(message)
            } else if let Some((remaining, _)) = control.fail_switch_after.as_mut() {
                if *remaining == 0 {
                    control.fail_switch_after.take().map(|(_, message)| message)
                } else {
                    *remaining -= 1;
                    None
                }
            } else {
                None
            }
        };
        if let Some(message) = failure {
            return Err(anyhow!(message));
        }
        if resolved.sample_rate != confirmed_sample_rate {
            return Err(anyhow!(
                "resolved target sample rate changed from {confirmed_sample_rate} to {}",
                resolved.sample_rate
            ));
        }
        if session.sample_rate != resolved.sample_rate {
            return Err(anyhow!(
                "prepared session sample rate does not match target"
            ));
        }
        let previous = self.active_audio_driver.clone();
        self.status.sample_rate = resolved.sample_rate;
        self.status.buffer_size = resolved.buffer_size;
        self.active_audio_driver = resolved;
        match self.replace_session(session) {
            Ok(mut replacement) => {
                let corrupt = {
                    let mut control = self
                        .audio_driver_control
                        .state
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    std::mem::take(&mut control.corrupt_next_replacement_mapping)
                };
                if corrupt {
                    replacement.tracks.clear();
                }
                Ok(replacement)
            }
            Err(error) => {
                self.status.sample_rate = previous.sample_rate;
                self.status.buffer_size = previous.buffer_size;
                self.active_audio_driver = previous;
                Err(error)
            }
        }
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            BackendLoopState {
                mode: BackendLoopMode::Stopped,
                ..Default::default()
            },
        );
        self.sync_sources.insert(id, None);
        self.loop_content.insert(
            id,
            BackendLoopContent {
                source_id: id.raw(),
                length: 0,
                gain: 1.0,
                balance: 0.0,
                audio: Vec::new(),
                midi: Vec::new(),
            },
        );
        self.operations.push(FakeOperation::CreateLoop(id));
        Ok(id)
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match &request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels: *audio_channels,
                midi: *midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetExternal { .. } => self.create_external_track(request),
            BackendTrackTopology::DryWetProcessor { .. } => self.create_processed_track(request),
        }
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        if let Some(remaining) = self.fail_track_creation_after.as_mut() {
            if *remaining == 0 {
                self.fail_track_creation_after = None;
                return Err(anyhow!("injected track creation failure"));
            }
            *remaining -= 1;
        }
        let audio_channels = usize::try_from(request.audio_channels)
            .map_err(|_| anyhow!("direct track audio channel count does not fit this target"))?;
        let port_capacity = audio_channels
            .checked_mul(2)
            .and_then(|count| count.checked_add(2))
            .ok_or_else(|| anyhow!("direct track audio channel count is too large"))?;
        let mut ports = Vec::with_capacity(port_capacity);
        for index in 0..request.audio_channels {
            let suffix = if request.audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            ports.push(self.next_port_descriptor(
                format!("{}_direct_in{suffix}", request.port_name_base),
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            ports.push(self.next_port_descriptor(
                format!("{}_direct_out{suffix}", request.port_name_base),
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
        }
        if request.midi {
            ports.push(self.next_port_descriptor(
                format!("{}_direct_midi_in", request.port_name_base),
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
            ));
            ports.push(self.next_port_descriptor(
                format!("{}_direct_midi_out", request.port_name_base),
                BackendPortDataType::Midi,
                BackendPortDirection::Output,
                BackendPortRole::MidiOutput,
            ));
        }
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            FakeTrack {
                port_name_base: request.port_name_base,
                state: BackendTrackState {
                    topology: BackendTrackTopology::Direct {
                        audio_channels: request.audio_channels,
                        midi: request.midi,
                    },
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                    ..Default::default()
                },
                loops: Vec::new(),
                ports: ports.iter().map(|port| port.id).collect(),
                fx_state_string: None,
            },
        );
        self.operations.push(FakeOperation::CreateTrack(track_id));
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.add_loop_to_track(track_id)?);
        }
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        if !self.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown fake track {track_id:?}"));
        }
        let loop_id = self.create_loop()?;
        let topology = self.tracks[&track_id].state.topology.clone();
        self.tracks
            .get_mut(&track_id)
            .expect("track was checked")
            .loops
            .push(loop_id);
        let (audio, midi, output_channels) = match topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => (
                (0..audio_channels)
                    .map(|_| empty_audio_content(BackendChannelMode::Direct))
                    .collect::<Vec<_>>(),
                midi.then(|| vec![empty_midi_content(BackendChannelMode::Direct)])
                    .unwrap_or_default(),
                audio_channels,
            ),
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
            } => (
                (0..dry_audio_channels)
                    .map(|_| empty_audio_content(BackendChannelMode::Dry))
                    .chain(
                        (0..wet_audio_channels)
                            .map(|_| empty_audio_content(BackendChannelMode::Wet)),
                    )
                    .collect::<Vec<_>>(),
                dry_midi
                    .then(|| vec![empty_midi_content(BackendChannelMode::Dry)])
                    .unwrap_or_default(),
                wet_audio_channels,
            ),
        };
        if let Some(state) = self.loops.get_mut(&loop_id) {
            state.stereo = output_channels == 2;
            state.gain = 1.0;
            state.audio_peaks = vec![-200.0; audio.len()];
        }
        if let Some(content) = self.loop_content.get_mut(&loop_id) {
            content.audio = audio;
            content.midi = midi;
        }
        self.operations
            .push(FakeOperation::AddTrackLoop(track_id, loop_id));
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?;
        match control {
            BackendTrackControl::OutputGainDb(value) => track.state.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => track.state.output_balance = value,
            BackendTrackControl::OutputMute(value) => track.state.output_muted = value,
            BackendTrackControl::InputGainDb(value) => track.state.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => track.state.input_balance = value,
            BackendTrackControl::InputMonitoring(value) => track.state.input_monitoring = value,
        }
        self.operations
            .push(FakeOperation::SetTrackControl(track_id, control));
        Ok(())
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        validate_midi_input_events(events)?;
        if self.failed_midi_input_tracks.contains(&track_id) {
            return Err(anyhow!("injected fake MIDI input failure {track_id:?}"));
        }
        let track = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?;
        if !track.state.topology.has_midi() {
            return Err(anyhow!("fake track has no MIDI input {track_id:?}"));
        }
        self.operations
            .push(FakeOperation::InjectMidiInput(track_id, events.to_vec()));
        Ok(())
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        let fx = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?
            .state
            .fx
            .as_mut()
            .ok_or_else(|| anyhow!("track has no processor"))?;
        if self.fail_fx_state_restore && matches!(control, BackendTrackFxControl::RestoreState(_)) {
            return Err(anyhow!("injected processor state restore failure"));
        }
        match control {
            BackendTrackFxControl::SetActive(active) => fx.active = active,
            BackendTrackFxControl::SetVisible(visible) => fx.visible = visible,
            BackendTrackFxControl::ToggleOrRecover => {
                if matches!(
                    fx.lifecycle,
                    FxLifecycle::Crashed | FxLifecycle::Unavailable
                ) {
                    fx.lifecycle = FxLifecycle::Running;
                    fx.generation = fx.generation.saturating_add(1);
                    fx.visible = true;
                } else {
                    fx.visible = !fx.visible;
                }
            }
            BackendTrackFxControl::RestoreState(_) => {}
            BackendTrackFxControl::ClearLogs => fx.logs = Arc::from([]),
            BackendTrackFxControl::TinySynthFx(_) => {
                return Err(anyhow!("Tiny Synth/FX controls are unavailable"));
            }
        }
        Ok(())
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        Ok(self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?
            .fx_state_string
            .clone())
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.gain = gain.clamp(0.0, 1.0);
        if let Some(content) = self.loop_content.get_mut(&loop_id) {
            content.gain = state.gain;
        }
        self.operations
            .push(FakeOperation::SetLoopGain(loop_id, state.gain));
        Ok(())
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.balance = balance.clamp(-1.0, 1.0);
        if let Some(content) = self.loop_content.get_mut(&loop_id) {
            content.balance = state.balance;
        }
        self.operations
            .push(FakeOperation::SetLoopBalance(loop_id, state.balance));
        Ok(())
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        for request in requests {
            self.require_loop(request.loop_id)?;
        }
        for request in requests {
            let state = self.loops.get_mut(&request.loop_id).expect("loop checked");
            state.mode = request.go_to_mode;
            if let Some(cycles) = request.cycles_length {
                state.length = cycles.max(0) as u32;
                if let Some(content) = self.loop_content.get_mut(&request.loop_id) {
                    content.length = state.length;
                }
            }
        }
        self.operations
            .push(FakeOperation::GrabLoops(requests.to_vec()));
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        self.require_loop(loop_id)?;
        Ok(Some(
            self.loop_content
                .get(&loop_id)
                .ok_or_else(|| anyhow!("missing fake loop content"))?
                .audio
                .iter()
                .map(|channel| Arc::from(channel.samples.clone()))
                .collect(),
        ))
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.require_loop(loop_id)?;
        if let Some(source) = source {
            self.require_loop(source)?;
        }
        self.sync_sources.insert(loop_id, source);
        self.operations
            .push(FakeOperation::SetSyncSource(loop_id, source));
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.mode = mode;
        state.next_mode = None;
        state.next_transition_delay = None;
        self.operations
            .push(FakeOperation::Transition(loop_id, mode, cycles_delay));
        Ok(())
    }

    fn transition_loop_aligned(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        _align_to_sync_at: Option<u32>,
    ) -> Result<()> {
        self.transition_loop(loop_id, mode, cycles_delay)
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        *state = BackendLoopState {
            mode: BackendLoopMode::Stopped,
            ..Default::default()
        };
        if let Some(content) = self.loop_content.get_mut(&loop_id) {
            content.length = 0;
            for channel in &mut content.audio {
                channel.samples.clear();
            }
            for channel in &mut content.midi {
                channel.length = 0;
                channel.start_state.clear();
                channel.events.clear();
            }
        }
        self.operations.push(FakeOperation::Clear(loop_id));
        Ok(())
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        if self.loops.values().any(|state| {
            matches!(
                state.mode,
                BackendLoopMode::Recording
                    | BackendLoopMode::Replacing
                    | BackendLoopMode::RecordingDryIntoWet
            )
        }) {
            return Err(anyhow!("loop content is changing"));
        }
        let connections = self.connection_snapshot();
        let tracks = self
            .tracks
            .iter()
            .map(|(track_id, track)| {
                let loops = track
                    .loops
                    .iter()
                    .map(|loop_id| {
                        self.loop_content
                            .get(loop_id)
                            .cloned()
                            .ok_or_else(|| anyhow!("missing fake loop content"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                let ports = track
                    .ports
                    .iter()
                    .map(|port_id| {
                        let descriptor = connections
                            .application_ports
                            .get(port_id)
                            .ok_or_else(|| anyhow!("missing fake application port"))?;
                        Ok(BackendSessionPort {
                            source_id: port_id.raw(),
                            descriptor: descriptor.clone(),
                            external_connections: connections
                                .confirmed_links
                                .iter()
                                .filter(|link| link.application_port_id == *port_id)
                                .map(|link| link.host_port_id.clone())
                                .collect(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(BackendSessionTrack {
                    source_id: track_id.raw(),
                    port_name_base: track.port_name_base.clone(),
                    topology: track.state.topology.clone(),
                    state: track.state.clone(),
                    loops,
                    ports,
                    processor_state: track.fx_state_string.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(BackendSessionData {
            sample_rate: self.status.sample_rate,
            tracks,
            use_legacy_browser_default_routes: false,
        })
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        if let Some(message) = self.fail_next_session_replace.take() {
            return Err(anyhow!(message));
        }
        if session.sample_rate != self.status.sample_rate {
            return Err(anyhow!(
                "prepared session sample rate does not match backend"
            ));
        }
        let external_ports = self
            .connections
            .with_state(|state| state.external_ports.clone());
        let mut staged = FakeBackend::default();
        staged.status = self.status;
        staged.active_audio_driver = self.active_audio_driver.clone();
        staged.audio_driver_control = self.audio_driver_control.clone();
        staged.processor_catalog = Arc::clone(&self.processor_catalog);
        staged
            .default_fx_state_string
            .clone_from(&self.default_fx_state_string);
        staged.fail_fx_state_restore = self.fail_fx_state_restore;
        staged.connections.with_state(|state| {
            state.external_ports = external_ports;
        });
        let mut replacement = BackendSessionReplacement::default();
        for source_track in &session.tracks {
            if source_track.state.topology != source_track.topology {
                return Err(anyhow!("prepared session topology state is inconsistent"));
            }
            let created = staged.create_track(TrackRequest {
                port_name_base: source_track.port_name_base.clone(),
                topology: source_track.topology.clone(),
                initial_loops: source_track.loops.len(),
            })?;
            if created.ports.len() != source_track.ports.len() {
                return Err(anyhow!("prepared session port shape changed"));
            }
            match &source_track.topology {
                BackendTrackTopology::DryWetProcessor { .. } => {
                    let state = source_track
                        .processor_state
                        .clone()
                        .ok_or_else(|| anyhow!("processed track has no saved state"))?;
                    let track = staged
                        .tracks
                        .get_mut(&created.track_id)
                        .ok_or_else(|| anyhow!("missing staged processed track"))?;
                    track.fx_state_string = Some(state);
                    if source_track.state.fx.is_some() {
                        track.state.fx.clone_from(&source_track.state.fx);
                    }
                }
                _ if source_track.processor_state.is_some() => {
                    return Err(anyhow!("unprocessed track has processor state"));
                }
                _ => {}
            }
            for control in [
                BackendTrackControl::OutputGainDb(source_track.state.output_gain_db),
                BackendTrackControl::OutputBalance(source_track.state.output_balance),
                BackendTrackControl::OutputMute(source_track.state.output_muted),
                BackendTrackControl::InputGainDb(source_track.state.input_gain_db),
                BackendTrackControl::InputBalance(source_track.state.input_balance),
                BackendTrackControl::InputMonitoring(source_track.state.input_monitoring),
            ] {
                staged.set_track_control(created.track_id, control)?;
            }
            for (source_loop, loop_id) in source_track.loops.iter().zip(&created.loops) {
                let target_loop = staged
                    .loop_content
                    .get(loop_id)
                    .ok_or_else(|| anyhow!("missing staged loop content"))?;
                let source_audio_modes = source_loop
                    .audio
                    .iter()
                    .map(|channel| channel.mode)
                    .collect::<Vec<_>>();
                let target_audio_modes = target_loop
                    .audio
                    .iter()
                    .map(|channel| channel.mode)
                    .collect::<Vec<_>>();
                let source_midi_modes = source_loop
                    .midi
                    .iter()
                    .map(|channel| channel.mode)
                    .collect::<Vec<_>>();
                let target_midi_modes = target_loop
                    .midi
                    .iter()
                    .map(|channel| channel.mode)
                    .collect::<Vec<_>>();
                if source_audio_modes != target_audio_modes
                    || source_midi_modes != target_midi_modes
                {
                    return Err(anyhow!("prepared session channel shape changed"));
                }
                staged.loop_content.insert(
                    *loop_id,
                    BackendLoopContent {
                        source_id: loop_id.raw(),
                        ..source_loop.clone()
                    },
                );
                if let Some(state) = staged.loops.get_mut(loop_id) {
                    state.length = source_loop.length;
                    state.gain = source_loop.gain;
                    state.balance = source_loop.balance;
                    state.mode = BackendLoopMode::Stopped;
                }
                replacement.loops.insert(source_loop.source_id, *loop_id);
            }
            for (source_port, created_port) in source_track.ports.iter().zip(&created.ports) {
                replacement
                    .ports
                    .insert(source_port.source_id, created_port.id);
                for external in &source_port.external_connections {
                    staged.set_port_connected(created_port.id, external, true)?;
                }
            }
            replacement
                .tracks
                .insert(source_track.source_id, created.clone());
        }
        *self = staged;
        Ok(replacement)
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let result = self.connections.with_state(|state| {
            let port = state
                .ports
                .get(&port_id)
                .ok_or_else(|| anyhow!("unknown fake port {port_id:?}"))?;
            let (direction, data_type) = state
                .external_ports
                .get(external_port)
                .copied()
                .ok_or_else(|| anyhow!("external port disappeared: {external_port}"))?;
            if direction != opposite_backend_direction(port.direction)
                || data_type != port.data_type
            {
                return Err(anyhow!("external port is incompatible: {external_port}"));
            }
            if let Some(message) = state.fail_next.take() {
                return Err(anyhow!(message));
            }
            if state.defer_mutations {
                if !state
                    .pending
                    .iter()
                    .any(|pending| pending == &(port_id, external_port.to_owned(), connected))
                {
                    state
                        .pending
                        .push((port_id, external_port.to_owned(), connected));
                }
            } else {
                apply_fake_connection(state, port_id, external_port.to_owned(), connected);
            }
            Ok(())
        });
        if result.is_ok() {
            self.operations.push(FakeOperation::SetPortConnected(
                port_id,
                external_port.to_owned(),
                connected,
            ));
        }
        result
    }

    fn advance(&mut self, _elapsed: Duration) {}

    fn poll(&mut self) -> Result<BackendSnapshot> {
        Ok(BackendSnapshot {
            status: self.status,
            audio_drivers: self.audio_driver_state()?,
            tracks: self
                .tracks
                .iter()
                .map(|(id, track)| (*id, track.state.clone()))
                .collect(),
            loops: self.loops.clone(),
            connections: self.connection_snapshot(),
        })
    }

    fn wait_idle(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_wet_processor_mapping_is_ordered_and_clamps_unequal_shapes() {
        assert_eq!(
            dry_wet_processor_mapping(4, 1, true, 2, 16, true),
            DryWetProcessorMapping {
                dry_audio: vec![(0, 0), (1, 1)],
                wet_audio: vec![(0, 0)],
                dry_midi: true,
            }
        );
        assert_eq!(
            dry_wet_processor_mapping(1, 3, true, 16, 2, false),
            DryWetProcessorMapping {
                dry_audio: vec![(0, 0)],
                wet_audio: vec![(0, 0), (1, 1)],
                dry_midi: false,
            }
        );
    }

    #[test]
    fn dry_wet_routing_matches_monitor_and_transition_truth_table() {
        let cases = [
            (
                false,
                vec![],
                vec![],
                DryWetRoutingState {
                    dry_input_passthrough_muted: true,
                    wet_output_passthrough_muted: true,
                    processor_active: false,
                    force_monitoring_off: false,
                },
            ),
            (
                true,
                vec![],
                vec![],
                DryWetRoutingState {
                    dry_input_passthrough_muted: false,
                    wet_output_passthrough_muted: false,
                    processor_active: true,
                    force_monitoring_off: false,
                },
            ),
            (
                false,
                vec![BackendLoopMode::Recording],
                vec![],
                DryWetRoutingState {
                    dry_input_passthrough_muted: false,
                    wet_output_passthrough_muted: true,
                    processor_active: true,
                    force_monitoring_off: false,
                },
            ),
            (
                false,
                vec![],
                vec![BackendLoopMode::Replacing],
                DryWetRoutingState {
                    dry_input_passthrough_muted: false,
                    wet_output_passthrough_muted: true,
                    processor_active: true,
                    force_monitoring_off: false,
                },
            ),
            (
                false,
                vec![BackendLoopMode::PlayingDryThroughWet],
                vec![],
                DryWetRoutingState {
                    dry_input_passthrough_muted: true,
                    wet_output_passthrough_muted: false,
                    processor_active: true,
                    force_monitoring_off: false,
                },
            ),
            (
                true,
                vec![BackendLoopMode::RecordingDryIntoWet],
                vec![],
                DryWetRoutingState {
                    dry_input_passthrough_muted: true,
                    wet_output_passthrough_muted: false,
                    processor_active: true,
                    force_monitoring_off: true,
                },
            ),
            (
                false,
                vec![],
                vec![BackendLoopMode::RecordingDryIntoWet],
                DryWetRoutingState {
                    dry_input_passthrough_muted: true,
                    wet_output_passthrough_muted: false,
                    processor_active: true,
                    force_monitoring_off: true,
                },
            ),
        ];
        for (monitoring, current, next, expected) in cases {
            assert_eq!(dry_wet_routing_state(monitoring, &current, &next), expected);
        }
    }

    #[test]
    fn fake_backend_publishes_empty_and_future_processor_catalogs() {
        let mut backend = FakeBackend::default();
        assert!(backend.track_processor_catalog().unwrap().is_empty());
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new("future_browser_fx"),
            label: "Future browser FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                max_dry_audio_channels: Some(8),
                max_wet_audio_channels: Some(8),
                matching_audio_channels: false,
                midi: shoop_app_api::TrackProcessorMidiPolicy::Optional,
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
        backend.set_track_processor_catalog(vec![descriptor.clone()]);
        assert_eq!(
            backend.track_processor_catalog().unwrap().as_ref(),
            &[descriptor]
        );
    }

    #[test]
    fn fake_backend_creates_and_round_trips_external_dry_wet_tracks() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "fx".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        assert_eq!(created.ports.len(), 8);
        assert_eq!(
            created
                .ports
                .iter()
                .map(|port| port.role)
                .collect::<Vec<_>>(),
            vec![
                BackendPortRole::AudioInput,
                BackendPortRole::AudioSend,
                BackendPortRole::AudioInput,
                BackendPortRole::AudioSend,
                BackendPortRole::AudioReturn,
                BackendPortRole::AudioOutput,
                BackendPortRole::MidiInput,
                BackendPortRole::MidiSend,
            ]
        );
        let second_loop = backend.add_loop_to_track(created.track_id).unwrap();
        backend
            .set_port_connected(created.ports[0].id, "system:capture_1", true)
            .unwrap();
        for mode in [
            BackendLoopMode::Recording,
            BackendLoopMode::Playing,
            BackendLoopMode::Replacing,
            BackendLoopMode::PlayingDryThroughWet,
            BackendLoopMode::RecordingDryIntoWet,
            BackendLoopMode::Stopped,
        ] {
            backend.transition_loop(second_loop, mode, Some(1)).unwrap();
            assert_eq!(backend.poll().unwrap().loops[&second_loop].mode, mode);
        }
        backend
            .grab_loops(&[BackendGrabRequest {
                loop_id: second_loop,
                reverse_start_cycle: Some(-2),
                cycles_length: Some(2),
                go_to_cycle: Some(0),
                go_to_mode: BackendLoopMode::PlayingDryThroughWet,
            }])
            .unwrap();
        let mut captured = backend.capture_session().unwrap();
        captured.tracks[0].loops[0].length = 2;
        captured.tracks[0].loops[0].audio[0].samples = vec![0.25, -0.5];
        captured.tracks[0].loops[0].audio[2].samples = vec![0.75, -1.0];
        assert_eq!(
            captured.tracks[0].topology,
            captured.tracks[0].state.topology
        );
        assert_eq!(
            captured.tracks[0].loops[0]
                .audio
                .iter()
                .map(|channel| channel.mode)
                .collect::<Vec<_>>(),
            vec![
                BackendChannelMode::Dry,
                BackendChannelMode::Dry,
                BackendChannelMode::Wet,
            ]
        );
        assert_eq!(
            captured.tracks[0].loops[0].midi[0].mode,
            BackendChannelMode::Dry
        );
        assert_eq!(captured.tracks[0].loops.len(), 2);
        assert_eq!(
            captured.tracks[0].ports[0].external_connections,
            ["system:capture_1"]
        );

        let mut restored = FakeBackend::default();
        restored.replace_session(&captured).unwrap();
        assert_eq!(restored.capture_session().unwrap(), captured);
    }

    #[test]
    fn engine_backend_rejects_processed_sessions_before_replacement() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 1).unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "existing".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let before = backend.capture_session().unwrap();
        let mut unsupported = before.clone();
        let topology = BackendTrackTopology::DryWetExternal {
            dry_audio_channels: 1,
            wet_audio_channels: 1,
            dry_midi: false,
        };
        unsupported.tracks[0].topology = topology.clone();
        unsupported.tracks[0].state.topology = topology;
        assert!(backend.replace_session(&unsupported).is_err());
        assert_eq!(backend.capture_session().unwrap(), before);
    }

    fn backend_contract(backend: &mut dyn Backend) {
        let sync = backend.create_loop().unwrap();
        let follower = backend.create_loop().unwrap();
        backend.wait_idle();
        backend
            .transition_loop(follower, BackendLoopMode::Recording, None)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.loops.contains_key(&sync));
        assert_eq!(
            snapshot.loops.get(&follower).unwrap().mode,
            BackendLoopMode::Recording
        );
        backend.set_loop_sync_source(follower, Some(sync)).unwrap();
        backend.wait_idle();
    }

    fn session_io_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "persistence".to_owned(),
                audio_channels: 3,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::OutputGainDb(-4.0))
            .unwrap();
        backend.set_loop_gain(created.loops[0], 0.75).unwrap();
        backend.set_loop_balance(created.loops[0], -0.25).unwrap();
        let input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap();
        backend
            .set_port_connected(input.id, "system:capture_1", true)
            .unwrap();
        let mut prepared = backend.capture_session().unwrap();
        let track = prepared
            .tracks
            .iter_mut()
            .find(|track| track.source_id == created.track_id.raw())
            .unwrap();
        let loop_ = track
            .loops
            .iter_mut()
            .find(|loop_| loop_.source_id == created.loops[0].raw())
            .unwrap();
        loop_.length = 4;
        loop_.audio[0].samples = vec![0.25, -0.5, 0.75, -1.0];
        loop_.audio[0].gain = 0.5;
        loop_.audio[0].start_offset = -2;
        loop_.audio[0].preplay = 3;
        loop_.midi[0] = BackendMidiContent {
            mode: BackendChannelMode::Direct,
            length: 4,
            start_state: vec![vec![0xB0, 7, 99]],
            events: vec![BackendMidiEvent {
                time: 2,
                data: vec![0x90, 60, 100],
            }],
            start_offset: -1,
            preplay: 2,
        };
        backend.advance(Duration::from_millis(20));
        let status_before_replace = backend.poll().unwrap().status;
        let mapping = backend.replace_session(&prepared).unwrap();
        let status_after_replace = backend.poll().unwrap().status;
        assert_eq!(
            status_after_replace.callback_count,
            status_before_replace.callback_count
        );
        assert_eq!(
            status_after_replace.processed_frames,
            status_before_replace.processed_frames
        );
        assert_eq!(mapping.tracks.len(), prepared.tracks.len());
        assert_eq!(mapping.loops.len(), 2);
        let captured = backend.capture_session().unwrap();
        let track = captured
            .tracks
            .iter()
            .find(|track| track.source_id == created.track_id.raw())
            .unwrap();
        assert_eq!(track.state.output_gain_db, -4.0);
        let loop_ = &track.loops[0];
        assert_eq!(loop_.length, 4);
        assert_eq!(loop_.gain, 0.75);
        assert_eq!(loop_.balance, -0.25);
        assert_eq!(loop_.audio[0].samples, vec![0.25, -0.5, 0.75, -1.0]);
        assert_eq!(loop_.audio[0].start_offset, -2);
        assert_eq!(loop_.audio[0].preplay, 3);
        assert_eq!(loop_.midi[0].events[0].time, 2);
        assert!(loop_.midi[0]
            .start_state
            .iter()
            .any(|message| message == &[0xB0, 7, 99]));
        assert!(track
            .ports
            .iter()
            .any(|port| port.external_connections == ["system:capture_1"]));

        let before_failure = backend.capture_session().unwrap();
        let mut invalid = before_failure.clone();
        invalid.tracks[0].loops[0].audio.pop();
        assert!(backend.replace_session(&invalid).is_err());
        assert_eq!(backend.capture_session().unwrap(), before_failure);
    }

    fn connection_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "connections".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        assert_eq!(created.ports.len(), 4);
        let audio_input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap();
        assert_eq!(audio_input.direction, BackendPortDirection::Input);
        assert_eq!(audio_input.data_type, BackendPortDataType::Audio);
        let snapshot = backend.poll().unwrap().connections;
        assert!(snapshot.available);
        assert_eq!(
            snapshot.application_ports.get(&audio_input.id),
            Some(audio_input)
        );
        assert!(snapshot.host_ports.contains_key("system:capture_1"));
        assert!(snapshot.host_ports.contains_key("system:playback_1"));
        assert!(snapshot.host_ports.contains_key("controller:midi_out"));
        assert!(!snapshot
            .host_ports
            .keys()
            .any(|id| id.starts_with("shoop:")));

        backend
            .set_port_connected(audio_input.id, "system:capture_1", true)
            .unwrap();
        backend
            .set_port_connected(audio_input.id, "system:capture_1", true)
            .unwrap();
        let snapshot = backend.poll().unwrap().connections;
        assert!(snapshot.confirmed_links.contains(&BackendConfirmedLink {
            application_port_id: audio_input.id,
            host_port_id: "system:capture_1".to_owned(),
        }));
        backend
            .set_port_connected(audio_input.id, "system:capture_1", false)
            .unwrap();
        assert!(backend
            .set_port_connected(audio_input.id, "missing:endpoint", true)
            .is_err());
    }

    fn direct_track_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "contract".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::OutputGainDb(-6.0))
            .unwrap();
        backend.set_loop_gain(created.loops[0], 0.5).unwrap();
        backend.set_loop_balance(created.loops[0], 0.25).unwrap();
        backend
            .inject_midi_input(
                created.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, 60, 100],
                }],
            )
            .unwrap();
        let third = backend.add_loop_to_track(created.track_id).unwrap();
        backend.wait_idle();
        let snapshot = backend.poll().unwrap();
        let track = &snapshot.tracks[&created.track_id];
        assert_eq!(track.audio_channels, 2);
        assert!(track.midi);
        assert_eq!(track.output_gain_db, -6.0);
        assert!(snapshot.loops[&created.loops[0]].stereo);
        assert_eq!(snapshot.loops[&created.loops[0]].gain, 0.5);
        assert_eq!(snapshot.loops[&created.loops[0]].balance, 0.25);
        assert!(snapshot.loops.contains_key(&third));
        assert_eq!(
            backend
                .loop_audio_data(created.loops[0])
                .unwrap()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn fake_backend_satisfies_contracts() {
        let mut backend = FakeBackend::default();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
        connection_contract(&mut backend);
    }

    #[test]
    fn engine_dummy_backend_satisfies_contracts() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
        connection_contract(&mut backend);
    }

    #[test]
    fn fake_and_engine_backends_satisfy_transactional_session_io_contract() {
        session_io_contract(&mut FakeBackend::default());
        session_io_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[test]
    fn driver_catalog_and_switch_contracts_are_typed_and_transactional() {
        let mut backend = FakeBackend::default();
        let catalog = backend.audio_driver_state().unwrap();
        assert!(catalog.supported);
        assert_eq!(catalog.catalog.len(), 3);
        assert!(catalog
            .catalog
            .iter()
            .all(|driver| driver.kind != AudioDriverKind::WebAudio));

        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "switch".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let before = backend.capture_session().unwrap();
        let target = AudioDriverConfig::Cpal(shoop_app_api::CpalAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
            ..Default::default()
        });
        let resolved = backend.preflight_audio_driver(&target).unwrap();
        assert_eq!(resolved.sample_rate, 48_000);
        backend.fail_next_driver_switch("injected switch failure");
        assert!(backend
            .switch_audio_driver(&target, resolved.sample_rate, &before)
            .is_err());
        assert_eq!(backend.capture_session().unwrap(), before);
        assert_eq!(
            backend
                .audio_driver_state()
                .unwrap()
                .active
                .unwrap()
                .configured
                .kind(),
            AudioDriverKind::Dummy
        );

        backend
            .switch_audio_driver(&target, resolved.sample_rate, &before)
            .unwrap();
        let active = backend.audio_driver_state().unwrap().active.unwrap();
        assert_eq!(active.configured, target);
        assert_eq!(active.buffer_size, 128);
    }

    #[test]
    fn engine_dummy_preflight_rejects_unconfirmed_rate_changes() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        let target = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 44_100,
            buffer_size: 128,
        });
        assert_eq!(
            backend.preflight_audio_driver(&target).unwrap().sample_rate,
            44_100
        );
        let before = backend.capture_session().unwrap();
        assert!(backend
            .switch_audio_driver(&target, 48_000, &before)
            .is_err());
        assert_eq!(backend.poll().unwrap().status.sample_rate, 48_000);
    }

    #[test]
    fn fake_connection_control_covers_churn_external_change_and_deferred_failure() {
        let mut backend = FakeBackend::default();
        let control = backend.connection_control();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "fake".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 0,
            })
            .unwrap();
        let input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap()
            .id;
        control.add_external_port(
            "device:new_output",
            BackendPortDirection::Output,
            BackendPortDataType::Audio,
        );
        control.externally_set_connected(input, "device:new_output", true);
        let snapshot = backend.poll().unwrap().connections;
        assert!(snapshot.host_ports.contains_key("device:new_output"));
        assert!(snapshot.confirmed_links.contains(&BackendConfirmedLink {
            application_port_id: input,
            host_port_id: "device:new_output".to_owned(),
        }));
        control.remove_external_port("device:new_output");
        let snapshot = backend.poll().unwrap().connections;
        assert!(!snapshot.host_ports.contains_key("device:new_output"));
        assert!(!snapshot.confirmed_links.iter().any(|link| {
            link.application_port_id == input && link.host_port_id == "device:new_output"
        }));

        control.defer_mutations(true);
        backend
            .set_port_connected(input, "system:capture_1", true)
            .unwrap();
        assert_eq!(control.pending_len(), 1);
        control.complete_pending(false);
        let failures = backend.poll().unwrap().connections.failures;
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].port_id, input);
    }

    #[test]
    fn cooperative_dummy_records_and_plays_real_engine_frames() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "cooperative".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = track.loops[0];
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
        backend.advance_frames(512);
        let recording = backend.poll().unwrap().loops[&loop_id].clone();
        assert_eq!(recording.mode, BackendLoopMode::Recording);
        assert_eq!(recording.length, 512);

        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        backend.advance_frames(256);
        let playing = backend.poll().unwrap().loops[&loop_id].clone();
        assert_eq!(playing.mode, BackendLoopMode::Playing);
        assert_eq!(playing.position, 256);
        assert_eq!(
            backend.loop_audio_data(loop_id).unwrap().unwrap()[0].len(),
            512
        );
    }

    #[test]
    fn empty_web_audio_host_inventory_preserves_application_ports() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "offline_device".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert!(!snapshot.audio_drivers.supported);
        assert_eq!(
            snapshot
                .audio_drivers
                .active
                .as_ref()
                .map(|active| active.configured.kind()),
            Some(AudioDriverKind::WebAudio)
        );
        let connections = snapshot.connections;
        assert!(connections.available);
        assert!(connections.host_ports.is_empty());
        assert!(connections.confirmed_links.is_empty());
        assert_eq!(connections.application_ports.len(), created.ports.len());
    }

    #[test]
    fn track_midi_injection_records_without_host_endpoints_or_links() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        let midi_track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "piano".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let audio_track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "audio_only".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        assert!(backend.poll().unwrap().connections.host_ports.is_empty());
        backend
            .transition_loop(midi_track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        backend
            .inject_midi_input(
                midi_track.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, 60, 100],
                }],
            )
            .unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut [], 0, 128)
            .unwrap();
        backend
            .inject_midi_input(
                midi_track.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x80, 60, 0],
                }],
            )
            .unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut [], 0, 128)
            .unwrap();
        backend
            .transition_loop(midi_track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let events = &backend.capture_session().unwrap().tracks[0].loops[0].midi[0].events;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, [0x90, 60, 100]);
        assert_eq!(events[1].data, [0x80, 60, 0]);
        assert!(events[1].time >= events[0].time);
        assert!(backend
            .inject_midi_input(
                audio_track.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, 61, 100],
                }],
            )
            .is_err());
        assert!(backend
            .inject_midi_input(
                midi_track.track_id,
                &[BackendMidiEvent {
                    time: 1,
                    data: vec![0x90, 61, 100],
                }],
            )
            .is_err());
    }

    #[test]
    fn web_midi_routes_record_monitor_and_playback_with_bounded_render_work() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        let endpoints = vec![
            BackendHostPortDescriptor {
                id: "webmidi:source:controller".to_owned(),
                name: "Controller input".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Output,
            },
            BackendHostPortDescriptor {
                id: "webmidi:sink:controller".to_owned(),
                name: "Controller output".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Input,
            },
        ];
        backend
            .configure_web_midi_endpoints(endpoints.clone())
            .unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "web_midi".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let input = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        let output = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiOutput)
            .unwrap();
        backend
            .set_port_connected(input.id, "webmidi:source:controller", true)
            .unwrap();
        backend
            .set_port_connected(output.id, "webmidi:sink:controller", true)
            .unwrap();
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        assert_eq!(
            backend
                .stage_web_midi_input("webmidi:source:controller", &[0x90, 60, 100])
                .unwrap(),
            1
        );
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&[], 0, &mut [], 0, 128)
                .unwrap();
        });
        let (monitored, dropped, refused) = backend.drain_web_midi_output(16);
        assert_eq!((dropped, refused), (0, 0));
        assert!(monitored.iter().any(|event| event.data == [0x90, 60, 100]));
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let session = backend.capture_session().unwrap();
        assert!(session.tracks[0].loops[0].midi[0]
            .events
            .iter()
            .any(|event| event.data == [0x90, 60, 100]));

        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(false))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&[], 0, &mut [], 0, 128)
                .unwrap();
        });
        let (played, dropped, refused) = backend.drain_web_midi_output(16);
        assert_eq!((dropped, refused), (0, 0));
        assert!(played.iter().any(|event| {
            event.host_port_id == "webmidi:sink:controller" && event.data == [0x90, 60, 100]
        }));

        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .set_port_connected(output.id, "webmidi:sink:controller", false)
            .unwrap();
        backend
            .stage_web_midi_input("webmidi:source:controller", &[0x90, 61, 100])
            .unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut [], 0, 128)
            .unwrap();
        assert!(backend.drain_web_midi_output(16).0.is_empty());
        backend
            .set_port_connected(output.id, "webmidi:sink:controller", true)
            .unwrap();
        backend
            .stage_web_midi_input("webmidi:source:controller", &[0x90, 62, 100])
            .unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut [], 0, 128)
            .unwrap();
        let reconnected_output = backend.drain_web_midi_output(16).0;
        assert_eq!(
            reconnected_output
                .iter()
                .filter(|event| event.data == [0x90, 62, 100])
                .count(),
            1
        );

        backend.configure_web_midi_endpoints(Vec::new()).unwrap();
        assert_eq!(
            backend
                .stage_web_midi_input("webmidi:source:controller", &[0xf8])
                .unwrap(),
            0
        );
        let disconnected = backend.poll().unwrap().connections;
        assert!(disconnected
            .host_ports
            .values()
            .all(|host| host.data_type != BackendPortDataType::Midi));
        assert!(disconnected
            .confirmed_links
            .iter()
            .all(|link| !link.host_port_id.starts_with("webmidi:")));
        let saved_while_missing = backend.capture_session().unwrap();
        assert_eq!(
            saved_while_missing.tracks[0]
                .ports
                .iter()
                .flat_map(|port| &port.external_connections)
                .filter(|endpoint| endpoint.starts_with("webmidi:"))
                .count(),
            2
        );
        backend.replace_session(&saved_while_missing).unwrap();
        backend.configure_web_midi_endpoints(endpoints).unwrap();
        let reconnected = backend.poll().unwrap().connections;
        assert_eq!(
            reconnected
                .confirmed_links
                .iter()
                .filter(|link| link.host_port_id.starts_with("webmidi:"))
                .count(),
            2
        );
    }

    #[test]
    fn web_midi_input_fans_out_once_to_every_connected_track() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        backend
            .configure_web_midi_endpoints(vec![BackendHostPortDescriptor {
                id: "webmidi:source:shared".to_owned(),
                name: "Shared input".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Output,
            }])
            .unwrap();
        let mut tracks = Vec::new();
        for name in ["first", "second"] {
            let track = backend
                .create_direct_track(DirectTrackRequest {
                    port_name_base: name.to_owned(),
                    audio_channels: 0,
                    midi: true,
                    initial_loops: 1,
                })
                .unwrap();
            let input = track
                .ports
                .iter()
                .find(|port| port.role == BackendPortRole::MidiInput)
                .unwrap();
            backend
                .set_port_connected(input.id, "webmidi:source:shared", true)
                .unwrap();
            backend
                .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
                .unwrap();
            tracks.push(track);
        }
        assert_eq!(
            backend
                .stage_web_midi_input("webmidi:source:shared", &[0x90, 65, 100])
                .unwrap(),
            2
        );
        backend
            .process_audio_quantum(&[], 0, &mut [], 0, 128)
            .unwrap();
        for track in &tracks {
            backend
                .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
                .unwrap();
        }
        let session = backend.capture_session().unwrap();
        assert_eq!(session.tracks.len(), tracks.len());
        for track in &session.tracks {
            assert_eq!(
                track.loops[0].midi[0]
                    .events
                    .iter()
                    .filter(|event| event.data == [0x90, 65, 100])
                    .count(),
                1
            );
        }
    }

    #[test]
    fn web_midi_output_fanout_survives_bounded_drains_without_duplicates() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        backend
            .configure_web_midi_endpoints(vec![
                BackendHostPortDescriptor {
                    id: "webmidi:source:controller".to_owned(),
                    name: "Controller input".to_owned(),
                    data_type: BackendPortDataType::Midi,
                    direction: BackendPortDirection::Output,
                },
                BackendHostPortDescriptor {
                    id: "webmidi:sink:first".to_owned(),
                    name: "First output".to_owned(),
                    data_type: BackendPortDataType::Midi,
                    direction: BackendPortDirection::Input,
                },
                BackendHostPortDescriptor {
                    id: "webmidi:sink:second".to_owned(),
                    name: "Second output".to_owned(),
                    data_type: BackendPortDataType::Midi,
                    direction: BackendPortDirection::Input,
                },
            ])
            .unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "fanout".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let input = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        let output = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiOutput)
            .unwrap();
        backend
            .set_port_connected(input.id, "webmidi:source:controller", true)
            .unwrap();
        for endpoint in ["webmidi:sink:first", "webmidi:sink:second"] {
            backend
                .set_port_connected(output.id, endpoint, true)
                .unwrap();
        }
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .stage_web_midi_input("webmidi:source:controller", &[0xf8])
            .unwrap();
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&[], 0, &mut [], 0, 128)
                .unwrap();
        });
        let mut events = backend.drain_web_midi_output(1).0;
        events.extend(backend.drain_web_midi_output(1).0);
        events.extend(backend.drain_web_midi_output(1).0);
        assert_eq!(events.len(), 2);
        assert_eq!(
            events
                .iter()
                .map(|event| event.host_port_id.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["webmidi:sink:first", "webmidi:sink:second"])
        );
    }

    #[test]
    fn saturated_web_midi_render_is_allocation_free_and_counts_refusal() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        backend
            .configure_web_midi_endpoints(vec![BackendHostPortDescriptor {
                id: "webmidi:source:dense".to_owned(),
                name: "Dense input".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Output,
            }])
            .unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "dense".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let input = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        backend
            .set_port_connected(input.id, "webmidi:source:dense", true)
            .unwrap();
        for _ in 0..256 {
            assert_eq!(
                backend
                    .stage_web_midi_input("webmidi:source:dense", &[0xf8])
                    .unwrap(),
                1
            );
        }
        assert_eq!(
            backend
                .stage_web_midi_input("webmidi:source:dense", &[0xf8])
                .unwrap(),
            0
        );
        assert_eq!(backend.web_midi_input_refused(), 1);
        assert_eq!(backend.drain_web_midi_output(0).2, 1);
        assert_eq!(backend.web_midi_input_refused(), 0);
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&[], 0, &mut [], 0, 128)
                .unwrap();
        });
    }

    #[test]
    fn tiny_synth_fx_processes_audio_midi_controls_and_session_state() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 2).unwrap();
        backend
            .configure_web_midi_endpoints(vec![BackendHostPortDescriptor {
                id: "webmidi:source:tiny".to_owned(),
                name: "Tiny MIDI".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Output,
            }])
            .unwrap();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "tiny".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        let midi_input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        backend
            .set_port_connected(midi_input.id, "webmidi:source:tiny", true)
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend.poll().unwrap();
        backend
            .stage_web_midi_input("webmidi:source:tiny", &[0x90, 69, 127])
            .unwrap();
        let input = vec![0.1; 128];
        let mut output = vec![0.0; 256];
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&input, 1, &mut output, 2, 128)
                .unwrap();
        });
        assert!(output.iter().any(|sample| sample.abs() > 0.01));
        assert!(output[..128]
            .iter()
            .zip(&output[128..])
            .all(|(left, right)| (*left - *right).abs() < 1.0e-6));

        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetActive(false))
            .unwrap();
        output.fill(1.0);
        backend
            .process_audio_quantum(&input, 1, &mut output, 2, 128)
            .unwrap();
        assert!(output.iter().all(|sample| sample.abs() < 1.0e-7));
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetActive(true))
            .unwrap();
        backend
            .process_audio_quantum(&input, 1, &mut output, 2, 128)
            .unwrap();
        assert!(output.iter().any(|sample| sample.abs() > 0.01));

        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SelectPreset(
                    "pad".to_owned(),
                )),
            )
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetReverbEnabled(true)),
            )
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetReverbAmount(0.4)),
            )
            .unwrap();
        let state = backend
            .track_fx_state_string(created.track_id)
            .unwrap()
            .unwrap();
        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].processor_state.as_deref(),
            Some(state.as_str())
        );
        assert!(backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::RestoreState("malformed".to_owned()),
            )
            .is_err());
        assert_eq!(
            backend.track_fx_state_string(created.track_id).unwrap(),
            Some(state.clone())
        );
        let mut malformed_session = captured.clone();
        malformed_session.tracks[0].processor_state = Some("malformed".to_owned());
        assert!(backend.replace_session(&malformed_session).is_err());
        assert_eq!(backend.capture_session().unwrap(), captured);

        let source_track = captured.tracks[0].source_id;
        let replacement = backend.replace_session(&captured).unwrap();
        let restored_track = replacement.tracks[&source_track].track_id;
        let snapshot = backend.poll().unwrap();
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) = snapshot.tracks[&restored_track]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing Tiny Synth/FX editor state");
        };
        assert_eq!(editor.selected_preset_id.as_deref(), Some("pad"));
        assert!(editor.reverb_enabled);
        assert_eq!(editor.reverb_amount, 0.4);
    }

    #[test]
    fn tiny_synth_fx_accepts_zero_mono_stereo_and_arbitrary_matched_channels() {
        for channels in [0, 1, 2, 7] {
            let mut backend = EngineBackend::new_dummy(48_000, 128).unwrap();
            let created = backend
                .create_track(TrackRequest {
                    port_name_base: format!("tiny_{channels}"),
                    topology: BackendTrackTopology::DryWetProcessor {
                        processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                        dry_audio_channels: channels,
                        wet_audio_channels: channels,
                        dry_midi: true,
                    },
                    initial_loops: 1,
                })
                .unwrap();
            let expected_roles = (0..channels)
                .flat_map(|_| [BackendPortRole::AudioInput, BackendPortRole::AudioOutput])
                .chain(std::iter::once(BackendPortRole::MidiInput))
                .collect::<Vec<_>>();
            assert_eq!(
                created
                    .ports
                    .iter()
                    .map(|port| port.role)
                    .collect::<Vec<_>>(),
                expected_roles
            );
            let captured = backend.capture_session().unwrap();
            assert_eq!(
                captured.tracks[0]
                    .ports
                    .iter()
                    .map(|port| port.descriptor.role)
                    .collect::<Vec<_>>(),
                expected_roles
            );
            let source_track = captured.tracks[0].source_id;
            let replacement = backend.replace_session(&captured).unwrap();
            assert!(replacement.tracks.contains_key(&source_track));
            assert_eq!(
                backend.capture_session().unwrap().tracks[0].topology,
                captured.tracks[0].topology
            );
        }
        let mut backend = EngineBackend::new_dummy(48_000, 128).unwrap();
        assert!(backend
            .create_track(TrackRequest {
                port_name_base: "bad_tiny".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .is_err());
        assert!(backend
            .create_track(TrackRequest {
                port_name_base: "missing_midi_tiny".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .is_err());
        assert!(backend.capture_session().unwrap().tracks.is_empty());
    }

    #[test]
    fn disconnected_web_audio_input_records_silence() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 1).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "disconnected_input".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let input_port = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap();
        backend
            .set_port_connected(input_port.id, "webaudio:capture_1", false)
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let input = vec![0.75; 128];
        let mut output = vec![0.0; 128];
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&input, 1, &mut output, 1, 128)
                .unwrap();
        });
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let recorded = backend.loop_audio_data(created.loops[0]).unwrap().unwrap();
        assert_eq!(recorded[0].len(), 128);
        assert!(recorded[0].iter().all(|sample| *sample == 0.0));
    }

    #[test]
    fn web_audio_backend_records_monitors_and_plays_non_zero_full_duplex_audio() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 2).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "web".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let input = vec![0.25; 128];
        let mut output = vec![0.0; 256];
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&input, 1, &mut output, 2, 128)
                .unwrap();
        });
        assert!(output[..128].iter().all(|sample| *sample == 0.25));
        assert!(output[128..].iter().all(|sample| *sample == 0.25));
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let recorded = backend.loop_audio_data(track.loops[0]).unwrap().unwrap();
        assert_eq!(recorded[0].len(), 128);
        assert!(recorded[0].iter().all(|sample| *sample == 0.25));
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(false))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        output.fill(0.0);
        backend
            .process_audio_quantum(&vec![0.0; 128], 1, &mut output, 2, 128)
            .unwrap();
        assert!(output.iter().any(|sample| *sample != 0.0));
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.connections.available);
        assert_eq!(snapshot.connections.application_ports.len(), 2);
        let status = snapshot.status;
        assert_eq!(status.callback_count, 2);
        assert_eq!(status.processed_frames, 256);
        assert!(status.input_peak == 0.0);
        assert!(status.output_peak > 0.0);
    }

    #[test]
    fn web_audio_session_replacement_preserves_user_route_changes_over_defaults() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 2).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "route_session".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let output = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioOutput)
            .unwrap();
        backend
            .set_port_connected(output.id, "webaudio:destination_1", false)
            .unwrap();
        let captured = backend.capture_session().unwrap();
        let replacement = backend.replace_session(&captured).unwrap();
        let replaced_output = replacement.ports[&output.id.raw()];
        let links = backend.poll().unwrap().connections.confirmed_links;
        assert!(!links.contains(&BackendConfirmedLink {
            application_port_id: replaced_output,
            host_port_id: "webaudio:destination_1".to_owned(),
        }));
        assert!(links.contains(&BackendConfirmedLink {
            application_port_id: replaced_output,
            host_port_id: "webaudio:destination_2".to_owned(),
        }));

        let mut legacy = captured;
        legacy.use_legacy_browser_default_routes = true;
        for track in &mut legacy.tracks {
            for port in &mut track.ports {
                port.external_connections.clear();
            }
        }
        let migrated = backend.replace_session(&legacy).unwrap();
        let migrated_output = migrated.ports[&output.id.raw()];
        let links = backend.poll().unwrap().connections.confirmed_links;
        assert!(WEB_AUDIO_DESTINATION_PORTS.iter().all(|host| {
            links.contains(&BackendConfirmedLink {
                application_port_id: migrated_output,
                host_port_id: (*host).to_owned(),
            })
        }));
    }

    #[test]
    fn web_audio_playback_deterministically_mixes_more_loop_channels_than_device_channels() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 2).unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "wide_web".to_owned(),
                audio_channels: 4,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let mut session = backend.capture_session().unwrap();
        let loop_ = &mut session.tracks[0].loops[0];
        loop_.length = 128;
        for (channel, value) in loop_.audio.iter_mut().zip([0.1, 0.2, 0.3, 0.4]) {
            channel.samples = vec![value; 128];
        }
        let source_loop_id = loop_.source_id;
        let replacement = backend.replace_session(&session).unwrap();
        let loaded_loop_id = replacement.loops[&source_loop_id];
        backend
            .transition_loop(loaded_loop_id, BackendLoopMode::Playing, None)
            .unwrap();

        let mut output = vec![0.0; 256];
        backend
            .process_audio_quantum(&[], 0, &mut output, 2, 128)
            .unwrap();
        assert!(output[..128]
            .iter()
            .all(|sample| (*sample - 0.1).abs() < 1.0e-6));
        assert!(output[128..]
            .iter()
            .all(|sample| (*sample - 0.9).abs() < 1.0e-6));
    }

    #[test]
    fn web_audio_and_midi_grab_adopt_recent_input_without_growing_in_the_callback() {
        let mut backend = EngineBackend::new_web_audio(128, 128).unwrap();
        backend.configure_web_audio_channels(1, 2).unwrap();
        backend
            .configure_web_midi_endpoints(vec![BackendHostPortDescriptor {
                id: "webmidi:source:grab".to_owned(),
                name: "Grab MIDI input".to_owned(),
                data_type: BackendPortDataType::Midi,
                direction: BackendPortDirection::Output,
            }])
            .unwrap();
        let sync = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "grab_sync".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "grab_target".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let midi_input = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        backend
            .set_port_connected(midi_input.id, "webmidi:source:grab", true)
            .unwrap();
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        let mut output = vec![0.0; 256];
        for _ in 0..8 {
            backend
                .process_audio_quantum(&vec![0.0; 128], 1, &mut output, 2, 128)
                .unwrap();
        }
        backend
            .transition_loop(sync.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        backend
            .process_audio_quantum(&vec![0.25; 128], 1, &mut output, 2, 128)
            .unwrap();
        backend
            .transition_loop(sync.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .set_loop_sync_source(track.loops[0], Some(sync.loops[0]))
            .unwrap();
        backend
            .stage_web_midi_input("webmidi:source:grab", &[0x90, 64, 100])
            .unwrap();
        backend
            .process_audio_quantum(&vec![0.5; 128], 1, &mut output, 2, 128)
            .unwrap();
        let midi_session_port = backend.tracks[&track.track_id].midi_input.unwrap();
        let mut midi_ringbuffer = MidiStorage::with_capacity_elems(1024);
        backend
            .session
            .port(midi_session_port)
            .and_then(Port::midi)
            .unwrap()
            .snapshot_ringbuffer_into(&mut midi_ringbuffer);
        assert!(midi_ringbuffer
            .iter()
            .any(|event| event.data() == [0x90, 64, 100]));
        backend
            .grab_loops(&[BackendGrabRequest {
                loop_id: track.loops[0],
                reverse_start_cycle: Some(1),
                cycles_length: Some(1),
                go_to_cycle: Some(0),
                go_to_mode: BackendLoopMode::Playing,
            }])
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.loops[&track.loops[0]].length, 128);
        assert_eq!(
            snapshot.loops[&track.loops[0]].mode,
            BackendLoopMode::Playing
        );
        let grabbed = backend.loop_audio_data(track.loops[0]).unwrap().unwrap();
        assert_eq!(grabbed[0].len(), 128);
        assert!(grabbed[0].iter().any(|sample| *sample != 0.0));
        let session = backend.capture_session().unwrap();
        assert!(
            session
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .flat_map(|loop_| &loop_.midi)
                .flat_map(|channel| &channel.events)
                .any(|event| event.data == [0x90, 64, 100]),
            "ring times: {:?}",
            midi_ringbuffer
                .iter()
                .map(|event| event.time)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn fake_grab_preflights_every_target() {
        let mut backend = FakeBackend::default();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "grab".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let operations = backend.operations().len();
        assert!(backend
            .grab_loops(&[
                BackendGrabRequest {
                    loop_id: track.loops[0],
                    reverse_start_cycle: Some(1),
                    cycles_length: Some(1),
                    go_to_cycle: Some(0),
                    go_to_mode: BackendLoopMode::Playing,
                },
                BackendGrabRequest {
                    loop_id: BackendLoopId::from_raw(999),
                    reverse_start_cycle: Some(1),
                    cycles_length: Some(1),
                    go_to_cycle: Some(0),
                    go_to_mode: BackendLoopMode::Playing,
                },
            ])
            .is_err());
        assert_eq!(backend.operations().len(), operations);
        assert_eq!(
            backend.poll().unwrap().loops[&track.loops[0]].mode,
            BackendLoopMode::Stopped
        );
    }

    #[test]
    fn elapsed_time_preserves_fractional_frame_remainders() {
        let mut backend = EngineBackend::new_dummy(1_000, 64).unwrap();
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 0);
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 1);
    }

    #[test]
    fn elapsed_time_processing_is_bounded_and_reports_dropped_time() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend.advance(Duration::from_secs(10));
        assert_eq!(
            backend.processed_frames(),
            u64::from(256 * MAX_CYCLES_PER_ADVANCE)
        );
        assert_eq!(backend.poll().unwrap().status.xruns, 1);

        backend.advance(Duration::from_millis(1));
        assert_eq!(
            backend.processed_frames(),
            u64::from(256 * MAX_CYCLES_PER_ADVANCE + 48)
        );
    }
}
