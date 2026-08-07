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

pub const MIN_TRACK_GAIN_DB: f32 = -30.0;
pub const MAX_TRACK_GAIN_DB: f32 = 20.0;

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
    pub apply_n_cycles: u32,
}

impl Default for GlobalControlState {
    fn default() -> Self {
        Self {
            default_recording_action: DefaultRecordingAction::Record,
            play_after_record: true,
            sync: true,
            solo: false,
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
    pub callback_budget_overruns: u32,
    pub render_discontinuities: u32,
    pub memory_growths: u32,
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

#[derive(Clone, Debug)]
pub struct LoopState {
    pub id: LoopId,
    pub name: String,
    pub position: f32,
    pub mode: LoopMode,
    pub next_mode: LoopMode,
    pub next_transition_delay: Option<u32>,
    pub empty: bool,
    pub composite_kind: CompositeKind,
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
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            id: LoopId::INVALID,
            name: "Loop".to_owned(),
            position: 0.0,
            mode: LoopMode::Unknown,
            next_mode: LoopMode::Unknown,
            next_transition_delay: None,
            empty: true,
            composite_kind: CompositeKind::None,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalPortConnectionState {
    pub full_name: String,
    pub eligible: bool,
    pub connected: bool,
    pub pending: Option<bool>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalPortConnectionState {
    pub id: PortId,
    pub track_id: TrackId,
    pub name: String,
    pub data_type: PortDataType,
    pub direction: PortDirection,
    pub role: PortRole,
    pub candidates: Arc<[ExternalPortConnectionState]>,
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
    pub ports: Arc<[LocalPortConnectionState]>,
    pub errors: Arc<[ConnectionErrorState]>,
}

impl Default for ConnectionViewState {
    fn default() -> Self {
        Self {
            revision: 0,
            loading: true,
            backend_available: false,
            ports: Arc::from([]),
            errors: Arc::from([]),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TrackState {
    pub id: TrackId,
    pub name: String,
    pub is_sync: bool,
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

#[derive(Clone, Debug, Default)]
pub struct LoopDetailsState {
    pub generation: u64,
    pub loop_id: LoopId,
    pub title: String,
    pub loading: bool,
    pub channels: Vec<WaveformChannelState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoTaskKind {
    SaveSession,
    LoadSession,
    ExportLoopAudio,
    ImportLoopAudio,
    ExportLoopMidi,
    ImportLoopMidi,
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

#[derive(Clone, Debug, Default)]
pub struct AppSnapshot {
    pub revision: u64,
    pub tracks: Vec<TrackState>,
    pub global_controls: GlobalControlState,
    pub status: StatusState,
    pub details: Option<LoopDetailsState>,
    pub connections: Arc<ConnectionViewState>,
    pub io_task: Option<IoTaskState>,
    pub notifications: Vec<AppNotification>,
}

pub type AppState = AppSnapshot;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectTrackSpec {
    pub name: String,
    pub audio_channels: u32,
    pub midi: bool,
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

#[derive(Clone, Debug, PartialEq)]
pub enum LoopAction {
    IconClicked(SelectionModifiers),
    IconDoubleClicked,
    PlayClicked,
    PlayDryClicked,
    RecordClicked,
    GrabClicked,
    RerecordClicked,
    StopClicked,
    GainChanged(f32),
    BalanceChanged(f32),
}

pub type LoopWidgetAction = LoopAction;

#[derive(Clone, Debug, PartialEq)]
pub enum TrackAction {
    NameChanged(String),
    OutputGainChanged(f32),
    OutputBalanceChanged(f32),
    OutputMuteChanged(bool),
    InputGainChanged(f32),
    InputBalanceChanged(f32),
    InputMonitoringChanged(bool),
}

pub type TrackWidgetAction = TrackAction;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GlobalControlAction {
    StopAll,
    DeselectAll,
    ClearRecordings { include_sync: bool },
    ClearAll { include_sync: bool },
    SetDefaultRecordingAction(DefaultRecordingAction),
    SetPlayAfterRecord(bool),
    SetSync(bool),
    SetSolo(bool),
    SetApplyNCycles(u32),
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
    AddTrack(DirectTrackSpec),
    AddLoop {
        track_id: TrackId,
    },
    SetPortConnected {
        port_id: PortId,
        external_port: String,
        connected: bool,
    },
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

    #[test]
    fn ids_retain_raw_identity_and_invalid_is_distinct() {
        let first = TrackId::from_raw(10);
        let second = TrackId::from_raw(11);
        assert!(first.is_valid());
        assert_ne!(first, second);
        assert_eq!(first.raw(), 10);
        assert!(!TrackId::INVALID.is_valid());
    }

    #[test]
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

    #[test]
    fn intents_preserve_stable_ids_and_selection_modifiers() {
        let track_id = TrackId::from_raw(7);
        let loop_id = LoopId::from_raw(42);
        let intent = AppIntent::Loop {
            track_id,
            loop_id,
            action: LoopAction::IconClicked(SelectionModifiers { additive: true }),
        };
        assert_eq!(
            intent,
            AppIntent::Loop {
                track_id,
                loop_id,
                action: LoopAction::IconClicked(SelectionModifiers { additive: true }),
            }
        );
    }

    #[test]
    fn connection_contract_preserves_identity_roles_and_exact_desired_state() {
        let port_id = PortId::from_raw(17);
        let track_id = TrackId::from_raw(3);
        let endpoint = "client:name:with:colons".to_owned();
        let port = LocalPortConnectionState {
            id: port_id,
            track_id,
            name: "Track direct in".to_owned(),
            data_type: PortDataType::Audio,
            direction: PortDirection::Input,
            role: PortRole::AudioInput,
            candidates: Arc::from([ExternalPortConnectionState {
                full_name: endpoint.clone(),
                eligible: true,
                connected: false,
                pending: Some(true),
                error: None,
            }]),
        };
        assert_eq!(port.track_id, track_id);
        assert_eq!(port.role, PortRole::AudioInput);
        assert_eq!(port.candidates[0].full_name, endpoint);
        assert_eq!(
            AppIntent::SetPortConnected {
                port_id,
                external_port: endpoint.clone(),
                connected: true,
            },
            AppIntent::SetPortConnected {
                port_id,
                external_port: endpoint,
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

    #[test]
    fn connection_snapshots_are_structurally_shared_and_independent() {
        let ports: Arc<[LocalPortConnectionState]> = Arc::from([]);
        let first = AppSnapshot {
            connections: Arc::new(ConnectionViewState {
                revision: 4,
                loading: false,
                backend_available: true,
                ports: Arc::clone(&ports),
                errors: Arc::from([]),
            }),
            ..Default::default()
        };
        let mut second = first.clone();
        second.revision = 9;
        assert!(Arc::ptr_eq(&first.connections, &second.connections));
        assert!(Arc::ptr_eq(
            &first.connections.ports,
            &second.connections.ports
        ));
        assert_eq!(first.revision, 0);
        assert_eq!(first.connections.revision, 4);
    }

    #[test]
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

    #[test]
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
