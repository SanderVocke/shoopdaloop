#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use std::fmt;
use std::sync::Arc;

macro_rules! entity_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
entity_id!(LoopId);
entity_id!(PortId);
entity_id!(ChannelId);
entity_id!(TaskId);
entity_id!(ScriptId);
entity_id!(ScriptDialogId);
entity_id!(ScriptDialogButtonId);

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
pub struct TrackProcessorTypeId(String);

impl TrackProcessorTypeId {
    pub const EXTERNAL: &'static str = "external";
    pub const CARLA_RACK: &'static str = "carla_rack";
    pub const CARLA_PATCHBAY: &'static str = "carla_patchbay";
    pub const CARLA_PATCHBAY_16X: &'static str = "carla_patchbay_16x";
    pub const TINY_SYNTH_FX: &'static str = "tiny_synth_fx";

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
pub struct TrackProcessorFeatures {
    pub state: bool,
    pub external_ui: bool,
    pub embedded_ui: bool,
    pub recovery: bool,
    pub logs: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TrackProcessorMidiPolicy {
    #[default]
    Unsupported,
    Optional,
    Required,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TrackProcessorConstraints {
    pub max_dry_audio_channels: Option<u32>,
    pub max_wet_audio_channels: Option<u32>,
    pub matching_audio_channels: bool,
    pub midi: TrackProcessorMidiPolicy,
}

impl TrackProcessorConstraints {
    pub fn accepts(self, dry_audio_channels: u32, wet_audio_channels: u32, dry_midi: bool) -> bool {
        self.max_dry_audio_channels
            .is_none_or(|limit| dry_audio_channels <= limit)
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
pub struct TrackProcessorPresetDescriptor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackProcessorEditorDescriptor {
    TinySynthFx {
        presets: Arc<[TrackProcessorPresetDescriptor]>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackProcessorDescriptor {
    pub id: TrackProcessorTypeId,
    pub label: String,
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub constraints: TrackProcessorConstraints,
    pub features: TrackProcessorFeatures,
    pub editor: Option<TrackProcessorEditorDescriptor>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FxLifecycle {
    #[default]
    Unavailable,
    Starting,
    Running,
    Crashed,
    Restarting,
    Stopped,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FxGenerationLogState {
    pub generation: u64,
    pub stdout: Arc<str>,
    pub stderr: Arc<str>,
    pub dropped_stdout_bytes: u64,
    pub dropped_stderr_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum TinySynthFxParameter {
    MasterGain,
    ReverbAmount,
    DistortionDrive,
    CompressorAmount,
    EqLow,
    EqMid,
    EqHigh,
}

impl TinySynthFxParameter {
    pub const ALL: [Self; 7] = [
        Self::MasterGain,
        Self::ReverbAmount,
        Self::DistortionDrive,
        Self::CompressorAmount,
        Self::EqLow,
        Self::EqMid,
        Self::EqHigh,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::MasterGain => "Master gain",
            Self::ReverbAmount => "Reverb amount",
            Self::DistortionDrive => "Distortion drive",
            Self::CompressorAmount => "Compressor amount",
            Self::EqLow => "EQ low",
            Self::EqMid => "EQ mid",
            Self::EqHigh => "EQ high",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TinySynthFxMidiCcAssignment {
    pub parameter: TinySynthFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatestMidiMessage {
    pub bytes: [u8; 4],
    pub len: u8,
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

#[derive(Clone, Debug, PartialEq)]
pub struct TinySynthFxState {
    pub selected_preset_id: Option<String>,
    pub master_gain_db: f32,
    pub reverb_enabled: bool,
    pub reverb_amount: f32,
    pub distortion_enabled: bool,
    pub distortion_drive: f32,
    pub compressor_enabled: bool,
    pub compressor_amount: f32,
    pub eq_enabled: bool,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub midi_cc_assignments: Arc<[TinySynthFxMidiCcAssignment]>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackProcessorEditorState {
    TinySynthFx(TinySynthFxState),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrackFxState {
    pub processor_type: TrackProcessorTypeId,
    pub active: bool,
    pub visible: bool,
    pub lifecycle: FxLifecycle,
    pub generation: u64,
    pub crash_summary: Option<String>,
    pub logs: Arc<[FxGenerationLogState]>,
    pub editor: Option<TrackProcessorEditorState>,
}

pub const MIN_TRACK_GAIN_DB: f32 = -30.0;
pub const MAX_TRACK_GAIN_DB: f32 = 20.0;
pub const MIN_TINY_SYNTH_FX_GAIN_DB: f32 = -60.0;
pub const MAX_TINY_SYNTH_FX_GAIN_DB: f32 = 0.0;
pub const MIN_TINY_SYNTH_FX_EQ_GAIN_DB: f32 = -12.0;
pub const MAX_TINY_SYNTH_FX_EQ_GAIN_DB: f32 = 12.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DefaultRecordingAction {
    #[default]
    Record,
    Grab,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
pub enum CompositeKind {
    #[default]
    None,
    Regular,
    Script,
}

#[derive(Clone, Debug)]
pub struct GlobalControlState {
    pub default_recording_action: DefaultRecordingAction,
    pub play_after_record: bool,
    pub sync: bool,
    pub solo: bool,
    pub auto_mute_other_track_inputs: bool,
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
            apply_n_cycles: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
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
    pub midi_inputs: Vec<String>,
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
pub enum AudioDriverConfig {
    Dummy(DummyAudioDriverConfig),
    Jack(JackAudioDriverConfig),
    Cpal(CpalAudioDriverConfig),
    WebAudio,
}

impl AudioDriverConfig {
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
pub struct ResolvedAudioDriverConfig {
    pub configured: AudioDriverConfig,
    pub sample_rate: u32,
    pub buffer_size: u32,
    pub instance_name: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
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
pub struct AudioDriverSwitchState {
    pub request_id: u64,
    pub status: AudioDriverSwitchStatus,
    pub source: Option<ResolvedAudioDriverConfig>,
    pub target: Option<ResolvedAudioDriverConfig>,
    pub message: String,
    pub persistence_retry_available: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioDriverRuntimeState {
    pub supported: bool,
    pub catalog: Arc<[AudioDriverDescriptor]>,
    pub active: Option<ResolvedAudioDriverConfig>,
    pub switch: AudioDriverSwitchState,
}

#[derive(Clone, Debug, Default)]
pub struct StatusState {
    pub version: String,
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
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
    pub fn latency_ms(&self) -> Option<f32> {
        (self.sample_rate > 0).then(|| self.buffer_size as f32 * 1000.0 / self.sample_rate as f32)
    }
}

#[derive(Clone, Debug)]
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
pub enum StructuralState {
    #[default]
    Confirmed,
    Creating,
    Removing,
}

#[derive(Clone, Debug)]
pub struct LoopState {
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
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PortDataType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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
pub enum TrackPortOwnerKind {
    Sync,
    Main,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ApplicationPortOwner {
    GlobalFxControl,
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
pub enum ConnectionPolicy {
    UserManaged,
    OwnerManaged,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationPortState {
    pub id: PortId,
    pub owner: ApplicationPortOwner,
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
    pub role: PortRole,
    pub connection_policy: ConnectionPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPortState {
    pub id: HostPortId,
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfirmedConnectionState {
    pub application_port_id: PortId,
    pub host_port_id: HostPortId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingConnectionState {
    pub application_port_id: PortId,
    pub host_port_id: HostPortId,
    pub desired_connected: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
pub struct ConnectionErrorState {
    pub port_id: Option<PortId>,
    pub external_port: Option<String>,
    pub kind: ConnectionErrorKind,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectionViewState {
    pub revision: u64,
    pub loading: bool,
    pub backend_available: bool,
    pub application_ports: Arc<[ApplicationPortState]>,
    pub host_ports: Arc<[HostPortState]>,
    pub confirmed_links: Arc<[ConfirmedConnectionState]>,
    pub pending_links: Arc<[PendingConnectionState]>,
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

#[derive(Clone, Debug, Default)]
pub struct TrackState {
    pub id: TrackId,
    pub name: String,
    pub structural_state: StructuralState,
    pub is_sync: bool,
    pub topology: TrackTopology,
    pub fx: Option<TrackFxState>,
    pub loops: Vec<LoopState>,
    pub controls: TrackControlState,
    pub port_ids: Arc<[PortId]>,
}

#[derive(Clone, Debug)]
pub struct WaveformChannelState {
    pub id: ChannelId,
    pub label: String,
    pub samples: Arc<[f32]>,
    pub start_offset: i64,
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
            loop_length: 0,
            played_sample: None,
        }
    }
}

#[derive(Clone, Debug)]
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
    pub start_offset: i64,
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
            start_offset: 0,
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeEventDetailsState {
    pub loop_id: LoopId,
    pub loop_name: String,
    pub track_id: TrackId,
    pub start_frame: u64,
    pub end_frame: u64,
    pub playlist_index: u32,
    pub section_index: u32,
    pub parallel_index: u32,
    pub mode: Option<String>,
    pub forced_n_cycles: Option<u32>,
    pub loop_mode: LoopMode,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompositeDetailsState {
    pub kind: CompositeKind,
    pub cycle_length_frames: u64,
    pub timeline_length_frames: u64,
    pub tracks: Vec<CompositeTrackDetailsState>,
    pub events: Vec<CompositeEventDetailsState>,
}

#[derive(Clone, Debug, Default)]
pub struct LoopDetailsState {
    pub generation: u64,
    pub loop_id: LoopId,
    pub title: String,
    pub loading: bool,
    pub channels: Vec<WaveformChannelState>,
    pub midi_loading: bool,
    pub midi_channels: Vec<MidiSequenceChannelState>,
    pub composite: Option<CompositeDetailsState>,
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
    AwaitingSampleRateConfirmation,
    AwaitingChannelMapping,
    AwaitingChannelSelection,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SampleRateWarning {
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
    pub sample_rate_warning: Option<SampleRateWarning>,
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

pub const LUA_API_VERSION: LuaApiVersion = LuaApiVersion { major: 1, minor: 2 };

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScriptKind {
    Bundled,
    Example,
    User,
    Session,
    Ephemeral,
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
        .collect::<std::collections::BTreeSet<_>>();
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
    pub kind: ScriptKind,
    pub enabled: bool,
    pub lifecycle: ScriptLifecycle,
    pub documentation: Option<String>,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClickTrackPreviewStatus {
    #[default]
    Idle,
    Queued,
    Playing,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClickSoundDescriptor {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClickTrackState {
    pub sounds: Arc<[ClickSoundDescriptor]>,
    pub max_click_count: u32,
    pub max_output_frames: u32,
    pub preview_request_id: u64,
    pub preview_status: ClickTrackPreviewStatus,
    pub preview_message: String,
}

#[derive(Clone, Debug, Default)]
pub struct AppSnapshot {
    pub revision: u64,
    pub tracks: Vec<TrackState>,
    pub track_processors: Arc<[TrackProcessorDescriptor]>,
    pub global_controls: GlobalControlState,
    pub status: StatusState,
    pub audio_drivers: AudioDriverRuntimeState,
    pub details: Option<LoopDetailsState>,
    pub connections: Arc<ConnectionViewState>,
    pub scripting: Arc<ScriptingState>,
    pub click_track: ClickTrackState,
    pub io_task: Option<IoTaskState>,
    pub notifications: Vec<AppNotification>,
}

pub type AppState = AppSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrackSpec {
    pub name: String,
    pub topology: TrackSpecTopology,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

impl TrackSpec {
    pub fn validate(&self, processors: &[TrackProcessorDescriptor]) -> Result<(), TrackSpecError> {
        if self.name.trim().is_empty() {
            return Err(TrackSpecError::EmptyName);
        }
        let TrackSpecTopology::DryWet {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
            processor_type,
        } = &self.topology
        else {
            return Ok(());
        };
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
    ProcessorUnavailable,
    UnsupportedShape,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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
pub struct SelectionModifiers {
    pub additive: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopAudioExportFormat {
    Exact,
    FloatWav,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClickTrackKind {
    #[default]
    Audio,
    Midi,
}

#[derive(Clone, Debug, PartialEq)]
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
    StopClicked,
    GainChanged(f32),
    BalanceChanged(f32),
    RestoreRecordedFxState,
    ConvertToComposite,
    Duplicate,
    DuplicateTo(LoopId),
    SwapWith(LoopId),
    MoveBefore(Option<LoopId>),
}

pub type LoopWidgetAction = LoopAction;

#[derive(Clone, Debug, PartialEq)]
pub enum TinySynthFxControl {
    SelectPreset(String),
    SetMasterGainDb(f32),
    SetReverbEnabled(bool),
    SetReverbAmount(f32),
    SetDistortionEnabled(bool),
    SetDistortionDrive(f32),
    SetCompressorEnabled(bool),
    SetCompressorAmount(f32),
    SetEqEnabled(bool),
    SetEqLowDb(f32),
    SetEqMidDb(f32),
    SetEqHighDb(f32),
    AssignMidiCc(TinySynthFxMidiCcAssignment),
    RemoveMidiCc(TinySynthFxParameter),
    ClearMidiCcAssignments,
    Panic,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackAction {
    Remove,
    MoveBefore(Option<TrackId>),
    NameChanged(String),
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
    TinySynthFx(TinySynthFxControl),
}

pub type TrackWidgetAction = TrackAction;

#[derive(Clone, Copy, Debug, PartialEq)]
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
    SetApplyNCycles(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiNote(u8);

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
pub enum PianoAction {
    Press(MidiNote),
    Release(MidiNote),
    ReleaseAll,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AppIntent {
    Loop {
        track_id: TrackId,
        loop_id: LoopId,
        action: LoopAction,
    },
    Track {
        track_id: TrackId,
        action: TrackAction,
    },
    Global(GlobalControlAction),
    Piano(PianoAction),
    AddTrack(DirectTrackSpec),
    AddTrackWithTopology(TrackSpec),
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
    KeyEvent(KeyEvent),
    AddScriptSource {
        name: String,
        source: Arc<str>,
        kind: ScriptKind,
        enabled: bool,
    },
    AddEphemeralScript {
        name: String,
        source: Arc<str>,
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
    InvokeScriptDialogButton {
        script_id: ScriptId,
        dialog_id: ScriptDialogId,
        button_id: ScriptDialogButtonId,
    },
    SetPortConnected {
        port_id: PortId,
        host_port_id: HostPortId,
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
    ResetXruns,
    RequestSaveSession,
    RequestLoadSessionPicker,
    LoadSessionBytes {
        name: String,
        bytes: Arc<[u8]>,
    },
    ConfirmSampleRateConversion {
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
    ReportFileIoError {
        task_id: Option<TaskId>,
        message: String,
    },
    PreviewClickTrack {
        loop_id: LoopId,
        request: ClickTrackRequest,
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
        bytes: Arc<[u8]>,
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
        bytes: Arc<[u8]>,
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
            Self::StopClicked => "loop.stop",
            Self::GainChanged(_) => "loop.gain",
            Self::BalanceChanged(_) => "loop.balance",
            Self::RestoreRecordedFxState => "loop.restore_recorded_fx",
            Self::ConvertToComposite => "loop.convert_to_composite",
            Self::Duplicate => "loop.duplicate",
            Self::DuplicateTo(_) => "loop.duplicate_to",
            Self::SwapWith(_) => "loop.swap_with",
            Self::MoveBefore(_) => "loop.move_before",
        }
    }
}

impl TinySynthFxControl {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::SelectPreset(_) => "track.tiny_synth_fx.select_preset",
            Self::SetMasterGainDb(_) => "track.tiny_synth_fx.master_gain",
            Self::SetReverbEnabled(_) => "track.tiny_synth_fx.reverb_enabled",
            Self::SetReverbAmount(_) => "track.tiny_synth_fx.reverb_amount",
            Self::SetDistortionEnabled(_) => "track.tiny_synth_fx.distortion_enabled",
            Self::SetDistortionDrive(_) => "track.tiny_synth_fx.distortion_drive",
            Self::SetCompressorEnabled(_) => "track.tiny_synth_fx.compressor_enabled",
            Self::SetCompressorAmount(_) => "track.tiny_synth_fx.compressor_amount",
            Self::SetEqEnabled(_) => "track.tiny_synth_fx.eq_enabled",
            Self::SetEqLowDb(_) => "track.tiny_synth_fx.eq_low",
            Self::SetEqMidDb(_) => "track.tiny_synth_fx.eq_mid",
            Self::SetEqHighDb(_) => "track.tiny_synth_fx.eq_high",
            Self::AssignMidiCc(_) => "track.tiny_synth_fx.midi_cc_assign",
            Self::RemoveMidiCc(_) => "track.tiny_synth_fx.midi_cc_remove",
            Self::ClearMidiCcAssignments => "track.tiny_synth_fx.midi_cc_clear",
            Self::Panic => "track.tiny_synth_fx.panic",
        }
    }
}

impl TrackAction {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Remove => "track.remove",
            Self::MoveBefore(_) => "track.move_before",
            Self::NameChanged(_) => "track.name",
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
            Self::TinySynthFx(control) => control.kind(),
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
            Self::Loop { action, .. } => action.kind(),
            Self::Track { action, .. } => action.kind(),
            Self::Global(action) => action.kind(),
            Self::Piano(action) => action.kind(),
            Self::AddTrack(_) => "track.add_direct",
            Self::AddTrackWithTopology(_) => "track.add_with_topology",
            Self::AddLoop { .. } => "loop.add_row",
            Self::ComposeLoopSerial { .. } => "loop.compose_serial",
            Self::ComposeLoopAt { .. } => "loop.compose_at",
            Self::KeyEvent(_) => "scripting.key_event",
            Self::AddScriptSource { .. } => "scripting.add_source",
            Self::AddEphemeralScript { .. } => "scripting.add_ephemeral",
            Self::SetScriptEnabled { .. } => "scripting.set_enabled",
            Self::RestartScript { .. } => "scripting.restart",
            Self::ReplaceScriptSource { .. } => "scripting.replace_source",
            Self::StopScript { .. } => "scripting.stop",
            Self::ForgetScript { .. } => "scripting.forget",
            Self::InvokeScriptDialogButton { .. } => "scripting.dialog_button",
            Self::SetPortConnected { .. } => "connection.set",
            Self::RefreshAudioDriverDiscovery { .. } => "audio_driver.refresh_discovery",
            Self::RequestAudioDriverSwitch { .. } => "audio_driver.request_switch",
            Self::ConfirmAudioDriverSwitch { .. } => "audio_driver.confirm_switch",
            Self::CompleteAudioDriverSwitchPersistence { .. } => {
                "audio_driver.complete_persistence"
            }
            Self::ResetXruns => "audio.reset_xruns",
            Self::RequestSaveSession => "session.request_save",
            Self::RequestLoadSessionPicker => "session.request_load_picker",
            Self::LoadSessionBytes { .. } => "session.load_bytes",
            Self::ConfirmSampleRateConversion { .. } => "io.confirm_sample_rate",
            Self::ConfirmAudioChannelMapping { .. } => "io.confirm_channel_mapping",
            Self::ConfirmAudioChannelSelection { .. } => "io.confirm_channel_selection",
            Self::CancelIoTask { .. } => "io.cancel",
            Self::ReportFileIoError { .. } => "io.report_error",
            Self::PreviewClickTrack { .. } => "click_track.preview",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationLevel {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppNotification {
    pub level: NotificationLevel,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ephemeral_script_display_name("controller.lua", std::iter::empty()),
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
            id: TrackProcessorTypeId::new("browser_native_test"),
            label: "Browser native test".to_owned(),
            available: true,
            unavailable_reason: None,
            constraints: TrackProcessorConstraints {
                max_dry_audio_channels: Some(4),
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
            },
        };
        assert_eq!(
            spec.validate(&[]),
            Err(TrackSpecError::ProcessorUnavailable)
        );
        assert!(spec.validate(std::slice::from_ref(&processor)).is_ok());
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
            },
            ..spec.clone()
        };
        assert_eq!(
            unsupported.validate(&[processor]),
            Err(TrackSpecError::UnsupportedShape)
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn tiny_synth_fx_constraints_require_matched_audio_and_midi() {
        let constraints = TrackProcessorConstraints {
            max_dry_audio_channels: None,
            max_wet_audio_channels: None,
            matching_audio_channels: true,
            midi: TrackProcessorMidiPolicy::Required,
        };
        for channels in [0, 1, 2, 7] {
            assert!(constraints.accepts(channels, channels, true));
        }
        assert!(!constraints.accepts(2, 1, true));
        assert!(!constraints.accepts(1, 1, false));
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
    }

    #[shoop_wasm_test_support::shoop_test]
    fn tiny_synth_controls_have_stable_intent_kinds() {
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::SelectPreset("pad".to_owned())).kind(),
            "track.tiny_synth_fx.select_preset"
        );
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::SetDistortionDrive(4.0)).kind(),
            "track.tiny_synth_fx.distortion_drive"
        );
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::AssignMidiCc(
                TinySynthFxMidiCcAssignment {
                    parameter: TinySynthFxParameter::EqLow,
                    channel: 2,
                    controller: 18,
                }
            ))
            .kind(),
            "track.tiny_synth_fx.midi_cc_assign"
        );
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::RemoveMidiCc(
                TinySynthFxParameter::EqLow
            ))
            .kind(),
            "track.tiny_synth_fx.midi_cc_remove"
        );
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::ClearMidiCcAssignments).kind(),
            "track.tiny_synth_fx.midi_cc_clear"
        );
        assert_eq!(
            TrackAction::TinySynthFx(TinySynthFxControl::Panic).kind(),
            "track.tiny_synth_fx.panic"
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
        assert_eq!(LUA_API_VERSION, LuaApiVersion { major: 1, minor: 2 });
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
            kind: ScriptKind::User,
            enabled: true,
            lifecycle: ScriptLifecycle::Listening,
            documentation: Some("Controller help\n".to_owned()),
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
