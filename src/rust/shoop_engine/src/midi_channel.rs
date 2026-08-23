//! MIDI channel: records incoming messages into storage and plays them back
//! under its parent loop's control.
//!
//! Unlike the audio channel there is no deferred-copy queue: MIDI is processed
//! immediately, so `finalize` has nothing to do.
//!
//! The subtle part is state restoration. A recording captures the input's
//! controller/note state at the moment recording began, so that starting playback
//! part-way through can first bring the receiver to the state it should have been
//! in. Messages skipped before the first audible one are folded into that state,
//! and the accumulated difference is emitted just ahead of the first message that
//! actually sounds.
//!
//! Port buffers are passed per call rather than held: a safe channel cannot keep
//! because one audio cycle is split across several `process` calls at points of
//! interest.

use crate::channel_mode::{channel_process_params, ChannelMode, ProcessFlags};
use crate::content_snapshot::MidiProcessSnapshotWriter;
use crate::latency_runtime::{LatchedLatencyRecipe, RuntimeLatencyRecipe};
use crate::loop_mode::LoopMode;
use crate::midi_state::{MidiStateTracker, TrackWhat, MAX_DIFF_MESSAGES};
use crate::midi_storage::{Cursor, MidiStorage, MidiStorageElem, TruncateSide};
use crate::state_mirror::MidiChannelStateMirror;
use shoop_latency::{LatencyDomainError, MAX_COMPENSATION_FRAMES};

use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum MidiChannelError {
    #[error("recording {n_samples} frames but only {available} remain in the input buffer")]
    RecordOutOfBounds { n_samples: u32, available: u32 },
    #[error("playing {n_samples} frames but only {available} remain in the output buffer")]
    PlaybackOutOfBounds { n_samples: u32, available: u32 },
    #[error("no recording buffer assigned")]
    NoRecordingBuffer,
    #[error("no playback buffer assigned")]
    NoPlaybackBuffer,
    #[error("MIDI replacement needs {required} event slots but capacity is {capacity}")]
    ReplaceCapacity { required: usize, capacity: usize },
    #[error("invalid MIDI replacement interval or event ordering")]
    InvalidReplacement,
    #[error("latency mapping exceeds the supported signed media position")]
    LatencyPositionOverflow,
    #[error("retained latency margin {frames} exceeds the supported maximum")]
    RetentionExceedsMaximum { frames: u32 },
}

/// How much of the cycle's input buffer has been consumed.
#[derive(Debug, Clone, Copy, Default)]
struct InBuf {
    n_events_processed: usize,
    n_frames_processed: u32,
    n_frames_total: u32,
}

/// How much of the cycle's output buffer has been consumed.
#[derive(Debug, Clone, Copy, Default)]
struct OutBuf {
    n_frames_processed: u32,
    n_frames_total: u32,
}

impl InBuf {
    fn frames_left(&self) -> u32 {
        self.n_frames_total.saturating_sub(self.n_frames_processed)
    }
}
impl OutBuf {
    fn frames_left(&self) -> u32 {
        self.n_frames_total.saturating_sub(self.n_frames_processed)
    }
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

#[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
#[derive(Debug)]
pub struct PreparedMidiChannelData {
    storage: MidiStorage,
    prerecord_storage: MidiStorage,
    length: u32,
    start_state: MidiStateTracker,
    start_state_valid: bool,
}

#[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
impl PreparedMidiChannelData {
    pub fn new(messages: &[MidiStorageElem], length: u32, start_state: Option<&[Vec<u8>]>) -> Self {
        let capacity = messages.len().max(1);
        let mut storage = MidiStorage::with_capacity_elems(capacity);
        for message in messages {
            storage.append(message.time, message.data(), false, None);
        }
        let mut state = MidiStateTracker::new(TrackWhat::ALL);
        for message in start_state.unwrap_or(&[]) {
            state.process(message);
        }
        Self {
            storage,
            prerecord_storage: MidiStorage::with_capacity_elems(capacity),
            length,
            start_state: state,
            start_state_valid: start_state.is_some(),
        }
    }
}

#[derive(Debug)]
pub struct MidiChannel {
    storage: MidiStorage,
    data_length: u32,
    prerecord_storage: MidiStorage,
    prerecord_data_length: u32,
    playback_cursor: Cursor,

    /// State of what has arrived on the input.
    input_state: MidiStateTracker,
    /// State of what has been emitted on the output.
    output_state: MidiStateTracker,
    /// Input state as of the first recorded message. Kept allocated with a
    /// validity flag rather than an `Option`, so arming it on the process path
    /// reuses its buffers instead of cloning a fresh tracker.
    recording_start_state: MidiStateTracker,
    recording_start_valid: bool,
    /// Same, for a recording that has not been committed to yet.
    temp_prerecording_start_state: MidiStateTracker,
    temp_prerecording_valid: bool,
    /// Target state to restore before the first audible playback message.
    pending_playback_state: MidiStateTracker,
    pending_playback_valid: bool,
    /// Loaded contents use the regular playback path and do not need the session's
    /// legacy pre-record fallback.
    loaded_contents: bool,

    mode: ChannelMode,
    /// Raw media layout offset retained for lead-in and MIDI start-state semantics.
    start_offset: i32,
    /// Frozen raw-take to logical-timeline mapping.
    capture_alignment_frames: i32,
    /// Ephemeral early dispatch for current processor rendering.
    render_advance_frames: u32,
    pre_play_samples: u32,
    n_events_triggered: u32,
    data_seq_nr: u32,
    last_played_back_sample: Option<i32>,
    prev_pos_after: u32,
    prev_process_flags: ProcessFlags,

    rec: Option<InBuf>,
    play: Option<OutBuf>,
    /// Reused for state-restoration messages, sized to the worst case so playback
    /// never grows it.
    restore_scratch: Vec<MidiStorageElem>,
    /// Incoming replacement events rebased to loop time without allocating.
    replace_scratch: Vec<MidiStorageElem>,
    state: Arc<MidiChannelStateMirror>,
    content_snapshots: Option<MidiProcessSnapshotWriter>,
    retained_before_frames: u32,
    retained_after_frames: u32,
    postroll_remaining_frames: u32,
    latency_retention_incomplete: bool,
    pending_latency_recipe: Option<RuntimeLatencyRecipe>,
    latched_latency_recipe: Option<LatchedLatencyRecipe>,
}

impl MidiChannel {
    pub fn with_capacity_elems(capacity: usize, mode: ChannelMode) -> Self {
        Self::with_capacity_elems_and_state(
            capacity,
            mode,
            Arc::new(MidiChannelStateMirror::default()),
        )
    }

    pub fn with_capacity_elems_and_state(
        capacity: usize,
        mode: ChannelMode,
        state: Arc<MidiChannelStateMirror>,
    ) -> Self {
        Self::with_capacity_state_and_snapshots(capacity, mode, state, None)
    }

    pub fn with_capacity_state_and_snapshots(
        capacity: usize,
        mode: ChannelMode,
        state: Arc<MidiChannelStateMirror>,
        content_snapshots: Option<MidiProcessSnapshotWriter>,
    ) -> Self {
        let storage = MidiStorage::with_capacity_elems(capacity);
        let playback_cursor = storage.create_cursor();
        let channel = Self {
            storage,
            data_length: 0,
            prerecord_storage: MidiStorage::with_capacity_elems(capacity),
            prerecord_data_length: 0,
            playback_cursor,
            input_state: MidiStateTracker::new(TrackWhat::ALL),
            output_state: MidiStateTracker::new(TrackWhat::ALL),
            recording_start_state: MidiStateTracker::new(TrackWhat::ALL),
            recording_start_valid: false,
            temp_prerecording_start_state: MidiStateTracker::new(TrackWhat::ALL),
            temp_prerecording_valid: false,
            pending_playback_state: MidiStateTracker::new(TrackWhat::ALL),
            pending_playback_valid: false,
            loaded_contents: false,
            mode,
            start_offset: 0,
            capture_alignment_frames: 0,
            render_advance_frames: 0,
            pre_play_samples: 0,
            n_events_triggered: 0,
            data_seq_nr: 0,
            last_played_back_sample: None,
            prev_pos_after: 0,
            prev_process_flags: ProcessFlags::NONE,
            rec: None,
            play: None,
            restore_scratch: Vec::with_capacity(MAX_DIFF_MESSAGES),
            replace_scratch: Vec::with_capacity(capacity),
            state,
            content_snapshots,
            retained_before_frames: 0,
            retained_after_frames: 0,
            postroll_remaining_frames: 0,
            latency_retention_incomplete: false,
            pending_latency_recipe: None,
            latched_latency_recipe: None,
        };
        channel.publish_state();
        channel
    }

    fn publish_state(&self) {
        self.state.publish(
            self.mode,
            self.output_state.n_notes_active(),
            self.data_length,
            self.start_offset,
            self.last_played_back_sample,
            self.pre_play_samples,
            self.data_seq_nr as u64,
        );
    }

    /// Compatibility publication for legacy control-side setters. Realtime recording and
    /// prepared application loads use bounded incremental/prepared operations instead.
    fn publish_snapshot_contents(&mut self, mutation: crate::content_snapshot::ContentMutation) {
        let MidiChannel {
            storage,
            data_length,
            recording_start_state,
            restore_scratch,
            content_snapshots,
            ..
        } = self;
        let Some(snapshots) = content_snapshots.as_mut() else {
            return;
        };
        snapshots.begin_working_generation();
        snapshots.begin_mutation(mutation);
        snapshots.clear(false);
        recording_start_state.state_as_messages_into(restore_scratch);
        snapshots.append_state_events(restore_scratch, *data_length);
        for event in storage.iter() {
            snapshots.append_storage_events(&[*event], *data_length, false);
        }
        snapshots.append_storage_events(&[], *data_length, true);
        snapshots.finish_mutation(false);
    }

    // --- accessors ---

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
        if let Some(total) = recipe.recipe.total_frames {
            match recipe.recipe.operation {
                shoop_latency::LatencyOperationKind::RecordDirect
                | shoop_latency::LatencyOperationKind::RecordDry
                | shoop_latency::LatencyOperationKind::RecordWet => {
                    self.capture_alignment_frames = total as i32;
                    self.latency_retention_incomplete = total > self.retained_after_frames;
                }
                shoop_latency::LatencyOperationKind::DryThroughWet
                | shoop_latency::LatencyOperationKind::RecordDryIntoWet => {
                    self.render_advance_frames = total;
                }
                shoop_latency::LatencyOperationKind::Grab(_)
                | shoop_latency::LatencyOperationKind::Replacement(_) => {}
            }
        }
        self.latched_latency_recipe = Some(recipe);
        self.state
            .publish_latency_retention_incomplete(self.latency_retention_incomplete);
        self.state.publish_latched_latency_recipe(Some(recipe));
    }
    pub fn set_mode(&mut self, mode: ChannelMode) {
        self.mode = mode;
        self.publish_state();
    }
    pub fn length(&self) -> u32 {
        self.data_length
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
    pub fn retained_before_frames(&self) -> u32 {
        self.retained_before_frames
    }
    pub fn retained_after_frames(&self) -> u32 {
        self.retained_after_frames
    }
    pub fn is_finalizing_latency_postroll(&self) -> bool {
        self.postroll_remaining_frames > 0
    }
    pub fn latency_retention_incomplete(&self) -> bool {
        self.latency_retention_incomplete
    }
    pub fn prepare_latency_retention(
        &mut self,
        retained_before_frames: u32,
        retained_after_frames: u32,
    ) -> Result<(), MidiChannelError> {
        for frames in [retained_before_frames, retained_after_frames] {
            if frames > shoop_latency::MAX_RETAINED_MARGIN_FRAMES {
                return Err(MidiChannelError::RetentionExceedsMaximum { frames });
            }
        }
        self.retained_before_frames = retained_before_frames;
        self.retained_after_frames = retained_after_frames;
        self.latency_retention_incomplete = false;
        self.state.publish_latency_retention_incomplete(false);
        Ok(())
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
    pub fn set_pre_play_samples(&mut self, n: u32) {
        self.pre_play_samples = n;
        self.publish_state();
    }
    pub fn data_seq_nr(&self) -> u32 {
        self.data_seq_nr
    }
    pub fn n_events_triggered(&self) -> u32 {
        self.n_events_triggered
    }
    pub fn n_notes_active(&self) -> u32 {
        self.output_state.n_notes_active()
    }
    pub fn played_back_sample(&self) -> Option<i32> {
        self.last_played_back_sample
    }
    pub fn n_events(&self) -> u32 {
        self.storage.n_events()
    }
    /// Recorded messages, oldest first.
    pub fn contents(&self) -> Vec<MidiStorageElem> {
        self.storage.iter().copied().collect()
    }
    pub fn output_state(&self) -> &MidiStateTracker {
        &self.output_state
    }
    pub fn input_state(&self) -> &MidiStateTracker {
        &self.input_state
    }
    pub fn contents_were_loaded(&self) -> bool {
        self.loaded_contents
    }

    /// Messages that reproduce the state captured when recording began.
    pub fn recording_start_state_messages(&self) -> Vec<Vec<u8>> {
        if self.recording_start_valid {
            self.recording_start_state.state_as_messages()
        } else {
            Vec::new()
        }
    }

    fn data_changed(&mut self) {
        self.data_seq_nr = self.data_seq_nr.wrapping_add(1);
        self.publish_state();
    }

    pub fn clear(&mut self) {
        self.storage.clear();
        self.prerecord_storage.clear();
        self.data_length = 0;
        self.prerecord_data_length = 0;
        self.playback_cursor = self.storage.create_cursor();
        self.recording_start_valid = false;
        self.temp_prerecording_valid = false;
        self.pending_playback_valid = false;
        self.loaded_contents = false;
        self.start_offset = 0;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.begin_working_generation();
            snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Clearing);
            snapshots.clear(true);
            snapshots.finish_mutation(false);
        }
        self.data_changed();
    }

    /// Replaces contents. `start_state` is the state to restore when playback
    /// begins part-way in.
    pub fn set_contents(
        &mut self,
        msgs: &[MidiStorageElem],
        length: u32,
        start_state: Option<&[Vec<u8>]>,
    ) {
        // Grow to fit rather than dropping what does not fit. Loading contents is a
        // control-path operation, so allocating here is fine, and the alternative is
        // silently keeping only the last few messages.
        //
        // Both storages grow together: the process path adopts pre-recorded material
        // with `copy_into`, which resizes its destination, so a capacity mismatch
        // would move that allocation onto the audio thread.
        if msgs.len() > self.storage.capacity_elems() {
            self.storage = MidiStorage::with_capacity_elems(msgs.len());
            self.prerecord_storage = MidiStorage::with_capacity_elems(msgs.len());
        } else {
            self.storage.clear();
        }
        for m in msgs {
            self.storage.append(m.time, m.data(), false, None);
        }
        self.data_length = length;
        self.playback_cursor = self.storage.create_cursor();
        self.recording_start_state.clear();
        self.recording_start_valid = start_state.is_some();
        self.loaded_contents = true;
        for m in start_state.unwrap_or(&[]) {
            self.recording_start_state.process(m);
        }
        self.publish_snapshot_contents(crate::content_snapshot::ContentMutation::Loading);
        self.data_changed();
    }

    #[cfg(any(feature = "app_backend", feature = "native_audio_backend"))]
    pub(crate) fn commit_prepared_data_and_snapshot(
        &mut self,
        prepared: &mut PreparedMidiChannelData,
        snapshot: crate::content_snapshot::PreparedMidiSnapshot,
    ) {
        std::mem::swap(&mut self.storage, &mut prepared.storage);
        std::mem::swap(&mut self.prerecord_storage, &mut prepared.prerecord_storage);
        std::mem::swap(&mut self.data_length, &mut prepared.length);
        std::mem::swap(&mut self.recording_start_state, &mut prepared.start_state);
        self.recording_start_valid = prepared.start_state_valid;
        self.playback_cursor = self.storage.create_cursor();
        self.loaded_contents = true;
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.install_prepared(snapshot);
        }
        self.data_changed();
    }

    pub fn set_length(&mut self, length: u32) {
        let old_length = self.data_length;
        let mut len = self.data_length;
        Self::set_length_impl(&mut self.storage, &mut len, length);
        let changed = len != self.data_length;
        self.data_length = len;
        if changed {
            if let Some(snapshots) = self.content_snapshots.as_mut() {
                snapshots.begin_working_generation();
                snapshots.begin_mutation(crate::content_snapshot::ContentMutation::Loading);
                if length < old_length {
                    snapshots.truncate_after(length as i32, length, true);
                } else {
                    snapshots.append_storage_events(&[], length, true);
                }
                snapshots.finish_mutation(false);
            }
            self.data_changed();
        }
    }

    /// Truncating the head discards anything recorded past the new length.
    fn set_length_impl(storage: &mut MidiStorage, current: &mut u32, length: u32) {
        if *current != length {
            storage.truncate(length, TruncateSide::Head, None);
            *current = length;
        }
    }

    pub fn reset_state_tracking(&mut self) {
        self.input_state.clear();
        self.output_state.clear();
        self.pending_playback_valid = false;
        self.publish_state();
    }

    // --- per-cycle buffers ---

    pub fn set_recording_buffer(&mut self, n_frames: u32) {
        self.rec = Some(InBuf {
            n_events_processed: 0,
            n_frames_processed: 0,
            n_frames_total: n_frames,
        });
    }
    pub fn set_playback_buffer(&mut self, n_frames: u32) {
        self.play = Some(OutBuf {
            n_frames_processed: 0,
            n_frames_total: n_frames,
        });
    }
    pub fn clear_buffers(&mut self) {
        self.rec = None;
        self.play = None;
    }

    /// considered here.
    pub fn next_poi(
        &self,
        mode: LoopMode,
        next_mode: LoopMode,
        next_mode_delay_cycles: Option<u32>,
        next_mode_eta: Option<u32>,
        position: i32,
    ) -> Option<u32> {
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
        let mut poi: Option<u32> = None;
        let mut merge = |v: u32| poi = Some(poi.map_or(v, |p: u32| p.min(v)));
        // constructs its buffer state up front with zero frames and only the
        // pointer absent, so a channel asked to record before it has been given a
        // buffer reports a point of interest of 0 -- which surfaces the
        // misconfiguration instead of quietly processing a full cycle into
        // nowhere.
        if params.flags.contains(ProcessFlags::PLAYBACK) {
            merge(self.play.map_or(0, |b| b.frames_left()));
        }
        if params
            .flags
            .contains(ProcessFlags::RECORD.with(ProcessFlags::PRE_RECORD))
        {
            merge(self.rec.map_or(0, |b| b.frames_left()));
        }
        poi
    }

    // --- processing ---

    #[allow(clippy::too_many_arguments)]
    pub fn process(
        &mut self,
        mode: LoopMode,
        next_mode: LoopMode,
        next_mode_delay_cycles: Option<u32>,
        next_mode_eta: Option<u32>,
        n_samples: u32,
        pos_before: i32,
        pos_after: u32,
        length_before: u32,
        input: &[MidiStorageElem],
        out: &mut Vec<MidiStorageElem>,
    ) -> Result<(), MidiChannelError> {
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
        if self.rec.is_none() {
            flags = ProcessFlags(
                flags.0
                    & !(ProcessFlags::PRE_RECORD.0
                        | ProcessFlags::RECORD.0
                        | ProcessFlags::REPLACE.0),
            );
        }
        if self.play.is_none() {
            flags = ProcessFlags(flags.0 & !ProcessFlags::PLAYBACK.0);
        }

        if self.prev_process_flags.contains(ProcessFlags::RECORD)
            && !flags.contains(ProcessFlags::RECORD)
            && self.postroll_remaining_frames == 0
        {
            self.postroll_remaining_frames = self.retained_after_frames;
        }
        let postroll_samples = if !flags.contains(ProcessFlags::RECORD) && self.rec.is_some() {
            n_samples.min(self.postroll_remaining_frames)
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
                    if previous == crate::content_snapshot::ContentMutation::PreRecording
                        && current_mutation
                            != Some(crate::content_snapshot::ContentMutation::Recording)
                    {
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
                    let carries_prerecord = previous_mutation
                        == Some(crate::content_snapshot::ContentMutation::PreRecording)
                        && mutation == crate::content_snapshot::ContentMutation::Recording;
                    if !carries_prerecord {
                        snapshots.begin_working_generation();
                    }
                    snapshots.begin_mutation(mutation);
                }
            }
        }

        // Anything other than plain forward playback can leave played notes hanging.
        let interrupted = self.prev_process_flags.contains(ProcessFlags::PLAYBACK)
            && (!flags.contains(ProcessFlags::PLAYBACK)
                || pos_before != self.prev_pos_after as i32);
        if interrupted && n_samples > 0 {
            let time = self.play.map(|p| p.n_frames_processed).unwrap_or(0);
            self.stop_active_playback_notes(out, time);
        }

        let mut adopted_prerecording = false;
        if !flags.contains(ProcessFlags::PRE_RECORD)
            && self.prev_process_flags.contains(ProcessFlags::PRE_RECORD)
        {
            if flags.contains(ProcessFlags::RECORD) {
                // Adopt the pre-recorded material, along with its cursor and the
                // state captured when it started.
                // Both storages were built with the same capacity, so copying
                // reuses the destination's buffers rather than allocating.
                let MidiChannel {
                    storage,
                    prerecord_storage,
                    ..
                } = self;
                prerecord_storage.copy_into(storage);
                self.playback_cursor = self.storage.create_cursor();
                self.data_length = self.prerecord_data_length;
                self.start_offset = self.prerecord_data_length as i32;
                let MidiChannel {
                    recording_start_state,
                    temp_prerecording_start_state,
                    ..
                } = self;
                recording_start_state.copy_relevant_state(temp_prerecording_start_state);
                self.recording_start_valid = self.temp_prerecording_valid;
                adopted_prerecording = true;
            }
            self.prerecord_storage.clear();
            self.prerecord_data_length = 0;
            self.temp_prerecording_valid = false;
        }

        let mut processed_input = false;

        if flags.contains(ProcessFlags::PLAYBACK) {
            let raw_position = params
                .position
                .checked_add(self.capture_alignment_frames)
                .ok_or(MidiChannelError::LatencyPositionOverflow)?;
            let dispatch_position = raw_position
                .checked_add(self.render_advance_frames as i32)
                .ok_or(MidiChannelError::LatencyPositionOverflow)?;
            self.state.publish_playback_positions(
                params.position.checked_sub(self.start_offset),
                Some(raw_position),
                Some(dispatch_position),
            );
            let restarting = !self.prev_process_flags.contains(ProcessFlags::PLAYBACK)
                || self
                    .last_played_back_sample
                    .is_some_and(|last| last > dispatch_position);
            if restarting {
                self.playback_cursor.reset(&self.storage);
                let MidiChannel {
                    pending_playback_state,
                    recording_start_state,
                    ..
                } = self;
                pending_playback_state.copy_relevant_state(recording_start_state);
                self.pending_playback_valid = self.recording_start_valid;
            }
            self.process_playback(dispatch_position, n_samples, false, out)?;
        } else {
            self.last_played_back_sample = None;
            self.state.publish_playback_positions(None, None, None);
        }

        if flags.contains(ProcessFlags::RECORD) {
            self.loaded_contents = false;
            let from = (length_before as i64 + self.start_offset as i64).max(0) as u32;
            self.process_record(false, from, n_samples, input)?;
            processed_input = true;
        } else if flags.contains(ProcessFlags::REPLACE) {
            self.loaded_contents = false;
            let raw_position = params
                .position
                .checked_add(self.capture_alignment_frames)
                .ok_or(MidiChannelError::LatencyPositionOverflow)?;
            self.process_replace(raw_position, n_samples, length_before, input)?;
            processed_input = true;
        } else if flags.contains(ProcessFlags::PRE_RECORD) {
            self.loaded_contents = false;
            let from = self.prerecord_data_length;
            self.process_record(true, from, n_samples, input)?;
            processed_input = true;
        }
        if postroll_samples > 0 {
            self.process_record(false, self.data_length, postroll_samples, input)?;
            self.postroll_remaining_frames = self
                .postroll_remaining_frames
                .saturating_sub(postroll_samples);
            processed_input = true;
            if self.postroll_remaining_frames == 0 {
                if let Some(snapshots) = self.content_snapshots.as_mut() {
                    snapshots.finish_mutation(false);
                }
            }
        }

        self.prev_pos_after = pos_after;
        self.prev_process_flags = if self.postroll_remaining_frames > 0 {
            flags.with(ProcessFlags::RECORD)
        } else {
            flags
        };

        if adopted_prerecording {
            self.data_changed();
        }
        if !processed_input {
            self.process_input_messages(n_samples, input);
        }

        if let Some(r) = self.rec.as_mut() {
            r.n_frames_processed += n_samples;
        }
        if let Some(p) = self.play.as_mut() {
            p.n_frames_processed += n_samples;
        }
        self.publish_state();
        Ok(())
    }

    /// MIDI is handled immediately, so there is nothing to finalize. Present for
    /// symmetry with the audio channel.
    pub fn finalize_process(&mut self) {}

    /// Keeps the input state current even when not recording.
    fn process_input_messages(&mut self, n_samples: u32, input: &[MidiStorageElem]) {
        let Some(mut rec) = self.rec else { return };
        let n = rec.frames_left().min(n_samples);
        if n == 0 {
            return;
        }
        let end = rec.n_frames_processed + n;
        let mut idx = rec.n_events_processed;
        while idx < input.len() {
            let e = input[idx];
            if e.time >= end {
                break;
            }
            self.input_state.process(e.data());
            idx += 1;
        }
        rec.n_events_processed = idx;
        self.rec = Some(rec);
    }

    fn process_replace(
        &mut self,
        position: i32,
        n_samples: u32,
        loop_length: u32,
        input: &[MidiStorageElem],
    ) -> Result<(), MidiChannelError> {
        let Some(mut rec) = self.rec else {
            return Err(MidiChannelError::NoRecordingBuffer);
        };
        if rec.frames_left() < n_samples {
            return Err(MidiChannelError::RecordOutOfBounds {
                n_samples,
                available: rec.frames_left(),
            });
        }

        let record_end = rec.n_frames_processed + n_samples;
        let skip = if position < 0 { (-position) as u32 } else { 0 }.min(n_samples);
        let start = position.max(0) as u32;
        let end = start.saturating_add(n_samples - skip).min(loop_length);
        self.data_length = self.data_length.max(loop_length);

        if start == 0 && start < end {
            self.recording_start_state
                .copy_relevant_state(&self.input_state);
            self.recording_start_valid = true;
        }

        self.replace_scratch.clear();
        let mut idx = rec.n_events_processed;
        while idx < input.len() {
            let event = input[idx];
            if event.time >= record_end {
                break;
            }
            if event.time >= rec.n_frames_processed {
                let relative = event.time - rec.n_frames_processed;
                let at = position + relative as i32;
                if at >= start as i32 && at < end as i32 {
                    if self.replace_scratch.len() == self.replace_scratch.capacity() {
                        return Err(MidiChannelError::ReplaceCapacity {
                            required: self.replace_scratch.len() + 1,
                            capacity: self.replace_scratch.capacity(),
                        });
                    }
                    self.replace_scratch.push(event.at_time(at as u32));
                }
                self.input_state.process(event.data());
            }
            idx += 1;
        }
        rec.n_events_processed = idx;
        self.rec = Some(rec);

        if start >= end {
            return Ok(());
        }
        self.storage
            .replace_range(start, end, &self.replace_scratch)
            .map_err(|error| match error {
                crate::midi_storage::ReplaceRangeError::OutOfCapacity { required, capacity } => {
                    MidiChannelError::ReplaceCapacity { required, capacity }
                }
                crate::midi_storage::ReplaceRangeError::InvalidRange
                | crate::midi_storage::ReplaceRangeError::OutOfOrder => {
                    MidiChannelError::InvalidReplacement
                }
            })?;
        self.playback_cursor.reset(&self.storage);
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.remove_range(start as i32, end as i32, self.data_length);
            snapshots.append_storage_events(&self.replace_scratch, self.data_length, false);
        }
        self.data_changed();
        Ok(())
    }

    fn process_record(
        &mut self,
        into_prerecord: bool,
        record_from: u32,
        n_samples: u32,
        input: &[MidiStorageElem],
    ) -> Result<(), MidiChannelError> {
        let Some(mut rec) = self.rec else {
            return Err(MidiChannelError::NoRecordingBuffer);
        };
        if rec.frames_left() < n_samples {
            return Err(MidiChannelError::RecordOutOfBounds {
                n_samples,
                available: rec.frames_left(),
            });
        }

        // Recording from a point discards anything already past it.
        {
            let (storage, len) = if into_prerecord {
                (&mut self.prerecord_storage, &mut self.prerecord_data_length)
            } else {
                (&mut self.storage, &mut self.data_length)
            };
            Self::set_length_impl(storage, len, record_from);
        }
        if !into_prerecord {
            if let Some(snapshots) = self.content_snapshots.as_mut() {
                snapshots.truncate_after(record_from as i32, record_from, false);
            }
        }

        let record_end = rec.n_frames_processed + n_samples;
        let mut changed = false;
        let mut idx = rec.n_events_processed;
        while idx < input.len() {
            let e = input[idx];
            if e.time >= record_end {
                break;
            }
            // Messages from before this window belong to an earlier call.
            if e.time >= rec.n_frames_processed {
                let first = if into_prerecord {
                    self.prerecord_storage.n_events() == 0
                } else {
                    self.storage.n_events() == 0
                };
                if first {
                    // Capture the input state so playback can restore it later.
                    let MidiChannel {
                        input_state,
                        temp_prerecording_start_state,
                        recording_start_state,
                        ..
                    } = self;
                    if into_prerecord {
                        temp_prerecording_start_state.copy_relevant_state(input_state);
                    } else {
                        recording_start_state.copy_relevant_state(input_state);
                    }
                    if into_prerecord {
                        self.temp_prerecording_valid = true;
                        let MidiChannel {
                            temp_prerecording_start_state,
                            restore_scratch,
                            ..
                        } = self;
                        temp_prerecording_start_state.state_as_messages_into(restore_scratch);
                        if let Some(snapshots) = self.content_snapshots.as_mut() {
                            snapshots.append_state_events(restore_scratch, record_from);
                        }
                    } else {
                        self.recording_start_valid = true;
                        let MidiChannel {
                            recording_start_state,
                            restore_scratch,
                            ..
                        } = self;
                        recording_start_state.state_as_messages_into(restore_scratch);
                        if let Some(snapshots) = self.content_snapshots.as_mut() {
                            snapshots.append_state_events(restore_scratch, record_from);
                        }
                    }
                }
                let at = record_from + e.time - rec.n_frames_processed;
                let stored = if into_prerecord {
                    self.prerecord_storage.append(at, e.data(), false, None)
                } else {
                    self.storage.append(at, e.data(), false, None)
                };
                if stored {
                    if let Some(snapshots) = self.content_snapshots.as_mut() {
                        if let Some(event) = MidiStorageElem::new(at, e.data()) {
                            snapshots.append_storage_events(&[event], at, false);
                        }
                    }
                }
                changed |= stored;
            }
            self.input_state.process(e.data());
            idx += 1;
        }
        rec.n_events_processed = idx;
        self.rec = Some(rec);

        {
            let (storage, len) = if into_prerecord {
                (&mut self.prerecord_storage, &mut self.prerecord_data_length)
            } else {
                (&mut self.storage, &mut self.data_length)
            };
            let target = *len + n_samples;
            Self::set_length_impl(storage, len, target);
        }
        if let Some(snapshots) = self.content_snapshots.as_mut() {
            snapshots.append_storage_events(
                &[],
                if into_prerecord {
                    self.prerecord_data_length
                } else {
                    self.data_length
                },
                !into_prerecord,
            );
        }
        if changed {
            self.data_changed();
        }
        Ok(())
    }

    fn process_playback(
        &mut self,
        our_pos: i32,
        n_samples: u32,
        muted: bool,
        out: &mut Vec<MidiStorageElem>,
    ) -> Result<(), MidiChannelError> {
        let Some(play) = self.play else {
            return Err(MidiChannelError::NoPlaybackBuffer);
        };
        if play.frames_left() < n_samples {
            return Err(MidiChannelError::PlaybackOutOfBounds {
                n_samples,
                available: play.frames_left(),
            });
        }

        self.playback_cursor.sync(&self.storage);

        // Skip to our position. Skipped messages still count towards the state to
        // restore, so the first audible message sounds in the right context.
        let mut last_skipped: Option<i32> = None;
        {
            let MidiChannel {
                playback_cursor,
                storage,
                pending_playback_state,
                pending_playback_valid,
                ..
            } = self;
            let valid = *pending_playback_valid;
            let mut cb = |e: &MidiStorageElem| {
                if valid {
                    pending_playback_state.process(e.data());
                }
                last_skipped = Some(e.time as i32);
            };
            playback_cursor.find_time_forward(storage, our_pos.max(0) as u32, Some(&mut cb));
        }
        if let Some(t) = last_skipped {
            self.last_played_back_sample = Some(t);
        }

        let valid_from = our_pos.max(
            self.start_offset
                .saturating_add(self.capture_alignment_frames)
                .saturating_sub(self.pre_play_samples as i32),
        );
        let valid_to = our_pos + n_samples as i32;

        while self.playback_cursor.valid() {
            let Some(event) = self.playback_cursor.get(&self.storage).copied() else {
                break;
            };
            let t = event.time as i32;

            // Restore the recorded state just before the first message that sounds.
            if self.pending_playback_valid
                && t >= valid_from
                && !muted
                && our_pos + n_samples as i32 > valid_from
            {
                self.pending_playback_valid = false;
                let time = play.n_frames_processed;
                let mut restore = std::mem::take(&mut self.restore_scratch);
                restore.clear();
                // Undo the drift of the *input*, not of what this channel sent. While
                // recording, this channel sends nothing, but the input still reaches
                // the receiver by passing through the port, so the input's drift since
                // record start is what the receiver actually saw.
                self.input_state
                    .diff_to_into(&self.pending_playback_state, &mut restore);
                for m in restore.iter() {
                    self.send(out, time, m.data());
                }
                self.restore_scratch = restore;
            }

            if t >= valid_to {
                break;
            }
            if t >= valid_from && !muted {
                let buffer_time = (t - our_pos) + play.n_frames_processed as i32;
                // `event` is an owned copy, so its payload can be passed straight
                // through without copying it again.
                self.send(out, buffer_time.max(0) as u32, event.data());
                self.last_played_back_sample = Some(t);
                self.n_events_triggered += 1;
                self.state.record_triggered_event();
            }
            if self.pending_playback_valid {
                self.pending_playback_state.process(event.data());
            }
            self.playback_cursor.next(&self.storage);
        }

        self.last_played_back_sample = Some(our_pos + n_samples as i32 - 1);
        Ok(())
    }

    fn send(&mut self, out: &mut Vec<MidiStorageElem>, time: u32, data: &[u8]) {
        self.output_state.process(data);
        if let Some(e) = MidiStorageElem::new(time, data) {
            out.push(e);
        }
    }

    fn stop_active_playback_notes(&mut self, out: &mut Vec<MidiStorageElem>, time: u32) {
        let mut cleanup = std::mem::take(&mut self.restore_scratch);
        self.output_state.all_notes_off_into(&mut cleanup);
        for message in &cleanup {
            self.send(out, time, message.data());
        }
        self.restore_scratch = cleanup;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::midi;
    use assert2::check;

    use ChannelMode as C;
    use LoopMode as L;

    fn channel() -> MidiChannel {
        MidiChannel::with_capacity_elems(64, C::Direct)
    }

    fn ev(time: u32, data: &[u8]) -> MidiStorageElem {
        MidiStorageElem::new(time, data).unwrap()
    }

    /// One cycle: size buffers, process, return emitted messages.
    #[allow(clippy::too_many_arguments)]
    fn cycle(
        ch: &mut MidiChannel,
        mode: LoopMode,
        n: u32,
        pos: i32,
        length: u32,
        input: &[MidiStorageElem],
    ) -> Vec<MidiStorageElem> {
        ch.set_recording_buffer(n);
        ch.set_playback_buffer(n);
        let mut out = Vec::new();
        assert2::assert!(let
            Ok(()) = ch.process(
                mode,
                L::Unknown,
                None,
                None,
                n,
                pos,
                (pos.max(0) as u32) + n,
                length,
                input,
                &mut out
            )
        );
        ch.finalize_process();
        out
    }

    fn times(msgs: &[MidiStorageElem]) -> Vec<u32> {
        msgs.iter().map(|m| m.time).collect()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn records_incoming_messages_with_relative_times() {
        let mut ch = channel();
        let input = [
            ev(2, &midi::note_on(0, 60, 100)),
            ev(5, &midi::note_off(0, 60, 0)),
        ];
        cycle(&mut ch, L::Recording, 8, 0, 0, &input);
        check!(ch.n_events() == 2);
        check!(times(&ch.contents()) == vec![2, 5]);
        check!(ch.length() == 8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn prepared_midi_latency_postroll_retains_final_events() {
        let mut ch = channel();
        ch.prepare_latency_retention(2, 3).unwrap();
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(1, &midi::note_on(0, 60, 100))],
        );
        cycle(
            &mut ch,
            L::Stopped,
            2,
            0,
            4,
            &[ev(1, &midi::note_on(0, 61, 100))],
        );
        check!(ch.is_finalizing_latency_postroll());
        cycle(
            &mut ch,
            L::Stopped,
            2,
            0,
            4,
            &[ev(0, &midi::note_on(0, 62, 100))],
        );
        check!(!ch.is_finalizing_latency_postroll());
        check!(ch.length() == 7);
        check!(times(&ch.contents()) == vec![1, 5, 6]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recording_across_two_calls_offsets_times() {
        let mut ch = channel();
        ch.set_recording_buffer(8);
        ch.set_playback_buffer(8);
        let input = [
            ev(1, &midi::note_on(0, 60, 1)),
            ev(6, &midi::note_on(0, 61, 1)),
        ];
        let mut out = Vec::new();
        // Two 4-frame halves of one 8-frame cycle.
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Recording,
                L::Unknown,
                None,
                None,
                4,
                0,
                4,
                0,
                &input,
                &mut out
            )
        );
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Recording,
                L::Unknown,
                None,
                None,
                4,
                4,
                8,
                4,
                &input,
                &mut out
            )
        );
        // The second message lands at its absolute recorded position.
        check!(times(&ch.contents()) == vec![1, 6]);
        check!(ch.length() == 8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacing_overwrites_the_processed_interval() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(6, &midi::note_off(0, 60, 0)),
            ],
            8,
            None,
        );

        cycle(
            &mut ch,
            L::Replacing,
            8,
            0,
            8,
            &[
                ev(2, &midi::note_on(0, 64, 100)),
                ev(5, &midi::note_off(0, 64, 0)),
            ],
        );

        check!(times(&ch.contents()) == vec![2, 5]);
        check!(ch.contents()[0].data() == midi::note_on(0, 64, 100).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn partial_replacement_preserves_events_outside_the_interval() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(3, &midi::note_off(0, 60, 0)),
                ev(6, &midi::note_on(0, 61, 100)),
            ],
            8,
            None,
        );

        cycle(
            &mut ch,
            L::Replacing,
            3,
            2,
            8,
            &[ev(1, &midi::cc(0, 7, 42))],
        );

        check!(times(&ch.contents()) == vec![1, 3, 6]);
        check!(ch.contents()[1].data() == midi::cc(0, 7, 42).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_with_no_input_erases_the_interval() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(3, &midi::note_off(0, 60, 0)),
                ev(6, &midi::note_on(0, 61, 100)),
            ],
            8,
            None,
        );

        cycle(&mut ch, L::Replacing, 3, 2, 8, &[]);

        check!(times(&ch.contents()) == vec![1, 6]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_across_split_calls_uses_loop_relative_times() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(3, &midi::note_off(0, 60, 0)),
                ev(5, &midi::note_on(0, 61, 100)),
                ev(7, &midi::note_off(0, 61, 0)),
            ],
            8,
            None,
        );
        let input = [
            ev(1, &midi::note_on(0, 64, 100)),
            ev(6, &midi::note_off(0, 64, 0)),
        ];
        ch.set_recording_buffer(8);
        ch.set_playback_buffer(8);
        let mut out = Vec::new();

        ch.process(
            L::Replacing,
            L::Unknown,
            None,
            None,
            4,
            0,
            4,
            8,
            &input,
            &mut out,
        )
        .unwrap();
        ch.process(
            L::Replacing,
            L::Unknown,
            None,
            None,
            4,
            4,
            8,
            8,
            &input,
            &mut out,
        )
        .unwrap();

        check!(times(&ch.contents()) == vec![1, 6]);
        check!(ch.contents()[0].data() == midi::note_on(0, 64, 100).as_slice());
        check!(ch.contents()[1].data() == midi::note_off(0, 64, 0).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_split_at_loop_wrap_uses_each_side_of_the_boundary() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 100)),
                ev(1, &midi::note_off(0, 60, 0)),
                ev(6, &midi::note_on(0, 61, 100)),
                ev(7, &midi::note_off(0, 61, 0)),
            ],
            8,
            None,
        );
        let input = [
            ev(1, &midi::note_on(0, 64, 100)),
            ev(3, &midi::note_off(0, 64, 0)),
        ];
        ch.set_recording_buffer(4);
        ch.set_playback_buffer(4);
        let mut out = Vec::new();

        ch.process(
            L::Replacing,
            L::Unknown,
            None,
            None,
            2,
            6,
            8,
            8,
            &input,
            &mut out,
        )
        .unwrap();
        ch.process(
            L::Replacing,
            L::Unknown,
            None,
            None,
            2,
            0,
            2,
            8,
            &input,
            &mut out,
        )
        .unwrap();

        check!(times(&ch.contents()) == vec![1, 7]);
        check!(ch.contents()[0].data() == midi::note_off(0, 64, 0).as_slice());
        check!(ch.contents()[1].data() == midi::note_on(0, 64, 100).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_skips_input_before_the_channel_start_offset() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 100)),
                ev(1, &midi::note_off(0, 60, 0)),
                ev(3, &midi::note_on(0, 61, 100)),
            ],
            4,
            None,
        );

        cycle(
            &mut ch,
            L::Replacing,
            4,
            -2,
            4,
            &[
                ev(0, &midi::note_on(0, 63, 100)),
                ev(3, &midi::note_on(0, 64, 100)),
            ],
        );

        check!(times(&ch.contents()) == vec![1, 3]);
        check!(ch.contents()[0].data() == midi::note_on(0, 64, 100).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_at_loop_start_updates_playback_start_state() {
        let mut ch = channel();
        ch.set_contents(&[ev(1, &midi::note_on(0, 60, 100))], 4, None);
        cycle(&mut ch, L::Stopped, 1, 0, 4, &[ev(0, &midi::cc(0, 7, 42))]);

        cycle(
            &mut ch,
            L::Replacing,
            4,
            0,
            4,
            &[
                ev(1, &midi::note_on(0, 64, 100)),
                ev(2, &midi::note_off(0, 64, 0)),
            ],
        );

        check!(ch
            .recording_start_state_messages()
            .contains(&midi::cc(0, 7, 42).to_vec()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn replacement_reports_insufficient_storage_without_partial_mutation() {
        let mut ch = MidiChannel::with_capacity_elems(3, C::Direct);
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 100)),
                ev(2, &midi::note_off(0, 60, 0)),
                ev(6, &midi::note_on(0, 61, 100)),
            ],
            8,
            None,
        );
        ch.set_recording_buffer(3);
        ch.set_playback_buffer(3);
        let mut out = Vec::new();

        let result = ch.process(
            L::Replacing,
            L::Unknown,
            None,
            None,
            3,
            1,
            4,
            8,
            &[
                ev(0, &midi::note_on(0, 64, 100)),
                ev(1, &midi::note_off(0, 64, 0)),
            ],
            &mut out,
        );

        check!(matches!(
            result,
            Err(MidiChannelError::ReplaceCapacity { .. })
        ));
        check!(times(&ch.contents()) == vec![0, 2, 6]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn messages_beyond_the_window_are_deferred() {
        let mut ch = channel();
        let input = [ev(9, &midi::note_on(0, 60, 1))];
        cycle(&mut ch, L::Recording, 4, 0, 0, &input);
        // Event at frame 9 is outside this 4-frame window.
        check!(ch.n_events() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn input_state_is_tracked_even_when_not_recording() {
        let mut ch = channel();
        let input = [ev(1, &midi::cc(0, 7, 42))];
        cycle(&mut ch, L::Stopped, 4, 0, 0, &input);
        check!(ch.n_events() == 0);
        check!(ch.input_state().cc_value(0, 7) == Some(42));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn plays_back_recorded_messages_at_buffer_relative_times() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            8,
            0,
            0,
            // A balanced pair, so the input state ends where it started and
            // playback has no drift to revert.
            &[
                ev(2, &midi::note_on(0, 60, 100)),
                ev(4, &midi::note_off(0, 60, 64)),
            ],
        );

        let out = cycle(&mut ch, L::Playing, 8, 0, 8, &[]);
        check!(times(&out) == vec![2, 4]);
        check!(out[0].data() == midi::note_on(0, 60, 100).as_slice());
        check!(ch.n_events_triggered() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn media_layout_capture_alignment_and_render_advance_map_midi_together() {
        let state = Arc::new(MidiChannelStateMirror::default());
        let mut ch = MidiChannel::with_capacity_elems_and_state(64, C::Direct, Arc::clone(&state));
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 50, 100)),
                ev(7, &midi::note_on(0, 57, 100)),
            ],
            12,
            None,
        );
        ch.set_start_offset(2);
        ch.set_capture_alignment_frames(3).unwrap();
        ch.set_render_advance_frames(2).unwrap();
        check!(ch.raw_position_for_logical(0) == Some(5));
        check!(ch.dispatch_raw_position_for_logical(0) == Some(7));
        let out = cycle(&mut ch, L::Playing, 2, 0, 2, &[]);
        check!(out
            .iter()
            .any(|event| event.time == 0 && event.data()[1] == 57));
        check!(!out.iter().any(|event| event.data()[1] == 50));
        let published = state.read(ch.data_seq_nr() as u64);
        check!(published.logical_played_position == Some(0));
        check!(published.raw_played_position == Some(5));
        check!(published.dispatch_position == Some(7));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_only_emits_messages_inside_the_window() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            8,
            0,
            0,
            &[
                ev(1, &midi::note_on(0, 60, 1)),
                ev(2, &midi::note_off(0, 60, 1)),
                ev(6, &midi::note_on(0, 61, 1)),
                ev(7, &midi::note_off(0, 61, 1)),
            ],
        );
        // Play only the first half.
        let out = cycle(&mut ch, L::Playing, 4, 0, 8, &[]);
        check!(times(&out) == vec![1, 2]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_updates_output_state() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(0, &midi::note_on(0, 60, 100))],
        );
        check!(ch.n_notes_active() == 0);
        cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(ch.n_notes_active() == 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_wrap_stops_only_notes_owned_by_playback() {
        let mut ch = channel();
        ch.set_contents(&[ev(0, &midi::note_on(0, 60, 100))], 4, None);
        let mut mixed_state = MidiStateTracker::new(TrackWhat::ALL);
        mixed_state.process(&midi::note_on(0, 64, 100));
        for message in cycle(&mut ch, L::Playing, 4, 0, 4, &[]) {
            mixed_state.process(message.data());
        }

        let out = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        for message in &out {
            mixed_state.process(message.data());
        }
        check!(out.len() == 2);
        check!(out[0].data() == midi::note_off(0, 60, 0).as_slice());
        check!(out[1].data() == midi::note_on(0, 60, 100).as_slice());
        check!(!out
            .iter()
            .any(|message| midi::all_sound_off_channel(message.data()).is_some()));
        check!(mixed_state.note_velocity(0, 60) == Some(100));
        check!(mixed_state.note_velocity(0, 64) == Some(100));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn interrupting_playback_stops_its_active_notes() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(0, &midi::note_on(0, 60, 100))],
        );
        cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        // Stopping mid-playback must silence anything left sounding.
        let out = cycle(&mut ch, L::Stopped, 4, 0, 4, &[]);
        check!(out.len() == 1);
        check!(out[0].data() == midi::note_off(0, 60, 0).as_slice());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn a_position_jump_also_counts_as_interruption() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            8,
            0,
            0,
            &[ev(0, &midi::note_on(0, 60, 1))],
        );
        cycle(&mut ch, L::Playing, 4, 0, 8, &[]);
        // Continuing from an unexpected position is a jump, not forward playback.
        let out = cycle(&mut ch, L::Playing, 4, 6, 8, &[]);
        check!(out
            .iter()
            .any(|message| message.data() == midi::note_off(0, 60, 0).as_slice()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn uninterrupted_forward_playback_sends_no_cleanup() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            8,
            0,
            0,
            &[
                ev(0, &midi::note_on(0, 60, 1)),
                ev(5, &midi::note_on(0, 61, 1)),
            ],
        );
        cycle(&mut ch, L::Playing, 4, 0, 8, &[]);
        let out = cycle(&mut ch, L::Playing, 4, 4, 8, &[]);
        check!(times(&out) == vec![1]); // frame 5 is buffer-relative frame 1
    }

    #[shoop_wasm_test_support::shoop_test]
    fn input_drift_since_record_start_is_reverted_on_playback() {
        let mut ch = channel();
        // A controller value the recording was made against.
        cycle(&mut ch, L::Stopped, 4, 0, 0, &[ev(0, &midi::cc(0, 7, 99))]);
        check!(ch.input_state().cc_value(0, 7) == Some(99));

        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(2, &midi::note_off(0, 60, 64)),
            ],
        );
        check!(ch
            .recording_start_state_messages()
            .contains(&midi::cc(0, 7, 99).to_vec()));

        // The input moves that controller afterwards, so the receiver is no longer
        // where the recording assumed it was.
        cycle(&mut ch, L::Stopped, 4, 0, 0, &[ev(0, &midi::cc(0, 7, 12))]);
        check!(ch.input_state().cc_value(0, 7) == Some(12));

        // Playback reverts it before the first recorded message sounds.
        let out = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(out[0].time == 0);
        check!(out[0].data() == midi::cc(0, 7, 99).as_slice());
        check!(ch.output_state().cc_value(0, 7) == Some(99));
    }

    #[shoop_wasm_test_support::shoop_test]
    /// State the input never moved needs no restore: in Direct mode the input
    /// already reached the receiver, so re-sending it would be noise.
    fn undrifted_state_is_not_restored_on_playback() {
        let mut ch = channel();
        cycle(&mut ch, L::Stopped, 4, 0, 0, &[ev(0, &midi::cc(0, 7, 99))]);
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[
                ev(1, &midi::note_on(0, 60, 100)),
                ev(2, &midi::note_off(0, 60, 64)),
            ],
        );

        let out = cycle(&mut ch, L::Playing, 4, 0, 4, &[]);
        check!(!out.iter().any(|m| midi::is_cc(m.data())));
        check!(times(&out) == vec![1, 2]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn set_length_truncates_recorded_messages() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            8,
            0,
            0,
            &[
                ev(1, &midi::note_on(0, 60, 1)),
                ev(6, &midi::note_on(0, 61, 1)),
            ],
        );
        check!(ch.n_events() == 2);
        ch.set_length(4);
        check!(ch.length() == 4);
        check!(times(&ch.contents()) == vec![1]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn clear_empties_everything() {
        let mut ch = channel();
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(0, &midi::note_on(0, 60, 1))],
        );
        ch.clear();
        check!(ch.n_events() == 0);
        check!(ch.length() == 0);
        check!(ch.recording_start_state_messages().is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn set_contents_restores_messages_and_start_state() {
        let mut ch = channel();
        let msgs = [ev(1, &midi::note_on(0, 60, 7))];
        ch.set_contents(&msgs, 8, Some(&[midi::cc(0, 7, 5).to_vec()]));
        check!(ch.n_events() == 1);
        check!(ch.length() == 8);

        let out = cycle(&mut ch, L::Playing, 8, 0, 8, &[]);
        check!(out.iter().any(|m| m.data() == midi::cc(0, 7, 5).as_slice()));
        check!(out
            .iter()
            .any(|m| m.data() == midi::note_on(0, 60, 7).as_slice()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn without_buffers_nothing_is_recorded() {
        let mut ch = channel();
        ch.clear_buffers();
        let mut out = Vec::new();
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Recording,
                L::Unknown,
                None,
                None,
                4,
                0,
                4,
                0,
                &[ev(0, &midi::note_on(0, 60, 1))],
                &mut out
            )
        );
        check!(ch.n_events() == 0);
        check!(out.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn recording_past_the_input_buffer_errors() {
        let mut ch = channel();
        ch.set_recording_buffer(2);
        ch.set_playback_buffer(8);
        let mut out = Vec::new();
        let r = ch.process(
            L::Recording,
            L::Unknown,
            None,
            None,
            8,
            0,
            8,
            0,
            &[],
            &mut out,
        );
        assert2::assert!(let
            Err(MidiChannelError::RecordOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playing_past_the_output_buffer_errors() {
        let mut ch = channel();
        ch.set_recording_buffer(8);
        ch.set_playback_buffer(2);
        let mut out = Vec::new();
        let r = ch.process(
            L::Playing,
            L::Unknown,
            None,
            None,
            8,
            0,
            8,
            8,
            &[],
            &mut out,
        );
        assert2::assert!(let
            Err(MidiChannelError::PlaybackOutOfBounds {
                n_samples,
                available
            }) = r
        );
        check!(n_samples == 8);
        check!(available == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn disabled_channel_does_nothing() {
        let mut ch = MidiChannel::with_capacity_elems(64, C::Disabled);
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(0, &midi::note_on(0, 60, 1))],
        );
        check!(ch.n_events() == 0);
        check!(ch.next_poi(L::Playing, L::Unknown, None, None, 0) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn next_poi_is_smallest_remaining_buffer() {
        let mut ch = channel();
        ch.set_playback_buffer(8);
        ch.set_recording_buffer(3);
        check!(ch.next_poi(L::Playing, L::Unknown, None, None, 0) == Some(8));
        check!(ch.next_poi(L::Recording, L::Unknown, None, None, 0) == Some(3));
        check!(ch.next_poi(L::Stopped, L::Unknown, None, None, 0) == None);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_record_carries_over_into_recording() {
        let mut ch = channel();
        // Recording is one trigger away: this cycle pre-records.
        ch.set_recording_buffer(4);
        ch.set_playback_buffer(4);
        let mut out = Vec::new();
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Stopped,
                L::Recording,
                Some(0),
                Some(4),
                4,
                0,
                4,
                0,
                &[ev(1, &midi::note_on(0, 60, 1))],
                &mut out
            )
        );
        // Main storage untouched so far.
        check!(ch.n_events() == 0);

        // Recording proper begins; the pre-recorded message becomes content and
        // the start offset marks where sample 0 really is.
        ch.set_recording_buffer(4);
        ch.set_playback_buffer(4);
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Recording,
                L::Unknown,
                None,
                None,
                4,
                0,
                4,
                0,
                &[ev(0, &midi::note_on(0, 61, 1))],
                &mut out
            )
        );
        // Both the pre-recorded message and the new one are present: the
        // pre-record storage was adopted, not discarded.
        check!(ch.n_events() == 2);
        let recorded: Vec<Vec<u8>> = ch.contents().iter().map(|m| m.data().to_vec()).collect();
        check!(recorded.contains(&midi::note_on(0, 60, 1).to_vec()));
        check!(recorded.contains(&midi::note_on(0, 61, 1).to_vec()));
        check!(ch.start_offset() == 4);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_record_discarded_when_recording_does_not_follow() {
        let mut ch = channel();
        ch.set_recording_buffer(4);
        ch.set_playback_buffer(4);
        let mut out = Vec::new();
        assert2::assert!(let
            Ok(()) = ch.process(
                L::Stopped,
                L::Recording,
                Some(0),
                Some(4),
                4,
                0,
                4,
                0,
                &[ev(1, &midi::note_on(0, 60, 1))],
                &mut out
            )
        );
        // Pre-record ends without entering Recording.
        cycle(&mut ch, L::Stopped, 4, 0, 0, &[]);
        check!(ch.n_events() == 0);
        check!(ch.start_offset() == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn record_position_accounts_for_start_offset() {
        let mut ch = channel();
        // A non-zero start offset makes the storage write position differ from
        // the input buffer's frame counter.
        ch.set_start_offset(10);
        cycle(
            &mut ch,
            L::Recording,
            4,
            0,
            0,
            &[ev(2, &midi::note_on(0, 60, 1))],
        );
        // record_from = length_before(0) + start_offset(10), event at frame 2.
        check!(times(&ch.contents()) == vec![12]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn playback_skips_messages_before_the_start_offset() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 1)),
                ev(5, &midi::note_on(0, 61, 1)),
            ],
            8,
            None,
        );
        // Playback may not sound anything before the start offset.
        ch.set_start_offset(4);
        let out = cycle(&mut ch, L::Playing, 8, 0, 8, &[]);
        let played: Vec<Vec<u8>> = out.iter().map(|m| m.data().to_vec()).collect();
        check!(!played.contains(&midi::note_on(0, 60, 1).to_vec()));
        check!(played.contains(&midi::note_on(0, 61, 1).to_vec()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_playing_before_the_start_offset_stays_silent() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 1)),
                ev(5, &midi::note_on(0, 61, 1)),
            ],
            8,
            None,
        );
        ch.set_start_offset(4);
        // Negative loop position: playback reaches back before the start offset,
        // so the cursor does not skip these messages and the validity window is
        // the only thing preventing them from sounding.
        let out = cycle(&mut ch, L::Playing, 8, -4, 8, &[]);
        let played: Vec<Vec<u8>> = out.iter().map(|m| m.data().to_vec()).collect();
        check!(!played.contains(&midi::note_on(0, 60, 1).to_vec()));
        check!(played.contains(&midi::note_on(0, 61, 1).to_vec()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn pre_play_samples_widen_the_validity_window() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(0, &midi::note_on(0, 60, 1)),
                ev(5, &midi::note_on(0, 61, 1)),
            ],
            8,
            None,
        );
        ch.set_start_offset(4);
        // Allowing 4 pre-play samples opens the window back to 0.
        ch.set_pre_play_samples(4);
        let out = cycle(&mut ch, L::Playing, 8, -4, 8, &[]);
        let played: Vec<Vec<u8>> = out.iter().map(|m| m.data().to_vec()).collect();
        check!(played.contains(&midi::note_on(0, 60, 1).to_vec()));
        check!(played.contains(&midi::note_on(0, 61, 1).to_vec()));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn start_state_is_restored_only_once_per_playback_run() {
        let mut ch = channel();
        ch.set_contents(
            &[
                ev(1, &midi::note_on(0, 60, 1)),
                ev(2, &midi::note_on(0, 61, 1)),
            ],
            8,
            Some(&[midi::cc(0, 7, 55).to_vec()]),
        );
        let out = cycle(&mut ch, L::Playing, 8, 0, 8, &[]);
        // The restore message precedes the first note and is not repeated before
        // the second.
        let n_cc = out.iter().filter(|m| midi::is_cc(m.data())).count();
        check!(n_cc == 1);
        check!(out.iter().filter(|m| midi::is_note_on(m.data())).count() == 2);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn reset_state_tracking_forgets_everything() {
        let mut ch = channel();
        cycle(&mut ch, L::Stopped, 4, 0, 0, &[ev(0, &midi::cc(0, 7, 9))]);
        check!(ch.input_state().cc_value(0, 7) == Some(9));
        ch.reset_state_tracking();
        check!(ch.input_state().cc_value(0, 7) == None);
    }
}
