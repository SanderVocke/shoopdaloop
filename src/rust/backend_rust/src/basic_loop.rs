//! BasicLoop implementation in Rust.
//!
//! BasicLoop implements core loop semantics shared by all loop types:
//! points of interest (POI), mode transitions, triggers, and sync source handling.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};

/// Loop mode enum matching C++ shoop_loop_mode_t.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum LoopMode {
    Unknown = 0,
    Stopped = 1,
    Playing = 2,
    Recording = 3,
    Replacing = 4,
    PlayingDryThroughWet = 5,
    RecordingDryIntoWet = 6,
    Invalid = 7,
}

impl Default for LoopMode {
    fn default() -> Self {
        LoopMode::Stopped
    }
}

impl LoopMode {
    /// Convert from u32 value (for FFI and atomic loads).
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => LoopMode::Unknown,
            1 => LoopMode::Stopped,
            2 => LoopMode::Playing,
            3 => LoopMode::Recording,
            4 => LoopMode::Replacing,
            5 => LoopMode::PlayingDryThroughWet,
            6 => LoopMode::RecordingDryIntoWet,
            _ => LoopMode::Invalid,
        }
    }

    /// Convert to u32 value (for FFI and atomic stores).
    pub fn to_u32(self) -> u32 {
        self as u32
    }
}

/// Point of interest flags (can be combined with bitwise OR).
pub struct PointOfInterestFlags;

impl PointOfInterestFlags {
    pub const TRIGGER: u32 = 1;
    pub const LOOP_END: u32 = 2;
    pub const CHANNEL_POI: u32 = 4;
}

/// A point of interest in the loop timeline.
#[derive(Debug, Clone)]
pub struct PointOfInterest {
    /// When the POI occurs (samples from current position).
    pub when: u32,
    /// Type flags (combination of PointOfInterestFlags).
    pub type_flags: u32,
}

/// Check if a loop mode is a "playing" mode.
/// Playing modes are: Playing, Replacing, PlayingDryThroughWet, RecordingDryIntoWet.
pub fn is_playing_mode(mode: LoopMode) -> bool {
    matches!(
        mode,
        LoopMode::Playing
            | LoopMode::Replacing
            | LoopMode::PlayingDryThroughWet
            | LoopMode::RecordingDryIntoWet
    )
}

/// Core state for BasicLoop.
///
/// This struct contains all state for BasicLoop, split into:
/// - Atomic state (ma_* prefix): Can be accessed from any thread.
/// - Process-thread state (mp_* prefix): Only accessed from process thread.
///
/// The design mirrors the C++ BasicLoop class but uses Rust atomics
/// and collections.
pub struct BasicLoopCore {
    // Process-thread only state (no atomics needed - single threaded access)
    /// Next point of interest.
    pub mp_next_poi: Option<PointOfInterest>,
    /// Next trigger ETA.
    pub mp_next_trigger: Option<u32>,
    /// Sync source (stored as raw pointer usize, C++ manages lifetime).
    pub mp_sync_source: usize,
    /// Planned mode transitions queue.
    pub mp_planned_states: VecDeque<LoopMode>,
    /// Countdowns for planned transitions (in trigger cycles).
    pub mp_planned_state_countdowns: VecDeque<i32>,

    // Atomic state (thread-safe access)
    /// Current loop mode.
    pub ma_mode: AtomicU32,
    /// Whether the loop is triggering now.
    pub ma_triggering_now: AtomicBool,
    /// Whether already triggered in this cycle.
    pub ma_already_triggered: AtomicBool,
    /// Loop length in samples.
    pub ma_length: AtomicU32,
    /// Current position in samples.
    pub ma_position: AtomicU32,
    /// Cached next planned mode.
    pub ma_maybe_next_planned_mode: AtomicU32,
    /// Cached next planned delay (-1 means none).
    pub ma_maybe_next_planned_delay: AtomicI32,
}

impl Default for BasicLoopCore {
    fn default() -> Self {
        Self::new()
    }
}

impl BasicLoopCore {
    /// Create a new BasicLoopCore with default state.
    pub fn new() -> Self {
        Self {
            mp_next_poi: None,
            mp_next_trigger: None,
            mp_sync_source: 0,
            mp_planned_states: VecDeque::new(),
            mp_planned_state_countdowns: VecDeque::new(),

            ma_mode: AtomicU32::new(LoopMode::Stopped.to_u32()),
            ma_triggering_now: AtomicBool::new(false),
            ma_already_triggered: AtomicBool::new(false),
            ma_length: AtomicU32::new(0),
            ma_position: AtomicU32::new(0),
            ma_maybe_next_planned_mode: AtomicU32::new(LoopMode::Unknown.to_u32()),
            ma_maybe_next_planned_delay: AtomicI32::new(-1),
        }
    }

    // ========================================================================
    // Basic Getters/Setters (Phase 2)
    // ========================================================================

    /// Get current position (thread-safe via atomic load).
    pub fn get_position(&self) -> u32 {
        self.ma_position.load(Ordering::Relaxed)
    }

    /// Set position (process-thread only - invalidates POI).
    pub fn set_position(&mut self, position: u32) {
        if position != self.ma_position.load(Ordering::Relaxed) {
            self.mp_next_poi = None;
            self.mp_next_trigger = None;
            self.ma_position.store(position, Ordering::Relaxed);
            self.proc_update_poi();
            self.proc_update_trigger_eta();
        }
    }

    /// Get current length (thread-safe via atomic load).
    pub fn get_length(&self) -> u32 {
        self.ma_length.load(Ordering::Relaxed)
    }

    /// Set length (process-thread only - invalidates POI).
    pub fn set_length(&mut self, len: u32) {
        if len != self.ma_length.load(Ordering::Relaxed) {
            self.ma_length.store(len, Ordering::Relaxed);
            let position = self.ma_position.load(Ordering::Relaxed);
            if position >= len {
                let new_pos = if len == 0 { 0 } else { len - 1 };
                self.set_position(new_pos);
            }
            self.mp_next_poi = None;
            self.mp_next_trigger = None;
            self.proc_update_poi();
            self.proc_update_trigger_eta();
        }
    }

    /// Get current mode (thread-safe via atomic load).
    pub fn get_mode(&self) -> LoopMode {
        LoopMode::from_u32(self.ma_mode.load(Ordering::Relaxed))
    }

    /// Set mode (process-thread only - calls handle_transition).
    pub fn set_mode(&mut self, mode: LoopMode) {
        self.proc_handle_transition(mode);
    }

    // ========================================================================
    // POI Logic (Phase 3)
    // ========================================================================

    /// Get the next point of interest (when it will occur).
    pub fn proc_get_next_poi(&self) -> Option<u32> {
        self.mp_next_poi.as_ref().map(|poi| poi.when)
    }

    /// Get predicted next trigger ETA.
    pub fn proc_predicted_next_trigger_eta(&self) -> Option<u32> {
        self.mp_next_trigger
    }

    /// Update the trigger ETA based on current state.
    pub fn proc_update_trigger_eta(&mut self) {
        let mode = self.get_mode();
        let position = self.ma_position.load(Ordering::Relaxed);
        let length = self.ma_length.load(Ordering::Relaxed);

        if is_playing_mode(mode) && position < length {
            self.mp_next_trigger = Some(length - position);
        } else {
            self.mp_next_trigger = None;
        }

        // Handle sync source - this requires calling back into C++ for now
        // The sync source pointer is checked via a callback mechanism
        // For now, we just set mp_next_trigger based on our own state
        // The C++ wrapper will handle the sync source case
    }

    /// Update the next point of interest based on current state.
    pub fn proc_update_poi(&mut self) {
        let mode = self.get_mode();
        let length = self.ma_length.load(Ordering::Relaxed);

        // If playing mode with zero length, stop
        if is_playing_mode(mode) && length == 0 {
            self.proc_handle_transition(LoopMode::Stopped);
        }

        // Clear LoopEnd and ChannelPOI flags from existing POI
        if let Some(ref mut poi) = self.mp_next_poi {
            poi.type_flags &= !(PointOfInterestFlags::LOOP_END | PointOfInterestFlags::CHANNEL_POI);
            if poi.type_flags == 0 {
                self.mp_next_poi = None;
            }
        }

        // Add loop end POI if applicable
        let position = self.ma_position.load(Ordering::Relaxed);
        if is_playing_mode(mode) && position < length {
            let loop_end_poi = PointOfInterest {
                when: length - position,
                type_flags: PointOfInterestFlags::LOOP_END,
            };
            self.mp_next_poi = Self::dominant_poi(self.mp_next_poi.take(), Some(loop_end_poi));
        }
    }

    /// Select the dominant (earlier or combined) POI from two options.
    pub fn dominant_poi(
        a: Option<PointOfInterest>,
        b: Option<PointOfInterest>,
    ) -> Option<PointOfInterest> {
        match (a, b) {
            (Some(poi_a), None) => Some(poi_a),
            (None, Some(poi_b)) => Some(poi_b),
            (None, None) => None,
            (Some(poi_a), Some(poi_b)) => {
                if poi_a.when == poi_b.when {
                    Some(PointOfInterest {
                        when: poi_a.when,
                        type_flags: poi_a.type_flags | poi_b.type_flags,
                    })
                } else if poi_a.when < poi_b.when {
                    Some(poi_a)
                } else {
                    Some(poi_b)
                }
            }
        }
    }

    /// Select the dominant POI from a vector of options.
    pub fn dominant_poi_vec(pois: Vec<Option<PointOfInterest>>) -> Option<PointOfInterest> {
        if pois.is_empty() {
            return None;
        }
        if pois.len() == 1 {
            return pois[0].clone();
        }
        if pois.len() == 2 {
            return Self::dominant_poi(pois[0].clone(), pois[1].clone());
        }
        // Recursive reduction
        let reduced = pois[1..].to_vec();
        Self::dominant_poi(pois[0].clone(), Self::dominant_poi_vec(reduced))
    }

    // ========================================================================
    // Trigger and Transition Logic (Phase 4)
    // ========================================================================

    /// Trigger the loop - execute planned transitions.
    pub fn proc_trigger(&mut self, propagate: bool) {
        if self.ma_already_triggered.load(Ordering::Relaxed) {
            return;
        }
        self.ma_already_triggered.store(true, Ordering::Relaxed);

        if propagate {
            self.ma_triggering_now.store(true, Ordering::Relaxed);
        }

        let mode = self.get_mode();
        let length = self.ma_length.load(Ordering::Relaxed);
        let position = self.ma_position.load(Ordering::Relaxed);

        if is_playing_mode(mode) && position >= length {
            self.ma_position.store(0, Ordering::Relaxed);
        }

        // Decrement all countdowns
        for elem in self.mp_planned_state_countdowns.iter_mut() {
            *elem -= 1;
        }

        // Process transitions with negative countdowns
        while !self.mp_planned_state_countdowns.is_empty()
            && self.mp_planned_state_countdowns[0] < 0
        {
            let new_state = self.mp_planned_states.pop_front().unwrap();
            self.mp_planned_state_countdowns.pop_front();
            self.proc_handle_transition(new_state);
        }

        self.proc_update_planned_transition_cache();
    }

    /// Handle a mode transition.
    pub fn proc_handle_transition(&mut self, new_state: LoopMode) {
        let current_mode = self.get_mode();

        if current_mode != new_state {
            let from_playing_to_playing =
                is_playing_mode(current_mode) && is_playing_mode(new_state);

            if !from_playing_to_playing {
                self.set_position(0);
            }

            if new_state == LoopMode::Recording {
                self.set_length(0);
            }

            self.ma_mode.store(new_state.to_u32(), Ordering::Relaxed);

            if new_state == LoopMode::Invalid {
                panic!("invalid mode");
            }

            if new_state == LoopMode::Stopped {
                self.ma_position.store(0, Ordering::Relaxed);
            }

            let position = self.ma_position.load(Ordering::Relaxed);
            if is_playing_mode(new_state) && position == 0 {
                self.ma_triggering_now.store(true, Ordering::Relaxed);
            }

            self.mp_next_poi = None;
            self.proc_update_poi();
            self.proc_update_trigger_eta();
        }
    }

    /// Handle reaching a point of interest.
    pub fn proc_handle_poi(&mut self) {
        // Only handle POIs that are reached right now
        if self.mp_next_poi.is_none() {
            return;
        }
        let poi_when = self.mp_next_poi.as_ref().unwrap().when;
        if poi_when != 0 {
            return;
        }

        let mut changed = false;

        // Clear ChannelPOI flag
        if let Some(ref mut poi) = self.mp_next_poi {
            poi.type_flags &= !PointOfInterestFlags::CHANNEL_POI;
        }

        // Handle Trigger flag - extract flags first to avoid borrow issues
        let trigger_present = self
            .mp_next_poi
            .as_ref()
            .map(|poi| poi.type_flags & PointOfInterestFlags::TRIGGER != 0)
            .unwrap_or(false);

        if trigger_present {
            self.proc_trigger(true);
            if let Some(ref mut poi) = self.mp_next_poi {
                poi.type_flags &= !PointOfInterestFlags::TRIGGER;
            }
            changed = true;
        }

        // Handle LoopEnd flag
        let loop_end_present = self
            .mp_next_poi
            .as_ref()
            .map(|poi| poi.type_flags & PointOfInterestFlags::LOOP_END != 0)
            .unwrap_or(false);

        if loop_end_present {
            if let Some(ref mut poi) = self.mp_next_poi {
                poi.type_flags &= !PointOfInterestFlags::LOOP_END;
            }
            // If no sync source or sync source not playing, trigger ourselves
            // The sync source check is handled externally (C++ wrapper)
            // For now, we trigger unconditionally
            self.proc_trigger(true);
            changed = true;
        }

        // Clear empty POI
        let should_clear = self
            .mp_next_poi
            .as_ref()
            .map(|poi| poi.type_flags == 0)
            .unwrap_or(false);
        if should_clear {
            self.mp_next_poi = None;
            changed = true;
        }

        if changed {
            self.proc_update_poi();
            self.proc_update_trigger_eta();
        }
    }

    /// Check if triggering now.
    pub fn proc_is_triggering_now(&mut self) -> bool {
        if self.mp_next_poi.is_some() && self.mp_next_poi.as_ref().unwrap().when == 0 {
            self.proc_handle_poi();
        }
        // Sync source check handled externally (C++ wrapper)
        if self.ma_triggering_now.load(Ordering::Relaxed) {
            return true;
        }
        false
    }

    // ========================================================================
    // Planned Transitions (Phase 5)
    // ========================================================================

    /// Update the cached planned transition info for lock-free reading.
    pub fn proc_update_planned_transition_cache(&mut self) {
        let next_mode = if !self.mp_planned_states.is_empty() {
            self.mp_planned_states[0].to_u32()
        } else {
            LoopMode::Unknown.to_u32()
        };
        let next_delay = if !self.mp_planned_state_countdowns.is_empty() {
            self.mp_planned_state_countdowns[0]
        } else {
            -1
        };

        self.ma_maybe_next_planned_mode
            .store(next_mode, Ordering::Relaxed);
        self.ma_maybe_next_planned_delay
            .store(next_delay, Ordering::Relaxed);
    }

    /// Get number of planned transitions.
    pub fn get_n_planned_transitions(&self) -> u32 {
        self.mp_planned_states.len() as u32
    }

    /// Get planned transition delay at index.
    pub fn get_planned_transition_delay(&self, idx: u32) -> u32 {
        let idx = idx as usize;
        if idx >= self.mp_planned_state_countdowns.len() {
            panic!("Attempted to get out-of-bounds planned transition");
        }
        self.mp_planned_state_countdowns[idx] as u32
    }

    /// Get planned transition state at index.
    pub fn get_planned_transition_state(&self, idx: u32) -> LoopMode {
        let idx = idx as usize;
        if idx >= self.mp_planned_states.len() {
            panic!("Attempted to get out-of-bounds planned transition");
        }
        self.mp_planned_states[idx]
    }

    /// Clear all planned transitions.
    pub fn clear_planned_transitions(&mut self) {
        self.mp_planned_states.clear();
        self.mp_planned_state_countdowns.clear();
        self.proc_update_planned_transition_cache();
    }

    /// Plan a mode transition.
    ///
    /// # Arguments
    /// * `mode` - The target mode
    /// * `maybe_n_cycles_delay` - Optional delay in trigger cycles
    /// * `maybe_to_sync_cycle` - Optional sync cycle alignment
    pub fn plan_transition(
        &mut self,
        mode: LoopMode,
        maybe_n_cycles_delay: Option<u32>,
        maybe_to_sync_cycle: Option<u32>,
    ) {
        let current_mode = self.get_mode();

        // Determine if we should transition immediately
        let transitioning_immediately = (self.mp_sync_source == 0
            && current_mode != LoopMode::Playing)
            || maybe_n_cycles_delay.is_none()
            || maybe_to_sync_cycle.is_some();

        if transitioning_immediately {
            // Un-synced loops transition immediately from non-playing states
            self.proc_handle_transition(mode);

            // Sync cycle handling is done externally (C++ wrapper)
            // when maybe_to_sync_cycle is set

            self.mp_planned_states.clear();
            self.mp_planned_state_countdowns.clear();
        } else {
            let n_cycles_delay = maybe_n_cycles_delay.unwrap_or(0);

            // Find insertion point
            let mut insertion_point = 0;
            for i in 0..=self.mp_planned_state_countdowns.len() {
                insertion_point = i;
                if i < self.mp_planned_state_countdowns.len()
                    && self.mp_planned_state_countdowns[i] >= n_cycles_delay as i32
                {
                    break;
                }
            }

            if insertion_point >= self.mp_planned_state_countdowns.len() {
                self.mp_planned_state_countdowns
                    .push_back(n_cycles_delay as i32);
                self.mp_planned_states.push_back(mode);
            } else {
                // Replace some planned states
                self.mp_planned_state_countdowns[insertion_point] = n_cycles_delay as i32;
                self.mp_planned_states[insertion_point] = mode;
                self.mp_planned_state_countdowns
                    .truncate(insertion_point + 1);
                self.mp_planned_states.truncate(insertion_point + 1);
            }
        }

        self.proc_update_planned_transition_cache();
        self.proc_update_trigger_eta();
    }

    /// Get the first planned transition info.
    pub fn get_first_planned_transition(&self) -> (LoopMode, u32) {
        let maybe_mode =
            LoopMode::from_u32(self.ma_maybe_next_planned_mode.load(Ordering::Relaxed));
        let maybe_delay = self.ma_maybe_next_planned_delay.load(Ordering::Relaxed);

        if maybe_delay >= 0 && maybe_mode != LoopMode::Unknown {
            (maybe_mode, maybe_delay as u32)
        } else {
            (LoopMode::Unknown, 0)
        }
    }

    // ========================================================================
    // Process Method (Phase 6)
    // ========================================================================

    /// Main processing loop.
    ///
    /// # Arguments
    /// * `n_samples` - Number of samples to process
    ///
    /// # Panics
    /// Panics if attempting to process beyond the next POI.
    pub fn proc_process(&mut self, n_samples: u32) {
        // Check for POI violation
        if let Some(ref poi) = self.mp_next_poi {
            if n_samples > poi.when {
                panic!("Attempted to process loop beyond its next POI.");
            }
        }

        // Reset triggering flags
        self.ma_triggering_now.store(false, Ordering::Relaxed);
        self.ma_already_triggered.store(false, Ordering::Relaxed);

        let pos_before = self.ma_position.load(Ordering::Relaxed);
        let mut pos_after = pos_before;
        let length_before = self.ma_length.load(Ordering::Relaxed);
        let mut length_after = length_before;

        let process_channel_mode = self.get_mode();

        // Update position/length based on mode
        match process_channel_mode {
            LoopMode::Recording => {
                length_after += n_samples;
            }
            LoopMode::Replacing => {
                pos_after += n_samples;
                length_after = std::cmp::max(length_after, pos_after);
            }
            LoopMode::Playing | LoopMode::PlayingDryThroughWet | LoopMode::RecordingDryIntoWet => {
                pos_after = std::cmp::min(pos_after + n_samples, length_after);
            }
            _ => {}
        }

        // Get planned transition info for process_channels
        let maybe_next_mode =
            LoopMode::from_u32(self.ma_maybe_next_planned_mode.load(Ordering::Relaxed));
        let maybe_next_delay = self.ma_maybe_next_planned_delay.load(Ordering::Relaxed);
        let maybe_next_mode_delay_cycles = if maybe_next_delay >= 0 {
            Some(maybe_next_delay as u32)
        } else {
            None
        };
        let maybe_next_mode_eta = if maybe_next_delay == 0 {
            self.proc_predicted_next_trigger_eta()
        } else {
            None
        };

        // Call virtual process_channels (empty in base class)
        self.proc_process_channels(
            process_channel_mode,
            if maybe_next_mode != LoopMode::Unknown {
                Some(maybe_next_mode)
            } else {
                None
            },
            maybe_next_mode_delay_cycles,
            maybe_next_mode_eta,
            n_samples,
            pos_before,
            pos_after,
            length_before,
            length_after,
        );

        // Update POI timing
        if let Some(ref mut poi) = self.mp_next_poi {
            poi.when -= n_samples;
        }

        // Update position and length
        self.ma_position.store(pos_after, Ordering::Relaxed);
        self.ma_length.store(length_after, Ordering::Relaxed);

        // Update trigger ETA
        if let Some(ref mut trigger) = self.mp_next_trigger {
            let new_val = (*trigger as i32 - n_samples as i32).max(0) as u32;
            if new_val == 0 {
                self.mp_next_trigger = None;
            } else {
                *trigger = new_val;
            }
        }

        // Handle POI
        self.proc_handle_poi();
    }

    /// Virtual method for channel processing (empty in base class).
    /// Override in derived classes (AudioMidiLoop).
    #[allow(unused_variables)]
    pub fn proc_process_channels(
        &mut self,
        mode: LoopMode,
        maybe_next_mode: Option<LoopMode>,
        maybe_next_mode_delay_cycles: Option<u32>,
        maybe_next_mode_eta: Option<u32>,
        n_samples: u32,
        pos_before: u32,
        pos_after: u32,
        length_before: u32,
        length_after: u32,
    ) {
        // Empty in base class - override in derived classes
    }

    // ========================================================================
    // Sync Source Handling (Phase 7)
    // ========================================================================

    /// Set the sync source (stored as raw pointer usize).
    pub fn set_sync_source(&mut self, ptr: usize) {
        self.mp_sync_source = ptr;
        self.proc_update_trigger_eta();
    }

    /// Get the sync source pointer.
    pub fn get_sync_source(&self) -> usize {
        self.mp_sync_source
    }
}

/// Constructor function for CXX bridge.
pub fn new_basic_loop_core() -> Box<BasicLoopCore> {
    Box::new(BasicLoopCore::new())
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loop_mode_conversions() {
        assert_eq!(LoopMode::from_u32(0), LoopMode::Unknown);
        assert_eq!(LoopMode::from_u32(1), LoopMode::Stopped);
        assert_eq!(LoopMode::from_u32(2), LoopMode::Playing);
        assert_eq!(LoopMode::from_u32(3), LoopMode::Recording);
        assert_eq!(LoopMode::from_u32(4), LoopMode::Replacing);
        assert_eq!(LoopMode::from_u32(5), LoopMode::PlayingDryThroughWet);
        assert_eq!(LoopMode::from_u32(6), LoopMode::RecordingDryIntoWet);
        assert_eq!(LoopMode::from_u32(7), LoopMode::Invalid);
        assert_eq!(LoopMode::from_u32(100), LoopMode::Invalid);
    }

    #[test]
    fn test_is_playing_mode() {
        assert!(!is_playing_mode(LoopMode::Unknown));
        assert!(!is_playing_mode(LoopMode::Stopped));
        assert!(is_playing_mode(LoopMode::Playing));
        assert!(is_playing_mode(LoopMode::Replacing));
        assert!(is_playing_mode(LoopMode::PlayingDryThroughWet));
        assert!(is_playing_mode(LoopMode::RecordingDryIntoWet));
        assert!(!is_playing_mode(LoopMode::Recording));
        assert!(!is_playing_mode(LoopMode::Invalid));
    }

    #[test]
    fn test_basic_loop_creation() {
        let core = BasicLoopCore::new();
        assert_eq!(core.get_mode(), LoopMode::Stopped);
        assert_eq!(core.get_position(), 0);
        assert_eq!(core.get_length(), 0);
        assert!(!core.ma_triggering_now.load(Ordering::Relaxed));
        assert!(!core.ma_already_triggered.load(Ordering::Relaxed));
    }

    #[test]
    fn test_position_setter() {
        let mut core = BasicLoopCore::new();
        core.set_position(100);
        assert_eq!(core.get_position(), 100);
        assert!(core.mp_next_poi.is_none());
        assert!(core.mp_next_trigger.is_none());
    }

    #[test]
    fn test_length_setter() {
        let mut core = BasicLoopCore::new();
        core.set_length(1000);
        assert_eq!(core.get_length(), 1000);

        // Position should be adjusted if >= length
        core.set_position(500);
        core.set_length(100);
        assert_eq!(core.get_position(), 99); // length - 1
    }

    #[test]
    fn test_dominant_poi() {
        let poi_a = PointOfInterest {
            when: 100,
            type_flags: PointOfInterestFlags::TRIGGER,
        };
        let poi_b = PointOfInterest {
            when: 50,
            type_flags: PointOfInterestFlags::LOOP_END,
        };

        // Earlier POI should win
        let result = BasicLoopCore::dominant_poi(Some(poi_a.clone()), Some(poi_b.clone()));
        assert_eq!(result.as_ref().unwrap().when, 50);
        assert_eq!(
            result.as_ref().unwrap().type_flags,
            PointOfInterestFlags::LOOP_END
        );

        // Same time should combine flags
        let poi_c = PointOfInterest {
            when: 100,
            type_flags: PointOfInterestFlags::TRIGGER,
        };
        let poi_d = PointOfInterest {
            when: 100,
            type_flags: PointOfInterestFlags::LOOP_END,
        };
        let result = BasicLoopCore::dominant_poi(Some(poi_c), Some(poi_d));
        assert_eq!(result.as_ref().unwrap().when, 100);
        assert_eq!(
            result.as_ref().unwrap().type_flags,
            PointOfInterestFlags::TRIGGER | PointOfInterestFlags::LOOP_END
        );
    }

    #[test]
    fn test_transition_to_recording() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        core.set_position(50);
        core.proc_handle_transition(LoopMode::Recording);

        assert_eq!(core.get_mode(), LoopMode::Recording);
        assert_eq!(core.get_length(), 0); // Recording resets length
        assert_eq!(core.get_position(), 0);
    }

    #[test]
    fn test_transition_to_stopped() {
        let mut core = BasicLoopCore::new();
        // First transition to playing mode, then to stopped
        core.set_length(100);
        core.proc_handle_transition(LoopMode::Playing);
        core.set_position(50); // Set position while playing

        // Now transition to stopped
        core.proc_handle_transition(LoopMode::Stopped);

        assert_eq!(core.get_mode(), LoopMode::Stopped);
        assert_eq!(core.get_position(), 0); // Stopped resets position
    }

    #[test]
    fn test_transition_to_playing_triggers() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        core.proc_handle_transition(LoopMode::Playing);

        assert_eq!(core.get_mode(), LoopMode::Playing);
        assert!(core.ma_triggering_now.load(Ordering::Relaxed)); // Position 0 triggers
    }

    #[test]
    fn test_poi_update_for_loop_end() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        // When transitioning to playing, position is reset to 0
        core.proc_handle_transition(LoopMode::Playing);

        // Should have a loop end POI at length - position = 100 - 0 = 100
        assert!(core.mp_next_poi.is_some());
        let poi = core.mp_next_poi.clone().unwrap();
        assert_eq!(poi.when, 100); // 100 - 0 = 100 samples to loop end
        assert_eq!(poi.type_flags, PointOfInterestFlags::LOOP_END);
    }

    #[test]
    fn test_planned_transition() {
        let mut core = BasicLoopCore::new();

        // Need sync source and non-playing mode for delayed transition
        // Set a non-zero sync source pointer (C++ would manage this)
        core.set_sync_source(0x1234);
        core.proc_handle_transition(LoopMode::Recording);
        core.set_length(100);
        core.proc_update_poi();

        // Plan a transition with delay - since we have a sync source, it won't be immediate
        core.plan_transition(LoopMode::Playing, Some(2), None);

        assert_eq!(core.get_n_planned_transitions(), 1);
        assert_eq!(core.get_planned_transition_delay(0), 2);
        assert_eq!(core.get_planned_transition_state(0), LoopMode::Playing);
    }

    #[test]
    fn test_planned_transition_immediate() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);

        // Plan with no delay - should transition immediately since not playing
        core.plan_transition(LoopMode::Recording, None, None);

        assert_eq!(core.get_n_planned_transitions(), 0);
        assert_eq!(core.get_mode(), LoopMode::Recording);
    }

    #[test]
    fn test_trigger_decrements_countdowns() {
        let mut core = BasicLoopCore::new();

        // Need sync source for delayed transitions
        core.set_sync_source(0x1234);
        core.set_length(100);
        core.proc_handle_transition(LoopMode::Playing); // Start in Playing mode (doesn't reset length)
        core.set_length(100);
        core.proc_update_poi();

        // Plan transitions with delay 0 means: next trigger executes it
        // delay 1 means: trigger once (countdown->0), then trigger again (countdown->-1) to execute
        core.plan_transition(LoopMode::Stopped, Some(0), None);
        core.plan_transition(LoopMode::Playing, Some(1), None);

        assert_eq!(core.get_n_planned_transitions(), 2);

        // Trigger once - first transition executes (countdown 0 -> -1)
        core.proc_trigger(true);
        assert_eq!(core.get_mode(), LoopMode::Stopped);
        assert_eq!(core.get_n_planned_transitions(), 1);

        // Process to reset the already_triggered flag (C++ tests do this)
        core.proc_process(1);

        // Trigger again - second transition executes (countdown 0 -> -1)
        core.proc_trigger(true);
        assert_eq!(core.get_mode(), LoopMode::Playing);
        assert_eq!(core.get_n_planned_transitions(), 0);
    }

    #[test]
    fn test_process_stopped_mode() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        core.set_position(50);

        // Process while stopped - no changes
        core.proc_process(10);

        assert_eq!(core.get_position(), 50);
        assert_eq!(core.get_length(), 100);
    }

    #[test]
    fn test_process_recording_mode() {
        let mut core = BasicLoopCore::new();
        core.proc_handle_transition(LoopMode::Recording);

        // Process while recording - length increases
        core.proc_process(100);

        assert_eq!(core.get_position(), 0);
        assert_eq!(core.get_length(), 100);
    }

    #[test]
    fn test_process_playing_mode() {
        let mut core = BasicLoopCore::new();
        core.set_length(200);
        // Transition to playing (resets position to 0)
        core.proc_handle_transition(LoopMode::Playing);
        // Now set position after we're in playing mode
        core.ma_position.store(50, Ordering::Relaxed);
        core.proc_update_poi();

        // Process while playing - position advances
        core.proc_process(100);

        assert_eq!(core.get_position(), 150);
        assert_eq!(core.get_length(), 200);
    }

    #[test]
    fn test_process_playing_hits_loop_end() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        // Transition to playing (resets position to 0)
        core.proc_handle_transition(LoopMode::Playing);

        // The POI should be at 100 samples (loop end from position 0)
        assert_eq!(core.proc_get_next_poi(), Some(100));

        // Now we need to be able to process without hitting the POI check
        // Let's first set up a position closer to the end
        core.ma_position.store(50, Ordering::Relaxed); // Set position directly
        core.proc_update_poi(); // Update POI based on new position

        // Now POI should be at 50 (100 - 50 = 50)
        assert_eq!(core.proc_get_next_poi(), Some(50));

        // Process exactly to loop end - this should work
        core.proc_process(50);

        // POI is now at when=0, which should trigger handling
        // After hitting loop end, we should trigger
    }

    #[test]
    fn test_process_beyond_poi_panics() {
        let mut core = BasicLoopCore::new();
        core.set_length(100);
        core.proc_handle_transition(LoopMode::Playing);
        // Now position is 0, POI is at 100

        // Set position closer to loop end
        core.ma_position.store(50, Ordering::Relaxed);
        core.proc_update_poi(); // POI is now at 50

        // Processing beyond POI should panic
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            core.proc_process(100);
        }));
        assert!(result.is_err());
    }
}
