//! Playback windows share immutable raw media. Coordinates never move capture time.

use crate::Error;

pub const MAX_AUDIO_CHANNELS: usize = 16;
pub const MAX_CHANNEL_UPDATES: usize = MAX_AUDIO_CHANNELS + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelWindow {
    pub gain_bits: u32,
    pub raw_frames: Option<u64>,
    pub start_offset: i32,
    pub capture_alignment: i32,
    pub preplay: u32,
}
impl ChannelWindow {
    pub fn origin(self) -> i64 {
        i64::from(self.start_offset) + i64::from(self.capture_alignment)
    }
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
pub struct Windows {
    pub audio: [ChannelWindow; MAX_AUDIO_CHANNELS],
    pub midi: ChannelWindow,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelUpdate {
    pub channel: u8,
    pub midi: bool,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Edit {
    pub length: Option<u64>,
    pub start_offset: Option<i32>,
    pub preplay: Option<u32>,
    pub count: u8,
    pub channels: [ChannelUpdate; MAX_CHANNEL_UPDATES],
}
impl Windows {
    pub fn edited(self, edit: Edit, audio: usize, midi: bool) -> Result<Self, Error> {
        if audio > MAX_AUDIO_CHANNELS || usize::from(edit.count) > MAX_CHANNEL_UPDATES {
            return Err(Error::Capacity);
        }
        if edit.channels[usize::from(edit.count)..]
            .iter()
            .any(|item| *item != ChannelUpdate::default())
        {
            return Err(Error::InvalidConfig);
        }
        if edit.count > 0
            && (edit.start_offset.is_some() || edit.preplay.is_some() || edit.length.is_some())
        {
            return Err(Error::InvalidConfig);
        }
        let mut result = self;
        for window in result
            .audio
            .iter_mut()
            .take(audio)
            .chain(midi.then_some(&mut result.midi))
        {
            if let Some(offset) = edit.start_offset {
                window.start_offset = offset;
            }
            if let Some(preplay) = edit.preplay {
                window.preplay = preplay;
            }
        }
        for (index, update) in edit.channels[..usize::from(edit.count)].iter().enumerate() {
            if edit.channels[..index]
                .iter()
                .any(|old| old.midi == update.midi && old.channel == update.channel)
            {
                return Err(Error::DuplicateId);
            }
            let window = if update.midi {
                if !midi || update.channel != 0 {
                    return Err(Error::InvalidId);
                }
                &mut result.midi
            } else {
                if usize::from(update.channel) >= audio {
                    return Err(Error::InvalidId);
                }
                &mut result.audio[usize::from(update.channel)]
            };
            if let Some(offset) = update.start_offset {
                window.start_offset = offset;
            }
            if let Some(preplay) = update.preplay {
                window.preplay = preplay;
            }
        }
        Ok(result)
    }
    pub fn validate_alignment(
        self,
        audio: usize,
        midi: bool,
        raw_audio: u64,
        raw_midi: u64,
        length: u64,
    ) -> Result<(), Error> {
        for (window, frames) in self
            .audio
            .iter()
            .take(audio)
            .map(|w| (*w, raw_audio))
            .chain(midi.then_some((self.midi, raw_midi)))
        {
            if !f32::from_bits(window.gain_bits).is_finite() {
                return Err(Error::InvalidConfig);
            }
            let frames = match window.raw_frames {
                Some(raw) if raw > frames => return Err(Error::InvalidFrame),
                Some(raw) => raw,
                None => frames,
            };
            if window.capture_alignment != 0 {
                let start = window.origin();
                if start < 0
                    || (start as u64)
                        .checked_add(length)
                        .is_none_or(|end| end > frames)
                {
                    return Err(Error::InvalidFrame);
                }
            }
        }
        Ok(())
    }
}

/// Audible half-open span inside a whole graph tick. Negative positions and raw
/// tails are silence; lead-in is limited independently for each channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Span {
    pub offset: usize,
    pub position: u64,
    pub frames: usize,
}
pub fn span(
    position: i64,
    window: ChannelWindow,
    lead: bool,
    frames: usize,
    raw_frames: u64,
) -> Span {
    let raw_frames = window.raw_frames.unwrap_or(raw_frames).min(raw_frames);
    let position = i128::from(position);
    let raw = position + i128::from(window.origin());
    let begin = (-raw)
        .max(if lead {
            -i128::from(window.preplay) - position
        } else {
            0
        })
        .clamp(0, frames as i128);
    let end = (i128::from(raw_frames) - raw).clamp(begin, frames as i128);
    Span {
        offset: begin as usize,
        position: (raw + begin).max(0).min(i128::from(u64::MAX)) as u64,
        frames: (end - begin) as usize,
    }
}
