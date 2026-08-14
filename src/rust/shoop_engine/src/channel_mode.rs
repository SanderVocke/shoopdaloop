//! Mapping from loop mode + channel mode to what a channel actually does.
//!
//! A loop's "advanced" modes (dry-through-wet, recording-dry-into-wet) collapse
//! to a simple playback/record/replace subset that differs per channel mode.
//! Pre-play and pre-record are folded in here too: when a transition is one
//! trigger away, a stopped channel may already need to play or record.

use crate::loop_mode::LoopMode;
use enum_iterator::Sequence;
use num_enum::{IntoPrimitive, TryFromPrimitive};

/// Role of a channel within its track.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, TryFromPrimitive, IntoPrimitive, Sequence)]
#[repr(i32)]
pub enum ChannelMode {
    Disabled = 0,
    #[default]
    Direct = 1,
    Dry = 2,
    Wet = 3,
}

/// What a channel should do this cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessFlags(pub u32);

impl ProcessFlags {
    pub const NONE: Self = Self(0);
    pub const PLAYBACK: Self = Self(0b0001);
    pub const RECORD: Self = Self(0b0010);
    pub const PRE_RECORD: Self = Self(0b0100);
    pub const REPLACE: Self = Self(0b1000);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Channel action plus the sample offset it applies to.
///
/// `position` is signed: pre-play reaches back before the start offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelProcessParams {
    pub flags: ProcessFlags,
    pub position: i32,
}

/// Collapses a loop mode to channel flags for the given channel mode.
pub fn loop_mode_to_channel_process_flags(
    loop_mode: LoopMode,
    channel_mode: ChannelMode,
) -> ProcessFlags {
    use ChannelMode as C;
    use LoopMode as L;

    // Channel-mode-specific overrides come first.
    match (channel_mode, loop_mode) {
        (C::Disabled, _) => return ProcessFlags::NONE,
        (C::Dry, L::Playing) => return ProcessFlags::NONE,
        (C::Dry, L::PlayingDryThroughWet | L::RecordingDryIntoWet) => {
            return ProcessFlags::PLAYBACK
        }
        (C::Wet, L::PlayingDryThroughWet) => return ProcessFlags::NONE,
        (C::Wet, L::RecordingDryIntoWet) => return ProcessFlags::REPLACE,
        (C::Direct, L::PlayingDryThroughWet) => return ProcessFlags::PLAYBACK,
        (C::Direct, L::RecordingDryIntoWet) => return ProcessFlags::REPLACE,
        _ => {}
    }

    match loop_mode {
        L::Playing => ProcessFlags::PLAYBACK,
        L::Recording => ProcessFlags::RECORD,
        L::Replacing => ProcessFlags::REPLACE,
        L::Stopped | L::Unknown | L::PlayingDryThroughWet | L::RecordingDryIntoWet => {
            ProcessFlags::NONE
        }
    }
}

/// Full per-cycle channel decision, including pre-play and pre-record.
///
/// `next_mode` is `Unknown` when nothing is planned, which yields no flags and
/// so disables both pre-roll branches.
#[allow(clippy::too_many_arguments)]
pub fn channel_process_params(
    loop_mode: LoopMode,
    next_mode: LoopMode,
    next_mode_delay_cycles: Option<u32>,
    next_mode_eta: Option<u32>,
    position: i32,
    start_offset: i32,
    channel_mode: ChannelMode,
) -> ChannelProcessParams {
    let flags = loop_mode_to_channel_process_flags(loop_mode, channel_mode);
    let mut out = ChannelProcessParams {
        flags,
        position: position + start_offset,
    };

    // Pre-roll only applies on the cycle immediately before the transition.
    let transition_imminent = next_mode_eta.is_some() && next_mode_delay_cycles.unwrap_or(999) == 0;
    if !transition_imminent {
        return out;
    }
    let next_flags = loop_mode_to_channel_process_flags(next_mode, channel_mode);

    if flags.is_empty() && next_flags.contains(ProcessFlags::PLAYBACK) {
        // Stopped but about to play: reach back so the lead-in is audible.
        out.position = start_offset - next_mode_eta.unwrap_or(0) as i32;
        out.flags = out.flags.with(ProcessFlags::PLAYBACK);
    } else if !flags.contains(ProcessFlags::RECORD) && next_flags.contains(ProcessFlags::RECORD) {
        out.flags = out.flags.with(ProcessFlags::PRE_RECORD);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    use ChannelMode as C;
    use LoopMode as L;

    fn flags(loop_mode: LoopMode, channel_mode: ChannelMode) -> ProcessFlags {
        loop_mode_to_channel_process_flags(loop_mode, channel_mode)
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn discriminants_match_c_abi() {
        check!(C::Disabled as u32 == 0);
        check!(C::Direct as u32 == 1);
        check!(C::Dry as u32 == 2);
        check!(C::Wet as u32 == 3);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn disabled_never_does_anything() {
        for m in [
            L::Stopped,
            L::Playing,
            L::Recording,
            L::Replacing,
            L::PlayingDryThroughWet,
            L::RecordingDryIntoWet,
        ] {
            check!(flags(m, C::Disabled) == ProcessFlags::NONE);
        }
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn direct_channel_simple_modes() {
        check!(flags(L::Stopped, C::Direct) == ProcessFlags::NONE);
        check!(flags(L::Playing, C::Direct) == ProcessFlags::PLAYBACK);
        check!(flags(L::Recording, C::Direct) == ProcessFlags::RECORD);
        check!(flags(L::Replacing, C::Direct) == ProcessFlags::REPLACE);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn dry_wet_split_on_advanced_modes() {
        // Dry-through-wet: dry plays out, wet passes the live signal (nothing).
        check!(flags(L::PlayingDryThroughWet, C::Dry) == ProcessFlags::PLAYBACK);
        check!(flags(L::PlayingDryThroughWet, C::Wet) == ProcessFlags::NONE);
        // Recording dry into wet: dry plays out, wet overwrites itself.
        check!(flags(L::RecordingDryIntoWet, C::Dry) == ProcessFlags::PLAYBACK);
        check!(flags(L::RecordingDryIntoWet, C::Wet) == ProcessFlags::REPLACE);
        // Plain playback uses the wet side only, so dry stays silent.
        check!(flags(L::Playing, C::Dry) == ProcessFlags::NONE);
        check!(flags(L::Playing, C::Wet) == ProcessFlags::PLAYBACK);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn direct_follows_advanced_modes_too() {
        check!(flags(L::PlayingDryThroughWet, C::Direct) == ProcessFlags::PLAYBACK);
        check!(flags(L::RecordingDryIntoWet, C::Direct) == ProcessFlags::REPLACE);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn position_offsets_by_start_offset() {
        let p = channel_process_params(L::Playing, L::Unknown, None, None, 10, 4, C::Direct);
        check!(p.flags == ProcessFlags::PLAYBACK);
        check!(p.position == 14);

        // Negative start offsets are allowed.
        let p = channel_process_params(L::Playing, L::Unknown, None, None, 10, -4, C::Direct);
        check!(p.position == 6);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn pre_play_reaches_back_before_start_offset() {
        // Stopped, but Playing is one trigger away with 3 samples to go.
        let p = channel_process_params(L::Stopped, L::Playing, Some(0), Some(3), 0, 5, C::Direct);
        check!(p.flags == ProcessFlags::PLAYBACK);
        check!(p.position == 2); // start_offset - eta
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn pre_record_sets_flag_without_moving_position() {
        let p = channel_process_params(L::Stopped, L::Recording, Some(0), Some(3), 7, 5, C::Direct);
        check!(p.flags.contains(ProcessFlags::PRE_RECORD));
        check!(p.position == 12); // unchanged: position + start_offset
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn pre_roll_requires_imminent_transition() {
        // Delay of 1 cycle: not yet.
        let p = channel_process_params(L::Stopped, L::Playing, Some(1), Some(3), 0, 5, C::Direct);
        check!(p.flags == ProcessFlags::NONE);
        check!(p.position == 5);

        // No ETA known: not yet.
        let p = channel_process_params(L::Stopped, L::Playing, Some(0), None, 0, 5, C::Direct);
        check!(p.flags == ProcessFlags::NONE);

        // No delay value at all maps to 999, i.e. never imminent.
        let p = channel_process_params(L::Stopped, L::Playing, None, Some(3), 0, 5, C::Direct);
        check!(p.flags == ProcessFlags::NONE);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn already_recording_does_not_pre_record() {
        let p = channel_process_params(
            L::Recording,
            L::Recording,
            Some(0),
            Some(3),
            0,
            0,
            C::Direct,
        );
        check!(p.flags == ProcessFlags::RECORD);
        check!(!p.flags.contains(ProcessFlags::PRE_RECORD));
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn playing_channel_does_not_pre_play_again() {
        // Already playing, so the pre-play branch is skipped; a planned record
        // still arms pre-record.
        let p = channel_process_params(L::Playing, L::Recording, Some(0), Some(3), 2, 0, C::Direct);
        check!(p.flags.contains(ProcessFlags::PLAYBACK));
        check!(p.flags.contains(ProcessFlags::PRE_RECORD));
        check!(p.position == 2);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn unknown_next_mode_disables_pre_roll() {
        let p = channel_process_params(L::Stopped, L::Unknown, Some(0), Some(3), 0, 5, C::Direct);
        check!(p.flags == ProcessFlags::NONE);
        check!(p.position == 5);
    }

    #[tracy_nextest_capture::tracy_capture_test]
    fn disabled_channel_ignores_pre_roll() {
        let p = channel_process_params(L::Stopped, L::Playing, Some(0), Some(3), 0, 5, C::Disabled);
        check!(p.flags == ProcessFlags::NONE);
    }
}
