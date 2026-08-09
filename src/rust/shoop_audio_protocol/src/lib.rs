use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 4;
pub const COMMAND_CAPACITY: usize = 256;
pub const COMMAND_MAX_BYTES: usize = 16 * 1024;
pub const SESSION_TRANSFER_CHUNK_BYTES: usize = 2 * 1024;
pub const SESSION_TRANSFER_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const WAVEFORM_CHUNK_SAMPLES: usize = 512;
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
    DrainMidiOutput {
        max_events: usize,
    },
    CreateTrack {
        expected_track_id: u64,
        expected_loop_ids: Vec<u64>,
        port_name_base: String,
        audio_channels: u32,
        midi: bool,
    },
    AddLoop {
        track_id: u64,
        expected_loop_id: u64,
    },
    SetTrackControl {
        track_id: u64,
        control: WireTrackControl,
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
            ) => existing_loop == replacement_loop,
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
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
    pub tracks: Vec<WireTrackState>,
    pub loops: Vec<WireLoopState>,
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
    pub name: String,
    pub data_type: WirePortDataType,
    pub direction: WirePortDirection,
    pub role: WirePortRole,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WireTrackState {
    pub id: u64,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WaveformChunk {
    pub loop_id: u64,
    pub revision: u64,
    pub channel: usize,
    pub channel_count: usize,
    pub offset: usize,
    pub total_samples: usize,
    pub final_chunk: bool,
    pub samples: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
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
    }

    #[test]
    fn protocol_round_trip_preserves_sequence_and_stable_ids() {
        let command = CommandEnvelope::new(
            42,
            Command::CreateTrack {
                expected_track_id: 7,
                expected_loop_ids: vec![8, 9],
                port_name_base: "track".to_owned(),
                audio_channels: 2,
                midi: false,
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
    }
}
