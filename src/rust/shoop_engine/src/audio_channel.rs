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
use crate::content_snapshot::AudioProcessSnapshotWriter;
use crate::latency_runtime::{cyclic_render_dispatch_position, LatchedLatency, PreparedLatency};
use crate::loop_mode::LoopMode;
use crate::state_mirror::AudioChannelStateMirror;
use shoop_latency::{CaptureFrameMapping, LatencyDomainError, MAX_COMPENSATION_FRAMES};

/// At most two copy commands (record and playback) per session sub-block.
/// The session processes no more than 16 sub-blocks in one callback.
const COPY_COMMAND_CAPACITY: usize = 32;

#[derive(Debug)]
struct PlaybackSmoother {
    duration_frames: usize,
    raw_active: bool,
    expected_source: Option<usize>,
    previous_gain: f32,
    previous_rendered: f32,
    correction: f32,
    correction_step: f32,
    correction_frames_left: usize,
}

impl Default for PlaybackSmoother {
    fn default() -> Self {
        Self {
            duration_frames: 0,
            raw_active: false,
            expected_source: None,
            previous_gain: 1.0,
            previous_rendered: 0.0,
            correction: 0.0,
            correction_step: 0.0,
            correction_frames_left: 0,
        }
    }
}

impl PlaybackSmoother {
    fn set_duration_frames(&mut self, frames: usize) {
        self.duration_frames = frames;
    }

    fn is_bypassed(&self) -> bool {
        self.duration_frames == 0 && self.correction_frames_left == 0
    }

    fn observe_range(&mut self, active: bool, source: usize, gain: f32, raw_last: f32, len: usize) {
        if len == 0 {
            return;
        }
        self.raw_active = active;
        self.expected_source = active.then(|| source.saturating_add(len));
        self.previous_gain = gain;
        self.previous_rendered = raw_last;
    }

    fn render(&mut self, raw: f32, active: bool, source: usize, gain: f32) -> f32 {
        let is_edge = active != self.raw_active
            || (active
                && (self.expected_source != Some(source)
                    || self.previous_gain.to_bits() != gain.to_bits()));
        if is_edge && self.duration_frames > 0 {
            self.correction = self.previous_rendered - raw;
            self.correction_step = self.correction / self.duration_frames as f32;
            self.correction_frames_left = self.duration_frames;
        }

        let rendered = raw + self.correction;
        if self.correction_frames_left > 0 {
            self.correction_frames_left -= 1;
            if self.correction_frames_left == 0 {
                self.correction = 0.0;
                self.correction_step = 0.0;
            } else {
                self.correction -= self.correction_step;
            }
        }

        self.raw_active = active;
        self.expected_source = active.then(|| source.saturating_add(1));
        self.previous_gain = gain;
        self.previous_rendered = rendered;
        rendered
    }
}

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
    #[error("latency mapping exceeds the supported signed media position")]
    LatencyPositionOverflow,
    #[error("retained latency margin {frames} exceeds the supported maximum")]
    RetentionExceedsMaximum { frames: u32 },
    #[error(
        "retained latency window needs {required} samples but prepared capacity is {capacity}"
    )]
    RetentionCapacity { required: usize, capacity: usize },
    #[error("recording storage is exhausted at its prepared capacity of {capacity} samples")]
    StorageExhausted { capacity: usize },
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

fn snapshot_mutation(flags: ProcessFlags) -> Option<crate::content_snapshot::ContentMutation> {
    if flags.contains(ProcessFlags::REPLACE) {
        Some(crate::content_snapshot::ContentMutation::Replacing)
    } else if flags.contains(ProcessFlags::RECORD) {
        Some(crate::content_snapshot::ContentMutation::Recording)
    } else if flags.contains(ProcessFlags::PRE_RECORD) {
        Some(crate::content_snapshot::ContentMutation::PreRecording)
    } else {
        None
    }
}

/// Tracks how much of a port buffer this cycle has been consumed.
#[derive(Debug, Clone, Copy, Default)]
struct CycleBuf {
    cursor: usize,
    remaining: usize,
}

#[derive(Debug)]
pub struct PreparedAudioChannelData {
    buffers: ChunkedSamples<f32>,
    length: usize,
}

impl PreparedAudioChannelData {
    pub fn new(chunk_size: usize, capacity: usize) -> Self {
        let n_chunks = capacity.max(1).div_ceil(chunk_size.max(1));
        Self {
            buffers: ChunkedSamples::with_reserve(chunk_size.max(1), n_chunks.saturating_sub(1)),
            length: 0,
        }
    }

    pub fn capacity(&self) -> usize {
        (self.buffers.n_chunks() + self.buffers.n_spare()) * self.buffers.chunk_size()
    }

    pub(crate) fn begin_load(&mut self, length: usize) {
        debug_assert!(length <= self.capacity());
        self.buffers.reset();
        if length > 0 {
            self.buffers.ensure_available(length - 1);
        }
        self.length = length;
    }

    pub(crate) fn write(&mut self, mut offset: usize, mut samples: &[f32]) {
        while !samples.is_empty() {
            let available = self.buffers.space_for_sample(offset).min(samples.len());
            let destination = self
                .buffers
                .chunk_slice_mut(offset)
                .expect("prepared adoption storage has sufficient capacity");
            destination[..available].copy_from_slice(&samples[..available]);
            offset += available;
            samples = &samples[available..];
        }
    }

    #[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
    pub(crate) fn contiguous_copy(&self) -> Vec<f32> {
        self.buffers.contiguous_copy(self.length)
    }

    pub(crate) fn copy_to_preallocated(&self, destination: &mut Vec<f32>) {
        debug_assert!(destination.capacity() >= self.length);
        destination.resize(self.length, 0.0);
        let mut offset = 0;
        while offset < self.length {
            let source = self
                .buffers
                .chunk_slice(offset)
                .expect("prepared adoption data is initialized");
            let count = source.len().min(self.length - offset);
            destination[offset..offset + count].copy_from_slice(&source[..count]);
            offset += count;
        }
    }
}

#[derive(Debug)]
pub struct AudioChannel {
    buffers: ChunkedSamples<f32>,
    data_length: usize,
    prerecord_buffers: ChunkedSamples<f32>,
    prerecord_data_length: usize,

    /// Raw media layout offset retained for legacy lead-in/preplay semantics.
    start_offset: i32,
    /// Frozen raw-take to logical-timeline mapping. Positive values select later raw media.
    capture_alignment_frames: i32,
    /// Ephemeral early dispatch for current processor rendering; never persisted into the take.
    render_advance_frames: u32,
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
    content_snapshots: Option<AudioProcessSnapshotWriter>,
    publish_snapshot_updates: bool,
    storage_capacity: Option<usize>,
    storage_exhaustions: u32,
    retained_before_frames: u32,
    retained_after_frames: u32,
    postroll_remaining_frames: u32,
    finish_recording_after_finalize: bool,
    latency_retention_incomplete: bool,
    pending_latency: Option<PreparedLatency>,
    latched_latency: Option<LatchedLatency>,
    playback_smoother: PlaybackSmoother,
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
        Self::with_chunk_size_state_and_snapshots(chunk_size, mode, state, None)
    }

    pub fn with_bounded_capacity(chunk_size: usize, capacity: usize, mode: ChannelMode) -> Self {
        let mut channel = Self::with_bounded_capacity_unprepared(chunk_size, capacity, mode);
        channel.prepare_bounded_capacity();
        channel
    }

    pub fn with_bounded_capacity_unprepared(
        chunk_size: usize,
        capacity: usize,
        mode: ChannelMode,
    ) -> Self {
        let mut channel = Self::with_chunk_size(chunk_size, mode);
        channel.buffers = ChunkedSamples::with_bounded_capacity_unprepared(chunk_size, capacity);
        channel.prerecord_buffers =
            ChunkedSamples::with_bounded_capacity_unprepared(chunk_size, capacity);
        channel.storage_capacity = Some(capacity.max(1));
        channel
    }

    pub fn prepare_bounded_capacity(&mut self) {
        self.buffers.prepare_bounded_capacity();
        self.prerecord_buffers.prepare_bounded_capacity();
    }

    pub fn with_chunk_size_state_and_snapshots(
        chunk_size: usize,
        mode: ChannelMode,
        state: Arc<AudioChannelStateMirror>,
        content_snapshots: Option<AudioProcessSnapshotWriter>,
    ) -> Self {
        let channel = Self {
            buffers: ChunkedSamples::with_chunk_size(chunk_size),
            data_length: 0,
            prerecord_buffers: ChunkedSamples::with_chunk_size(chunk_size),
            prerecord_data_length: 0,
            start_offset: 0,
            capture_alignment_frames: 0,
            render_advance_frames: 0,
            pre_play_samples: 0,
            output_peak: 0.0,
            gain: 1.0,
            mode,
            data_seq_nr: 0,
            last_played_back_sample: None,
            prev_process_flags: ProcessFlags::NONE,
            playback: None,
            recording: None,
            queue: Vec::with_capacity(COPY_COMMAND_CAPACITY),
            state,
            content_snapshots,
            publish_snapshot_updates: true,
            storage_capacity: None,
            storage_exhaustions: 0,
            retained_before_frames: 0,
            retained_after_frames: 0,
            postroll_remaining_frames: 0,
            finish_recording_after_finalize: false,
            latency_retention_incomplete: false,
            pending_latency: None,
            latched_latency: None,
            playback_smoother: PlaybackSmoother::default(),
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
            self.capture_alignment_frames,
            self.postroll_remaining_frames,
            self.render_advance_frames,
            self.last_played_back_sample,
            self.pre_play_samples,
            self.data_seq_nr as u64,
        );
    }

    fn publish_all_data(&mut self) {
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Loading);
            let mut offset = 0;
            while offset < self.data_length {
                let source = self
                    .buffers
                    .chunk_slice(offset)
                    .expect("channel data is addressable");
                let count = source.len().min(self.data_length - offset);
                snapshots.publish_range(
                    offset,
                    &source[..count],
                    self.data_length,
                    offset + count == self.data_length,
                );
                offset += count;
            }
            if self.data_length == 0 {
                snapshots.publish_range(0, &[], 0, true);
            }
            snapshots.finish_mutation(false);
        }
    }

    // --- accessors ---

    pub fn length(&self) -> usize {
        self.data_length
    }
    pub fn chunk_size(&self) -> usize {
        self.buffers.chunk_size()
    }
    pub fn set_length(&mut self, length: usize) {
        self.data_length = length;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Loading);
            snapshots.publish_range(0, &[], length, true);
            snapshots.finish_mutation(false);
        }
        self.data_changed();
    }
    pub fn mode(&self) -> ChannelMode {
        self.mode
    }
    pub const fn pending_latency(&self) -> Option<PreparedLatency> {
        self.pending_latency
    }

    pub const fn latched_latency(&self) -> Option<LatchedLatency> {
        self.latched_latency
    }

    pub fn set_pending_latency(&mut self, values: PreparedLatency) {
        self.pending_latency = Some(values);
    }

    pub fn latch_recording_latency(
        &mut self,
        operation_frame: u64,
        require_captured_prerecord: bool,
        recording_offset: shoop_latency::RecordingOffset,
    ) -> bool {
        let Some(values) = self.pending_latency else {
            return false;
        };
        let frames = recording_offset.frames();
        self.capture_alignment_frames = frames;
        self.latency_retention_incomplete = if frames >= 0 {
            frames as u32 > self.retained_after_frames
        } else {
            let required = frames.unsigned_abs();
            required > self.retained_before_frames
                || (require_captured_prerecord && required as usize > self.prerecord_data_length)
        };
        self.latched_latency = Some(LatchedLatency {
            values,
            operation_frame,
        });
        self.publish_state();
        true
    }

    pub fn latch_render_latency(&mut self, operation_frame: u64) -> bool {
        let Some(values) = self.pending_latency else {
            return false;
        };
        self.render_advance_frames = values.processor_advance().frames();
        self.latched_latency = Some(LatchedLatency {
            values,
            operation_frame,
        });
        self.publish_state();
        true
    }

    pub fn latch_canonical_render_destination(&mut self, operation_frame: u64) -> bool {
        let Some(values) = self.pending_latency else {
            return false;
        };
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        self.latched_latency = Some(LatchedLatency {
            values,
            operation_frame,
        });
        self.publish_state();
        true
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
    pub fn media_layout_offset(&self) -> i32 {
        self.start_offset
    }
    pub fn capture_alignment_frames(&self) -> i32 {
        self.capture_alignment_frames
    }
    pub fn set_capture_alignment_frames(&mut self, frames: i32) -> Result<(), LatencyDomainError> {
        if frames.unsigned_abs() > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::ValueExceedsMaximum(
                frames.unsigned_abs(),
            ));
        }
        self.capture_alignment_frames = frames;
        self.publish_state();
        Ok(())
    }
    pub fn render_advance_frames(&self) -> u32 {
        self.render_advance_frames
    }
    pub fn retained_before_frames(&self) -> u32 {
        self.retained_before_frames
    }
    pub fn retained_after_frames(&self) -> u32 {
        self.retained_after_frames
    }
    pub fn restore_retained_margin_metadata(
        &mut self,
        retained_before_frames: u32,
        retained_after_frames: u32,
    ) -> Result<(), ChannelError> {
        for frames in [retained_before_frames, retained_after_frames] {
            if frames > shoop_latency::MAX_RETAINED_MARGIN_FRAMES {
                return Err(ChannelError::RetentionExceedsMaximum { frames });
            }
        }
        self.retained_before_frames = retained_before_frames;
        self.retained_after_frames = retained_after_frames;
        Ok(())
    }
    pub fn is_finalizing_latency_postroll(&self) -> bool {
        self.postroll_remaining_frames > 0 || self.finish_recording_after_finalize
    }
    pub fn has_unsettled_latency_postroll(&self) -> bool {
        self.is_finalizing_latency_postroll()
            || (self.retained_after_frames > 0
                && self.prev_process_flags.contains(ProcessFlags::RECORD))
    }
    pub fn latency_postroll_remaining_frames(&self) -> u32 {
        self.postroll_remaining_frames
    }
    pub fn latency_retention_incomplete(&self) -> bool {
        self.latency_retention_incomplete
    }
    pub fn compensated_take_ready(&self, logical_length: u32) -> bool {
        let Some(raw_start) = self.raw_position_for_logical(0) else {
            return false;
        };
        if self.capture_alignment_frames <= 0
            || (self.capture_alignment_frames as u32) < logical_length
        {
            return raw_start >= 0;
        }
        let Some(raw_end) = raw_start.checked_add(logical_length.min(i32::MAX as u32) as i32)
        else {
            return false;
        };
        raw_start >= 0 && raw_end >= 0 && raw_end as usize <= self.data_length
    }
    pub fn prepare_latency_retention(
        &mut self,
        logical_capacity: usize,
        retained_before_frames: u32,
        retained_after_frames: u32,
    ) -> Result<(), ChannelError> {
        for frames in [retained_before_frames, retained_after_frames] {
            if frames > shoop_latency::MAX_RETAINED_MARGIN_FRAMES {
                return Err(ChannelError::RetentionExceedsMaximum { frames });
            }
        }
        let required = logical_capacity
            .checked_add(retained_before_frames as usize)
            .and_then(|value| value.checked_add(retained_after_frames as usize))
            .ok_or(ChannelError::RetentionCapacity {
                required: usize::MAX,
                capacity: self.storage_capacity.unwrap_or(usize::MAX),
            })?;
        if let Some(capacity) = self.storage_capacity {
            if required > capacity {
                return Err(ChannelError::RetentionCapacity { required, capacity });
            }
            self.prepare_bounded_capacity();
        } else if required > 0 {
            self.buffers.ensure_available(required - 1);
            if retained_before_frames > 0 {
                self.prerecord_buffers
                    .ensure_available(retained_before_frames as usize - 1);
            }
        }
        self.retained_before_frames = retained_before_frames;
        self.retained_after_frames = retained_after_frames;
        self.latency_retention_incomplete = false;
        Ok(())
    }
    pub fn set_render_advance_frames(&mut self, frames: u32) -> Result<(), LatencyDomainError> {
        if frames > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::ValueExceedsMaximum(frames));
        }
        self.render_advance_frames = frames;
        self.publish_state();
        Ok(())
    }
    pub fn raw_position_for_logical(&self, logical_position: i32) -> Option<i32> {
        let mapping = CaptureFrameMapping::new(self.capture_alignment_frames).ok()?;
        i32::try_from(
            mapping
                .raw_media_frame(i64::from(logical_position), i64::from(self.start_offset))
                .ok()?,
        )
        .ok()
    }
    pub fn dispatch_raw_position_for_logical(&self, logical_position: i32) -> Option<i32> {
        self.raw_position_for_logical(logical_position)?
            .checked_add(i32::try_from(self.render_advance_frames).ok()?)
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
    pub fn set_loop_smoothing_frames(&mut self, frames: usize) {
        self.playback_smoother.set_duration_frames(frames);
    }
    pub fn loop_smoothing_frames(&self) -> usize {
        self.playback_smoother.duration_frames
    }
    pub fn data_seq_nr(&self) -> u32 {
        self.data_seq_nr
    }
    pub fn storage_exhaustions(&self) -> u32 {
        self.storage_exhaustions
    }
    pub fn storage_remaining(&self) -> Option<usize> {
        self.storage_capacity
            .map(|capacity| capacity.saturating_sub(self.data_length))
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

    pub fn data_range(&self, offset: usize, max_samples: usize) -> Vec<f32> {
        let end = offset.saturating_add(max_samples).min(self.data_length);
        if offset >= end {
            return Vec::new();
        }
        (offset..end)
            .filter_map(|position| self.at(position))
            .collect()
    }

    pub fn load_data(&mut self, samples: &[f32]) {
        self.buffers.set_contents(samples);
        self.data_length = samples.len();
        self.start_offset = 0;
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Loading);
            snapshots.publish_range(0, samples, samples.len(), true);
            snapshots.finish_mutation(false);
        }
        self.data_changed();
    }

    pub(crate) fn can_load_without_allocation(&self, length: usize) -> bool {
        let available_chunks = self.buffers.n_chunks() + self.buffers.n_spare();
        length <= available_chunks.saturating_mul(self.buffers.chunk_size())
    }

    pub(crate) fn begin_bounded_load(&mut self, length: usize) {
        debug_assert!(self.can_load_without_allocation(length));
        self.buffers.reset();
        if length > 0 {
            self.buffers.ensure_available(length - 1);
        }
        self.data_length = length;
        self.start_offset = 0;
    }

    pub(crate) fn write_bounded_load(&mut self, mut offset: usize, mut samples: &[f32]) {
        debug_assert!(offset.saturating_add(samples.len()) <= self.data_length);
        while !samples.is_empty() {
            let available = self.buffers.space_for_sample(offset).min(samples.len());
            let destination = self
                .buffers
                .chunk_slice_mut(offset)
                .expect("bounded load storage was prepared");
            destination[..available].copy_from_slice(&samples[..available]);
            offset += available;
            samples = &samples[available..];
        }
    }

    pub(crate) fn finish_bounded_load(&mut self) {
        self.publish_all_data();
        self.data_changed();
    }

    pub(crate) fn commit_prepared_data(&mut self, prepared: &mut PreparedAudioChannelData) {
        std::mem::swap(&mut self.buffers, &mut prepared.buffers);
        std::mem::swap(&mut self.data_length, &mut prepared.length);
        self.start_offset = 0;
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        self.publish_all_data();
        self.data_changed();
    }

    pub(crate) fn commit_prepared_data_and_snapshot(
        &mut self,
        prepared: &mut PreparedAudioChannelData,
        snapshot: crate::content_snapshot::PreparedAudioSnapshot,
    ) {
        std::mem::swap(&mut self.buffers, &mut prepared.buffers);
        std::mem::swap(&mut self.data_length, &mut prepared.length);
        self.start_offset = 0;
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.install_prepared(snapshot);
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
        self.prerecord_buffers.reset();
        self.prerecord_data_length = 0;
        self.queue.clear();
        self.start_offset = 0;
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        self.prev_process_flags = ProcessFlags::NONE;
        self.retained_before_frames = 0;
        self.retained_after_frames = 0;
        self.postroll_remaining_frames = 0;
        self.finish_recording_after_finalize = false;
        self.latency_retention_incomplete = false;
        self.latched_latency = None;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.clear_prerecord();
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Clearing);
            snapshots.publish_range(0, &[], length, true);
            snapshots.finish_mutation(false);
        }
        self.data_changed();
    }

    pub(crate) fn abort_latency_take(&mut self) {
        self.clear(0);
        self.prerecord_buffers.reset();
        self.prerecord_data_length = 0;
        self.queue.clear();
        self.prev_process_flags = ProcessFlags::NONE;
        self.postroll_remaining_frames = 0;
        self.finish_recording_after_finalize = false;
        self.latency_retention_incomplete = false;
        self.latched_latency = None;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.clear_prerecord();
        }
        self.publish_state();
    }

    /// Replaces `length` samples with silence.
    ///
    /// Distinct from [`Self::clear`] because clearing to a non-zero length leaves the previous
    /// recording in the chunks, where it stays both audible and visible in the waveform.
    pub fn silence(&mut self, length: usize) {
        self.buffers.fill(length, 0.0);
        self.data_length = length;
        self.start_offset = 0;
        self.capture_alignment_frames = 0;
        self.render_advance_frames = 0;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Clearing);
            snapshots.publish_silence(length);
            snapshots.finish_mutation(false);
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

        if self.prev_process_flags.contains(ProcessFlags::RECORD)
            && !flags.contains(ProcessFlags::RECORD)
            && self.postroll_remaining_frames == 0
        {
            self.postroll_remaining_frames = self.retained_after_frames;
        }
        let postroll_samples = if !flags.contains(ProcessFlags::RECORD) && self.recording.is_some()
        {
            n_samples.min(self.postroll_remaining_frames as usize)
        } else {
            0
        };

        let previous_mutation = snapshot_mutation(self.prev_process_flags);
        let current_mutation = snapshot_mutation(flags).or_else(|| {
            (postroll_samples > 0).then_some(crate::content_snapshot::ContentMutation::Recording)
        });
        if previous_mutation != current_mutation {
            if let Some(previous) = previous_mutation {
                if let Some(snapshots) = self.content_snapshots.as_mut() {
                    if previous == crate::content_snapshot::ContentMutation::PreRecording {
                        snapshots.cancel_mutation();
                    } else {
                        snapshots.finish_mutation(
                            previous == crate::content_snapshot::ContentMutation::Replacing,
                        );
                    }
                }
            }
            if let Some(mutation) = current_mutation {
                if let Some(snapshots) = self.content_snapshots.as_mut() {
                    snapshots.begin_working_generation();
                    snapshots.begin_mutation(mutation);
                }
            }
        }
        self.publish_snapshot_updates =
            current_mutation != Some(crate::content_snapshot::ContentMutation::Replacing);

        if !flags.contains(ProcessFlags::PRE_RECORD)
            && self.prev_process_flags.contains(ProcessFlags::PRE_RECORD)
        {
            if flags.contains(ProcessFlags::RECORD) {
                // Transitioning pre-record -> record: adopt what was buffered,
                // and offset playback so the lead-in sits before sample 0.
                // Adopt the pre-recorded chunks by swapping ownership. Cloning here used
                // to allocate in the callback exactly when recording began; the displaced
                // main storage becomes the reusable prerecord buffer below.
                std::mem::swap(&mut self.buffers, &mut self.prerecord_buffers);
                self.data_length = self.prerecord_data_length;
                self.start_offset = self.prerecord_data_length as i32;
                if let Some(snapshots) = self.content_snapshots.as_mut() {
                    snapshots.adopt_prerecord(self.data_length);
                }
            } else if let Some(snapshots) = self.content_snapshots.as_mut() {
                snapshots.clear_prerecord();
            }
            self.prerecord_buffers.reset();
            self.prerecord_data_length = 0;
        }

        if postroll_samples > 0 {
            self.process_record(postroll_samples, self.data_length, false)?;
            self.postroll_remaining_frames = self
                .postroll_remaining_frames
                .saturating_sub(postroll_samples as u32);
            self.finish_recording_after_finalize = self.postroll_remaining_frames == 0;
        }

        if flags.contains(ProcessFlags::PLAYBACK) {
            self.last_played_back_sample = Some(params.position);
            let mapping = CaptureFrameMapping::new(self.capture_alignment_frames)
                .map_err(|_| ChannelError::LatencyPositionOverflow)?;
            let render_advance_frames = if matches!(
                mode,
                LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet
            ) || (params.flags.contains(ProcessFlags::PLAYBACK)
                && matches!(
                    next_mode,
                    LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet
                )) {
                self.render_advance_frames
            } else {
                0
            };
            let dispatch_position = cyclic_render_dispatch_position(
                params.position,
                self.start_offset,
                self.capture_alignment_frames,
                render_advance_frames,
                length_before.min(u32::MAX as usize) as u32,
            )
            .ok_or(ChannelError::LatencyPositionOverflow)?;
            let cyclic_window = (render_advance_frames > 0 && length_before > 0)
                .then(|| {
                    let start = i32::try_from(
                        mapping
                            .raw_frame(i64::from(self.start_offset))
                            .map_err(|_| ChannelError::LatencyPositionOverflow)?,
                    )
                    .map_err(|_| ChannelError::LatencyPositionOverflow)?;
                    let length = i32::try_from(length_before)
                        .map_err(|_| ChannelError::LatencyPositionOverflow)?;
                    let end = start
                        .checked_add(length)
                        .ok_or(ChannelError::LatencyPositionOverflow)?;
                    Ok::<_, ChannelError>((start, end))
                })
                .transpose()?;
            self.process_playback(dispatch_position, n_samples, cyclic_window)?;
        } else {
            self.last_played_back_sample = None;
        }
        if flags.contains(ProcessFlags::RECORD) {
            let from = (length_before as i64 + self.start_offset as i64).max(0) as usize;
            self.process_record(n_samples, from, false)?;
        }
        if flags.contains(ProcessFlags::REPLACE) {
            // Live replacement input already arrives on the raw capture timeline.
            // Adding the take's playback alignment here would apply it twice.
            self.process_replace(params.position, n_samples)?;
        }
        if flags.contains(ProcessFlags::PRE_RECORD) {
            let from = self.prerecord_data_length;
            self.process_record(n_samples, from, true)?;
        }
        if self.prev_process_flags.contains(ProcessFlags::REPLACE)
            && !flags.contains(ProcessFlags::REPLACE)
        {}
        self.prev_process_flags = if self.postroll_remaining_frames > 0 {
            flags.with(ProcessFlags::RECORD)
        } else {
            flags
        };

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

        let requested_end = record_from.saturating_add(n_samples);
        if self
            .storage_capacity
            .is_some_and(|capacity| requested_end > capacity)
        {
            self.storage_exhaustions = self.storage_exhaustions.saturating_add(1);
            return Err(ChannelError::StorageExhausted {
                capacity: self.storage_capacity.unwrap_or(0),
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
        cyclic_window: Option<(i32, i32)>,
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

        if cyclic_window.is_none() {
            // Playback may not start before the pre-play window opens.
            let starting = self
                .start_offset
                .saturating_add(self.capture_alignment_frames)
                .saturating_sub(self.pre_play_samples as i32)
                .max(0);
            let skip = (starting - pos).max(0);
            if skip > 0 {
                let skip = skip as usize;
                pos += skip as i32;
                left = left.saturating_sub(skip);
                dst += skip.min(buf.remaining);
            }
        }

        while left > 0 && pos >= 0 && (pos as usize) < self.data_length {
            let p = pos as usize;
            let until_wrap = cyclic_window
                .map(|(_, end)| (end - pos).max(0) as usize)
                .unwrap_or(usize::MAX);
            let available = cyclic_window
                .map(|_| self.data_length - p)
                .unwrap_or(usize::MAX);
            let n = left
                .min(self.buffers.space_for_sample(p))
                .min(available)
                .min(until_wrap);
            if n == 0 {
                break;
            }
            self.queue.push(CopyCmd::OutOfMain {
                src: p,
                dst,
                len: n,
                gain: self.gain,
            });
            pos += n as i32;
            dst += n;
            left -= n;
            if let Some((start, end)) = cyclic_window {
                if pos == end {
                    pos = start;
                }
            }
        }
        Ok(())
    }

    /// Applies everything `process` queued this cycle.
    ///
    /// `record_src` and `playback_dst` are the whole cycle's port buffers, as
    /// handed to `set_*_buffer_size`; queued offsets index into them.
    pub fn finalize_process(&mut self, record_src: &[f32], playback_dst: &mut [f32]) {
        if self.playback_smoother.is_bypassed() {
            self.finalize_process_bypassed(record_src, playback_dst);
            return;
        }

        let mut peak = self.output_peak;
        let mut published_peak = 0.0f32;
        let mut timeline_cursor = 0;
        for cmd in self.queue.drain(..) {
            match cmd {
                CopyCmd::IntoMain { dst, src, len } => {
                    let source = &record_src[src..src + len];
                    copy_in(&mut self.buffers, dst, source);
                    if let Some(snapshots) = self.content_snapshots.as_mut() {
                        snapshots.publish_range(
                            dst,
                            source,
                            self.data_length,
                            self.publish_snapshot_updates,
                        );
                    }
                }
                CopyCmd::IntoPreRecord { dst, src, len } => {
                    let source = &record_src[src..src + len];
                    copy_in(&mut self.prerecord_buffers, dst, source);
                    if let Some(snapshots) = self.content_snapshots.as_mut() {
                        snapshots.publish_prerecord_range(dst, source, self.prerecord_data_length);
                    }
                }
                CopyCmd::OutOfMain {
                    src,
                    dst,
                    len,
                    gain,
                } => {
                    for at in timeline_cursor..dst.min(playback_dst.len()) {
                        let rendered = self.playback_smoother.render(0.0, false, 0, 1.0);
                        let sample = playback_dst[at] + rendered;
                        playback_dst[at] = sample;
                        peak = peak.max(sample.abs());
                        published_peak = published_peak.max(sample.abs());
                    }
                    if let Some(from) = self.buffers.chunk_slice(src) {
                        for i in 0..len {
                            let raw = from[i] * gain;
                            let rendered = self.playback_smoother.render(raw, true, src + i, gain);
                            let sample = playback_dst[dst + i] + rendered;
                            playback_dst[dst + i] = sample;
                            peak = peak.max(sample.abs());
                            published_peak = published_peak.max(sample.abs());
                        }
                    }
                    timeline_cursor = dst.saturating_add(len);
                }
            }
        }
        for at in timeline_cursor..playback_dst.len() {
            let rendered = self.playback_smoother.render(0.0, false, 0, 1.0);
            let sample = playback_dst[at] + rendered;
            playback_dst[at] = sample;
            peak = peak.max(sample.abs());
            published_peak = published_peak.max(sample.abs());
        }
        self.output_peak = peak;
        self.state.publish_output_peak(published_peak);
        self.finish_latency_recording_mutation();
        self.publish_state();
    }

    fn finalize_process_bypassed(&mut self, record_src: &[f32], playback_dst: &mut [f32]) {
        let mut peak = self.output_peak;
        let mut published_peak = 0.0f32;
        let mut timeline_cursor = 0;
        for cmd in self.queue.drain(..) {
            match cmd {
                CopyCmd::IntoMain { dst, src, len } => {
                    let source = &record_src[src..src + len];
                    copy_in(&mut self.buffers, dst, source);
                    if let Some(snapshots) = self.content_snapshots.as_mut() {
                        snapshots.publish_range(
                            dst,
                            source,
                            self.data_length,
                            self.publish_snapshot_updates,
                        );
                    }
                }
                CopyCmd::IntoPreRecord { dst, src, len } => {
                    let source = &record_src[src..src + len];
                    copy_in(&mut self.prerecord_buffers, dst, source);
                    if let Some(snapshots) = self.content_snapshots.as_mut() {
                        snapshots.publish_prerecord_range(dst, source, self.prerecord_data_length);
                    }
                }
                CopyCmd::OutOfMain {
                    src,
                    dst,
                    len,
                    gain,
                } => {
                    if dst > timeline_cursor {
                        self.playback_smoother.observe_range(
                            false,
                            0,
                            1.0,
                            0.0,
                            dst - timeline_cursor,
                        );
                    }
                    if let Some(from) = self.buffers.chunk_slice(src) {
                        for i in 0..len {
                            let sample = playback_dst[dst + i] + from[i] * gain;
                            playback_dst[dst + i] = sample;
                            peak = peak.max(sample.abs());
                            published_peak = published_peak.max(sample.abs());
                        }
                        self.playback_smoother.observe_range(
                            true,
                            src,
                            gain,
                            from[len - 1] * gain,
                            len,
                        );
                    }
                    timeline_cursor = dst.saturating_add(len);
                }
            }
        }
        if playback_dst.len() > timeline_cursor {
            self.playback_smoother.observe_range(
                false,
                0,
                1.0,
                0.0,
                playback_dst.len() - timeline_cursor,
            );
        }
        self.output_peak = peak;
        self.state.publish_output_peak(published_peak);
        self.finish_latency_recording_mutation();
        self.publish_state();
    }

    fn finish_latency_recording_mutation(&mut self) {
        if self.finish_recording_after_finalize {
            if let Some(snapshots) = self.content_snapshots.as_mut() {
                snapshots.finish_mutation(false);
            }
            self.finish_recording_after_finalize = false;
        }
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
    use assert2::check;

    use ChannelMode as C;
    use LoopMode as L;

    fn channel() -> AudioChannel {
        AudioChannel::with_chunk_size(4, C::Direct)
    }

    fn playback_mode_for_test(role: ChannelMode) -> LoopMode {
        if role == C::Dry {
            L::PlayingDryThroughWet
        } else {
            L::Playing
        }
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
        assert2::assert!(let Ok(()) = ch.process(mode, L::Unknown, None, None, n, pos, length));
        ch.finalize_process(&src, &mut out);
        out
    }

    #[shoop_wasm_test_support::shoop_test]
    fn records_input_and_grows_length() {
        let mut ch = channel();
        cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]);
        check!(ch.length() == 4);
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn recording_appends_at_existing_length() {
        let mut ch = channel();
        cycle(&mut ch, L::Recording, 3, 0, 0, &[1.0, 2.0, 3.0]);
        cycle(&mut ch, L::Recording, 3, 0, 3, &[4.0, 5.0, 6.0]);
        check!(ch.length() == 6);
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn plays_back_additively_with_gain() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_gain(2.0);
        let out = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(out == vec![2.0, 4.0, 6.0, 8.0]);
        check!(ch.played_back_sample() == Some(0));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_adds_on_top_of_existing_output() {
        let mut ch = channel();
        ch.load_data(&[1.0, 1.0]);
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        let mut out = vec![10.0, 20.0];
        assert2::assert!(let Ok(()) = ch.process(L::Playing, L::Unknown, None, None, 2, 0, 2));
        ch.finalize_process(&[0.0, 0.0], &mut out);
        check!(out == vec![11.0, 21.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_tracks_output_peak() {
        let mut ch = channel();
        ch.load_data(&[0.5, -0.9, 0.2, 0.0]);
        cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(ch.output_peak() == 0.9);
        ch.reset_output_peak();
        check!(ch.output_peak() == 0.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_stops_at_recorded_length() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        // Ask for 4 but only 2 are recorded; the rest stays silent.
        let out = cycle(&mut ch, L::Playing, 4, 0, 2, &[]);
        check!(out == vec![1.0, 2.0, 0.0, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn media_layout_capture_alignment_and_render_advance_are_independent() {
        let state = Arc::new(AudioChannelStateMirror::default());
        let mut ch = AudioChannel::with_chunk_size_and_state(4, C::Direct, Arc::clone(&state));
        ch.load_data(&(0..12).map(|sample| sample as f32).collect::<Vec<_>>());
        ch.set_start_offset(2);
        ch.set_capture_alignment_frames(3).unwrap();
        ch.set_render_advance_frames(2).unwrap();
        check!(ch.media_layout_offset() == 2);
        check!(ch.raw_position_for_logical(0) == Some(5));
        check!(ch.dispatch_raw_position_for_logical(0) == Some(7));
        check!(cycle(&mut ch, L::PlayingDryThroughWet, 2, 0, 8, &[]) == vec![7.0, 8.0]);
        let published = state.read(ch.data_seq_nr() as u64);
        check!(published.capture_alignment_frames == 3);
        check!(published.render_advance_frames == 2);

        check!(cycle(&mut ch, L::Playing, 2, 0, 8, &[]) == vec![5.0, 6.0]);

        ch.set_capture_alignment_frames(-2).unwrap();
        ch.set_render_advance_frames(0).unwrap();
        check!(cycle(&mut ch, L::Playing, 2, 0, 2, &[]) == vec![0.0, 1.0]);
        check!(ch
            .set_capture_alignment_frames(-(MAX_COMPENSATION_FRAMES as i32) - 1)
            .is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dry_render_advance_cycles_inside_selected_take() {
        let mut ch = AudioChannel::with_chunk_size(2, C::Dry);
        ch.load_data(&[100.0, 101.0, 102.0, 10.0, 11.0, 12.0, 13.0, 103.0]);
        ch.set_start_offset(2);
        ch.set_capture_alignment_frames(1).unwrap();
        ch.set_render_advance_frames(11).unwrap();

        let out = cycle(&mut ch, L::PlayingDryThroughWet, 4, 0, 4, &[]);
        check!(out == vec![13.0, 10.0, 11.0, 12.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_honours_start_offset() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        let out = cycle(&mut ch, L::Playing, 2, 0, 4, &[]);
        check!(out == vec![3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_before_start_offset_is_skipped_without_pre_play() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        // position -2 + offset 2 = 0, below the start offset, so the first two
        // output samples are skipped rather than sounding.
        let out = cycle(&mut ch, L::Playing, 4, -2, 4, &[]);
        check!(out == vec![0.0, 0.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_play_opens_the_window_earlier() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        ch.set_start_offset(2);
        ch.set_pre_play_samples(2);
        // Now playback may reach back to sample 0.
        let out = cycle(&mut ch, L::Playing, 4, -2, 4, &[]);
        check!(out == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn dry_into_wet_replacement_writes_canonical_logical_frames() {
        let mut ch = AudioChannel::with_chunk_size(4, C::Wet);
        ch.load_data(&[9.0, 9.0, 9.0, 9.0]);
        ch.set_capture_alignment_frames(2).unwrap();
        cycle(
            &mut ch,
            L::RecordingDryIntoWet,
            4,
            0,
            4,
            &[1.0, 2.0, 3.0, 4.0],
        );
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0]);

        ch.set_capture_alignment_frames(0).unwrap();
        check!(cycle(&mut ch, L::Playing, 4, 0, 4, &[]) == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn latency_compatible_replacement_writes_raw_timeline_once() {
        for role in [C::Direct, C::Dry, C::Wet] {
            let mut ch = AudioChannel::with_chunk_size(4, role);
            ch.load_data(&[0.0; 8]);
            ch.set_capture_alignment_frames(3).unwrap();
            cycle(&mut ch, L::Replacing, 1, 5, 8, &[1.0]);
            check!(ch.data()[5] == 1.0);
            check!(cycle(&mut ch, playback_mode_for_test(role), 1, 2, 8, &[]) == vec![1.0]);
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replace_overwrites_in_place_without_growing() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        cycle(&mut ch, L::Replacing, 2, 1, 4, &[9.0, 8.0]);
        check!(ch.length() == 4);
        check!(ch.data() == vec![1.0, 9.0, 8.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replace_skips_negative_positions() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0, 4.0]);
        // Starts two samples early; those two inputs are discarded.
        cycle(&mut ch, L::Replacing, 4, -2, 4, &[7.0, 8.0, 9.0, 6.0]);
        check!(ch.data() == vec![9.0, 6.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replace_past_recorded_length_errors() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        let r = ch.process(L::Replacing, L::Unknown, None, None, 4, 0, 2);
        assert2::assert!(let Err(ChannelError::ReplaceOutOfBounds { position, length }) = r);
        check!(position == 2);
        check!(length == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prepared_latency_postroll_finishes_after_logical_recording_without_growth() {
        let mut ch = AudioChannel::with_bounded_capacity(4, 9, C::Direct);
        ch.prepare_latency_retention(4, 2, 3).unwrap();
        check!(ch.retained_before_frames() == 2);
        check!(ch.retained_after_frames() == 3);

        check!(cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]) == vec![0.0; 4]);
        check!(cycle(&mut ch, L::Stopped, 2, 0, 4, &[5.0, 6.0]) == vec![0.0; 2]);
        check!(ch.is_finalizing_latency_postroll());
        check!(cycle(&mut ch, L::Stopped, 2, 0, 4, &[7.0, 8.0]) == vec![0.0; 2]);
        check!(!ch.is_finalizing_latency_postroll());
        check!(ch.data() == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn compensated_record_then_play_maps_postroll_across_callbacks_and_wrap() {
        let mut ch = AudioChannel::with_bounded_capacity(4, 6, C::Direct);
        ch.prepare_latency_retention(4, 0, 2).unwrap();
        ch.set_capture_alignment_frames(2).unwrap();
        cycle(&mut ch, L::Recording, 4, 0, 0, &[0.0; 4]);
        cycle(&mut ch, L::Stopped, 2, 0, 4, &[0.0, 1.0]);
        check!(ch.length() == 6);

        check!(cycle(&mut ch, L::Playing, 2, 0, 4, &[]) == vec![0.0, 0.0]);
        check!(cycle(&mut ch, L::Playing, 2, 2, 4, &[]) == vec![0.0, 1.0]);
        check!(cycle(&mut ch, L::Playing, 2, 0, 4, &[]) == vec![0.0, 0.0]);
        check!(cycle(&mut ch, L::Playing, 2, 2, 4, &[]) == vec![0.0, 1.0]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn snapshot_stays_unsettled_through_latency_postroll_then_publishes_complete_take() {
        use crate::content_snapshot::{ContentSnapshotRuntime, CurrentDataError};
        use std::time::{Duration, Instant};

        let runtime = ContentSnapshotRuntime::new();
        let (writer, _control, reader) = runtime.create_audio_channel(4, 8);
        let mut ch = AudioChannel::with_chunk_size_state_and_snapshots(
            4,
            C::Direct,
            Arc::new(AudioChannelStateMirror::default()),
            Some(writer),
        );
        ch.prepare_latency_retention(4, 0, 3).unwrap();
        ch.set_loop_smoothing_frames(2);

        cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]);
        check!(matches!(
            reader.try_current(),
            Err(CurrentDataError::MutationActive(_))
        ));
        cycle(&mut ch, L::Stopped, 2, 0, 4, &[5.0, 6.0]);
        check!(matches!(
            reader.try_current(),
            Err(CurrentDataError::MutationActive(_))
        ));
        cycle(&mut ch, L::Stopped, 1, 0, 4, &[7.0]);

        let deadline = Instant::now() + Duration::from_secs(2);
        let snapshot = loop {
            if let Ok(snapshot) = reader.try_current() {
                break snapshot;
            }
            assert!(Instant::now() < deadline, "snapshot publication timed out");
            std::thread::sleep(Duration::from_millis(1));
        };
        check!(snapshot.metadata.length == 7);
        check!(snapshot.contiguous() == vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[shoop_wasm_test_support::shoop_test]
    fn bounded_grab_load_publishes_a_complete_new_snapshot_generation() {
        use crate::content_snapshot::ContentSnapshotRuntime;
        use std::time::{Duration, Instant};

        let runtime = ContentSnapshotRuntime::new();
        let (writer, _control, reader) = runtime.create_audio_channel(4, 8);
        let mut ch = AudioChannel::with_chunk_size_state_and_snapshots(
            4,
            C::Direct,
            Arc::new(AudioChannelStateMirror::default()),
            Some(writer),
        );
        ch.load_data(&[9.0]);
        let deadline = Instant::now() + Duration::from_secs(2);
        let previous_revision = loop {
            if let Ok(snapshot) = reader.try_current() {
                break snapshot.revision;
            }
            assert!(
                Instant::now() < deadline,
                "initial snapshot publication timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        };

        ch.begin_bounded_load(4);
        ch.write_bounded_load(0, &[1.0, 2.0, 3.0, 4.0]);
        ch.finish_bounded_load();
        ch.set_start_offset(0);
        ch.set_capture_alignment_frames(1).unwrap();

        let deadline = Instant::now() + Duration::from_secs(2);
        let snapshot = loop {
            if let Ok(snapshot) = reader.try_current() {
                if snapshot.revision > previous_revision {
                    break snapshot;
                }
            }
            assert!(
                Instant::now() < deadline,
                "grab snapshot publication timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        };
        check!(snapshot.metadata.length == 4);
        check!(snapshot.contiguous() == vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn legacy_media_layout_offset_is_independent_of_latency_bounds() {
        let mut ch = channel();
        let media_offset = i32::MAX - 1;
        ch.set_start_offset(media_offset);
        ch.set_capture_alignment_frames(7).unwrap();
        check!(ch.start_offset() == media_offset);
        check!(ch.capture_alignment_frames() == 7);
        assert!(ch
            .set_capture_alignment_frames(shoop_latency::MAX_COMPENSATION_FRAMES as i32 + 1)
            .is_err());
        check!(ch.start_offset() == media_offset);
        check!(ch.capture_alignment_frames() == 7);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn armed_latency_record_and_postroll_allocate_nothing() {
        let mut ch = AudioChannel::with_bounded_capacity(4, 9, C::Direct);
        ch.prepare_latency_retention(4, 2, 3).unwrap();
        let recording = [1.0, 2.0, 3.0, 4.0];
        let postroll = [5.0, 6.0, 7.0, 8.0];
        let mut output = [0.0; 4];

        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        assert_no_alloc::assert_no_alloc(|| {
            ch.process(L::Recording, L::Unknown, None, None, 4, 0, 0)
                .unwrap();
            ch.finalize_process(&recording, &mut output);
        });
        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        output.fill(0.0);
        assert_no_alloc::assert_no_alloc(|| {
            ch.process(L::Stopped, L::Unknown, None, None, 4, 0, 4)
                .unwrap();
            ch.finalize_process(&postroll, &mut output);
        });
        check!(ch.length() == 7);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn bounded_recording_exhaustion_is_visible_and_does_not_grow_storage() {
        let mut channel = AudioChannel::with_bounded_capacity(4, 8, C::Direct);
        channel.set_recording_buffer_size(9);
        channel.set_playback_buffer_size(9);
        let result = channel.process(L::Recording, L::Unknown, None, None, 9, 0, 0);
        assert_eq!(result, Err(ChannelError::StorageExhausted { capacity: 8 }));
        assert_eq!(channel.length(), 0);
        assert_eq!(channel.storage_exhaustions(), 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn record_beyond_input_buffer_errors() {
        let mut ch = channel();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(8);
        let r = ch.process(L::Recording, L::Unknown, None, None, 8, 0, 0);
        assert2::assert!(let
            Err(ChannelError::RecordOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_beyond_output_buffer_errors() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        ch.set_recording_buffer_size(8);
        ch.set_playback_buffer_size(2);
        let r = ch.process(L::Playing, L::Unknown, None, None, 8, 0, 2);
        assert2::assert!(let
            Err(ChannelError::PlaybackOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn without_buffers_nothing_is_attempted() {
        let mut ch = channel();
        ch.clear_buffers();
        // No port buffers assigned: record/replace/playback are all masked off.
        assert2::assert!(let Ok(()) = ch.process(L::Recording, L::Unknown, None, None, 4, 0, 0));
        check!(ch.length() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_channel_does_nothing() {
        let mut ch = AudioChannel::with_chunk_size(4, C::Disabled);
        cycle(&mut ch, L::Recording, 4, 0, 0, &[1.0, 2.0, 3.0, 4.0]);
        check!(ch.length() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_record_buffers_carry_over_into_record() {
        let mut ch = channel();
        // Recording is one trigger away, so this cycle pre-records.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        assert2::assert!(let Ok(()) = ch.process(L::Stopped, L::Recording, Some(0), Some(2), 2, 0, 0));
        ch.finalize_process(&[5.0, 6.0], &mut [0.0, 0.0]);
        check!(ch.length() == 0); // main storage untouched so far

        // Now recording proper begins: the pre-recorded samples become content
        // and the start offset marks where "sample 0" really is.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        assert2::assert!(let Ok(()) = ch.process(L::Recording, L::Unknown, None, None, 2, 0, 0));
        ch.finalize_process(&[7.0, 8.0], &mut [0.0, 0.0]);
        check!(ch.start_offset() == 2);
        check!(ch.data() == vec![5.0, 6.0, 7.0, 8.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn negative_take_alignment_selects_retained_prerecord_material() {
        let mut ch = channel();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        ch.process(L::Stopped, L::Recording, Some(0), Some(2), 2, 0, 0)
            .unwrap();
        ch.finalize_process(&[1.0, 0.0], &mut [0.0; 2]);

        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        ch.process(L::Recording, L::Unknown, None, None, 4, 0, 0)
            .unwrap();
        ch.finalize_process(&[0.0; 4], &mut [0.0; 4]);
        check!(ch.start_offset() == 2);
        ch.set_capture_alignment_frames(-2).unwrap();
        check!(cycle(&mut ch, L::Playing, 4, 0, 4, &[]) == vec![1.0, 0.0, 0.0, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_record_discarded_when_recording_does_not_follow() {
        let mut ch = channel();
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        assert2::assert!(let Ok(()) = ch.process(L::Stopped, L::Recording, Some(0), Some(2), 2, 0, 0));
        ch.finalize_process(&[5.0, 6.0], &mut [0.0, 0.0]);

        // Pre-record ends without entering Recording: buffers are dropped.
        ch.set_recording_buffer_size(2);
        ch.set_playback_buffer_size(2);
        assert2::assert!(let Ok(()) = ch.process(L::Stopped, L::Unknown, None, None, 2, 0, 0));
        ch.finalize_process(&[7.0, 8.0], &mut [0.0, 0.0]);
        check!(ch.length() == 0);
        check!(ch.start_offset() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_channel_has_no_poi() {
        let mut ch = AudioChannel::with_chunk_size(4, C::Disabled);
        ch.set_playback_buffer_size(8);
        check!(ch.next_poi(L::Playing, L::Unknown, None, None, 0) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn clear_sets_length_and_resets_offset() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0, 3.0]);
        ch.set_start_offset(2);
        ch.clear(8);
        check!(ch.length() == 8);
        check!(ch.start_offset() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stopped_channel_reports_no_played_back_sample() {
        let mut ch = channel();
        ch.load_data(&[1.0, 2.0]);
        cycle(&mut ch, L::Playing, 2, 0, 2, &[]);
        check!(ch.played_back_sample() == Some(0));
        cycle(&mut ch, L::Stopped, 2, 0, 2, &[]);
        check!(ch.played_back_sample() == None);
    }

    fn playback_subblocks(ch: &mut AudioChannel, blocks: &[(usize, i32)]) -> Vec<f32> {
        let total = blocks.iter().map(|(len, _)| *len).sum();
        ch.set_recording_buffer_size(total);
        ch.set_playback_buffer_size(total);
        for (len, position) in blocks {
            assert2::assert!(let Ok(()) = ch.process(
                L::Playing,
                L::Unknown,
                None,
                None,
                *len,
                *position,
                ch.length(),
            ));
        }
        let mut output = vec![0.0; total];
        ch.finalize_process(&vec![0.0; total], &mut output);
        output
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_fades_from_silence_at_start() {
        let mut ch = channel();
        ch.load_data(&[1.0; 8]);
        ch.set_loop_smoothing_frames(4);
        let output = cycle(&mut ch, L::Playing, 5, 0, 8, &[]);
        check!(output == vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_emits_a_bounded_stop_tail_without_reading_content() {
        let mut ch = channel();
        ch.load_data(&[1.0; 8]);
        ch.set_loop_smoothing_frames(4);
        cycle(&mut ch, L::Playing, 8, 0, 8, &[]);
        let output = cycle(&mut ch, L::Stopped, 6, 0, 8, &[]);
        check!(output == vec![1.0, 0.75, 0.5, 0.25, 0.0, 0.0]);
        check!(ch.played_back_sample() == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_handles_wraps_within_and_between_callbacks() {
        let mut boundary = channel();
        boundary.load_data(&[-1.0, -1.0, 1.0, 1.0]);
        playback_subblocks(&mut boundary, &[(2, 2)]);
        boundary.set_loop_smoothing_frames(4);
        let boundary_wrap = playback_subblocks(&mut boundary, &[(2, 0)]);
        check!(boundary_wrap == vec![1.0, 0.5]);

        let mut within = channel();
        within.load_data(&[-1.0, -1.0, 1.0, 1.0]);
        playback_subblocks(&mut within, &[(2, 0)]);
        within.set_loop_smoothing_frames(4);
        let within_callback_wrap = playback_subblocks(&mut within, &[(2, 2), (2, 0)]);
        check!(within_callback_wrap == vec![1.0, 1.0, 1.0, 0.5]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_handles_seeks_and_gain_changes() {
        let mut ch = channel();
        ch.load_data(&[-1.0, -1.0, 1.0, 1.0]);
        playback_subblocks(&mut ch, &[(2, 2)]);
        ch.set_loop_smoothing_frames(2);

        let seek = playback_subblocks(&mut ch, &[(2, 0)]);
        check!(seek == vec![1.0, 0.0]);
        ch.set_gain(0.5);
        let gain_change = playback_subblocks(&mut ch, &[(2, 2)]);
        check!(gain_change == vec![0.0, 0.25]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_restarts_continuously_for_short_repeated_loops() {
        let mut ch = channel();
        ch.load_data(&[1.0, -1.0]);
        ch.set_loop_smoothing_frames(4);
        let output = playback_subblocks(&mut ch, &[(1, 0), (1, 0), (1, 1), (1, 0)]);
        check!(output == vec![0.0, 0.0, -1.75, -1.75]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_continues_into_silence_after_content_ends() {
        let mut ch = channel();
        ch.load_data(&[1.0; 4]);
        ch.set_loop_smoothing_frames(2);
        let output = cycle(&mut ch, L::Playing, 7, 0, 8, &[]);
        check!(output == vec![0.0, 0.5, 1.0, 1.0, 1.0, 0.5, 0.0]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_changes_only_the_channel_contribution() {
        let mut ch = channel();
        ch.load_data(&[1.0; 4]);
        ch.set_loop_smoothing_frames(2);
        ch.set_recording_buffer_size(4);
        ch.set_playback_buffer_size(4);
        assert2::assert!(let Ok(()) = ch.process(L::Playing, L::Unknown, None, None, 4, 0, 4));
        let mut output = vec![10.0; 4];
        ch.finalize_process(&[0.0; 4], &mut output);
        check!(output == vec![10.0, 10.5, 11.0, 11.0]);
        check!(ch.output_peak() == 11.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_does_not_mark_chunk_or_callback_continuity_as_an_edge() {
        let mut ch = channel();
        ch.load_data(&[1.0; 12]);
        ch.set_loop_smoothing_frames(4);
        let across_chunk = cycle(&mut ch, L::Playing, 8, 0, 12, &[]);
        check!(across_chunk == vec![0.0, 0.25, 0.5, 0.75, 1.0, 1.0, 1.0, 1.0]);
        let across_callback = cycle(&mut ch, L::Playing, 4, 8, 12, &[]);
        check!(across_callback == vec![1.0; 4]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn smoothing_duration_changes_apply_to_later_edges() {
        let mut ch = channel();
        ch.load_data(&[1.0, 1.0, 1.0, 1.0, 1.0, -1.0, -1.0]);
        ch.set_loop_smoothing_frames(4);
        cycle(&mut ch, L::Playing, 4, 0, 7, &[]);
        ch.set_loop_smoothing_frames(2);
        let output = cycle(&mut ch, L::Playing, 2, 5, 7, &[]);
        check!(output == vec![0.75, -0.125]);
        check!(ch.loop_smoothing_frames() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn zero_duration_is_exact_additive_copy_and_primes_continuity() {
        let mut ch = channel();
        ch.load_data(&[1.0, -2.0, 3.0, -4.0]);
        ch.set_gain(0.5);
        let output = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(output == vec![0.5, -1.0, 1.5, -2.0]);

        ch.set_loop_smoothing_frames(3);
        ch.load_data(&[-4.0, -4.0, -4.0]);
        let discontinuity = cycle(&mut ch, L::Playing, 3, 0, 3, &[]);
        check!(discontinuity[0] == -2.0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabling_smoothing_allows_the_current_correction_to_finish() {
        let mut ch = channel();
        ch.load_data(&[1.0; 8]);
        ch.set_loop_smoothing_frames(4);
        let first = cycle(&mut ch, L::Playing, 2, 0, 8, &[]);
        check!(first == vec![0.0, 0.25]);
        ch.set_loop_smoothing_frames(0);
        let rest = cycle(&mut ch, L::Playing, 3, 2, 8, &[]);
        check!(rest == vec![0.5, 0.75, 1.0]);
    }
}
