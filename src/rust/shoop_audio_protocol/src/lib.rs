use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 15;
pub const COMMAND_CAPACITY: usize = 256;
pub const COMMAND_MAX_BYTES: usize = 64 * 1024;
pub const SESSION_TRANSFER_CHUNK_BYTES: usize = 2 * 1024;
pub const SESSION_TRANSFER_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const WAVEFORM_CHUNK_SAMPLES: usize = 512;
pub const MIDI_DETAIL_CHUNK_EVENTS: usize = 16;
pub const STATUS_INTERVAL_MS: u32 = 50;
pub const MAX_DEVICE_AUDIO_CHANNELS: usize = 2;
pub const MIDI_BATCH_CAPACITY: usize = 128;
pub const MIDI_OUTPUT_QUEUE_CAPACITY: usize = 1024;
pub const TRACK_MIDI_MESSAGE_BYTES: usize = 4;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CommandEnvelope {
    pub version: u16,
    pub sequence: u64,
    pub command: Command,
}

impl CommandEnvelope {
    pub fn new(sequence: u64, command: Command) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            sequence,
            command,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Command {
    ConfigureDeviceChannels {
        input_channels: u32,
        output_channels: u32,
    },
    ConfigureMidiEndpoints {
        endpoints: Vec<WireHostPort>,
    },
    PushMidiInput {
        host_port_id: String,
        events: Vec<WireMidiEvent>,
    },
    InjectTrackMidiInput {
        track_id: u64,
        events: Vec<WireMidiEvent>,
    },
    DrainMidiOutput {
        max_events: usize,
    },
    CreateTrack {
        expected_track_id: u64,
        expected_loop_ids: Vec<u64>,
        port_name_base: String,
        topology: WireTrackTopology,
    },
    RemoveTrack {
        track_id: u64,
    },
    AddLoop {
        track_id: u64,
        expected_loop_id: u64,
    },
    CreateComposite {
        expected_composite_id: u64,
    },
    ConfigureComposite {
        composite_id: u64,
        config: WireCompositeConfig,
    },
    TransitionComposite {
        composite_id: u64,
        mode: WireLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    },
    SetCompositePlayAfterRecord {
        composite_id: u64,
        enabled: bool,
    },
    RemoveComposite {
        composite_id: u64,
    },
    SetTrackControl {
        track_id: u64,
        control: WireTrackControl,
    },
    SetTrackFxControl {
        track_id: u64,
        control: WireTrackFxControl,
    },
    SetLoopGain {
        loop_id: u64,
        gain: f32,
    },
    SetLoopBalance {
        loop_id: u64,
        balance: f32,
    },
    GrabLoops {
        requests: Vec<WireGrabRequest>,
    },
    SetLoopSyncSource {
        loop_id: u64,
        source: Option<u64>,
    },
    TransitionLoop {
        loop_id: u64,
        mode: WireLoopMode,
        cycles_delay: Option<u32>,
    },
    ClearLoop {
        loop_id: u64,
    },
    SetLoopLength {
        loop_id: u64,
        length: u32,
    },
    SetLoopTiming {
        loop_id: u64,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    },
    BeginLoopContentReplace {
        generation: u64,
        loop_id: u64,
        total_bytes: usize,
    },
    WriteLoopContentReplace {
        generation: u64,
        offset: usize,
        bytes: Vec<u8>,
    },
    CommitLoopContentReplace {
        generation: u64,
    },
    SetPortConnected {
        application_port_id: u64,
        host_port_id: String,
        connected: bool,
    },
    RequestWaveform {
        loop_id: u64,
        revision: u64,
        channel: usize,
        offset: usize,
        max_samples: usize,
    },
    RequestMidiData {
        loop_id: u64,
        generation: u64,
        channel: usize,
        offset: usize,
        max_events: usize,
    },
    BeginSessionCapture {
        generation: u64,
    },
    ReadSessionCapture {
        generation: u64,
        offset: usize,
        max_bytes: usize,
    },
    BeginSessionReplace {
        generation: u64,
        total_bytes: usize,
    },
    WriteSessionReplace {
        generation: u64,
        offset: usize,
        bytes: Vec<u8>,
    },
    CommitSessionReplace {
        generation: u64,
    },
    AbortSessionTransfer {
        generation: u64,
    },
    BeginSoundFontImport {
        generation: u64,
        original_filename: String,
        total_bytes: usize,
    },
    WriteSoundFontImport {
        generation: u64,
        offset: usize,
        bytes: Vec<u8>,
    },
    CommitSoundFontImport {
        generation: u64,
    },
    Poll,
    Shutdown,
}

impl Command {
    pub fn supersedes_in_journal(&self, existing: &Self) -> bool {
        match (existing, self) {
            (
                Self::SetTrackControl {
                    track_id: existing_track,
                    control: existing_control,
                },
                Self::SetTrackControl {
                    track_id: replacement_track,
                    control: replacement_control,
                },
            ) => {
                existing_track == replacement_track
                    && std::mem::discriminant(existing_control)
                        == std::mem::discriminant(replacement_control)
            }
            (
                Self::SetTrackFxControl {
                    track_id: existing_track,
                    control: existing_control,
                },
                Self::SetTrackFxControl {
                    track_id: replacement_track,
                    control: replacement_control,
                },
            ) => {
                existing_track == replacement_track
                    && existing_control.supersedable_parameter()
                        == replacement_control.supersedable_parameter()
                    && replacement_control.supersedable_parameter().is_some()
            }
            (
                Self::SetLoopGain {
                    loop_id: existing_loop,
                    ..
                },
                Self::SetLoopGain {
                    loop_id: replacement_loop,
                    ..
                },
            )
            | (
                Self::SetLoopBalance {
                    loop_id: existing_loop,
                    ..
                },
                Self::SetLoopBalance {
                    loop_id: replacement_loop,
                    ..
                },
            )
            | (
                Self::SetLoopSyncSource {
                    loop_id: existing_loop,
                    ..
                },
                Self::SetLoopSyncSource {
                    loop_id: replacement_loop,
                    ..
                },
            )
            | (
                Self::TransitionLoop {
                    loop_id: existing_loop,
                    ..
                },
                Self::TransitionLoop {
                    loop_id: replacement_loop,
                    ..
                },
            )
            | (
                Self::ClearLoop {
                    loop_id: existing_loop,
                },
                Self::ClearLoop {
                    loop_id: replacement_loop,
                },
            )
            | (
                Self::SetLoopLength {
                    loop_id: existing_loop,
                    ..
                },
                Self::SetLoopLength {
                    loop_id: replacement_loop,
                    ..
                },
            ) => existing_loop == replacement_loop,
            (
                Self::SetLoopTiming {
                    loop_id: existing_loop,
                    start_offset: existing_start_offset,
                    preplay: existing_preplay,
                    length: existing_length,
                },
                Self::SetLoopTiming {
                    loop_id: replacement_loop,
                    start_offset: replacement_start_offset,
                    preplay: replacement_preplay,
                    length: replacement_length,
                },
            ) => {
                existing_loop == replacement_loop
                    && (existing_start_offset.is_none() || replacement_start_offset.is_some())
                    && (existing_preplay.is_none() || replacement_preplay.is_some())
                    && (existing_length.is_none() || replacement_length.is_some())
            }
            (Self::ConfigureDeviceChannels { .. }, Self::ConfigureDeviceChannels { .. })
            | (Self::ConfigureMidiEndpoints { .. }, Self::ConfigureMidiEndpoints { .. }) => true,
            (
                Self::SetPortConnected {
                    application_port_id: existing_port,
                    host_port_id: existing_host,
                    ..
                },
                Self::SetPortConnected {
                    application_port_id: replacement_port,
                    host_port_id: replacement_host,
                    ..
                },
            ) => existing_port == replacement_port && existing_host == replacement_host,
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "control", content = "value", rename_all = "snake_case")]
pub enum WireTrackControl {
    OutputGainDb(f32),
    OutputBalance(f32),
    OutputMute(bool),
    InputGainDb(f32),
    InputBalance(f32),
    InputMonitoring(bool),
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WireTrackTopology {
    Direct { audio_channels: u32, midi: bool },
    TinySynthFx { audio_channels: u32 },
    OxiSynth,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "control", content = "value", rename_all = "snake_case")]
pub enum WireTrackFxControl {
    SetActive(bool),
    SetVisible(bool),
    ToggleOrRecover,
    RestoreState(String),
    ClearLogs,
    TinySelectPreset(String),
    TinySetMasterGainDb(f32),
    TinySetReverbEnabled(bool),
    TinySetReverbAmount(f32),
    TinySetDistortionEnabled(bool),
    TinySetDistortionDrive(f32),
    TinySetCompressorEnabled(bool),
    TinySetCompressorAmount(f32),
    TinySetEqEnabled(bool),
    TinySetEqLowDb(f32),
    TinySetEqMidDb(f32),
    TinySetEqHighDb(f32),
    TinyAssignMidiCc(WireTinySynthFxMidiCcAssignment),
    TinyRemoveMidiCc(WireTinySynthFxParameter),
    TinyClearMidiCcAssignments,
    TinyPanic,
    OxiSetMasterGain(f32),
    OxiSetReverb(WireOxiSynthReverbState),
    OxiSetChorus(WireOxiSynthChorusState),
    OxiSelectProgram {
        channel: u8,
        bank: u32,
        program: u8,
    },
    OxiSelectSoundFont(String),
    OxiAudition {
        channel: u8,
        key: u8,
        velocity: u8,
        pressed: bool,
    },
    OxiPanic,
}

impl WireTrackFxControl {
    fn supersedable_parameter(&self) -> Option<u8> {
        Some(match self {
            Self::SetActive(_) => 0,
            Self::TinySetMasterGainDb(_) => 1,
            Self::TinySetReverbAmount(_) => 2,
            Self::TinySetDistortionDrive(_) => 3,
            Self::TinySetCompressorAmount(_) => 4,
            Self::TinySetEqLowDb(_) => 5,
            Self::TinySetEqMidDb(_) => 6,
            Self::TinySetEqHighDb(_) => 7,
            Self::OxiSetMasterGain(_) => 8,
            Self::OxiSetReverb(_) => 9,
            Self::OxiSetChorus(_) => 10,
            Self::OxiSelectProgram { channel, .. } => 16u8.saturating_add(*channel),
            Self::OxiSelectSoundFont(_) => 15,
            Self::SetVisible(_)
            | Self::ToggleOrRecover
            | Self::RestoreState(_)
            | Self::ClearLogs
            | Self::TinySelectPreset(_)
            | Self::TinySetReverbEnabled(_)
            | Self::TinySetDistortionEnabled(_)
            | Self::TinySetCompressorEnabled(_)
            | Self::TinySetEqEnabled(_)
            | Self::TinyAssignMidiCc(_)
            | Self::TinyRemoveMidiCc(_)
            | Self::TinyClearMidiCcAssignments
            | Self::TinyPanic
            | Self::OxiAudition { .. }
            | Self::OxiPanic => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireLoopMode {
    #[default]
    Unknown,
    Stopped,
    Playing,
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}

#[derive(Clone, Copy, Debug, Default, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireChannelMode {
    #[default]
    Direct,
    Dry,
    Wet,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireCompositeKind {
    Regular,
    Script,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum WireCompositeTarget {
    Loop(u64),
    Composite(u64),
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireCompositeEntry {
    pub target: WireCompositeTarget,
    pub delay: i64,
    pub n_cycles: Option<i64>,
    pub mode: Option<WireLoopMode>,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireCompositeConfig {
    pub kind: WireCompositeKind,
    pub sync_source: u64,
    pub timelines: Vec<Vec<Vec<WireCompositeEntry>>>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireGrabRequest {
    pub loop_id: u64,
    pub reverse_start_cycle: Option<i32>,
    pub cycles_length: Option<i32>,
    pub go_to_cycle: Option<i32>,
    pub go_to_mode: WireLoopMode,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct EventEnvelope {
    pub version: u16,
    pub sequence: u64,
    pub event: Event,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Ack,
    Error {
        message: String,
    },
    ConnectionMutationFailed {
        application_port_id: u64,
        host_port_id: String,
        desired_connected: bool,
        message: String,
    },
    MidiOutput {
        events: Vec<WireMidiOutputEvent>,
        dropped: u32,
        refused_input: u32,
    },
    Snapshot(WireSnapshot),
    Waveform(WaveformChunk),
    MidiData(MidiDataChunk),
    SessionCaptureReady {
        generation: u64,
        total_bytes: usize,
    },
    SessionCaptureChunk {
        generation: u64,
        offset: usize,
        total_bytes: usize,
        final_chunk: bool,
        bytes: Vec<u8>,
    },
    SessionReplaceComplete {
        generation: u64,
    },
    LoopContentReplaceComplete {
        generation: u64,
    },
    SessionTransferAborted {
        generation: u64,
    },
    Stopped,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WireSnapshot {
    pub sample_rate: u32,
    pub quantum: u32,
    pub callback_count: u64,
    pub processed_frames: u64,
    pub input_peak: f32,
    pub output_peak: f32,
    pub xruns: u32,
    pub callback_budget_overruns: u32,
    pub render_discontinuities: u32,
    pub memory_growths: u32,
    pub render_memory_growths: u32,
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
    pub tracks: Vec<WireTrackState>,
    pub loops: Vec<WireLoopState>,
    pub composites: Vec<WireCompositeState>,
    pub application_ports: Vec<WireApplicationPort>,
    pub host_ports: Vec<WireHostPort>,
    pub confirmed_links: Vec<WireConfirmedLink>,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WirePortDataType {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WirePortDirection {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WirePortRole {
    AudioInput,
    AudioOutput,
    AudioSend,
    AudioReturn,
    MidiInput,
    MidiOutput,
    MidiSend,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireApplicationPort {
    pub id: u64,
    pub owner: WireApplicationPortOwner,
    pub name: String,
    pub data_type: WirePortDataType,
    pub direction: WirePortDirection,
    pub role: WirePortRole,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireApplicationPortOwner {
    Track,
    GlobalFxControl,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireHostPort {
    pub id: String,
    pub name: String,
    pub data_type: WirePortDataType,
    pub direction: WirePortDirection,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireConfirmedLink {
    pub application_port_id: u64,
    pub host_port_id: String,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WireTinySynthFxParameter {
    MasterGain,
    ReverbAmount,
    DistortionDrive,
    CompressorAmount,
    EqLow,
    EqMid,
    EqHigh,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireTinySynthFxMidiCcAssignment {
    pub parameter: WireTinySynthFxParameter,
    pub channel: u8,
    pub controller: u8,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireLatestMidiMessage {
    pub bytes: [u8; 4],
    pub len: u8,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireMidiEvent {
    pub frame: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireMidiOutputEvent {
    pub application_port_id: u64,
    pub host_port_id: String,
    pub frame: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireTrackState {
    pub id: u64,
    pub topology: WireTrackTopology,
    pub fx: Option<WireTrackFxState>,
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
    #[serde(default)]
    pub latest_input_midi_message: Option<WireLatestMidiMessage>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireTrackFxState {
    pub processor_type: String,
    pub active: bool,
    pub visible: bool,
    pub tiny: Option<WireTinySynthFxState>,
    #[serde(default)]
    pub oxisynth: Option<WireOxiSynthState>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireOxiSynthState {
    pub soundfont_sha256: String,
    pub soundfont_name: String,
    pub presets: Vec<WireOxiSynthPreset>,
    pub revision: u64,
    pub midi_activity_revision: u64,
    pub master_gain: f32,
    pub reverb: WireOxiSynthReverbState,
    pub chorus: WireOxiSynthChorusState,
    pub channels: Vec<WireOxiSynthChannelState>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireOxiSynthReverbState {
    pub room_size: f32,
    pub damp: f32,
    pub width: f32,
    pub level: f32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireOxiSynthChorusState {
    pub voices: u32,
    pub level: f32,
    pub speed_hz: f32,
    pub depth_ms: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireOxiSynthPreset {
    pub bank: u32,
    pub program: u8,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireOxiSynthChannelState {
    pub baseline_bank: u32,
    pub baseline_program: u8,
    pub current_bank: u32,
    pub current_program: u8,
    pub volume: u8,
    pub pan: u8,
    pub expression: u8,
    pub pitch_bend: u16,
    pub channel_pressure: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WireTinySynthFxState {
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
    #[serde(default)]
    pub midi_cc_assignments: Vec<WireTinySynthFxMidiCcAssignment>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WireLoopState {
    pub id: u64,
    pub mode: WireLoopMode,
    pub length: u32,
    pub position: u32,
    pub next_mode: Option<WireLoopMode>,
    pub next_transition_delay: Option<u32>,
    pub stereo: bool,
    pub gain: f32,
    pub balance: f32,
    pub audio_peaks: Vec<f32>,
    pub midi_activity: bool,
}

#[derive(Clone, Copy, Debug, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireActiveCompositeChild {
    pub target: WireCompositeTarget,
    pub mode: WireLoopMode,
    pub cycle_offset: u32,
}

#[derive(Clone, Debug, Default, Eq, Serialize, Deserialize, PartialEq)]
pub struct WireCompositeState {
    pub id: u64,
    pub mode: WireLoopMode,
    pub next_mode: Option<WireLoopMode>,
    pub next_transition_delay: Option<u32>,
    pub iteration: u32,
    pub cycle_count: u64,
    pub length: u64,
    pub position: u64,
    pub active_plan_version: u64,
    pub pending_plan_version: Option<u64>,
    pub active_children: Vec<WireActiveCompositeChild>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WaveformChunk {
    pub loop_id: u64,
    pub revision: u64,
    pub channel: usize,
    pub channel_count: usize,
    pub offset: usize,
    pub total_samples: usize,
    pub start_offset: i32,
    pub preplay: u32,
    pub final_chunk: bool,
    pub samples: Vec<f32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct MidiDataChunk {
    pub loop_id: u64,
    pub generation: u64,
    pub content_revision: u64,
    pub mode: WireChannelMode,
    pub channel: usize,
    pub channel_count: usize,
    pub offset: usize,
    pub total_events: usize,
    pub length: u32,
    pub start_offset: i32,
    pub preplay: u32,
    pub final_chunk: bool,
    pub events: Vec<WireMidiEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

    #[shoop_wasm_test_support::shoop_test]
    fn composite_configuration_round_trips_without_losing_targets_or_modes() {
        let command = CommandEnvelope::new(
            41,
            Command::ConfigureComposite {
                composite_id: 7,
                config: WireCompositeConfig {
                    kind: WireCompositeKind::Script,
                    sync_source: 1,
                    timelines: vec![vec![vec![WireCompositeEntry {
                        target: WireCompositeTarget::Composite(8),
                        delay: 2,
                        n_cycles: Some(3),
                        mode: Some(WireLoopMode::Recording),
                    }]]],
                },
            },
        );
        let encoded = serde_json::to_vec(&command).unwrap();
        assert!(encoded.len() <= COMMAND_MAX_BYTES);
        assert_eq!(
            serde_json::from_slice::<CommandEnvelope>(&encoded).unwrap(),
            command
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn journal_coalesces_only_superseded_controls_for_the_same_entity() {
        let first = Command::SetTrackControl {
            track_id: 2,
            control: WireTrackControl::OutputGainDb(-3.0),
        };
        let replacement = Command::SetTrackControl {
            track_id: 2,
            control: WireTrackControl::OutputGainDb(-6.0),
        };
        let other_control = Command::SetTrackControl {
            track_id: 2,
            control: WireTrackControl::OutputMute(true),
        };
        let other_track = Command::SetTrackControl {
            track_id: 3,
            control: WireTrackControl::OutputGainDb(-6.0),
        };
        assert!(replacement.supersedes_in_journal(&first));
        assert!(!other_control.supersedes_in_journal(&first));
        assert!(!other_track.supersedes_in_journal(&first));

        let route = Command::SetPortConnected {
            application_port_id: 9,
            host_port_id: "webaudio:destination_1".to_owned(),
            connected: false,
        };
        let same_route = Command::SetPortConnected {
            application_port_id: 9,
            host_port_id: "webaudio:destination_1".to_owned(),
            connected: true,
        };
        let other_route = Command::SetPortConnected {
            application_port_id: 9,
            host_port_id: "webaudio:destination_2".to_owned(),
            connected: true,
        };
        assert!(same_route.supersedes_in_journal(&route));
        assert!(!other_route.supersedes_in_journal(&route));
        assert!(Command::ConfigureDeviceChannels {
            input_channels: 1,
            output_channels: 2,
        }
        .supersedes_in_journal(&Command::ConfigureDeviceChannels {
            input_channels: 0,
            output_channels: 1,
        }));
        let gain = Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::TinySetMasterGainDb(-12.0),
        };
        let replacement_gain = Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::TinySetMasterGainDb(-18.0),
        };
        let panic = Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::TinyPanic,
        };
        assert!(replacement_gain.supersedes_in_journal(&gain));
        assert!(!panic.supersedes_in_journal(&gain));
        assert!(!Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::SetVisible(false),
        }
        .supersedes_in_journal(&Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::SetVisible(true),
        }));
        assert!(!Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::TinySetReverbEnabled(false),
        }
        .supersedes_in_journal(&Command::SetTrackFxControl {
            track_id: 4,
            control: WireTrackFxControl::TinySetReverbEnabled(true),
        }));

        assert!(Command::ConfigureMidiEndpoints {
            endpoints: vec![WireHostPort {
                id: "webmidi:source:new".to_owned(),
                name: "new".to_owned(),
                data_type: WirePortDataType::Midi,
                direction: WirePortDirection::Output,
            }],
        }
        .supersedes_in_journal(&Command::ConfigureMidiEndpoints {
            endpoints: Vec::new(),
        }));

        let start_only = Command::SetLoopTiming {
            loop_id: 5,
            start_offset: Some(-8),
            preplay: None,
            length: None,
        };
        let length_only = Command::SetLoopTiming {
            loop_id: 5,
            start_offset: None,
            preplay: None,
            length: Some(64),
        };
        let complete = Command::SetLoopTiming {
            loop_id: 5,
            start_offset: Some(-4),
            preplay: Some(12),
            length: Some(96),
        };
        assert!(!length_only.supersedes_in_journal(&start_only));
        assert!(!start_only.supersedes_in_journal(&length_only));
        assert!(complete.supersedes_in_journal(&start_only));
        assert!(complete.supersedes_in_journal(&length_only));
        assert!(!Command::SetLoopTiming {
            loop_id: 6,
            start_offset: Some(-4),
            preplay: Some(12),
            length: Some(96),
        }
        .supersedes_in_journal(&complete));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn global_application_port_owner_round_trips_explicitly() {
        let port = WireApplicationPort {
            id: 99,
            owner: WireApplicationPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".to_owned(),
            data_type: WirePortDataType::Midi,
            direction: WirePortDirection::Input,
            role: WirePortRole::MidiInput,
        };
        let encoded = serde_json::to_vec(&port).unwrap();
        assert_eq!(
            serde_json::from_slice::<WireApplicationPort>(&encoded).unwrap(),
            port
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_detail_chunks_round_trip_with_request_identity_and_metadata() {
        let request = CommandEnvelope::new(
            4,
            Command::RequestMidiData {
                loop_id: 8,
                generation: 3,
                channel: 1,
                offset: 128,
                max_events: MIDI_DETAIL_CHUNK_EVENTS,
            },
        );
        let request_json = serde_json::to_vec(&request).unwrap();
        assert_eq!(
            serde_json::from_slice::<CommandEnvelope>(&request_json).unwrap(),
            request
        );

        let response = EventEnvelope {
            version: PROTOCOL_VERSION,
            sequence: 4,
            event: Event::MidiData(MidiDataChunk {
                loop_id: 8,
                generation: 3,
                content_revision: 12,
                mode: WireChannelMode::Dry,
                channel: 1,
                channel_count: 2,
                offset: 128,
                total_events: 129,
                length: 512,
                start_offset: -7,
                preplay: 9,
                final_chunk: true,
                events: vec![WireMidiEvent {
                    frame: 400,
                    data: vec![0x90, 60, 100],
                }],
            }),
        };
        let response_json = serde_json::to_vec(&response).unwrap();
        assert_eq!(
            serde_json::from_slice::<EventEnvelope>(&response_json).unwrap(),
            response
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn production_envelopes_have_stable_json_bytes() {
        let command = serde_json::to_string(&CommandEnvelope::new(17, Command::Poll)).unwrap();
        assert_eq!(
            command,
            r#"{"version":13,"sequence":17,"command":{"kind":"poll"}}"#
        );

        let event = serde_json::to_string(&EventEnvelope {
            version: PROTOCOL_VERSION,
            sequence: 17,
            event: Event::Ack,
        })
        .unwrap();
        assert_eq!(
            event,
            r#"{"version":13,"sequence":17,"event":{"kind":"ack"}}"#
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn protocol_round_trip_preserves_sequence_and_stable_ids() {
        let command = CommandEnvelope::new(
            42,
            Command::CreateTrack {
                expected_track_id: 7,
                expected_loop_ids: vec![8, 9],
                port_name_base: "track".to_owned(),
                topology: WireTrackTopology::Direct {
                    audio_channels: 2,
                    midi: false,
                },
            },
        );
        let encoded = serde_json::to_string(&command).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, command);

        let grab = CommandEnvelope::new(
            43,
            Command::GrabLoops {
                requests: vec![WireGrabRequest {
                    loop_id: 9,
                    reverse_start_cycle: Some(2),
                    cycles_length: Some(1),
                    go_to_cycle: Some(0),
                    go_to_mode: WireLoopMode::Playing,
                }],
            },
        );
        let encoded = serde_json::to_string(&grab).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, grab);

        let midi = CommandEnvelope::new(
            44,
            Command::PushMidiInput {
                host_port_id: "webmidi:source:controller".to_owned(),
                events: vec![WireMidiEvent {
                    frame: 0,
                    data: vec![0x90, 60, 100],
                }],
            },
        );
        let encoded = serde_json::to_string(&midi).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, midi);

        let piano = CommandEnvelope::new(
            45,
            Command::InjectTrackMidiInput {
                track_id: 7,
                events: vec![WireMidiEvent {
                    frame: 0,
                    data: vec![0x80, 60, 0],
                }],
            },
        );
        let encoded = serde_json::to_string(&piano).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, piano);

        let tiny_topology = CommandEnvelope::new(
            46,
            Command::CreateTrack {
                expected_track_id: 10,
                expected_loop_ids: vec![11],
                port_name_base: "tiny".to_owned(),
                topology: WireTrackTopology::TinySynthFx { audio_channels: 7 },
            },
        );
        let encoded = serde_json::to_string(&tiny_topology).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, tiny_topology);

        let oxisynth_topology = CommandEnvelope::new(
            47,
            Command::CreateTrack {
                expected_track_id: 12,
                expected_loop_ids: vec![13],
                port_name_base: "oxisynth".to_owned(),
                topology: WireTrackTopology::OxiSynth,
            },
        );
        let encoded = serde_json::to_string(&oxisynth_topology).unwrap();
        let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, oxisynth_topology);

        for (index, control) in [
            WireTrackFxControl::SetActive(false),
            WireTrackFxControl::SetVisible(true),
            WireTrackFxControl::RestoreState("state".to_owned()),
            WireTrackFxControl::TinySelectPreset("pad".to_owned()),
            WireTrackFxControl::TinySetMasterGainDb(-12.0),
            WireTrackFxControl::TinySetReverbEnabled(true),
            WireTrackFxControl::TinySetReverbAmount(0.4),
            WireTrackFxControl::TinySetDistortionEnabled(true),
            WireTrackFxControl::TinySetDistortionDrive(8.0),
            WireTrackFxControl::TinySetCompressorEnabled(true),
            WireTrackFxControl::TinySetCompressorAmount(0.6),
            WireTrackFxControl::TinySetEqEnabled(true),
            WireTrackFxControl::TinySetEqLowDb(3.0),
            WireTrackFxControl::TinySetEqMidDb(-2.0),
            WireTrackFxControl::TinySetEqHighDb(1.5),
            WireTrackFxControl::TinyAssignMidiCc(WireTinySynthFxMidiCcAssignment {
                parameter: WireTinySynthFxParameter::EqHigh,
                channel: 3,
                controller: 74,
            }),
            WireTrackFxControl::TinyRemoveMidiCc(WireTinySynthFxParameter::EqHigh),
            WireTrackFxControl::TinyClearMidiCcAssignments,
            WireTrackFxControl::TinyPanic,
        ]
        .into_iter()
        .enumerate()
        {
            let command = CommandEnvelope::new(
                47 + index as u64,
                Command::SetTrackFxControl {
                    track_id: 10,
                    control,
                },
            );
            let encoded = serde_json::to_string(&command).unwrap();
            let decoded: CommandEnvelope = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, command);
        }
    }
}
