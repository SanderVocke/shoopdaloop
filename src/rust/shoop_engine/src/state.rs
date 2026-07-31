//! The state shapes the application-facing backend interface reports.
//!
//! Here rather than beside the handles that assemble them, because two layers need them and
//! neither should have to depend on the other: the handle API builds them from a session it
//! has borrowed, and [`crate::engine`] publishes them from the audio thread for the control
//! side to poll. Keeping them in `control.rs` made the publishing side depend on the handle
//! layer, which is backwards -- the handles are what will be rebuilt on top of publishing.
//!
//! # Names are not in here
//!
//! A port's name is a `String`, and the audio thread can neither clone one nor read one it
//! does not own. So the polled shapes ([`AudioPortSnapshot`], [`MidiPortSnapshot`]) carry
//! only the per-cycle numbers, and the public shapes ([`AudioPortState`],
//! [`MidiPortState`]) are assembled on the control side by adding the name -- which whoever
//! holds a port handle already knows, having supplied it when the port was created.
//!
//! The channel shapes need no such split: nothing in them is owned data, so the audio
//! thread can fill them directly and the polled and public shapes are the same type.

use crate::channel_mode::ChannelMode;
use crate::loop_mode::LoopMode;

#[derive(Clone, Debug, PartialEq)]
pub struct LoopState {
    pub mode: LoopMode,
    pub length: u32,
    pub position: u32,
    pub maybe_next_mode: Option<LoopMode>,
    pub maybe_next_mode_delay: Option<u32>,
}

impl Default for LoopState {
    fn default() -> Self {
        Self {
            mode: LoopMode::Unknown,
            length: 0,
            position: 0,
            maybe_next_mode: None,
            maybe_next_mode_delay: None,
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
    pub played_back_sample: Option<i32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}

/// MIDI-channel state exposed through the application-facing backend interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiChannelState {
    pub mode: ChannelMode,
    pub n_events_triggered: u32,
    pub n_notes_active: u32,
    pub length: u32,
    pub start_offset: i32,
    pub played_back_sample: Option<i32>,
    pub n_preplay_samples: u32,
    pub data_dirty: bool,
}

/// An audio port's per-cycle numbers, without its name.
///
/// What the audio thread can publish: no owned data, so filling one allocates nothing. Pair
/// it with a name to get an [`AudioPortState`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioPortSnapshot {
    pub input_peak: f32,
    pub output_peak: f32,
    pub gain: f32,
    pub muted: bool,
    pub passthrough_muted: bool,
    /// Samples currently retained for retroactive recording, not the window that was
    /// requested. This is the figure a caller checking whether there is anything to adopt is
    /// reading, so it is worth saying.
    pub ringbuffer_n_samples: u32,
}

/// A MIDI port's per-cycle numbers, without its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidiPortSnapshot {
    pub n_input_events: u32,
    pub n_input_notes_active: u32,
    pub n_output_events: u32,
    pub n_output_notes_active: u32,
    pub muted: bool,
    pub passthrough_muted: bool,
    pub ringbuffer_n_samples: u32,
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
    pub name: String,
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
    pub name: String,
}

impl AudioPortSnapshot {
    /// Completes this into the public shape by supplying the name.
    pub fn named(self, name: impl Into<String>) -> AudioPortState {
        AudioPortState {
            input_peak: self.input_peak,
            output_peak: self.output_peak,
            gain: self.gain,
            muted: self.muted,
            passthrough_muted: self.passthrough_muted,
            ringbuffer_n_samples: self.ringbuffer_n_samples,
            name: name.into(),
        }
    }
}

impl MidiPortSnapshot {
    /// Completes this into the public shape by supplying the name.
    pub fn named(self, name: impl Into<String>) -> MidiPortState {
        MidiPortState {
            n_input_events: self.n_input_events,
            n_input_notes_active: self.n_input_notes_active,
            n_output_events: self.n_output_events,
            n_output_notes_active: self.n_output_notes_active,
            muted: self.muted,
            passthrough_muted: self.passthrough_muted,
            ringbuffer_n_samples: self.ringbuffer_n_samples,
            name: name.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    /// The split exists so the audio thread never touches a `String`; adding the name back
    /// must not change any of the numbers.
    #[test]
    fn naming_an_audio_snapshot_preserves_every_metric() {
        let snap = AudioPortSnapshot {
            input_peak: 0.25,
            output_peak: 0.5,
            gain: 0.75,
            muted: true,
            passthrough_muted: false,
            ringbuffer_n_samples: 128,
        };

        let named = snap.named("out");

        check!(named.name == "out");
        check!(named.input_peak == snap.input_peak);
        check!(named.output_peak == snap.output_peak);
        check!(named.gain == snap.gain);
        check!(named.muted == snap.muted);
        check!(named.passthrough_muted == snap.passthrough_muted);
        check!(named.ringbuffer_n_samples == snap.ringbuffer_n_samples);
    }

    #[test]
    fn naming_a_midi_snapshot_preserves_every_metric() {
        let snap = MidiPortSnapshot {
            n_input_events: 3,
            n_input_notes_active: 2,
            n_output_events: 5,
            n_output_notes_active: 1,
            muted: false,
            passthrough_muted: true,
            ringbuffer_n_samples: 64,
        };

        let named = snap.named("min");

        check!(named.name == "min");
        check!(named.n_input_events == snap.n_input_events);
        check!(named.n_input_notes_active == snap.n_input_notes_active);
        check!(named.n_output_events == snap.n_output_events);
        check!(named.n_output_notes_active == snap.n_output_notes_active);
        check!(named.muted == snap.muted);
        check!(named.passthrough_muted == snap.passthrough_muted);
        check!(named.ringbuffer_n_samples == snap.ringbuffer_n_samples);
    }
}
