//! The state shapes the application-facing backend interface reports.
//!
//! Scalar fields are published through per-object mirrors. Port names remain handle-owned
//! strings and are supplied when a mirror is read, so the process thread never clones them.

use crate::channel_mode::ChannelMode;
use crate::latency_runtime::{PublishedLatencyRecipe, RuntimeLatencyObservation};
use crate::loop_mode::LoopMode;

#[derive(Clone, Debug, PartialEq)]
pub struct LoopState {
    pub mode: LoopMode,
    pub length: u32,
    pub position: u32,
    pub cycle_count: u64,
    pub maybe_next_mode: Option<LoopMode>,
    pub maybe_next_mode_delay: Option<u32>,
    pub deferred_latency_mode: Option<LoopMode>,
    pub current_latency_recipe: PublishedLatencyRecipe,
    pub latched_latency_recipe: PublishedLatencyRecipe,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            mode: LoopMode::Unknown,
            length: 0,
            position: 0,
            cycle_count: 0,
            maybe_next_mode: None,
            maybe_next_mode_delay: None,
            deferred_latency_mode: None,
            current_latency_recipe: PublishedLatencyRecipe::default(),
            latched_latency_recipe: PublishedLatencyRecipe::default(),
        }
    }
}

/// Audio-channel state exposed through the application-facing backend interface.
///
/// Also what the audio thread publishes per cycle: every field is a plain number, so there
/// is nothing here it cannot fill in place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioChannelState {
    pub mode: ChannelMode,
    pub gain: f32,
    pub output_peak: f32,
    pub length: u32,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub render_advance_frames: u32,
    pub played_back_sample: Option<i32>,
    pub logical_played_position: Option<i32>,
    pub raw_played_position: Option<i32>,
    pub dispatch_position: Option<i32>,
    pub n_preplay_samples: u32,
    pub latency_retention_incomplete: bool,
    pub latency_history_variable: bool,
    pub latency_history_revisions: u32,
    pub data_dirty: bool,
    pub current_latency_recipe: PublishedLatencyRecipe,
    pub latched_latency_recipe: PublishedLatencyRecipe,
}

/// MIDI-channel state exposed through the application-facing backend interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiChannelState {
    pub mode: ChannelMode,
    pub n_events_triggered: u32,
    pub n_notes_active: u32,
    pub length: u32,
    pub start_offset: i32,
    pub capture_alignment_frames: i32,
    pub render_advance_frames: u32,
    pub played_back_sample: Option<i32>,
    pub logical_played_position: Option<i32>,
    pub raw_played_position: Option<i32>,
    pub dispatch_position: Option<i32>,
    pub n_preplay_samples: u32,
    pub latency_retention_incomplete: bool,
    pub latency_history_variable: bool,
    pub latency_history_revisions: u32,
    pub data_dirty: bool,
    pub current_latency_recipe: PublishedLatencyRecipe,
    pub latched_latency_recipe: PublishedLatencyRecipe,
}

/// Audio-port state exposed through the application-facing backend interface.
///
/// `muted` and `passthrough_muted` are booleans here, where the C struct used `u32`
/// because it had to cross a C boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPortState {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain: f32,
    pub muted: bool,
    pub passthrough_muted: bool,
    /// Samples currently retained for retroactive recording, not the window that was
    /// requested.
    pub ringbuffer_n_samples: u32,
    pub capture_latency: RuntimeLatencyObservation,
    pub playback_latency: RuntimeLatencyObservation,
    pub name: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LatestMidiMessage {
    pub bytes: [u8; 4],
    pub len: u8,
}

impl LatestMidiMessage {
    pub fn new(data: &[u8]) -> Option<Self> {
        if data.is_empty() || data.len() > 4 {
            return None;
        }
        let mut bytes = [0; 4];
        bytes[..data.len()].copy_from_slice(data);
        Some(Self {
            bytes,
            len: data.len() as u8,
        })
    }

    pub fn data(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// MIDI-port state exposed through the application-facing backend interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiPortState {
    pub n_input_events: u32,
    pub n_input_notes_active: u32,
    pub n_output_events: u32,
    pub n_output_notes_active: u32,
    pub muted: bool,
    pub passthrough_muted: bool,
    pub ringbuffer_n_samples: u32,
    pub capture_latency: RuntimeLatencyObservation,
    pub playback_latency: RuntimeLatencyObservation,
    pub latest_input_message: Option<LatestMidiMessage>,
    pub name: String,
}
