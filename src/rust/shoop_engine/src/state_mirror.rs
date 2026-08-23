use crate::channel_mode::ChannelMode;
use crate::composite_plan::{LoopIdentity, LoopTargetKind, MAX_COMPOSITE_TARGETS};
use crate::composite_runtime::{
    ActiveCompositeChild, CompositeRuntimeCounters, CompositeRuntimeFault,
};
use crate::latency_runtime::{
    AtomicLatencyObservation, AtomicLatencyRecipePublication, LatchedLatencyRecipe,
    RuntimeLatencyObservation, RuntimeLatencyRecipe,
};
use crate::loop_mode::LoopMode;
use crate::state::{
    AudioChannelState, AudioPortState, LatestMidiMessage, LoopState, MidiChannelState,
    MidiPortState,
};
use std::array;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};

const NO_MODE: i32 = -1;
const NO_DELAY: u64 = u64::MAX;
const NO_SAMPLE: i32 = i32::MIN;

#[derive(Debug)]
pub struct LoopStateMirror {
    mode: AtomicI32,
    length: AtomicU32,
    position: AtomicU32,
    cycle_count: AtomicU64,
    next_mode: AtomicI32,
    next_delay: AtomicU64,
    deferred_latency_mode: AtomicI32,
    current_latency_recipe: AtomicLatencyRecipePublication,
    latched_latency_recipe: AtomicLatencyRecipePublication,
}

impl Default for LoopStateMirror {
    fn default() -> Self {
        Self {
            mode: AtomicI32::new(LoopMode::Stopped as i32),
            length: AtomicU32::new(0),
            position: AtomicU32::new(0),
            cycle_count: AtomicU64::new(0),
            next_mode: AtomicI32::new(NO_MODE),
            next_delay: AtomicU64::new(NO_DELAY),
            deferred_latency_mode: AtomicI32::new(NO_MODE),
            current_latency_recipe: AtomicLatencyRecipePublication::default(),
            latched_latency_recipe: AtomicLatencyRecipePublication::default(),
        }
    }
}

impl LoopStateMirror {
    pub fn publish(
        &self,
        mode: LoopMode,
        length: u32,
        position: u32,
        cycle_count: u64,
        next: Option<(LoopMode, u32)>,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.position.store(position, Ordering::Relaxed);
        self.cycle_count.store(cycle_count, Ordering::Relaxed);
        self.next_mode.store(
            next.map(|(mode, _)| mode as i32).unwrap_or(NO_MODE),
            Ordering::Relaxed,
        );
        self.next_delay.store(
            next.map(|(_, delay)| delay as u64).unwrap_or(NO_DELAY),
            Ordering::Relaxed,
        );
    }

    pub fn set_mode(&self, mode: LoopMode) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.next_mode.store(NO_MODE, Ordering::Relaxed);
        self.next_delay.store(NO_DELAY, Ordering::Relaxed);
    }

    pub fn set_length(&self, length: u32) {
        self.length.store(length, Ordering::Relaxed);
    }

    pub fn set_position(&self, position: u32) {
        self.position.store(position, Ordering::Relaxed);
    }

    pub fn publish_deferred_latency_mode(&self, mode: Option<LoopMode>) {
        self.deferred_latency_mode.store(
            mode.map(|mode| mode as i32).unwrap_or(NO_MODE),
            Ordering::Relaxed,
        );
    }

    pub fn publish_current_latency_recipe(&self, recipe: Option<RuntimeLatencyRecipe>) {
        self.current_latency_recipe.publish_pending(recipe);
    }

    pub fn publish_latched_latency_recipe(&self, recipe: Option<LatchedLatencyRecipe>) {
        self.latched_latency_recipe.publish_latched(recipe);
    }

    pub fn read(&self) -> LoopState {
        let next_mode = self.next_mode.load(Ordering::Relaxed);
        let next_delay = self.next_delay.load(Ordering::Relaxed);
        let deferred_latency_mode = self.deferred_latency_mode.load(Ordering::Relaxed);
        LoopState {
            mode: LoopMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(LoopMode::Unknown),
            length: self.length.load(Ordering::Relaxed),
            position: self.position.load(Ordering::Relaxed),
            cycle_count: self.cycle_count.load(Ordering::Relaxed),
            maybe_next_mode: (next_mode != NO_MODE)
                .then(|| LoopMode::try_from(next_mode).unwrap_or(LoopMode::Unknown)),
            maybe_next_mode_delay: (next_delay != NO_DELAY).then_some(next_delay as u32),
            deferred_latency_mode: (deferred_latency_mode != NO_MODE)
                .then(|| LoopMode::try_from(deferred_latency_mode).unwrap_or(LoopMode::Unknown)),
            current_latency_recipe: self.current_latency_recipe.read(),
            latched_latency_recipe: self.latched_latency_recipe.read(),
        }
    }
}

const EMPTY_IDENTITY: LoopIdentity = LoopIdentity {
    slot: 0,
    generation: 0,
    kind: LoopTargetKind::Basic,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompositeStateMirrorSnapshot {
    pub identity: LoopIdentity,
    pub sync_source: LoopIdentity,
    pub installed: bool,
    pub active_plan_version: u64,
    pub pending_plan_version: Option<u64>,
    pub mode: LoopMode,
    pub next_mode: Option<LoopMode>,
    pub next_mode_delay: Option<u32>,
    pub iteration: u32,
    pub cycle_count: u64,
    pub length: u64,
    pub position: u64,
    pub play_after_record: bool,
    pub runtime_counters: CompositeRuntimeCounters,
    pub runtime_fault: CompositeRuntimeFault,
    active_children: [Option<ActiveCompositeChild>; MAX_COMPOSITE_TARGETS],
    n_active_children: usize,
}

impl CompositeStateMirrorSnapshot {
    pub fn active_children(&self) -> impl Iterator<Item = ActiveCompositeChild> + '_ {
        self.active_children[..self.n_active_children]
            .iter()
            .flatten()
            .copied()
    }
}

#[derive(Debug)]
struct ActiveChildMirror {
    slot: AtomicU32,
    generation: AtomicU32,
    kind: AtomicU32,
    mode: AtomicI32,
    cycle_offset: AtomicU32,
}

impl Default for ActiveChildMirror {
    fn default() -> Self {
        Self {
            slot: AtomicU32::new(0),
            generation: AtomicU32::new(0),
            kind: AtomicU32::new(0),
            mode: AtomicI32::new(LoopMode::Stopped as i32),
            cycle_offset: AtomicU32::new(0),
        }
    }
}

impl ActiveChildMirror {
    fn publish(&self, child: ActiveCompositeChild) {
        self.slot.store(child.identity.slot, Ordering::Relaxed);
        self.generation
            .store(child.identity.generation, Ordering::Relaxed);
        self.kind.store(
            match child.identity.kind {
                LoopTargetKind::Basic => 0,
                LoopTargetKind::Composite => 1,
            },
            Ordering::Relaxed,
        );
        self.mode.store(child.mode as i32, Ordering::Relaxed);
        self.cycle_offset
            .store(child.cycle_offset, Ordering::Relaxed);
    }

    fn read(&self) -> ActiveCompositeChild {
        ActiveCompositeChild {
            identity: LoopIdentity {
                slot: self.slot.load(Ordering::Relaxed),
                generation: self.generation.load(Ordering::Relaxed),
                kind: if self.kind.load(Ordering::Relaxed) == 0 {
                    LoopTargetKind::Basic
                } else {
                    LoopTargetKind::Composite
                },
            },
            mode: LoopMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(LoopMode::Unknown),
            cycle_offset: self.cycle_offset.load(Ordering::Relaxed),
        }
    }
}

/// Lock-free per-composite publication shared by the engine and its control handle.
///
/// The generation is a seqlock: the single engine writer makes it odd while updating and
/// even once the complete state is visible. Readers retry rather than taking a lock that
/// could ever block the realtime callback.
#[derive(Debug)]
pub struct CompositeStateMirror {
    identity: LoopIdentity,
    generation: AtomicU64,
    installed: AtomicBool,
    sync_slot: AtomicU32,
    sync_generation: AtomicU32,
    sync_kind: AtomicU32,
    active_plan_version: AtomicU64,
    pending_plan_version: AtomicU64,
    mode: AtomicI32,
    next_mode: AtomicI32,
    next_delay: AtomicU64,
    iteration: AtomicU32,
    cycle_count: AtomicU64,
    length: AtomicU64,
    position: AtomicU64,
    play_after_record: AtomicBool,
    stale_targets: AtomicU64,
    invalid_seeks: AtomicU64,
    rejected_modes: AtomicU64,
    plan_mismatches: AtomicU64,
    output_overflows: AtomicU64,
    arithmetic_overflows: AtomicU64,
    runtime_fault: AtomicU32,
    active_children: [ActiveChildMirror; MAX_COMPOSITE_TARGETS],
    n_active_children: AtomicUsize,
}

impl CompositeStateMirror {
    pub fn new(identity: LoopIdentity) -> Self {
        Self {
            identity,
            generation: AtomicU64::new(0),
            installed: AtomicBool::new(false),
            sync_slot: AtomicU32::new(0),
            sync_generation: AtomicU32::new(0),
            sync_kind: AtomicU32::new(0),
            active_plan_version: AtomicU64::new(0),
            pending_plan_version: AtomicU64::new(0),
            mode: AtomicI32::new(LoopMode::Stopped as i32),
            next_mode: AtomicI32::new(NO_MODE),
            next_delay: AtomicU64::new(NO_DELAY),
            iteration: AtomicU32::new(0),
            cycle_count: AtomicU64::new(0),
            length: AtomicU64::new(0),
            position: AtomicU64::new(0),
            play_after_record: AtomicBool::new(false),
            stale_targets: AtomicU64::new(0),
            invalid_seeks: AtomicU64::new(0),
            rejected_modes: AtomicU64::new(0),
            plan_mismatches: AtomicU64::new(0),
            output_overflows: AtomicU64::new(0),
            arithmetic_overflows: AtomicU64::new(0),
            runtime_fault: AtomicU32::new(0),
            active_children: array::from_fn(|_| ActiveChildMirror::default()),
            n_active_children: AtomicUsize::new(0),
        }
    }

    pub fn identity(&self) -> LoopIdentity {
        self.identity
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &self,
        sync_source: LoopIdentity,
        active_plan_version: u64,
        pending_plan_version: Option<u64>,
        mode: LoopMode,
        next: Option<(LoopMode, u32)>,
        iteration: u32,
        cycle_count: u64,
        length: u64,
        position: u64,
        play_after_record: bool,
        runtime_counters: CompositeRuntimeCounters,
        runtime_fault: CompositeRuntimeFault,
        active_children: impl Iterator<Item = ActiveCompositeChild>,
    ) {
        self.generation.fetch_add(1, Ordering::Release);
        self.sync_slot.store(sync_source.slot, Ordering::Relaxed);
        self.sync_generation
            .store(sync_source.generation, Ordering::Relaxed);
        self.sync_kind.store(
            match sync_source.kind {
                LoopTargetKind::Basic => 0,
                LoopTargetKind::Composite => 1,
            },
            Ordering::Relaxed,
        );
        self.active_plan_version
            .store(active_plan_version, Ordering::Relaxed);
        self.pending_plan_version
            .store(pending_plan_version.unwrap_or(0), Ordering::Relaxed);
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.next_mode.store(
            next.map(|(next_mode, _)| next_mode as i32)
                .unwrap_or(NO_MODE),
            Ordering::Relaxed,
        );
        self.next_delay.store(
            next.map(|(_, delay)| u64::from(delay)).unwrap_or(NO_DELAY),
            Ordering::Relaxed,
        );
        self.iteration.store(iteration, Ordering::Relaxed);
        self.cycle_count.store(cycle_count, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.position.store(position, Ordering::Relaxed);
        self.play_after_record
            .store(play_after_record, Ordering::Relaxed);
        self.stale_targets
            .store(runtime_counters.stale_targets, Ordering::Relaxed);
        self.invalid_seeks
            .store(runtime_counters.invalid_seeks, Ordering::Relaxed);
        self.rejected_modes
            .store(runtime_counters.rejected_modes, Ordering::Relaxed);
        self.plan_mismatches
            .store(runtime_counters.plan_mismatches, Ordering::Relaxed);
        self.output_overflows
            .store(runtime_counters.output_overflows, Ordering::Relaxed);
        self.arithmetic_overflows
            .store(runtime_counters.arithmetic_overflows, Ordering::Relaxed);
        self.runtime_fault.store(
            match runtime_fault {
                CompositeRuntimeFault::None => 0,
                CompositeRuntimeFault::OutputCapacity => 1,
            },
            Ordering::Relaxed,
        );
        let mut count = 0;
        for child in active_children.take(MAX_COMPOSITE_TARGETS) {
            self.active_children[count].publish(child);
            count += 1;
        }
        self.n_active_children.store(count, Ordering::Relaxed);
        self.installed.store(true, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn mark_uninstalled(&self) {
        self.generation.fetch_add(1, Ordering::Release);
        self.installed.store(false, Ordering::Relaxed);
        self.mode.store(LoopMode::Stopped as i32, Ordering::Relaxed);
        self.next_mode.store(NO_MODE, Ordering::Relaxed);
        self.next_delay.store(NO_DELAY, Ordering::Relaxed);
        self.n_active_children.store(0, Ordering::Relaxed);
        self.generation.fetch_add(1, Ordering::Release);
    }

    pub fn set_play_after_record(&self, enabled: bool) {
        self.play_after_record.store(enabled, Ordering::Relaxed);
    }

    pub fn read(&self) -> CompositeStateMirrorSnapshot {
        loop {
            let before = self.generation.load(Ordering::Acquire);
            if before & 1 != 0 {
                std::hint::spin_loop();
                continue;
            }
            let count = self
                .n_active_children
                .load(Ordering::Relaxed)
                .min(MAX_COMPOSITE_TARGETS);
            let mut active_children = [None; MAX_COMPOSITE_TARGETS];
            for (destination, source) in active_children[..count]
                .iter_mut()
                .zip(&self.active_children[..count])
            {
                *destination = Some(source.read());
            }
            let next_mode = self.next_mode.load(Ordering::Relaxed);
            let next_delay = self.next_delay.load(Ordering::Relaxed);
            let pending_version = self.pending_plan_version.load(Ordering::Relaxed);
            let snapshot = CompositeStateMirrorSnapshot {
                identity: self.identity,
                sync_source: LoopIdentity {
                    slot: self.sync_slot.load(Ordering::Relaxed),
                    generation: self.sync_generation.load(Ordering::Relaxed),
                    kind: if self.sync_kind.load(Ordering::Relaxed) == 0 {
                        LoopTargetKind::Basic
                    } else {
                        LoopTargetKind::Composite
                    },
                },
                installed: self.installed.load(Ordering::Relaxed),
                active_plan_version: self.active_plan_version.load(Ordering::Relaxed),
                pending_plan_version: (pending_version != 0).then_some(pending_version),
                mode: LoopMode::try_from(self.mode.load(Ordering::Relaxed))
                    .unwrap_or(LoopMode::Unknown),
                next_mode: (next_mode != NO_MODE)
                    .then(|| LoopMode::try_from(next_mode).unwrap_or(LoopMode::Unknown)),
                next_mode_delay: (next_delay != NO_DELAY).then_some(next_delay as u32),
                iteration: self.iteration.load(Ordering::Relaxed),
                cycle_count: self.cycle_count.load(Ordering::Relaxed),
                length: self.length.load(Ordering::Relaxed),
                position: self.position.load(Ordering::Relaxed),
                play_after_record: self.play_after_record.load(Ordering::Relaxed),
                runtime_counters: CompositeRuntimeCounters {
                    stale_targets: self.stale_targets.load(Ordering::Relaxed),
                    invalid_seeks: self.invalid_seeks.load(Ordering::Relaxed),
                    rejected_modes: self.rejected_modes.load(Ordering::Relaxed),
                    plan_mismatches: self.plan_mismatches.load(Ordering::Relaxed),
                    output_overflows: self.output_overflows.load(Ordering::Relaxed),
                    arithmetic_overflows: self.arithmetic_overflows.load(Ordering::Relaxed),
                },
                runtime_fault: if self.runtime_fault.load(Ordering::Relaxed) == 0 {
                    CompositeRuntimeFault::None
                } else {
                    CompositeRuntimeFault::OutputCapacity
                },
                active_children,
                n_active_children: count,
            };
            let after = self.generation.load(Ordering::Acquire);
            if before == after {
                return snapshot;
            }
        }
    }
}

impl Default for CompositeStateMirror {
    fn default() -> Self {
        Self::new(EMPTY_IDENTITY)
    }
}

#[derive(Debug)]
pub struct AudioChannelStateMirror {
    mode: AtomicI32,
    gain: AtomicU32,
    output_peak: AtomicU32,
    length: AtomicU32,
    start_offset: AtomicI32,
    capture_alignment_frames: AtomicI32,
    render_advance_frames: AtomicU32,
    played_back_sample: AtomicI32,
    logical_played_position: AtomicI32,
    raw_played_position: AtomicI32,
    dispatch_position: AtomicI32,
    n_preplay_samples: AtomicU32,
    latency_retention_incomplete: AtomicBool,
    latency_history_variable: AtomicBool,
    latency_history_revisions: AtomicU32,
    data_sequence: AtomicU64,
    current_latency_recipe: AtomicLatencyRecipePublication,
    latched_latency_recipe: AtomicLatencyRecipePublication,
}

impl Default for AudioChannelStateMirror {
    fn default() -> Self {
        Self {
            mode: AtomicI32::new(ChannelMode::Disabled as i32),
            gain: AtomicU32::new(0.0f32.to_bits()),
            output_peak: AtomicU32::new(0.0f32.to_bits()),
            length: AtomicU32::new(0),
            start_offset: AtomicI32::new(0),
            capture_alignment_frames: AtomicI32::new(0),
            render_advance_frames: AtomicU32::new(0),
            played_back_sample: AtomicI32::new(NO_SAMPLE),
            logical_played_position: AtomicI32::new(NO_SAMPLE),
            raw_played_position: AtomicI32::new(NO_SAMPLE),
            dispatch_position: AtomicI32::new(NO_SAMPLE),
            n_preplay_samples: AtomicU32::new(0),
            latency_retention_incomplete: AtomicBool::new(false),
            latency_history_variable: AtomicBool::new(false),
            latency_history_revisions: AtomicU32::new(0),
            data_sequence: AtomicU64::new(0),
            current_latency_recipe: AtomicLatencyRecipePublication::default(),
            latched_latency_recipe: AtomicLatencyRecipePublication::default(),
        }
    }
}

impl AudioChannelStateMirror {
    pub fn publish(
        &self,
        mode: ChannelMode,
        gain: f32,
        length: usize,
        start_offset: i32,
        capture_alignment_frames: i32,
        render_advance_frames: u32,
        played_back_sample: Option<i32>,
        n_preplay_samples: u32,
        data_sequence: u64,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
        self.length.store(length as u32, Ordering::Relaxed);
        self.start_offset.store(start_offset, Ordering::Relaxed);
        self.capture_alignment_frames
            .store(capture_alignment_frames, Ordering::Relaxed);
        self.render_advance_frames
            .store(render_advance_frames, Ordering::Relaxed);
        self.played_back_sample
            .store(played_back_sample.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.n_preplay_samples
            .store(n_preplay_samples, Ordering::Relaxed);
        self.data_sequence.store(data_sequence, Ordering::Relaxed);
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        self.mode.store(mode as i32, Ordering::Relaxed);
    }

    pub fn set_start_offset(&self, offset: i32) {
        self.start_offset.store(offset, Ordering::Relaxed);
    }

    pub fn set_capture_alignment_frames(&self, frames: i32) {
        self.capture_alignment_frames
            .store(frames, Ordering::Relaxed);
    }

    pub fn set_n_preplay_samples(&self, samples: u32) {
        self.n_preplay_samples.store(samples, Ordering::Relaxed);
    }

    pub fn publish_output_peak(&self, peak: f32) {
        atomic_max_f32(&self.output_peak, peak);
    }

    pub fn publish_playback_positions(
        &self,
        logical: Option<i32>,
        raw: Option<i32>,
        dispatch: Option<i32>,
    ) {
        self.logical_played_position
            .store(logical.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.raw_played_position
            .store(raw.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.dispatch_position
            .store(dispatch.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
    }

    pub fn publish_latency_retention_incomplete(&self, incomplete: bool) {
        self.latency_retention_incomplete
            .store(incomplete, Ordering::Relaxed);
    }

    pub fn publish_latency_history(&self, variable: bool, revisions: u32) {
        self.latency_history_variable
            .store(variable, Ordering::Relaxed);
        self.latency_history_revisions
            .store(revisions, Ordering::Relaxed);
    }

    pub fn publish_current_latency_recipe(&self, recipe: Option<RuntimeLatencyRecipe>) {
        self.current_latency_recipe.publish_pending(recipe);
    }

    pub fn publish_latched_latency_recipe(&self, recipe: Option<LatchedLatencyRecipe>) {
        self.latched_latency_recipe.publish_latched(recipe);
    }

    pub fn read(&self, acknowledged_data_sequence: u64) -> AudioChannelState {
        let played = self.played_back_sample.load(Ordering::Relaxed);
        let logical = self.logical_played_position.load(Ordering::Relaxed);
        let raw = self.raw_played_position.load(Ordering::Relaxed);
        let dispatch = self.dispatch_position.load(Ordering::Relaxed);
        AudioChannelState {
            mode: ChannelMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(ChannelMode::Disabled),
            gain: f32::from_bits(self.gain.load(Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak.swap(0.0f32.to_bits(), Ordering::Relaxed)),
            length: self.length.load(Ordering::Relaxed),
            start_offset: self.start_offset.load(Ordering::Relaxed),
            capture_alignment_frames: self.capture_alignment_frames.load(Ordering::Relaxed),
            render_advance_frames: self.render_advance_frames.load(Ordering::Relaxed),
            played_back_sample: (played != NO_SAMPLE).then_some(played),
            logical_played_position: (logical != NO_SAMPLE).then_some(logical),
            raw_played_position: (raw != NO_SAMPLE).then_some(raw),
            dispatch_position: (dispatch != NO_SAMPLE).then_some(dispatch),
            n_preplay_samples: self.n_preplay_samples.load(Ordering::Relaxed),
            latency_retention_incomplete: self.latency_retention_incomplete.load(Ordering::Relaxed),
            latency_history_variable: self.latency_history_variable.load(Ordering::Relaxed),
            latency_history_revisions: self.latency_history_revisions.load(Ordering::Relaxed),
            data_dirty: self.data_sequence.load(Ordering::Relaxed) != acknowledged_data_sequence,
            current_latency_recipe: self.current_latency_recipe.read(),
            latched_latency_recipe: self.latched_latency_recipe.read(),
        }
    }

    pub fn data_sequence(&self) -> u64 {
        self.data_sequence.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct MidiChannelStateMirror {
    mode: AtomicI32,
    n_events_triggered: AtomicU32,
    n_notes_active: AtomicU32,
    length: AtomicU32,
    start_offset: AtomicI32,
    capture_alignment_frames: AtomicI32,
    render_advance_frames: AtomicU32,
    played_back_sample: AtomicI32,
    logical_played_position: AtomicI32,
    raw_played_position: AtomicI32,
    dispatch_position: AtomicI32,
    n_preplay_samples: AtomicU32,
    latency_retention_incomplete: AtomicBool,
    latency_history_variable: AtomicBool,
    latency_history_revisions: AtomicU32,
    data_sequence: AtomicU64,
    current_latency_recipe: AtomicLatencyRecipePublication,
    latched_latency_recipe: AtomicLatencyRecipePublication,
}

impl Default for MidiChannelStateMirror {
    fn default() -> Self {
        Self {
            mode: AtomicI32::new(ChannelMode::Disabled as i32),
            n_events_triggered: AtomicU32::new(0),
            n_notes_active: AtomicU32::new(0),
            length: AtomicU32::new(0),
            start_offset: AtomicI32::new(0),
            capture_alignment_frames: AtomicI32::new(0),
            render_advance_frames: AtomicU32::new(0),
            played_back_sample: AtomicI32::new(NO_SAMPLE),
            logical_played_position: AtomicI32::new(NO_SAMPLE),
            raw_played_position: AtomicI32::new(NO_SAMPLE),
            dispatch_position: AtomicI32::new(NO_SAMPLE),
            n_preplay_samples: AtomicU32::new(0),
            latency_retention_incomplete: AtomicBool::new(false),
            latency_history_variable: AtomicBool::new(false),
            latency_history_revisions: AtomicU32::new(0),
            data_sequence: AtomicU64::new(0),
            current_latency_recipe: AtomicLatencyRecipePublication::default(),
            latched_latency_recipe: AtomicLatencyRecipePublication::default(),
        }
    }
}

impl MidiChannelStateMirror {
    pub fn publish(
        &self,
        mode: ChannelMode,
        n_notes_active: u32,
        length: u32,
        start_offset: i32,
        capture_alignment_frames: i32,
        render_advance_frames: u32,
        played_back_sample: Option<i32>,
        n_preplay_samples: u32,
        data_sequence: u64,
    ) {
        self.mode.store(mode as i32, Ordering::Relaxed);
        self.n_notes_active.store(n_notes_active, Ordering::Relaxed);
        self.length.store(length, Ordering::Relaxed);
        self.start_offset.store(start_offset, Ordering::Relaxed);
        self.capture_alignment_frames
            .store(capture_alignment_frames, Ordering::Relaxed);
        self.render_advance_frames
            .store(render_advance_frames, Ordering::Relaxed);
        self.played_back_sample
            .store(played_back_sample.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.n_preplay_samples
            .store(n_preplay_samples, Ordering::Relaxed);
        self.data_sequence.store(data_sequence, Ordering::Relaxed);
    }

    pub fn set_mode(&self, mode: ChannelMode) {
        self.mode.store(mode as i32, Ordering::Relaxed);
    }

    pub fn set_start_offset(&self, offset: i32) {
        self.start_offset.store(offset, Ordering::Relaxed);
    }

    pub fn set_capture_alignment_frames(&self, frames: i32) {
        self.capture_alignment_frames
            .store(frames, Ordering::Relaxed);
    }

    pub fn set_n_preplay_samples(&self, samples: u32) {
        self.n_preplay_samples.store(samples, Ordering::Relaxed);
    }

    pub fn record_triggered_event(&self) {
        self.n_events_triggered.fetch_add(1, Ordering::Relaxed);
    }

    pub fn publish_playback_positions(
        &self,
        logical: Option<i32>,
        raw: Option<i32>,
        dispatch: Option<i32>,
    ) {
        self.logical_played_position
            .store(logical.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.raw_played_position
            .store(raw.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
        self.dispatch_position
            .store(dispatch.unwrap_or(NO_SAMPLE), Ordering::Relaxed);
    }

    pub fn publish_latency_retention_incomplete(&self, incomplete: bool) {
        self.latency_retention_incomplete
            .store(incomplete, Ordering::Relaxed);
    }

    pub fn publish_latency_history(&self, variable: bool, revisions: u32) {
        self.latency_history_variable
            .store(variable, Ordering::Relaxed);
        self.latency_history_revisions
            .store(revisions, Ordering::Relaxed);
    }

    pub fn publish_current_latency_recipe(&self, recipe: Option<RuntimeLatencyRecipe>) {
        self.current_latency_recipe.publish_pending(recipe);
    }

    pub fn publish_latched_latency_recipe(&self, recipe: Option<LatchedLatencyRecipe>) {
        self.latched_latency_recipe.publish_latched(recipe);
    }

    pub fn read(&self, acknowledged_data_sequence: u64) -> MidiChannelState {
        let played = self.played_back_sample.load(Ordering::Relaxed);
        let logical = self.logical_played_position.load(Ordering::Relaxed);
        let raw = self.raw_played_position.load(Ordering::Relaxed);
        let dispatch = self.dispatch_position.load(Ordering::Relaxed);
        MidiChannelState {
            mode: ChannelMode::try_from(self.mode.load(Ordering::Relaxed))
                .unwrap_or(ChannelMode::Disabled),
            n_events_triggered: self.n_events_triggered.swap(0, Ordering::Relaxed),
            n_notes_active: self.n_notes_active.load(Ordering::Relaxed),
            length: self.length.load(Ordering::Relaxed),
            start_offset: self.start_offset.load(Ordering::Relaxed),
            capture_alignment_frames: self.capture_alignment_frames.load(Ordering::Relaxed),
            render_advance_frames: self.render_advance_frames.load(Ordering::Relaxed),
            played_back_sample: (played != NO_SAMPLE).then_some(played),
            logical_played_position: (logical != NO_SAMPLE).then_some(logical),
            raw_played_position: (raw != NO_SAMPLE).then_some(raw),
            dispatch_position: (dispatch != NO_SAMPLE).then_some(dispatch),
            n_preplay_samples: self.n_preplay_samples.load(Ordering::Relaxed),
            latency_retention_incomplete: self.latency_retention_incomplete.load(Ordering::Relaxed),
            latency_history_variable: self.latency_history_variable.load(Ordering::Relaxed),
            latency_history_revisions: self.latency_history_revisions.load(Ordering::Relaxed),
            data_dirty: self.data_sequence.load(Ordering::Relaxed) != acknowledged_data_sequence,
            current_latency_recipe: self.current_latency_recipe.read(),
            latched_latency_recipe: self.latched_latency_recipe.read(),
        }
    }

    pub fn data_sequence(&self) -> u64 {
        self.data_sequence.load(Ordering::Relaxed)
    }
}

#[derive(Debug)]
pub struct AudioPortStateMirror {
    gain: AtomicU32,
    muted: AtomicBool,
    passthrough_muted: AtomicBool,
    input_peak: AtomicU32,
    output_peak: AtomicU32,
    ringbuffer_n_samples: AtomicU32,
    capture_latency: AtomicLatencyObservation,
    playback_latency: AtomicLatencyObservation,
}

impl Default for AudioPortStateMirror {
    fn default() -> Self {
        Self {
            gain: AtomicU32::new(1.0f32.to_bits()),
            muted: AtomicBool::new(false),
            passthrough_muted: AtomicBool::new(false),
            input_peak: AtomicU32::new(0.0f32.to_bits()),
            output_peak: AtomicU32::new(0.0f32.to_bits()),
            ringbuffer_n_samples: AtomicU32::new(0),
            capture_latency: AtomicLatencyObservation::default(),
            playback_latency: AtomicLatencyObservation::default(),
        }
    }
}

impl AudioPortStateMirror {
    pub fn publish_scalars(&self, gain: f32, muted: bool, passthrough_muted: bool, ring: usize) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
        self.muted.store(muted, Ordering::Relaxed);
        self.passthrough_muted
            .store(passthrough_muted, Ordering::Relaxed);
        self.ringbuffer_n_samples
            .store(ring as u32, Ordering::Relaxed);
    }

    pub fn set_gain(&self, gain: f32) {
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_passthrough_muted(&self, muted: bool) {
        self.passthrough_muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_ringbuffer_n_samples(&self, samples: u32) {
        self.ringbuffer_n_samples.store(samples, Ordering::Relaxed);
    }

    pub fn publish_peaks(&self, input: f32, output: f32) {
        atomic_max_f32(&self.input_peak, input);
        atomic_max_f32(&self.output_peak, output);
    }

    pub fn publish_capture_latency(&self, observation: RuntimeLatencyObservation) {
        self.capture_latency.publish(observation);
    }

    pub fn publish_playback_latency(&self, observation: RuntimeLatencyObservation) {
        self.playback_latency.publish(observation);
    }

    pub fn capture_latency(&self) -> RuntimeLatencyObservation {
        self.capture_latency.read()
    }

    pub fn playback_latency(&self) -> RuntimeLatencyObservation {
        self.playback_latency.read()
    }

    pub fn read(&self, name: String) -> AudioPortState {
        AudioPortState {
            input_peak: f32::from_bits(self.input_peak.swap(0, Ordering::Relaxed)),
            output_peak: f32::from_bits(self.output_peak.swap(0, Ordering::Relaxed)),
            gain: f32::from_bits(self.gain.load(Ordering::Relaxed)),
            muted: self.muted.load(Ordering::Relaxed),
            passthrough_muted: self.passthrough_muted.load(Ordering::Relaxed),
            ringbuffer_n_samples: self.ringbuffer_n_samples.load(Ordering::Relaxed),
            capture_latency: self.capture_latency.read(),
            playback_latency: self.playback_latency.read(),
            name,
        }
    }
}

#[derive(Debug, Default)]
pub struct MidiPortStateMirror {
    n_input_events: AtomicU32,
    n_input_notes_active: AtomicU32,
    n_output_events: AtomicU32,
    n_output_notes_active: AtomicU32,
    muted: AtomicBool,
    passthrough_muted: AtomicBool,
    ringbuffer_n_samples: AtomicU32,
    capture_latency: AtomicLatencyObservation,
    playback_latency: AtomicLatencyObservation,
    latest_input_message: AtomicU64,
}

impl MidiPortStateMirror {
    pub fn publish_scalars(
        &self,
        input_notes: u32,
        output_notes: u32,
        muted: bool,
        passthrough_muted: bool,
        ring: u32,
    ) {
        self.n_input_notes_active
            .store(input_notes, Ordering::Relaxed);
        self.n_output_notes_active
            .store(output_notes, Ordering::Relaxed);
        self.muted.store(muted, Ordering::Relaxed);
        self.passthrough_muted
            .store(passthrough_muted, Ordering::Relaxed);
        self.ringbuffer_n_samples.store(ring, Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_passthrough_muted(&self, muted: bool) {
        self.passthrough_muted.store(muted, Ordering::Relaxed);
    }

    pub fn set_ringbuffer_n_samples(&self, samples: u32) {
        self.ringbuffer_n_samples.store(samples, Ordering::Relaxed);
    }

    pub fn record_events(&self, input: u32, output: u32) {
        self.n_input_events.fetch_add(input, Ordering::Relaxed);
        self.n_output_events.fetch_add(output, Ordering::Relaxed);
    }

    pub fn publish_capture_latency(&self, observation: RuntimeLatencyObservation) {
        self.capture_latency.publish(observation);
    }

    pub fn publish_playback_latency(&self, observation: RuntimeLatencyObservation) {
        self.playback_latency.publish(observation);
    }

    pub fn capture_latency(&self) -> RuntimeLatencyObservation {
        self.capture_latency.read()
    }

    pub fn playback_latency(&self) -> RuntimeLatencyObservation {
        self.playback_latency.read()
    }

    pub fn publish_latest_input_message(&self, message: LatestMidiMessage) {
        let bytes = u32::from_le_bytes(message.bytes) as u64;
        self.latest_input_message
            .store(bytes | ((message.len as u64) << 32), Ordering::Relaxed);
    }

    pub fn read(&self, name: String) -> MidiPortState {
        let packed_message = self.latest_input_message.load(Ordering::Relaxed);
        let message_len = (packed_message >> 32) as u8;
        let latest_input_message = (message_len != 0).then(|| LatestMidiMessage {
            bytes: (packed_message as u32).to_le_bytes(),
            len: message_len,
        });
        MidiPortState {
            n_input_events: self.n_input_events.swap(0, Ordering::Relaxed),
            n_input_notes_active: self.n_input_notes_active.load(Ordering::Relaxed),
            n_output_events: self.n_output_events.swap(0, Ordering::Relaxed),
            n_output_notes_active: self.n_output_notes_active.load(Ordering::Relaxed),
            muted: self.muted.load(Ordering::Relaxed),
            passthrough_muted: self.passthrough_muted.load(Ordering::Relaxed),
            ringbuffer_n_samples: self.ringbuffer_n_samples.load(Ordering::Relaxed),
            capture_latency: self.capture_latency.read(),
            playback_latency: self.playback_latency.read(),
            latest_input_message,
            name,
        }
    }
}

fn atomic_max_f32(target: &AtomicU32, value: f32) {
    if !value.is_finite() || value <= 0.0 {
        return;
    }
    let mut current = target.load(Ordering::Relaxed);
    while value > f32::from_bits(current) {
        match target.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    #[shoop_wasm_test_support::shoop_test]
    fn composite_state_is_coherently_published_and_uninstalled() {
        let identity = LoopIdentity {
            slot: 12,
            generation: 3,
            kind: LoopTargetKind::Composite,
        };
        let sync = LoopIdentity {
            slot: 1,
            generation: 1,
            kind: LoopTargetKind::Basic,
        };
        let child = ActiveCompositeChild {
            identity: LoopIdentity {
                slot: 2,
                generation: 1,
                kind: LoopTargetKind::Basic,
            },
            mode: LoopMode::Playing,
            cycle_offset: 4,
        };
        let mirror = CompositeStateMirror::new(identity);
        check!(!mirror.read().installed);

        mirror.publish(
            sync,
            7,
            Some(8),
            LoopMode::Recording,
            Some((LoopMode::Playing, 2)),
            3,
            9,
            128,
            47,
            true,
            CompositeRuntimeCounters {
                stale_targets: 1,
                ..CompositeRuntimeCounters::default()
            },
            CompositeRuntimeFault::OutputCapacity,
            [child].into_iter(),
        );
        let state = mirror.read();
        check!(state.installed);
        check!(state.identity == identity);
        check!(state.sync_source == sync);
        check!(state.active_plan_version == 7);
        check!(state.pending_plan_version == Some(8));
        check!(state.mode == LoopMode::Recording);
        check!(state.next_mode == Some(LoopMode::Playing));
        check!(state.next_mode_delay == Some(2));
        check!(state.iteration == 3);
        check!(state.cycle_count == 9);
        check!(state.length == 128);
        check!(state.position == 47);
        check!(state.play_after_record);
        check!(state.runtime_counters.stale_targets == 1);
        check!(state.runtime_fault == CompositeRuntimeFault::OutputCapacity);
        check!(state.active_children().collect::<Vec<_>>() == vec![child]);

        mirror.mark_uninstalled();
        let state = mirror.read();
        check!(!state.installed);
        check!(state.mode == LoopMode::Stopped);
        check!(state.active_children().next().is_none());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_accumulators_are_consumed_without_commands() {
        let audio = AudioChannelStateMirror::default();
        audio.publish_output_peak(0.25);
        audio.publish_output_peak(0.75);
        check!(audio.read(0).output_peak == 0.75);
        check!(audio.read(0).output_peak == 0.0);

        let midi = MidiChannelStateMirror::default();
        midi.record_triggered_event();
        midi.record_triggered_event();
        check!(midi.read(0).n_events_triggered == 2);
        check!(midi.read(0).n_events_triggered == 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn channel_data_sequences_support_local_acknowledgement() {
        let audio = AudioChannelStateMirror::default();
        audio.publish(ChannelMode::Direct, 1.0, 4, 0, 0, 0, None, 0, 3);
        check!(audio.read(0).data_dirty);
        check!(!audio.read(3).data_dirty);

        let midi = MidiChannelStateMirror::default();
        midi.publish(ChannelMode::Direct, 0, 4, 0, 0, 0, None, 0, 7);
        check!(midi.read(0).data_dirty);
        check!(!midi.read(7).data_dirty);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn port_accumulators_are_consumed_without_reset_commands() {
        let audio = AudioPortStateMirror::default();
        audio.publish_peaks(0.25, 0.5);
        audio.publish_peaks(0.75, 0.4);
        let first = audio.read("audio".to_string());
        check!(first.input_peak == 0.75);
        check!(first.output_peak == 0.5);
        check!(audio.read("audio".to_string()).input_peak == 0.0);

        let midi = MidiPortStateMirror::default();
        midi.record_events(2, 1);
        midi.record_events(3, 4);
        midi.publish_latest_input_message(LatestMidiMessage::new(&[0xb3, 17, 99]).unwrap());
        let first = midi.read("midi".to_string());
        check!(first.n_input_events == 5);
        check!(first.n_output_events == 5);
        check!(first.latest_input_message.unwrap().data() == [0xb3, 17, 99]);
        let second = midi.read("midi".to_string());
        check!(second.n_input_events == 0);
        check!(second.latest_input_message.unwrap().data() == [0xb3, 17, 99]);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn port_latency_observations_publish_with_independent_revisions() {
        let capture = RuntimeLatencyObservation::exact(17, 48_000, 3).unwrap();
        let playback = RuntimeLatencyObservation::exact(29, 48_000, 8).unwrap();

        let audio = AudioPortStateMirror::default();
        audio.publish_capture_latency(capture);
        audio.publish_playback_latency(playback);
        let state = audio.read("audio".to_string());
        check!(state.capture_latency == capture);
        check!(state.playback_latency == playback);

        let midi = MidiPortStateMirror::default();
        midi.publish_capture_latency(capture);
        midi.publish_playback_latency(playback);
        check!(midi.capture_latency() == capture);
        check!(midi.playback_latency() == playback);
        let state = midi.read("midi".to_string());
        check!(state.capture_latency.revision == 3);
        check!(state.playback_latency.revision == 8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn loop_state_fields_are_independently_published() {
        let mirror = LoopStateMirror::default();
        check!(mirror.read().mode == LoopMode::Stopped);

        mirror.publish(
            LoopMode::Playing,
            128,
            17,
            3,
            Some((LoopMode::Recording, 2)),
        );
        let state = mirror.read();
        check!(state.mode == LoopMode::Playing);
        check!(state.length == 128);
        check!(state.position == 17);
        check!(state.cycle_count == 3);
        check!(state.maybe_next_mode == Some(LoopMode::Recording));
        check!(state.maybe_next_mode_delay == Some(2));

        mirror.publish(LoopMode::Stopped, 0, 0, 4, None);
        let state = mirror.read();
        check!(state.maybe_next_mode.is_none());
        check!(state.maybe_next_mode_delay.is_none());
    }
}
