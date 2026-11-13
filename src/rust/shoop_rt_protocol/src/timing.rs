//! Fixed-size timing edits and confirmed playback windows.

use crate::MAX_STRIP_CHANNELS;
pub const MAX_TIMING_UPDATES: usize = MAX_STRIP_CHANNELS * 2 + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelWindow {
    pub gain_bits: u32,
    pub start_offset: i32,
    pub capture_alignment: i32,
    pub preplay: u32,
    pub raw_frames: Option<u64>,
}
impl Default for ChannelWindow {
    fn default() -> Self {
        Self {
            gain_bits: 1f32.to_bits(),
            start_offset: 0,
            capture_alignment: 0,
            preplay: 0,
            raw_frames: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlaybackWindows {
    pub audio: [ChannelWindow; MAX_STRIP_CHANNELS * 2],
    pub midi: ChannelWindow,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelTimingUpdate {
    pub channel: u8,
    pub midi: bool,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimingEdit {
    pub length: Option<u64>,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
    pub count: u8,
    pub channels: [ChannelTimingUpdate; MAX_TIMING_UPDATES],
}
impl TimingEdit {
    pub fn canonical(self) -> bool {
        let count = usize::from(self.count);
        count <= MAX_TIMING_UPDATES
            && self.channels[count..]
                .iter()
                .all(|update| *update == ChannelTimingUpdate::default())
            && (count == 0
                || (self.length.is_none() && self.start_offset.is_none() && self.preplay.is_none()))
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimingState {
    pub revision: u64,
    pub content_revision: u64,
    pub length: u64,
    pub raw_audio_frames: u64,
    pub raw_midi_frames: u64,
    pub windows: PlaybackWindows,
}
