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
use crate::latency_runtime::{LatchedLatencyRecipe, RuntimeLatencyRecipe};
use crate::loop_mode::LoopMode;
use crate::state_mirror::AudioChannelStateMirror;
use shoop_latency::{LatencyDomainError, MAX_COMPENSATION_FRAMES};

/// At most two copy commands (record and playback) per session sub-block.
/// The session processes no more than 16 sub-blocks in one callback.
const COPY_COMMAND_CAPACITY: usize = 32;

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
    pending_latency_recipe: Option<RuntimeLatencyRecipe>,
    latched_latency_recipe: Option<LatchedLatencyRecipe>,
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
            pending_latency_recipe: None,
            latched_latency_recipe: None,
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
    pub fn pending_latency_recipe(&self) -> Option<RuntimeLatencyRecipe> {
        self.pending_latency_recipe
    }
    pub fn latched_latency_recipe(&self) -> Option<LatchedLatencyRecipe> {
        self.latched_latency_recipe
    }
    pub fn set_pending_latency_recipe(&mut self, recipe: Option<RuntimeLatencyRecipe>) {
        self.pending_latency_recipe = recipe;
        self.state.publish_current_latency_recipe(recipe);
    }
    pub fn set_latched_latency_recipe(&mut self, recipe: LatchedLatencyRecipe) {
        self.latched_latency_recipe = Some(recipe);
        self.state.publish_latched_latency_recipe(Some(recipe));
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
        Ok(())
    }
    pub fn render_advance_frames(&self) -> u32 {
        self.render_advance_frames
    }
    pub fn set_render_advance_frames(&mut self, frames: u32) -> Result<(), LatencyDomainError> {
        if frames > MAX_COMPENSATION_FRAMES {
            return Err(LatencyDomainError::ValueExceedsMaximum(frames));
        }
        self.render_advance_frames = frames;
        Ok(())
    }
    pub fn raw_position_for_logical(&self, logical_position: i32) -> Option<i32> {
        logical_position
            .checked_add(self.start_offset)?
            .checked_add(self.capture_alignment_frames)
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
        self.data_changed();
    }

    pub(crate) fn commit_prepared_data(&mut self, prepared: &mut PreparedAudioChannelData) {
        std::mem::swap(&mut self.buffers, &mut prepared.buffers);
        std::mem::swap(&mut self.data_length, &mut prepared.length);
        self.start_offset = 0;
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
        self.start_offset = 0;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Clearing);
            snapshots.publish_range(0, &[], length, true);
            snapshots.finish_mutation(false);
        }
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

        let previous_mutation = snapshot_mutation(self.prev_process_flags);
        let current_mutation = snapshot_mutation(flags);
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

        if flags.contains(ProcessFlags::PLAYBACK) {
            self.last_played_back_sample = Some(params.position);
            let raw_position = params
                .position
                .checked_add(self.capture_alignment_frames)
                .ok_or(ChannelError::LatencyPositionOverflow)?;
            let dispatch_position = raw_position
                .checked_add(self.render_advance_frames as i32)
                .ok_or(ChannelError::LatencyPositionOverflow)?;
            self.state.publish_playback_positions(
                params.position.checked_sub(self.start_offset),
                Some(raw_position),
                Some(dispatch_position),
            );
            self.process_playback(dispatch_position, n_samples)?;
        } else {
            self.last_played_back_sample = None;
            self.state.publish_playback_positions(None, None, None);
        }
        if flags.contains(ProcessFlags::RECORD) {
            let from = (length_before as i64 + self.start_offset as i64).max(0) as usize;
            self.process_record(n_samples, from, false)?;
        }
        if flags.contains(ProcessFlags::REPLACE) {
            let raw_position = params
                .position
                .checked_add(self.capture_alignment_frames)
                .ok_or(ChannelError::LatencyPositionOverflow)?;
            self.process_replace(raw_position, n_samples)?;
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
    use assert2::check;

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
        check!(cycle(&mut ch, L::Playing, 2, 0, 2, &[]) == vec![7.0, 8.0]);
        let published = state.read(ch.data_seq_nr() as u64);
        check!(published.logical_played_position == Some(0));
        check!(published.raw_played_position == Some(5));
        check!(published.dispatch_position == Some(7));

        ch.set_capture_alignment_frames(-2).unwrap();
        ch.set_render_advance_frames(0).unwrap();
        check!(cycle(&mut ch, L::Playing, 2, 0, 2, &[]) == vec![0.0, 1.0]);
        check!(ch
            .set_capture_alignment_frames(-(MAX_COMPENSATION_FRAMES as i32) - 1)
            .is_err());
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
}
