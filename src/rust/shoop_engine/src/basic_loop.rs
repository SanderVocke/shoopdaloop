//! Loop mechanics shared by all loop types: point-of-interest tracking,
//! trigger propagation and planned mode transitions.

use std::collections::VecDeque;
use std::sync::Arc;

use crate::loop_mode::LoopMode;
use crate::state_mirror::LoopStateMirror;

/// Bit flags marking why a point of interest exists. Several can coincide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PoiFlags(pub u32);

impl PoiFlags {
    pub const NONE: Self = Self(0);
    pub const TRIGGER: Self = Self(1);
    pub const LOOP_END: Self = Self(2);
    pub const CHANNEL: Self = Self(4);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
    pub fn is_empty(self) -> bool {
        self.0 == 0
    }
    fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
    fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// The first point until which the loop can be processed without changing state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointOfInterest {
    pub when: u32,
    pub flags: PoiFlags,
}

/// Snapshot of the loop this loop is synced to.
///
/// source on every query. Here the session refreshes a snapshot before
/// processing dependents, which the graph schedule already orders correctly.
/// That keeps the hot path free of pointer chasing and refcount traffic, and
/// makes loop mechanics testable without constructing a second loop.
///
/// `None` means unsynced, which is behaviourally distinct from a synced-but-idle
/// source: an unsynced loop transitions immediately instead of queueing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncSourceState {
    pub mode: LoopMode,
    pub triggering_now: bool,
    pub next_trigger_eta: Option<u32>,
    pub position: u32,
    pub length: u32,
}

/// What the owning loop type should do to its channels this cycle. Produced by
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelProcessParams {
    pub mode: LoopMode,
    /// `Unknown` when no transition is planned.
    pub next_planned_mode: LoopMode,
    pub next_planned_delay_cycles: Option<u32>,
    pub next_planned_eta: Option<u32>,
    pub n_samples: u32,
    pub pos_before: u32,
    pub pos_after: u32,
    pub length_before: u32,
    pub length_after: u32,
}

#[derive(Debug, Default)]
pub struct BasicLoop {
    next_poi: Option<PointOfInterest>,
    next_trigger: Option<u32>,
    sync_source: Option<SyncSourceState>,
    planned_modes: VecDeque<LoopMode>,
    planned_countdowns: VecDeque<i32>,
    mode: LoopMode,
    triggering_now: bool,
    already_triggered: bool,
    length: u32,
    position: u32,
    cycle_count: u64,
    state: Arc<LoopStateMirror>,
}

impl BasicLoop {
    pub fn with_state_mirror(state: Arc<LoopStateMirror>) -> Self {
        Self {
            state,
            ..Default::default()
        }
    }

    pub fn state_mirror(&self) -> &Arc<LoopStateMirror> {
        &self.state
    }

    fn publish_state(&self) {
        self.publish_state_with_transition(self.first_planned_transition());
    }

    pub(crate) fn publish_state_with_transition(&self, transition: Option<(LoopMode, u32)>) {
        self.state.publish(
            self.mode,
            self.length,
            self.position,
            self.cycle_count,
            transition,
        );
    }

    // --- queries ---

    pub fn mode(&self) -> LoopMode {
        self.mode
    }
    pub fn length(&self) -> u32 {
        self.length
    }
    pub fn position(&self) -> u32 {
        self.position
    }
    pub fn next_poi(&self) -> Option<u32> {
        self.next_poi.map(|p| p.when)
    }
    pub fn predicted_next_trigger_eta(&self) -> Option<u32> {
        self.next_trigger
    }
    pub fn sync_source(&self) -> Option<SyncSourceState> {
        self.sync_source
    }

    /// Mechanics-only view for a dependent loop's snapshot. `triggering_now` is
    /// deliberately not folded in here; use [`Self::is_triggering_now`], which
    /// may settle a pending POI first.
    pub fn as_sync_source_state(&self) -> SyncSourceState {
        SyncSourceState {
            mode: self.mode,
            triggering_now: self.triggering_now,
            next_trigger_eta: self.next_trigger,
            position: self.position,
            length: self.length,
        }
    }

    // --- sync source ---

    pub fn set_sync_source(&mut self, src: Option<SyncSourceState>) {
        self.sync_source = src;
        self.update_trigger_eta();
    }

    // --- point of interest / trigger bookkeeping ---

    pub fn update_trigger_eta(&mut self) {
        self.next_trigger = if self.mode.is_playing_mode() && self.position < self.length {
            Some(self.length - self.position)
        } else {
            None
        };

        if let Some(eta) = self.sync_source.and_then(|s| s.next_trigger_eta) {
            self.next_trigger = Some(match self.next_trigger {
                Some(own) => own.min(eta),
                None => eta,
            });
        }
    }

    pub fn update_poi(&mut self) {
        if self.mode.is_playing_mode() && self.length == 0 {
            self.handle_transition(LoopMode::Stopped);
        }

        // Loop-end and channel POIs are recalculated; anything else survives.
        if let Some(poi) = self.next_poi.as_mut() {
            poi.flags = poi
                .flags
                .without(PoiFlags::LOOP_END)
                .without(PoiFlags::CHANNEL);
            if poi.flags.is_empty() {
                self.next_poi = None;
            }
        }

        if self.mode.is_playing_mode() && self.position < self.length {
            let loop_end = PointOfInterest {
                when: self.length - self.position,
                flags: PoiFlags::LOOP_END,
            };
            self.next_poi = dominant_poi(self.next_poi, Some(loop_end));
        }
    }

    /// Folds a channel's point of interest into this loop's.
    ///
    /// A strictly earlier channel POI *replaces* the current one rather than
    /// after every `update_poi`, which is safe because `update_poi` recomputes
    /// loop-end and channel POIs from scratch each time.
    pub fn merge_channel_poi(&mut self, when: u32) {
        if self.next_poi.is_none_or(|p| when < p.when) {
            self.next_poi = Some(PointOfInterest {
                when,
                flags: PoiFlags::CHANNEL,
            });
        }
    }

    pub fn handle_poi(&mut self) {
        // Only a POI reached exactly now is actionable.
        if self.next_poi.map(|p| p.when) != Some(0) {
            return;
        }
        let mut changed = false;

        if let Some(poi) = self.next_poi.as_mut() {
            poi.flags = poi.flags.without(PoiFlags::CHANNEL);
        }

        if self
            .next_poi
            .is_some_and(|p| p.flags.contains(PoiFlags::TRIGGER))
        {
            self.trigger(true);
            if let Some(poi) = self.next_poi.as_mut() {
                poi.flags = poi.flags.without(PoiFlags::TRIGGER);
            }
            changed = true;
        }
        if self
            .next_poi
            .is_some_and(|p| p.flags.contains(PoiFlags::LOOP_END))
        {
            if let Some(poi) = self.next_poi.as_mut() {
                poi.flags = poi.flags.without(PoiFlags::LOOP_END);
            }
            // Trigger ourselves only when no active sync source will do it.
            if self.sync_source.is_none_or(|s| !s.mode.is_playing_mode()) {
                self.trigger(true);
            }
            changed = true;
        }

        if self.next_poi.is_some_and(|p| p.flags.is_empty()) {
            self.next_poi = None;
            changed = true;
        }

        if changed {
            self.update_poi();
            self.update_trigger_eta();
            self.publish_state();
        }
    }

    /// Whether this loop triggers on the current cycle. Settles a POI standing
    /// at zero first, so this is `&mut self`.
    pub fn is_triggering_now(&mut self) -> bool {
        if self.next_poi.map(|p| p.when) == Some(0) {
            self.handle_poi();
        }
        if self.sync_source.is_some_and(|s| s.triggering_now) {
            return true;
        }
        self.triggering_now
    }

    pub fn trigger(&mut self, propagate: bool) {
        if self.already_triggered {
            return;
        }
        self.already_triggered = true;

        if propagate {
            self.triggering_now = true;
        }

        if self.mode.is_playing_mode() && self.position >= self.length {
            self.position = 0;
            self.cycle_count = self.cycle_count.saturating_add(1);
        }

        for c in self.planned_countdowns.iter_mut() {
            *c -= 1;
        }
        while self.planned_countdowns.front().is_some_and(|c| *c < 0) {
            let mode = self.planned_modes[0];
            self.handle_transition(mode);
            self.planned_countdowns.pop_front();
            self.planned_modes.pop_front();
        }
        self.publish_state();
    }

    pub fn handle_sync(&mut self) {
        if self.sync_source.is_some_and(|s| s.triggering_now) {
            self.trigger(true);
        }
    }

    pub fn handle_transition(&mut self, new_mode: LoopMode) {
        if self.mode == new_mode {
            return;
        }
        let playing_to_playing = self.mode.is_playing_mode() && new_mode.is_playing_mode();
        if !playing_to_playing {
            self.set_position(0);
        }
        if new_mode == LoopMode::Recording {
            // Recording always resets the loop; channels are cleared elsewhere.
            self.set_length(0);
        }
        self.mode = new_mode;
        if self.mode == LoopMode::Stopped {
            self.position = 0;
        }
        if self.mode.is_playing_mode() && self.position == 0 {
            self.triggering_now = true;
        }
        self.next_poi = None;
        self.update_poi();
        self.update_trigger_eta();
        self.publish_state();
    }

    // --- processing ---

    pub fn process(&mut self, n_samples: u32) {
        self.process_with(n_samples, |_| {});
    }

    /// Advances the loop by `n_samples`. `on_channels` runs at the point the
    /// computed but before they are committed.
    ///
    /// Panics if asked to cross the next POI: that is a scheduler contract
    pub fn process_with(
        &mut self,
        n_samples: u32,
        on_channels: impl FnOnce(&ChannelProcessParams),
    ) {
        if let Some(poi) = self.next_poi {
            assert!(
                n_samples <= poi.when,
                "attempted to process loop {n_samples} samples beyond its next POI at {}",
                poi.when
            );
        }

        self.triggering_now = false;
        self.already_triggered = false;

        let pos_before = self.position;
        let length_before = self.length;
        let mut pos_after = self.position;
        let mut length_after = self.length;

        match self.mode {
            LoopMode::Recording => length_after += n_samples,
            LoopMode::Replacing => {
                pos_after += n_samples;
                length_after = length_after.max(pos_after);
            }
            LoopMode::Playing | LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet => {
                pos_after = (pos_after + n_samples).min(length_after);
            }
            _ => {}
        }

        let (next_planned_mode, next_planned_delay) = self.planned_transition_head();
        on_channels(&ChannelProcessParams {
            mode: self.mode,
            next_planned_mode,
            next_planned_delay_cycles: next_planned_delay,
            next_planned_eta: if next_planned_delay == Some(0) {
                self.predicted_next_trigger_eta()
            } else {
                None
            },
            n_samples,
            pos_before,
            pos_after,
            length_before,
            length_after,
        });

        if let Some(poi) = self.next_poi.as_mut() {
            poi.when -= n_samples;
        }
        self.position = pos_after;
        self.length = length_after;
        if let Some(eta) = self.next_trigger {
            let remaining = eta.saturating_sub(n_samples);
            self.next_trigger = if remaining == 0 {
                None
            } else {
                Some(remaining)
            };
        }
        self.handle_poi();
        self.publish_state();
    }

    // --- planned transitions ---

    /// Head of the planned queue as (mode, delay). `Unknown`/`None` when empty.
    fn planned_transition_head(&self) -> (LoopMode, Option<u32>) {
        (
            self.planned_modes
                .front()
                .copied()
                .unwrap_or(LoopMode::Unknown),
            self.planned_countdowns.front().map(|c| *c as u32),
        )
    }

    pub fn n_planned_transitions(&self) -> usize {
        self.planned_modes.len()
    }

    pub fn planned_transition_delay(&self, idx: usize) -> Option<i32> {
        self.planned_countdowns.get(idx).copied()
    }

    pub fn planned_transition_mode(&self, idx: usize) -> Option<LoopMode> {
        self.planned_modes.get(idx).copied()
    }

    pub fn clear_planned_transitions(&mut self) {
        self.planned_modes.clear();
        self.planned_countdowns.clear();
        self.publish_state();
    }

    /// First planned transition, or `None` when nothing is queued.
    pub fn first_planned_transition(&self) -> Option<(LoopMode, u32)> {
        match self.planned_transition_head() {
            (LoopMode::Unknown, _) => None,
            (mode, Some(delay)) => Some((mode, delay)),
            (_, None) => None,
        }
    }

    /// Queues a mode transition.
    ///
    /// `n_cycles_delay` counts sync triggers to wait; `None` forces immediate.
    /// `to_sync_cycle` also forces immediate and additionally aligns position to
    /// the sync source.
    pub fn plan_transition(
        &mut self,
        mode: LoopMode,
        n_cycles_delay: Option<u32>,
        to_sync_cycle: Option<u32>,
    ) {
        let immediately = (self.sync_source.is_none() && self.mode != LoopMode::Playing)
            || n_cycles_delay.is_none()
            || to_sync_cycle.is_some();

        if immediately {
            self.handle_transition(mode);
            if let (Some(cycle), Some(sync)) = (to_sync_cycle, self.sync_source) {
                let pos = sync.position + cycle * sync.length;
                if mode == LoopMode::Recording {
                    self.set_position(0);
                    self.set_length(pos);
                } else {
                    self.set_position(pos);
                }
            }
            self.planned_modes.clear();
            self.planned_countdowns.clear();
        } else {
            // Insert in delay order, dropping anything planned for later: a
            // nearer transition supersedes the ones behind it.
            let delay = n_cycles_delay.unwrap_or(0) as i32;
            let insertion_point = self
                .planned_countdowns
                .iter()
                .position(|c| *c >= delay)
                .unwrap_or(self.planned_countdowns.len());

            if insertion_point >= self.planned_countdowns.len() {
                self.planned_countdowns.push_back(delay);
                self.planned_modes.push_back(mode);
            } else {
                self.planned_countdowns[insertion_point] = delay;
                self.planned_modes[insertion_point] = mode;
                self.planned_countdowns.truncate(insertion_point + 1);
                self.planned_modes.truncate(insertion_point + 1);
            }
        }
        self.update_trigger_eta();
        self.publish_state();
    }

    // --- setters ---

    pub fn set_position(&mut self, position: u32) {
        if position == self.position {
            return;
        }
        self.next_poi = None;
        self.next_trigger = None;
        self.position = position;
        self.update_poi();
        self.update_trigger_eta();
        self.publish_state();
    }

    pub fn set_length(&mut self, length: u32) {
        if length == self.length {
            return;
        }
        self.length = length;
        if self.position >= length {
            self.set_position(if length == 0 {
                0
            } else {
                self.position % length
            });
        }
        self.next_poi = None;
        self.next_trigger = None;
        self.update_poi();
        self.update_trigger_eta();
        self.publish_state();
    }

    pub fn set_mode(&mut self, mode: LoopMode) {
        self.handle_transition(mode);
    }
}

/// Merges two candidate POIs: the earlier one wins, coincident ones union their
/// flags.
pub fn dominant_poi(
    a: Option<PointOfInterest>,
    b: Option<PointOfInterest>,
) -> Option<PointOfInterest> {
    match (a, b) {
        (Some(a), Some(b)) if a.when == b.when => Some(PointOfInterest {
            when: a.when,
            flags: a.flags.with(b.flags),
        }),
        (Some(a), Some(b)) => Some(if a.when < b.when { a } else { b }),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

/// [`dominant_poi`] folded over many candidates.
pub fn dominant_poi_of(pois: &[Option<PointOfInterest>]) -> Option<PointOfInterest> {
    pois.iter().copied().fold(None, dominant_poi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::check;

    /// source. Stopped, not triggering, no ETA.
    fn idle_sync_source() -> Option<SyncSourceState> {
        Some(SyncSourceState::default())
    }

    #[test]
    fn stop() {
        let mut l = BasicLoop::default();

        check!(l.mode() == LoopMode::Stopped);
        check!(l.next_poi() == None);
        check!(l.length() == 0);
        check!(l.position() == 0);

        l.process(1000);

        check!(l.mode() == LoopMode::Stopped);
        check!(l.next_poi() == None);
        check!(l.length() == 0);
        check!(l.position() == 0);
    }

    #[test]
    fn record() {
        let mut l = BasicLoop::default();
        l.set_mode(LoopMode::Recording);
        l.update_poi();

        check!(l.mode() == LoopMode::Recording);
        check!(l.next_poi() == None);
        check!(l.length() == 0);
        check!(l.position() == 0);

        l.process(20);

        check!(l.mode() == LoopMode::Recording);
        check!(l.next_poi() == None);
        check!(l.length() == 20);
        check!(l.position() == 0);
    }

    #[test]
    fn planned_transition() {
        let mut l = BasicLoop::default();
        l.set_sync_source(idle_sync_source());
        l.set_mode(LoopMode::Recording);
        l.set_length(10);
        l.update_poi();

        l.plan_transition(LoopMode::Playing, Some(0), None);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.trigger(true);

        check!(l.mode() == LoopMode::Playing);
        check!(l.next_poi().unwrap_or(999) == 10); // end of loop
    }

    #[test]
    fn planned_transition_delayed() {
        let mut l = BasicLoop::default();
        l.set_sync_source(idle_sync_source());
        l.set_mode(LoopMode::Recording);
        l.set_length(10);
        l.update_poi();

        l.plan_transition(LoopMode::Playing, Some(1), None);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.trigger(true);
        l.process(1); // cannot trigger twice in the same cycle

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.trigger(true);

        check!(l.mode() == LoopMode::Playing);
        check!(l.next_poi().unwrap_or(999) == 11); // end of loop
    }

    #[test]
    fn planned_transitions_delayed() {
        let mut l = BasicLoop::default();
        l.set_sync_source(idle_sync_source());
        l.set_mode(LoopMode::Recording);
        l.set_length(10);
        l.update_poi();

        l.plan_transition(LoopMode::Playing, Some(1), None);
        l.plan_transition(LoopMode::Recording, Some(3), None);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.trigger(true);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.process(1);
        l.trigger(true);

        check!(l.mode() == LoopMode::Playing);
        check!(l.next_poi().unwrap_or(999) == 11);

        l.process(1);
        l.trigger(true);

        check!(l.mode() == LoopMode::Playing);
        check!(l.next_poi().unwrap_or(999) == 10);

        l.process(1);
        l.trigger(true);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);
    }

    #[test]
    fn planned_transitions_cancellation_1() {
        let mut l = BasicLoop::default();
        l.set_sync_source(idle_sync_source());
        l.set_mode(LoopMode::Recording);
        l.set_length(10);
        l.update_poi();

        l.plan_transition(LoopMode::Playing, Some(3), None);
        l.plan_transition(LoopMode::Stopped, Some(2), None);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.trigger(true);
        l.process(1);
        l.trigger(true);

        check!(l.next_poi().unwrap_or(999) == 999);
        check!(l.mode() == LoopMode::Recording);

        l.process(1);
        l.trigger(true);

        check!(l.mode() == LoopMode::Stopped);
        check!(l.next_poi().unwrap_or(999) == 999);

        l.process(1);
        l.trigger(true);
        l.process(1);
        l.trigger(true);

        check!(l.mode() == LoopMode::Stopped);
        check!(l.next_poi().unwrap_or(999) == 999);
    }

    #[test]
    fn generate_trigger() {
        let mut l = BasicLoop::default();
        l.set_mode(LoopMode::Stopped);
        l.set_length(10);
        l.set_position(0);

        check!(l.is_triggering_now() == false);
        l.trigger(true);
        check!(l.is_triggering_now() == true);
    }

    #[test]
    fn generate_trigger_on_restart() {
        let mut l = BasicLoop::default();
        check!(l.is_triggering_now() == false);

        l.set_length(10);
        l.set_mode(LoopMode::Playing);
        l.process(1);

        check!(l.is_triggering_now() == false);

        l.update_poi();
        l.process(8);

        check!(l.is_triggering_now() == false);

        l.process(1);
        check!(l.is_triggering_now() == true);
        check!(l.state_mirror().read().cycle_count == 1);

        l.handle_poi();

        check!(l.position() == 0);
        check!(l.state_mirror().read().cycle_count == 1);

        l.process(5);

        check!(l.is_triggering_now() == false);
    }

    #[test]
    fn playback_0_length() {
        let mut l = BasicLoop::default();
        l.set_mode(LoopMode::Playing);
        l.set_length(0);
        l.set_position(0);

        l.update_poi();
        l.process(10);

        check!(l.mode() == LoopMode::Stopped);
    }

    #[test]
    fn dominant_poi_prefers_earlier_and_unions_coincident() {
        let a = PointOfInterest {
            when: 5,
            flags: PoiFlags::TRIGGER,
        };
        let b = PointOfInterest {
            when: 7,
            flags: PoiFlags::LOOP_END,
        };
        check!(dominant_poi(Some(a), Some(b)) == Some(a));
        check!(dominant_poi(Some(b), Some(a)) == Some(a));
        check!(dominant_poi(Some(a), None) == Some(a));
        check!(dominant_poi(None, Some(b)) == Some(b));
        check!(dominant_poi(None, None) == None);

        let c = PointOfInterest {
            when: 5,
            flags: PoiFlags::LOOP_END,
        };
        check!(
            dominant_poi(Some(a), Some(c))
                == Some(PointOfInterest {
                    when: 5,
                    flags: PoiFlags(PoiFlags::TRIGGER.0 | PoiFlags::LOOP_END.0),
                })
        );
    }

    #[test]
    fn dominant_poi_of_folds() {
        let a = PointOfInterest {
            when: 9,
            flags: PoiFlags::TRIGGER,
        };
        let b = PointOfInterest {
            when: 3,
            flags: PoiFlags::LOOP_END,
        };
        check!(dominant_poi_of(&[]) == None);
        check!(dominant_poi_of(&[Some(a)]) == Some(a));
        check!(dominant_poi_of(&[Some(a), None, Some(b)]) == Some(b));
    }

    #[test]
    #[should_panic(expected = "beyond its next POI")]
    fn processing_past_poi_panics() {
        let mut l = BasicLoop::default();
        l.set_length(10);
        l.set_mode(LoopMode::Playing);
        l.update_poi();
        l.process(11);
    }

    #[test]
    fn nothing_pending_once_position_reaches_length() {
        let mut l = BasicLoop::default();
        l.set_length(10);
        l.set_mode(LoopMode::Playing);
        l.set_position(10);
        l.update_poi();
        // Strictly `position < length`: at the very end there is no remaining
        // span, so neither a loop-end POI nor a trigger ETA may be produced.
        check!(l.next_poi() == None);
        check!(l.predicted_next_trigger_eta() == None);

        l.set_position(9);
        l.update_poi();
        check!(l.next_poi() == Some(1));
        check!(l.predicted_next_trigger_eta() == Some(1));
    }

    #[test]
    fn playing_to_playing_preserves_position() {
        let mut l = BasicLoop::default();
        l.set_length(10);
        l.set_mode(LoopMode::Playing);
        l.process(4);
        check!(l.position() == 4);

        // Playing -> Replacing stays within playing modes: position survives.
        l.set_mode(LoopMode::Replacing);
        check!(l.position() == 4);
        l.set_mode(LoopMode::PlayingDryThroughWet);
        check!(l.position() == 4);

        // Leaving the playing modes rewinds.
        l.set_mode(LoopMode::Stopped);
        check!(l.position() == 0);
    }

    #[test]
    fn recording_resets_length() {
        let mut l = BasicLoop::default();
        l.set_length(10);
        l.set_mode(LoopMode::Playing);
        l.process(4);
        l.set_mode(LoopMode::Recording);
        check!(l.length() == 0);
        check!(l.position() == 0);
    }

    #[test]
    fn unsynced_loop_transitions_immediately() {
        let mut l = BasicLoop::default();
        l.set_mode(LoopMode::Recording);
        l.set_length(10);
        // No sync source and not Playing: the delay is ignored.
        l.plan_transition(LoopMode::Stopped, Some(5), None);
        check!(l.mode() == LoopMode::Stopped);
        check!(l.n_planned_transitions() == 0);
    }
}
