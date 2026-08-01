//! Audio channel: records from, replaces into and plays back out of chunked
//! sample storage under its parent loop's control.
//!
//! Copies are deferred. `process` decides what to move and queues it; `finalize`
//! performs the moves. That ordering lets every node in a graph step settle its
//! state before any buffer contents are touched.
//!
//! are resolved against the cycle's port buffers in `finalize`, which keeps the
//! crate free of unsafe code.

use crate::channel_mode::{channel_process_params, ChannelMode, ProcessFlags};
use crate::chunked_samples::ChunkedSamples;
use crate::loop_mode::LoopMode;
use crate::state_mirror::AudioChannelStateMirror;

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ChannelError {
    #[error("recording {n_samples} samples exceeds the {available} available in the input buffer")]
    RecordOutOfBounds { n_samples: usize, available: usize },
    #[error("replacing {n_samples} samples exceeds the {available} available in the input buffer")]
    ReplaceInputOutOfBounds { n_samples: usize, available: usize },
    #[error("replace reached position {position} at or beyond recorded length {length}")]
    ReplaceOutOfBounds { position: usize, length: usize },
    #[error("playing {n_samples} samples exceeds the {available} available in the output buffer")]
    PlaybackOutOfBounds { n_samples: usize, available: usize },
}

/// A copy queued during `process`, applied in `finalize`.
///
/// Offsets into the chunked store are always chunk-local by construction: the
/// producing loops split at chunk boundaries.
#[derive(Debug, Clone, Copy, PartialEq)]
enum CopyCmd {
    /// Port input buffer -> main storage.
    IntoMain { dst: usize, src: usize, len: usize },
    /// Port input buffer -> pre-record storage.
    IntoPreRecord { dst: usize, src: usize, len: usize },
    /// Main storage -> port output buffer, added on top of what is there.
    OutOfMain {
        src: usize,
        dst: usize,
        len: usize,
        gain: f32,
    },
}

/// Tracks how much of a port buffer this cycle has been consumed.
#[derive(Debug, Clone, Copy, Default)]
struct CycleBuf {
    cursor: usize,
    remaining: usize,
}

#[derive(Debug)]
pub struct AudioChannel {
    buffers: ChunkedSamples<f32>,
    data_length: usize,
    prerecord_buffers: ChunkedSamples<f32>,
    prerecord_data_length: usize,

    start_offset: i32,
    pre_play_samples: u32,
    output_peak: f32,
    gain: f32,
    mode: ChannelMode,
    data_seq_nr: u32,
    last_played_back_sample: Option<i32>,
    prev_process_flags: ProcessFlags,

    playback: Option<CycleBuf>,
    recording: Option<CycleBuf>,
    queue: Vec<CopyCmd>,
    state: Arc<AudioChannelStateMirror>,
}

impl AudioChannel {
    pub fn with_chunk_size(chunk_size: usize, mode: ChannelMode) -> Self {
        Self::with_chunk_size_and_state(
            chunk_size,
            mode,
            Arc::new(AudioChannelStateMirror::default()),
        )
    }

    pub fn with_chunk_size_and_state(
        chunk_size: usize,
        mode: ChannelMode,
        state: Arc<AudioChannelStateMirror>,
    ) -> Self {
        let channel = Self {
            buffers: ChunkedSamples::with_chunk_size(chunk_size),
            data_length: 0,
            prerecord_buffers: ChunkedSamples::with_chunk_size(chunk_size),
            prerecord_data_length: 0,
            start_offset: 0,
            pre_play_samples: 0,
            output_peak: 0.0,
            gain: 1.0,
            mode,
            data_seq_nr: 0,
            last_played_back_sample: None,
            prev_process_flags: ProcessFlags::NONE,
            playback: None,
            recording: None,
            queue: Vec::new(),
            state,
        };
        channel.publish_state();
        channel
    }

    fn publish_state(&self) {
        self.state.publish(
            self.mode,
            self.gain,
            self.data_length,
            self.start_offset,
            self.last_played_back_sample,
            self.pre_play_samples,
            self.data_seq_nr as u64,
        );
    }

    fn publish_all_data(&self) {
        if self.state.complex_data_enabled() {
            self.state
                .replace_data(self.buffers.contiguous_copy(self.data_length));
        }
    }

    // --- accessors ---

    pub fn length(&self) -> usize {
        self.data_length
    }
    pub fn set_length(&mut self, length: usize) {
        self.data_length = length;
        self.publish_all_data();
        self.data_changed();
    }
    pub fn mode(&self) -> ChannelMode {
        self.mode
    }
    pub fn set_mode(&mut self, mode: ChannelMode) {
        self.mode = mode;
        self.publish_state();
    }
    pub fn gain(&self) -> f32 {
        self.gain
    }
    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
        self.publish_state();
    }
    pub fn start_offset(&self) -> i32 {
        self.start_offset
    }
    pub fn set_start_offset(&mut self, offset: i32) {
        self.start_offset = offset;
        self.publish_state();
    }
    pub fn pre_play_samples(&self) -> u32 {
        self.pre_play_samples
    }
    pub fn set_pre_play_samples(&mut self, samples: u32) {
        self.pre_play_samples = samples;
        self.publish_state();
    }
    pub fn output_peak(&self) -> f32 {
        self.output_peak
    }
    pub fn reset_output_peak(&mut self) {
        self.output_peak = 0.0;
    }
    pub fn data_seq_nr(&self) -> u32 {
        self.data_seq_nr
    }
    /// `None` when nothing was played back last cycle.
    pub fn played_back_sample(&self) -> Option<i32> {
        self.last_played_back_sample
    }
    pub fn at(&self, position: usize) -> Option<f32> {
        self.buffers.get(position).copied()
    }

    fn data_changed(&mut self) {
        self.data_seq_nr = self.data_seq_nr.wrapping_add(1);
        self.publish_state();
    }

    /// Recorded content, up to the recorded length.
    pub fn data(&self) -> Vec<f32> {
        self.buffers.contiguous_copy(self.data_length)
    }

    pub fn load_data(&mut self, samples: &[f32]) {
        self.buffers.set_contents(samples);
        self.data_length = samples.len();
        self.start_offset = 0;
        if self.state.complex_data_enabled() {
            self.state.replace_data(samples.to_vec());
        }
        self.data_changed();
    }

    ///
    /// Does *not* zero the samples, which is only safe because the caller sets the length it means:
    /// `clear(0)` leaves the old audio unreachable. For a length that keeps them reachable, use
    /// [`Self::silence`].
    pub fn clear(&mut self, length: usize) {
        self.buffers.ensure_available(length);
        self.data_length = length;
        self.start_offset = 0;
        self.publish_all_data();
        self.data_changed();
    }

    /// Replaces `length` samples with silence.
    ///
    /// Distinct from [`Self::clear`] because clearing to a non-zero length leaves the previous
    /// recording in the chunks, where it stays both audible and visible in the waveform.
    pub fn silence(&mut self, length: usize) {
        self.buffers.fill(length, 0.0);
        self.data_length = length;
        self.start_offset = 0;
        if self.state.complex_data_enabled() {
            self.state.replace_data(vec![0.0; length]);
        }
        self.data_changed();
    }

    // --- per-cycle port buffers ---

    pub fn set_playback_buffer_size(&mut self, size: usize) {
        self.playback = Some(CycleBuf {
            cursor: 0,
            remaining: size,
        });
    }
    pub fn set_recording_buffer_size(&mut self, size: usize) {
        self.recording = Some(CycleBuf {
            cursor: 0,
            remaining: size,
        });
    }
    pub fn clear_buffers(&mut self) {
        self.playback = None;
        self.recording = None;
    }

    /// First point until which this channel can be processed freely: whichever
    /// port buffer runs out first.
    ///
    /// replacing channel can still be asked for more samples than its input
    /// buffer holds; `process` reports that as an error rather than overrunning.
    pub fn next_poi(
        &self,
        mode: LoopMode,
        next_mode: LoopMode,
        next_mode_delay_cycles: Option<u32>,
        next_mode_eta: Option<u32>,
        position: i32,
    ) -> Option<usize> {
        if self.mode == ChannelMode::Disabled {
            return None;
        }
        let params = channel_process_params(
            mode,
            next_mode,
            next_mode_delay_cycles,
            next_mode_eta,
            position,
            self.start_offset,
            self.mode,
        );
        let mut poi: Option<usize> = None;
        let mut merge = |v: usize| poi = Some(poi.map_or(v, |p: usize| p.min(v)));

        // An unassigned buffer contributes zero rather than nothing, matching the
        // merged without a null check.
        if params.flags.contains(ProcessFlags::PLAYBACK) {
            merge(self.playback.map_or(0, |b| b.remaining));
        }
        if params
            .flags
            .contains(ProcessFlags::RECORD.with(ProcessFlags::PRE_RECORD))
        {
            merge(self.recording.map_or(0, |b| b.remaining));
        }
        poi
    }

    // --- processing ---

    /// Decides and queues this cycle's copies.
    ///
    /// implementation ignored them.
    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        mode: LoopMode,
        next_mode: LoopMode,
        next_mode_delay_cycles: Option<u32>,
        next_mode_eta: Option<u32>,
        n_samples: usize,
        pos_before: i32,
        length_before: usize,
    ) -> Result<(), ChannelError> {
        let params = channel_process_params(
            mode,
            next_mode,
            next_mode_delay_cycles,
            next_mode_eta,
            pos_before,
            self.start_offset,
            self.mode,
        );
        let mut flags = params.flags;

        // A freshly created channel can be asked to pre-record before it has
        // been given port buffers; without them there is nothing to do.
        if self.recording.is_none() {
            flags = ProcessFlags(
                flags.0
                    & !(ProcessFlags::PRE_RECORD.0
                        | ProcessFlags::RECORD.0
                        | ProcessFlags::REPLACE.0),
            );
        }
        if self.playback.is_none() {
            flags = ProcessFlags(flags.0 & !ProcessFlags::PLAYBACK.0);
        }

        if !flags.contains(ProcessFlags::PRE_RECORD)
            && self.prev_process_flags.contains(ProcessFlags::PRE_RECORD)
        {
            if flags.contains(ProcessFlags::RECORD) {
                // Transitioning pre-record -> record: adopt what was buffered,
                // and offset playback so the lead-in sits before sample 0.
                self.buffers = self.prerecord_buffers.clone();
                self.data_length = self.prerecord_data_length;
                self.start_offset = self.prerecord_data_length as i32;
                self.publish_all_data();
            }
            self.prerecord_buffers.reset();
            self.prerecord_data_length = 0;
        }

        if flags.contains(ProcessFlags::PLAYBACK) {
            self.last_played_back_sample = Some(params.position);
            self.process_playback(params.position, n_samples)?;
        } else {
            self.last_played_back_sample = None;
        }
        if flags.contains(ProcessFlags::RECORD) {
            let from = (length_before as i64 + self.start_offset as i64).max(0) as usize;
            self.process_record(n_samples, from, false)?;
        }
        if flags.contains(ProcessFlags::REPLACE) {
            self.process_replace(params.position, n_samples)?;
        }
        if flags.contains(ProcessFlags::PRE_RECORD) {
            let from = self.prerecord_data_length;
            self.process_record(n_samples, from, true)?;
        }

        self.prev_process_flags = flags;

        if let Some(b) = self.recording.as_mut() {
            b.cursor += n_samples;
            b.remaining = b.remaining.saturating_sub(n_samples);
        }
        if let Some(b) = self.playback.as_mut() {
            b.cursor += n_samples;
            b.remaining = b.remaining.saturating_sub(n_samples);
        }
        self.publish_state();
        Ok(())
    }

    fn process_record(
        &mut self,
        n_samples: usize,
        record_from: usize,
        into_prerecord: bool,
    ) -> Result<(), ChannelError> {
        let buf = self.recording.unwrap_or_default();
        if buf.remaining < n_samples {
            return Err(ChannelError::RecordOutOfBounds {
                n_samples,
                available: buf.remaining,
            });
        }

        let mut at = record_from;
        let mut src = buf.cursor;
        let mut left = n_samples;
        let mut chunks_touched = 0u32;

        while left > 0 {
            let n = {
                let buffers = if into_prerecord {
                    &mut self.prerecord_buffers
                } else {
                    &mut self.buffers
                };
                buffers.ensure_available(at + left);
                left.min(buffers.space_for_sample(at))
            };
            self.queue.push(if into_prerecord {
                CopyCmd::IntoPreRecord {
                    dst: at,
                    src,
                    len: n,
                }
            } else {
                CopyCmd::IntoMain {
                    dst: at,
                    src,
                    len: n,
                }
            });
            if into_prerecord {
                self.prerecord_data_length = at + n;
            } else {
                self.data_length = at + n;
            }
            at += n;
            src += n;
            left -= n;
            chunks_touched += 1;
        }

        for _ in 0..chunks_touched {
            self.data_changed();
        }
        Ok(())
    }

    fn process_replace(
        &mut self,
        data_position: i32,
        n_samples: usize,
    ) -> Result<(), ChannelError> {
        let buf = self.recording.unwrap_or_default();
        if buf.remaining < n_samples {
            return Err(ChannelError::ReplaceInputOutOfBounds {
                n_samples,
                available: buf.remaining,
            });
        }

        let mut src = buf.cursor;
        let mut left = n_samples;
        let mut pos = data_position;

        // Anything before sample 0 is not ours to write; skip past it.
        if pos < 0 {
            let skip = (-pos) as usize;
            src += skip.min(buf.remaining);
            left = left.saturating_sub(skip);
            pos = 0;
        }
        let mut pos = pos as usize;
        let mut chunks_touched = 0u32;

        while left > 0 {
            if self.buffers.ensure_available(pos + left) {
                chunks_touched += 1;
            }
            // length this yields 0 and surfaces as an error below, rather than
            // wrapping and writing outside the recorded region.
            let samples_left = self.data_length.saturating_sub(pos);
            let n = left
                .min(samples_left)
                .min(self.buffers.space_for_sample(pos));
            if n == 0 {
                return Err(ChannelError::ReplaceOutOfBounds {
                    position: pos,
                    length: self.data_length,
                });
            }
            self.queue.push(CopyCmd::IntoMain {
                dst: pos,
                src,
                len: n,
            });
            pos += n;
            src += n;
            left -= n;
            chunks_touched += 1;
        }

        for _ in 0..chunks_touched {
            self.data_changed();
        }
        Ok(())
    }

    fn process_playback(
        &mut self,
        data_position: i32,
        n_samples: usize,
    ) -> Result<(), ChannelError> {
        let buf = self.playback.unwrap_or_default();
        if buf.remaining < n_samples {
            return Err(ChannelError::PlaybackOutOfBounds {
                n_samples,
                available: buf.remaining,
            });
        }

        let mut pos = data_position;
        let mut left = n_samples;
        let mut dst = buf.cursor;

        // Playback may not start before the pre-play window opens.
        let starting = (self.start_offset - self.pre_play_samples as i32).max(0);
        let skip = (starting - pos).max(0);
        if skip > 0 {
            let skip = skip as usize;
            pos += skip as i32;
            left = left.saturating_sub(skip);
            dst += skip.min(buf.remaining);
        }

        while left > 0 && (pos as usize) < self.data_length {
            let p = pos as usize;
            let n = left.min(self.buffers.space_for_sample(p));
            self.queue.push(CopyCmd::OutOfMain {
                src: p,
                dst,
                len: n,
                gain: self.gain,
            });
            pos += n as i32;
            dst += n;
            left -= n;
        }
        Ok(())
    }

    /// Applies everything `process` queued this cycle.
    ///
    /// `record_src` and `playback_dst` are the whole cycle's port buffers, as
    /// handed to `set_*_buffer_size`; queued offsets index into them.
    pub fn finalize_process(&mut self, record_src: &[f32], playback_dst: &mut [f32]) {
        let mut peak = self.output_peak;
        let mut published_peak = 0.0f32;
        for cmd in self.queue.drain(..) {
            match cmd {
                CopyCmd::IntoMain { dst, src, len } => {
                    let source = &record_src[src..src + len];
                    copy_in(&mut self.buffers, dst, source);
                    self.state.write_data(dst, source, self.data_length);
                }
                CopyCmd::IntoPreRecord { dst, src, len } => {
                    copy_in(
                        &mut self.prerecord_buffers,
                        dst,
                        &record_src[src..src + len],
                    );
                }
                CopyCmd::OutOfMain {
                    src,
                    dst,
                    len,
                    gain,
                } => {
                    if let Some(from) = self.buffers.chunk_slice(src) {
                        for i in 0..len {
                            let sample = playback_dst[dst + i] + from[i] * gain;
                            playback_dst[dst + i] = sample;
                            peak = peak.max(sample.abs());
                            published_peak = published_peak.max(sample.abs());
                        }
                    }
                }
            }
        }
        self.output_peak = peak;
        self.state.publish_output_peak(published_peak);
        self.publish_state();
    }
}

/// Writes `src` into `buffers` starting at `at`. The caller has already split at
/// chunk boundaries, so this stays within one chunk.
fn copy_in(buffers: &mut ChunkedSamples<f32>, at: usize, src: &[f32]) {
    buffers.ensure_available(at + src.len().saturating_sub(1));
    if let Some(dst) = buffers.chunk_slice_mut(at) {
        let n = src.len().min(dst.len());
        dst[..n].copy_from_slice(&src[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    use ChannelMode as C;
    use LoopMode as L;

    fn channel() -> AudioChannel {
        AudioChannel::with_chunk_size(4, C::Direct)
    }

    /// Runs one cycle: sizes the port buffers, processes, finalizes.
    fn cycle(
        ch: &mut AudioChannel,
        mode: LoopMode,
        n: usize,
        pos: i32,
        length: usize,
        input: &[f32],
    ) -> Vec<f32> {
        ch.set_recording_buffer_size(input.len().max(n));
        ch.set_playback_buffer_size(n);
        let mut out = vec![0.0; n];
        let mut src = input.to_vec();
        src.resize(input.len().max(n), 0.0);
        let_assert!(Ok(()) = ch.process(mode, L::Unknown, None, None, n, pos, length));
        ch.finalize_process(&src, &mut out);
        out
    }

    #[test]
    fn records_input_and_grows_length() {
        let mut ch = channel();
        cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]);
        check!(ch.length() == 4);
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn recording_spans_chunk_boundaries() {
        let mut ch = channel();
        // 6 samples into 4-sample chunks: split into two queued copies.
        cycle(
            &mut ch,
            L::Recording,
            6,
            0,
            0,
            &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
        );
        check!(ch.length() == 6);
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn recording_appends_at_existing_length() {
        let mut ch = channel();
        cycle(&mut ch, L::Recording, 3, 0, 0, &[1.0, 2.0, 3.0]);
        cycle(&mut ch, L::Recording, 3, 0, 3, &[4.0, 5.0, 6.0]);
        check!(ch.length() == 6);
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn plays_back_additively_with_gain() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_gain(2.0);
        let out = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(out == vec![2.0, 4.0, 6.0, 8.0]);
        check!(ch.played_back_sample() == Some(0));
    }

    #[test]
    fn playback_adds_on_top_of_existing_output() {
        let mut ch = channel();
        ch.load_data(&[1.0, 1.0]);
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let mut out = vec![10.0, 20.0];
        let_assert!(Ok(()) = ch.process(L::Playing, L::Unknown, None, None, 2, 0, 2));
        ch.finalize_process(&[0.0, 0.0], &mut out);
        check!(out == vec![11.0, 21.0]);
    }

    #[test]
    fn playback_tracks_output_peak() {
        let mut ch = channel();
        ch.load_data(&[0.5, -0.9, 0.2, 0.0]);
        cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(ch.output_peak() == 0.9);
        ch.reset_output_peak();
        check!(ch.output_peak() == 0.0);
    }

    #[test]
    fn playback_stops_at_recorded_length() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        // Ask for 4 but only 2 are recorded; the rest stays silent.
        let out = cycle(&mut ch, L::Playing, 4, 0, 2, &[]);
        check!(out == vec![1.0, 2.0, 0.0, 0.0]);
    }

    #[test]
    fn playback_past_recorded_length_stops_at_chunk_granularity() {
        let mut ch = channel();
        // Two full chunks of content, recorded length shortened to 2.
        ch.load_data(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
        ch.set_length(2);
        let out = cycle(&mut ch, L::Playing, 8, 0, 2, &[]);
        // The recorded-length check gates entry to each chunk, not the size of
        // the copy within it. So the whole first chunk sounds even though only
        // 2 samples are "recorded", and the second chunk is never entered.
        check!(out == vec![1.0, 2.0, 3.0, 4.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn playback_honours_start_offset() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        let out = cycle(&mut ch, L::Playing, 2, 0, 4, &[]);
        check!(out == vec![3.0, 4.0]);
    }

    #[test]
    fn playback_before_start_offset_is_skipped_without_pre_play() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        // position -2 + offset 2 = 0, below the start offset, so the first two
        // output samples are skipped rather than sounding.
        let out = cycle(&mut ch, L::Playing, 4, -2, 4, &[]);
        check!(out == vec![0.0, 0.0, 3.0, 4.0]);
    }

    #[test]
    fn pre_play_opens_the_window_earlier() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        ch.set_pre_play_samples(2);
        // Now playback may reach back to sample 0.
        let out = cycle(&mut ch, L::Playing, 4, -2, 4, &[]);
        check!(out == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn replace_overwrites_in_place_without_growing() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        cycle(&mut ch, L::Replacing, 2, 1, 4, &[9.0, 8.0]);
        check!(ch.length() == 4);
        check!(ch.data() == vec![1.0, 9.0, 8.0, 4.0]);
    }

    #[test]
    fn replace_skips_negative_positions() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        // Starts two samples early; those two inputs are discarded.
        cycle(&mut ch, L::Replacing, 4, -2, 4, &[7.0, 8.0, 9.0, 6.0]);
        check!(ch.data() == vec![9.0, 6.0, 3.0, 4.0]);
    }

    #[test]
    fn replace_past_recorded_length_errors() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        let r = ch.process(L::Replacing, L::Unknown, None, None, 4, 0, 2);
        let_assert!(Err(ChannelError::ReplaceOutOfBounds { position, length }) = r);
        check!(position == 2);
        check!(length == 2);
    }

    #[test]
    fn record_beyond_input_buffer_errors() {
        let mut ch = channel();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(8);
        let r = ch.process(L::Recording, L::Unknown, None, None, 8, 0, 0);
        let_assert!(
            Err(ChannelError::RecordOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[test]
    fn playback_beyond_output_buffer_errors() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        ch.set_recording_buffer_size(8);
        ch.set_playback_buffer_size(2);
        let r = ch.process(L::Playing, L::Unknown, None, None, 8, 0, 2);
        let_assert!(
            Err(ChannelError::PlaybackOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[test]
    fn without_buffers_nothing_is_attempted() {
        let mut ch = channel();
        ch.clear_buffers();
        // No port buffers assigned: record/replace/playback are all masked off.
        let_assert!(Ok(()) = ch.process(L::Recording, L::Unknown, None, None, 4, 0, 0));
        check!(ch.length() == 0);
    }

    #[test]
    fn disabled_channel_does_nothing() {
        let mut ch = AudioChannel::with_chunk_size(4, C::Disabled);
        cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]);
        check!(ch.length() == 0);
    }

    #[test]
    fn pre_record_buffers_carry_over_into_record() {
        let mut ch = channel();
        // Recording is one trigger away, so this cycle pre-records.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let_assert!(Ok(()) = ch.process(L::Stopped, L::Recording, Some(0), Some(2), 2, 0, 0));
        ch.finalize_process(&[5.0, 6.0], &mut [0.0, 0.0]);
        check!(ch.length() == 0); // main storage untouched so far

        // Now recording proper begins: the pre-recorded samples become content
        // and the start offset marks where "sample 0" really is.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let_assert!(Ok(()) = ch.process(L::Recording, L::Unknown, None, None, 2, 0, 0));
        ch.finalize_process(&[7.0, 8.0], &mut [0.0, 0.0]);
        check!(ch.start_offset() == 2);
        check!(ch.data() == vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[test]
    fn pre_record_discarded_when_recording_does_not_follow() {
        let mut ch = channel();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let_assert!(Ok(()) = ch.process(L::Stopped, L::Recording, Some(0), Some(2), 2, 0, 0));
        ch.finalize_process(&[5.0, 6.0], &mut [0.0, 0.0]);

        // Pre-record ends without entering Recording: buffers are dropped.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let_assert!(Ok(()) = ch.process(L::Stopped, L::Unknown, None, None, 2, 0, 0));
        ch.finalize_process(&[7.0, 8.0], &mut [0.0, 0.0]);
        check!(ch.length() == 0);
        check!(ch.start_offset() == 0);
    }

    #[test]
    fn next_poi_is_smallest_remaining_buffer() {
        let mut ch = channel();
        ch.set_playback_buffer_size(8);
        ch.set_recording_buffer_size(3);
        // Playing only consults the playback buffer.
        check!(ch.next_poi(L::Playing, L::Unknown, None, None, 0) == Some(8));
        // Recording only consults the input buffer.
        check!(ch.next_poi(L::Recording, L::Unknown, None, None, 0) == Some(3));
        // Stopped needs neither.
        check!(ch.next_poi(L::Stopped, L::Unknown, None, None, 0) == None);
    }

    #[test]
    fn disabled_channel_has_no_poi() {
        let mut ch = AudioChannel::with_chunk_size(4, C::Disabled);
        ch.set_playback_buffer_size(8);
        check!(ch.next_poi(L::Playing, L::Unknown, None, None, 0) == None);
    }

    #[test]
    fn data_seq_nr_advances_on_content_change() {
        let mut ch = channel();
        let before = ch.data_seq_nr();
        cycle(&mut ch, L::Recording, 2, 0, 0, &[1.0, 2.0]);
        check!(ch.data_seq_nr() > before);
        let after_record = ch.data_seq_nr();
        // Playback does not change content.
        cycle(&mut ch, L::Playing, 2, 0, 2, &[]);
        check!(ch.data_seq_nr() == after_record);
    }

    #[test]
    fn clear_sets_length_and_resets_offset() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0]);
        ch.set_start_offset(2);
        ch.clear(8);
        check!(ch.length() == 8);
        check!(ch.start_offset() == 0);
    }

    #[test]
    fn stopped_channel_reports_no_played_back_sample() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        cycle(&mut ch, L::Playing, 2, 0, 2, &[]);
        check!(ch.played_back_sample() == Some(0));
        cycle(&mut ch, L::Stopped, 2, 0, 2, &[]);
        check!(ch.played_back_sample() == None);
    }
}
