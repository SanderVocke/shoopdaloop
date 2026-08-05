use std::sync::Arc;

use crate::{LoopState, LoopWidgetAction};

pub const MIN_TRACK_GAIN_DB: f32 = -30.0;
pub const MAX_TRACK_GAIN_DB: f32 = 20.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DefaultRecordingAction {
    #[default]
    Record,
    Grab,
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

#[derive(Clone, Debug, Default)]
pub struct StatusState {
    pub version: String,
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
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

#[derive(Clone, Debug, Default)]
pub struct TrackState {
    pub name: String,
    pub loops: Vec<LoopState>,
    pub controls: TrackControlState,
}

#[derive(Clone, Debug)]
pub struct WaveformChannelState {
    pub id: String,
    pub samples: Arc<[f32]>,
    pub start_offset: i64,
    pub loop_length: u64,
    pub played_sample: Option<i64>,
}

impl Default for WaveformChannelState {
    fn default() -> Self {
        Self {
            id: String::new(),
            samples: Arc::from([]),
            start_offset: 0,
            loop_length: 0,
            played_sample: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct LoopDetailsState {
    pub title: String,
    pub loading: bool,
    pub channels: Vec<WaveformChannelState>,
}

#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub tracks: Vec<TrackState>,
    pub global_controls: GlobalControlState,
    pub status: StatusState,
    pub details: Option<LoopDetailsState>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackWidgetAction {
    NameChanged(String),
    OutputGainChanged(f32),
    OutputBalanceChanged(f32),
    OutputMuteChanged(bool),
    InputGainChanged(f32),
    InputBalanceChanged(f32),
    InputMonitoringChanged(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexedTrackAction {
    pub track_index: usize,
    pub action: TrackWidgetAction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IndexedLoopAction {
    pub track_index: usize,
    pub loop_index: usize,
    pub action: LoopWidgetAction,
}

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
pub enum AppAction {
    Loop(IndexedLoopAction),
    Track(IndexedTrackAction),
    Global(GlobalControlAction),
}

#[cfg(test)]
mod tests {
    use super::*;

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
