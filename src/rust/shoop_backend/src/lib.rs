#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(all(feature = "native-drivers", not(target_arch = "wasm32")))]
mod native;
#[cfg(all(feature = "native-drivers", not(target_arch = "wasm32")))]
pub use native::NativeBackend;
#[cfg(all(feature = "native-fx", not(target_arch = "wasm32")))]
pub use native::{
    carla_runtime_path, configure_carla_hosting_mode, configured_carla_hosting_mode,
    run_carla_worker_if_requested, smoke_test_carla_runtime, smoke_test_carla_ui,
};
pub use shoop_app_api::{
    OxiSynthControl, OxiSynthMidiCcAssignment, OxiSynthParameter, OxiSynthState,
    TrackProcessorEditorState, TrackProcessorTypeId,
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
use shoop_engine::midi_state::{MidiStateTracker, TrackWhat};
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
pub struct BackendCompositeId(u64);

impl BackendCompositeId {
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
pub struct BackendBusId(u64);

impl BackendBusId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BackendBusChannelId(u64);

impl BackendBusChannelId {
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
pub enum BackendPortOwner {
    Track,
    Bus(BackendBusId),
    GlobalFxControl,
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
    pub owner: BackendPortOwner,
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
pub struct BackendBusChannelState {
    pub id: BackendBusChannelId,
    pub label: String,
    pub output_port_id: BackendPortId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendBusState {
    pub id: BackendBusId,
    pub name: String,
    pub channels: Vec<BackendBusChannelState>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct BackendMixerLink {
    pub source_port_id: BackendPortId,
    pub destination_channel_id: BackendBusChannelId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendMixerFailure {
    pub link: BackendMixerLink,
    pub desired_connected: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendMixerSnapshot {
    pub revision: u64,
    pub buses: BTreeMap<BackendBusId, BackendBusState>,
    pub confirmed_links: BTreeSet<BackendMixerLink>,
    pub failures: Vec<BackendMixerFailure>,
}

pub const MASTER_BUS_NAME: &str = "Master";
pub const MASTER_BUS_CHANNEL_LABELS: [&str; 2] = ["Left", "Right"];

fn validate_mixer_source_descriptor(descriptor: &BackendPortDescriptor) -> Result<()> {
    if descriptor.owner != BackendPortOwner::Track
        || descriptor.data_type != BackendPortDataType::Audio
        || descriptor.direction != BackendPortDirection::Output
        || descriptor.role != BackendPortRole::AudioOutput
    {
        Err(anyhow!("mixer source is not a track audio output"))
    } else {
        Ok(())
    }
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

    pub const fn has_wet_channels(&self) -> bool {
        match self {
            Self::Direct { .. } => false,
            Self::DryWetExternal {
                wet_audio_channels, ..
            }
            | Self::DryWetProcessor {
                wet_audio_channels, ..
            } => *wet_audio_channels > 0,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendRecordingOffsetAdjustment {
    #[default]
    Automatic,
    ManualOverride(i32),
    AutomaticPlusTrim(i32),
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendProcessorLatencyAdjustment {
    Automatic,
    #[default]
    ManualOverride,
    AutomaticPlusTrim,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendTrackLatencyState {
    pub automatic_offset_frames: Option<i32>,
    pub adjustment: BackendRecordingOffsetAdjustment,
    pub effective_offset_frames: Option<i32>,
    pub automatic_processor_advance_frames: Option<u32>,
    pub processor_adjustment: BackendProcessorLatencyAdjustment,
    pub processor_manual_frames: i32,
    pub effective_processor_advance_frames: Option<u32>,
    #[serde(skip)]
    pub pending: bool,
    #[serde(skip)]
    pub error: Option<String>,
}

impl Default for BackendTrackLatencyState {
    fn default() -> Self {
        Self {
            automatic_offset_frames: None,
            adjustment: BackendRecordingOffsetAdjustment::ManualOverride(0),
            effective_offset_frames: Some(0),
            automatic_processor_advance_frames: Some(0),
            processor_adjustment: BackendProcessorLatencyAdjustment::ManualOverride,
            processor_manual_frames: 0,
            effective_processor_advance_frames: Some(0),
            pending: false,
            error: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendTrackFxControl {
    SetActive(bool),
    SetVisible(bool),
    ToggleOrRecover,
    RestoreState(String),
    ClearLogs,
    OxiSynth(OxiSynthControl),
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
    #[serde(default)]
    pub latency: BackendTrackLatencyState,
    pub input_peaks: Vec<f32>,
    pub output_peaks: Vec<f32>,
    pub input_midi_activity: bool,
    pub output_midi_activity: bool,
    pub latest_input_midi_message: Option<BackendLatestMidiMessage>,
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
    pub render_memory_growths: u32,
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
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
    pub capture_alignment_frames: i32,
    pub processor_alignment_frames: Option<u32>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendCompositeKind {
    Regular,
    Script,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BackendCompositeTarget {
    Loop(BackendLoopId),
    Composite(BackendCompositeId),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendCompositeEntry {
    pub target: BackendCompositeTarget,
    pub delay: i64,
    pub n_cycles: Option<i64>,
    pub mode: Option<BackendLoopMode>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendCompositeConfig {
    pub kind: BackendCompositeKind,
    pub sync_source: BackendLoopId,
    pub timelines: Vec<Vec<Vec<BackendCompositeEntry>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendActiveCompositeChild {
    pub target: BackendCompositeTarget,
    pub mode: BackendLoopMode,
    pub cycle_offset: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCompositeState {
    pub mode: BackendLoopMode,
    pub next_mode: Option<BackendLoopMode>,
    pub next_transition_delay: Option<u32>,
    pub iteration: u32,
    pub cycle_count: u64,
    pub length: u64,
    pub position: u64,
    pub active_plan_version: u64,
    pub pending_plan_version: Option<u64>,
    pub active_children: Vec<BackendActiveCompositeChild>,
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
    #[serde(default)]
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendAudioChannelData {
    pub samples: Arc<[f32]>,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendAudioData {
    pub channels: Vec<BackendAudioChannelData>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendLatestMidiMessage {
    pub bytes: [u8; 4],
    pub len: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendOxiSynthMidiCcAssignment {
    pub parameter: BackendOxiSynthParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BackendOxiSynthParameter {
    ReverbSend,
    ChorusSend,
}

impl From<shoop_engine::LatestMidiMessage> for BackendLatestMidiMessage {
    fn from(value: shoop_engine::LatestMidiMessage) -> Self {
        Self {
            bytes: value.bytes,
            len: value.len,
        }
    }
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
    #[serde(default)]
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendMidiChannelData {
    pub content_revision: u64,
    pub mode: BackendChannelMode,
    pub length: u32,
    pub events: Vec<BackendMidiEvent>,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendMidiData {
    pub channels: Vec<BackendMidiChannelData>,
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
pub struct BackendAudioChannelUpdate {
    pub channel: usize,
    pub samples: Vec<f32>,
    pub start_offset: Option<i32>,
    #[serde(default)]
    pub capture_alignment_frames: Option<i32>,
    pub preplay: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendMidiChannelUpdate {
    pub channel: usize,
    pub length: u32,
    pub start_state: Vec<Vec<u8>>,
    pub events: Vec<BackendMidiEvent>,
    pub start_offset: Option<i32>,
    #[serde(default)]
    pub capture_alignment_frames: Option<i32>,
    pub preplay: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendLoopContentUpdate {
    pub audio: Vec<BackendAudioChannelUpdate>,
    pub midi: Vec<BackendMidiChannelUpdate>,
    pub length: Option<u32>,
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
    #[serde(default)]
    pub oxisynth_midi_cc_assignments: Vec<BackendOxiSynthMidiCcAssignment>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionBusChannel {
    pub source_id: u64,
    pub label: String,
    pub output_port: BackendSessionPort,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionBus {
    pub source_id: u64,
    pub name: String,
    pub channels: Vec<BackendSessionBusChannel>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendSessionMixerRoute {
    pub source_port_id: u64,
    pub destination_channel_id: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionData {
    pub sample_rate: u32,
    pub tracks: Vec<BackendSessionTrack>,
    #[serde(default)]
    pub buses: Vec<BackendSessionBus>,
    #[serde(default)]
    pub mixer_routes: Vec<BackendSessionMixerRoute>,
    #[serde(default)]
    pub global_ports: Vec<BackendSessionPort>,
    #[serde(default)]
    pub use_legacy_browser_default_routes: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendSessionReplacement {
    pub tracks: BTreeMap<u64, BackendTrackCreation>,
    pub loops: BTreeMap<u64, BackendLoopId>,
    pub ports: BTreeMap<u64, BackendPortId>,
    pub buses: BTreeMap<u64, BackendBusId>,
    pub bus_channels: BTreeMap<u64, BackendBusChannelId>,
    pub bus_output_ports: BTreeMap<u64, BackendPortId>,
    pub global_ports: BTreeMap<u64, BackendPortId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendAudioDataChunk {
    pub content_revision: u64,
    pub channel: usize,
    pub channel_count: usize,
    pub offset: usize,
    pub total_samples: usize,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub preplay: u32,
    pub samples: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum BackendOperationKind {
    SessionCapture,
    SessionReplacement,
    LoopContentReplacement,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendOperationProgress {
    pub key: u64,
    pub kind: BackendOperationKind,
    pub completed: usize,
    pub total: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendAsyncResult<T> {
    Pending(BackendOperationProgress),
    Ready(T),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendMutationKind {
    DriverConfiguration,
    AudioProcessing,
    TrackStructure,
    CompositeStructure,
    TrackControl,
    TrackFxControl,
    MidiInput,
    LoopControl,
    LoopContent,
    SessionTransfer,
    Connection,
    MixerRoute,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendMutationDetail {
    TrackCreation,
    TrackRemoval,
    LoopCreation { loop_id: BackendLoopId },
    TrackControl(BackendTrackControl),
    TrackFxControl(BackendTrackFxControl),
    LoopGain(f32),
    LoopBalance(f32),
    LoopTiming,
    TakeAlignment,
    TakeProcessorAlignment,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackendMutationFailure {
    pub driver_generation: u64,
    pub sequence: u64,
    pub operation_key: Option<u64>,
    pub kind: BackendMutationKind,
    pub entity: Option<u64>,
    pub detail: Option<BackendMutationDetail>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendSnapshot {
    pub status: BackendStatus,
    pub audio_drivers: AudioDriverRuntimeState,
    pub tracks: BTreeMap<BackendTrackId, BackendTrackState>,
    pub loops: BTreeMap<BackendLoopId, BackendLoopState>,
    pub composites: BTreeMap<BackendCompositeId, BackendCompositeState>,
    pub connections: BackendConnectionSnapshot,
    pub mixer: BackendMixerSnapshot,
    pub mutation_failures: Vec<BackendMutationFailure>,
}

pub trait Backend {
    fn supports_composite_loops(&self) -> bool {
        false
    }

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
    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        Err(anyhow!("composite loops are unavailable"))
    }
    fn configure_composite_loop(
        &mut self,
        _composite_id: BackendCompositeId,
        _config: &BackendCompositeConfig,
    ) -> Result<()> {
        Err(anyhow!("composite loops are unavailable"))
    }
    fn transition_composite_loop(
        &mut self,
        _composite_id: BackendCompositeId,
        _mode: BackendLoopMode,
        _cycles_delay: Option<u32>,
        _align_to_iteration: Option<i64>,
    ) -> Result<()> {
        Err(anyhow!("composite loops are unavailable"))
    }
    fn set_composite_play_after_record(
        &mut self,
        _composite_id: BackendCompositeId,
        _enabled: bool,
    ) -> Result<()> {
        Err(anyhow!("composite loops are unavailable"))
    }
    fn remove_composite_loop(&mut self, _composite_id: BackendCompositeId) -> Result<()> {
        Err(anyhow!("composite loops are unavailable"))
    }
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
    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()>;
    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId>;
    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()>;
    fn set_track_latency(
        &mut self,
        _track_id: BackendTrackId,
        _adjustment: BackendRecordingOffsetAdjustment,
        _processor_adjustment: BackendProcessorLatencyAdjustment,
        _processor_manual_frames: i32,
    ) -> Result<()> {
        Err(anyhow!("latency control is unavailable"))
    }
    fn set_take_alignment(
        &mut self,
        _loop_id: BackendLoopId,
        _capture_alignment_frames: i32,
    ) -> Result<()> {
        Err(anyhow!("take alignment control is unavailable"))
    }
    fn set_take_processor_alignment(
        &mut self,
        _loop_id: BackendLoopId,
        _processor_alignment_frames: u32,
    ) -> Result<()> {
        Err(anyhow!("take processor alignment control is unavailable"))
    }
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
    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        Ok(self
            .loop_audio_data(loop_id)?
            .map(|channels| BackendAudioData {
                channels: channels
                    .into_iter()
                    .map(|samples| BackendAudioChannelData {
                        samples,
                        ..Default::default()
                    })
                    .collect(),
            }))
    }
    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>>;
    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        let channels = self
            .loop_audio_data_with_metadata(loop_id)?
            .unwrap_or_default()
            .channels;
        let channel_data = channels.get(channel);
        let samples = channel_data
            .map(|channel| Arc::clone(&channel.samples))
            .unwrap_or_else(|| Arc::from([]));
        let start_offset = channel_data.map_or(0, |channel| channel.start_offset);
        let capture_alignment_frames =
            channel_data.map_or(0, |channel| channel.capture_alignment_frames);
        let preplay = channel_data.map_or(0, |channel| channel.preplay);
        let end = offset.saturating_add(max_samples).min(samples.len());
        Ok(BackendAudioDataChunk {
            content_revision: 0,
            channel,
            channel_count: channels.len(),
            offset,
            total_samples: samples.len(),
            start_offset,
            capture_alignment_frames,
            preplay,
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
    fn replace_loop_content(
        &mut self,
        _loop_id: BackendLoopId,
        _update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        Err(anyhow!("targeted loop content replacement is unavailable"))
    }
    fn set_loop_length(&mut self, _loop_id: BackendLoopId, _length: u32) -> Result<()> {
        Err(anyhow!("targeted loop length updates are unavailable"))
    }
    fn set_loop_timing(
        &mut self,
        _loop_id: BackendLoopId,
        _start_offset: Option<i32>,
        _preplay: Option<u32>,
        _length: Option<u32>,
    ) -> Result<()> {
        Err(anyhow!("targeted loop timing updates are unavailable"))
    }
    fn capture_session(&mut self) -> Result<BackendSessionData> {
        Err(anyhow!("session capture is unavailable"))
    }
    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        self.capture_session().map(BackendAsyncResult::Ready)
    }
    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let _ = session;
        Err(anyhow!("session replacement is unavailable"))
    }
    /// Starts replacement when no operation is active, then polls that single operation.
    /// Implementations consume `session` only while starting and while applying a successful
    /// completion; callers must keep the original value available until completion.
    fn replace_session_async(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        self.replace_session(session).map(BackendAsyncResult::Ready)
    }
    fn replace_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        self.replace_loop_content(loop_id, update)
            .map(BackendAsyncResult::Ready)
    }
    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()>;
    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let _ = (port_id, external_port, connected);
        Err(anyhow!("external connection management is unavailable"))
    }
    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        let _ = (source_port_id, destination_channel_id, connected);
        Err(anyhow!("mixer routing is unavailable"))
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
pub const RECORDING_CAPACITY_SECONDS: u32 = 120;
pub const INPUT_CAPTURE_CAPACITY_SECONDS: u32 = 30;
pub const WEB_MIDI_OUTPUT_QUEUE_CAPACITY: usize = 1024;

fn engine_oxisynth_parameter(
    parameter: OxiSynthParameter,
) -> shoop_engine::oxisynth::OxiSynthParameter {
    match parameter {
        OxiSynthParameter::ReverbSend => shoop_engine::oxisynth::OxiSynthParameter::ReverbSend,
        OxiSynthParameter::ChorusSend => shoop_engine::oxisynth::OxiSynthParameter::ChorusSend,
    }
}

fn app_oxisynth_parameter(
    parameter: shoop_engine::oxisynth::OxiSynthParameter,
) -> OxiSynthParameter {
    match parameter {
        shoop_engine::oxisynth::OxiSynthParameter::ReverbSend => OxiSynthParameter::ReverbSend,
        shoop_engine::oxisynth::OxiSynthParameter::ChorusSend => OxiSynthParameter::ChorusSend,
    }
}

fn engine_oxisynth_midi_cc_assignment(
    assignment: OxiSynthMidiCcAssignment,
) -> shoop_engine::oxisynth::OxiSynthMidiCcAssignment {
    shoop_engine::oxisynth::OxiSynthMidiCcAssignment {
        parameter: engine_oxisynth_parameter(assignment.parameter),
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn app_oxisynth_midi_cc_assignment(
    assignment: shoop_engine::oxisynth::OxiSynthMidiCcAssignment,
) -> OxiSynthMidiCcAssignment {
    OxiSynthMidiCcAssignment {
        parameter: app_oxisynth_parameter(assignment.parameter),
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn backend_oxisynth_midi_cc_assignment(
    assignment: OxiSynthMidiCcAssignment,
) -> BackendOxiSynthMidiCcAssignment {
    let parameter = match assignment.parameter {
        OxiSynthParameter::ReverbSend => BackendOxiSynthParameter::ReverbSend,
        OxiSynthParameter::ChorusSend => BackendOxiSynthParameter::ChorusSend,
    };
    BackendOxiSynthMidiCcAssignment {
        parameter,
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn app_backend_oxisynth_midi_cc_assignment(
    assignment: BackendOxiSynthMidiCcAssignment,
) -> OxiSynthMidiCcAssignment {
    let parameter = match assignment.parameter {
        BackendOxiSynthParameter::ReverbSend => OxiSynthParameter::ReverbSend,
        BackendOxiSynthParameter::ChorusSend => OxiSynthParameter::ChorusSend,
    };
    OxiSynthMidiCcAssignment {
        parameter,
        channel: assignment.channel,
        controller: assignment.controller,
    }
}

fn validate_backend_midi_cc_assignments(track: &BackendSessionTrack) -> Result<()> {
    if !track.oxisynth_midi_cc_assignments.is_empty()
        && !matches!(
            &track.topology,
            BackendTrackTopology::DryWetProcessor { processor_type, .. }
                if processor_type == TrackProcessorTypeId::OXISYNTH
        )
    {
        return Err(anyhow!(
            "OxiSynth MIDI CC assignments belong to a non-OxiSynth processor"
        ));
    }
    let mut parameters = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for assignment in &track.oxisynth_midi_cc_assignments {
        if assignment.channel > 15
            || assignment.controller > 127
            || !parameters.insert(assignment.parameter)
            || !sources.insert((assignment.channel, assignment.controller))
        {
            return Err(anyhow!("invalid or duplicate OxiSynth MIDI CC assignments"));
        }
    }
    Ok(())
}

pub fn oxisynth_descriptor() -> TrackProcessorDescriptor {
    TrackProcessorDescriptor {
        id: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
        label: "Built-in Synth".to_owned(),
        available: true,
        unavailable_reason: None,
        constraints: shoop_app_api::TrackProcessorConstraints {
            min_dry_audio_channels: Some(2),
            max_dry_audio_channels: Some(2),
            min_wet_audio_channels: Some(2),
            max_wet_audio_channels: Some(2),
            matching_audio_channels: false,
            midi: shoop_app_api::TrackProcessorMidiPolicy::Required,
        },
        features: shoop_app_api::TrackProcessorFeatures {
            state: true,
            embedded_ui: true,
            ..shoop_app_api::TrackProcessorFeatures::default()
        },
        editor: Some(shoop_app_api::TrackProcessorEditorDescriptor::OxiSynth {
            presets: shoop_engine::oxisynth::available_presets()
                .iter()
                .map(|preset| shoop_app_api::TrackProcessorPresetDescriptor {
                    id: preset.id.stable_id(),
                    name: preset.name.to_owned(),
                })
                .collect::<Vec<_>>()
                .into(),
        }),
    }
}

#[cfg(test)]
mod oxisynth_descriptor_tests {
    use super::*;

    #[shoop_wasm_test_support::shoop_test]
    fn descriptor_is_fixed_stereo_midi_only_and_stateful() {
        let descriptor = oxisynth_descriptor();
        assert_eq!(descriptor.id.as_str(), TrackProcessorTypeId::OXISYNTH);
        assert_eq!(descriptor.label, "Built-in Synth");
        assert!(descriptor.available);
        assert!(descriptor.constraints.accepts(2, 2, true));
        assert!(!descriptor.constraints.accepts(0, 1, true));
        assert!(!descriptor.constraints.accepts(0, 2, true));
        assert!(descriptor.features.state);
        assert!(descriptor.features.embedded_ui);
        let Some(shoop_app_api::TrackProcessorEditorDescriptor::OxiSynth { presets }) =
            descriptor.editor
        else {
            panic!("missing OxiSynth editor descriptor");
        };
        assert_eq!(presets.len(), 136);
        assert_eq!(presets[0].id, "0:0");
        assert_eq!(presets[0].name, "Piano 1");
    }
}

pub fn encode_oxisynth_state(state: &OxiSynthState) -> Result<String> {
    let preset =
        shoop_engine::oxisynth::OxiSynthPresetId::from_stable_id(&state.selected_preset_id)?;
    let mut control = shoop_engine::oxisynth::OxiSynthControlState::default();
    control.select_preset(preset)?;
    control.set_send(
        shoop_engine::oxisynth::OxiSynthParameter::ReverbSend,
        state.reverb_send,
    )?;
    control.set_send(
        shoop_engine::oxisynth::OxiSynthParameter::ChorusSend,
        state.chorus_send,
    )?;
    Ok(control.encode())
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

/// Selects concrete session port implementations only; scheduling policy is
/// owned by the surrounding driver and never branches on this value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EnginePortModel {
    Dummy,
    Physical,
}

/// Local elapsed-time policy. The engine runtime itself advances only through
/// explicit process quanta supplied by a driver.
#[derive(Clone, Copy, Default)]
struct LocalElapsedScheduler {
    frame_numerator: u128,
}

impl LocalElapsedScheduler {
    fn frames_due(&mut self, elapsed: Duration, sample_rate: u32, buffer_size: u32) -> (u32, bool) {
        self.frame_numerator = self
            .frame_numerator
            .saturating_add(elapsed.as_nanos().saturating_mul(sample_rate as u128));
        let due = self.frame_numerator / NANOSECONDS_PER_SECOND;
        let max_frames = buffer_size.saturating_mul(MAX_CYCLES_PER_ADVANCE) as u128;
        let processed = due.min(max_frames) as u32;
        self.frame_numerator -= processed as u128 * NANOSECONDS_PER_SECOND;
        let overrun = due > max_frames;
        if overrun {
            self.frame_numerator = 0;
        }
        (processed, overrun)
    }
}

pub struct EngineBackend {
    session: Session,
    global_fx_midi: usize,
    global_fx_port: BackendPortId,
    sample_rate: u32,
    buffer_size: u32,
    processed_frames: u64,
    xruns: u32,
    loops: BTreeMap<BackendLoopId, usize>,
    loop_channels: BTreeMap<BackendLoopId, EngineLoopChannels>,
    composites: BTreeMap<BackendCompositeId, EngineComposite>,
    tracks: BTreeMap<BackendTrackId, EngineTrack>,
    master_bus: EngineBus,
    mixer_routes: BTreeSet<BackendMixerLink>,
    mixer_failures: Vec<BackendMixerFailure>,
    mixer_revision: u64,
    next_loop_id: u64,
    next_composite_id: u64,
    next_composite_slot: u32,
    next_composite_version: u64,
    next_track_id: u64,
    next_port_id: u64,
    next_backend_port_id: u64,
    connection_revision: u64,
    connection_failures: Vec<BackendConnectionFailure>,
    connection_ports: BTreeMap<BackendPortId, EngineConnectionPort>,
    external_connections: DummyExternalConnections,
    web_midi_hosts: BTreeMap<String, BackendHostPortDescriptor>,
    desired_web_midi_connections: BTreeSet<(BackendPortId, String)>,
    port_model: EnginePortModel,
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

/// Local deterministic driver wrapper. It owns elapsed-time policy while the
/// enclosed engine runtime exposes only explicit quantum progression.
pub struct LocalDummyBackend {
    runtime: EngineBackend,
    scheduler: LocalElapsedScheduler,
}

impl std::ops::Deref for LocalDummyBackend {
    type Target = EngineBackend;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl std::ops::DerefMut for LocalDummyBackend {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.runtime
    }
}

struct EngineComposite {
    identity: shoop_engine::LoopIdentity,
    config: Option<BackendCompositeConfig>,
    state: Arc<shoop_engine::state_mirror::CompositeStateMirror>,
    play_after_record: bool,
}

#[derive(Clone)]
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
    engine_port_index: usize,
}

struct EngineBus {
    id: BackendBusId,
    name: String,
    channels: Vec<EngineBusChannel>,
}

struct EngineBusChannel {
    id: BackendBusChannelId,
    label: String,
    input: usize,
    output: usize,
    output_port_id: BackendPortId,
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
    latency: BackendTrackLatencyState,
    oxisynth: Option<EngineOxiFx>,
}

struct EngineOxiFx {
    control: shoop_engine::oxisynth::OxiSynthControlState,
    active: bool,
    visible: bool,
}

impl EngineBackend {
    pub fn loop_smoothing_ms(&self) -> u32 {
        self.session.loop_smoothing_ms()
    }

    pub fn new_dummy(sample_rate: u32, buffer_size: u32) -> Result<LocalDummyBackend> {
        Ok(LocalDummyBackend {
            runtime: Self::new_dummy_runtime(sample_rate, buffer_size)?,
            scheduler: LocalElapsedScheduler::default(),
        })
    }

    fn new_dummy_runtime(sample_rate: u32, buffer_size: u32) -> Result<Self> {
        if sample_rate == 0 || buffer_size == 0 {
            return Err(anyhow!(
                "dummy sample rate and buffer size must be non-zero"
            ));
        }
        let mut session = Session::default();
        session.set_sample_rate(sample_rate);
        session.set_buffer_size(buffer_size);
        let global_registry = PortId(1);
        let global_fx_midi = session.add_port(Port::DummyMidi(DummyMidiPort::new(
            global_registry,
            "global_fx_control_midi_in",
            PortDirection::Input,
        )));
        session.set_global_fx_midi_input(global_fx_midi)?;
        let global_fx_port = BackendPortId::from_raw(9_007_199_254_740_991);
        let global_descriptor = BackendPortDescriptor {
            id: global_fx_port,
            owner: BackendPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Input,
            role: BackendPortRole::MidiInput,
        };
        let mut backend = Self {
            session,
            global_fx_midi,
            global_fx_port,
            sample_rate,
            buffer_size,
            processed_frames: 0,
            xruns: 0,
            loops: BTreeMap::new(),
            loop_channels: BTreeMap::new(),
            composites: BTreeMap::new(),
            tracks: BTreeMap::new(),
            master_bus: EngineBus {
                id: BackendBusId::from_raw(1),
                name: "Master".to_owned(),
                channels: Vec::new(),
            },
            mixer_routes: BTreeSet::new(),
            mixer_failures: Vec::new(),
            mixer_revision: 1,
            next_loop_id: 1,
            next_composite_id: 1,
            next_composite_slot: 0x8000_0000,
            next_composite_version: 1,
            next_track_id: 1,
            next_port_id: 2,
            next_backend_port_id: 1,
            connection_revision: 1,
            connection_failures: Vec::new(),
            connection_ports: BTreeMap::from([(
                global_fx_port,
                EngineConnectionPort {
                    descriptor: global_descriptor,
                    registry_id: global_registry,
                    engine_port_index: global_fx_midi,
                },
            )]),
            external_connections: representative_external_connections(),
            web_midi_hosts: BTreeMap::new(),
            desired_web_midi_connections: BTreeSet::new(),
            port_model: EnginePortModel::Dummy,
            callback_count: 0,
            input_peak: 0.0,
            output_peak: 0.0,
            last_quantum: buffer_size,
            route_scratch: vec![0.0; buffer_size as usize],
            web_midi_output: VecDeque::with_capacity(WEB_MIDI_OUTPUT_QUEUE_CAPACITY),
            web_midi_output_pending: VecDeque::with_capacity(WEB_MIDI_OUTPUT_QUEUE_CAPACITY),
            web_midi_output_dropped: 0,
            web_midi_input_refused: 0,
        };
        backend.initialize_master_bus()?;
        Ok(backend)
    }

    pub fn new_web_audio(sample_rate: u32, max_quantum: u32) -> Result<Self> {
        if sample_rate == 0 || max_quantum == 0 || max_quantum > MAX_WEB_AUDIO_QUANTUM {
            return Err(anyhow!(
                "Web Audio sample rate must be non-zero and quantum must be in 1..={MAX_WEB_AUDIO_QUANTUM}"
            ));
        }
        let mut backend = Self::new_dummy_runtime(sample_rate, max_quantum)?;
        backend.remove_master_bus()?;
        backend.port_model = EnginePortModel::Physical;
        backend.external_connections.remove_all_mock_ports();
        let global = ExternalMidiPort::new("global_fx_control_midi_in", PortDirection::Input);
        backend.session.remove_port(backend.global_fx_midi)?;
        backend.global_fx_midi = backend.session.add_port(Port::ExternalMidi(global));
        backend
            .session
            .set_global_fx_midi_input(backend.global_fx_midi)?;
        backend
            .connection_ports
            .get_mut(&backend.global_fx_port)
            .unwrap()
            .registry_id = PortId(backend.global_fx_midi as u64);
        backend
            .connection_ports
            .get_mut(&backend.global_fx_port)
            .unwrap()
            .engine_port_index = backend.global_fx_midi;
        backend.initialize_master_bus()?;
        Ok(backend)
    }

    fn composite_target_identity(
        &self,
        target: BackendCompositeTarget,
    ) -> Result<shoop_engine::LoopIdentity> {
        match target {
            BackendCompositeTarget::Loop(id) => {
                let index = self.engine_loop_index(id)?;
                self.session
                    .loop_identity(index)
                    .ok_or_else(|| anyhow!("stale composite loop target {id:?}"))
            }
            BackendCompositeTarget::Composite(id) => self
                .composites
                .get(&id)
                .map(|composite| composite.identity)
                .ok_or_else(|| anyhow!("stale composite target {id:?}")),
        }
    }

    fn backend_composite_target(
        &self,
        identity: shoop_engine::LoopIdentity,
    ) -> Option<BackendCompositeTarget> {
        match identity.kind {
            shoop_engine::LoopTargetKind::Basic => self.loops.iter().find_map(|(id, index)| {
                (*index == identity.slot as usize).then_some(BackendCompositeTarget::Loop(*id))
            }),
            shoop_engine::LoopTargetKind::Composite => {
                self.composites.iter().find_map(|(id, composite)| {
                    (composite.identity == identity)
                        .then_some(BackendCompositeTarget::Composite(*id))
                })
            }
        }
    }

    fn composite_primitive_targets(
        &self,
        composite_id: BackendCompositeId,
        visited: &mut BTreeSet<BackendCompositeId>,
        targets: &mut BTreeSet<BackendLoopId>,
    ) -> Result<()> {
        if !visited.insert(composite_id) {
            return Err(anyhow!("composite dependency cycle"));
        }
        let config = self
            .composites
            .get(&composite_id)
            .and_then(|composite| composite.config.as_ref())
            .ok_or_else(|| anyhow!("composite is not configured"))?;
        for entry in config.timelines.iter().flatten().flatten() {
            match entry.target {
                BackendCompositeTarget::Loop(id) => {
                    targets.insert(id);
                }
                BackendCompositeTarget::Composite(id) => {
                    self.composite_primitive_targets(id, visited, targets)?;
                }
            }
        }
        visited.remove(&composite_id);
        Ok(())
    }

    fn compile_composite_timeline(
        &self,
        configs: &BTreeMap<BackendCompositeId, BackendCompositeConfig>,
    ) -> Result<shoop_engine::CompositeBoundaryTimeline> {
        let mut metadata = BTreeMap::new();
        for (id, index) in &self.loops {
            let identity = self
                .session
                .loop_identity(*index)
                .ok_or_else(|| anyhow!("stale primitive loop {id:?}"))?;
            let length = self
                .session
                .loop_(*index)
                .map(|loop_| u64::from(loop_.length()))
                .unwrap_or(0);
            metadata.insert(
                identity,
                shoop_engine::LoopTargetMetadata {
                    identity,
                    length_samples: length,
                },
            );
        }
        for id in configs.keys() {
            let identity = self
                .composites
                .get(id)
                .ok_or_else(|| anyhow!("unknown composite {id:?}"))?
                .identity;
            metadata.insert(
                identity,
                shoop_engine::LoopTargetMetadata {
                    identity,
                    length_samples: 0,
                },
            );
        }

        let dependencies = configs
            .iter()
            .map(|(id, config)| {
                let source = self.composites[id].identity;
                let composite_children = config
                    .timelines
                    .iter()
                    .flatten()
                    .flatten()
                    .filter_map(|entry| match entry.target {
                        BackendCompositeTarget::Composite(child) => {
                            self.composites.get(&child).map(|child| child.identity)
                        }
                        BackendCompositeTarget::Loop(_) => None,
                    })
                    .collect();
                shoop_engine::CompositeDependency {
                    source,
                    composite_children,
                }
            })
            .collect::<Vec<_>>();
        let mut pending = configs.keys().copied().collect::<BTreeSet<_>>();
        let mut compiled = BTreeMap::new();
        while !pending.is_empty() {
            let ready = pending.iter().copied().find(|id| {
                configs[id]
                    .timelines
                    .iter()
                    .flatten()
                    .flatten()
                    .all(|entry| match entry.target {
                        BackendCompositeTarget::Loop(_) => true,
                        BackendCompositeTarget::Composite(child) => compiled.contains_key(&child),
                    })
            });
            let id = ready.ok_or_else(|| anyhow!("composite dependency cycle"))?;
            let config = &configs[&id];
            let source = self.composites[&id].identity;
            let sync_index = self.engine_loop_index(config.sync_source)?;
            let sync_identity = self
                .session
                .loop_identity(sync_index)
                .ok_or_else(|| anyhow!("stale composite sync source"))?;
            let sync_length = self
                .session
                .loop_(sync_index)
                .map(|loop_| u64::from(loop_.length()))
                .unwrap_or(0)
                .max(1);
            let timelines = config
                .timelines
                .iter()
                .map(|sections| {
                    Ok(shoop_engine::CompositeTimeline {
                        sections: sections
                            .iter()
                            .map(|entries| {
                                Ok(shoop_engine::CompositeSection {
                                    entries: entries
                                        .iter()
                                        .map(|entry| {
                                            Ok(shoop_engine::CompositeEntry {
                                                target: self
                                                    .composite_target_identity(entry.target)?,
                                                delay: entry.delay,
                                                n_cycles: entry.n_cycles,
                                                mode: entry.mode.map(to_engine_mode),
                                            })
                                        })
                                        .collect::<Result<Vec<_>>>()?,
                                })
                            })
                            .collect::<Result<Vec<_>>>()?,
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let descriptor = shoop_engine::CompositePlanDescriptor {
                source,
                sync_length,
                timelines,
            };
            let catalog =
                shoop_engine::LoopTargetCatalog::new(metadata.values().copied().collect())
                    .map_err(|error| anyhow!("invalid composite target catalog: {error}"))?;
            let plan = shoop_engine::compile_composite_plan(
                &descriptor,
                &catalog,
                &dependencies,
                shoop_engine::CompositePlanLimits::default(),
            )
            .map_err(|error| anyhow!("composite plan validation failed: {error}"))?;
            let actual_kind = match plan.kind() {
                shoop_engine::CompiledCompositeKind::Regular => BackendCompositeKind::Regular,
                shoop_engine::CompiledCompositeKind::Script => BackendCompositeKind::Script,
            };
            if actual_kind != config.kind {
                return Err(anyhow!("composite kind does not match its entry modes"));
            }
            metadata.get_mut(&source).unwrap().length_samples =
                u64::from(plan.n_iterations()).saturating_mul(sync_length);
            compiled.insert(id, (plan, sync_identity));
            pending.remove(&id);
        }

        let mut timeline = shoop_engine::CompositeBoundaryTimeline::new(
            compiled
                .into_values()
                .map(|(plan, sync_source)| shoop_engine::CompositeTimelineNode {
                    plan,
                    sync_source,
                })
                .collect(),
            shoop_engine::CompositeTimelineLimits::default(),
        )
        .map_err(|error| anyhow!("composite timeline validation failed: {error}"))?;
        for (id, composite) in &self.composites {
            if configs.contains_key(id)
                && !timeline.set_state_mirror(composite.identity, Arc::clone(&composite.state))
            {
                return Err(anyhow!("compiled composite is missing from the timeline"));
            }
        }
        Ok(timeline)
    }

    fn install_composite_configs(
        &mut self,
        configs: BTreeMap<BackendCompositeId, BackendCompositeConfig>,
    ) -> Result<()> {
        let mut timeline = self.compile_composite_timeline(&configs)?;
        let version = self.next_composite_version;
        self.next_composite_version = self.next_composite_version.saturating_add(1);
        timeline
            .prepare_install(version, self.session.primitive_sync_sources())
            .map_err(|error| anyhow!("could not prepare composite timeline: {error}"))?;
        let reclaimed = self
            .session
            .install_prepared_composite_timeline(timeline)
            .map_err(|rejected| {
                anyhow!("could not install composite timeline: {}", rejected.error)
            })?;
        drop(reclaimed);
        for (id, config) in configs {
            if let Some(composite) = self.composites.get_mut(&id) {
                composite.config = Some(config);
                self.session.accept_composite_play_after_record(
                    composite.identity,
                    composite.play_after_record,
                )?;
            }
        }
        Ok(())
    }

    pub fn configure_web_audio_channels(
        &mut self,
        input_channels: u32,
        output_channels: u32,
    ) -> Result<()> {
        if self.port_model != EnginePortModel::Physical {
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
        if self.port_model != EnginePortModel::Physical {
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
        let mut destinations = self
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
        let global_registry = self.connection_ports[&self.global_fx_port].registry_id;
        if self
            .external_connections
            .is_connected(global_registry, host_port_id)
        {
            destinations.push((self.global_fx_midi, self.global_fx_port));
        }
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
        if self.port_model != EnginePortModel::Physical {
            return Err(anyhow!("audio quantum supplied to a non-physical runtime"));
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
        for channel in &self.master_bus.channels {
            let registry_id = self.connection_ports[&channel.output_port_id].registry_id;
            let samples = self
                .session
                .port(channel.output)
                .and_then(Port::as_external)
                .ok_or_else(|| anyhow!("missing physical Master output port"))?
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
        let (supported, configured, kind, instance_name) = match self.port_model {
            EnginePortModel::Dummy => (
                true,
                AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: self.sample_rate,
                    buffer_size: self.buffer_size,
                }),
                AudioDriverKind::Dummy,
                "dummy".to_owned(),
            ),
            EnginePortModel::Physical => (
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
        engine_port_index: usize,
        owner: BackendPortOwner,
        name: String,
        data_type: BackendPortDataType,
        direction: BackendPortDirection,
        role: BackendPortRole,
    ) -> BackendPortDescriptor {
        let id = BackendPortId::from_raw(self.next_backend_port_id);
        self.next_backend_port_id = self.next_backend_port_id.saturating_add(1);
        self.register_connection_port_with_id(
            id,
            registry_id,
            engine_port_index,
            owner,
            name,
            data_type,
            direction,
            role,
        )
    }

    fn register_connection_port_with_id(
        &mut self,
        id: BackendPortId,
        registry_id: PortId,
        engine_port_index: usize,
        owner: BackendPortOwner,
        name: String,
        data_type: BackendPortDataType,
        direction: BackendPortDirection,
        role: BackendPortRole,
    ) -> BackendPortDescriptor {
        let descriptor = BackendPortDescriptor {
            id,
            owner,
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
                engine_port_index,
            },
        );
        if self.port_model == EnginePortModel::Dummy {
            self.external_connections.add_mock_port(
                format!("shoop:{name}"),
                engine_direction(direction),
                engine_data_type(data_type),
            );
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        descriptor
    }

    fn initialize_master_bus(&mut self) -> Result<()> {
        if !self.master_bus.channels.is_empty() {
            return Ok(());
        }
        for (index, label) in MASTER_BUS_CHANNEL_LABELS.into_iter().enumerate() {
            let input = self.session.add_port(Port::Internal(InternalAudioPort::new(
                format!("master:input_{}", index + 1),
                self.buffer_size as usize,
                shoop_engine::PortConnectability::INTERNAL,
                shoop_engine::PortConnectability::INTERNAL,
                0,
            )));
            let output_name = format!("master_out_{}", index + 1);
            let output_registry_id = self.next_port_id();
            let output = if self.port_model == EnginePortModel::Physical {
                self.session.add_port(Port::External(ExternalAudioPort::new(
                    output_name.clone(),
                    PortDirection::Output,
                    self.buffer_size as usize,
                )))
            } else {
                self.session.add_port(Port::Dummy(DummyAudioPort::new(
                    output_registry_id,
                    output_name.clone(),
                    PortDirection::Output,
                    1,
                )))
            };
            self.session.connect_ports_internal(input, output)?;
            let descriptor = self.register_connection_port_with_id(
                BackendPortId::from_raw(9_007_199_254_740_989 + index as u64),
                output_registry_id,
                output,
                BackendPortOwner::Bus(self.master_bus.id),
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            );
            self.master_bus.channels.push(EngineBusChannel {
                id: BackendBusChannelId::from_raw(index as u64 + 1),
                label: label.to_owned(),
                input,
                output,
                output_port_id: descriptor.id,
            });
        }
        self.session.apply_graph_changes()?;
        self.mixer_revision = self.mixer_revision.wrapping_add(1);
        Ok(())
    }

    fn remove_master_bus(&mut self) -> Result<()> {
        let channels = std::mem::take(&mut self.master_bus.channels);
        for channel in channels {
            self.session.remove_port(channel.input)?;
            self.session.remove_port(channel.output)?;
            if let Some(port) = self.connection_ports.remove(&channel.output_port_id) {
                self.external_connections
                    .remove_mock_port(&format!("shoop:{}", port.descriptor.name));
            }
        }
        self.mixer_routes.clear();
        self.session.apply_graph_changes()?;
        self.connection_revision = self.connection_revision.wrapping_add(1);
        self.mixer_revision = self.mixer_revision.wrapping_add(1);
        Ok(())
    }

    fn mixer_snapshot(&mut self) -> BackendMixerSnapshot {
        let channels = self
            .master_bus
            .channels
            .iter()
            .map(|channel| BackendBusChannelState {
                id: channel.id,
                label: channel.label.clone(),
                output_port_id: channel.output_port_id,
            })
            .collect();
        BackendMixerSnapshot {
            revision: self.mixer_revision,
            buses: BTreeMap::from([(
                self.master_bus.id,
                BackendBusState {
                    id: self.master_bus.id,
                    name: self.master_bus.name.clone(),
                    channels,
                },
            )]),
            confirmed_links: self.mixer_routes.clone(),
            failures: std::mem::take(&mut self.mixer_failures),
        }
    }

    fn apply_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        let source = self
            .connection_ports
            .get(&source_port_id)
            .ok_or_else(|| anyhow!("unknown mixer source port {source_port_id:?}"))?;
        validate_mixer_source_descriptor(&source.descriptor)?;
        let source_index = source.engine_port_index;
        let destination_index = self
            .master_bus
            .channels
            .iter()
            .find(|channel| channel.id == destination_channel_id)
            .map(|channel| channel.input)
            .ok_or_else(|| anyhow!("unknown Master bus channel {destination_channel_id:?}"))?;
        let link = BackendMixerLink {
            source_port_id,
            destination_channel_id,
        };
        if self.mixer_routes.contains(&link) == connected {
            return Ok(());
        }
        if connected {
            self.session
                .connect_ports_internal(source_index, destination_index)?;
        } else {
            self.session
                .disconnect_ports_internal(source_index, destination_index)?;
        }
        if let Err(error) = self.session.apply_graph_changes() {
            if connected {
                let _ = self
                    .session
                    .disconnect_ports_internal(source_index, destination_index);
            } else {
                let _ = self
                    .session
                    .connect_ports_internal(source_index, destination_index);
            }
            let _ = self.session.apply_graph_changes();
            self.mixer_failures.push(BackendMixerFailure {
                link,
                desired_connected: connected,
                message: error.to_string(),
            });
            self.mixer_revision = self.mixer_revision.wrapping_add(1);
            return Err(error.into());
        }
        if connected {
            self.mixer_routes.insert(link);
        } else {
            self.mixer_routes.remove(&link);
        }
        self.mixer_revision = self.mixer_revision.wrapping_add(1);
        Ok(())
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

    fn connection_snapshot(&mut self) -> BackendConnectionSnapshot {
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
            failures: std::mem::take(&mut self.connection_failures),
        }
    }

    fn engine_loop_index(&self, id: BackendLoopId) -> Result<usize> {
        self.loops
            .get(&id)
            .copied()
            .ok_or_else(|| anyhow!("unknown backend loop {id:?}"))
    }

    fn loop_has_unsettled_latency_postroll(&self, loop_id: BackendLoopId) -> Result<bool> {
        self.session
            .loop_(self.engine_loop_index(loop_id)?)
            .map(shoop_engine::AudioMidiLoop::has_unsettled_latency_postroll)
            .ok_or_else(|| anyhow!("missing engine loop"))
    }

    fn create_track_loop(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let (audio_routes, midi_route, latency, has_wet_channels) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
            let (audio_routes, midi_route) = match track.topology {
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
            };
            (
                audio_routes,
                midi_route,
                track.latency.clone(),
                track.topology.has_wet_channels(),
            )
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
            let channel = if self.port_model == EnginePortModel::Physical {
                self.session
                    .add_audio_channel_with_bounded_capacity_unprepared(
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
        if let Ok(values) = callback_backend_latency(&latency, has_wet_channels) {
            self.session
                .loop_mut(engine_loop)
                .expect("engine loop was created above")
                .set_pending_latency(values);
        }
        Ok(loop_id)
    }

    fn create_oxisynth_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetProcessor {
            processor_type,
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected processed track topology"));
        };
        if processor_type != TrackProcessorTypeId::OXISYNTH
            || dry_audio_channels != 2
            || wet_audio_channels != 2
            || !dry_midi
        {
            return Err(anyhow!(
                "OxiSynth requires two dry audio channels, two wet audio channels, and one MIDI input"
            ));
        }
        let capture_samples = self.sample_rate as usize * INPUT_CAPTURE_CAPACITY_SECONDS as usize;
        let capture_block_size = capture_samples.div_ceil(32).max(self.buffer_size as usize);
        let mut audio_inputs = Vec::with_capacity(2);
        let mut audio_sends = Vec::with_capacity(2);
        let mut audio_outputs = Vec::with_capacity(2);
        let mut audio_returns = Vec::with_capacity(2);
        let mut ports = Vec::with_capacity(5);
        for index in 0..2 {
            let input_name = format!("{}_audio_dry_in_{}", request.port_name_base, index + 1);
            let input_registry_id = self.next_port_id();
            let input = if self.port_model == EnginePortModel::Physical {
                let mut input = ExternalAudioPort::new(
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                self.session.add_port(Port::External(input))
            } else {
                let mut input = DummyAudioPort::new(
                    input_registry_id,
                    input_name.clone(),
                    PortDirection::Input,
                    capture_block_size,
                );
                input.audio_mut().set_passthrough_muted(true);
                input.audio_mut().set_ringbuffer_n_samples(capture_samples);
                self.session.add_port(Port::Dummy(input))
            };
            let send = self.session.add_port(Port::Internal(InternalAudioPort::new(
                format!("{}:audio_in_{index}", request.port_name_base),
                self.buffer_size as usize,
                shoop_engine::PortConnectability::INTERNAL,
                shoop_engine::PortConnectability::INTERNAL,
                0,
            )));
            self.session.connect_ports_internal(input, send)?;
            ports.push(self.register_connection_port(
                input_registry_id,
                input,
                BackendPortOwner::Track,
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            audio_inputs.push(input);
            audio_sends.push(send);
        }
        for index in 0..2 {
            let output_name = format!("{}_audio_wet_out_{}", request.port_name_base, index + 1);
            let output_registry_id = self.next_port_id();
            let output = if self.port_model == EnginePortModel::Physical {
                self.session.add_port(Port::External(ExternalAudioPort::new(
                    output_name.clone(),
                    PortDirection::Output,
                    self.buffer_size as usize,
                )))
            } else {
                self.session.add_port(Port::Dummy(DummyAudioPort::new(
                    output_registry_id,
                    output_name.clone(),
                    PortDirection::Output,
                    1,
                )))
            };
            let mut receive = InternalAudioPort::new(
                format!("{}:audio_out_{index}", request.port_name_base),
                self.buffer_size as usize,
                shoop_engine::PortConnectability::INTERNAL,
                shoop_engine::PortConnectability::INTERNAL,
                capture_block_size,
            );
            receive
                .audio_mut()
                .set_ringbuffer_n_samples(capture_samples);
            let receive = self.session.add_port(Port::Internal(receive));
            self.session.connect_ports_internal(receive, output)?;
            ports.push(self.register_connection_port(
                output_registry_id,
                output,
                BackendPortOwner::Track,
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
            ));
            audio_outputs.push(output);
            audio_returns.push(receive);
        }
        if self.port_model == EnginePortModel::Physical {
            let output_channels = WEB_AUDIO_DESTINATION_PORTS
                .iter()
                .filter(|host| {
                    self.external_connections
                        .mock_ports()
                        .iter()
                        .any(|port| port.name == **host)
                })
                .count();
            for channel in 0..output_channels.min(2) {
                let registry = self.connection_ports[&ports[2 + channel].id].registry_id;
                self.external_connections
                    .connect(registry, WEB_AUDIO_DESTINATION_PORTS[channel])?;
            }
            self.connection_revision = self.connection_revision.wrapping_add(1);
        }
        let midi_name = format!("{}_dry_midi_in", request.port_name_base);
        let midi_registry_id = self.next_port_id();
        let midi_input = if self.port_model == EnginePortModel::Physical {
            let mut input = ExternalMidiPort::new(midi_name.clone(), PortDirection::Input);
            input.midi_mut().set_passthrough_muted(true);
            self.session.add_port(Port::ExternalMidi(input))
        } else {
            let mut input =
                DummyMidiPort::new(midi_registry_id, midi_name.clone(), PortDirection::Input);
            input.midi_mut().set_passthrough_muted(true);
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
            midi_input,
            BackendPortOwner::Track,
            midi_name,
            BackendPortDataType::Midi,
            BackendPortDirection::Input,
            BackendPortRole::MidiInput,
        );
        let midi_input_port = Some(midi_descriptor.id);
        ports.push(midi_descriptor);

        let control = shoop_engine::oxisynth::OxiSynthControlState::default();
        let processor =
            control.prepare_processor(self.sample_rate as f32, self.buffer_size as usize)?;
        let _ = self
            .session
            .set_oxisynth_processor(request.port_name_base.clone(), processor);
        self.session.set_processor_ports(
            &request.port_name_base,
            Vec::new(),
            audio_returns.clone(),
            vec![midi_target],
        )?;
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
                latency: BackendTrackLatencyState::default(),
                oxisynth: Some(EngineOxiFx {
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
        if let Some(oxisynth) = track.oxisynth.as_mut() {
            oxisynth.active = routing.processor_active;
            self.session
                .set_oxisynth_active(&title, routing.processor_active);
        }
        Ok(())
    }

    fn capture_session_data(&mut self) -> Result<BackendSessionData> {
        let connections = self.connection_snapshot();
        let mut tracks = Vec::with_capacity(self.tracks.len());
        for (track_id, track) in &mut self.tracks {
            let state = BackendTrackState {
                topology: track.topology.clone(),
                fx: track.oxisynth.as_ref().map(engine_oxisynth_fx_state),
                audio_channels: track.audio_outputs.len() as u32,
                midi: track.midi_input.is_some(),
                output_gain_db: track.output_gain_db,
                output_balance: track.output_balance,
                output_muted: track.output_muted,
                input_gain_db: track.input_gain_db,
                input_balance: track.input_balance,
                input_monitoring: track.input_monitoring,
                latency: track.latency.clone(),
                ..Default::default()
            };
            let mut loops = Vec::with_capacity(track.loops.len());
            for loop_id in &track.loops {
                let engine_loop = *self
                    .loops
                    .get(loop_id)
                    .ok_or_else(|| anyhow!("unknown backend loop {loop_id:?}"))?;
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
                if loop_state.has_unsettled_latency_postroll() {
                    return Err(anyhow!(
                        "loop alignment postroll is still finalizing; retry after it settles"
                    ));
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
                            capture_alignment_frames: channel.capture_alignment_frames(),
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
                            capture_alignment_frames: channel.capture_alignment_frames(),
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
                    if self.port_model == EnginePortModel::Physical
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
                oxisynth_midi_cc_assignments: track
                    .oxisynth
                    .as_ref()
                    .map(|fx| fx.control.midi_cc_assignments())
                    .into_iter()
                    .flat_map(|assignments| assignments.iter().collect::<Vec<_>>())
                    .map(app_oxisynth_midi_cc_assignment)
                    .map(backend_oxisynth_midi_cc_assignment)
                    .collect(),
                processor_state: track.oxisynth.as_ref().map(|fx| fx.control.encode()),
            });
        }
        let mut global_connections = connections
            .confirmed_links
            .iter()
            .filter(|link| link.application_port_id == self.global_fx_port)
            .map(|link| link.host_port_id.clone())
            .collect::<BTreeSet<_>>();
        if self.port_model == EnginePortModel::Physical {
            global_connections.extend(
                self.desired_web_midi_connections
                    .iter()
                    .filter(|(desired_port, _)| *desired_port == self.global_fx_port)
                    .map(|(_, host_id)| host_id.clone()),
            );
        }
        let global_ports = vec![BackendSessionPort {
            source_id: self.global_fx_port.raw(),
            descriptor: self.connection_ports[&self.global_fx_port]
                .descriptor
                .clone(),
            external_connections: global_connections.into_iter().collect(),
        }];
        let buses = vec![BackendSessionBus {
            source_id: self.master_bus.id.raw(),
            name: self.master_bus.name.clone(),
            channels: self
                .master_bus
                .channels
                .iter()
                .map(|channel| {
                    let descriptor = self.connection_ports[&channel.output_port_id]
                        .descriptor
                        .clone();
                    let external_connections = connections
                        .confirmed_links
                        .iter()
                        .filter(|link| link.application_port_id == channel.output_port_id)
                        .map(|link| link.host_port_id.clone())
                        .collect();
                    BackendSessionBusChannel {
                        source_id: channel.id.raw(),
                        label: channel.label.clone(),
                        output_port: BackendSessionPort {
                            source_id: channel.output_port_id.raw(),
                            descriptor,
                            external_connections,
                        },
                    }
                })
                .collect(),
        }];
        let mixer_routes = self
            .mixer_routes
            .iter()
            .map(|link| BackendSessionMixerRoute {
                source_port_id: link.source_port_id.raw(),
                destination_channel_id: link.destination_channel_id.raw(),
            })
            .collect();
        Ok(BackendSessionData {
            sample_rate: self.sample_rate,
            tracks,
            buses,
            mixer_routes,
            global_ports,
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
        for track in &data.tracks {
            validate_backend_midi_cc_assignments(track)?;
        }
        if data.tracks.iter().any(|track| {
            let processor_state_valid = match &track.topology {
                BackendTrackTopology::Direct { .. } => track.processor_state.is_none(),
                BackendTrackTopology::DryWetProcessor { processor_type, .. }
                    if processor_type == TrackProcessorTypeId::OXISYNTH =>
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
        let mut replacement = BackendSessionReplacement::default();
        let mut staged = match self.port_model {
            EnginePortModel::Dummy => Self::new_dummy_runtime(self.sample_rate, self.buffer_size)?,
            EnginePortModel::Physical => Self::new_web_audio(self.sample_rate, self.buffer_size)?,
        };
        staged
            .session
            .set_loop_smoothing_ms(self.session.loop_smoothing_ms());
        let source_global = data
            .global_ports
            .first()
            .ok_or_else(|| anyhow!("session has no global FX control port"))?;
        if data.global_ports.len() != 1
            || source_global.descriptor.owner != BackendPortOwner::GlobalFxControl
            || source_global.descriptor.data_type != BackendPortDataType::Midi
            || source_global.descriptor.direction != BackendPortDirection::Input
        {
            return Err(anyhow!("session global FX control port is invalid"));
        }
        replacement
            .global_ports
            .insert(source_global.source_id, staged.global_fx_port);
        staged.external_connections = DummyExternalConnections::default();
        for descriptor in self.external_connections.mock_ports() {
            staged.external_connections.add_mock_port(
                descriptor.name.clone(),
                descriptor.direction,
                descriptor.data_type,
            );
        }
        staged.web_midi_hosts = self.web_midi_hosts.clone();
        if let Some(source_bus) = data.buses.first() {
            if data.buses.len() != 1
                || source_bus.name != "Master"
                || source_bus.channels.len() != 2
                || source_bus.channels.iter().any(|channel| {
                    channel.output_port.descriptor.owner
                        != BackendPortOwner::Bus(BackendBusId::from_raw(source_bus.source_id))
                        || channel.output_port.descriptor.data_type != BackendPortDataType::Audio
                        || channel.output_port.descriptor.direction != BackendPortDirection::Output
                })
            {
                return Err(anyhow!("session Master bus shape is invalid"));
            }
            replacement
                .buses
                .insert(source_bus.source_id, staged.master_bus.id);
            let staged_channels = staged
                .master_bus
                .channels
                .iter()
                .map(|channel| (channel.id, channel.output_port_id))
                .collect::<Vec<_>>();
            for (source_channel, (staged_channel_id, staged_output_port_id)) in
                source_bus.channels.iter().zip(staged_channels)
            {
                replacement
                    .bus_channels
                    .insert(source_channel.source_id, staged_channel_id);
                replacement
                    .bus_output_ports
                    .insert(source_channel.source_id, staged_output_port_id);
                replacement
                    .ports
                    .insert(source_channel.output_port.source_id, staged_output_port_id);
                for external in &source_channel.output_port.external_connections {
                    if let Err(error) =
                        staged.set_port_connected(staged_output_port_id, external, true)
                    {
                        staged.connection_failures.push(BackendConnectionFailure {
                            port_id: staged_output_port_id,
                            external_port: external.clone(),
                            desired_connected: true,
                            message: format!(
                                "could not restore external endpoint {external}: {error}"
                            ),
                        });
                        staged.connection_revision = staged.connection_revision.wrapping_add(1);
                    }
                }
            }
        }
        for external in &source_global.external_connections {
            if let Err(error) = staged.set_port_connected(staged.global_fx_port, external, true) {
                staged.connection_failures.push(BackendConnectionFailure {
                    port_id: staged.global_fx_port,
                    external_port: external.clone(),
                    desired_connected: true,
                    message: format!("could not restore external endpoint {external}: {error}"),
                });
                staged.connection_revision = staged.connection_revision.wrapping_add(1);
            }
        }
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
            for assignment in &source_track.oxisynth_midi_cc_assignments {
                staged.set_track_fx_control(
                    created.track_id,
                    BackendTrackFxControl::OxiSynth(OxiSynthControl::AssignMidiCc(
                        app_backend_oxisynth_midi_cc_assignment(*assignment),
                    )),
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
            if let Err(error) = staged.set_track_latency(
                created.track_id,
                source_track.state.latency.adjustment,
                source_track.state.latency.processor_adjustment,
                source_track.state.latency.processor_manual_frames,
            ) {
                if source_track.state.latency.effective_offset_frames.is_some()
                    && source_track
                        .state
                        .latency
                        .effective_processor_advance_frames
                        .is_some()
                {
                    return Err(error);
                }
                staged
                    .tracks
                    .get_mut(&created.track_id)
                    .expect("created track exists")
                    .latency
                    .clone_from(&source_track.state.latency);
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
                    channel.set_capture_alignment_frames(content.capture_alignment_frames)?;
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
                    channel.set_capture_alignment_frames(content.capture_alignment_frames)?;
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
                if !(staged.port_model == EnginePortModel::Physical
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
                        if let Err(error) =
                            staged.set_port_connected(created_port.id, external, true)
                        {
                            staged.connection_failures.push(BackendConnectionFailure {
                                port_id: created_port.id,
                                external_port: external.clone(),
                                desired_connected: true,
                                message: format!(
                                    "could not restore external endpoint {external}: {error}"
                                ),
                            });
                            staged.connection_revision = staged.connection_revision.wrapping_add(1);
                        }
                    }
                }
            }
            replacement
                .tracks
                .insert(source_track.source_id, created.clone());
        }
        for route in &data.mixer_routes {
            let source_port_id = replacement
                .ports
                .get(&route.source_port_id)
                .copied()
                .ok_or_else(|| anyhow!("session mixer route has a stale source port"))?;
            let destination_channel_id = replacement
                .bus_channels
                .get(&route.destination_channel_id)
                .copied()
                .ok_or_else(|| anyhow!("session mixer route has a stale bus channel"))?;
            staged.apply_mixer_route(source_port_id, destination_channel_id, true)?;
        }
        staged.apply_graph_changes()?;
        Ok((staged, replacement))
    }

    fn prepare_recording_storage(
        &mut self,
        loop_id: BackendLoopId,
        require_existing_alignment: bool,
    ) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        if self
            .session
            .loop_(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .has_unsettled_latency_postroll()
        {
            return Err(anyhow!(
                "loop alignment postroll is still finalizing; retry after it settles"
            ));
        }
        let audio_channels = self
            .loop_channels
            .get(&loop_id)
            .map(|channels| channels.audio.clone())
            .unwrap_or_default();
        if self.port_model == EnginePortModel::Physical {
            for channel in audio_channels {
                self.session
                    .audio_channel_mut(channel)
                    .ok_or_else(|| anyhow!("missing audio loop channel"))?
                    .prepare_bounded_capacity();
            }
        }
        let (latency, has_wet_channels) = self
            .tracks
            .values()
            .find(|track| track.loops.contains(&loop_id))
            .map(|track| (track.latency.clone(), track.topology.has_wet_channels()))
            .unwrap_or_default();
        let values = prepared_backend_latency(&latency, has_wet_channels)?;
        if require_existing_alignment {
            let expected = values.recording_offset().frames();
            let wet_expected = values.wet_recording_offset().frames();
            let channel_set = self.loop_channels.get(&loop_id);
            let has_wet = channel_set.is_some_and(|set| {
                set.audio_modes
                    .iter()
                    .chain(&set.midi_modes)
                    .any(|mode| *mode == BackendChannelMode::Wet)
            });
            if expected != 0 || (has_wet && wet_expected != 0) {
                return Err(anyhow!(
                    "replacement with a nonzero recording offset is unsupported; record a new take instead"
                ));
            }
            let audio_matches = channel_set
                .into_iter()
                .flat_map(|set| &set.audio)
                .all(|channel| {
                    self.session.audio_channel(*channel).is_some_and(|channel| {
                        let expected = if channel.mode() == ChannelMode::Wet {
                            wet_expected
                        } else {
                            expected
                        };
                        channel.capture_alignment_frames() == expected
                    })
                });
            let midi_matches = channel_set
                .into_iter()
                .flat_map(|set| &set.midi)
                .all(|channel| {
                    self.session.midi_channel(*channel).is_some_and(|channel| {
                        let expected = if channel.mode() == ChannelMode::Wet {
                            wet_expected
                        } else {
                            expected
                        };
                        channel.capture_alignment_frames() == expected
                    })
                });
            if !audio_matches || !midi_matches {
                return Err(anyhow!(
                    "replacement offset differs from the take; match the take alignment first"
                ));
            }
        }
        let logical_capacity = self
            .session
            .loop_(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .length() as usize;
        self.session
            .loop_mut(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .prepare_latency(values, logical_capacity)
            .map_err(|error| anyhow!("could not prepare latency retention: {error}"))
    }

    fn apply_graph_changes(&mut self) -> Result<()> {
        self.session
            .apply_graph_changes()
            .map_err(|error| anyhow!("could not apply dummy engine graph: {error}"))
    }
}

fn processor_alignment_from_values(
    values: impl IntoIterator<Item = (BackendChannelMode, i32)>,
) -> Option<u32> {
    let mut dry = None;
    let mut wet = None;
    for (mode, alignment) in values {
        match mode {
            BackendChannelMode::Dry if dry.is_none() => dry = Some(alignment),
            BackendChannelMode::Wet if wet.is_none() => wet = Some(alignment),
            _ => {}
        }
    }
    u32::try_from(wet?.checked_sub(dry?)?).ok()
}

fn validate_take_alignment_window(
    capture_alignment_frames: i32,
    media_layout_offset: i32,
    raw_length: u64,
    logical_length: u32,
    channel_kind: &str,
    channel_index: usize,
) -> Result<()> {
    let mapping = shoop_latency::CaptureFrameMapping::new(capture_alignment_frames)?;
    let raw_start = mapping.raw_media_frame(0, i64::from(media_layout_offset))?;
    let raw_end =
        mapping.raw_media_frame(i64::from(logical_length), i64::from(media_layout_offset))?;
    let complete = raw_start >= 0
        && raw_end >= raw_start
        && u64::try_from(raw_end).is_ok_and(|raw_end| raw_end <= raw_length);
    if !complete {
        return Err(anyhow!(
            "take alignment {capture_alignment_frames} requires {channel_kind} channel {channel_index} media outside its retained raw window"
        ));
    }
    Ok(())
}

pub fn canonical_midi_start_state<'a>(
    start_state: &[Vec<u8>],
    preceding_events: impl IntoIterator<Item = &'a [u8]>,
) -> Vec<Vec<u8>> {
    let mut state = MidiStateTracker::new(TrackWhat::ALL);
    for message in start_state {
        state.process(message);
    }
    for message in preceding_events {
        state.process(message);
    }
    state.state_as_messages()
}

fn resolved_processor_advance(
    state: &BackendTrackLatencyState,
) -> Result<shoop_latency::ProcessorRenderAdvance> {
    let automatic = state
        .automatic_processor_advance_frames
        .map(shoop_latency::ProcessorRenderAdvance::new)
        .transpose()?;
    let adjustment = match state.processor_adjustment {
        BackendProcessorLatencyAdjustment::Automatic => {
            shoop_latency::ProcessorLatencyAdjustment::Automatic
        }
        BackendProcessorLatencyAdjustment::ManualOverride => {
            shoop_latency::ProcessorLatencyAdjustment::ManualOverride
        }
        BackendProcessorLatencyAdjustment::AutomaticPlusTrim => {
            shoop_latency::ProcessorLatencyAdjustment::AutomaticPlusTrim
        }
    };
    Ok(shoop_latency::resolve_processor_advance(
        automatic,
        adjustment,
        state.processor_manual_frames,
    )?)
}

fn resolved_recording_offset(
    state: &BackendTrackLatencyState,
) -> Result<shoop_latency::RecordingOffset> {
    let automatic = state
        .automatic_offset_frames
        .map(shoop_latency::RecordingOffset::new)
        .transpose()?;
    let adjustment = match state.adjustment {
        BackendRecordingOffsetAdjustment::Automatic => {
            shoop_latency::RecordingOffsetAdjustment::Automatic
        }
        BackendRecordingOffsetAdjustment::ManualOverride(frames) => {
            shoop_latency::RecordingOffsetAdjustment::ManualOverride(frames)
        }
        BackendRecordingOffsetAdjustment::AutomaticPlusTrim(frames) => {
            shoop_latency::RecordingOffsetAdjustment::AutomaticPlusTrim(frames)
        }
    };
    Ok(shoop_latency::resolve_recording_offset(
        automatic, adjustment,
    )?)
}

fn prepared_backend_latency(
    state: &BackendTrackLatencyState,
    has_wet_channels: bool,
) -> Result<shoop_engine::PreparedLatency> {
    Ok(shoop_engine::PreparedLatency::new_for_track(
        resolved_recording_offset(state)?,
        resolved_processor_advance(state)?,
        has_wet_channels,
    )?)
}

fn callback_backend_latency(
    state: &BackendTrackLatencyState,
    has_wet_channels: bool,
) -> Result<shoop_engine::PreparedLatency> {
    match prepared_backend_latency(state, has_wet_channels) {
        Ok(values) => Ok(values),
        Err(_) => Ok(shoop_engine::PreparedLatency::new_for_track(
            shoop_latency::RecordingOffset::default(),
            resolved_processor_advance(state)?,
            has_wet_channels,
        )?),
    }
}

fn update_backend_latency(
    state: &mut BackendTrackLatencyState,
    adjustment: BackendRecordingOffsetAdjustment,
    processor_adjustment: BackendProcessorLatencyAdjustment,
    processor_manual_frames: i32,
    has_wet_channels: bool,
) -> Result<()> {
    let mut candidate = state.clone();
    candidate.adjustment = adjustment;
    candidate.processor_adjustment = processor_adjustment;
    candidate.processor_manual_frames = processor_manual_frames;
    let recording = resolved_recording_offset(&candidate);
    let processor = resolved_processor_advance(&candidate);
    candidate.effective_offset_frames = recording.as_ref().ok().map(|value| value.frames());
    candidate.effective_processor_advance_frames =
        processor.as_ref().ok().map(|value| value.frames());
    match prepared_backend_latency(&candidate, has_wet_channels) {
        Ok(_) => {
            candidate.pending = false;
            candidate.error = None;
            *state = candidate;
            Ok(())
        }
        Err(error) => {
            // The two settings are independent unless both resolve and only their
            // checked Wet sum is invalid. Apply a valid side of a partially
            // rejected edit, but never retain an invalid or non-serializable value.
            if recording.is_ok() && processor.is_err() {
                let mut partial = state.clone();
                partial.adjustment = candidate.adjustment;
                partial.effective_offset_frames = candidate.effective_offset_frames;
                if prepared_backend_latency(&partial, has_wet_channels).is_ok() {
                    state.adjustment = partial.adjustment;
                    state.effective_offset_frames = partial.effective_offset_frames;
                }
            } else if processor.is_ok() && recording.is_err() {
                let mut partial = state.clone();
                partial.processor_adjustment = candidate.processor_adjustment;
                partial.processor_manual_frames = candidate.processor_manual_frames;
                partial.effective_processor_advance_frames =
                    candidate.effective_processor_advance_frames;
                if prepared_backend_latency(&partial, has_wet_channels).is_ok() {
                    state.processor_adjustment = partial.processor_adjustment;
                    state.processor_manual_frames = partial.processor_manual_frames;
                    state.effective_processor_advance_frames =
                        partial.effective_processor_advance_frames;
                }
            }
            state.pending = false;
            state.error = Some(error.to_string());
            Err(error)
        }
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

fn engine_oxisynth_fx_state(fx: &EngineOxiFx) -> TrackFxState {
    let editor = fx.control.editor_state();
    TrackFxState {
        processor_type: TrackProcessorTypeId::new(TrackProcessorTypeId::OXISYNTH),
        active: fx.active,
        visible: fx.visible,
        lifecycle: FxLifecycle::Running,
        generation: 0,
        crash_summary: None,
        logs: Arc::from([]),
        editor: Some(TrackProcessorEditorState::OxiSynth(OxiSynthState {
            selected_preset_id: editor.selected_preset.stable_id(),
            reverb_send: editor.reverb_send,
            chorus_send: editor.chorus_send,
            midi_cc_assignments: editor
                .midi_cc_assignments
                .into_iter()
                .map(app_oxisynth_midi_cc_assignment)
                .collect::<Vec<_>>()
                .into(),
        })),
    }
}

impl Backend for EngineBackend {
    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()> {
        self.session.set_loop_smoothing_ms(milliseconds);
        Ok(())
    }

    fn supports_composite_loops(&self) -> bool {
        true
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(vec![oxisynth_descriptor()].into())
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
        if self.port_model != EnginePortModel::Dummy {
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
        let mut target =
            EngineBackend::new_dummy_runtime(resolved.sample_rate, resolved.buffer_size)?;
        target
            .session
            .set_loop_smoothing_ms(self.session.loop_smoothing_ms());
        target.external_connections = self.external_connections.clone();
        let (mut replacement, mapping) = target.build_replacement(session)?;
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
                if processor_type == TrackProcessorTypeId::OXISYNTH =>
            {
                self.create_oxisynth_track(request)
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

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        if self.next_composite_slot == u32::MAX {
            return Err(anyhow!("composite identity capacity exhausted"));
        }
        let id = BackendCompositeId::from_raw(self.next_composite_id);
        self.next_composite_id = self.next_composite_id.saturating_add(1);
        let identity = shoop_engine::LoopIdentity {
            slot: self.next_composite_slot,
            generation: 1,
            kind: shoop_engine::LoopTargetKind::Composite,
        };
        self.next_composite_slot += 1;
        self.composites.insert(
            id,
            EngineComposite {
                identity,
                config: None,
                state: Arc::new(shoop_engine::state_mirror::CompositeStateMirror::new(
                    identity,
                )),
                play_after_record: false,
            },
        );
        Ok(id)
    }

    fn configure_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        if !self.composites.contains_key(&composite_id) {
            return Err(anyhow!("unknown composite {composite_id:?}"));
        }
        let mut configs = self
            .composites
            .iter()
            .filter_map(|(id, composite)| composite.config.clone().map(|config| (*id, config)))
            .collect::<BTreeMap<_, _>>();
        configs.insert(composite_id, config.clone());
        self.install_composite_configs(configs)
    }

    fn transition_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    ) -> Result<()> {
        let (identity, empty) = {
            let composite = self
                .composites
                .get(&composite_id)
                .ok_or_else(|| anyhow!("unknown composite {composite_id:?}"))?;
            (composite.identity, composite.state.read().length == 0)
        };
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            let mut targets = BTreeSet::new();
            self.composite_primitive_targets(composite_id, &mut BTreeSet::new(), &mut targets)?;
            for target in targets {
                self.prepare_recording_storage(target, mode == BackendLoopMode::Replacing)?;
            }
        }
        if mode != BackendLoopMode::Stopped && empty {
            return Ok(());
        }
        if let Some(iteration) = align_to_iteration {
            self.session.accept_composite_immediate_transition(
                identity,
                to_engine_mode(mode),
                iteration,
            )?;
        } else if let Some(delay) = cycles_delay {
            self.session
                .accept_composite_transition(identity, to_engine_mode(mode), delay)?;
        } else {
            self.session.accept_composite_immediate_transition(
                identity,
                to_engine_mode(mode),
                0,
            )?;
        }
        Ok(())
    }

    fn set_composite_play_after_record(
        &mut self,
        composite_id: BackendCompositeId,
        enabled: bool,
    ) -> Result<()> {
        let composite = self
            .composites
            .get_mut(&composite_id)
            .ok_or_else(|| anyhow!("unknown composite {composite_id:?}"))?;
        composite.play_after_record = enabled;
        if composite.config.is_some() {
            self.session
                .accept_composite_play_after_record(composite.identity, enabled)?;
        }
        Ok(())
    }

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<()> {
        if !self.composites.contains_key(&composite_id) {
            return Ok(());
        }
        let configs = self
            .composites
            .iter()
            .filter(|(id, _)| **id != composite_id)
            .filter_map(|(id, composite)| composite.config.clone().map(|config| (*id, config)))
            .collect::<BTreeMap<_, _>>();
        self.install_composite_configs(configs)?;
        self.composites.remove(&composite_id);
        Ok(())
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
        let capture_samples = self.sample_rate as usize * INPUT_CAPTURE_CAPACITY_SECONDS as usize;
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
            let (input, output) = if self.port_model == EnginePortModel::Physical {
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
                input,
                BackendPortOwner::Track,
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
            ));
            ports.push(self.register_connection_port(
                output_registry_id,
                output,
                BackendPortOwner::Track,
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
            let (input, output) = if self.port_model == EnginePortModel::Physical {
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
                input,
                BackendPortOwner::Track,
                input_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
            );
            let output_port = self.register_connection_port(
                output_registry_id,
                output,
                BackendPortOwner::Track,
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
        if self.port_model == EnginePortModel::Physical {
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
                latency: BackendTrackLatencyState::default(),
                oxisynth: None,
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

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        let Some(track) = self.tracks.remove(&track_id) else {
            return Ok(());
        };
        for loop_id in &track.loops {
            if let Some(engine_loop) = self.loops.remove(loop_id) {
                self.session.remove_loop(engine_loop)?;
            }
            self.loop_channels.remove(loop_id);
        }
        for port in track
            .audio_inputs
            .iter()
            .chain(&track.audio_outputs)
            .chain(&track.audio_sends)
            .chain(&track.audio_returns)
            .copied()
            .chain(track.midi_input)
            .chain(track.midi_output)
        {
            self.session.remove_port(port)?;
        }
        self.session.remove_processor(&track.port_name_base);
        let route_count = self.mixer_routes.len();
        self.mixer_routes
            .retain(|link| !track.ports.contains(&link.source_port_id));
        if self.mixer_routes.len() != route_count {
            self.mixer_revision = self.mixer_revision.wrapping_add(1);
        }
        for port_id in &track.ports {
            self.desired_web_midi_connections
                .retain(|(candidate, _)| candidate != port_id);
            if let Some(port) = self.connection_ports.remove(port_id) {
                for endpoint in self
                    .external_connections
                    .connection_status_of(port.registry_id)
                    .keys()
                {
                    let _ = self
                        .external_connections
                        .disconnect(port.registry_id, endpoint);
                }
            }
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        self.apply_graph_changes()?;
        Ok(())
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

    fn set_track_latency(
        &mut self,
        track_id: BackendTrackId,
        adjustment: BackendRecordingOffsetAdjustment,
        processor_adjustment: BackendProcessorLatencyAdjustment,
        processor_manual_frames: i32,
    ) -> Result<()> {
        let loops = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?
            .loops
            .clone();
        for loop_id in &loops {
            let loop_ = self
                .session
                .loop_(self.engine_loop_index(*loop_id)?)
                .ok_or_else(|| anyhow!("missing engine loop"))?;
            if loop_.has_planned_latency_transition() {
                return Err(anyhow!(
                    "cannot change track latency while an operation is armed; cancel it first"
                ));
            }
        }
        let track = self
            .tracks
            .get_mut(&track_id)
            .expect("backend track was checked above");
        let has_wet_channels = track.topology.has_wet_channels();
        let resolution = update_backend_latency(
            &mut track.latency,
            adjustment,
            processor_adjustment,
            processor_manual_frames,
            has_wet_channels,
        );
        let values = callback_backend_latency(&track.latency, has_wet_channels)?;
        for loop_id in loops {
            let engine_loop = self.engine_loop_index(loop_id)?;
            self.session
                .loop_mut(engine_loop)
                .ok_or_else(|| anyhow!("missing engine loop"))?
                .set_pending_latency(values);
        }
        resolution
    }

    fn set_take_alignment(
        &mut self,
        loop_id: BackendLoopId,
        capture_alignment_frames: i32,
    ) -> Result<()> {
        shoop_latency::RecordingOffset::new(capture_alignment_frames)?;
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop {loop_id:?}"))?;
        let engine_loop = self
            .session
            .loop_(self.engine_loop_index(loop_id)?)
            .ok_or_else(|| anyhow!("missing engine loop"))?;
        if matches!(
            engine_loop.mode(),
            LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
        ) {
            return Err(anyhow!(
                "cannot edit take alignment while loop content is changing"
            ));
        }
        if engine_loop.has_planned_recording_transition() {
            return Err(anyhow!(
                "cannot edit take alignment while a recording operation is armed"
            ));
        }
        if engine_loop.mode().is_playing_mode() {
            return Err(anyhow!("stop loop playback before editing take alignment"));
        }
        if engine_loop.has_unsettled_latency_postroll() {
            return Err(anyhow!(
                "cannot edit take alignment while latency postroll is finalizing"
            ));
        }
        let logical_length = engine_loop.length();
        let reference = channels
            .audio
            .first()
            .and_then(|channel| self.session.audio_channel(*channel))
            .map(shoop_engine::AudioChannel::capture_alignment_frames)
            .or_else(|| {
                channels
                    .midi
                    .first()
                    .and_then(|channel| self.session.midi_channel(*channel))
                    .map(shoop_engine::MidiChannel::capture_alignment_frames)
            })
            .unwrap_or(0);
        let delta = capture_alignment_frames
            .checked_sub(reference)
            .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
        let mut audio_candidates = Vec::with_capacity(channels.audio.len());
        for (index, channel_index) in channels.audio.iter().enumerate() {
            let channel = self
                .session
                .audio_channel(*channel_index)
                .ok_or_else(|| anyhow!("missing audio channel"))?;
            let candidate = channel
                .capture_alignment_frames()
                .checked_add(delta)
                .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
            shoop_latency::RecordingOffset::new(candidate)?;
            validate_take_alignment_window(
                candidate,
                channel.start_offset(),
                channel.length() as u64,
                logical_length,
                "audio",
                index,
            )?;
            audio_candidates.push((*channel_index, candidate));
        }
        let mut midi_candidates = Vec::with_capacity(channels.midi.len());
        for (index, channel_index) in channels.midi.iter().enumerate() {
            let channel = self
                .session
                .midi_channel(*channel_index)
                .ok_or_else(|| anyhow!("missing MIDI channel"))?;
            let candidate = channel
                .capture_alignment_frames()
                .checked_add(delta)
                .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
            shoop_latency::RecordingOffset::new(candidate)?;
            validate_take_alignment_window(
                candidate,
                channel.start_offset(),
                u64::from(channel.length()),
                logical_length,
                "MIDI",
                index,
            )?;
            midi_candidates.push((*channel_index, candidate));
        }
        for (channel, candidate) in audio_candidates {
            self.session
                .audio_channel_mut(channel)
                .ok_or_else(|| anyhow!("missing audio channel"))?
                .set_capture_alignment_frames(candidate)
                .expect("take alignment was validated");
        }
        for (channel, candidate) in midi_candidates {
            self.session
                .midi_channel_mut(channel)
                .ok_or_else(|| anyhow!("missing MIDI channel"))?
                .set_capture_alignment_frames(candidate)
                .expect("take alignment was validated");
        }
        Ok(())
    }

    fn set_take_processor_alignment(
        &mut self,
        loop_id: BackendLoopId,
        processor_alignment_frames: u32,
    ) -> Result<()> {
        shoop_latency::ProcessorRenderAdvance::new(processor_alignment_frames)?;
        let channels = self
            .loop_channels
            .get(&loop_id)
            .cloned()
            .ok_or_else(|| anyhow!("unknown backend loop {loop_id:?}"))?;
        let engine_loop = self
            .session
            .loop_(self.engine_loop_index(loop_id)?)
            .ok_or_else(|| anyhow!("missing engine loop"))?;
        if matches!(
            engine_loop.mode(),
            LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
        ) || engine_loop.has_unsettled_latency_postroll()
        {
            return Err(anyhow!(
                "cannot edit take processor alignment while loop content is changing"
            ));
        }
        if engine_loop.has_planned_recording_transition() {
            return Err(anyhow!(
                "cannot edit take processor alignment while a recording operation is armed"
            ));
        }
        if engine_loop.mode().is_playing_mode() {
            return Err(anyhow!(
                "stop loop playback before editing take processor alignment"
            ));
        }
        let logical_length = engine_loop.length();
        let dry_reference = channels
            .audio
            .iter()
            .filter_map(|index| self.session.audio_channel(*index))
            .find(|channel| channel.mode() == ChannelMode::Dry)
            .map(shoop_engine::AudioChannel::capture_alignment_frames)
            .or_else(|| {
                channels
                    .midi
                    .iter()
                    .filter_map(|index| self.session.midi_channel(*index))
                    .find(|channel| channel.mode() == ChannelMode::Dry)
                    .map(shoop_engine::MidiChannel::capture_alignment_frames)
            })
            .ok_or_else(|| anyhow!("take has no dry channel"))?;
        let wet_reference = channels
            .audio
            .iter()
            .filter_map(|index| self.session.audio_channel(*index))
            .find(|channel| channel.mode() == ChannelMode::Wet)
            .map(shoop_engine::AudioChannel::capture_alignment_frames)
            .or_else(|| {
                channels
                    .midi
                    .iter()
                    .filter_map(|index| self.session.midi_channel(*index))
                    .find(|channel| channel.mode() == ChannelMode::Wet)
                    .map(shoop_engine::MidiChannel::capture_alignment_frames)
            })
            .ok_or_else(|| anyhow!("take has no wet channel"))?;
        let current = i64::from(wet_reference) - i64::from(dry_reference);
        let delta = i64::from(processor_alignment_frames) - current;
        let delta = i32::try_from(delta)
            .map_err(|_| anyhow!("take processor alignment adjustment overflowed"))?;
        let mut audio_candidates = Vec::new();
        for (index, channel_index) in channels.audio.iter().enumerate() {
            let channel = self
                .session
                .audio_channel(*channel_index)
                .ok_or_else(|| anyhow!("missing audio channel"))?;
            if channel.mode() != ChannelMode::Wet {
                continue;
            }
            let candidate = channel
                .capture_alignment_frames()
                .checked_add(delta)
                .ok_or_else(|| anyhow!("take processor alignment adjustment overflowed"))?;
            shoop_latency::RecordingOffset::new(candidate)?;
            validate_take_alignment_window(
                candidate,
                channel.start_offset(),
                channel.length() as u64,
                logical_length,
                "audio",
                index,
            )?;
            audio_candidates.push((*channel_index, candidate));
        }
        let mut midi_candidates = Vec::new();
        for (index, channel_index) in channels.midi.iter().enumerate() {
            let channel = self
                .session
                .midi_channel(*channel_index)
                .ok_or_else(|| anyhow!("missing MIDI channel"))?;
            if channel.mode() != ChannelMode::Wet {
                continue;
            }
            let candidate = channel
                .capture_alignment_frames()
                .checked_add(delta)
                .ok_or_else(|| anyhow!("take processor alignment adjustment overflowed"))?;
            shoop_latency::RecordingOffset::new(candidate)?;
            validate_take_alignment_window(
                candidate,
                channel.start_offset(),
                u64::from(channel.length()),
                logical_length,
                "MIDI",
                index,
            )?;
            midi_candidates.push((*channel_index, candidate));
        }
        for (channel, candidate) in audio_candidates {
            self.session
                .audio_channel_mut(channel)
                .ok_or_else(|| anyhow!("missing audio channel"))?
                .set_capture_alignment_frames(candidate)
                .expect("take processor alignment was validated");
        }
        for (channel, candidate) in midi_candidates {
            self.session
                .midi_channel_mut(channel)
                .ok_or_else(|| anyhow!("missing MIDI channel"))?
                .set_capture_alignment_frames(candidate)
                .expect("take processor alignment was validated");
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
        if let Some(fx) = track.oxisynth.as_mut() {
            let title = track.port_name_base.clone();
            match control {
                BackendTrackFxControl::SetActive(active) => {
                    fx.active = active;
                    self.session.set_oxisynth_active(&title, active);
                }
                BackendTrackFxControl::SetVisible(visible) => fx.visible = visible,
                BackendTrackFxControl::ToggleOrRecover => fx.visible = !fx.visible,
                BackendTrackFxControl::RestoreState(state) => {
                    let assignments = fx.control.midi_cc_assignments();
                    let mut replacement =
                        shoop_engine::oxisynth::OxiSynthControlState::from_encoded(&state)?;
                    replacement.set_midi_cc_assignments(assignments);
                    let processor = replacement
                        .prepare_processor(self.sample_rate as f32, self.buffer_size as usize)?;
                    let displaced = self.session.set_oxisynth_processor(title, processor);
                    drop(displaced);
                    fx.control = replacement;
                }
                BackendTrackFxControl::ClearLogs => {}
                BackendTrackFxControl::OxiSynth(control) => match control {
                    OxiSynthControl::SelectPreset(id) => {
                        let preset = shoop_engine::oxisynth::OxiSynthPresetId::from_stable_id(&id)?;
                        let processor = self
                            .session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?;
                        processor.select_preset(preset)?;
                        fx.control.select_preset(preset)?;
                    }
                    OxiSynthControl::SetReverbSend(value) => {
                        let parameter = shoop_engine::oxisynth::OxiSynthParameter::ReverbSend;
                        fx.control.set_send(parameter, value)?;
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .set_send(parameter, value)?;
                    }
                    OxiSynthControl::SetChorusSend(value) => {
                        let parameter = shoop_engine::oxisynth::OxiSynthParameter::ChorusSend;
                        fx.control.set_send(parameter, value)?;
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .set_send(parameter, value)?;
                    }
                    OxiSynthControl::AssignMidiCc(assignment) => {
                        let assignment = engine_oxisynth_midi_cc_assignment(assignment);
                        if !fx.control.assign_midi_cc(assignment) {
                            return Err(anyhow!("invalid OxiSynth MIDI CC assignment"));
                        }
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .assign_midi_cc(assignment);
                    }
                    OxiSynthControl::RemoveMidiCc(parameter) => {
                        let parameter = engine_oxisynth_parameter(parameter);
                        fx.control.remove_midi_cc(parameter);
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .remove_midi_cc(parameter);
                    }
                    OxiSynthControl::ClearMidiCcAssignments => {
                        fx.control.clear_midi_cc_assignments();
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .clear_midi_cc_assignments();
                    }
                    OxiSynthControl::Panic => {
                        self.session
                            .oxisynth_processor_mut(&title)
                            .ok_or_else(|| anyhow!("missing OxiSynth processor"))?
                            .panic();
                    }
                },
            }
            return Ok(());
        }
        Err(anyhow!("track has no processor"))
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        Ok(track.oxisynth.as_ref().map(|fx| fx.control.encode()))
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
        for request in requests {
            let track = self
                .tracks
                .values()
                .find(|track| track.loops.contains(&request.loop_id))
                .ok_or_else(|| anyhow!("loop has no owning track"))?;
            let values =
                prepared_backend_latency(&track.latency, track.topology.has_wet_channels())?;
            let has_wet = !matches!(track.topology, BackendTrackTopology::Direct { .. });
            if values.recording_offset().frames() != 0
                || (has_wet && values.wet_recording_offset().frames() != 0)
            {
                return Err(anyhow!(
                    "grab with a nonzero recording offset is unsupported; record the loop instead"
                ));
            }
        }
        let mut audio_requests = Vec::with_capacity(requests.len());
        let mut midi_captures = Vec::new();
        for request in requests {
            self.prepare_recording_storage(request.loop_id, false)?;
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
        if self.loop_has_unsettled_latency_postroll(loop_id)? {
            return Ok(None);
        }
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

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        if self.loop_has_unsettled_latency_postroll(loop_id)? {
            return Ok(None);
        }
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        let channels = channels
            .audio
            .iter()
            .map(|index| {
                let channel = self
                    .session
                    .audio_channel(*index)
                    .ok_or_else(|| anyhow!("missing audio loop channel"))?;
                Ok(BackendAudioChannelData {
                    samples: Arc::from(channel.data()),
                    start_offset: channel.start_offset(),
                    capture_alignment_frames: channel.capture_alignment_frames(),
                    preplay: channel.pre_play_samples(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(BackendAudioData { channels }))
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        if self.loop_has_unsettled_latency_postroll(loop_id)? {
            return Ok(None);
        }
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        let channels = channels
            .midi
            .iter()
            .zip(&channels.midi_modes)
            .map(|(channel, mode)| {
                let channel = self
                    .session
                    .midi_channel(*channel)
                    .ok_or_else(|| anyhow!("missing MIDI loop channel"))?;
                Ok(BackendMidiChannelData {
                    content_revision: u64::from(channel.data_seq_nr()),
                    mode: *mode,
                    length: channel.length(),
                    events: channel
                        .contents()
                        .into_iter()
                        .map(|event| BackendMidiEvent {
                            time: event.time,
                            data: event.data().to_vec(),
                        })
                        .collect(),
                    start_offset: channel.start_offset(),
                    capture_alignment_frames: channel.capture_alignment_frames(),
                    preplay: channel.pre_play_samples(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(BackendMidiData { channels }))
    }

    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        if self.loop_has_unsettled_latency_postroll(loop_id)? {
            return Err(anyhow!(
                "loop alignment postroll is still finalizing; retry after it settles"
            ));
        }
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
            start_offset: channel_ref.start_offset(),
            capture_alignment_frames: channel_ref.capture_alignment_frames(),
            preplay: channel_ref.pre_play_samples(),
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
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            self.prepare_recording_storage(loop_id, mode == BackendLoopMode::Replacing)?;
        }
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
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            self.prepare_recording_storage(loop_id, mode == BackendLoopMode::Replacing)?;
        }
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

    fn replace_loop_content(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        if update.audio.is_empty() && update.midi.is_empty() {
            return Err(anyhow!("loop content update is empty"));
        }
        let engine_loop = self.engine_loop_index(loop_id)?;
        let state = self
            .session
            .loop_(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?;
        if matches!(
            state.mode(),
            LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
        ) {
            return Err(anyhow!("loop content is changing"));
        }
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing loop channels"))?;
        let audio_indices = update
            .audio
            .iter()
            .map(|item| {
                channels
                    .audio
                    .get(item.channel)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown audio channel {}", item.channel))
            })
            .collect::<Result<Vec<_>>>()?;
        let midi_indices = update
            .midi
            .iter()
            .map(|item| {
                channels
                    .midi
                    .get(item.channel)
                    .copied()
                    .ok_or_else(|| anyhow!("unknown MIDI channel {}", item.channel))
            })
            .collect::<Result<Vec<_>>>()?;
        if audio_indices.iter().collect::<BTreeSet<_>>().len() != audio_indices.len()
            || midi_indices.iter().collect::<BTreeSet<_>>().len() != midi_indices.len()
        {
            return Err(anyhow!("loop content update contains a duplicate channel"));
        }
        for alignment in update
            .audio
            .iter()
            .filter_map(|item| item.capture_alignment_frames)
            .chain(
                update
                    .midi
                    .iter()
                    .filter_map(|item| item.capture_alignment_frames),
            )
        {
            shoop_latency::RecordingOffset::new(alignment)?;
        }
        let midi_events = update
            .midi
            .iter()
            .map(|item| {
                item.events
                    .iter()
                    .map(|event| {
                        shoop_engine::MidiStorageElem::new(event.time, &event.data)
                            .ok_or_else(|| anyhow!("invalid MIDI event"))
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        let logical_length = update.length.unwrap_or(state.length());
        for (channel_index, index) in channels.audio.iter().enumerate() {
            let channel = self
                .session
                .audio_channel(*index)
                .ok_or_else(|| anyhow!("missing audio loop channel"))?;
            let replacement = update
                .audio
                .iter()
                .find(|item| item.channel == channel_index);
            let alignment = replacement
                .and_then(|item| item.capture_alignment_frames)
                .unwrap_or(channel.capture_alignment_frames());
            if alignment != 0 {
                validate_take_alignment_window(
                    alignment,
                    replacement
                        .and_then(|item| item.start_offset)
                        .unwrap_or(channel.start_offset()),
                    replacement.map_or(channel.length() as u64, |item| item.samples.len() as u64),
                    logical_length,
                    "audio",
                    channel_index,
                )?;
            }
        }
        for (channel_index, index) in channels.midi.iter().enumerate() {
            let channel = self
                .session
                .midi_channel(*index)
                .ok_or_else(|| anyhow!("missing MIDI loop channel"))?;
            let replacement = update
                .midi
                .iter()
                .find(|item| item.channel == channel_index);
            let alignment = replacement
                .and_then(|item| item.capture_alignment_frames)
                .unwrap_or(channel.capture_alignment_frames());
            if alignment != 0 {
                validate_take_alignment_window(
                    alignment,
                    replacement
                        .and_then(|item| item.start_offset)
                        .unwrap_or(channel.start_offset()),
                    replacement.map_or(u64::from(channel.length()), |item| u64::from(item.length)),
                    logical_length,
                    "MIDI",
                    channel_index,
                )?;
            }
        }

        for (item, index) in update.audio.iter().zip(audio_indices) {
            let channel = self
                .session
                .audio_channel_mut(index)
                .ok_or_else(|| anyhow!("missing audio channel"))?;
            let retained_offset = channel.start_offset();
            let retained_alignment = channel.capture_alignment_frames();
            channel.load_data(&item.samples);
            channel.set_start_offset(item.start_offset.unwrap_or(retained_offset));
            channel.set_capture_alignment_frames(
                item.capture_alignment_frames.unwrap_or(retained_alignment),
            )?;
            if let Some(preplay) = item.preplay {
                channel.set_pre_play_samples(preplay);
            }
        }
        for ((item, events), index) in update.midi.iter().zip(midi_events).zip(midi_indices) {
            let channel = self
                .session
                .midi_channel_mut(index)
                .ok_or_else(|| anyhow!("missing MIDI channel"))?;
            let retained_offset = channel.start_offset();
            let retained_alignment = channel.capture_alignment_frames();
            channel.set_contents(&events, item.length, Some(&item.start_state));
            channel.set_start_offset(item.start_offset.unwrap_or(retained_offset));
            channel.set_capture_alignment_frames(
                item.capture_alignment_frames.unwrap_or(retained_alignment),
            )?;
            if let Some(preplay) = item.preplay {
                channel.set_pre_play_samples(preplay);
            }
        }
        let loop_ = self
            .session
            .loop_mut(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?;
        loop_.clear_planned_transitions();
        loop_.set_mode(LoopMode::Stopped);
        if let Some(length) = update.length {
            loop_.set_length(length);
        }
        Ok(())
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        if self.loop_channels.contains_key(&loop_id) {
            self.set_loop_timing(loop_id, None, None, Some(length))
        } else {
            let engine_loop = self.engine_loop_index(loop_id)?;
            self.session
                .loop_mut(engine_loop)
                .ok_or_else(|| anyhow!("missing engine loop"))?
                .set_length(length);
            Ok(())
        }
    }

    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        let loop_ = self
            .session
            .loop_(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?;
        if length.is_some()
            && (matches!(
                loop_.mode(),
                LoopMode::Recording | LoopMode::Replacing | LoopMode::RecordingDryIntoWet
            ) || loop_.has_planned_recording_transition())
        {
            return Err(anyhow!(
                "cannot change loop length while a recording operation is armed"
            ));
        }
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        let logical_length = length.unwrap_or(loop_.length());
        for (channel_index, index) in channels.audio.iter().enumerate() {
            let channel = self
                .session
                .audio_channel(*index)
                .ok_or_else(|| anyhow!("missing audio loop channel"))?;
            if channel.capture_alignment_frames() != 0 {
                validate_take_alignment_window(
                    channel.capture_alignment_frames(),
                    start_offset.unwrap_or(channel.start_offset()),
                    channel.length() as u64,
                    logical_length,
                    "audio",
                    channel_index,
                )?;
            }
        }
        for (channel_index, index) in channels.midi.iter().enumerate() {
            let channel = self
                .session
                .midi_channel(*index)
                .ok_or_else(|| anyhow!("missing MIDI loop channel"))?;
            if channel.capture_alignment_frames() != 0 {
                validate_take_alignment_window(
                    channel.capture_alignment_frames(),
                    start_offset.unwrap_or(channel.start_offset()),
                    u64::from(channel.length()),
                    logical_length,
                    "MIDI",
                    channel_index,
                )?;
            }
        }
        for index in &channels.audio {
            let channel = self
                .session
                .audio_channel_mut(*index)
                .ok_or_else(|| anyhow!("missing audio loop channel"))?;
            if let Some(offset) = start_offset {
                channel.set_start_offset(offset);
            }
            if let Some(samples) = preplay {
                channel.set_pre_play_samples(samples);
            }
        }
        for index in &channels.midi {
            let channel = self
                .session
                .midi_channel_mut(*index)
                .ok_or_else(|| anyhow!("missing MIDI loop channel"))?;
            if let Some(offset) = start_offset {
                channel.set_start_offset(offset);
            }
            if let Some(samples) = preplay {
                channel.set_pre_play_samples(samples);
            }
        }
        if let Some(length) = length {
            self.session
                .loop_mut(engine_loop)
                .ok_or_else(|| anyhow!("missing engine loop"))?
                .set_length(length);
        }
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
        let is_web_midi = self.port_model == EnginePortModel::Physical
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

    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        self.apply_mixer_route(source_port_id, destination_channel_id, connected)
    }

    fn advance(&mut self, _elapsed: Duration) {
        // Runtime progression is supplied explicitly by a driver. Local
        // elapsed-time behavior lives in LocalDummyBackend.
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let track_ids = self.tracks.keys().copied().collect::<Vec<_>>();
        for track_id in track_ids {
            self.apply_engine_track_routing(track_id)?;
        }
        let mut tracks = BTreeMap::new();
        for (id, track) in &mut self.tracks {
            let input_peaks = track
                .audio_inputs
                .iter()
                .map(|port| {
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .map(|port| {
                            let peak = amplitude_db(port.input_peak());
                            port.reset_input_peak();
                            peak
                        })
                        .unwrap_or(-200.0)
                })
                .collect();
            let output_peaks = track
                .audio_outputs
                .iter()
                .map(|port| {
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .map(|port| {
                            let peak = amplitude_db(port.output_peak());
                            port.reset_output_peak();
                            peak
                        })
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
                    fx: track.oxisynth.as_ref().map(engine_oxisynth_fx_state),
                    audio_channels: track.audio_outputs.len() as u32,
                    midi: track.midi_input.is_some(),
                    output_gain_db: track.output_gain_db,
                    output_balance: track.output_balance,
                    output_muted: track.output_muted,
                    input_gain_db: track.input_gain_db,
                    input_balance: track.input_balance,
                    input_monitoring: track.input_monitoring,
                    latency: track.latency.clone(),
                    input_peaks,
                    output_peaks,
                    input_midi_activity,
                    output_midi_activity,
                    latest_input_midi_message: track
                        .midi_input
                        .and_then(|port| self.session.port(port))
                        .and_then(Port::midi)
                        .and_then(|port| port.latest_input_message())
                        .map(Into::into),
                },
            );
        }
        let mut loops = BTreeMap::new();
        for (id, engine_loop) in &self.loops {
            let Some((mode, length, position, next_mode, next_transition_delay)) =
                self.session.loop_(*engine_loop).map(|state| {
                    (
                        from_engine_mode(state.mode()),
                        state.length(),
                        state.position(),
                        state
                            .first_planned_transition()
                            .map(|(mode, _)| from_engine_mode(mode)),
                        state.first_planned_transition().map(|(_, delay)| delay),
                    )
                })
            else {
                continue;
            };
            let channels = self.loop_channels.get(id);
            let audio_peaks = channels
                .into_iter()
                .flat_map(|channels| &channels.audio)
                .filter_map(|channel| {
                    let channel = self.session.audio_channel_mut(*channel)?;
                    let peak = amplitude_db(channel.output_peak());
                    channel.reset_output_peak();
                    Some(peak)
                })
                .collect();
            let midi_activity = channels
                .into_iter()
                .flat_map(|channels| &channels.midi)
                .filter_map(|channel| self.session.midi_channel(*channel))
                .any(|channel| channel.n_events_triggered() > 0 || channel.n_notes_active() > 0);
            loops.insert(
                *id,
                BackendLoopState {
                    mode,
                    length,
                    position,
                    next_mode,
                    next_transition_delay,
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
                    audio_peaks,
                    midi_activity,
                    capture_alignment_frames: channels
                        .and_then(|channels| {
                            channels
                                .audio
                                .first()
                                .and_then(|channel| self.session.audio_channel(*channel))
                                .map(|channel| channel.capture_alignment_frames())
                                .or_else(|| {
                                    channels
                                        .midi
                                        .first()
                                        .and_then(|channel| self.session.midi_channel(*channel))
                                        .map(|channel| channel.capture_alignment_frames())
                                })
                        })
                        .unwrap_or(0),
                    processor_alignment_frames: channels.and_then(|channels| {
                        processor_alignment_from_values(
                            channels
                                .audio
                                .iter()
                                .zip(&channels.audio_modes)
                                .filter_map(|(channel, mode)| {
                                    self.session
                                        .audio_channel(*channel)
                                        .map(|channel| (*mode, channel.capture_alignment_frames()))
                                })
                                .chain(channels.midi.iter().zip(&channels.midi_modes).filter_map(
                                    |(channel, mode)| {
                                        self.session.midi_channel(*channel).map(|channel| {
                                            (*mode, channel.capture_alignment_frames())
                                        })
                                    },
                                )),
                        )
                    }),
                },
            );
        }
        let composites = self
            .composites
            .iter()
            .filter_map(|(id, composite)| {
                let state = composite.state.read();
                state.installed.then(|| {
                    let active_children = state
                        .active_children()
                        .filter_map(|child| {
                            Some(BackendActiveCompositeChild {
                                target: self.backend_composite_target(child.identity)?,
                                mode: from_engine_mode(child.mode),
                                cycle_offset: child.cycle_offset,
                            })
                        })
                        .collect();
                    (
                        *id,
                        BackendCompositeState {
                            mode: from_engine_mode(state.mode),
                            next_mode: state.next_mode.map(from_engine_mode),
                            next_transition_delay: state.next_mode_delay,
                            iteration: state.iteration,
                            cycle_count: state.cycle_count,
                            length: state.length,
                            position: state.position,
                            active_plan_version: state.active_plan_version,
                            pending_plan_version: state.pending_plan_version,
                            active_children,
                        },
                    )
                })
            })
            .collect();
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: 0.0,
                xruns: self.xruns,
                buffer_size: if self.port_model == EnginePortModel::Physical {
                    self.last_quantum
                } else {
                    self.buffer_size
                },
                sample_rate: self.sample_rate,
                driver_state: if self.port_model == EnginePortModel::Physical {
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
                render_memory_growths: 0,
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
            composites,
            connections: self.connection_snapshot(),
            mixer: self.mixer_snapshot(),
            mutation_failures: Vec::new(),
        })
    }

    fn wait_idle(&mut self) {
        let _ = self.apply_graph_changes();
    }
}

impl Backend for LocalDummyBackend {
    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()> {
        self.runtime.set_loop_smoothing_ms(milliseconds)
    }

    fn supports_composite_loops(&self) -> bool {
        self.runtime.supports_composite_loops()
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        self.runtime.track_processor_catalog()
    }

    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        self.runtime.audio_driver_state()
    }

    fn preflight_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        self.runtime.preflight_audio_driver(config)
    }

    fn switch_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
        confirmed_sample_rate: u32,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let replacement =
            self.runtime
                .switch_audio_driver(config, confirmed_sample_rate, session)?;
        self.scheduler = LocalElapsedScheduler::default();
        Ok(replacement)
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        self.runtime.create_track(request)
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        self.runtime.create_loop()
    }

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        self.runtime.create_composite_loop()
    }

    fn configure_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        self.runtime.configure_composite_loop(composite_id, config)
    }

    fn transition_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    ) -> Result<()> {
        self.runtime
            .transition_composite_loop(composite_id, mode, cycles_delay, align_to_iteration)
    }

    fn set_composite_play_after_record(
        &mut self,
        composite_id: BackendCompositeId,
        enabled: bool,
    ) -> Result<()> {
        self.runtime
            .set_composite_play_after_record(composite_id, enabled)
    }

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<()> {
        self.runtime.remove_composite_loop(composite_id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        self.runtime.create_direct_track(request)
    }

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        self.runtime.remove_track(track_id)
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        self.runtime.add_loop_to_track(track_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        self.runtime.set_track_control(track_id, control)
    }

    fn set_track_latency(
        &mut self,
        track_id: BackendTrackId,
        adjustment: BackendRecordingOffsetAdjustment,
        processor_adjustment: BackendProcessorLatencyAdjustment,
        processor_manual_frames: i32,
    ) -> Result<()> {
        self.runtime.set_track_latency(
            track_id,
            adjustment,
            processor_adjustment,
            processor_manual_frames,
        )
    }

    fn set_take_alignment(
        &mut self,
        loop_id: BackendLoopId,
        capture_alignment_frames: i32,
    ) -> Result<()> {
        self.runtime
            .set_take_alignment(loop_id, capture_alignment_frames)
    }

    fn set_take_processor_alignment(
        &mut self,
        loop_id: BackendLoopId,
        processor_alignment_frames: u32,
    ) -> Result<()> {
        self.runtime
            .set_take_processor_alignment(loop_id, processor_alignment_frames)
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        self.runtime.inject_midi_input(track_id, events)
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        self.runtime.set_track_fx_control(track_id, control)
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        self.runtime.track_fx_state_string(track_id)
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        self.runtime.set_loop_gain(loop_id, gain)
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        self.runtime.set_loop_balance(loop_id, balance)
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        self.runtime.grab_loops(requests)
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        self.runtime.loop_audio_data(loop_id)
    }

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        self.runtime.loop_audio_data_with_metadata(loop_id)
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        self.runtime.loop_midi_data(loop_id)
    }

    fn loop_audio_data_chunk(
        &mut self,
        loop_id: BackendLoopId,
        channel: usize,
        offset: usize,
        max_samples: usize,
    ) -> Result<BackendAudioDataChunk> {
        self.runtime
            .loop_audio_data_chunk(loop_id, channel, offset, max_samples)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.runtime.set_loop_sync_source(loop_id, source)
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        self.runtime.transition_loop(loop_id, mode, cycles_delay)
    }

    fn transition_loop_aligned(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    ) -> Result<()> {
        self.runtime
            .transition_loop_aligned(loop_id, mode, cycles_delay, align_to_sync_at)
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.runtime.clear_loop(loop_id)
    }

    fn replace_loop_content(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        self.runtime.replace_loop_content(loop_id, update)
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        self.runtime.set_loop_length(loop_id, length)
    }

    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        self.runtime
            .set_loop_timing(loop_id, start_offset, preplay, length)
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        self.runtime.capture_session()
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        self.runtime.replace_session(session)
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        self.runtime
            .set_port_connected(port_id, external_port, connected)
    }

    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        self.runtime
            .set_mixer_route(source_port_id, destination_channel_id, connected)
    }

    fn advance(&mut self, elapsed: Duration) {
        let (processed, overrun) =
            self.scheduler
                .frames_due(elapsed, self.runtime.sample_rate, self.runtime.buffer_size);
        if overrun {
            self.runtime.xruns = self.runtime.xruns.saturating_add(1);
        }
        self.runtime.advance_frames(processed);
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        self.runtime.poll()
    }

    fn wait_idle(&mut self) {
        self.runtime.wait_idle();
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
        capture_alignment_frames: 0,
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
        capture_alignment_frames: 0,
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
    composites: BTreeMap<BackendCompositeId, BackendCompositeState>,
    composite_configs: BTreeMap<BackendCompositeId, BackendCompositeConfig>,
    composite_loops_supported: bool,
    fail_next_composite_configuration: Option<String>,
    next_loop_id: u64,
    next_composite_id: u64,
    next_track_id: u64,
    next_port_id: u64,
    fail_track_creation_after: Option<usize>,
    fail_next_session_replace: Option<String>,
    fail_next_loop_content_replace: Option<String>,
    pending_session_captures: usize,
    pending_loop_content_replacements: usize,
    failed_midi_input_tracks: BTreeSet<BackendTrackId>,
    processor_catalog: Arc<[TrackProcessorDescriptor]>,
    default_fx_state_string: String,
    fail_fx_state_restore: bool,
    audio_driver_control: FakeAudioDriverControl,
    operations: Vec<FakeOperation>,
    connections: FakeConnectionControl,
    mixer: BackendMixerSnapshot,
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
    CreateComposite(BackendCompositeId),
    ConfigureComposite(BackendCompositeId, BackendCompositeConfig),
    TransitionComposite(
        BackendCompositeId,
        BackendLoopMode,
        Option<u32>,
        Option<i64>,
    ),
    RemoveComposite(BackendCompositeId),
    CreateTrack(BackendTrackId),
    RemoveTrack(BackendTrackId),
    AddTrackLoop(BackendTrackId, BackendLoopId),
    SetTrackControl(BackendTrackId, BackendTrackControl),
    SetLoopGain(BackendLoopId, f32),
    SetLoopBalance(BackendLoopId, f32),
    GrabLoops(Vec<BackendGrabRequest>),
    SetSyncSource(BackendLoopId, Option<BackendLoopId>),
    Transition(BackendLoopId, BackendLoopMode, Option<u32>),
    Clear(BackendLoopId),
    ReplaceLoopContent(BackendLoopId, BackendLoopContentUpdate),
    SetLoopLength(BackendLoopId, u32),
    InjectMidiInput(BackendTrackId, Vec<BackendMidiEvent>),
    SetPortConnected(BackendPortId, String, bool),
    SetMixerRoute(BackendMixerLink, bool),
    SetLoopSmoothingMs(u32),
}

impl Default for FakeBackend {
    fn default() -> Self {
        let connections = FakeConnectionControl {
            state: Arc::new(Mutex::new(FakeConnectionState::default())),
        };
        let global_fx_port = BackendPortDescriptor {
            id: BackendPortId::from_raw(9_007_199_254_740_991),
            owner: BackendPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Input,
            role: BackendPortRole::MidiInput,
        };
        let bus_id = BackendBusId::from_raw(1);
        let master_channels = [
            (
                BackendBusChannelId::from_raw(1),
                "Left",
                BackendPortId::from_raw(9_007_199_254_740_989),
            ),
            (
                BackendBusChannelId::from_raw(2),
                "Right",
                BackendPortId::from_raw(9_007_199_254_740_990),
            ),
        ];
        connections.with_state(|state| {
            state.ports.insert(global_fx_port.id, global_fx_port);
            for (_, label, output_port_id) in master_channels {
                state.ports.insert(
                    output_port_id,
                    BackendPortDescriptor {
                        id: output_port_id,
                        owner: BackendPortOwner::Bus(bus_id),
                        name: format!("master_out_{}", label.to_lowercase()),
                        data_type: BackendPortDataType::Audio,
                        direction: BackendPortDirection::Output,
                        role: BackendPortRole::AudioOutput,
                    },
                );
            }
        });
        let mixer = BackendMixerSnapshot {
            revision: 1,
            buses: BTreeMap::from([(
                bus_id,
                BackendBusState {
                    id: bus_id,
                    name: "Master".to_owned(),
                    channels: master_channels
                        .into_iter()
                        .map(|(id, label, output_port_id)| BackendBusChannelState {
                            id,
                            label: label.to_owned(),
                            output_port_id,
                        })
                        .collect(),
                },
            )]),
            ..Default::default()
        };
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
            composites: BTreeMap::new(),
            composite_configs: BTreeMap::new(),
            composite_loops_supported: false,
            fail_next_composite_configuration: None,
            next_loop_id: 1,
            next_composite_id: 1,
            next_track_id: 1,
            next_port_id: 1,
            fail_track_creation_after: None,
            fail_next_session_replace: None,
            fail_next_loop_content_replace: None,
            pending_session_captures: 0,
            pending_loop_content_replacements: 0,
            failed_midi_input_tracks: BTreeSet::new(),
            processor_catalog: Arc::from([]),
            default_fx_state_string: "{}".to_owned(),
            fail_fx_state_restore: false,
            audio_driver_control: FakeAudioDriverControl::default(),
            operations: Vec::new(),
            connections,
            mixer,
            loop_content: BTreeMap::new(),
        }
    }
}

impl FakeBackend {
    pub fn operations(&self) -> &[FakeOperation] {
        &self.operations
    }

    pub fn loop_smoothing_ms(&self) -> Option<u32> {
        self.operations
            .iter()
            .rev()
            .find_map(|operation| match operation {
                FakeOperation::SetLoopSmoothingMs(milliseconds) => Some(*milliseconds),
                _ => None,
            })
    }

    pub fn loop_sync_source(&self, loop_id: BackendLoopId) -> Option<BackendLoopId> {
        self.sync_sources.get(&loop_id).copied().flatten()
    }

    pub fn enable_composite_loops(&mut self) {
        self.composite_loops_supported = true;
    }

    pub fn fail_next_composite_configuration(&mut self, message: impl Into<String>) {
        self.fail_next_composite_configuration = Some(message.into());
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

    pub fn fail_next_loop_content_replace(&mut self, message: impl Into<String>) {
        self.fail_next_loop_content_replace = Some(message.into());
    }

    pub fn delay_next_async_loop_copy(&mut self) {
        self.pending_session_captures = 1;
        self.pending_loop_content_replacements = 1;
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
            owner: BackendPortOwner::Track,
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
    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()> {
        self.operations
            .push(FakeOperation::SetLoopSmoothingMs(milliseconds));
        Ok(())
    }

    fn supports_composite_loops(&self) -> bool {
        self.composite_loops_supported
    }

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
                    replacement.global_ports.clear();
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

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        let id = BackendCompositeId::from_raw(self.next_composite_id);
        self.next_composite_id = self.next_composite_id.saturating_add(1);
        self.composites.insert(id, BackendCompositeState::default());
        self.operations.push(FakeOperation::CreateComposite(id));
        Ok(id)
    }

    fn configure_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        if !self.composites.contains_key(&composite_id) {
            return Err(anyhow!("unknown fake composite {composite_id:?}"));
        }
        if let Some(message) = self.fail_next_composite_configuration.take() {
            return Err(anyhow!(message));
        }
        let sync_length = self
            .loops
            .get(&config.sync_source)
            .map(|state| u64::from(state.length))
            .ok_or_else(|| anyhow!("unknown fake composite sync source"))?;
        if sync_length == 0 && config.timelines.iter().flatten().flatten().next().is_some() {
            return Err(anyhow!("composite synchronization length is zero"));
        }
        let mut length_cycles = 0u64;
        for timeline in &config.timelines {
            let mut timeline_cycles = 0u64;
            for section in timeline {
                let mut section_cycles = 0u64;
                for entry in section {
                    let delay = u64::try_from(entry.delay)
                        .map_err(|_| anyhow!("composite entry delay is negative"))?;
                    let child_length = match entry.target {
                        BackendCompositeTarget::Loop(id) => self
                            .loops
                            .get(&id)
                            .map(|state| u64::from(state.length))
                            .ok_or_else(|| anyhow!("unknown fake composite loop target {id:?}"))?,
                        BackendCompositeTarget::Composite(id) if id == composite_id => {
                            return Err(anyhow!("composite dependency cycle"));
                        }
                        BackendCompositeTarget::Composite(id) => self
                            .composites
                            .get(&id)
                            .map(|state| state.length)
                            .ok_or_else(|| anyhow!("unknown fake composite target {id:?}"))?,
                    };
                    let duration = match entry.n_cycles {
                        Some(cycles) if cycles <= 0 => {
                            return Err(anyhow!("composite cycle count is not positive"));
                        }
                        Some(cycles) => u64::try_from(cycles)
                            .map_err(|_| anyhow!("composite cycle count is out of range"))?,
                        None => child_length.div_ceil(sync_length).max(1),
                    };
                    section_cycles = section_cycles.max(
                        delay
                            .checked_add(duration)
                            .ok_or_else(|| anyhow!("composite duration overflow"))?,
                    );
                }
                timeline_cycles = timeline_cycles
                    .checked_add(section_cycles)
                    .ok_or_else(|| anyhow!("composite timeline overflow"))?;
            }
            length_cycles = length_cycles.max(timeline_cycles);
        }
        let length = length_cycles
            .checked_mul(sync_length)
            .ok_or_else(|| anyhow!("composite length overflow"))?;
        let state = self.composites.get_mut(&composite_id).unwrap();
        state.length = length;
        state.active_plan_version = state.active_plan_version.saturating_add(1);
        self.composite_configs.insert(composite_id, config.clone());
        self.operations.push(FakeOperation::ConfigureComposite(
            composite_id,
            config.clone(),
        ));
        Ok(())
    }

    fn transition_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    ) -> Result<()> {
        let state = self
            .composites
            .get_mut(&composite_id)
            .ok_or_else(|| anyhow!("unknown fake composite {composite_id:?}"))?;
        if cycles_delay.is_some() && align_to_iteration.is_none() {
            state.next_mode = Some(mode);
            state.next_transition_delay = cycles_delay;
        } else {
            state.mode = if state.length == 0 {
                BackendLoopMode::Stopped
            } else {
                mode
            };
            state.next_mode = None;
            state.next_transition_delay = None;
            state.iteration = align_to_iteration.unwrap_or(0).max(0) as u32;
            state.position = u64::from(state.iteration).min(state.length);
        }
        self.operations.push(FakeOperation::TransitionComposite(
            composite_id,
            mode,
            cycles_delay,
            align_to_iteration,
        ));
        Ok(())
    }

    fn set_composite_play_after_record(
        &mut self,
        composite_id: BackendCompositeId,
        _enabled: bool,
    ) -> Result<()> {
        if !self.composites.contains_key(&composite_id) {
            return Err(anyhow!("unknown fake composite {composite_id:?}"));
        }
        Ok(())
    }

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<()> {
        self.composites.remove(&composite_id);
        self.composite_configs.remove(&composite_id);
        self.operations
            .push(FakeOperation::RemoveComposite(composite_id));
        Ok(())
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

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        let Some(track) = self.tracks.remove(&track_id) else {
            return Ok(());
        };
        for loop_id in track.loops {
            self.loops.remove(&loop_id);
            self.loop_content.remove(&loop_id);
            self.sync_sources.remove(&loop_id);
            for source in self.sync_sources.values_mut() {
                if *source == Some(loop_id) {
                    *source = None;
                }
            }
        }
        self.failed_midi_input_tracks.remove(&track_id);
        let removed_ports = track.ports;
        self.connections.with_state(|state| {
            for port_id in &removed_ports {
                state.ports.remove(port_id);
                state
                    .connected
                    .retain(|(candidate, _)| candidate != port_id);
                state
                    .pending
                    .retain(|(candidate, _, _)| candidate != port_id);
                state.failures.retain(|failure| failure.port_id != *port_id);
            }
            state.revision = state.revision.wrapping_add(1);
        });
        let route_count = self.mixer.confirmed_links.len();
        self.mixer
            .confirmed_links
            .retain(|link| !removed_ports.contains(&link.source_port_id));
        if self.mixer.confirmed_links.len() != route_count {
            self.mixer.revision = self.mixer.revision.wrapping_add(1);
        }
        self.operations.push(FakeOperation::RemoveTrack(track_id));
        Ok(())
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
                if midi {
                    vec![empty_midi_content(BackendChannelMode::Direct)]
                } else {
                    Vec::new()
                },
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
                if dry_midi {
                    vec![empty_midi_content(BackendChannelMode::Dry)]
                } else {
                    Vec::new()
                },
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

    fn set_track_latency(
        &mut self,
        track_id: BackendTrackId,
        adjustment: BackendRecordingOffsetAdjustment,
        processor_adjustment: BackendProcessorLatencyAdjustment,
        processor_manual_frames: i32,
    ) -> Result<()> {
        let loops = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?
            .loops
            .clone();
        if loops.iter().any(|loop_id| {
            self.loops.get(loop_id).is_some_and(|state| {
                state.next_mode.is_some_and(|mode| {
                    matches!(
                        mode,
                        BackendLoopMode::Recording
                            | BackendLoopMode::Replacing
                            | BackendLoopMode::PlayingDryThroughWet
                            | BackendLoopMode::RecordingDryIntoWet
                    )
                })
            })
        }) {
            return Err(anyhow!(
                "cannot change track latency while an operation is armed; cancel it first"
            ));
        }
        let track = self
            .tracks
            .get_mut(&track_id)
            .expect("fake track was checked above");
        let has_wet_channels = track.state.topology.has_wet_channels();
        update_backend_latency(
            &mut track.state.latency,
            adjustment,
            processor_adjustment,
            processor_manual_frames,
            has_wet_channels,
        )
    }

    fn set_take_alignment(
        &mut self,
        loop_id: BackendLoopId,
        capture_alignment_frames: i32,
    ) -> Result<()> {
        shoop_latency::RecordingOffset::new(capture_alignment_frames)?;
        if self.loops.get(&loop_id).is_some_and(|state| {
            matches!(
                state.mode,
                BackendLoopMode::Recording
                    | BackendLoopMode::Replacing
                    | BackendLoopMode::RecordingDryIntoWet
            )
        }) {
            return Err(anyhow!(
                "cannot edit take alignment while loop content is changing"
            ));
        }
        if self.loops.get(&loop_id).is_some_and(|state| {
            state.next_mode.is_some_and(|mode| {
                matches!(
                    mode,
                    BackendLoopMode::Recording
                        | BackendLoopMode::Replacing
                        | BackendLoopMode::RecordingDryIntoWet
                )
            })
        }) {
            return Err(anyhow!(
                "cannot edit take alignment while a recording operation is armed"
            ));
        }
        if self.loops.get(&loop_id).is_some_and(|state| {
            matches!(
                state.mode,
                BackendLoopMode::Playing | BackendLoopMode::PlayingDryThroughWet
            )
        }) {
            return Err(anyhow!("stop loop playback before editing take alignment"));
        }
        let content = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        let reference = content
            .audio
            .first()
            .map(|channel| channel.capture_alignment_frames)
            .or_else(|| {
                content
                    .midi
                    .first()
                    .map(|channel| channel.capture_alignment_frames)
            })
            .unwrap_or(0);
        let delta = capture_alignment_frames
            .checked_sub(reference)
            .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
        let audio_candidates = content
            .audio
            .iter()
            .enumerate()
            .map(|(index, channel)| {
                let candidate = channel
                    .capture_alignment_frames
                    .checked_add(delta)
                    .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
                shoop_latency::RecordingOffset::new(candidate)?;
                validate_take_alignment_window(
                    candidate,
                    channel.start_offset,
                    channel.samples.len() as u64,
                    content.length,
                    "audio",
                    index,
                )?;
                Ok(candidate)
            })
            .collect::<Result<Vec<_>>>()?;
        let midi_candidates = content
            .midi
            .iter()
            .enumerate()
            .map(|(index, channel)| {
                let candidate = channel
                    .capture_alignment_frames
                    .checked_add(delta)
                    .ok_or_else(|| anyhow!("take alignment adjustment overflowed"))?;
                shoop_latency::RecordingOffset::new(candidate)?;
                validate_take_alignment_window(
                    candidate,
                    channel.start_offset,
                    u64::from(channel.length),
                    content.length,
                    "MIDI",
                    index,
                )?;
                Ok(candidate)
            })
            .collect::<Result<Vec<_>>>()?;
        let content = self
            .loop_content
            .get_mut(&loop_id)
            .expect("fake loop was checked above");
        for (channel, candidate) in content.audio.iter_mut().zip(audio_candidates) {
            channel.capture_alignment_frames = candidate;
        }
        for (channel, candidate) in content.midi.iter_mut().zip(midi_candidates) {
            channel.capture_alignment_frames = candidate;
        }
        if let Some(state) = self.loops.get_mut(&loop_id) {
            state.capture_alignment_frames = capture_alignment_frames;
        }
        Ok(())
    }

    fn set_take_processor_alignment(
        &mut self,
        loop_id: BackendLoopId,
        processor_alignment_frames: u32,
    ) -> Result<()> {
        shoop_latency::ProcessorRenderAdvance::new(processor_alignment_frames)?;
        if self.loops.get(&loop_id).is_some_and(|state| {
            matches!(
                state.mode,
                BackendLoopMode::Recording
                    | BackendLoopMode::Replacing
                    | BackendLoopMode::RecordingDryIntoWet
            )
        }) {
            return Err(anyhow!(
                "cannot edit take processor alignment while loop content is changing"
            ));
        }
        if self.loops.get(&loop_id).is_some_and(|state| {
            state.next_mode.is_some_and(|mode| {
                matches!(
                    mode,
                    BackendLoopMode::Recording
                        | BackendLoopMode::Replacing
                        | BackendLoopMode::RecordingDryIntoWet
                )
            })
        }) {
            return Err(anyhow!(
                "cannot edit take processor alignment while a recording operation is armed"
            ));
        }
        if self.loops.get(&loop_id).is_some_and(|state| {
            matches!(
                state.mode,
                BackendLoopMode::Playing | BackendLoopMode::PlayingDryThroughWet
            )
        }) {
            return Err(anyhow!(
                "stop loop playback before editing take processor alignment"
            ));
        }
        let content = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        let dry_reference = content
            .audio
            .iter()
            .find(|channel| channel.mode == BackendChannelMode::Dry)
            .map(|channel| channel.capture_alignment_frames)
            .or_else(|| {
                content
                    .midi
                    .iter()
                    .find(|channel| channel.mode == BackendChannelMode::Dry)
                    .map(|channel| channel.capture_alignment_frames)
            })
            .ok_or_else(|| anyhow!("take has no dry channel"))?;
        let wet_reference = content
            .audio
            .iter()
            .find(|channel| channel.mode == BackendChannelMode::Wet)
            .map(|channel| channel.capture_alignment_frames)
            .or_else(|| {
                content
                    .midi
                    .iter()
                    .find(|channel| channel.mode == BackendChannelMode::Wet)
                    .map(|channel| channel.capture_alignment_frames)
            })
            .ok_or_else(|| anyhow!("take has no wet channel"))?;
        let current = i64::from(wet_reference) - i64::from(dry_reference);
        let delta = i64::from(processor_alignment_frames) - current;
        let delta = i32::try_from(delta)
            .map_err(|_| anyhow!("take processor alignment adjustment overflowed"))?;
        let audio_candidates = content
            .audio
            .iter()
            .enumerate()
            .filter(|(_, channel)| channel.mode == BackendChannelMode::Wet)
            .map(|(index, channel)| {
                let candidate = channel
                    .capture_alignment_frames
                    .checked_add(delta)
                    .ok_or_else(|| anyhow!("take processor alignment adjustment overflowed"))?;
                shoop_latency::RecordingOffset::new(candidate)?;
                validate_take_alignment_window(
                    candidate,
                    channel.start_offset,
                    channel.samples.len() as u64,
                    content.length,
                    "audio",
                    index,
                )?;
                Ok((index, candidate))
            })
            .collect::<Result<Vec<_>>>()?;
        let midi_candidates = content
            .midi
            .iter()
            .enumerate()
            .filter(|(_, channel)| channel.mode == BackendChannelMode::Wet)
            .map(|(index, channel)| {
                let candidate = channel
                    .capture_alignment_frames
                    .checked_add(delta)
                    .ok_or_else(|| anyhow!("take processor alignment adjustment overflowed"))?;
                shoop_latency::RecordingOffset::new(candidate)?;
                validate_take_alignment_window(
                    candidate,
                    channel.start_offset,
                    u64::from(channel.length),
                    content.length,
                    "MIDI",
                    index,
                )?;
                Ok((index, candidate))
            })
            .collect::<Result<Vec<_>>>()?;
        let content = self
            .loop_content
            .get_mut(&loop_id)
            .expect("fake loop was checked above");
        for (index, candidate) in audio_candidates {
            content.audio[index].capture_alignment_frames = candidate;
        }
        for (index, candidate) in midi_candidates {
            content.midi[index].capture_alignment_frames = candidate;
        }
        if let Some(state) = self.loops.get_mut(&loop_id) {
            state.processor_alignment_frames = Some(processor_alignment_frames);
        }
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
            BackendTrackFxControl::OxiSynth(control) => {
                let Some(TrackProcessorEditorState::OxiSynth(editor)) = fx.editor.as_mut() else {
                    return Err(anyhow!("OxiSynth editor state is unavailable"));
                };
                match control {
                    OxiSynthControl::SelectPreset(id) => editor.selected_preset_id = id,
                    OxiSynthControl::SetReverbSend(value) => editor.reverb_send = value,
                    OxiSynthControl::SetChorusSend(value) => editor.chorus_send = value,
                    OxiSynthControl::AssignMidiCc(assignment) => {
                        let mut assignments = editor.midi_cc_assignments.to_vec();
                        assignments.retain(|existing| {
                            existing.parameter != assignment.parameter
                                && (existing.channel, existing.controller)
                                    != (assignment.channel, assignment.controller)
                        });
                        assignments.push(assignment);
                        editor.midi_cc_assignments = assignments.into();
                    }
                    OxiSynthControl::RemoveMidiCc(parameter) => {
                        editor.midi_cc_assignments = editor
                            .midi_cc_assignments
                            .iter()
                            .copied()
                            .filter(|assignment| assignment.parameter != parameter)
                            .collect::<Vec<_>>()
                            .into();
                    }
                    OxiSynthControl::ClearMidiCcAssignments => {
                        editor.midi_cc_assignments = Arc::from([]);
                    }
                    OxiSynthControl::Panic => {}
                }
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
            let track = self
                .tracks
                .values()
                .find(|track| track.loops.contains(&request.loop_id))
                .ok_or_else(|| anyhow!("loop has no owning track"))?;
            let values = prepared_backend_latency(
                &track.state.latency,
                track.state.topology.has_wet_channels(),
            )?;
            let has_wet = !matches!(track.state.topology, BackendTrackTopology::Direct { .. });
            if values.recording_offset().frames() != 0
                || (has_wet && values.wet_recording_offset().frames() != 0)
            {
                return Err(anyhow!(
                    "grab with a nonzero recording offset is unsupported; record the loop instead"
                ));
            }
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

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        self.require_loop(loop_id)?;
        let channels = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing fake loop content"))?
            .audio
            .iter()
            .map(|channel| BackendAudioChannelData {
                samples: Arc::from(channel.samples.clone()),
                start_offset: channel.start_offset,
                capture_alignment_frames: channel.capture_alignment_frames,
                preplay: channel.preplay,
            })
            .collect();
        Ok(Some(BackendAudioData { channels }))
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        self.require_loop(loop_id)?;
        let channels = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing fake loop content"))?
            .midi
            .iter()
            .map(|channel| BackendMidiChannelData {
                content_revision: 0,
                mode: channel.mode,
                length: channel.length,
                events: channel.events.clone(),
                start_offset: channel.start_offset,
                capture_alignment_frames: channel.capture_alignment_frames,
                preplay: channel.preplay,
            })
            .collect();
        Ok(Some(BackendMidiData { channels }))
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
        if matches!(
            mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            let latency = self
                .tracks
                .values()
                .find(|track| track.loops.contains(&loop_id))
                .map(|track| &track.state.latency);
            let alignment = latency
                .map(|latency| {
                    latency.effective_offset_frames.ok_or_else(|| {
                        anyhow!("recording offset is unavailable; enter a manual value")
                    })
                })
                .transpose()?
                .unwrap_or(0);
            let has_wet = self.loop_content.get(&loop_id).is_some_and(|content| {
                content
                    .audio
                    .iter()
                    .any(|channel| channel.mode == BackendChannelMode::Wet)
                    || content
                        .midi
                        .iter()
                        .any(|channel| channel.mode == BackendChannelMode::Wet)
            });
            let wet_alignment = if has_wet {
                let processor_advance = latency
                    .map(|latency| {
                        latency.effective_processor_advance_frames.ok_or_else(|| {
                            anyhow!(
                                "processor latency is unavailable; enter a non-negative manual value"
                            )
                        })
                    })
                    .transpose()?
                    .unwrap_or(0);
                shoop_latency::wet_recording_offset(
                    shoop_latency::RecordingOffset::new(alignment)?,
                    shoop_latency::ProcessorRenderAdvance::new(processor_advance)?,
                )?
                .frames()
            } else {
                alignment
            };
            if mode == BackendLoopMode::Replacing {
                if alignment != 0 || (has_wet && wet_alignment != 0) {
                    return Err(anyhow!(
                        "replacement with a nonzero recording offset is unsupported; record a new take instead"
                    ));
                }
                let alignment_matches = self.loop_content.get(&loop_id).is_none_or(|content| {
                    content
                        .audio
                        .iter()
                        .map(|channel| (channel.mode, channel.capture_alignment_frames))
                        .chain(
                            content
                                .midi
                                .iter()
                                .map(|channel| (channel.mode, channel.capture_alignment_frames)),
                        )
                        .all(|(mode, actual)| {
                            let expected = if mode == BackendChannelMode::Wet {
                                wet_alignment
                            } else {
                                alignment
                            };
                            actual == expected
                        })
                });
                if !alignment_matches {
                    return Err(anyhow!(
                        "replacement offset differs from the take; match the take alignment first"
                    ));
                }
            }
            if let Some(content) = self.loop_content.get_mut(&loop_id) {
                for channel in &mut content.audio {
                    if mode == BackendLoopMode::RecordingDryIntoWet {
                        if channel.mode != BackendChannelMode::Dry {
                            channel.capture_alignment_frames = 0;
                        }
                    } else {
                        channel.capture_alignment_frames =
                            if channel.mode == BackendChannelMode::Wet {
                                wet_alignment
                            } else {
                                alignment
                            };
                    }
                }
                for channel in &mut content.midi {
                    if mode == BackendLoopMode::RecordingDryIntoWet {
                        if channel.mode != BackendChannelMode::Dry {
                            channel.capture_alignment_frames = 0;
                        }
                    } else {
                        channel.capture_alignment_frames =
                            if channel.mode == BackendChannelMode::Wet {
                                wet_alignment
                            } else {
                                alignment
                            };
                    }
                }
            }
        }
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.mode = mode;
        state.capture_alignment_frames = self
            .loop_content
            .get(&loop_id)
            .and_then(|content| {
                content
                    .audio
                    .first()
                    .map(|channel| channel.capture_alignment_frames)
                    .or_else(|| {
                        content
                            .midi
                            .first()
                            .map(|channel| channel.capture_alignment_frames)
                    })
            })
            .unwrap_or(0);
        state.processor_alignment_frames = self.loop_content.get(&loop_id).and_then(|content| {
            processor_alignment_from_values(
                content
                    .audio
                    .iter()
                    .map(|channel| (channel.mode, channel.capture_alignment_frames))
                    .chain(
                        content
                            .midi
                            .iter()
                            .map(|channel| (channel.mode, channel.capture_alignment_frames)),
                    ),
            )
        });
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
                channel.capture_alignment_frames = 0;
            }
            for channel in &mut content.midi {
                channel.length = 0;
                channel.start_state.clear();
                channel.events.clear();
                channel.capture_alignment_frames = 0;
            }
        }
        self.operations.push(FakeOperation::Clear(loop_id));
        Ok(())
    }

    fn replace_loop_content(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        if update.audio.is_empty() && update.midi.is_empty() {
            return Err(anyhow!("loop content update is empty"));
        }
        if let Some(message) = self.fail_next_loop_content_replace.take() {
            return Err(anyhow!(message));
        }
        let state = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        if matches!(
            state.mode,
            BackendLoopMode::Recording
                | BackendLoopMode::Replacing
                | BackendLoopMode::RecordingDryIntoWet
        ) {
            return Err(anyhow!("loop content is changing"));
        }
        let content = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing fake loop content"))?;
        let audio_indices = update
            .audio
            .iter()
            .map(|item| {
                content
                    .audio
                    .get(item.channel)
                    .map(|_| item.channel)
                    .ok_or_else(|| anyhow!("unknown audio channel {}", item.channel))
            })
            .collect::<Result<Vec<_>>>()?;
        let midi_indices = update
            .midi
            .iter()
            .map(|item| {
                content
                    .midi
                    .get(item.channel)
                    .map(|_| item.channel)
                    .ok_or_else(|| anyhow!("unknown MIDI channel {}", item.channel))
            })
            .collect::<Result<Vec<_>>>()?;
        if audio_indices.iter().collect::<BTreeSet<_>>().len() != audio_indices.len()
            || midi_indices.iter().collect::<BTreeSet<_>>().len() != midi_indices.len()
        {
            return Err(anyhow!("loop content update contains a duplicate channel"));
        }
        for alignment in update
            .audio
            .iter()
            .filter_map(|item| item.capture_alignment_frames)
            .chain(
                update
                    .midi
                    .iter()
                    .filter_map(|item| item.capture_alignment_frames),
            )
        {
            shoop_latency::RecordingOffset::new(alignment)?;
        }
        if update
            .midi
            .iter()
            .flat_map(|item| &item.events)
            .any(|event| shoop_engine::MidiStorageElem::new(event.time, &event.data).is_none())
        {
            return Err(anyhow!("invalid MIDI event"));
        }
        let logical_length = update.length.unwrap_or(content.length);
        for (index, channel) in content.audio.iter().enumerate() {
            let replacement = update.audio.iter().find(|item| item.channel == index);
            let alignment = replacement
                .and_then(|item| item.capture_alignment_frames)
                .unwrap_or(channel.capture_alignment_frames);
            if alignment != 0 {
                validate_take_alignment_window(
                    alignment,
                    replacement
                        .and_then(|item| item.start_offset)
                        .unwrap_or(channel.start_offset),
                    replacement.map_or(channel.samples.len() as u64, |item| {
                        item.samples.len() as u64
                    }),
                    logical_length,
                    "audio",
                    index,
                )?;
            }
        }
        for (index, channel) in content.midi.iter().enumerate() {
            let replacement = update.midi.iter().find(|item| item.channel == index);
            let alignment = replacement
                .and_then(|item| item.capture_alignment_frames)
                .unwrap_or(channel.capture_alignment_frames);
            if alignment != 0 {
                validate_take_alignment_window(
                    alignment,
                    replacement
                        .and_then(|item| item.start_offset)
                        .unwrap_or(channel.start_offset),
                    replacement.map_or(u64::from(channel.length), |item| u64::from(item.length)),
                    logical_length,
                    "MIDI",
                    index,
                )?;
            }
        }

        let content = self
            .loop_content
            .get_mut(&loop_id)
            .expect("loop content was validated");
        for item in &update.audio {
            let channel = &mut content.audio[item.channel];
            channel.samples.clone_from(&item.samples);
            if let Some(alignment) = item.capture_alignment_frames {
                channel.capture_alignment_frames = alignment;
            }
            if let Some(offset) = item.start_offset {
                channel.start_offset = offset;
            }
            if let Some(preplay) = item.preplay {
                channel.preplay = preplay;
            }
        }
        for item in &update.midi {
            let channel = &mut content.midi[item.channel];
            channel.length = item.length;
            channel.start_state.clone_from(&item.start_state);
            channel.events.clone_from(&item.events);
            if let Some(alignment) = item.capture_alignment_frames {
                channel.capture_alignment_frames = alignment;
            }
            if let Some(offset) = item.start_offset {
                channel.start_offset = offset;
            }
            if let Some(preplay) = item.preplay {
                channel.preplay = preplay;
            }
        }
        if let Some(length) = update.length {
            content.length = length;
        }
        let state = self.loops.get_mut(&loop_id).expect("loop was validated");
        state.mode = BackendLoopMode::Stopped;
        state.capture_alignment_frames = content
            .audio
            .first()
            .map(|channel| channel.capture_alignment_frames)
            .or_else(|| {
                content
                    .midi
                    .first()
                    .map(|channel| channel.capture_alignment_frames)
            })
            .unwrap_or(0);
        state.processor_alignment_frames = processor_alignment_from_values(
            content
                .audio
                .iter()
                .map(|channel| (channel.mode, channel.capture_alignment_frames))
                .chain(
                    content
                        .midi
                        .iter()
                        .map(|channel| (channel.mode, channel.capture_alignment_frames)),
                ),
        );
        state.next_mode = None;
        state.next_transition_delay = None;
        if let Some(length) = update.length {
            state.length = length;
            if state.position >= length {
                state.position = if length == 0 {
                    0
                } else {
                    state.position % length
                };
            }
        }
        self.operations
            .push(FakeOperation::ReplaceLoopContent(loop_id, update.clone()));
        Ok(())
    }

    fn replace_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        if self.pending_loop_content_replacements > 0 {
            self.pending_loop_content_replacements -= 1;
            return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                key: 1,
                kind: BackendOperationKind::LoopContentReplacement,
                completed: 0,
                total: Some(1),
            }));
        }
        self.replace_loop_content(loop_id, update)
            .map(BackendAsyncResult::Ready)
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        self.set_loop_timing(loop_id, None, None, Some(length))
    }

    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        self.require_loop(loop_id)?;
        let state = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        if length.is_some()
            && (matches!(
                state.mode,
                BackendLoopMode::Recording
                    | BackendLoopMode::Replacing
                    | BackendLoopMode::RecordingDryIntoWet
            ) || state.next_mode.is_some_and(|mode| {
                matches!(
                    mode,
                    BackendLoopMode::Recording
                        | BackendLoopMode::Replacing
                        | BackendLoopMode::RecordingDryIntoWet
                )
            }))
        {
            return Err(anyhow!(
                "cannot change loop length while a recording operation is armed"
            ));
        }
        let content = self
            .loop_content
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing fake loop content"))?;
        let logical_length = length.unwrap_or(content.length);
        for (index, channel) in content.audio.iter().enumerate() {
            if channel.capture_alignment_frames != 0 {
                validate_take_alignment_window(
                    channel.capture_alignment_frames,
                    start_offset.unwrap_or(channel.start_offset),
                    channel.samples.len() as u64,
                    logical_length,
                    "audio",
                    index,
                )?;
            }
        }
        for (index, channel) in content.midi.iter().enumerate() {
            if channel.capture_alignment_frames != 0 {
                validate_take_alignment_window(
                    channel.capture_alignment_frames,
                    start_offset.unwrap_or(channel.start_offset),
                    u64::from(channel.length),
                    logical_length,
                    "MIDI",
                    index,
                )?;
            }
        }
        let content = self
            .loop_content
            .get_mut(&loop_id)
            .expect("fake loop content was checked above");
        for channel in &mut content.audio {
            if let Some(offset) = start_offset {
                channel.start_offset = offset;
            }
            if let Some(samples) = preplay {
                channel.preplay = samples;
            }
        }
        for channel in &mut content.midi {
            if let Some(offset) = start_offset {
                channel.start_offset = offset;
            }
            if let Some(samples) = preplay {
                channel.preplay = samples;
            }
        }
        if let Some(length) = length {
            content.length = length;
            let state = self
                .loops
                .get_mut(&loop_id)
                .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
            state.length = length;
            if state.position >= length {
                state.position = if length == 0 {
                    0
                } else {
                    state.position % length
                };
            }
            self.operations
                .push(FakeOperation::SetLoopLength(loop_id, length));
        }
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
                    oxisynth_midi_cc_assignments: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let global_descriptor = connections
            .application_ports
            .values()
            .find(|port| port.owner == BackendPortOwner::GlobalFxControl)
            .cloned()
            .ok_or_else(|| anyhow!("missing fake global FX control port"))?;
        let global_ports = vec![BackendSessionPort {
            source_id: global_descriptor.id.raw(),
            external_connections: connections
                .confirmed_links
                .iter()
                .filter(|link| link.application_port_id == global_descriptor.id)
                .map(|link| link.host_port_id.clone())
                .collect(),
            descriptor: global_descriptor,
        }];
        let buses = self
            .mixer
            .buses
            .values()
            .map(|bus| BackendSessionBus {
                source_id: bus.id.raw(),
                name: bus.name.clone(),
                channels: bus
                    .channels
                    .iter()
                    .map(|channel| BackendSessionBusChannel {
                        source_id: channel.id.raw(),
                        label: channel.label.clone(),
                        output_port: BackendSessionPort {
                            source_id: channel.output_port_id.raw(),
                            descriptor: connections.application_ports[&channel.output_port_id]
                                .clone(),
                            external_connections: connections
                                .confirmed_links
                                .iter()
                                .filter(|link| link.application_port_id == channel.output_port_id)
                                .map(|link| link.host_port_id.clone())
                                .collect(),
                        },
                    })
                    .collect(),
            })
            .collect();
        let mixer_routes = self
            .mixer
            .confirmed_links
            .iter()
            .map(|link| BackendSessionMixerRoute {
                source_port_id: link.source_port_id.raw(),
                destination_channel_id: link.destination_channel_id.raw(),
            })
            .collect();
        Ok(BackendSessionData {
            sample_rate: self.status.sample_rate,
            tracks,
            buses,
            mixer_routes,
            global_ports,
            use_legacy_browser_default_routes: false,
        })
    }

    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        if self.pending_session_captures > 0 {
            self.pending_session_captures -= 1;
            return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                key: 1,
                kind: BackendOperationKind::SessionCapture,
                completed: 0,
                total: Some(1),
            }));
        }
        self.capture_session().map(BackendAsyncResult::Ready)
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
        let source_global = session
            .global_ports
            .first()
            .ok_or_else(|| anyhow!("session has no global FX control port"))?;
        if session.global_ports.len() != 1
            || source_global.descriptor.owner != BackendPortOwner::GlobalFxControl
        {
            return Err(anyhow!("session global FX control port is invalid"));
        }
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
        let staged_global = staged
            .connections
            .with_state(|state| {
                state
                    .ports
                    .values()
                    .find(|port| port.owner == BackendPortOwner::GlobalFxControl)
                    .map(|port| port.id)
            })
            .ok_or_else(|| anyhow!("staged backend has no global FX control port"))?;
        replacement
            .global_ports
            .insert(source_global.source_id, staged_global);
        if let Some(source_bus) = session.buses.first() {
            let staged_bus = staged
                .mixer
                .buses
                .values()
                .next()
                .cloned()
                .ok_or_else(|| anyhow!("staged backend has no Master bus"))?;
            if session.buses.len() != 1
                || source_bus.name != "Master"
                || source_bus.channels.len() != staged_bus.channels.len()
            {
                return Err(anyhow!("session Master bus shape is invalid"));
            }
            replacement
                .buses
                .insert(source_bus.source_id, staged_bus.id);
            for (source_channel, staged_channel) in
                source_bus.channels.iter().zip(&staged_bus.channels)
            {
                replacement
                    .bus_channels
                    .insert(source_channel.source_id, staged_channel.id);
                replacement
                    .bus_output_ports
                    .insert(source_channel.source_id, staged_channel.output_port_id);
                replacement.ports.insert(
                    source_channel.output_port.source_id,
                    staged_channel.output_port_id,
                );
                for external in &source_channel.output_port.external_connections {
                    staged.set_port_connected(staged_channel.output_port_id, external, true)?;
                }
            }
        }
        for external in &source_global.external_connections {
            if let Err(error) = staged.set_port_connected(staged_global, external, true) {
                staged.connections.with_state(|state| {
                    state.failures.push(BackendConnectionFailure {
                        port_id: staged_global,
                        external_port: external.clone(),
                        desired_connected: true,
                        message: format!("could not restore external endpoint {external}: {error}"),
                    });
                    state.revision = state.revision.wrapping_add(1);
                });
            }
        }
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
            {
                let target_latency = &mut staged
                    .tracks
                    .get_mut(&created.track_id)
                    .expect("created track exists")
                    .state
                    .latency;
                target_latency.automatic_offset_frames =
                    source_track.state.latency.automatic_offset_frames;
                target_latency.automatic_processor_advance_frames = source_track
                    .state
                    .latency
                    .automatic_processor_advance_frames;
            }
            if let Err(error) = staged.set_track_latency(
                created.track_id,
                source_track.state.latency.adjustment,
                source_track.state.latency.processor_adjustment,
                source_track.state.latency.processor_manual_frames,
            ) {
                if source_track.state.latency.effective_offset_frames.is_some()
                    && source_track
                        .state
                        .latency
                        .effective_processor_advance_frames
                        .is_some()
                {
                    return Err(error);
                }
                staged
                    .tracks
                    .get_mut(&created.track_id)
                    .expect("created track exists")
                    .state
                    .latency
                    .clone_from(&source_track.state.latency);
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
                    if let Err(error) = staged.set_port_connected(created_port.id, external, true) {
                        staged.connections.with_state(|state| {
                            state.failures.push(BackendConnectionFailure {
                                port_id: created_port.id,
                                external_port: external.clone(),
                                desired_connected: true,
                                message: format!(
                                    "could not restore external endpoint {external}: {error}"
                                ),
                            });
                            state.revision = state.revision.wrapping_add(1);
                        });
                    }
                }
            }
            replacement
                .tracks
                .insert(source_track.source_id, created.clone());
        }
        for route in &session.mixer_routes {
            let source_port_id = replacement
                .ports
                .get(&route.source_port_id)
                .copied()
                .ok_or_else(|| anyhow!("session mixer route has a stale source"))?;
            let destination_channel_id = replacement
                .bus_channels
                .get(&route.destination_channel_id)
                .copied()
                .ok_or_else(|| anyhow!("session mixer route has a stale destination"))?;
            staged.set_mixer_route(source_port_id, destination_channel_id, true)?;
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

    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        let source_is_valid = self.connections.with_state(|state| {
            state.ports.get(&source_port_id).is_some_and(|port| {
                port.owner == BackendPortOwner::Track
                    && port.data_type == BackendPortDataType::Audio
                    && port.direction == BackendPortDirection::Output
                    && port.role == BackendPortRole::AudioOutput
            })
        });
        if !source_is_valid {
            return Err(anyhow!("mixer source is not a track audio output"));
        }
        if !self.mixer.buses.values().any(|bus| {
            bus.channels
                .iter()
                .any(|channel| channel.id == destination_channel_id)
        }) {
            return Err(anyhow!("unknown Master bus channel"));
        }
        let link = BackendMixerLink {
            source_port_id,
            destination_channel_id,
        };
        if connected {
            self.mixer.confirmed_links.insert(link);
        } else {
            self.mixer.confirmed_links.remove(&link);
        }
        self.mixer.revision = self.mixer.revision.wrapping_add(1);
        self.operations
            .push(FakeOperation::SetMixerRoute(link, connected));
        Ok(())
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
            composites: self.composites.clone(),
            connections: self.connection_snapshot(),
            mixer: self.mixer.clone(),
            mutation_failures: Vec::new(),
        })
    }

    fn wait_idle(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load_direct_loop(backend: &mut EngineBackend, loop_id: BackendLoopId, value: f32) {
        let engine_loop = backend.engine_loop_index(loop_id).unwrap();
        backend
            .session
            .loop_mut(engine_loop)
            .unwrap()
            .audio_channel_mut(0)
            .unwrap()
            .load_data(&[value; 4]);
        backend.session.loop_mut(engine_loop).unwrap().set_length(4);
        backend
            .session
            .set_loop_mode(engine_loop, LoopMode::Playing)
            .unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_master_bus_is_disconnected_and_sums_explicit_fanout_routes() {
        let mut backend = EngineBackend::new_dummy_runtime(48_000, 4).unwrap();
        let initial = backend.poll().unwrap();
        let master = initial.mixer.buses.values().next().unwrap();
        assert_eq!(master.name, "Master");
        assert_eq!(
            master
                .channels
                .iter()
                .map(|channel| channel.label.as_str())
                .collect::<Vec<_>>(),
            ["Left", "Right"]
        );
        assert!(initial.mixer.confirmed_links.is_empty());
        assert!(backend
            .set_mixer_route(backend.global_fx_port, master.channels[0].id, true)
            .is_err());
        assert!(initial.connections.confirmed_links.iter().all(|link| {
            !master
                .channels
                .iter()
                .any(|channel| channel.output_port_id == link.application_port_id)
        }));

        let first = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "first".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let second = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "second".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        load_direct_loop(&mut backend, first.loops[0], 0.25);
        load_direct_loop(&mut backend, second.loops[0], 0.5);
        let first_output = first
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioOutput)
            .unwrap()
            .id;
        let second_output = second
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioOutput)
            .unwrap()
            .id;
        let left = backend.master_bus.channels[0].id;
        let right = backend.master_bus.channels[1].id;
        backend.set_mixer_route(first_output, left, true).unwrap();
        backend.set_mixer_route(first_output, right, true).unwrap();
        backend.set_mixer_route(second_output, left, true).unwrap();

        for channel in &backend.master_bus.channels {
            backend
                .session
                .port_mut(channel.output)
                .unwrap()
                .as_dummy_mut()
                .unwrap()
                .request_data(4);
        }
        let first_output_index = backend.connection_ports[&first_output].engine_port_index;
        backend
            .session
            .port_mut(first_output_index)
            .unwrap()
            .as_dummy_mut()
            .unwrap()
            .request_data(4);
        backend.advance_frames(4);

        let left_output = backend.master_bus.channels[0].output;
        let right_output = backend.master_bus.channels[1].output;
        assert_eq!(
            backend
                .session
                .port_mut(left_output)
                .unwrap()
                .as_dummy_mut()
                .unwrap()
                .dequeue_data(4)
                .unwrap(),
            vec![0.75; 4]
        );
        assert_eq!(
            backend
                .session
                .port_mut(right_output)
                .unwrap()
                .as_dummy_mut()
                .unwrap()
                .dequeue_data(4)
                .unwrap(),
            vec![0.25; 4]
        );
        assert_eq!(
            backend
                .session
                .port_mut(first_output_index)
                .unwrap()
                .as_dummy_mut()
                .unwrap()
                .dequeue_data(4)
                .unwrap(),
            vec![0.25; 4]
        );

        let master_left_output = backend.master_bus.channels[0].output_port_id;
        backend
            .set_port_connected(master_left_output, "system:playback_1", true)
            .unwrap();
        let captured = backend.capture_session().unwrap();
        assert_eq!(captured.buses.len(), 1);
        assert_eq!(captured.mixer_routes.len(), 3);
        assert_eq!(
            captured.buses[0].channels[0]
                .output_port
                .external_connections,
            ["system:playback_1"]
        );
        let source_first_output = captured.tracks[0]
            .ports
            .iter()
            .find(|port| port.descriptor.role == BackendPortRole::AudioOutput)
            .unwrap()
            .source_id;
        let source_left = captured.buses[0].channels[0].source_id;
        let replacement = backend.replace_session(&captured).unwrap();
        let restored_first_output = replacement.ports[&source_first_output];
        let restored_left = replacement.bus_channels[&source_left];
        let restored = backend.poll().unwrap();
        assert_eq!(restored.mixer.confirmed_links.len(), 3);
        assert!(restored.connections.confirmed_links.iter().any(|link| {
            link.application_port_id
                == restored.mixer.buses.values().next().unwrap().channels[0].output_port_id
                && link.host_port_id == "system:playback_1"
        }));

        backend
            .set_mixer_route(restored_first_output, restored_left, false)
            .unwrap();
        assert!(!backend
            .poll()
            .unwrap()
            .mixer
            .confirmed_links
            .contains(&BackendMixerLink {
                source_port_id: restored_first_output,
                destination_channel_id: restored_left,
            }));
        let restored_first_track = replacement.tracks[&captured.tracks[0].source_id].track_id;
        backend.remove_track(restored_first_track).unwrap();
        assert!(backend
            .poll()
            .unwrap()
            .mixer
            .confirmed_links
            .iter()
            .all(|link| { link.source_port_id != restored_first_output }));
    }

    fn backend_composite_lifecycle_contract(backend: &mut dyn Backend) {
        let sync = backend.create_loop().unwrap();
        let child = backend.create_loop().unwrap();
        backend.set_loop_length(sync, 1).unwrap();
        backend.set_loop_length(child, 4).unwrap();
        let composite = backend.create_composite_loop().unwrap();
        let config = BackendCompositeConfig {
            kind: BackendCompositeKind::Regular,
            sync_source: sync,
            timelines: vec![vec![vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Loop(child),
                delay: 0,
                n_cycles: None,
                mode: None,
            }]]],
        };
        backend
            .configure_composite_loop(composite, &config)
            .unwrap();
        backend
            .transition_composite_loop(composite, BackendLoopMode::Playing, None, None)
            .unwrap();
        let configured = backend.poll().unwrap().composites[&composite].clone();
        assert_eq!(configured.mode, BackendLoopMode::Playing);
        assert_eq!(configured.length, 4);

        let stale = BackendCompositeConfig {
            timelines: vec![vec![vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Loop(BackendLoopId::from_raw(u64::MAX)),
                delay: 0,
                n_cycles: None,
                mode: None,
            }]]],
            ..config
        };
        assert!(backend.configure_composite_loop(composite, &stale).is_err());
        assert_eq!(backend.poll().unwrap().composites[&composite], configured);

        backend.remove_composite_loop(composite).unwrap();
        assert!(!backend.poll().unwrap().composites.contains_key(&composite));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_satisfies_shared_composite_lifecycle_contract() {
        backend_composite_lifecycle_contract(&mut FakeBackend::default());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_satisfies_shared_composite_lifecycle_contract() {
        let mut backend = EngineBackend::new_dummy(1_000, 1).unwrap();
        backend_composite_lifecycle_contract(&mut backend);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_composite_contract_is_independent_and_transactional() {
        let mut backend = EngineBackend::new_dummy(1_000, 1).unwrap();
        let sync = backend.create_loop().unwrap();
        let children = [
            backend.create_loop().unwrap(),
            backend.create_loop().unwrap(),
            backend.create_loop().unwrap(),
        ];
        backend.set_loop_length(sync, 1).unwrap();
        for child in children {
            backend.set_loop_length(child, 4).unwrap();
        }
        backend.apply_graph_changes().unwrap();
        let empty = backend.create_composite_loop().unwrap();
        backend
            .configure_composite_loop(
                empty,
                &BackendCompositeConfig {
                    kind: BackendCompositeKind::Regular,
                    sync_source: sync,
                    timelines: Vec::new(),
                },
            )
            .unwrap();
        backend
            .transition_composite_loop(empty, BackendLoopMode::Playing, None, None)
            .unwrap();
        assert_eq!(
            backend.poll().unwrap().composites[&empty].mode,
            BackendLoopMode::Stopped
        );

        let composite = backend.create_composite_loop().unwrap();
        let config = BackendCompositeConfig {
            kind: BackendCompositeKind::Regular,
            sync_source: sync,
            timelines: vec![children
                .into_iter()
                .map(|target| {
                    vec![BackendCompositeEntry {
                        target: BackendCompositeTarget::Loop(target),
                        delay: 0,
                        n_cycles: None,
                        mode: None,
                    }]
                })
                .collect()],
        };
        backend
            .configure_composite_loop(composite, &config)
            .unwrap();
        backend
            .transition_loop(sync, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_composite_loop(composite, BackendLoopMode::Playing, None, None)
            .unwrap();
        let started = backend.poll().unwrap();
        assert_eq!(
            started.composites[&composite].mode,
            BackendLoopMode::Playing
        );
        assert_eq!(
            started.composites[&composite].active_children[0].target,
            BackendCompositeTarget::Loop(children[0])
        );

        backend.advance_frames(4);
        let advanced = backend.poll().unwrap();
        assert_eq!(advanced.composites[&composite].iteration, 4);
        assert_eq!(
            advanced.composites[&composite].active_children[0].target,
            BackendCompositeTarget::Loop(children[1])
        );

        let aligned = backend.create_composite_loop().unwrap();
        backend.configure_composite_loop(aligned, &config).unwrap();
        backend
            .transition_composite_loop(aligned, BackendLoopMode::Playing, None, Some(5))
            .unwrap();
        let aligned_state = backend.poll().unwrap().composites[&aligned].clone();
        assert_eq!(aligned_state.iteration, 5);
        assert_eq!(aligned_state.position, 5);
        assert_eq!(
            aligned_state.active_children[0].target,
            BackendCompositeTarget::Loop(children[1])
        );

        let stopped = backend.create_composite_loop().unwrap();
        backend.configure_composite_loop(stopped, &config).unwrap();
        backend
            .transition_composite_loop(stopped, BackendLoopMode::Playing, Some(2), None)
            .unwrap();
        let pending = backend.poll().unwrap().composites[&stopped].clone();
        assert_eq!(pending.mode, BackendLoopMode::Stopped);
        assert_eq!(pending.next_mode, Some(BackendLoopMode::Playing));
        assert_eq!(pending.next_transition_delay, Some(2));
        backend
            .transition_loop(children[2], BackendLoopMode::Playing, None)
            .unwrap();
        backend.advance_frames(1);
        assert_eq!(
            backend.poll().unwrap().composites[&stopped].mode,
            BackendLoopMode::Stopped
        );

        let reconfigured = backend.create_composite_loop().unwrap();
        backend
            .configure_composite_loop(reconfigured, &config)
            .unwrap();
        let parallel = BackendCompositeConfig {
            kind: BackendCompositeKind::Regular,
            sync_source: sync,
            timelines: vec![vec![
                children[..2]
                    .iter()
                    .map(|target| BackendCompositeEntry {
                        target: BackendCompositeTarget::Loop(*target),
                        delay: 0,
                        n_cycles: None,
                        mode: None,
                    })
                    .collect(),
                vec![BackendCompositeEntry {
                    target: BackendCompositeTarget::Loop(children[2]),
                    delay: 0,
                    n_cycles: None,
                    mode: None,
                }],
            ]],
        };
        backend
            .configure_composite_loop(reconfigured, &parallel)
            .unwrap();
        backend
            .transition_composite_loop(reconfigured, BackendLoopMode::Playing, None, None)
            .unwrap();
        let parallel_state = backend.poll().unwrap().composites[&reconfigured].clone();
        assert_eq!(parallel_state.length, 8);
        assert_eq!(parallel_state.active_children.len(), 2);
        backend.remove_composite_loop(reconfigured).unwrap();
        assert!(!backend
            .poll()
            .unwrap()
            .composites
            .contains_key(&reconfigured));
        assert!(backend
            .transition_composite_loop(reconfigured, BackendLoopMode::Playing, None, None,)
            .is_err());

        let stale = BackendCompositeConfig {
            kind: BackendCompositeKind::Regular,
            sync_source: sync,
            timelines: vec![vec![vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Loop(BackendLoopId::from_raw(u64::MAX)),
                delay: 0,
                n_cycles: None,
                mode: None,
            }]]],
        };
        let before_stale = backend.poll().unwrap().composites[&composite].clone();
        assert!(backend.configure_composite_loop(composite, &stale).is_err());
        assert_eq!(backend.poll().unwrap().composites[&composite], before_stale);

        let invalid = BackendCompositeConfig {
            kind: BackendCompositeKind::Regular,
            sync_source: sync,
            timelines: vec![vec![vec![BackendCompositeEntry {
                target: BackendCompositeTarget::Composite(composite),
                delay: 0,
                n_cycles: Some(1),
                mode: None,
            }]]],
        };
        assert!(backend
            .configure_composite_loop(composite, &invalid)
            .unwrap_err()
            .to_string()
            .contains("cycle"));
        assert_eq!(backend.poll().unwrap().composites[&composite], before_stale);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn default_input_capture_is_thirty_seconds() {
        assert_eq!(INPUT_CAPTURE_CAPACITY_SECONDS, 30);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_publishes_empty_and_future_processor_catalogs() {
        let mut backend = FakeBackend::default();
        assert!(backend.track_processor_catalog().unwrap().is_empty());
        let descriptor = shoop_app_api::TrackProcessorDescriptor {
            id: shoop_app_api::TrackProcessorTypeId::new("future_browser_fx"),
            label: "Future browser FX".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: shoop_app_api::TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(8),
                min_wet_audio_channels: None,
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

    #[shoop_wasm_test_support::shoop_test]
    fn fake_session_replacement_restores_latency_and_future_alignment() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "restored-latency".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(5),
                BackendProcessorLatencyAdjustment::ManualOverride,
                7,
            )
            .unwrap();
        let captured = backend.capture_session().unwrap();
        let replacement = backend.replace_session(&captured).unwrap();
        let restored_track = replacement.tracks[&created.track_id.raw()].track_id;
        let restored_loop = replacement.loops[&created.loops[0].raw()];
        let latency = &backend.poll().unwrap().tracks[&restored_track].latency;
        assert_eq!(latency.effective_offset_frames, Some(5));
        assert_eq!(latency.effective_processor_advance_frames, Some(7));

        backend
            .transition_loop(restored_loop, BackendLoopMode::Recording, None)
            .unwrap();
        let content = &backend.loop_content[&restored_loop];
        assert_eq!(
            content
                .audio
                .iter()
                .map(|channel| channel.capture_alignment_frames)
                .collect::<Vec<_>>(),
            [5, 12]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_clear_resets_channel_alignment_with_media() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "clear-alignment".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![0.0; 6],
                        start_offset: None,
                        capture_alignment_frames: Some(2),
                        preplay: None,
                    }],
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 6,
                        start_state: Vec::new(),
                        events: Vec::new(),
                        start_offset: None,
                        capture_alignment_frames: Some(2),
                        preplay: None,
                    }],
                    length: Some(4),
                },
            )
            .unwrap();
        backend.clear_loop(loop_id).unwrap();
        let captured = backend.capture_session().unwrap();
        let loop_ = &captured.tracks[0].loops[0];
        assert_eq!(loop_.length, 0);
        assert!(loop_.audio[0].samples.is_empty());
        assert_eq!(loop_.audio[0].capture_alignment_frames, 0);
        assert_eq!(loop_.midi[0].length, 0);
        assert_eq!(loop_.midi[0].capture_alignment_frames, 0);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    fn loop_content_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "content".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        let sync = created.loops[0];
        let target = created.loops[1];
        backend.set_loop_gain(target, 0.75).unwrap();
        backend.set_loop_balance(target, -0.25).unwrap();
        backend.set_loop_sync_source(target, Some(sync)).unwrap();
        backend
            .transition_loop(target, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .replace_loop_content(
                target,
                &BackendLoopContentUpdate {
                    audio: vec![
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![1.0, 2.0, 3.0],
                            start_offset: Some(-1),
                            capture_alignment_frames: None,
                            preplay: Some(4),
                        },
                        BackendAudioChannelUpdate {
                            channel: 1,
                            samples: vec![10.0, 20.0, 30.0],
                            start_offset: Some(-2),
                            capture_alignment_frames: None,
                            preplay: Some(5),
                        },
                    ],
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 3,
                        start_state: vec![vec![0xB0, 7, 99]],
                        events: vec![BackendMidiEvent {
                            time: 2,
                            data: vec![0x90, 64, 127],
                        }],
                        start_offset: Some(-3),
                        capture_alignment_frames: None,
                        preplay: Some(6),
                    }],
                    length: Some(3),
                },
            )
            .unwrap();
        backend.wait_idle();

        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.loops[&target].mode, BackendLoopMode::Stopped);
        assert_eq!(snapshot.loops[&target].length, 3);
        assert_eq!(snapshot.loops[&target].gain, 0.75);
        assert_eq!(snapshot.loops[&target].balance, -0.25);
        let captured = backend.capture_session().unwrap();
        let target_content = captured
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == target.raw())
            .unwrap();
        assert_eq!(target_content.audio[0].samples, [1.0, 2.0, 3.0]);
        assert_eq!(target_content.audio[1].samples, [10.0, 20.0, 30.0]);
        assert_eq!(target_content.audio[0].start_offset, -1);
        assert_eq!(target_content.audio[1].preplay, 5);
        assert_eq!(target_content.midi[0].events[0].time, 2);
        assert!(target_content.midi[0]
            .start_state
            .iter()
            .any(|message| message == &[0xB0, 7, 99]));

        backend
            .replace_loop_content(
                target,
                &BackendLoopContentUpdate {
                    audio: vec![
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![1.0, 2.0, 3.0],
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                        BackendAudioChannelUpdate {
                            channel: 1,
                            samples: vec![10.0, 20.0, 30.0],
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                    ],
                    ..Default::default()
                },
            )
            .unwrap();
        let retained_settings = backend.capture_session().unwrap();
        let retained_content = retained_settings
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == target.raw())
            .unwrap();
        assert_eq!(retained_content.audio[0].start_offset, -1);
        assert_eq!(retained_content.audio[0].preplay, 4);
        assert_eq!(retained_content.audio[1].start_offset, -2);
        assert_eq!(retained_content.audio[1].preplay, 5);
        assert_eq!(retained_content.midi[0].events[0].time, 2);

        backend
            .set_loop_timing(target, Some(-7), Some(12), Some(9))
            .unwrap();
        let audio_chunk = backend.loop_audio_data_chunk(target, 0, 1, 1).unwrap();
        assert_eq!(audio_chunk.start_offset, -7);
        assert_eq!(audio_chunk.preplay, 12);
        assert_eq!(audio_chunk.samples, [2.0]);
        let edited = backend.capture_session().unwrap();
        let edited_content = edited
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == target.raw())
            .unwrap();
        assert_eq!(edited_content.length, 9);
        assert!(edited_content
            .audio
            .iter()
            .all(|channel| channel.start_offset == -7 && channel.preplay == 12));
        assert!(edited_content
            .midi
            .iter()
            .all(|channel| channel.start_offset == -7 && channel.preplay == 12));

        backend
            .transition_loop(sync, BackendLoopMode::Recording, None)
            .unwrap();
        let audio_before_rejected = backend.loop_audio_data(sync).unwrap();
        assert!(backend
            .replace_loop_content(
                sync,
                &BackendLoopContentUpdate {
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![99.0],
                        start_offset: None,
                        capture_alignment_frames: None,
                        preplay: None,
                    }],
                    ..Default::default()
                },
            )
            .is_err());
        assert_eq!(
            backend.loop_audio_data(sync).unwrap(),
            audio_before_rejected
        );
        backend
            .transition_loop(sync, BackendLoopMode::Stopped, None)
            .unwrap();

        backend
            .transition_loop(target, BackendLoopMode::Playing, None)
            .unwrap();
        backend.set_loop_length(target, 9).unwrap();
        backend.wait_idle();
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.loops[&target].mode, BackendLoopMode::Playing);
        assert_eq!(snapshot.loops[&target].length, 9);
        let lengthened = backend.capture_session().unwrap();
        let lengthened_content = lengthened
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == target.raw())
            .unwrap();
        assert_eq!(lengthened_content.audio[0].samples, [1.0, 2.0, 3.0]);
        assert_eq!(lengthened_content.audio[1].samples, [10.0, 20.0, 30.0]);

        let before_invalid = backend.capture_session().unwrap();
        assert!(backend
            .replace_loop_content(
                target,
                &BackendLoopContentUpdate {
                    audio: vec![
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![99.0],
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![100.0],
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                    ],
                    ..Default::default()
                },
            )
            .is_err());
        assert_eq!(backend.capture_session().unwrap(), before_invalid);
    }

    fn armed_recording_offset_update_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "armed-latency".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        backend.set_loop_length(created.loops[0], 4).unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(1),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Stopped, Some(1))
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Recording, Some(2))
            .unwrap();
        let error = backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(4),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap_err();
        assert!(error.to_string().contains("operation is armed"));
        assert_eq!(
            backend.poll().unwrap().tracks[&created.track_id]
                .latency
                .effective_offset_frames,
            Some(1)
        );
    }

    fn armed_dry_wet_playback_latency_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "armed-dry-wet-latency".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend.set_loop_length(loop_id, 4).unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                5,
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, Some(1))
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::PlayingDryThroughWet, Some(2))
            .unwrap();

        let error = backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                9,
            )
            .unwrap_err();
        assert!(error.to_string().contains("operation is armed"));
        assert_eq!(
            backend.poll().unwrap().tracks[&created.track_id]
                .latency
                .effective_processor_advance_frames,
            Some(5)
        );
    }

    fn armed_recording_length_edit_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "armed-length".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend.set_loop_length(loop_id, 4).unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, Some(1))
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, Some(2))
            .unwrap();

        let timing_error = backend
            .set_loop_timing(loop_id, None, None, Some(8))
            .unwrap_err();
        assert!(timing_error
            .to_string()
            .contains("recording operation is armed"));
        let length_error = backend.set_loop_length(loop_id, 8).unwrap_err();
        assert!(length_error
            .to_string()
            .contains("recording operation is armed"));
        assert_eq!(backend.poll().unwrap().loops[&loop_id].length, 4);
    }

    fn armed_take_correction_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "armed-take-correction".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio: (0..4)
                        .map(|channel| BackendAudioChannelUpdate {
                            channel,
                            samples: vec![0.0; 8],
                            start_offset: Some(0),
                            capture_alignment_frames: Some(0),
                            preplay: None,
                        })
                        .collect(),
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 8,
                        start_state: Vec::new(),
                        events: Vec::new(),
                        start_offset: Some(0),
                        capture_alignment_frames: Some(0),
                        preplay: None,
                    }],
                    length: Some(4),
                },
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, Some(1))
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Replacing, Some(2))
            .unwrap();

        let alignment_error = backend.set_take_alignment(loop_id, 1).unwrap_err();
        assert!(alignment_error
            .to_string()
            .contains("recording operation is armed"));
        let processor_error = backend
            .set_take_processor_alignment(loop_id, 1)
            .unwrap_err();
        assert!(processor_error
            .to_string()
            .contains("recording operation is armed"));
    }

    fn take_alignment_window_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "alignment-window".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio: vec![
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![0.0; 6],
                            start_offset: Some(1),
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                        BackendAudioChannelUpdate {
                            channel: 1,
                            samples: vec![0.0; 6],
                            start_offset: Some(1),
                            capture_alignment_frames: None,
                            preplay: None,
                        },
                    ],
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 6,
                        start_state: Vec::new(),
                        events: Vec::new(),
                        start_offset: Some(1),
                        capture_alignment_frames: None,
                        preplay: None,
                    }],
                    length: Some(4),
                },
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        let playing_error = backend.set_take_alignment(loop_id, -1).unwrap_err();
        assert!(playing_error.to_string().contains("stop loop playback"));
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, None)
            .unwrap();
        assert!(backend
            .set_take_processor_alignment(loop_id, 1)
            .unwrap_err()
            .to_string()
            .contains("no dry channel"));
        backend.set_take_alignment(loop_id, -1).unwrap();
        let aligned = backend.capture_session().unwrap();
        let content = aligned
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == loop_id.raw())
            .unwrap();
        assert!(content
            .audio
            .iter()
            .all(|channel| channel.capture_alignment_frames == -1));
        assert!(content
            .midi
            .iter()
            .all(|channel| channel.capture_alignment_frames == -1));

        let offset_error = backend
            .set_loop_timing(loop_id, Some(0), None, None)
            .unwrap_err();
        assert!(offset_error.to_string().contains("retained raw window"));
        let length_error = backend
            .set_loop_timing(loop_id, None, None, Some(7))
            .unwrap_err();
        assert!(length_error.to_string().contains("retained raw window"));
        let direct_length_error = backend.set_loop_length(loop_id, 7).unwrap_err();
        assert!(direct_length_error
            .to_string()
            .contains("retained raw window"));
        let replacement_error = backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio: (0..2)
                        .map(|channel| BackendAudioChannelUpdate {
                            channel,
                            samples: vec![0.0; 8],
                            start_offset: None,
                            capture_alignment_frames: None,
                            preplay: None,
                        })
                        .collect(),
                    length: Some(7),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(replacement_error
            .to_string()
            .contains("retained raw window"));
        let after_rejected_lengths = backend.capture_session().unwrap();
        let content = after_rejected_lengths
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == loop_id.raw())
            .unwrap();
        assert_eq!(content.length, 4);
        assert!(content
            .audio
            .iter()
            .all(|channel| channel.samples.len() == 6));
        assert_eq!(content.midi[0].length, 6);

        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(-1),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        let replacement_error = backend
            .transition_loop(loop_id, BackendLoopMode::Replacing, None)
            .unwrap_err();
        assert!(replacement_error
            .to_string()
            .contains("replacement with a nonzero recording offset"));

        let error = backend.set_take_alignment(loop_id, 2).unwrap_err();
        assert!(error.to_string().contains("retained raw window"));
        let rejected = backend.capture_session().unwrap();
        let content = rejected
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == loop_id.raw())
            .unwrap();
        assert!(content
            .audio
            .iter()
            .all(|channel| channel.capture_alignment_frames == -1));
        assert!(content
            .midi
            .iter()
            .all(|channel| channel.capture_alignment_frames == -1));
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
            capture_alignment_frames: 0,
            preplay: 2,
        };
        track
            .ports
            .iter_mut()
            .find(|port| port.source_id == input.id.raw())
            .unwrap()
            .external_connections
            .push("removed:stale_capture".to_owned());
        backend.advance(Duration::from_millis(20));
        let status_before_replace = backend.poll().unwrap().status;
        let mapping = backend.replace_session(&prepared).unwrap();
        let after_replace = backend.poll().unwrap();
        let status_after_replace = after_replace.status;
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
        assert_eq!(after_replace.connections.failures.len(), 1);
        assert_eq!(
            after_replace.connections.failures[0].external_port,
            "removed:stale_capture"
        );
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

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_satisfies_contracts() {
        let mut backend = FakeBackend::default();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
        connection_contract(&mut backend);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_dummy_backend_satisfies_contracts() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
        connection_contract(&mut backend);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_track_latency_applies_to_future_operations_only() {
        let mut backend = EngineBackend::new_dummy(48_000, 64).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "latency".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let automatic = backend.set_track_latency(
            track.track_id,
            BackendRecordingOffsetAdjustment::Automatic,
            BackendProcessorLatencyAdjustment::ManualOverride,
            0,
        );
        assert!(automatic.is_err());
        assert!(backend.tracks[&track.track_id]
            .latency
            .error
            .as_deref()
            .is_some_and(|error| error.contains("manual")));
        backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(3),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let channel = backend.loop_channels[&track.loops[0]].audio[0];
        assert_eq!(
            backend
                .session
                .audio_channel(channel)
                .unwrap()
                .capture_alignment_frames(),
            3
        );

        backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(7),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        assert_eq!(
            backend
                .session
                .audio_channel(channel)
                .unwrap()
                .capture_alignment_frames(),
            3
        );
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        assert_eq!(
            backend
                .session
                .audio_channel(channel)
                .unwrap()
                .capture_alignment_frames(),
            7
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_new_loop_inherits_configured_processor_advance() {
        let mut backend = EngineBackend::new_dummy(48_000, 64).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "future-loop-latency".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let unavailable = backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::Automatic,
                BackendProcessorLatencyAdjustment::ManualOverride,
                19,
            )
            .unwrap_err();
        assert!(unavailable.to_string().contains("manual"));
        let added_loop = backend.add_loop_to_track(track.track_id).unwrap();
        for loop_id in [track.loops[0], added_loop] {
            backend
                .transition_loop(loop_id, BackendLoopMode::PlayingDryThroughWet, None)
                .unwrap();
            let channel = backend.loop_channels[&loop_id].audio[0];
            assert_eq!(
                backend
                    .session
                    .audio_channel(channel)
                    .unwrap()
                    .render_advance_frames(),
                19
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_rejected_processor_edit_preserves_dry_through_wet_advance() {
        let mut backend = EngineBackend::new_dummy(48_000, 64).unwrap();
        let track = backend
            .create_track(TrackRequest {
                port_name_base: "retained-processor-latency".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                17,
            )
            .unwrap();
        assert!(backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                -1,
            )
            .is_err());
        backend
            .transition_loop(track.loops[0], BackendLoopMode::PlayingDryThroughWet, None)
            .unwrap();
        let channels = &backend.loop_channels[&track.loops[0]];
        for (channel, mode) in channels.audio.iter().zip(&channels.audio_modes) {
            assert_eq!(
                backend
                    .session
                    .audio_channel(*channel)
                    .unwrap()
                    .render_advance_frames(),
                if *mode == BackendChannelMode::Wet {
                    0
                } else {
                    17
                }
            );
        }
        let captured = backend.capture_session().unwrap();
        assert_eq!(captured.tracks[0].state.latency.processor_manual_frames, 17);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn compensated_grab_is_rejected_before_target_mutation() {
        let mut backend = FakeBackend::default();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "grab-latency".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                track.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(3),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        let before = backend.loop_content[&track.loops[0]].clone();
        let result = backend.grab_loops(&[BackendGrabRequest {
            loop_id: track.loops[0],
            reverse_start_cycle: None,
            cycles_length: None,
            go_to_cycle: None,
            go_to_mode: BackendLoopMode::Stopped,
        }]);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("record the loop instead"));
        assert_eq!(backend.loop_content[&track.loops[0]], before);
    }

    fn dry_wet_alignment_contract(backend: &mut dyn Backend, topology: BackendTrackTopology) {
        let (dry_audio_channels, wet_audio_channels, dry_midi) = match &topology {
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
            } => (*dry_audio_channels, *wet_audio_channels, *dry_midi),
            BackendTrackTopology::Direct { .. } => panic!("expected dry/wet topology"),
        };
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "dry-wet-alignment".to_owned(),
                topology,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(3),
                BackendProcessorLatencyAdjustment::ManualOverride,
                5,
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
        let recorded = backend.poll().unwrap().loops[&loop_id].clone();
        assert_eq!(recorded.capture_alignment_frames, 3);
        assert_eq!(recorded.processor_alignment_frames, Some(5));
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, None)
            .unwrap();

        let dry_audio_channels = usize::try_from(dry_audio_channels).unwrap();
        let wet_audio_channels = usize::try_from(wet_audio_channels).unwrap();
        let audio = (0..dry_audio_channels + wet_audio_channels)
            .map(|channel| BackendAudioChannelUpdate {
                channel,
                samples: vec![0.0; 24],
                start_offset: Some(0),
                capture_alignment_frames: Some(if channel < dry_audio_channels {
                    2
                } else {
                    7 + i32::try_from(channel - dry_audio_channels).unwrap()
                }),
                preplay: None,
            })
            .collect();
        let midi = dry_midi
            .then(|| BackendMidiChannelUpdate {
                channel: 0,
                length: 24,
                start_state: Vec::new(),
                events: Vec::new(),
                start_offset: Some(0),
                capture_alignment_frames: Some(2),
                preplay: None,
            })
            .into_iter()
            .collect();
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio,
                    midi,
                    length: Some(10),
                },
            )
            .unwrap();
        backend.set_take_processor_alignment(loop_id, 8).unwrap();
        assert_eq!(
            backend.poll().unwrap().loops[&loop_id].processor_alignment_frames,
            Some(8)
        );
        let session = backend.capture_session().unwrap();
        let content = session
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|content| content.source_id == loop_id.raw())
            .unwrap();
        let mut wet_index = 0;
        for channel in &content.audio {
            let expected = if channel.mode == BackendChannelMode::Wet {
                let expected = 10 + wet_index;
                wet_index += 1;
                expected
            } else {
                2
            };
            assert_eq!(channel.capture_alignment_frames, expected);
        }
        assert!(content
            .midi
            .iter()
            .all(|channel| channel.capture_alignment_frames == 2));

        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        assert!(backend
            .set_take_processor_alignment(loop_id, 9)
            .unwrap_err()
            .to_string()
            .contains("stop loop playback"));
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, None)
            .unwrap();
        let before_rejection = backend.capture_session().unwrap();
        assert!(backend
            .set_take_processor_alignment(loop_id, shoop_latency::MAX_COMPENSATION_FRAMES)
            .is_err());
        assert_eq!(backend.capture_session().unwrap(), before_rejection);

        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::AutomaticPlusTrim,
                5,
            )
            .unwrap();
        let latency = &backend.poll().unwrap().tracks[&created.track_id].latency;
        assert_eq!(latency.automatic_processor_advance_frames, Some(0));
        assert_eq!(latency.effective_processor_advance_frames, Some(5));
        let before_grab = backend.capture_session().unwrap();
        let grab = backend
            .grab_loops(&[BackendGrabRequest {
                loop_id,
                reverse_start_cycle: None,
                cycles_length: None,
                go_to_cycle: None,
                go_to_mode: BackendLoopMode::Stopped,
            }])
            .unwrap_err();
        assert!(grab.to_string().contains("record the loop instead"));
        assert_eq!(backend.capture_session().unwrap(), before_grab);
        let replacement = backend
            .transition_loop(loop_id, BackendLoopMode::Replacing, None)
            .unwrap_err();
        assert!(replacement
            .to_string()
            .contains("replacement with a nonzero recording offset"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_and_engine_backends_align_ordinary_dry_wet_recording_and_take_correction() {
        dry_wet_alignment_contract(
            &mut FakeBackend::default(),
            BackendTrackTopology::DryWetExternal {
                dry_audio_channels: 1,
                wet_audio_channels: 1,
                dry_midi: true,
            },
        );
        dry_wet_alignment_contract(
            &mut EngineBackend::new_dummy(48_000, 64).unwrap(),
            BackendTrackTopology::DryWetProcessor {
                processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                dry_audio_channels: 2,
                wet_audio_channels: 2,
                dry_midi: true,
            },
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_processed_track_retains_valid_processor_after_rejected_edit() {
        let mut backend = FakeBackend::default();
        let processed = backend
            .create_track(TrackRequest {
                port_name_base: "invalid-processor-recording".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                processed.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                17,
            )
            .unwrap();
        assert!(backend
            .set_track_latency(
                processed.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                -1,
            )
            .is_err());
        let latency = &backend.poll().unwrap().tracks[&processed.track_id].latency;
        assert_eq!(latency.processor_manual_frames, 17);
        assert_eq!(latency.effective_processor_advance_frames, Some(17));
        assert!(latency.error.is_some());
        let captured = backend.capture_session().unwrap();
        assert_eq!(captured.tracks[0].state.latency.processor_manual_frames, 17);

        backend
            .transition_loop(processed.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        assert_eq!(
            backend.poll().unwrap().loops[&processed.loops[0]].processor_alignment_frames,
            Some(17)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejected_partial_latency_edit_cannot_invalidate_retained_wet_sum() {
        let mut backend = FakeBackend::default();
        let processed = backend
            .create_track(TrackRequest {
                port_name_base: "partial-wet-sum".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                processed.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                700_000,
            )
            .unwrap();
        assert!(backend
            .set_track_latency(
                processed.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(100_000),
                BackendProcessorLatencyAdjustment::ManualOverride,
                -1,
            )
            .is_err());
        let latency = &backend.poll().unwrap().tracks[&processed.track_id].latency;
        assert_eq!(latency.effective_offset_frames, Some(0));
        assert_eq!(latency.effective_processor_advance_frames, Some(700_000));
        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].state.latency.effective_offset_frames,
            Some(0)
        );
        assert_eq!(
            captured.tracks[0]
                .state
                .latency
                .effective_processor_advance_frames,
            Some(700_000)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn direct_tracks_do_not_validate_an_unused_wet_sum() {
        let mut backend = FakeBackend::default();
        let direct = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "direct-large-independent-latencies".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                direct.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(700_000),
                BackendProcessorLatencyAdjustment::ManualOverride,
                100_000,
            )
            .unwrap();
        backend
            .transition_loop(direct.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        assert_eq!(
            backend.poll().unwrap().loops[&direct.loops[0]].capture_alignment_frames,
            700_000
        );

        let processed = backend
            .create_track(TrackRequest {
                port_name_base: "processed-large-combined-latency".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        assert!(backend
            .set_track_latency(
                processed.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(700_000),
                BackendProcessorLatencyAdjustment::ManualOverride,
                100_000,
            )
            .is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn carla_uses_zero_processor_automatic_baseline_and_trim() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "carla-zero-baseline".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: "carla_rack".to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::Automatic,
                99,
            )
            .unwrap();
        let automatic = &backend.poll().unwrap().tracks[&created.track_id].latency;
        assert_eq!(automatic.automatic_processor_advance_frames, Some(0));
        assert_eq!(automatic.effective_processor_advance_frames, Some(0));
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::AutomaticPlusTrim,
                13,
            )
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert_eq!(
            snapshot.tracks[&created.track_id]
                .latency
                .effective_processor_advance_frames,
            Some(13)
        );
        assert_eq!(
            snapshot.loops[&created.loops[0]].processor_alignment_frames,
            Some(13)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_and_engine_backends_satisfy_transactional_session_io_contract() {
        session_io_contract(&mut FakeBackend::default());
        session_io_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_and_engine_backends_update_loop_content_without_session_replacement() {
        loop_content_contract(&mut FakeBackend::default());
        loop_content_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_rejects_armed_recording_offset_updates() {
        armed_recording_offset_update_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_freezes_latency_while_dry_wet_playback_is_armed() {
        armed_dry_wet_playback_latency_contract(
            &mut EngineBackend::new_dummy(48_000, 256).unwrap(),
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_and_engine_backends_reject_length_edits_while_recording_is_armed() {
        armed_recording_length_edit_contract(&mut FakeBackend::default());
        armed_recording_length_edit_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_rejects_take_corrections_while_recording_is_armed() {
        armed_take_correction_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_and_engine_backends_reject_take_alignment_outside_retained_media_atomically() {
        take_alignment_window_contract(&mut FakeBackend::default());
        take_alignment_window_contract(&mut EngineBackend::new_dummy(48_000, 256).unwrap());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_replacement_rejects_mismatched_retained_take_alignment() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "replacement-alignment".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    audio: vec![BackendAudioChannelUpdate {
                        channel: 0,
                        samples: vec![0.0; 8],
                        start_offset: Some(0),
                        capture_alignment_frames: Some(2),
                        preplay: None,
                    }],
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 8,
                        start_state: Vec::new(),
                        events: Vec::new(),
                        start_offset: Some(0),
                        capture_alignment_frames: Some(2),
                        preplay: None,
                    }],
                    length: Some(4),
                },
            )
            .unwrap();
        let before = backend.loop_content[&loop_id].clone();

        let error = backend
            .transition_loop(loop_id, BackendLoopMode::Replacing, None)
            .unwrap_err();
        assert!(error.to_string().contains("offset differs from the take"));
        assert_eq!(backend.loop_content[&loop_id], before);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_take_corrections_reject_planned_recording_state() {
        let mut backend = FakeBackend::default();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "fake-armed-take-correction".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend.loops.get_mut(&loop_id).unwrap().next_mode =
            Some(BackendLoopMode::RecordingDryIntoWet);

        assert!(backend
            .set_take_alignment(loop_id, 1)
            .unwrap_err()
            .to_string()
            .contains("recording operation is armed"));
        assert!(backend
            .set_take_processor_alignment(loop_id, 1)
            .unwrap_err()
            .to_string()
            .contains("recording operation is armed"));

        backend.loops.get_mut(&loop_id).unwrap().next_mode =
            Some(BackendLoopMode::PlayingDryThroughWet);
        assert!(backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(0),
                BackendProcessorLatencyAdjustment::ManualOverride,
                1,
            )
            .unwrap_err()
            .to_string()
            .contains("operation is armed"));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_aborts_negative_recording_without_captured_preroll() {
        let mut backend = EngineBackend::new_dummy(48_000, 4).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "preroll-guard".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend.set_loop_length(loop_id, 4).unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(-3),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
        backend.advance_frames(1);

        assert_eq!(
            backend.poll().unwrap().loops[&loop_id].mode,
            BackendLoopMode::Stopped
        );
        let captured = backend.capture_session().unwrap();
        let content = captured
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .find(|loop_| loop_.source_id == loop_id.raw())
            .unwrap();
        assert_eq!(content.length, 0);
        assert!(content
            .audio
            .iter()
            .all(|channel| channel.samples.is_empty()));
        assert!(content
            .midi
            .iter()
            .all(|channel| channel.length == 0 && channel.events.is_empty()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_backend_rejects_new_recording_until_postroll_settles() {
        let mut backend = EngineBackend::new_dummy(48_000, 4).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "postroll-guard".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = created.loops[0];
        backend.set_loop_length(loop_id, 4).unwrap();
        backend
            .set_track_latency(
                created.track_id,
                BackendRecordingOffsetAdjustment::ManualOverride(3),
                BackendProcessorLatencyAdjustment::ManualOverride,
                0,
            )
            .unwrap();
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
        backend.advance_frames(4);
        backend
            .transition_loop(loop_id, BackendLoopMode::Stopped, None)
            .unwrap();
        assert!(backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .is_none());
        assert!(backend.loop_midi_data(loop_id).unwrap().is_none());
        assert!(backend
            .capture_session()
            .unwrap_err()
            .to_string()
            .contains("postroll is still finalizing"));
        let immediate_error = backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap_err();
        assert!(immediate_error
            .to_string()
            .contains("postroll is still finalizing"));
        backend.advance_frames(1);

        let error = backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap_err();
        assert!(error.to_string().contains("postroll is still finalizing"));
        assert_eq!(
            backend.poll().unwrap().loops[&loop_id].mode,
            BackendLoopMode::Stopped
        );

        backend.advance_frames(2);
        assert!(backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .is_some());
        assert!(backend.loop_midi_data(loop_id).unwrap().is_some());
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        assert_eq!(connections.application_ports.len(), created.ports.len() + 3);
        assert_eq!(
            connections
                .application_ports
                .values()
                .filter(|port| port.owner == BackendPortOwner::GlobalFxControl)
                .count(),
            1
        );
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn web_midi_dual_route_is_additive_but_only_track_copy_records() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        let endpoint = BackendHostPortDescriptor {
            id: "webmidi:source:dual".to_owned(),
            name: "Dual route".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Output,
        };
        backend
            .configure_web_midi_endpoints(vec![endpoint.clone()])
            .unwrap();
        let track = backend
            .create_track(TrackRequest {
                port_name_base: "dual_tiny".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        let input = track
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::MidiInput)
            .unwrap();
        let global = backend
            .poll()
            .unwrap()
            .connections
            .application_ports
            .values()
            .find(|port| port.owner == BackendPortOwner::GlobalFxControl)
            .unwrap()
            .id;
        backend
            .set_port_connected(input.id, &endpoint.id, true)
            .unwrap();
        backend
            .set_port_connected(global, &endpoint.id, true)
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        assert_eq!(
            backend
                .stage_web_midi_input(&endpoint.id, &[0xb0, 7, 99])
                .unwrap(),
            2
        );
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&[], 0, &mut [], 0, 128)
                .unwrap();
        });
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].loops[0].midi[0]
                .events
                .iter()
                .filter(|event| event.data == [0xb0, 7, 99])
                .count(),
            1
        );
        assert_eq!(
            captured.global_ports[0].external_connections,
            vec![endpoint.id]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn missing_desired_global_web_midi_identity_survives_replace_and_reconnects() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 0).unwrap();
        let endpoint = BackendHostPortDescriptor {
            id: "webmidi:source:global-hotplug".to_owned(),
            name: "Global hotplug".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Output,
        };
        backend
            .configure_web_midi_endpoints(vec![endpoint.clone()])
            .unwrap();
        let global = backend
            .poll()
            .unwrap()
            .connections
            .application_ports
            .values()
            .find(|port| port.owner == BackendPortOwner::GlobalFxControl)
            .unwrap()
            .id;
        backend
            .set_port_connected(global, &endpoint.id, true)
            .unwrap();
        backend.configure_web_midi_endpoints(Vec::new()).unwrap();
        let saved = backend.capture_session().unwrap();
        assert_eq!(
            saved.global_ports[0].external_connections,
            vec![endpoint.id.clone()]
        );
        backend.replace_session(&saved).unwrap();
        backend
            .configure_web_midi_endpoints(vec![endpoint.clone()])
            .unwrap();
        assert!(backend
            .poll()
            .unwrap()
            .connections
            .confirmed_links
            .contains(&BackendConfirmedLink {
                application_port_id: backend.global_fx_port,
                host_port_id: endpoint.id,
            }));
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_ignores_dry_audio_inputs() {
        let render = |input_value: f32| {
            let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
            backend.configure_web_audio_channels(2, 2).unwrap();
            let created = backend
                .create_track(TrackRequest {
                    port_name_base: "oxisynth-ignore-audio".to_owned(),
                    topology: BackendTrackTopology::DryWetProcessor {
                        processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                        dry_audio_channels: 2,
                        wet_audio_channels: 2,
                        dry_midi: true,
                    },
                    initial_loops: 1,
                })
                .unwrap();
            backend
                .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
                .unwrap();
            backend.poll().unwrap();
            let input = vec![input_value; 256];
            let mut output = vec![0.0; 256];
            for _ in 0..4 {
                backend
                    .process_audio_quantum(&input, 2, &mut output, 2, 128)
                    .unwrap();
            }
            output
        };
        assert_eq!(render(0.0), render(0.75));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_controls_snapshot_and_session_state_are_transactional() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(0, 2).unwrap();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "oxisynth-state".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend.poll().unwrap();
        backend
            .inject_midi_input(
                created.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x9f, 60, 100],
                }],
            )
            .unwrap();
        let mut output = vec![0.0; 256];
        for _ in 0..8 {
            backend
                .process_audio_quantum(&[], 0, &mut output, 2, 128)
                .unwrap();
        }
        assert!(output.iter().any(|sample| sample.abs() > f32::EPSILON));

        backend
            .inject_midi_input(
                created.track_id,
                &[
                    BackendMidiEvent {
                        time: 0,
                        data: vec![0xcf, 40],
                    },
                    BackendMidiEvent {
                        time: 0,
                        data: vec![0xbf, 0, 1],
                    },
                    BackendMidiEvent {
                        time: 0,
                        data: vec![0xbf, 32, 1],
                    },
                ],
            )
            .unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut output, 2, 128)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        let Some(TrackProcessorEditorState::OxiSynth(editor)) = snapshot.tracks[&created.track_id]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:0");

        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset("0:40".to_owned())),
            )
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SetReverbSend(0.25)),
            )
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SetChorusSend(0.5)),
            )
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::AssignMidiCc(
                    OxiSynthMidiCcAssignment {
                        parameter: OxiSynthParameter::ReverbSend,
                        channel: 15,
                        controller: 91,
                    },
                )),
            )
            .unwrap();
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetVisible(true))
            .unwrap();
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::ToggleOrRecover)
            .unwrap();
        assert!(
            !backend.poll().unwrap().tracks[&created.track_id]
                .fx
                .as_ref()
                .unwrap()
                .visible
        );
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetVisible(true))
            .unwrap();
        let state = backend
            .track_fx_state_string(created.track_id)
            .unwrap()
            .unwrap();
        assert_eq!(state, "shoop-oxisynth:2:timgm6mb:0:40:3e800000:3f000000");
        let preserved_midi = vec![
            BackendMidiEvent {
                time: 0,
                data: vec![0xcf, 41],
            },
            BackendMidiEvent {
                time: 1,
                data: vec![0xbf, 0, 1],
            },
            BackendMidiEvent {
                time: 2,
                data: vec![0xbf, 32, 2],
            },
            BackendMidiEvent {
                time: 3,
                data: vec![0x9f, 64, 100],
            },
        ];
        backend
            .replace_loop_content(
                created.loops[0],
                &BackendLoopContentUpdate {
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 128,
                        start_state: vec![vec![0xbf, 0, 3], vec![0xcf, 42]],
                        events: preserved_midi.clone(),
                        start_offset: None,
                        capture_alignment_frames: None,
                        preplay: None,
                    }],
                    length: Some(128),
                    ..BackendLoopContentUpdate::default()
                },
            )
            .unwrap();
        backend
            .transition_loop(
                created.loops[0],
                BackendLoopMode::PlayingDryThroughWet,
                None,
            )
            .unwrap();
        backend.poll().unwrap();
        backend
            .process_audio_quantum(&[], 0, &mut output, 2, 128)
            .unwrap();
        let midi = backend.loop_midi_data(created.loops[0]).unwrap().unwrap();
        assert_eq!(midi.channels[0].events, preserved_midi);
        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].processor_state.as_deref(),
            Some(state.as_str())
        );
        assert!(captured.tracks[0].loops[0].midi[0]
            .start_state
            .contains(&vec![0xbf, 0, 3]));
        assert!(captured.tracks[0].loops[0].midi[0]
            .start_state
            .contains(&vec![0xcf, 42]));
        assert_eq!(captured.tracks[0].loops[0].midi[0].events, preserved_midi);
        assert_eq!(
            captured.tracks[0].oxisynth_midi_cc_assignments,
            [BackendOxiSynthMidiCcAssignment {
                parameter: BackendOxiSynthParameter::ReverbSend,
                channel: 15,
                controller: 91,
            }]
        );

        assert!(backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::RestoreState("malformed".to_owned()),
            )
            .is_err());
        assert!(backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset("1:0".to_owned(),)),
            )
            .is_err());
        assert_eq!(
            backend.track_fx_state_string(created.track_id).unwrap(),
            Some(state.clone())
        );
        let source_track = captured.tracks[0].source_id;
        let replacement = backend.replace_session(&captured).unwrap();
        let restored_track = replacement.tracks[&source_track].track_id;
        let snapshot = backend.poll().unwrap();
        let fx = snapshot.tracks[&restored_track].fx.as_ref().unwrap();
        let Some(TrackProcessorEditorState::OxiSynth(editor)) = fx.editor.as_ref() else {
            panic!("missing restored OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(editor.reverb_send, 0.25);
        assert_eq!(editor.chorus_send, 0.5);
        assert_eq!(
            editor.midi_cc_assignments.as_ref(),
            [OxiSynthMidiCcAssignment {
                parameter: OxiSynthParameter::ReverbSend,
                channel: 15,
                controller: 91,
            }]
        );
        assert!(!fx.visible);
        backend
            .set_track_fx_control(
                restored_track,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::Panic),
            )
            .unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        assert_eq!(snapshot.connections.application_ports.len(), 5);
        let status = snapshot.status;
        assert_eq!(status.callback_count, 2);
        assert_eq!(status.processed_frames, 256);
        assert!(status.input_peak == 0.0);
        assert!(status.output_peak > 0.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_peak_publication_resets_measurement_window() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        backend.configure_web_audio_channels(1, 1).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "peak_window".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let mut output = vec![0.0; 128];
        backend
            .process_audio_quantum(&vec![0.5; 128], 1, &mut output, 1, 128)
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let _ = backend.poll().unwrap();

        backend
            .transition_loop(track.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        output.fill(0.0);
        backend
            .process_audio_quantum(&vec![0.0; 128], 1, &mut output, 1, 128)
            .unwrap();
        let loud = backend.poll().unwrap();
        assert!(loud.tracks[&track.track_id].output_peaks[0] > -100.0);
        assert!(loud.loops[&track.loops[0]].audio_peaks[0] > -100.0);

        let silent = backend.poll().unwrap();
        assert!(silent.tracks[&track.track_id].output_peaks[0] <= -100.0);
        assert!(silent.loops[&track.loops[0]].audio_peaks[0] <= -100.0);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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
        for _ in 0..INPUT_CAPTURE_CAPACITY_SECONDS - 2 {
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn targeted_midi_data_preserves_channel_metadata_events_and_content() {
        let mut backend = EngineBackend::new_dummy(48_000, 64).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "midi_details".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = track.loops[0];
        backend
            .replace_loop_content(
                loop_id,
                &BackendLoopContentUpdate {
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 32,
                        start_state: Vec::new(),
                        events: vec![
                            BackendMidiEvent {
                                time: 3,
                                data: vec![0x90, 64, 100],
                            },
                            BackendMidiEvent {
                                time: 19,
                                data: vec![0x80, 64, 0],
                            },
                        ],
                        start_offset: Some(-4),
                        capture_alignment_frames: None,
                        preplay: Some(7),
                    }],
                    length: Some(32),
                    ..Default::default()
                },
            )
            .unwrap();
        let before = backend.capture_session().unwrap();
        let data = backend.loop_midi_data(loop_id).unwrap().unwrap();
        let channel = &data.channels[0];
        assert_eq!(channel.mode, BackendChannelMode::Direct);
        assert_eq!(channel.length, 32);
        assert_eq!(channel.start_offset, -4);
        assert_eq!(channel.preplay, 7);
        assert_eq!(channel.events[0].data, [0x90, 64, 100]);
        assert_eq!(channel.events[1].time, 19);
        assert!(channel.content_revision > 0);
        assert_eq!(backend.capture_session().unwrap(), before);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn engine_runtime_progresses_only_from_explicit_quanta() {
        let mut runtime = EngineBackend::new_dummy_runtime(48_000, 128).unwrap();
        runtime.advance(Duration::from_secs(1));
        assert_eq!(runtime.processed_frames(), 0);
        runtime.advance_frames(128);
        assert_eq!(runtime.processed_frames(), 128);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn elapsed_time_preserves_fractional_frame_remainders() {
        let mut backend = EngineBackend::new_dummy(1_000, 64).unwrap();
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 0);
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn global_loop_smoothing_survives_local_session_and_driver_replacement() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend.set_loop_smoothing_ms(7).unwrap();
        let session = backend.capture_session().unwrap();
        assert!(!serde_json::to_string(&session).unwrap().contains("smooth"));
        assert_eq!(backend.loop_smoothing_ms(), 7);
        backend.replace_session(&session).unwrap();
        assert_eq!(backend.loop_smoothing_ms(), 7);

        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        });
        backend
            .switch_audio_driver(&config, 48_000, &session)
            .unwrap();
        assert_eq!(backend.loop_smoothing_ms(), 7);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fake_backend_observes_loop_smoothing_requests() {
        let mut backend = FakeBackend::default();
        backend.set_loop_smoothing_ms(0).unwrap();
        backend.set_loop_smoothing_ms(31).unwrap();
        assert_eq!(backend.loop_smoothing_ms(), Some(31));
        assert!(backend
            .operations()
            .contains(&FakeOperation::SetLoopSmoothingMs(0)));
    }

    #[shoop_wasm_test_support::shoop_test]
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
