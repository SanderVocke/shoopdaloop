use super::MediaTiming;
use crate::{ExactMidi, SessionError};

pub struct AudioConversion<'a>(shoop_format_conversion::AudioConversion<'a>);
impl<'a> AudioConversion<'a> {
    pub fn new(
        input: &'a [f32],
        timing: MediaTiming,
        periodic: bool,
        max_frames: u64,
    ) -> Result<Self, SessionError> {
        shoop_format_conversion::AudioConversion::new(input, timing, periodic, max_frames)
            .map(Self)
            .map_err(Into::into)
    }
    pub fn step(&mut self, frames: usize) -> Result<bool, SessionError> {
        self.0.step(frames).map_err(Into::into)
    }
    pub fn finish(self) -> Result<Vec<f32>, SessionError> {
        self.0.finish().map_err(Into::into)
    }
}
pub fn convert_audio(
    input: &[f32],
    timing: MediaTiming,
    periodic: bool,
    max_frames: u64,
) -> Result<Vec<f32>, SessionError> {
    shoop_format_conversion::convert_audio(input, timing, periodic, max_frames).map_err(Into::into)
}
pub fn convert_midi(
    input: &ExactMidi,
    timing: MediaTiming,
    sample_rate: u32,
) -> Result<ExactMidi, SessionError> {
    shoop_format_conversion::convert_midi(input, timing, sample_rate).map_err(Into::into)
}
pub fn convert_midi_with_cancel(
    input: &ExactMidi,
    timing: MediaTiming,
    sample_rate: u32,
    cancelled: impl FnMut() -> bool,
) -> Result<ExactMidi, SessionError> {
    shoop_format_conversion::convert_midi_with_cancel(input, timing, sample_rate, cancelled)
        .map_err(Into::into)
}
