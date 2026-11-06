#![no_std]
extern crate alloc;

mod host_graph;
pub use host_graph::*;

#[cfg(test)]
extern crate std;

use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use alloc::sync::Arc;
use core::fmt;

macro_rules! entity_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
        pub struct $name(u64);

        impl $name {
            pub const INVALID: Self = Self(0);

            pub const fn from_raw(value: u64) -> Self {
                Self(value)
            }

            pub const fn raw(self) -> u64 {
                self.0
            }

            pub const fn is_valid(self) -> bool {
                self.0 != 0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

entity_id!(TrackId);
entity_id!(BusId);
entity_id!(BusChannelId);
entity_id!(LoopId);
entity_id!(PortId);
entity_id!(ChannelId);
entity_id!(TaskId);
entity_id!(ScriptId);
entity_id!(ScriptDialogId);
entity_id!(ScriptDialogButtonId);

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostPortId(String);

impl HostPortId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HostPortId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackProcessorTypeId(String);

impl TrackProcessorTypeId {
    pub const EXTERNAL: &'static str = "external";
    pub const CARLA_RACK: &'static str = "carla_rack";
    pub const CARLA_PATCHBAY: &'static str = "carla_patchbay";
    pub const CARLA_PATCHBAY_16X: &'static str = "carla_patchbay_16x";
    pub const OXISYNTH: &'static str = "oxisynth";
    pub const BUILTIN_FX: &'static str = "builtin_fx";

    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TrackProcessorTypeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackProcessorFeatures {
    pub state: bool,
    pub external_ui: bool,
    pub embedded_ui: bool,
    pub recovery: bool,
    pub logs: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackProcessorMidiPolicy {
    #[default]
    Unsupported,
    Optional,
    Required,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackProcessorConstraints {
    pub min_dry_audio_channels: Option<u32>,
    pub max_dry_audio_channels: Option<u32>,
    pub min_wet_audio_channels: Option<u32>,
    pub max_wet_audio_channels: Option<u32>,
    pub matching_audio_channels: bool,
    pub midi: TrackProcessorMidiPolicy,
}

impl TrackProcessorConstraints {
    pub fn accepts(self, dry_audio_channels: u32, wet_audio_channels: u32, dry_midi: bool) -> bool {
        self.min_dry_audio_channels
            .is_none_or(|limit| dry_audio_channels >= limit)
            && self
                .max_dry_audio_channels
                .is_none_or(|limit| dry_audio_channels <= limit)
            && self
                .min_wet_audio_channels
                .is_none_or(|limit| wet_audio_channels >= limit)
            && self
                .max_wet_audio_channels
                .is_none_or(|limit| wet_audio_channels <= limit)
            && (!self.matching_audio_channels || dry_audio_channels == wet_audio_channels)
            && match self.midi {
                TrackProcessorMidiPolicy::Unsupported => !dry_midi,
                TrackProcessorMidiPolicy::Optional => true,
                TrackProcessorMidiPolicy::Required => dry_midi,
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackProcessorPresetDescriptor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackProcessorEditorDescriptor {
    BuiltInFx,
    OxiSynth {
        presets: Arc<[TrackProcessorPresetDescriptor]>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostedProcessorResources {
    pub instance_limit: u16,
    pub instances_in_use: u16,
    pub buffering_frames: u32,
    pub reserved_host_bytes: u64,
    pub budget_host_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackProcessorDescriptor {
    #[cfg_attr(feature = "serde", serde(default))]
    pub resources: Option<HostedProcessorResources>,
    pub id: TrackProcessorTypeId,
    pub label: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub constraints: TrackProcessorConstraints,
    pub features: TrackProcessorFeatures,
    pub editor: Option<TrackProcessorEditorDescriptor>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FxLifecycle {
    #[default]
    Unavailable,
    Starting,
    Running,
    Degraded,
    Crashed,
    Restarting,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FxGenerationLogState {
    pub generation: u64,
    pub stdout: Arc<str>,
    pub stderr: Arc<str>,
    pub dropped_stdout_bytes: u64,
    pub dropped_stderr_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatestMidiMessage {
    pub bytes: [u8; 4],
    pub len: u8,
}

#[cfg(feature = "serde")]
impl serde::Serialize for LatestMidiMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&(self.bytes, self.len), serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LatestMidiMessage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (bytes, len) = <([u8; 4], u8) as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(bytes, len).ok_or_else(|| serde::de::Error::custom("invalid MIDI message length"))
    }
}

impl LatestMidiMessage {
    pub const fn new(bytes: [u8; 4], len: u8) -> Option<Self> {
        if len == 0 || len > 4 {
            None
        } else {
            Some(Self { bytes, len })
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }

    pub const fn midi_cc(self) -> Option<(u8, u8, u8)> {
        if self.len == 3
            && self.bytes[0] & 0xf0 == 0xb0
            && self.bytes[1] <= 127
            && self.bytes[2] <= 127
        {
            Some((self.bytes[0] & 0x0f, self.bytes[1], self.bytes[2]))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OxiSynthParameter {
    ReverbSend,
    ChorusSend,
}

impl OxiSynthParameter {
    pub const ALL: [Self; 2] = [Self::ReverbSend, Self::ChorusSend];

    pub const fn label(self) -> &'static str {
        match self {
            Self::ReverbSend => "Reverb send",
            Self::ChorusSend => "Chorus send",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OxiSynthMidiCcAssignment {
    pub parameter: OxiSynthParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OxiSynthState {
    pub selected_preset_id: String,
    pub reverb_send: f32,
    pub chorus_send: f32,
    pub midi_cc_assignments: Arc<[OxiSynthMidiCcAssignment]>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxStage {
    Compressor,
    Drive,
    Eq,
    Chorus,
    Modulation,
    Reverb,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxDriveType {
    #[default]
    Saturation,
    Overdrive,
    Distortion,
    Fuzz,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxModulationType {
    #[default]
    Tremolo,
    Flanger,
    Phaser,
}

#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxReverbType {
    #[default]
    Room,
    Hall,
    Plate,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxParameter {
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

impl BuiltInFxParameter {
    pub const ALL: [Self; 23] = [
        Self::CompressorThreshold,
        Self::CompressorRatio,
        Self::CompressorAttack,
        Self::CompressorRelease,
        Self::CompressorMakeup,
        Self::Drive,
        Self::DriveTone,
        Self::DriveMix,
        Self::DriveOutput,
        Self::EqLow,
        Self::EqMid,
        Self::EqHigh,
        Self::ChorusRate,
        Self::ChorusDepth,
        Self::ChorusMix,
        Self::ChorusWidth,
        Self::ModulationRate,
        Self::ModulationDepth,
        Self::ModulationMix,
        Self::ModulationFeedback,
        Self::ModulationSpread,
        Self::ReverbAmount,
        Self::ReverbTone,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::CompressorThreshold => "Compressor — Threshold",
            Self::CompressorRatio => "Compressor — Ratio",
            Self::CompressorAttack => "Compressor — Attack",
            Self::CompressorRelease => "Compressor — Release",
            Self::CompressorMakeup => "Compressor — Makeup",
            Self::Drive => "Drive — Drive",
            Self::DriveTone => "Drive — Tone",
            Self::DriveMix => "Drive — Mix",
            Self::DriveOutput => "Drive — Output",
            Self::EqLow => "EQ — Low",
            Self::EqMid => "EQ — Mid",
            Self::EqHigh => "EQ — High",
            Self::ChorusRate => "Chorus — Rate",
            Self::ChorusDepth => "Chorus — Depth",
            Self::ChorusMix => "Chorus — Mix",
            Self::ChorusWidth => "Chorus — Stereo Width",
            Self::ModulationRate => "Modulation — Rate",
            Self::ModulationDepth => "Modulation — Depth",
            Self::ModulationMix => "Modulation — Mix",
            Self::ModulationFeedback => "Modulation — Feedback",
            Self::ModulationSpread => "Modulation — Stereo Spread",
            Self::ReverbAmount => "Reverb — Amount",
            Self::ReverbTone => "Reverb — Tone",
        }
    }

    pub const fn range(self) -> (f32, f32) {
        match self {
            Self::CompressorThreshold => (-48.0, 0.0),
            Self::CompressorRatio => (1.0, 20.0),
            Self::CompressorAttack => (0.5, 100.0),
            Self::CompressorRelease => (20.0, 1_000.0),
            Self::CompressorMakeup => (0.0, 18.0),
            Self::Drive => (0.0, 36.0),
            Self::DriveOutput => (-18.0, 6.0),
            Self::EqLow | Self::EqMid | Self::EqHigh => (-12.0, 12.0),
            Self::ChorusRate | Self::ModulationRate => (0.05, 5.0),
            Self::ModulationFeedback => (-0.95, 0.95),
            Self::DriveTone
            | Self::DriveMix
            | Self::ChorusDepth
            | Self::ChorusMix
            | Self::ChorusWidth
            | Self::ModulationDepth
            | Self::ModulationMix
            | Self::ModulationSpread
            | Self::ReverbAmount
            | Self::ReverbTone => (0.0, 1.0),
        }
    }

    pub fn is_valid(self, value: f32) -> bool {
        let (minimum, maximum) = self.range();
        value.is_finite() && (minimum..=maximum).contains(&value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuiltInFxMidiCcAssignment {
    pub parameter: BuiltInFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BuiltInFxState {
    pub compressor_enabled: bool,
    pub compressor_threshold_db: f32,
    pub compressor_ratio: f32,
    pub compressor_attack_ms: f32,
    pub compressor_release_ms: f32,
    pub compressor_makeup_db: f32,
    pub drive_enabled: bool,
    pub drive_type: BuiltInFxDriveType,
    pub drive_db: f32,
    pub drive_tone: f32,
    pub drive_mix: f32,
    pub drive_output_db: f32,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub chorus_enabled: bool,
    pub chorus_rate_hz: f32,
    pub chorus_depth: f32,
    pub chorus_mix: f32,
    pub chorus_width: f32,
    pub modulation_enabled: bool,
    pub modulation_type: BuiltInFxModulationType,
    pub modulation_rate_hz: f32,
    pub modulation_depth: f32,
    pub modulation_mix: f32,
    pub modulation_feedback: f32,
    pub modulation_spread: f32,
    pub reverb_enabled: bool,
    pub reverb_type: BuiltInFxReverbType,
    pub reverb_amount: f32,
    pub reverb_tone: f32,
    pub midi_cc_assignments: Arc<[BuiltInFxMidiCcAssignment]>,
}

impl Default for BuiltInFxState {
    fn default() -> Self {
        Self {
            compressor_enabled: false,
            compressor_threshold_db: -18.0,
            compressor_ratio: 4.0,
            compressor_attack_ms: 10.0,
            compressor_release_ms: 150.0,
            compressor_makeup_db: 0.0,
            drive_enabled: false,
            drive_type: BuiltInFxDriveType::Saturation,
            drive_db: 12.0,
            drive_tone: 0.5,
            drive_mix: 1.0,
            drive_output_db: 0.0,
            eq_enabled: false,
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            chorus_enabled: false,
            chorus_rate_hz: 0.3,
            chorus_depth: 0.5,
            chorus_mix: 0.3,
            chorus_width: 1.0,
            modulation_enabled: false,
            modulation_type: BuiltInFxModulationType::Tremolo,
            modulation_rate_hz: 0.5,
            modulation_depth: 0.5,
            modulation_mix: 0.5,
            modulation_feedback: 0.25,
            modulation_spread: 1.0,
            reverb_enabled: true,
            reverb_type: BuiltInFxReverbType::Room,
            reverb_amount: 0.2,
            reverb_tone: 0.5,
            midi_cc_assignments: Arc::from([]),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackProcessorEditorState {
    BuiltInFx(BuiltInFxState),
    OxiSynth(OxiSynthState),
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackFxState {
    pub processor_type: TrackProcessorTypeId,
    pub active: bool,
    pub visible: bool,
    pub lifecycle: FxLifecycle,
    pub generation: u64,
    pub deadline_misses: u64,
    pub stale_completions: u64,
    pub status_summary: Option<String>,
    pub crash_summary: Option<String>,
    pub logs: Arc<[FxGenerationLogState]>,
    pub editor: Option<TrackProcessorEditorState>,
}

impl TrackProcessorEditorState {
    pub fn canonical_state(&self) -> Result<String, shoop_processor_protocol::state::InvalidState> {
        use shoop_processor_protocol::{
            state::{InvalidState, State},
            Kind,
        };
        let state = match self {
            Self::BuiltInFx(fx) => {
                let mut state = State::new(Kind::BuiltInFx)?;
                state.values[..23].copy_from_slice(&[
                    fx.compressor_threshold_db,
                    fx.compressor_ratio,
                    fx.compressor_attack_ms,
                    fx.compressor_release_ms,
                    fx.compressor_makeup_db,
                    fx.drive_db,
                    fx.drive_tone,
                    fx.drive_mix,
                    fx.drive_output_db,
                    fx.eq_low_db,
                    fx.eq_mid_db,
                    fx.eq_high_db,
                    fx.chorus_rate_hz,
                    fx.chorus_depth,
                    fx.chorus_mix,
                    fx.chorus_width,
                    fx.modulation_rate_hz,
                    fx.modulation_depth,
                    fx.modulation_mix,
                    fx.modulation_feedback,
                    fx.modulation_spread,
                    fx.reverb_amount,
                    fx.reverb_tone,
                ]);
                for (index, enabled) in [
                    fx.compressor_enabled,
                    fx.drive_enabled,
                    fx.eq_enabled,
                    fx.chorus_enabled,
                    fx.modulation_enabled,
                    fx.reverb_enabled,
                ]
                .into_iter()
                .enumerate()
                {
                    state.values[23 + index] = u8::from(enabled) as f32;
                }
                state.values[29] = fx.drive_type as u8 as f32;
                state.values[30] = fx.modulation_type as u8 as f32;
                state.values[31] = fx.reverb_type as u8 as f32;
                state
            }
            Self::OxiSynth(synth) => {
                let (bank, program) = synth
                    .selected_preset_id
                    .split_once(':')
                    .ok_or(InvalidState)?;
                let bank = bank.parse::<u32>().map_err(|_| InvalidState)?;
                let program = program.parse::<u32>().map_err(|_| InvalidState)?;
                if bank > 65535 || program > 127 {
                    return Err(InvalidState);
                }
                let mut state = State::new(Kind::OxiSynth)?;
                state.values[0] = synth.reverb_send;
                state.values[1] = synth.chorus_send;
                state.values[2] = (bank * 128 + program) as f32;
                state
            }
        };
        state.validate()?;
        Ok(state.encode())
    }
}

pub const MIN_TRACK_GAIN_DB: f32 = -30.0;
pub const MAX_TRACK_GAIN_DB: f32 = 20.0;
pub const MIN_BUS_GAIN_DB: f32 = MIN_TRACK_GAIN_DB;
pub const MAX_BUS_GAIN_DB: f32 = MAX_TRACK_GAIN_DB;
pub const MIN_OXISYNTH_SEND: f32 = 0.0;
pub const MAX_OXISYNTH_SEND: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RecordingOffsetAdjustmentState {
    Automatic,
    #[default]
    ManualOverride,
    AutomaticPlusTrim,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ProcessorLatencyAdjustmentState {
    Automatic,
    #[default]
    ManualOverride,
    AutomaticPlusTrim,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackLatencyState {
    pub automatic_offset_frames: Option<i32>,
    pub adjustment: RecordingOffsetAdjustmentState,
    pub manual_frames: i32,
    pub effective_offset_frames: Option<i32>,
    pub automatic_processor_advance_frames: Option<u32>,
    pub processor_adjustment: ProcessorLatencyAdjustmentState,
    pub processor_manual_frames: i32,
    pub effective_processor_advance_frames: Option<u32>,
    pub pending: bool,
    pub error: Option<String>,
}

impl Default for TrackLatencyState {
    fn default() -> Self {
        Self {
            automatic_offset_frames: None,
            adjustment: RecordingOffsetAdjustmentState::ManualOverride,
            manual_frames: 0,
            effective_offset_frames: Some(0),
            automatic_processor_advance_frames: Some(0),
            processor_adjustment: ProcessorLatencyAdjustmentState::ManualOverride,
            processor_manual_frames: 0,
            effective_processor_advance_frames: Some(0),
            pending: false,
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DefaultRecordingAction {
    #[default]
    Record,
    Grab,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DefaultPlaybackMode {
    #[default]
    Regular,
    DryThroughWet,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LoopMode {
    #[default]
    Unknown,
    Stopped,
    Playing,
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CompositeKind {
    #[default]
    None,
    Regular,
    Script,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GlobalControlState {
    pub default_recording_action: DefaultRecordingAction,
    pub play_after_record: bool,
    pub sync: bool,
    pub solo: bool,
    pub auto_mute_other_track_inputs: bool,
    pub auto_arm_track_inputs: bool,
    pub apply_n_cycles: u32,
}

impl Default for GlobalControlState {
    fn default() -> Self {
        Self {
            default_recording_action: DefaultRecordingAction::Record,
            play_after_record: true,
            sync: true,
            solo: false,
            auto_mute_other_track_inputs: false,
            auto_arm_track_inputs: true,
            apply_n_cycles: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioDriverState {
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

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioDriverKind {
    #[default]
    Dummy,
    Jack,
    Cpal,
    WebAudio,
}

impl AudioDriverKind {
    pub const fn id(self) -> &'static str {
        match self {
            Self::Dummy => "dummy",
            Self::Jack => "jack",
            Self::Cpal => "cpal",
            Self::WebAudio => "webaudio",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Dummy => "Dummy / offline",
            Self::Jack => "JACK",
            Self::Cpal => "CPAL",
            Self::WebAudio => "Web Audio",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DummyAudioDriverConfig {
    pub sample_rate: u32,
    pub buffer_size: u32,
}

impl Default for DummyAudioDriverConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            buffer_size: 256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default, deny_unknown_fields))]
pub struct JackAudioDriverConfig {
    pub client_name: String,
}

impl Default for JackAudioDriverConfig {
    fn default() -> Self {
        Self {
            client_name: "ShoopDaLoop".to_owned(),
        }
    }
}

impl JackAudioDriverConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.client_name.trim().is_empty()
            || self.client_name.len() > 63
            || self.client_name.contains([':', '\0'])
        {
            return Err(
                "JACK client name must contain 1 to 63 bytes without a colon or NUL".to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CpalAudioDriverConfig {
    pub client_name: String,
    pub host: String,
    pub output_device: String,
    pub input_device: String,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub output_channels: String,
    pub input_channels: String,
    pub capture_ring_frames: u32,
    /// MIDI selectors; `=name` selects one exact port name.
    pub midi_inputs: Vec<String>,
    /// MIDI selectors; `=name` selects one exact port name.
    pub midi_outputs: Vec<String>,
}

impl Default for CpalAudioDriverConfig {
    fn default() -> Self {
        Self {
            client_name: "ShoopDaLoop".to_owned(),
            host: "default".to_owned(),
            output_device: "default".to_owned(),
            input_device: "default".to_owned(),
            sample_rate: 0,
            buffer_size: 0,
            output_channels: "all".to_owned(),
            input_channels: "all".to_owned(),
            capture_ring_frames: 4096,
            midi_inputs: vec!["all".to_owned()],
            midi_outputs: vec!["all".to_owned()],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioDriverConfig {
    Dummy(DummyAudioDriverConfig),
    Jack(JackAudioDriverConfig),
    Cpal(CpalAudioDriverConfig),
    WebAudio,
}

impl AudioDriverConfig {
    pub fn bounded(&self) -> bool {
        let text = |value: &str| value.len() <= 128 && !value.contains('\0');
        match self {
            Self::Dummy(config) => {
                (1..=768_000).contains(&config.sample_rate)
                    && (1..=65_536).contains(&config.buffer_size)
            }
            Self::Jack(config) => text(&config.client_name) && config.validate().is_ok(),
            Self::Cpal(config) => {
                [
                    &config.client_name,
                    &config.host,
                    &config.output_device,
                    &config.input_device,
                    &config.output_channels,
                    &config.input_channels,
                ]
                .into_iter()
                .all(|value| text(value))
                    && config.sample_rate <= 768_000
                    && config.buffer_size <= 65_536
                    && config.capture_ring_frames <= 1_048_576
                    && config.midi_inputs.len() <= 32
                    && config.midi_outputs.len() <= 32
                    && config
                        .midi_inputs
                        .iter()
                        .chain(&config.midi_outputs)
                        .all(|value| text(value))
                    && [
                        &config.client_name,
                        &config.host,
                        &config.output_device,
                        &config.input_device,
                        &config.output_channels,
                        &config.input_channels,
                    ]
                    .into_iter()
                    .chain(config.midi_inputs.iter())
                    .chain(config.midi_outputs.iter())
                    .map(String::len)
                    .sum::<usize>()
                        <= 768
            }
            Self::WebAudio => false,
        }
    }

    pub const fn kind(&self) -> AudioDriverKind {
        match self {
            Self::Dummy(_) => AudioDriverKind::Dummy,
            Self::Jack(_) => AudioDriverKind::Jack,
            Self::Cpal(_) => AudioDriverKind::Cpal,
            Self::WebAudio => AudioDriverKind::WebAudio,
        }
    }
}

impl Default for AudioDriverConfig {
    fn default() -> Self {
        Self::Dummy(DummyAudioDriverConfig::default())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioDriverDescriptor {
    pub kind: AudioDriverKind,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub hosts: Vec<String>,
    pub input_devices: Vec<String>,
    pub output_devices: Vec<String>,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResolvedAudioDriverConfig {
    pub configured: AudioDriverConfig,
    pub sample_rate: u32,
    /// Zero during preflight means that the device chooses its callback size on open.
    pub buffer_size: u32,
    pub instance_name: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AudioDriverSwitchStatus {
    #[default]
    Idle,
    AwaitingConfirmation,
    Switching,
    Resampling,
    Restoring,
    Persisting,
    Completed,
    Failed,
    Fatal,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioDriverSwitchState {
    pub request_id: u64,
    pub status: AudioDriverSwitchStatus,
    pub source: Option<ResolvedAudioDriverConfig>,
    pub target: Option<ResolvedAudioDriverConfig>,
    pub message: String,
    pub persistence_retry_available: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AudioDriverRuntimeState {
    pub supported: bool,
    pub catalog: Arc<[AudioDriverDescriptor]>,
    pub active: Option<ResolvedAudioDriverConfig>,
    pub switch: AudioDriverSwitchState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConversionTarget {
    pub sample_rate: u32,
    pub quantum_frames: u32,
}

impl ConversionTarget {
    pub fn is_valid(self) -> bool {
        self.sample_rate > 0 && matches!(self.quantum_frames, 1 | 16 | 32 | 64 | 128)
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusState {
    pub version: String,
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
    pub quantum_frames: u32,
    pub audio_driver: AudioDriverState,
    pub callback_count: u64,
    pub processed_frames: u64,
    pub input_peak: f32,
    pub output_peak: f32,
    pub render_discontinuities: u32,
    pub memory_growths: u32,
    pub render_memory_growths: u32,
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
}

impl StatusState {
    pub fn conversion_target(&self) -> Option<ConversionTarget> {
        let target = ConversionTarget {
            sample_rate: self.sample_rate,
            quantum_frames: self.quantum_frames,
        };
        target.is_valid().then_some(target)
    }

    pub fn latency_ms(&self) -> Option<f32> {
        (self.sample_rate > 0).then(|| self.buffer_size as f32 * 1000.0 / self.sample_rate as f32)
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackControlState {
    pub has_output: bool,
    pub has_output_audio: bool,
    pub output_stereo: bool,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub output_peak_left_db: f32,
    pub output_peak_right_db: f32,
    pub output_midi_activity: bool,
    pub has_input: bool,
    pub has_input_audio: bool,
    pub input_stereo: bool,
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub input_monitoring: bool,
    pub input_peak_left_db: f32,
    pub input_peak_right_db: f32,
    pub input_midi_activity: bool,
    pub latest_input_midi_message: Option<LatestMidiMessage>,
}

impl Default for TrackControlState {
    fn default() -> Self {
        Self {
            has_output: false,
            has_output_audio: false,
            output_stereo: false,
            output_gain_db: 0.0,
            output_balance: 0.0,
            output_muted: false,
            output_peak_left_db: -200.0,
            output_peak_right_db: -200.0,
            output_midi_activity: false,
            has_input: false,
            has_input_audio: false,
            input_stereo: false,
            input_gain_db: 0.0,
            input_balance: 0.0,
            input_monitoring: false,
            input_peak_left_db: -200.0,
            input_peak_right_db: -200.0,
            input_midi_activity: false,
            latest_input_midi_message: None,
        }
    }
}

impl TrackControlState {
    pub fn clamp(&mut self) {
        self.output_gain_db = self
            .output_gain_db
            .clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB);
        self.input_gain_db = self
            .input_gain_db
            .clamp(MIN_TRACK_GAIN_DB, MAX_TRACK_GAIN_DB);
        self.output_balance = self.output_balance.clamp(-1.0, 1.0);
        self.input_balance = self.input_balance.clamp(-1.0, 1.0);
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StructuralState {
    #[default]
    Confirmed,
    Creating,
    Removing,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopChannelTimingState {
    pub id: ChannelId,
    pub is_midi: bool,
    pub data_length: u64,
    pub start_offset: i64,
    pub capture_alignment_frames: i64,
    pub preplay_samples: u64,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LoopState {
    #[cfg_attr(feature = "serde", serde(default))]
    pub capture_incomplete: bool,
    pub repeat_sync: bool,
    pub timing_revision: u64,
    pub channel_timings: Arc<[LoopChannelTimingState]>,
    pub content_revision: u64,
    pub id: LoopId,
    pub name: String,
    pub structural_state: StructuralState,
    pub length_frames: u64,
    pub position: f32,
    pub mode: LoopMode,
    pub next_mode: LoopMode,
    pub next_transition_delay: Option<u32>,
    pub empty: bool,
    pub composite_kind: CompositeKind,
    pub composite_iteration: Option<u32>,
    pub composite_cycle_count: u64,
    pub active_composite_children: Arc<[LoopId]>,
    pub sync: bool,
    pub targeted: bool,
    pub selected: bool,
    pub selected_composite_kind: CompositeKind,
    pub show_gain: bool,
    pub has_audio: bool,
    pub has_midi: bool,
    pub gain: f32,
    pub balance: f32,
    pub play_after_record: bool,
    pub stereo: bool,
    pub peak_left_db: f32,
    pub peak_right_db: f32,
    pub midi_activity: bool,
    pub has_recorded_fx_state: bool,
    pub capture_alignment_frames: i32,
    pub processor_alignment_frames: Option<u32>,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            capture_incomplete: false,
            repeat_sync: false,
            timing_revision: 0,
            channel_timings: Arc::from([]),
            content_revision: 0,
            id: LoopId::INVALID,
            name: "Loop".to_owned(),
            structural_state: StructuralState::Confirmed,
            length_frames: 0,
            position: 0.0,
            mode: LoopMode::Unknown,
            next_mode: LoopMode::Unknown,
            next_transition_delay: None,
            empty: true,
            composite_kind: CompositeKind::None,
            composite_iteration: None,
            composite_cycle_count: 0,
            active_composite_children: Arc::from([]),
            sync: false,
            targeted: false,
            selected: false,
            selected_composite_kind: CompositeKind::None,
            show_gain: false,
            has_audio: false,
            has_midi: false,
            gain: 0.6,
            balance: 0.0,
            play_after_record: true,
            stereo: false,
            peak_left_db: -200.0,
            peak_right_db: -200.0,
            midi_activity: false,
            has_recorded_fx_state: false,
            capture_alignment_frames: 0,
            processor_alignment_frames: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PortRole {
    AudioInput,
    AudioOutput,
    AudioSend,
    AudioReturn,
    MidiInput,
    MidiOutput,
    MidiSend,
}

impl PortRole {
    pub const ORDERED: [Self; 7] = [
        Self::AudioInput,
        Self::AudioOutput,
        Self::AudioSend,
        Self::AudioReturn,
        Self::MidiInput,
        Self::MidiOutput,
        Self::MidiSend,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::AudioInput => "Audio in",
            Self::AudioOutput => "Audio out",
            Self::AudioSend => "Audio send",
            Self::AudioReturn => "Audio return",
            Self::MidiInput => "MIDI in",
            Self::MidiOutput => "MIDI out",
            Self::MidiSend => "MIDI send",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackPortOwnerKind {
    Sync,
    Main,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ApplicationPortOwner {
    GlobalFxControl,
    Bus {
        bus_id: BusId,
    },
    Track {
        track_id: TrackId,
        kind: TrackPortOwnerKind,
    },
    LuaControl {
        script_id: ScriptId,
        registration: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ConnectionPolicy {
    UserManaged,
    OwnerManaged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ApplicationPortState {
    #[cfg_attr(feature = "serde", serde(default))]
    pub publication: Option<HostGraphLocalKey>,
    pub id: PortId,
    pub owner: ApplicationPortOwner,
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
    pub role: PortRole,
    pub connection_policy: ConnectionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HostPortState {
    pub id: HostPortId,
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
    pub available: bool,
}

pub const MASTER_BUS_NAME: &str = "Master";
pub const MASTER_BUS_CHANNEL_LABELS: [&str; 2] = ["Left", "Right"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusLimits {
    pub buses: u32,
    pub channels: u32,
    pub total_channels: u32,
}
impl Default for BusLimits {
    fn default() -> Self {
        Self {
            buses: MAX_BUSES as u32,
            channels: MAX_BUS_CHANNELS as u32,
            total_channels: MAX_TOTAL_BUS_CHANNELS as u32,
        }
    }
}

pub const MAX_BUS_NAME_BYTES: usize = 128;
pub const MAX_BUSES: usize = 64;
pub const MAX_BUS_CHANNELS: usize = 64;
pub const MAX_TOTAL_BUS_CHANNELS: usize = 256;
pub const MAX_MIXER_ROUTES: usize = 4_096;
pub const MAX_BUS_HOST_LINKS: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusChannelState {
    pub id: BusChannelId,
    pub label: String,
    pub output_port_id: PortId,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusState {
    pub id: BusId,
    pub name: String,
    pub structural_state: StructuralState,
    pub structural_error: Option<String>,
    pub channels: Arc<[BusChannelState]>,
    pub gain_db: f32,
    pub balance: f32,
    pub muted: bool,
    pub output_peaks_db: Arc<[f32]>,
    pub control_pending: bool,
    pub control_error: Option<String>,
    pub fx: Option<TrackFxState>,
}

impl BusState {
    pub fn stereo(&self) -> bool {
        self.channels.len() == 2
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MixerRouteState {
    pub source_port_id: PortId,
    pub destination_channel_id: BusChannelId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingMixerRouteState {
    pub route: MixerRouteState,
    pub desired_connected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MixerRouteErrorState {
    pub route: MixerRouteState,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConfirmedConnectionState {
    pub application_port_id: PortId,
    pub host_port_id: HostPortId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PendingConnectionState {
    pub application_port_id: PortId,
    pub host_port_id: HostPortId,
    pub desired_connected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ConnectionErrorKind {
    StaleLocalPort,
    EndpointUnavailable,
    Incompatible,
    CommandSaturated,
    BackendRejected,
    TimedOut,
    BackendUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectionErrorState {
    pub port_id: Option<PortId>,
    pub external_port: Option<String>,
    pub kind: ConnectionErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConnectionViewState {
    pub revision: u64,
    pub loading: bool,
    pub backend_available: bool,
    pub application_ports: Arc<[ApplicationPortState]>,
    pub host_ports: Arc<[HostPortState]>,
    pub confirmed_links: Arc<[ConfirmedConnectionState]>,
    pub pending_links: Arc<[PendingConnectionState]>,
    pub mixer_links: Arc<[MixerRouteState]>,
    pub pending_mixer_links: Arc<[PendingMixerRouteState]>,
    pub mixer_errors: Arc<[MixerRouteErrorState]>,
    pub errors: Arc<[ConnectionErrorState]>,
}

impl Default for ConnectionViewState {
    fn default() -> Self {
        Self {
            revision: 0,
            loading: true,
            backend_available: false,
            application_ports: Arc::from([]),
            host_ports: Arc::from([]),
            confirmed_links: Arc::from([]),
            pending_links: Arc::from([]),
            mixer_links: Arc::from([]),
            pending_mixer_links: Arc::from([]),
            mixer_errors: Arc::from([]),
            errors: Arc::from([]),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TrackChannelRole {
    DirectAudio(u32),
    DirectMidi,
    DryAudio(u32),
    DryMidi,
    WetAudio(u32),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackTopology {
    #[default]
    Direct,
    DryWet {
        dry_audio_channels: u32,
        wet_audio_channels: u32,
        dry_midi: bool,
        processor_type: TrackProcessorTypeId,
    },
}
impl TrackTopology {
    pub fn audio_channel_labels(&self, direct_channels: usize) -> Vec<String> {
        match self {
            Self::Direct => (1..=direct_channels)
                .map(|index| format!("Audio {index}"))
                .collect(),
            Self::DryWet {
                dry_audio_channels,
                wet_audio_channels,
                ..
            } => (1..=*dry_audio_channels)
                .map(|index| format!("Dry {index}"))
                .chain((1..=*wet_audio_channels).map(|index| format!("Wet {index}")))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackState {
    pub id: TrackId,
    pub name: String,
    pub structural_state: StructuralState,
    pub is_sync: bool,
    pub topology: TrackTopology,
    pub default_playback_mode: DefaultPlaybackMode,
    pub fx: Option<TrackFxState>,
    pub loops: Vec<LoopState>,
    pub controls: TrackControlState,
    pub latency: TrackLatencyState,
    pub port_ids: Arc<[PortId]>,
}

#[derive(Clone, Debug)]
pub struct WaveformChannelState {
    pub id: ChannelId,
    pub label: String,
    pub samples: Arc<[f32]>,
    pub start_offset: i64,
    pub preplay_samples: u64,
    pub loop_length: u64,
    pub played_sample: Option<i64>,
}

impl Default for WaveformChannelState {
    fn default() -> Self {
        Self {
            id: ChannelId::INVALID,
            label: String::new(),
            samples: Arc::from([]),
            start_offset: 0,
            preplay_samples: 0,
            loop_length: 0,
            played_sample: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MidiEventState {
    pub frame: u32,
    pub data: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct MidiSequenceChannelState {
    pub id: ChannelId,
    pub label: String,
    pub content_revision: u64,
    pub events: Arc<[MidiEventState]>,
    pub data_length: u64,
    pub capture_alignment_frames: i64,
    pub start_offset: i64,
    pub preplay_samples: u64,
    pub loop_length: u64,
    pub played_sample: Option<i64>,
}

impl Default for MidiSequenceChannelState {
    fn default() -> Self {
        Self {
            id: ChannelId::INVALID,
            label: String::new(),
            content_revision: 0,
            events: Arc::from([]),
            data_length: 0,
            capture_alignment_frames: 0,
            start_offset: 0,
            preplay_samples: 0,
            loop_length: 0,
            played_sample: None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeTrackDetailsState {
    pub id: TrackId,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CompositeEventId {
    pub instance_id: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeEventDetailsState {
    pub loop_id: LoopId,
    pub loop_name: String,
    pub track_id: TrackId,
    pub start_frame: u64,
    pub end_frame: u64,
    pub instance_id: u64,
    pub mode: Option<String>,
    pub forced_n_cycles: Option<u32>,
    pub loop_mode: LoopMode,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompositeTimingState {
    pub basis_sample_rate: u32,
    pub basis_frames: u64,
    pub render_sample_rate: u32,
    pub quantum_frames: u32,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeExecutionState {
    pub active_version: Option<u64>,
    pub pending_version: Option<u64>,
    pub pending_stage: Option<String>,
    pub waiting_repeat: bool,
    pub restoring: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeDetailsState {
    pub timing: Option<CompositeTimingState>,
    pub execution: CompositeExecutionState,
    pub kind: CompositeKind,
    pub available_event_modes: Vec<LoopMode>,
    pub cycle_length_frames: u64,
    pub timeline_length_frames: u64,
    pub played_frame: Option<u64>,
    pub tracks: Vec<CompositeTrackDetailsState>,
    pub events: Vec<CompositeEventDetailsState>,
}

impl CompositeDetailsState {
    pub fn frame_cycles(&self, frame: u64) -> f64 {
        if let Some(timing) = self
            .timing
            .filter(|timing| timing.basis_frames != 0 && timing.render_sample_rate != 0)
        {
            frame as f64 * f64::from(timing.basis_sample_rate)
                / (timing.basis_frames as f64 * f64::from(timing.render_sample_rate))
        } else {
            frame as f64 / self.cycle_length_frames.max(1) as f64
        }
    }
    pub fn cycles_ceil(&self, frame: u64) -> u64 {
        if let Some(timing) = self
            .timing
            .filter(|timing| timing.basis_frames != 0 && timing.render_sample_rate != 0)
        {
            let numerator = u128::from(frame) * u128::from(timing.basis_sample_rate);
            let denominator =
                u128::from(timing.basis_frames) * u128::from(timing.render_sample_rate);
            u64::try_from(numerator.div_ceil(denominator)).unwrap_or(u64::MAX)
        } else {
            frame.div_ceil(self.cycle_length_frames.max(1))
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoopDetailsState {
    pub quantum_frames: u32,
    pub length_frames: u64,
    pub generation: u64,
    pub loop_id: LoopId,
    pub title: String,
    pub loading: bool,
    pub channels: Vec<WaveformChannelState>,
    pub midi_loading: bool,
    pub midi_channels: Vec<MidiSequenceChannelState>,
    pub sync_loop_length: u64,
    pub composite: Option<CompositeDetailsState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimelineEditTool {
    LoopStart,
    PreplayStart,
    LoopEnd,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoTaskKind {
    SaveSession,
    LoadSession,
    ExportLoopAudio,
    ImportLoopAudio,
    ExportLoopMidi,
    ImportLoopMidi,
    GenerateClickTrack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoTaskStatus {
    Running,
    AwaitingFormatConversionConfirmation,
    AwaitingChannelMapping,
    AwaitingChannelSelection,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileOutputOutcome {
    Saved,
    DownloadHandedOff,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormatConversionWarning {
    pub source_rate: u32,
    pub target_rate: u32,
    pub affected_media: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioChannelMappingState {
    pub source_channels: Vec<String>,
    pub destination_channels: Vec<String>,
    pub default_mapping: Vec<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioChannelSelectionState {
    pub available_channels: Vec<String>,
    pub default_selection: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IoTaskState {
    pub id: TaskId,
    pub kind: IoTaskKind,
    pub status: IoTaskStatus,
    pub progress: f32,
    pub message: String,
    pub format_conversion_warning: Option<FormatConversionWarning>,
    pub audio_channel_mapping: Option<AudioChannelMappingState>,
    pub audio_channel_selection: Option<AudioChannelSelectionState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyEventType {
    Pressed,
    Released,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyEvent {
    pub event_type: KeyEventType,
    pub key: i64,
    pub modifiers: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LuaApiVersion {
    pub major: u32,
    pub minor: u32,
}

impl LuaApiVersion {
    pub const fn accepts(self, requested: Self) -> bool {
        requested.major == self.major && requested.minor <= self.minor
    }
}

pub const LUA_API_VERSION: LuaApiVersion = LuaApiVersion { major: 1, minor: 6 };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptKind {
    Bundled,
    Example,
    User,
    Session,
    Ephemeral,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogScriptSource {
    pub identity: String,
    pub name: String,
    pub source: Arc<str>,
    pub source_path: Option<String>,
    pub resource_bundle: Option<Arc<shoop_script_resources::ScriptResourceBundle>>,
    pub kind: ScriptKind,
    pub enabled: bool,
}

pub fn is_ephemeral_script_version(display_name: &str, source_name: &str) -> bool {
    if display_name == source_name {
        return true;
    }
    display_name
        .strip_prefix(source_name)
        .and_then(|suffix| suffix.strip_prefix(" (run once "))
        .and_then(|suffix| suffix.strip_suffix(')'))
        .and_then(|version| version.parse::<u32>().ok())
        .is_some_and(|version| version >= 2)
}

pub fn ephemeral_script_display_name<'a>(
    source_name: &str,
    existing_names: impl IntoIterator<Item = &'a str>,
) -> String {
    let existing_names = existing_names
        .into_iter()
        .collect::<alloc::collections::BTreeSet<_>>();
    if !existing_names.contains(source_name) {
        return source_name.to_owned();
    }
    for version in 2_u32.. {
        let candidate = format!("{source_name} (run once {version})");
        if !existing_names.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("u32 script version space exhausted")
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScriptLifecycle {
    #[default]
    Inactive,
    Running,
    Listening,
    Finished,
    Incompatible,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptLogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptLogState {
    pub level: ScriptLogLevel,
    pub message: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptActivityDiagnostics {
    pub loop_callbacks: u32,
    pub global_callbacks: u32,
    pub keyboard_callbacks: u32,
    pub timers: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptMidiRuleDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptMidiEndpointDiagnostics {
    pub id: String,
    pub name: String,
    pub connected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptMidiRuleDiagnostics {
    pub direction: ScriptMidiRuleDirection,
    pub pattern: String,
    pub matched_endpoints: Arc<[String]>,
    pub connected_endpoints: Arc<[String]>,
    pub endpoints: Arc<[ScriptMidiEndpointDiagnostics]>,
    pub latest_error: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScriptMidiDiagnostics {
    pub rules: u32,
    pub connections: u32,
    pub dropped_messages: u32,
    pub errors: u32,
    pub rule_states: Arc<[ScriptMidiRuleDiagnostics]>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScriptDialogRichTextStyle {
    pub strong: bool,
    pub italics: bool,
    pub monospace: bool,
    pub underline: bool,
    pub strikethrough: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptDialogMarkdownLink {
    pub destination: String,
    pub callback_id: ScriptDialogButtonId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScriptDialogElement {
    RichText {
        text: String,
        style: ScriptDialogRichTextStyle,
    },
    Markdown {
        text: String,
        links: Arc<[ScriptDialogMarkdownLink]>,
        resource_base_uri: Option<Arc<str>>,
    },
    Button {
        id: Option<ScriptDialogButtonId>,
        label: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptDialogContent {
    pub elements: Arc<[ScriptDialogElement]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScriptDialogKind {
    Simple(ScriptDialogContent),
    Paged(Arc<[ScriptDialogContent]>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptDialogState {
    pub id: ScriptDialogId,
    pub owner_script_id: ScriptId,
    pub owner_script_name: String,
    pub name: String,
    pub kind: ScriptDialogKind,
    pub open_request: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptState {
    pub id: ScriptId,
    pub name: String,
    pub identity: Option<Arc<str>>,
    pub kind: ScriptKind,
    pub enabled: bool,
    pub lifecycle: ScriptLifecycle,
    pub documentation: Option<String>,
    pub resource_base_uri: Option<Arc<str>>,
    pub latest_error: Option<String>,
    pub activity: ScriptActivityDiagnostics,
    pub midi: ScriptMidiDiagnostics,
    pub logs: Arc<[ScriptLogState]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScriptingState {
    pub supported: bool,
    pub api_version: LuaApiVersion,
    pub scripts: Arc<[ScriptState]>,
    pub dialogs: Arc<[ScriptDialogState]>,
}

impl Default for ScriptingState {
    fn default() -> Self {
        Self {
            supported: false,
            api_version: LUA_API_VERSION,
            scripts: Arc::from([]),
            dialogs: Arc::from([]),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClickTrackPreviewOutput {
    System,
    MasterBus,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClickTrackPreviewStatus {
    Cancelled,
    #[default]
    Idle,
    Queued,
    Playing,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClickSoundDescriptor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClickTrackState {
    pub preview_output: Option<ClickTrackPreviewOutput>,
    pub master_preview_available: bool,
    pub sounds: Arc<[ClickSoundDescriptor]>,
    pub max_click_count: u32,
    pub max_output_frames: u32,
    pub preview_request_id: u64,
    pub preview_status: ClickTrackPreviewStatus,
    pub preview_message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackCreationResult {
    pub request_id: u64,
    pub success: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusCreationResult {
    pub request_id: u64,
    pub success: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusSpec {
    pub name: String,
    pub channel_count: u32,
    pub fx: Option<BusFxSpec>,
    pub creation_request_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BusFxSpec {
    pub processor_type: TrackProcessorTypeId,
}

/// An optional backend feature. Every feature outside the core looping workflow is one of these.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BackendCapability {
    SessionCapture,
    /// Loading sessions whose loops carry content. Structure-only documents need no capability.
    SessionContentLoad,
    /// Prepared composite arrangements executed by the session engine.
    CompositeLoops,
    Grab,
    ReplacingMode,
    /// Playing dry through wet and recording dry into wet, as loop modes and track defaults.
    DryWetModes,
    /// Dry/wet tracks, with an external or a hosted processor.
    DryWetTracks,
    TrackFx,
    BusFx,
    LatencyCompensation,
    TakeAlignment,
    /// Replacing loop content from standalone audio/MIDI imports.
    LoopContentImport,
    /// Editing loop length, start offset and pre-play.
    LoopTimingEdits,
    /// Creating and removing buses other than Master.
    BusManagement,
    AudioDriverSwitching,
    MidiInjection,
    LoopDuplication,
    ClickTrackInsertion,
}

impl BackendCapability {
    pub const ALL: [Self; 18] = [
        Self::SessionCapture,
        Self::SessionContentLoad,
        Self::CompositeLoops,
        Self::Grab,
        Self::ReplacingMode,
        Self::DryWetModes,
        Self::DryWetTracks,
        Self::TrackFx,
        Self::BusFx,
        Self::LatencyCompensation,
        Self::TakeAlignment,
        Self::LoopContentImport,
        Self::LoopTimingEdits,
        Self::BusManagement,
        Self::AudioDriverSwitching,
        Self::MidiInjection,
        Self::LoopDuplication,
        Self::ClickTrackInsertion,
    ];

    const fn bit(self) -> u32 {
        1 << self as u32
    }

    pub const fn unavailable_reason(self) -> &'static str {
        match self {
            Self::SessionCapture => "The session engine cannot currently save sessions.",
            Self::SessionContentLoad => {
                "The session engine cannot currently load sessions with loop content."
            }
            Self::CompositeLoops => "The session engine does not yet support composite loops.",
            Self::Grab => "The session engine does not yet support grabbing.",
            Self::ReplacingMode => "The session engine does not yet support replacing.",
            Self::DryWetModes => "The session engine does not yet support the dry/wet loop modes.",
            Self::DryWetTracks => "The session engine does not yet support dry/wet tracks.",
            Self::TrackFx => "The session engine does not yet support track FX.",
            Self::BusFx => "The session engine does not yet support bus FX.",
            Self::LatencyCompensation => {
                "The session engine does not yet support latency compensation."
            }
            Self::TakeAlignment => "The session engine does not yet support take alignment.",
            Self::LoopContentImport => "The session engine cannot currently replace loop content.",
            Self::LoopTimingEdits => {
                "The session engine does not yet support editing loop length or timing."
            }
            Self::BusManagement => "Additional buses are temporarily unavailable in the session engine.",
            Self::AudioDriverSwitching => {
                "The session engine cannot currently switch audio drivers at runtime; restart with other settings instead."
            }
            Self::MidiInjection => "The session engine does not yet support MIDI input injection.",
            Self::LoopDuplication => "The session engine does not yet support loop duplication.",
            Self::ClickTrackInsertion => "The session engine does not yet support generated click-track insertion.",
        }
    }
}

/// The set of optional features a backend supports. The application presents the others as
/// unavailable, refuses them and rejects documents that need them. Defaults to every feature.
#[derive(Clone, Copy, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BackendCapabilities {
    supported: u32,
}

impl BackendCapabilities {
    pub const FULL: Self = {
        let mut supported = 0;
        let mut index = 0;
        while index < BackendCapability::ALL.len() {
            supported |= BackendCapability::ALL[index].bit();
            index += 1;
        }
        Self { supported }
    };
    pub const NONE: Self = Self { supported: 0 };

    pub const fn supports(self, capability: BackendCapability) -> bool {
        self.supported & capability.bit() != 0
    }

    #[must_use]
    pub const fn with(self, capability: BackendCapability, supported: bool) -> Self {
        Self {
            supported: if supported {
                self.supported | capability.bit()
            } else {
                self.supported & !capability.bit()
            },
        }
    }

    pub fn require(self, capability: BackendCapability) -> Result<(), String> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(capability.unavailable_reason().to_owned())
        }
    }

    pub fn unsupported(self) -> impl Iterator<Item = BackendCapability> {
        BackendCapability::ALL
            .into_iter()
            .filter(move |capability| !self.supports(*capability))
    }
}

impl Default for BackendCapabilities {
    fn default() -> Self {
        Self::FULL
    }
}

impl fmt::Debug for BackendCapabilities {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_set()
            .entries(
                BackendCapability::ALL
                    .into_iter()
                    .filter(|capability| self.supports(*capability)),
            )
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationLevel {
    Warning,
    Error,
}

/// A message for the user about a refused or failed action, shown until dismissed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppNotification {
    pub id: u64,
    pub level: NotificationLevel,
    pub message: String,
}

pub use rt_profiling as profiling_data;
pub use rt_profiling::View as ProfilingView;

#[derive(Clone, Debug, PartialEq)]
pub struct ProfilingSource {
    pub descriptor: profiling_protocol::Descriptor,
    pub status: Option<profiling_protocol::Status>,
    pub age_ms: u64,
    pub assembly_losses: u64,
    pub supported: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DiagnosticsView {
    pub sources: Vec<ProfilingSource>,
    pub tasks: freertos_profiling::View,
}
pub use freertos_profiling as task_profiling;
pub use profiling_protocol as diagnostics_protocol;

#[derive(Clone, Debug, Default)]
pub struct AppSnapshot {
    pub diagnostics: DiagnosticsView,
    pub profiling: ProfilingView,
    pub bus_limits: BusLimits,
    pub revision: u64,
    pub tracks: Vec<TrackState>,
    pub buses: Arc<[BusState]>,
    pub track_processors: Arc<[TrackProcessorDescriptor]>,
    pub bus_processors: Arc<[TrackProcessorDescriptor]>,
    pub track_creation_results: Arc<[TrackCreationResult]>,
    pub bus_creation_results: Arc<[BusCreationResult]>,
    pub global_controls: GlobalControlState,
    pub status: StatusState,
    pub audio_drivers: AudioDriverRuntimeState,
    pub capabilities: BackendCapabilities,
    pub notifications: Arc<[AppNotification]>,
    pub details: Option<LoopDetailsState>,
    pub connections: Arc<ConnectionViewState>,
    pub host_graph: Arc<HostGraphState>,
    pub scripting: Arc<ScriptingState>,
    pub click_track: ClickTrackState,
    pub io_task: Option<IoTaskState>,
}

pub type AppState = AppSnapshot;

pub const MAX_TRACK_LATENCY_FRAMES: i32 = shoop_latency::MAX_COMPENSATION_FRAMES as i32;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackLatencySpec {
    pub adjustment: RecordingOffsetAdjustmentState,
    pub manual_frames: i32,
    pub processor_adjustment: ProcessorLatencyAdjustmentState,
    pub processor_manual_frames: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TrackSpec {
    pub name: String,
    pub topology: TrackSpecTopology,
    pub latency: TrackLatencySpec,
    pub initial_output_bus_names: Vec<String>,
    pub creation_request_id: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackSpecTopology {
    Direct {
        audio_channels: u32,
        midi: bool,
    },
    DryWet {
        dry_audio_channels: u32,
        wet_audio_channels: u32,
        dry_midi: bool,
        processor_type: TrackProcessorTypeId,
        default_playback_mode: DefaultPlaybackMode,
    },
}

impl TrackSpecTopology {
    pub fn channel_roles(&self) -> Vec<TrackChannelRole> {
        match self {
            Self::Direct {
                audio_channels,
                midi,
            } => (0..*audio_channels)
                .map(TrackChannelRole::DirectAudio)
                .chain((*midi).then_some(TrackChannelRole::DirectMidi))
                .collect(),
            Self::DryWet {
                dry_audio_channels,
                wet_audio_channels,
                dry_midi,
                ..
            } => (0..*dry_audio_channels)
                .map(TrackChannelRole::DryAudio)
                .chain((*dry_midi).then_some(TrackChannelRole::DryMidi))
                .chain((0..*wet_audio_channels).map(TrackChannelRole::WetAudio))
                .collect(),
        }
    }
}

impl TrackLatencySpec {
    fn validate(self) -> Result<(), TrackSpecError> {
        if self.adjustment != RecordingOffsetAdjustmentState::Automatic
            && shoop_latency::RecordingOffset::new(self.manual_frames).is_err()
        {
            return Err(TrackSpecError::InvalidLatency);
        }
        match self.processor_adjustment {
            ProcessorLatencyAdjustmentState::Automatic => {}
            ProcessorLatencyAdjustmentState::ManualOverride => {
                let Ok(frames) = u32::try_from(self.processor_manual_frames) else {
                    return Err(TrackSpecError::InvalidLatency);
                };
                if shoop_latency::ProcessorRenderAdvance::new(frames).is_err() {
                    return Err(TrackSpecError::InvalidLatency);
                }
            }
            ProcessorLatencyAdjustmentState::AutomaticPlusTrim => {
                if self.processor_manual_frames.unsigned_abs()
                    > shoop_latency::MAX_COMPENSATION_FRAMES
                {
                    return Err(TrackSpecError::InvalidLatency);
                }
            }
        }
        Ok(())
    }
}

impl TrackSpec {
    pub fn validate(&self, processors: &[TrackProcessorDescriptor]) -> Result<(), TrackSpecError> {
        if self.name.trim().is_empty() {
            return Err(TrackSpecError::EmptyName);
        }
        self.latency.validate()?;
        let TrackSpecTopology::DryWet {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            processor_type,
            default_playback_mode,
        } = &self.topology
        else {
            return Ok(());
        };
        if *default_playback_mode == DefaultPlaybackMode::DryThroughWet && *wet_audio_channels == 0
        {
            return Err(TrackSpecError::UnsupportedShape);
        }
        let processor = processors
            .iter()
            .find(|candidate| candidate.id == *processor_type)
            .ok_or(TrackSpecError::ProcessorUnavailable)?;
        if !processor.available {
            return Err(TrackSpecError::ProcessorUnavailable);
        }
        if !processor
            .constraints
            .accepts(*dry_audio_channels, *wet_audio_channels, *dry_midi)
        {
            return Err(TrackSpecError::UnsupportedShape);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackSpecError {
    EmptyName,
    InvalidLatency,
    ProcessorUnavailable,
    UnsupportedShape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirectTrackSpec {
    pub name: String,
    pub audio_channels: u32,
    pub midi: bool,
}

impl From<DirectTrackSpec> for TrackSpec {
    fn from(value: DirectTrackSpec) -> Self {
        Self {
            name: value.name,
            topology: TrackSpecTopology::Direct {
                audio_channels: value.audio_channels,
                midi: value.midi,
            },
            latency: TrackLatencySpec::default(),
            initial_output_bus_names: Vec::new(),
            creation_request_id: None,
        }
    }
}

impl DirectTrackSpec {
    pub fn validate(&self) -> Result<(), DirectTrackSpecError> {
        if self.name.trim().is_empty() {
            return Err(DirectTrackSpecError::EmptyName);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectTrackSpecError {
    EmptyName,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SelectionModifiers {
    pub additive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopAudioExportFormat {
    Exact,
    FloatWav,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ClickTrackKind {
    #[default]
    Audio,
    Midi,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ClickTrackRequest {
    pub kind: ClickTrackKind,
    pub primary_sound_id: String,
    pub secondary_sound_id: Option<String>,
    pub secondary_clicks_per_primary: u32,
    pub bpm: f64,
    pub click_count: u32,
    pub odd_click_delay_percent: f64,
    pub midi_note: u8,
    pub midi_note_length_seconds: f64,
}

impl Default for ClickTrackRequest {
    fn default() -> Self {
        Self {
            kind: ClickTrackKind::Audio,
            primary_sound_id: "click_high".to_owned(),
            secondary_sound_id: Some("click_low".to_owned()),
            secondary_clicks_per_primary: 3,
            bpm: 100.0,
            click_count: 4,
            odd_click_delay_percent: 0.0,
            midi_note: 64,
            midi_note_length_seconds: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LoopAction {
    NameChanged(String),
    IconClicked(SelectionModifiers),
    IconDoubleClicked,
    DefaultClicked,
    PlayClicked,
    PlayDryClicked,
    RecordClicked,
    GrabClicked,
    RerecordClicked,
    ReplaceClicked,
    StopClicked,
    GainChanged(f32),
    BalanceChanged(f32),
    RestoreRecordedFxState,
    ConvertToComposite,
    Duplicate,
    DuplicateTo(LoopId),
    ComposeIntoEnd(LoopId),
    ComposeIntoParallel(LoopId),
    SwapWith(LoopId),
    MoveBefore(Option<LoopId>),
}

pub type LoopWidgetAction = LoopAction;

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BuiltInFxControl {
    SetStageEnabled(BuiltInFxStage, bool),
    SetDriveType(BuiltInFxDriveType),
    SetModulationType(BuiltInFxModulationType),
    SetReverbType(BuiltInFxReverbType),
    SetParameter(BuiltInFxParameter, f32),
    AssignMidiCc(BuiltInFxMidiCcAssignment),
    RemoveMidiCc(BuiltInFxParameter),
    ClearMidiCcAssignments,
    SetReverbEnabled(bool),
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum OxiSynthControl {
    SelectPreset(String),
    SetReverbSend(f32),
    SetChorusSend(f32),
    AssignMidiCc(OxiSynthMidiCcAssignment),
    RemoveMidiCc(OxiSynthParameter),
    ClearMidiCcAssignments,
    Panic,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TrackAction {
    Remove,
    MoveBefore(Option<TrackId>),
    NameChanged(String),
    DefaultPlaybackModeChanged(DefaultPlaybackMode),
    OutputGainChanged(f32),
    OutputBalanceChanged(f32),
    OutputMuteChanged(bool),
    InputGainChanged(f32),
    InputBalanceChanged(f32),
    InputMonitoringChanged {
        enabled: bool,
        respect_auto_mute: bool,
    },
    FxActiveChanged(bool),
    FxVisibilityChanged(bool),
    FxToggleOrRecover,
    FxRestoreState(String),
    FxClearLogs,
    BuiltInFx(BuiltInFxControl),
    OxiSynth(OxiSynthControl),
}

pub type TrackWidgetAction = TrackAction;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BusAction {
    Remove,
    MoveBefore(Option<BusId>),
    GainChanged(f32),
    BalanceChanged(f32),
    MuteChanged(bool),
    FxActiveChanged(bool),
    FxVisibilityChanged(bool),
    FxToggleOrRecover,
    FxRestoreState(String),
    FxClearLogs,
    BuiltInFx(BuiltInFxControl),
    FxProcessorChanged(Option<TrackProcessorTypeId>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GlobalControlAction {
    StopAll,
    MidiPanic,
    DeselectAll,
    ClearRecordings { include_sync: bool },
    ClearAll { include_sync: bool },
    SetDefaultRecordingAction(DefaultRecordingAction),
    SetPlayAfterRecord(bool),
    SetSync(bool),
    SetSolo(bool),
    SetAutoMuteOtherTrackInputs(bool),
    SetAutoArmTrackInputs(bool),
    SetApplyNCycles(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiNote(u8);

#[cfg(feature = "serde")]
impl serde::Serialize for MidiNote {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for MidiNote {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <u8 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("invalid MIDI note"))
    }
}

impl MidiNote {
    pub const fn new(value: u8) -> Option<Self> {
        if value <= 127 {
            Some(Self(value))
        } else {
            None
        }
    }

    pub const fn value(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PianoAction {
    Press(MidiNote),
    Release(MidiNote),
    ReleaseAll,
}

pub const MAX_LOOP_CHANNEL_TIMING_UPDATES: usize = 17;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChannelTimelineUpdate {
    pub channel_id: ChannelId,
    pub start_offset: Option<i64>,
    pub preplay_samples: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppIntent {
    ControlProfiling(profiling_protocol::Request),
    SetProfiling(bool),
    InspectProfiling(profiling_data::CaptureRequest),
    SetLoopSmoothingMs(u32),
    SetMasterAutoConnect(bool),
    SetLoopTimeline {
        loop_id: LoopId,
        start_offset: Option<i64>,
        preplay_samples: Option<u64>,
        loop_length: Option<u64>,
    },
    SetLoopChannelTimings {
        loop_id: LoopId,
        updates: Vec<ChannelTimelineUpdate>,
    },
    Loop {
        track_id: TrackId,
        loop_id: LoopId,
        action: LoopAction,
    },
    Track {
        track_id: TrackId,
        action: TrackAction,
    },
    Bus {
        bus_id: BusId,
        action: BusAction,
    },
    SetTrackLatency {
        track_id: TrackId,
        adjustment: RecordingOffsetAdjustmentState,
        manual_frames: i32,
        processor_adjustment: ProcessorLatencyAdjustmentState,
        processor_manual_frames: i32,
    },
    SetTakeAlignment {
        loop_id: LoopId,
        capture_alignment_frames: i32,
    },
    SetTakeProcessorAlignment {
        loop_id: LoopId,
        processor_alignment_frames: u32,
    },
    Global(GlobalControlAction),
    Piano(PianoAction),
    AddTrack(DirectTrackSpec),
    AddTrackWithTopology(TrackSpec),
    AddBus(BusSpec),
    AddLoop {
        track_id: TrackId,
    },
    ComposeLoopSerial {
        target_loop_id: LoopId,
        source_loop_id: LoopId,
    },
    ComposeLoopAt {
        target_loop_id: LoopId,
        source_loop_id: LoopId,
        start_iteration: u64,
    },
    DeleteCompositeEvents {
        target_loop_id: LoopId,
        events: Vec<CompositeEventId>,
    },
    RelocateCompositeEvents {
        target_loop_id: LoopId,
        events: Vec<CompositeEventId>,
        start_iteration: u64,
        duplicate: bool,
    },
    SetCompositeLoopCycles {
        target_loop_id: LoopId,
        event: CompositeEventId,
        n_cycles: Option<u32>,
    },
    SetCompositeKind {
        target_loop_id: LoopId,
        kind: CompositeKind,
    },
    SetCompositeTimingBasis {
        target_loop_id: LoopId,
        sample_rate: u32,
        unit_frames: u64,
    },
    CancelCompositeEdit {
        target_loop_id: LoopId,
        version: u64,
    },
    SetCompositeEventMode {
        target_loop_id: LoopId,
        event: CompositeEventId,
        mode: LoopMode,
    },
    KeyEvent(KeyEvent),
    AddScriptSource {
        name: String,
        source: Arc<str>,
        kind: ScriptKind,
        enabled: bool,
    },
    AddScriptFileSource {
        name: String,
        source: Arc<str>,
        source_path: String,
        kind: ScriptKind,
        enabled: bool,
    },
    AddEphemeralScript {
        name: String,
        source: Arc<str>,
        source_path: Option<String>,
    },
    ReconcileCatalogScripts {
        scripts: Arc<[CatalogScriptSource]>,
        preserve_identities: Arc<[String]>,
    },
    SetScriptEnabled {
        script_id: ScriptId,
        enabled: bool,
    },
    RestartScript {
        script_id: ScriptId,
    },
    ReplaceScriptSource {
        script_id: ScriptId,
        source: Arc<str>,
    },
    StopScript {
        script_id: ScriptId,
    },
    ForgetScript {
        script_id: ScriptId,
    },
    ExportScript {
        script_id: ScriptId,
    },
    ConvertScriptKind {
        script_id: ScriptId,
        kind: ScriptKind,
    },
    RemoveSessionScript {
        script_id: ScriptId,
    },
    InvokeScriptDialogButton {
        script_id: ScriptId,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    },
    SetHostGraphConnected(HostGraphRequest),
    SetPortConnected {
        port_id: PortId,
        host_port_id: HostPortId,
        connected: bool,
    },
    SetMixerRouteConnected {
        source_port_id: PortId,
        destination_channel_id: BusChannelId,
        connected: bool,
    },
    RefreshAudioDriverDiscovery {
        config: AudioDriverConfig,
    },
    RequestAudioDriverSwitch {
        config: AudioDriverConfig,
    },
    ConfirmAudioDriverSwitch {
        request_id: u64,
        accept: bool,
    },
    CompleteAudioDriverSwitchPersistence {
        request_id: u64,
        success: bool,
        message: String,
    },
    RetryAudioDriverPersistence {
        request_id: u64,
    },
    ResetXruns,
    DismissNotification {
        id: u64,
    },
    RequestNewSession,
    RequestSaveSession,
    RequestLoadSessionPicker,
    RequestLoadSessionUrl,
    RequestLoadSessionSource {
        source: String,
    },
    LoadSessionBytes {
        name: String,
        bytes: Arc<Vec<u8>>,
    },
    CompleteIoInput {
        task_id: TaskId,
        name: String,
        bytes: Arc<Vec<u8>>,
    },
    CompleteIoOutput {
        task_id: TaskId,
        outcome: FileOutputOutcome,
    },
    ConfirmFormatConversion {
        task_id: TaskId,
        accept: bool,
    },
    ConfirmAudioChannelMapping {
        task_id: TaskId,
        source_for_destination: Vec<u32>,
    },
    ConfirmAudioChannelSelection {
        task_id: TaskId,
        channels: Vec<u32>,
    },
    CancelIoTask {
        task_id: TaskId,
    },
    FailIoTask {
        task_id: TaskId,
        message: String,
    },
    FailIoWorkflow {
        kind: IoTaskKind,
        message: String,
    },
    PreviewClickTrack {
        output: ClickTrackPreviewOutput,
        loop_id: LoopId,
        request: ClickTrackRequest,
    },
    StopClickTrackPreview,
    StartMasterClickTrackPreview {
        request_id: u64,
        sample_rate: u32,
        samples: Arc<[f32]>,
    },
    ClickTrackPreviewStarted {
        request_id: u64,
    },
    CompleteClickTrackPreview {
        request_id: u64,
        success: bool,
        message: String,
    },
    GenerateClickTrack {
        loop_id: LoopId,
        request: ClickTrackRequest,
    },
    RequestLoopAudioExport {
        loop_id: LoopId,
        format: LoopAudioExportFormat,
    },
    RequestLoopAudioImportPicker {
        loop_id: LoopId,
    },
    ImportLoopAudioBytes {
        loop_id: LoopId,
        name: String,
        bytes: Arc<Vec<u8>>,
        update_loop_length: bool,
    },
    RequestLoopMidiExport {
        loop_id: LoopId,
        standard: bool,
    },
    RequestLoopMidiImportPicker {
        loop_id: LoopId,
    },
    ImportLoopMidiBytes {
        loop_id: LoopId,
        name: String,
        bytes: Arc<Vec<u8>>,
        update_loop_length: bool,
    },
}

impl LoopAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NameChanged(_) => "loop.name",
            Self::IconClicked(_) => "loop.icon_clicked",
            Self::IconDoubleClicked => "loop.icon_double_clicked",
            Self::DefaultClicked => "loop.default",
            Self::PlayClicked => "loop.play",
            Self::PlayDryClicked => "loop.play_dry",
            Self::RecordClicked => "loop.record",
            Self::GrabClicked => "loop.grab",
            Self::RerecordClicked => "loop.rerecord",
            Self::ReplaceClicked => "loop.replace",
            Self::StopClicked => "loop.stop",
            Self::GainChanged(_) => "loop.gain",
            Self::BalanceChanged(_) => "loop.balance",
            Self::RestoreRecordedFxState => "loop.restore_recorded_fx",
            Self::ConvertToComposite => "loop.convert_to_composite",
            Self::Duplicate => "loop.duplicate",
            Self::DuplicateTo(_) => "loop.duplicate_to",
            Self::ComposeIntoEnd(_) => "loop.compose_into_end",
            Self::ComposeIntoParallel(_) => "loop.compose_into_parallel",
            Self::SwapWith(_) => "loop.swap_with",
            Self::MoveBefore(_) => "loop.move_before",
        }
    }
}

impl BuiltInFxControl {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SetStageEnabled(_, _) => "track.builtin_fx.stage_enabled",
            Self::SetDriveType(_) => "track.builtin_fx.drive_type",
            Self::SetModulationType(_) => "track.builtin_fx.modulation_type",
            Self::SetReverbType(_) => "track.builtin_fx.reverb_type",
            Self::SetParameter(_, _) => "track.builtin_fx.parameter",
            Self::AssignMidiCc(_) => "track.builtin_fx.midi_cc_assign",
            Self::RemoveMidiCc(_) => "track.builtin_fx.midi_cc_remove",
            Self::ClearMidiCcAssignments => "track.builtin_fx.midi_cc_clear",
            Self::SetReverbEnabled(_) => "track.builtin_fx.reverb_enabled",
        }
    }
}

impl OxiSynthControl {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SelectPreset(_) => "track.oxisynth.select_preset",
            Self::SetReverbSend(_) => "track.oxisynth.reverb_send",
            Self::SetChorusSend(_) => "track.oxisynth.chorus_send",
            Self::AssignMidiCc(_) => "track.oxisynth.midi_cc_assign",
            Self::RemoveMidiCc(_) => "track.oxisynth.midi_cc_remove",
            Self::ClearMidiCcAssignments => "track.oxisynth.midi_cc_clear",
            Self::Panic => "track.oxisynth.panic",
        }
    }
}

impl TrackAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Remove => "track.remove",
            Self::MoveBefore(_) => "track.move_before",
            Self::NameChanged(_) => "track.name",
            Self::DefaultPlaybackModeChanged(_) => "track.default_playback_mode",
            Self::OutputGainChanged(_) => "track.output_gain",
            Self::OutputBalanceChanged(_) => "track.output_balance",
            Self::OutputMuteChanged(_) => "track.output_mute",
            Self::InputGainChanged(_) => "track.input_gain",
            Self::InputBalanceChanged(_) => "track.input_balance",
            Self::InputMonitoringChanged { .. } => "track.input_monitoring",
            Self::FxActiveChanged(_) => "track.fx_active",
            Self::FxVisibilityChanged(_) => "track.fx_visibility",
            Self::FxToggleOrRecover => "track.fx_toggle_or_recover",
            Self::FxRestoreState(_) => "track.fx_restore_state",
            Self::FxClearLogs => "track.fx_clear_logs",
            Self::BuiltInFx(control) => control.kind(),
            Self::OxiSynth(control) => control.kind(),
        }
    }
}

impl BusAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Remove => "bus.remove",
            Self::MoveBefore(_) => "bus.move_before",
            Self::GainChanged(_) => "bus.gain",
            Self::BalanceChanged(_) => "bus.balance",
            Self::MuteChanged(_) => "bus.mute",
            Self::FxActiveChanged(_) => "bus.fx_active",
            Self::FxVisibilityChanged(_) => "bus.fx_visibility",
            Self::FxToggleOrRecover => "bus.fx_toggle_or_recover",
            Self::FxRestoreState(_) => "bus.fx_restore_state",
            Self::FxClearLogs => "bus.fx_clear_logs",
            Self::BuiltInFx(control) => control.kind(),
            Self::FxProcessorChanged(_) => "bus.fx_processor",
        }
    }
}

impl GlobalControlAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::StopAll => "global.stop_all",
            Self::MidiPanic => "global.midi_panic",
            Self::DeselectAll => "global.deselect_all",
            Self::ClearRecordings { .. } => "global.clear_recordings",
            Self::ClearAll { .. } => "global.clear_all",
            Self::SetDefaultRecordingAction(_) => "global.default_recording_action",
            Self::SetPlayAfterRecord(_) => "global.play_after_record",
            Self::SetSync(_) => "global.sync",
            Self::SetSolo(_) => "global.solo",
            Self::SetAutoMuteOtherTrackInputs(_) => "global.auto_mute_other_track_inputs",
            Self::SetAutoArmTrackInputs(_) => "global.auto_arm_track_inputs",
            Self::SetApplyNCycles(_) => "global.apply_n_cycles",
        }
    }
}

impl PianoAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Press(_) => "piano.press",
            Self::Release(_) => "piano.release",
            Self::ReleaseAll => "piano.release_all",
        }
    }
}

impl AppIntent {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SetProfiling(_) | Self::InspectProfiling(_) | Self::ControlProfiling(_) => {
                "developer.profiling"
            }
            Self::SetLoopSmoothingMs(_) => "audio.loop_smoothing",
            Self::SetMasterAutoConnect(_) => "audio.master_auto_connect",
            Self::SetLoopTimeline { .. } | Self::SetLoopChannelTimings { .. } => "loop.timeline",
            Self::Loop { action, .. } => action.kind(),
            Self::Track { action, .. } => action.kind(),
            Self::Bus { action, .. } => action.kind(),
            Self::SetTrackLatency { .. } => "track.latency",
            Self::SetTakeAlignment { .. } => "loop.capture_alignment",
            Self::SetTakeProcessorAlignment { .. } => "loop.processor_alignment",
            Self::Global(action) => action.kind(),
            Self::Piano(action) => action.kind(),
            Self::AddTrack(_) => "track.add_direct",
            Self::AddTrackWithTopology(_) => "track.add_with_topology",
            Self::AddBus(_) => "bus.add",
            Self::AddLoop { .. } => "loop.add_row",
            Self::ComposeLoopSerial { .. } => "loop.compose_serial",
            Self::ComposeLoopAt { .. } => "loop.compose_at",
            Self::DeleteCompositeEvents { .. } => "loop.composite.delete_events",
            Self::RelocateCompositeEvents { .. } => "loop.composite.relocate_events",
            Self::SetCompositeLoopCycles { .. } => "loop.composite.set_loop_cycles",
            Self::SetCompositeKind { .. } => "loop.composite.set_kind",
            Self::SetCompositeEventMode { .. } => "loop.composite.set_event_mode",
            Self::SetCompositeTimingBasis { .. } => "loop.composite.set_timing_basis",
            Self::CancelCompositeEdit { .. } => "loop.composite.cancel_edit",
            Self::KeyEvent(_) => "scripting.key_event",
            Self::AddScriptSource { .. } => "scripting.add_source",
            Self::AddScriptFileSource { .. } => "scripting.add_file_source",
            Self::AddEphemeralScript { .. } => "scripting.add_ephemeral",
            Self::ReconcileCatalogScripts { .. } => "scripting.reconcile_catalog",
            Self::SetScriptEnabled { .. } => "scripting.set_enabled",
            Self::RestartScript { .. } => "scripting.restart",
            Self::ReplaceScriptSource { .. } => "scripting.replace_source",
            Self::StopScript { .. } => "scripting.stop",
            Self::ForgetScript { .. } => "scripting.forget",
            Self::ExportScript { .. } => "scripting.export",
            Self::ConvertScriptKind { .. } => "scripting.convert_kind",
            Self::RemoveSessionScript { .. } => "scripting.remove_session",
            Self::InvokeScriptDialogButton { .. } => "scripting.dialog_button",
            Self::SetHostGraphConnected(_) => "host_graph.set",
            Self::SetPortConnected { .. } => "connection.set",
            Self::SetMixerRouteConnected { .. } => "mixer.connection.set",
            Self::RefreshAudioDriverDiscovery { .. } => "audio_driver.refresh_discovery",
            Self::RequestAudioDriverSwitch { .. } => "audio_driver.request_switch",
            Self::ConfirmAudioDriverSwitch { .. } => "audio_driver.confirm_switch",
            Self::CompleteAudioDriverSwitchPersistence { .. } => {
                "audio_driver.complete_persistence"
            }
            Self::RetryAudioDriverPersistence { .. } => "audio_driver.retry_persistence",
            Self::ResetXruns => "audio.reset_xruns",
            Self::DismissNotification { .. } => "app.dismiss_notification",
            Self::RequestNewSession => "session.request_new",
            Self::RequestSaveSession => "session.request_save",
            Self::RequestLoadSessionPicker => "session.request_load_picker",
            Self::RequestLoadSessionUrl => "session.request_load_url",
            Self::LoadSessionBytes { .. } => "session.load_bytes",
            Self::ConfirmFormatConversion { .. } => "io.confirm_format_conversion",
            Self::ConfirmAudioChannelMapping { .. } => "io.confirm_channel_mapping",
            Self::CompleteIoInput { .. } => "io.input_complete",
            Self::RequestLoadSessionSource { .. } => "session.load_source",
            Self::CompleteIoOutput { .. } => "io.output_complete",
            Self::ConfirmAudioChannelSelection { .. } => "io.confirm_channel_selection",
            Self::CancelIoTask { .. } => "io.cancel",
            Self::FailIoTask { .. } => "io.fail",
            Self::FailIoWorkflow { .. } => "io.fail_workflow",
            Self::PreviewClickTrack { .. } => "click_track.preview",
            Self::StopClickTrackPreview => "click_track.preview.stop",
            Self::StartMasterClickTrackPreview { .. } => "click_track.preview.master",
            Self::ClickTrackPreviewStarted { .. } => "click_track.preview.started",
            Self::CompleteClickTrackPreview { .. } => "click_track.complete_preview",
            Self::GenerateClickTrack { .. } => "click_track.generate",
            Self::RequestLoopAudioExport { .. } => "loop_audio.request_export",
            Self::RequestLoopAudioImportPicker { .. } => "loop_audio.request_import_picker",
            Self::ImportLoopAudioBytes { .. } => "loop_audio.import_bytes",
            Self::RequestLoopMidiExport { .. } => "loop_midi.request_export",
            Self::RequestLoopMidiImportPicker { .. } => "loop_midi.request_import_picker",
            Self::ImportLoopMidiBytes { .. } => "loop_midi.import_bytes",
        }
    }
}

pub type AppAction = AppIntent;

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[shoop_wasm_test_support::shoop_test]
    fn backend_capabilities_toggle_each_capability_independently() {
        assert_eq!(BackendCapabilities::default(), BackendCapabilities::FULL);
        assert_eq!(BackendCapabilities::FULL.unsupported().count(), 0);
        assert_eq!(
            BackendCapabilities::NONE.unsupported().collect::<Vec<_>>(),
            BackendCapability::ALL
        );
        for capability in BackendCapability::ALL {
            let reduced = BackendCapabilities::FULL.with(capability, false);
            assert!(!reduced.supports(capability));
            assert_eq!(reduced.unsupported().collect::<Vec<_>>(), [capability]);
            assert_eq!(
                reduced.require(capability),
                Err(capability.unavailable_reason().to_owned())
            );
            assert_eq!(reduced.with(capability, true), BackendCapabilities::FULL);
            assert!(BackendCapabilities::NONE
                .with(capability, true)
                .require(capability)
                .is_ok());
        }
        assert_eq!(
            format!(
                "{:?}",
                BackendCapabilities::NONE.with(BackendCapability::Grab, true)
            ),
            "{Grab}"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bus_state_derives_stereo_capability_from_ordered_channel_count() {
        let channel = |id| BusChannelState {
            id: BusChannelId::from_raw(id),
            label: id.to_string(),
            output_port_id: PortId::from_raw(id),
        };
        let mut state = BusState {
            id: BusId::from_raw(1),
            name: "Master".to_owned(),
            structural_state: StructuralState::Confirmed,
            structural_error: None,
            channels: Arc::from([channel(1)]),
            gain_db: 0.0,
            balance: 0.0,
            muted: false,
            output_peaks_db: Arc::from([-200.0]),
            control_pending: false,
            control_error: None,
            fx: None,
        };
        assert!(!state.stereo());
        state.channels = Arc::from([channel(1), channel(2)]);
        state.output_peaks_db = Arc::from([-12.0, -6.0]);
        assert!(state.stereo());
        state.channels = Arc::from([channel(1), channel(2), channel(3)]);
        assert!(!state.stereo());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ephemeral_script_names_track_source_versions_without_colliding() {
        assert!(is_ephemeral_script_version(
            "controller.lua",
            "controller.lua"
        ));
        assert!(is_ephemeral_script_version(
            "controller.lua (run once 2)",
            "controller.lua"
        ));
        assert!(!is_ephemeral_script_version(
            "controller.lua (copy)",
            "controller.lua"
        ));
        assert_eq!(
            ephemeral_script_display_name("controller.lua", core::iter::empty()),
            "controller.lua"
        );
        assert_eq!(
            ephemeral_script_display_name(
                "controller.lua",
                ["controller.lua", "controller.lua (run once 2)"]
            ),
            "controller.lua (run once 3)"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ids_retain_raw_identity_and_invalid_is_distinct() {
        let first = TrackId::from_raw(10);
        let second = TrackId::from_raw(11);
        assert!(first.is_valid());
        assert_ne!(first, second);
        assert_eq!(first.raw(), 10);
        assert!(!TrackId::INVALID.is_valid());
    }

    fn synthetic_processor() -> TrackProcessorDescriptor {
        TrackProcessorDescriptor {
            resources: None,
            id: TrackProcessorTypeId::new("browser_native_test"),
            label: "Browser native test".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                min_dry_audio_channels: None,
                max_dry_audio_channels: Some(4),
                min_wet_audio_channels: None,
                max_wet_audio_channels: Some(2),
                matching_audio_channels: false,
                midi: TrackProcessorMidiPolicy::Unsupported,
            },
            features: TrackProcessorFeatures {
                state: true,
                external_ui: true,
                embedded_ui: false,
                recovery: false,
                logs: false,
            },
            editor: None,
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn track_spec_uses_capability_catalog_and_constraints() {
        let processor = synthetic_processor();
        let spec = TrackSpec {
            name: "Processed".to_owned(),
            topology: TrackSpecTopology::DryWet {
                dry_audio_channels: 4,
                wet_audio_channels: 2,
                dry_midi: false,
                processor_type: processor.id.clone(),
                default_playback_mode: DefaultPlaybackMode::Regular,
            },
            latency: TrackLatencySpec::default(),
            initial_output_bus_names: Vec::new(),
            creation_request_id: None,
        };
        assert_eq!(
            spec.validate(&[]),
            Err(TrackSpecError::ProcessorUnavailable)
        );
        assert!(spec.validate(core::slice::from_ref(&processor)).is_ok());
        assert_eq!(
            spec.topology.channel_roles(),
            vec![
                TrackChannelRole::DryAudio(0),
                TrackChannelRole::DryAudio(1),
                TrackChannelRole::DryAudio(2),
                TrackChannelRole::DryAudio(3),
                TrackChannelRole::WetAudio(0),
                TrackChannelRole::WetAudio(1),
            ]
        );

        let unsupported = TrackSpec {
            topology: TrackSpecTopology::DryWet {
                dry_audio_channels: 5,
                wet_audio_channels: 2,
                dry_midi: false,
                processor_type: processor.id.clone(),
                default_playback_mode: DefaultPlaybackMode::Regular,
            },
            ..spec.clone()
        };
        assert_eq!(
            unsupported.validate(core::slice::from_ref(&processor)),
            Err(TrackSpecError::UnsupportedShape)
        );

        let no_wet_default = TrackSpec {
            topology: TrackSpecTopology::DryWet {
                dry_audio_channels: 2,
                wet_audio_channels: 0,
                dry_midi: false,
                processor_type: processor.id.clone(),
                default_playback_mode: DefaultPlaybackMode::DryThroughWet,
            },
            ..spec
        };
        assert_eq!(
            no_wet_default.validate(&[processor]),
            Err(TrackSpecError::UnsupportedShape)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn track_spec_rejects_latency_values_outside_the_domain() {
        let mut spec: TrackSpec = DirectTrackSpec {
            name: "Track".to_owned(),
            audio_channels: 2,
            midi: false,
        }
        .into();
        spec.latency.adjustment = RecordingOffsetAdjustmentState::ManualOverride;
        spec.latency.manual_frames = 768_001;
        assert_eq!(spec.validate(&[]), Err(TrackSpecError::InvalidLatency));

        spec.latency.manual_frames = 0;
        spec.latency.processor_adjustment = ProcessorLatencyAdjustmentState::ManualOverride;
        spec.latency.processor_manual_frames = -1;
        assert_eq!(spec.validate(&[]), Err(TrackSpecError::InvalidLatency));

        spec.latency.processor_manual_frames = 768_000;
        assert!(spec.validate(&[]).is_ok());
        spec.latency.processor_adjustment = ProcessorLatencyAdjustmentState::AutomaticPlusTrim;
        spec.latency.processor_manual_frames = -768_001;
        assert_eq!(spec.validate(&[]), Err(TrackSpecError::InvalidLatency));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_constraints_enforce_exact_channel_counts() {
        let constraints = TrackProcessorConstraints {
            min_dry_audio_channels: Some(0),
            max_dry_audio_channels: Some(0),
            min_wet_audio_channels: Some(2),
            max_wet_audio_channels: Some(2),
            matching_audio_channels: false,
            midi: TrackProcessorMidiPolicy::Required,
        };
        assert!(constraints.accepts(0, 2, true));
        assert!(!constraints.accepts(0, 1, true));
        assert!(!constraints.accepts(0, 3, true));
        assert!(!constraints.accepts(1, 2, true));
        assert!(!constraints.accepts(0, 2, false));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn processor_descriptors_preserve_future_ui_facets() {
        let processor = synthetic_processor();
        assert_eq!(processor.id.as_str(), "browser_native_test");
        assert!(processor.features.state);
        assert!(processor.features.external_ui);
        assert!(!processor.features.logs);
        assert_eq!(AppSnapshot::default().track_processors.len(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn direct_track_spec_validates_name_and_audio_range() {
        assert_eq!(
            DirectTrackSpec {
                name: "  ".to_owned(),
                audio_channels: 2,
                midi: false,
            }
            .validate(),
            Err(DirectTrackSpecError::EmptyName)
        );
        assert!(DirectTrackSpec {
            name: "Many channels".to_owned(),
            audio_channels: 1000,
            midi: false,
        }
        .validate()
        .is_ok());
        assert!(DirectTrackSpec {
            name: "Track".to_owned(),
            audio_channels: 2,
            midi: true,
        }
        .validate()
        .is_ok());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn click_track_defaults_and_intents_preserve_visible_contract_and_target() {
        let request = ClickTrackRequest::default();
        assert_eq!(request.kind, ClickTrackKind::Audio);
        assert_eq!(request.primary_sound_id, "click_high");
        assert_eq!(request.secondary_sound_id.as_deref(), Some("click_low"));
        assert_eq!(request.secondary_clicks_per_primary, 3);
        assert_eq!(request.bpm, 100.0);
        assert_eq!(request.click_count, 4);
        assert_eq!(request.odd_click_delay_percent, 0.0);
        assert_eq!(request.midi_note, 64);
        assert_eq!(request.midi_note_length_seconds, 0.1);
        let loop_id = LoopId::from_raw(42);
        assert_eq!(
            AppIntent::GenerateClickTrack {
                loop_id,
                request: request.clone(),
            },
            AppIntent::GenerateClickTrack { loop_id, request }
        );
        assert_eq!(
            AppSnapshot::default().click_track,
            ClickTrackState::default()
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_notes_validate_the_full_midi_range() {
        assert_eq!(MidiNote::new(0).unwrap().value(), 0);
        assert_eq!(MidiNote::new(60).unwrap().value(), 60);
        assert_eq!(MidiNote::new(127).unwrap().value(), 127);
        assert_eq!(MidiNote::new(128), None);
        assert_eq!(
            AppIntent::Piano(PianoAction::Press(MidiNote::new(60).unwrap())),
            AppIntent::Piano(PianoAction::Press(MidiNote::new(60).unwrap()))
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn intents_preserve_stable_ids_and_selection_modifiers() {
        let track_id = TrackId::from_raw(7);
        let loop_id = LoopId::from_raw(42);
        let intent = AppIntent::Loop {
            track_id,
            loop_id,
            action: LoopAction::IconClicked(SelectionModifiers { additive: true }),
        };
        assert_eq!(intent.kind(), "loop.icon_clicked");
        assert_eq!(
            intent,
            AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers { additive: true }),
            }
        );
        let source_loop_id = LoopId::from_raw(99);
        let compose = AppIntent::ComposeLoopSerial {
            target_loop_id: loop_id,
            source_loop_id,
        };
        assert_eq!(compose.kind(), "loop.compose_serial");
        assert_eq!(
            compose,
            AppIntent::ComposeLoopSerial {
                target_loop_id: loop_id,
                source_loop_id,
            }
        );
        let positioned = AppIntent::ComposeLoopAt {
            target_loop_id: loop_id,
            source_loop_id,
            start_iteration: 3,
        };
        assert_eq!(positioned.kind(), "loop.compose_at");
        assert_eq!(
            positioned,
            AppIntent::ComposeLoopAt {
                target_loop_id: loop_id,
                source_loop_id,
                start_iteration: 3,
            }
        );
        let delete = AppIntent::DeleteCompositeEvents {
            target_loop_id: loop_id,
            events: vec![CompositeEventId { instance_id: 123 }],
        };
        assert_eq!(delete.kind(), "loop.composite.delete_events");
        assert_eq!(
            delete,
            AppIntent::DeleteCompositeEvents {
                target_loop_id: loop_id,
                events: vec![CompositeEventId { instance_id: 123 }],
            }
        );
        let force_length = AppIntent::SetCompositeLoopCycles {
            target_loop_id: loop_id,
            event: CompositeEventId { instance_id: 123 },
            n_cycles: Some(4),
        };
        assert_eq!(force_length.kind(), "loop.composite.set_loop_cycles");
        assert_eq!(
            AppIntent::SetCompositeKind {
                target_loop_id: loop_id,
                kind: CompositeKind::Script,
            }
            .kind(),
            "loop.composite.set_kind"
        );
        assert_eq!(
            AppIntent::SetCompositeEventMode {
                target_loop_id: loop_id,
                event: CompositeEventId { instance_id: 123 },
                mode: LoopMode::Recording,
            }
            .kind(),
            "loop.composite.set_event_mode"
        );
        assert_eq!(
            LoopAction::NameChanged("Verse".to_owned()).kind(),
            "loop.name"
        );
        assert_eq!(
            LoopAction::ConvertToComposite.kind(),
            "loop.convert_to_composite"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn global_controls_have_stable_intent_kinds() {
        assert_eq!(GlobalControlAction::MidiPanic.kind(), "global.midi_panic");
        assert_eq!(
            GlobalControlAction::SetAutoArmTrackInputs(false).kind(),
            "global.auto_arm_track_inputs"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn builtin_fx_controls_have_stable_intent_kinds() {
        for (control, kind) in [
            (
                BuiltInFxControl::SetStageEnabled(BuiltInFxStage::Drive, true),
                "track.builtin_fx.stage_enabled",
            ),
            (
                BuiltInFxControl::SetDriveType(BuiltInFxDriveType::Fuzz),
                "track.builtin_fx.drive_type",
            ),
            (
                BuiltInFxControl::SetModulationType(BuiltInFxModulationType::Phaser),
                "track.builtin_fx.modulation_type",
            ),
            (
                BuiltInFxControl::SetReverbType(BuiltInFxReverbType::Plate),
                "track.builtin_fx.reverb_type",
            ),
            (
                BuiltInFxControl::SetParameter(BuiltInFxParameter::Drive, 12.0),
                "track.builtin_fx.parameter",
            ),
            (
                BuiltInFxControl::AssignMidiCc(BuiltInFxMidiCcAssignment {
                    parameter: BuiltInFxParameter::Drive,
                    channel: 2,
                    controller: 17,
                }),
                "track.builtin_fx.midi_cc_assign",
            ),
            (
                BuiltInFxControl::RemoveMidiCc(BuiltInFxParameter::Drive),
                "track.builtin_fx.midi_cc_remove",
            ),
            (
                BuiltInFxControl::ClearMidiCcAssignments,
                "track.builtin_fx.midi_cc_clear",
            ),
            (
                BuiltInFxControl::SetReverbEnabled(false),
                "track.builtin_fx.reverb_enabled",
            ),
        ] {
            assert_eq!(TrackAction::BuiltInFx(control).kind(), kind);
        }
        assert_eq!(BuiltInFxParameter::ALL.len(), 23);
        let labels = BuiltInFxParameter::ALL
            .into_iter()
            .map(BuiltInFxParameter::label)
            .collect::<alloc::collections::BTreeSet<_>>();
        assert_eq!(labels.len(), BuiltInFxParameter::ALL.len());
        assert_eq!(BuiltInFxState::default().midi_cc_assignments.len(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_controls_have_stable_intent_kinds() {
        assert_eq!(
            TrackAction::OxiSynth(OxiSynthControl::SelectPreset("0:40".to_owned())).kind(),
            "track.oxisynth.select_preset"
        );
        assert_eq!(
            TrackAction::OxiSynth(OxiSynthControl::SetReverbSend(0.5)).kind(),
            "track.oxisynth.reverb_send"
        );
        assert_eq!(
            TrackAction::OxiSynth(OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
                parameter: OxiSynthParameter::ChorusSend,
                channel: 2,
                controller: 18,
            }))
            .kind(),
            "track.oxisynth.midi_cc_assign"
        );
        assert_eq!(
            TrackAction::OxiSynth(OxiSynthControl::ClearMidiCcAssignments).kind(),
            "track.oxisynth.midi_cc_clear"
        );
        assert_eq!(
            TrackAction::OxiSynth(OxiSynthControl::Panic).kind(),
            "track.oxisynth.panic"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn latest_midi_message_only_recognizes_complete_control_changes() {
        assert_eq!(
            LatestMidiMessage::new([0xb7, 74, 99, 0], 3)
                .unwrap()
                .midi_cc(),
            Some((7, 74, 99))
        );
        for message in [
            LatestMidiMessage::new([0xb7, 74, 0, 0], 2).unwrap(),
            LatestMidiMessage::new([0x97, 74, 99, 0], 3).unwrap(),
            LatestMidiMessage::new([0xb7, 74, 99, 1], 4).unwrap(),
            LatestMidiMessage::new([0xb7, 200, 99, 0], 3).unwrap(),
            LatestMidiMessage::new([0xb7, 74, 200, 0], 3).unwrap(),
        ] {
            assert_eq!(message.midi_cc(), None);
        }
        assert!(LatestMidiMessage::new([0; 4], 0).is_none());
        assert!(LatestMidiMessage::new([0; 4], 5).is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn lua_api_versions_use_major_equality_and_minor_backwards_compatibility() {
        let host = LuaApiVersion { major: 2, minor: 4 };
        assert!(host.accepts(LuaApiVersion { major: 2, minor: 0 }));
        assert!(host.accepts(host));
        assert!(!host.accepts(LuaApiVersion { major: 2, minor: 5 }));
        assert!(!host.accepts(LuaApiVersion { major: 1, minor: 4 }));
        assert!(!host.accepts(LuaApiVersion { major: 3, minor: 0 }));
        assert_eq!(LUA_API_VERSION, LuaApiVersion { major: 1, minor: 6 });
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dialog_contract_preserves_plain_order_and_callback_identity() {
        let script_id = ScriptId::from_raw(8);
        let dialog_id = ScriptDialogId::from_raw(12);
        let button_id = ScriptDialogButtonId::from_raw(14);
        let state = ScriptDialogState {
            id: dialog_id,
            owner_script_id: script_id,
            owner_script_name: "owner.lua".to_owned(),
            name: "Help".to_owned(),
            kind: ScriptDialogKind::Simple(ScriptDialogContent {
                elements: Arc::from([
                    ScriptDialogElement::RichText {
                        text: "First".to_owned(),
                        style: ScriptDialogRichTextStyle {
                            strong: true,
                            ..Default::default()
                        },
                    },
                    ScriptDialogElement::Markdown {
                        text: "[More](more)".to_owned(),
                        links: Arc::from([ScriptDialogMarkdownLink {
                            destination: "more".to_owned(),
                            callback_id: button_id,
                        }]),
                        resource_base_uri: None,
                    },
                    ScriptDialogElement::Button {
                        id: Some(button_id),
                        label: "Run".to_owned(),
                    },
                ]),
            }),
            open_request: 3,
        };
        let ScriptDialogKind::Simple(content) = &state.kind else {
            panic!("expected simple dialog");
        };
        assert_eq!(content.elements.len(), 3);
        assert_eq!(state.owner_script_id, script_id);
        assert_eq!(state.open_request, 3);
        assert_eq!(
            AppIntent::InvokeScriptDialogButton {
                script_id,
                dialog_id,
                button_id,
            }
            .kind(),
            "scripting.dialog_button"
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn script_contract_preserves_plain_state_and_stable_intents() {
        let script_id = ScriptId::from_raw(8);
        let state = ScriptState {
            id: script_id,
            name: "controller.lua".to_owned(),
            identity: None,
            kind: ScriptKind::User,
            enabled: true,
            lifecycle: ScriptLifecycle::Listening,
            documentation: Some("Controller help\n".to_owned()),
            resource_base_uri: None,
            latest_error: None,
            activity: ScriptActivityDiagnostics::default(),
            midi: ScriptMidiDiagnostics::default(),
            logs: Arc::from([]),
        };
        assert_eq!(state.id, script_id);
        assert_eq!(state.kind, ScriptKind::User);
        assert_eq!(
            AppIntent::SetScriptEnabled {
                script_id,
                enabled: false,
            },
            AppIntent::SetScriptEnabled {
                script_id,
                enabled: false,
            }
        );
        assert!(!AppSnapshot::default().scripting.supported);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connection_contract_preserves_identity_roles_and_exact_desired_state() {
        let port_id = PortId::from_raw(17);
        let track_id = TrackId::from_raw(3);
        let endpoint = "client:name:with:colons".to_owned();
        let port = ApplicationPortState {
            publication: None,
            id: port_id,
            owner: ApplicationPortOwner::Track {
                track_id,
                kind: TrackPortOwnerKind::Main,
            },
            name: "Track direct in".to_owned(),
            data_type: PortDataType::Audio,
            direction: PortDirection::Input,
            role: PortRole::AudioInput,
            connection_policy: ConnectionPolicy::UserManaged,
        };
        let host = HostPortState {
            id: HostPortId::new(endpoint.clone()),
            name: endpoint.clone(),
            data_type: PortDataType::Audio,
            direction: PortDirection::Output,
            available: true,
        };
        let pending = PendingConnectionState {
            application_port_id: port_id,
            host_port_id: host.id.clone(),
            desired_connected: true,
        };
        assert_eq!(
            port.owner,
            ApplicationPortOwner::Track {
                track_id,
                kind: TrackPortOwnerKind::Main,
            }
        );
        assert_eq!(port.role, PortRole::AudioInput);
        assert_eq!(pending.host_port_id.as_str(), endpoint);
        assert_eq!(
            AppIntent::SetPortConnected {
                port_id,
                host_port_id: HostPortId::new(endpoint.clone()),
                connected: true,
            },
            AppIntent::SetPortConnected {
                port_id,
                host_port_id: HostPortId::new(endpoint),
                connected: true,
            }
        );
        let mixer_intent = AppIntent::SetMixerRouteConnected {
            source_port_id: port_id,
            destination_channel_id: BusChannelId::from_raw(2),
            connected: true,
        };
        assert_eq!(mixer_intent.kind(), "mixer.connection.set");
        assert_eq!(
            PortRole::ORDERED.map(PortRole::label),
            [
                "Audio in",
                "Audio out",
                "Audio send",
                "Audio return",
                "MIDI in",
                "MIDI out",
                "MIDI send",
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn connection_snapshots_are_structurally_shared_and_independent() {
        let ports: Arc<[ApplicationPortState]> = Arc::from([]);
        let first = AppSnapshot {
            connections: Arc::new(ConnectionViewState {
                revision: 4,
                loading: false,
                backend_available: true,
                application_ports: Arc::clone(&ports),
                host_ports: Arc::from([]),
                confirmed_links: Arc::from([]),
                pending_links: Arc::from([]),
                mixer_links: Arc::from([]),
                pending_mixer_links: Arc::from([]),
                mixer_errors: Arc::from([]),
                errors: Arc::from([]),
            }),
            ..Default::default()
        };
        let mut second = first.clone();
        second.revision = 9;
        assert!(Arc::ptr_eq(&first.connections, &second.connections));
        assert!(Arc::ptr_eq(
            &first.connections.application_ports,
            &second.connections.application_ports
        ));
        assert_eq!(first.revision, 0);
        assert_eq!(first.connections.revision, 4);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn track_controls_are_clamped_to_ui_ranges() {
        let mut state = TrackControlState {
            output_gain_db: 50.0,
            input_gain_db: -100.0,
            output_balance: -4.0,
            input_balance: 3.0,
            ..Default::default()
        };
        state.clamp();
        assert_eq!(state.output_gain_db, MAX_TRACK_GAIN_DB);
        assert_eq!(state.input_gain_db, MIN_TRACK_GAIN_DB);
        assert_eq!(state.output_balance, -1.0);
        assert_eq!(state.input_balance, 1.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn audio_driver_configs_have_stable_kinds_and_independent_defaults() {
        let dummy = AudioDriverConfig::default();
        let jack = AudioDriverConfig::Jack(JackAudioDriverConfig::default());
        let cpal = AudioDriverConfig::Cpal(CpalAudioDriverConfig::default());
        assert_eq!(dummy.kind(), AudioDriverKind::Dummy);
        assert_eq!(jack.kind(), AudioDriverKind::Jack);
        assert_eq!(cpal.kind(), AudioDriverKind::Cpal);
        assert_eq!(AudioDriverKind::Cpal.id(), "cpal");
        assert_eq!(AudioDriverKind::Jack.label(), "JACK");
        assert_ne!(dummy, jack);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn latency_is_calculated_from_buffer_size_and_sample_rate() {
        let status = StatusState {
            buffer_size: 256,
            sample_rate: 48_000,
            ..Default::default()
        };
        assert_eq!(status.latency_ms(), Some(256.0 * 1000.0 / 48_000.0));
        assert_eq!(StatusState::default().latency_ms(), None);
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ControlOperation {
    Transition {
        loops: Vec<LoopId>,
        mode: LoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    },
    Trigger {
        loops: Vec<LoopId>,
        mode: LoopMode,
    },
    TriggerDefaultPlayback {
        loops: Vec<LoopId>,
    },
    Grab {
        loops: Vec<LoopId>,
    },
    RecordN {
        loops: Vec<LoopId>,
        n_cycles: u32,
        cycles_delay: u32,
    },
    RecordWithTargeted {
        loops: Vec<LoopId>,
    },
    SetLoopGain {
        loops: Vec<LoopId>,
        gain: f32,
    },
    SetLoopBalance {
        loops: Vec<LoopId>,
        balance: f32,
    },
    SetLoopSelection {
        loops: Vec<LoopId>,
        selected: bool,
        clear_others: bool,
    },
    SetTarget {
        target: Option<LoopId>,
    },
    ClearLoops {
        loops: Vec<LoopId>,
    },
    AdoptRingbuffers {
        loops: Vec<LoopId>,
        reverse_cycle_start: u32,
        cycles_length: u32,
        go_to_cycle: u32,
        go_to_mode: LoopMode,
    },
    ComposeAddToEnd {
        target: LoopId,
        add: Vec<LoopId>,
        parallel: bool,
    },
    SetRepeatSync {
        loops: Vec<LoopId>,
        active: bool,
    },
    SetTrackGain {
        tracks: Vec<TrackId>,
        gain_db: f32,
    },
    SetTrackBalance {
        tracks: Vec<TrackId>,
        balance: f32,
    },
    SetTrackMuted {
        tracks: Vec<TrackId>,
        muted: bool,
    },
    SetTrackInputGain {
        tracks: Vec<TrackId>,
        gain_db: f32,
    },
    SetTrackInputBalance {
        tracks: Vec<TrackId>,
        balance: f32,
    },
    SetTrackInputMuted {
        tracks: Vec<TrackId>,
        muted: bool,
        respect_auto_mute: bool,
    },
    SetBusGain {
        buses: Vec<BusId>,
        gain_db: f32,
    },
    SetBusBalance {
        buses: Vec<BusId>,
        balance: f32,
    },
    SetBusMuted {
        buses: Vec<BusId>,
        muted: bool,
    },
    SetApplyNCycles(u32),
    SetSolo(bool),
    SetSyncActive(bool),
    SetPlayAfterRecord(bool),
    SetAutoMuteOtherTrackInputs(bool),
    SetDefaultRecordingAction(DefaultRecordingAction),
}

impl ControlOperation {
    pub fn bounded(&self) -> bool {
        match self {
            Self::Transition { loops, .. }
            | Self::Trigger { loops, .. }
            | Self::TriggerDefaultPlayback { loops }
            | Self::Grab { loops }
            | Self::RecordN { loops, .. }
            | Self::RecordWithTargeted { loops }
            | Self::SetLoopGain { loops, .. }
            | Self::SetLoopBalance { loops, .. }
            | Self::SetLoopSelection { loops, .. }
            | Self::ClearLoops { loops }
            | Self::AdoptRingbuffers { loops, .. }
            | Self::SetRepeatSync { loops, .. } => loops.len() <= 128,
            Self::ComposeAddToEnd { add, .. } => add.len() <= 128,
            Self::SetTrackGain { tracks, .. }
            | Self::SetTrackBalance { tracks, .. }
            | Self::SetTrackMuted { tracks, .. }
            | Self::SetTrackInputGain { tracks, .. }
            | Self::SetTrackInputBalance { tracks, .. }
            | Self::SetTrackInputMuted { tracks, .. } => tracks.len() <= 128,
            Self::SetBusGain { buses, .. }
            | Self::SetBusBalance { buses, .. }
            | Self::SetBusMuted { buses, .. } => buses.len() <= 128,
            _ => true,
        }
    }
}

/// Host-local admission ceilings within the shared protocol limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostResourceLimits {
    pub media_bytes: u64,
    pub metadata_bytes: u64,
    pub objects: u32,
    pub midi_events: usize,
}

impl Default for HostResourceLimits {
    fn default() -> Self {
        Self {
            media_bytes: 256 * 1024 * 1024,
            metadata_bytes: 8 * 1024 * 1024,
            objects: 4096,
            midi_events: 1_000_000,
        }
    }
}

impl HostResourceLimits {
    pub fn is_valid(self) -> bool {
        let ceiling = Self::default();
        (1..=ceiling.media_bytes).contains(&self.media_bytes)
            && (1..=ceiling.metadata_bytes).contains(&self.metadata_bytes)
            && (1..=ceiling.objects).contains(&self.objects)
            && (1..=ceiling.midi_events).contains(&self.midi_events)
    }
}

/// Capacities the new engine backend prepares at startup.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct NewEnginePools {
    /// Graph tick in frames; immutable during a running session.
    pub quantum: u32,
    pub buses: u32,
    pub bus_channels: u32,
    /// Tracks, including the sync track.
    pub tracks: u32,
    /// Loop slots shared by all tracks.
    pub loops: u32,
    /// Audio channels per track direction.
    pub channels: u32,
    pub loop_memory_mb: u32,
}

impl Default for NewEnginePools {
    fn default() -> Self {
        Self {
            quantum: 32,
            buses: 8,
            bus_channels: 8,
            tracks: 16,
            loops: 128,
            channels: 2,
            loop_memory_mb: 256,
        }
    }
}

/// Host-owned startup settings shared by native configuration and UI launch defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default, deny_unknown_fields))]
pub struct HostSettings {
    pub driver: AudioDriverConfig,
    pub pools: NewEnginePools,
    pub processors: ProcessorSettings,
    pub master_auto_connect: bool,
    pub loop_smoothing_ms: u32,
}
impl Default for HostSettings {
    fn default() -> Self {
        Self {
            driver: Default::default(),
            pools: Default::default(),
            processors: ProcessorSettings::default(),
            master_auto_connect: true,
            loop_smoothing_ms: 3,
        }
    }
}
impl HostSettings {
    pub fn validate(&self) -> Result<(), &'static str> {
        self.processors
            .validate()
            .map_err(|_| "invalid processor startup capacities")?;
        if !matches!(self.pools.quantum, 1 | 16 | 32 | 64 | 128) {
            return Err("processing quantum must be 1, 16, 32, 64 or 128 frames");
        }
        if !(1..=8).contains(&self.pools.buses)
            || !(2..=8).contains(&self.pools.bus_channels)
            || !(1..=64).contains(&self.pools.tracks)
            || !(1..=256).contains(&self.pools.loops)
            || !(1..=8).contains(&self.pools.channels)
            || !(1..=1024).contains(&self.pools.loop_memory_mb)
        {
            return Err("session engine capacities exceed supported bounds");
        }
        if let AudioDriverConfig::Jack(config) = &self.driver {
            config
                .validate()
                .map_err(|_| "invalid JACK driver configuration")?;
        }
        if self.loop_smoothing_ms > 100 {
            return Err("loop smoothing exceeds 100 ms");
        }
        if let AudioDriverConfig::Dummy(driver) = &self.driver {
            if !(8000..=384000).contains(&driver.sample_rate)
                || !(1..=16384).contains(&driver.buffer_size)
            {
                return Err("invalid dummy driver configuration");
            }
        }
        Ok(())
    }
}

pub use shoop_processor_protocol::{Policy as ProcessorPolicy, Settings as ProcessorSettings};
