#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

#[cfg(test)]
extern crate std;

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(all(feature = "legacy", any(test, feature = "contracts")))]
pub mod contracts;
#[cfg(feature = "legacy")]
mod legacy;
#[cfg(feature = "legacy")]
pub use legacy::*;

pub use shoop_app_api::{
    BackendCapabilities, BackendCapability, BuiltInFxControl, BuiltInFxDriveType,
    BuiltInFxMidiCcAssignment, BuiltInFxModulationType, BuiltInFxParameter, BuiltInFxReverbType,
    BuiltInFxStage, BuiltInFxState, OxiSynthControl, OxiSynthMidiCcAssignment, OxiSynthParameter,
    OxiSynthState, TrackProcessorEditorState, TrackProcessorTypeId, MAX_BUSES, MAX_BUS_CHANNELS,
    MAX_BUS_HOST_LINKS, MAX_BUS_NAME_BYTES, MAX_MIXER_ROUTES, MAX_TOTAL_BUS_CHANNELS,
};

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::sync::Arc;
use anyhow::{anyhow, Result};
use core::time::Duration;
use serde::{Deserialize, Serialize};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverRuntimeState, ResolvedAudioDriverConfig, TrackFxState,
    TrackProcessorDescriptor, MAX_BUS_GAIN_DB, MIN_BUS_GAIN_DB,
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
    pub preferred_playback: bool,
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
pub struct BackendBusFxRequest {
    pub processor_type: String,
    pub audio_channels: u32,
}

impl BackendBusFxRequest {
    pub fn normalized(self, channel_count: usize) -> Result<Self> {
        let audio_channels = usize::try_from(self.audio_channels)
            .map_err(|_| anyhow!("bus FX channel count exceeds this platform"))?;
        if audio_channels != channel_count {
            return Err(anyhow!("bus FX channels must match the bus channel count"));
        }
        if self.processor_type == TrackProcessorTypeId::OXISYNTH {
            return Err(anyhow!("Built-in Synth is unavailable on buses"));
        }
        Ok(Self {
            processor_type: self.processor_type,
            audio_channels: self.audio_channels,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BackendBusFxControl {
    SetActive(bool),
    SetVisible(bool),
    ToggleOrRecover,
    RestoreState(String),
    ClearLogs,
    BuiltInFx(BuiltInFxControl),
    SetProcessor(Option<BackendBusFxRequest>),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendBusRequest {
    pub name: String,
    pub channel_count: u32,
    #[serde(default)]
    pub fx: Option<BackendBusFxRequest>,
}

impl BackendBusRequest {
    pub fn normalized(self) -> Result<Self> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            return Err(anyhow!("bus name must not be empty"));
        }
        if name.len() > MAX_BUS_NAME_BYTES {
            return Err(anyhow!(
                "bus name exceeds the {MAX_BUS_NAME_BYTES}-byte limit"
            ));
        }
        if name.chars().any(char::is_control) {
            return Err(anyhow!("bus name must not contain control characters"));
        }
        let channel_count = usize::try_from(self.channel_count)
            .map_err(|_| anyhow!("bus channel count exceeds this platform"))?;
        if channel_count == 0 || channel_count > MAX_BUS_CHANNELS {
            return Err(anyhow!(
                "bus channel count must be in 1..={MAX_BUS_CHANNELS}"
            ));
        }
        Ok(Self {
            name,
            channel_count: self.channel_count,
            fx: self.fx,
        })
    }
}

pub fn default_bus_channel_labels(channel_count: usize) -> Result<Vec<String>> {
    if channel_count == 0 || channel_count > MAX_BUS_CHANNELS {
        return Err(anyhow!(
            "bus channel count must be in 1..={MAX_BUS_CHANNELS}"
        ));
    }
    Ok(match channel_count {
        1 => vec!["Mono".to_owned()],
        2 => vec!["Left".to_owned(), "Right".to_owned()],
        _ => (1..=channel_count)
            .map(|index| format!("Channel {index}"))
            .collect(),
    })
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendBusChannelState {
    pub id: BackendBusChannelId,
    pub label: String,
    pub output_port_id: BackendPortId,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendBusCreation {
    pub bus_id: BackendBusId,
    pub channels: Vec<BackendBusChannelState>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum BackendBusControl {
    GainDb(f32),
    Balance(f32),
    Mute(bool),
}

impl BackendBusControl {
    pub fn normalized(self, channel_count: usize) -> Result<Self> {
        match self {
            Self::GainDb(value) if value.is_finite() => {
                Ok(Self::GainDb(value.clamp(MIN_BUS_GAIN_DB, MAX_BUS_GAIN_DB)))
            }
            Self::Balance(value) if value.is_finite() && channel_count == 2 => {
                Ok(Self::Balance(value.clamp(-1.0, 1.0)))
            }
            Self::Mute(value) => Ok(Self::Mute(value)),
            Self::GainDb(_) => Err(anyhow!("bus gain must be finite")),
            Self::Balance(_) if channel_count != 2 => {
                Err(anyhow!("balance is available only for stereo buses"))
            }
            Self::Balance(_) => Err(anyhow!("bus balance must be finite")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendBusState {
    pub id: BackendBusId,
    pub name: String,
    pub channels: Vec<BackendBusChannelState>,
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub balance: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub output_peaks_db: Vec<f32>,
    #[serde(skip)]
    pub fx: Option<TrackFxState>,
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendMixerSnapshot {
    pub revision: u64,
    pub buses: BTreeMap<BackendBusId, BackendBusState>,
    pub confirmed_links: BTreeSet<BackendMixerLink>,
    pub failures: Vec<BackendMixerFailure>,
}

pub const GLOBAL_FX_PORT_ID: BackendPortId = BackendPortId::from_raw(9_007_199_254_740_991);
pub use shoop_app_api::MASTER_BUS_NAME;
pub const MASTER_BUS_ID: BackendBusId = BackendBusId::from_raw(1);
pub use shoop_app_api::MASTER_BUS_CHANNEL_LABELS;
pub const MASTER_BUS_CHANNEL_IDS: [BackendBusChannelId; 2] = [
    BackendBusChannelId::from_raw(1),
    BackendBusChannelId::from_raw(2),
];
pub const MASTER_BUS_OUTPUT_PORT_IDS: [BackendPortId; 2] = [
    BackendPortId::from_raw(9_007_199_254_740_989),
    BackendPortId::from_raw(9_007_199_254_740_990),
];

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
    BuiltInFx(BuiltInFxControl),
    OxiSynth(OxiSynthControl),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendTrackState {
    pub topology: BackendTrackTopology,
    #[serde(default)]
    pub default_playback_mode: BackendDefaultPlaybackMode,
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
    pub quantum_frames: u32,
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
pub enum BackendDefaultPlaybackMode {
    #[default]
    Regular,
    DryThroughWet,
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
    pub recording_armed: bool,
    pub timing_revision: u64,
    pub channel_timings: Vec<BackendChannelTimingState>,
    pub content_revision: u64,
    pub sync_source: Option<BackendLoopId>,
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
    pub samples: Arc<Vec<f32>>,
    pub gain: f32,
    pub start_offset: i32,
    #[serde(default)]
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendAudioChannelData {
    pub samples: Arc<Vec<f32>>,
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
pub struct BackendBuiltInFxMidiCcAssignment {
    pub parameter: BackendBuiltInFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum BackendBuiltInFxParameter {
    CompressorThreshold,
    CompressorRatio,
    CompressorAttack,
    CompressorRelease,
    CompressorMakeup,
    Drive,
    DriveTone,
    DriveMix,
    DriveOutput,
    EqLow,
    EqMid,
    EqHigh,
    ChorusRate,
    ChorusDepth,
    ChorusMix,
    ChorusWidth,
    ModulationRate,
    ModulationDepth,
    ModulationMix,
    ModulationFeedback,
    ModulationSpread,
    ReverbAmount,
    ReverbTone,
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

pub use shoop_app_api::MidiEventState as BackendMidiEvent;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendMidiContent {
    pub mode: BackendChannelMode,
    pub length: u32,
    pub start_state: Vec<Vec<u8>>,
    pub events: Arc<Vec<BackendMidiEvent>>,
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
    pub events: Arc<Vec<BackendMidiEvent>>,
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

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct BackendChannelTimingState {
    pub channel: usize,
    pub is_midi: bool,
    pub frames: u64,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub preplay: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendChannelTimingUpdate {
    pub channel: usize,
    pub is_midi: bool,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BackendAudioChannelUpdate {
    pub channel: usize,
    pub samples: Arc<Vec<f32>>,
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
    pub events: Arc<Vec<BackendMidiEvent>>,
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

/// A complete content cut with fences for the affected sync family and its ancestors.
#[derive(Clone, Debug, PartialEq)]
pub struct BackendLoopFamilyUpdate {
    pub root: BackendLoopId,
    pub sample_rate: u32,
    pub quantum_frames: u32,
    pub members: Vec<BackendLoopFamilyMember>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BackendLoopFamilyMember {
    pub loop_id: BackendLoopId,
    pub sync_source: Option<BackendLoopId>,
    pub expected_revision: u64,
    pub expected_frames: u32,
    pub fence_content: bool,
    pub restart: bool,
    pub content: Option<BackendLoopContent>,
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
    pub builtin_fx_midi_cc_assignments: Vec<BackendBuiltInFxMidiCcAssignment>,
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
    #[serde(default)]
    pub gain_db: f32,
    #[serde(default)]
    pub balance: f32,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub processor_type: Option<String>,
    #[serde(default)]
    pub processor_state: Option<String>,
    #[serde(default)]
    pub builtin_fx_midi_cc_assignments: Vec<BackendBuiltInFxMidiCcAssignment>,
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

/// Whether any loop in the session carries content, as opposed to a structure-only session.
pub fn session_has_loop_content(session: &BackendSessionData) -> bool {
    session
        .tracks
        .iter()
        .flat_map(|track| &track.loops)
        .any(|content| {
            content.length > 0
                || content.audio.iter().any(|channel| {
                    !channel.samples.is_empty()
                        || channel.start_offset != 0
                        || channel.capture_alignment_frames != 0
                        || channel.preplay != 0
                })
                || content.midi.iter().any(|channel| {
                    channel.length > 0
                        || !channel.events.is_empty()
                        || !channel.start_state.is_empty()
                        || channel.start_offset != 0
                        || channel.capture_alignment_frames != 0
                        || channel.preplay != 0
                })
        })
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
    BusStructure,
    TrackControl,
    BusControl,
    TrackFxControl,
    BusFxControl,
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
    BusCreation,
    BusRemoval,
    LoopCreation { loop_id: BackendLoopId },
    CompositeConfiguration { plan_version: u64 },
    TrackControl(BackendTrackControl),
    BusControl(BackendBusControl),
    TrackDefaultPlaybackMode(BackendDefaultPlaybackMode),
    TrackFxControl(BackendTrackFxControl),
    BusFxControl(BackendBusFxControl),
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

/// One admitted mutation's engine outcome, distinct from telemetry publication.
#[derive(Clone, Debug, PartialEq)]
pub struct BackendMutationCompletion {
    pub sequence: u64,
    pub result: Result<(), String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendSnapshot {
    pub profiling: shoop_app_api::ProfilingView,
    pub status: BackendStatus,
    pub audio_drivers: AudioDriverRuntimeState,
    pub tracks: BTreeMap<BackendTrackId, BackendTrackState>,
    pub loops: BTreeMap<BackendLoopId, BackendLoopState>,
    pub composites: BTreeMap<BackendCompositeId, BackendCompositeState>,
    pub removed_composites: Vec<BackendCompositeId>,
    pub connections: BackendConnectionSnapshot,
    pub mixer: BackendMixerSnapshot,
    pub mutation_failures: Vec<BackendMutationFailure>,
    pub mutation_completions: Vec<BackendMutationCompletion>,
}

pub trait Backend {
    fn inspect_profiling(
        &mut self,
        _request: shoop_app_api::profiling_data::CaptureRequest,
    ) -> Result<()> {
        Err(anyhow!(
            "Profiling captures are unavailable on this backend"
        ))
    }
    fn set_profiling(&mut self, enabled: bool) -> Result<()> {
        if enabled {
            Err(anyhow!("Runtime profiling is unavailable on this backend"))
        } else {
            Ok(())
        }
    }
    fn bus_limits(&self) -> shoop_app_api::BusLimits {
        shoop_app_api::BusLimits::default()
    }
    /// Synchronous backends use cursor zero and complete before returning.
    fn mutation_cursor(&self) -> u64 {
        0
    }
    fn mutations_pending_through(&self, _sequence: u64) -> bool {
        false
    }
    /// Available outcome-tracking slots, reserved before submitting a shared operation.
    fn mutation_capacity(&self) -> usize {
        usize::MAX
    }
    /// Discard client-owned bulk reads without canceling session mutations.
    fn cancel_media_reads(&mut self) {}
    fn close_connection(&mut self) {}

    fn start_bus_audio_preview_async(
        &mut self,
        _bus: BackendBusId,
        _request_id: u64,
        _sample_rate: u32,
        _samples: Arc<Vec<f32>>,
    ) -> Result<BackendAsyncResult<()>> {
        Err(anyhow!("Master bus preview is unavailable in this backend"))
    }
    fn audio_preview_playing(&self) -> bool {
        false
    }
    fn supports_audio_preview(&self) -> bool {
        false
    }
    fn start_audio_preview(
        &mut self,
        _request_id: u64,
        _sample_rate: u32,
        _samples: Arc<[f32]>,
    ) -> Result<()> {
        Err(anyhow!("Master bus preview is unavailable in this backend"))
    }
    fn stop_audio_preview(&mut self) -> Result<()> {
        Ok(())
    }
    fn finished_audio_preview(&self) -> Option<u64> {
        None
    }

    /// Whether the backend schedules composite loops itself; otherwise the application does.
    /// Implies `BackendCapability::CompositeLoops`.
    fn supports_composite_loops(&self) -> bool {
        false
    }

    /// The optional features this backend supports.
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::FULL
    }

    fn composite_plan_mutations_are_synchronous(&self) -> bool {
        true
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(Arc::from([]))
    }

    fn bus_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
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
    /// Installs a composite configuration and returns its backend-wide plan version.
    fn configure_composite_loop(
        &mut self,
        _composite_id: BackendCompositeId,
        _config: &BackendCompositeConfig,
    ) -> Result<u64> {
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
    fn remove_composite_loop(&mut self, _composite_id: BackendCompositeId) -> Result<Option<u64>> {
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
    fn set_track_default_playback_mode(
        &mut self,
        _track_id: BackendTrackId,
        _mode: BackendDefaultPlaybackMode,
    ) -> Result<()> {
        Err(anyhow!("track default playback control is unavailable"))
    }
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
                        samples: samples.to_vec().into(),
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
            .unwrap_or_else(|| Arc::new(Vec::new()));
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
    fn set_loop_channel_timings(
        &mut self,
        _loop_id: BackendLoopId,
        _updates: &[BackendChannelTimingUpdate],
    ) -> Result<()> {
        Err(anyhow!("channel timing updates are unavailable"))
    }
    fn capture_session(&mut self) -> Result<BackendSessionData> {
        Err(anyhow!("session capture is unavailable"))
    }
    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        self.capture_session().map(BackendAsyncResult::Ready)
    }
    fn capture_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<BackendAsyncResult<BackendLoopContent>> {
        match self.capture_session_async()? {
            BackendAsyncResult::Pending(progress) => Ok(BackendAsyncResult::Pending(progress)),
            BackendAsyncResult::Ready(session) => session
                .tracks
                .into_iter()
                .flat_map(|track| track.loops)
                .find(|content| content.source_id == loop_id.raw())
                .map(BackendAsyncResult::Ready)
                .ok_or_else(|| anyhow!("capture omitted loop {loop_id:?}")),
        }
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
    fn cancel_session_replacement(&mut self) -> Result<()> {
        Ok(())
    }
    fn replace_session_with_sync_async(
        &mut self,
        session: &BackendSessionData,
        sync: &BTreeMap<u64, u64>,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        if !sync.is_empty() {
            return Err(anyhow!("atomic session sync restoration is unavailable"));
        }
        self.replace_session_async(session)
    }
    fn replace_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        self.replace_loop_content(loop_id, update)
            .map(BackendAsyncResult::Ready)
    }
    fn replace_loop_family_async(
        &mut self,
        update: &BackendLoopFamilyUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        let _ = update;
        Err(anyhow!("atomic loop family replacement is unavailable"))
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
    fn create_bus(&mut self, request: BackendBusRequest) -> Result<BackendBusCreation> {
        let _ = request;
        Err(anyhow!("bus creation is unavailable"))
    }
    fn remove_bus(&mut self, bus_id: BackendBusId) -> Result<()> {
        let _ = bus_id;
        Err(anyhow!("bus removal is unavailable"))
    }
    fn set_bus_control(&mut self, bus_id: BackendBusId, control: BackendBusControl) -> Result<()> {
        let _ = (bus_id, control);
        Err(anyhow!("bus control is unavailable"))
    }
    fn set_bus_fx_control(
        &mut self,
        _bus_id: BackendBusId,
        _control: BackendBusFxControl,
    ) -> Result<()> {
        Err(anyhow!("bus FX controls are unavailable"))
    }
    fn bus_fx_state_string(&mut self, _bus_id: BackendBusId) -> Result<Option<String>> {
        Ok(None)
    }
    fn advance(&mut self, elapsed: Duration);
    fn poll(&mut self) -> Result<BackendSnapshot>;
    fn wait_idle(&mut self);
}
