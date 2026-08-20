use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SESSION_FORMAT: &str = "shoop-session";
pub const MIDI_FORMAT: &str = "shoop-midi";
pub const AUDIO_FORMAT: &str = "shoop-audio";
pub const FORMAT_MAJOR: u16 = 1;
pub const FORMAT_MINOR: u16 = 0;
pub const DOCUMENT_VERSION: u16 = 1;
pub const SESSION_DOCUMENT_VERSION: u16 = 2;
pub const CONNECTION_MODEL_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct FormatVersion {
    pub major: u16,
    pub minor: u16,
}

impl Default for FormatVersion {
    fn default() -> Self {
        Self {
            major: FORMAT_MAJOR,
            minor: FORMAT_MINOR,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionBundle {
    pub document: SessionDocument,
    #[serde(skip)]
    pub media: BTreeMap<String, MediaPayload>,
}

impl SessionBundle {
    pub fn new(document: SessionDocument) -> Self {
        Self {
            document,
            media: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionDocument {
    pub sample_rate: u32,
    /// Zero identifies pre-normalized documents and enables target-specific migration.
    #[serde(default)]
    pub connection_model_version: u16,
    pub global: GlobalControlsDocument,
    pub track_groups: Vec<TrackGroupDocument>,
    pub selected_loop_ids: Vec<u64>,
    pub targeted_loop_id: Option<u64>,
    pub buses: Vec<BusDocument>,
    pub global_ports: Vec<PortDocument>,
    pub fx_states: Vec<FxStateDocument>,
    pub scripts: Vec<ScriptDocument>,
    pub midi_control: MidiControlDocument,
    pub settings: Vec<SessionSettingDocument>,
}

impl SessionDocument {
    pub fn empty(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            connection_model_version: CONNECTION_MODEL_VERSION,
            global: GlobalControlsDocument::default(),
            track_groups: Vec::new(),
            selected_loop_ids: Vec::new(),
            targeted_loop_id: None,
            buses: Vec::new(),
            global_ports: Vec::new(),
            fx_states: Vec::new(),
            scripts: Vec::new(),
            midi_control: MidiControlDocument::default(),
            settings: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobalControlsDocument {
    pub default_recording_action: RecordingActionDocument,
    pub play_after_record: bool,
    pub sync: bool,
    pub solo: bool,
    #[serde(default)]
    pub auto_mute_other_track_inputs: bool,
    pub apply_n_cycles: u32,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RecordingActionDocument {
    #[default]
    Record,
    Grab,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrackGroupDocument {
    pub name: String,
    pub tracks: Vec<TrackDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TrackDocument {
    pub id: u64,
    pub name: String,
    pub port_name_base: String,
    pub is_sync: bool,
    pub width: Option<f32>,
    pub topology: TrackTopologyDocument,
    pub controls: TrackControlsDocument,
    pub loops: Vec<LoopDocument>,
    pub ports: Vec<PortDocument>,
    pub fx_chain: Option<FxChainDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrackTopologyDocument {
    Direct {
        audio_channels: u32,
        midi: bool,
    },
    DryWetExternal {
        dry_audio_channels: u32,
        wet_audio_channels: u32,
        dry_midi: bool,
    },
    Carla {
        chain_type: FxChainTypeDocument,
        /// Legacy equal dry/wet count retained for backward compatibility.
        audio_channels: u32,
        midi: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dry_audio_channels: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        wet_audio_channels: Option<u32>,
    },
    TinySynthFx {
        audio_channels: u32,
    },
    Trigger,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TrackControlsDocument {
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub input_monitoring: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LoopDocument {
    pub id: u64,
    pub name: String,
    pub length_frames: u64,
    pub is_sync: bool,
    pub gain: f32,
    pub balance: f32,
    pub channels: Vec<ChannelDocument>,
    pub composite: Option<CompositeDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ChannelDocument {
    pub id: u64,
    pub mode: ChannelModeDocument,
    pub data_type: DataTypeDocument,
    pub data_length_frames: u64,
    pub start_offset_frames: i64,
    pub preplay_frames: u64,
    pub gain: f32,
    pub connected_port_ids: Vec<u64>,
    pub media_id: Option<String>,
    pub recording_started_at: Option<String>,
    pub recording_fx_state_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelModeDocument {
    Disabled,
    Direct,
    Dry,
    Wet,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DataTypeDocument {
    Audio,
    Midi,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct PortDocument {
    pub id: u64,
    pub name: String,
    pub data_type: DataTypeDocument,
    pub direction: PortDirectionDocument,
    pub role: PortRoleDocument,
    pub input_connectability: Vec<ConnectabilityDocument>,
    pub output_connectability: Vec<ConnectabilityDocument>,
    pub gain: f32,
    pub muted: bool,
    pub passthrough_muted: bool,
    pub internal_connections: Vec<u64>,
    pub external_connections: Vec<String>,
    pub ringbuffer_frames: u64,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PortDirectionDocument {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PortRoleDocument {
    AudioInput,
    AudioOutput,
    AudioSend,
    AudioReturn,
    MidiInput,
    MidiOutput,
    MidiSend,
    Internal,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectabilityDocument {
    Internal,
    External,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct BusDocument {
    pub id: u64,
    pub name: String,
    pub ports: Vec<PortDocument>,
    pub fx_chain: Option<FxChainDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FxChainDocument {
    pub id: u64,
    pub title: String,
    pub chain_type: FxChainTypeDocument,
    pub ports: Vec<PortDocument>,
    pub internal_state: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub midi_cc_assignments: Vec<TinySynthFxMidiCcAssignmentDocument>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct TinySynthFxMidiCcAssignmentDocument {
    pub parameter: TinySynthFxParameterDocument,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, Ord, PartialEq, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub enum TinySynthFxParameterDocument {
    MasterGain,
    ReverbAmount,
    DistortionDrive,
    CompressorAmount,
    EqLow,
    EqMid,
    EqHigh,
    VocoderMix,
    VocoderSensitivity,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum FxChainTypeDocument {
    CarlaRack,
    CarlaPatchbay,
    CarlaPatchbay16x,
    TinySynthFx,
    Test,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct FxStateDocument {
    pub id: u64,
    pub chain_type: FxChainTypeDocument,
    pub internal_state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompositeDocument {
    pub kind: CompositeKindDocument,
    pub playlists: Vec<Vec<Vec<CompositeEventDocument>>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompositeKindDocument {
    Regular,
    Script,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CompositeEventDocument {
    /// Sync-loop iterations after the containing section begins.
    #[serde(alias = "delay_frames")]
    pub delay: u64,
    pub loop_id: u64,
    pub mode: Option<String>,
    pub n_cycles: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ScriptDocument {
    pub id: u64,
    pub name: String,
    pub source: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MidiControlDocument {
    pub bindings: Vec<MidiBindingDocument>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MidiBindingDocument {
    pub id: u64,
    pub message: Vec<u8>,
    pub action: String,
    pub target_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SessionSettingDocument {
    pub key: String,
    pub value: SettingValueDocument,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SettingValueDocument {
    Bool(bool),
    Integer(i64),
    Number(f64),
    String(String),
    StringList(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaPayload {
    Audio(AudioPayload),
    Midi(ExactMidi),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioPayload {
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExactMidi {
    pub sample_rate: u32,
    pub length_frames: u64,
    pub start_state: Vec<Vec<u8>>,
    pub events: Vec<ExactMidiEvent>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ExactMidiEvent {
    pub frame: u64,
    pub order: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopAudio {
    pub sample_rate: u32,
    pub channels: Vec<LoopAudioChannel>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopAudioChannel {
    pub label: String,
    pub role: String,
    pub samples: Vec<f32>,
}
