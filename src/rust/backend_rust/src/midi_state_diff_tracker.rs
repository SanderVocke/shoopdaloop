use crate::midi_state_tracker::{MidiStateTracker, Subscriber};
use std::sync::atomic::{AtomicU64, Ordering};

const NOTE_INACTIVE: u8 = 0x80;
const CC_VALUE_UNKNOWN: u8 = 0x80;
const PROGRAM_UNKNOWN: u8 = 0x80;
const CHANNEL_PRESSURE_UNKNOWN: u8 = 0x80;
const PITCH_WHEEL_UNKNOWN: u16 = 0x8000;

/// Action constants for reset_with_ptrs, matching C++ StateDiffTrackerAction.
pub const ACTION_NONE: i32 = 0;
pub const ACTION_CLEAR_DIFF: i32 = 1;
pub const ACTION_SCAN_DIFF: i32 = 2;

static NEXT_DIFF_TRACKER_ID: AtomicU64 = AtomicU64::new(1);

/// MidiStateDiffTracker tracks differences between two MidiStateTracker instances.
/// It implements Subscriber to receive synchronous state change callbacks.
pub struct MidiStateDiffTracker {
    id: u64,
    tracker_a: *mut MidiStateTracker,
    tracker_a_id: u64,
    tracker_b: *mut MidiStateTracker,
    tracker_b_id: u64,
    /// Flat sorted array of diffs. Kept sorted for O(log n) lookup and cache-friendly iteration.
    /// Pre-allocated to avoid audio-thread allocations.
    diffs: Vec<[u8; 2]>,
}

impl MidiStateDiffTracker {
    pub fn new() -> Self {
        let id = NEXT_DIFF_TRACKER_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            tracker_a: std::ptr::null_mut(),
            tracker_a_id: 0,
            tracker_b: std::ptr::null_mut(),
            tracker_b_id: 0,
            diffs: Vec::with_capacity(4096),
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Bridge adapter: takes raw byte pointers instead of *mut MidiStateTracker
    /// to avoid cross-bridge type conflicts in cxx.
    pub unsafe fn do_reset_with_ptrs(&mut self, a: *mut u8, b: *mut u8, action: i32) {
        self.reset_with_ptrs(
            a as *mut MidiStateTracker,
            b as *mut MidiStateTracker,
            action,
        );
    }

    /// Bridge adapter: takes raw byte pointer instead of &mut MidiStateTracker
    /// to avoid cross-bridge type conflicts in cxx.
    pub unsafe fn do_resolve_to_wrapper(
        &mut self,
        to: *mut u8,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        self.resolve_to_wrapper(
            &mut *(to as *mut MidiStateTracker),
            notes,
            controls,
            programs,
        )
    }

    // ---- flat diff set helpers (no allocations on audio thread) ----

    fn diffs_insert(&mut self, key: [u8; 2]) {
        match self.diffs.binary_search(&key) {
            Ok(_) => {} // already present
            Err(idx) => self.diffs.insert(idx, key),
        }
    }

    fn diffs_remove(&mut self, key: &[u8; 2]) {
        if let Ok(idx) = self.diffs.binary_search(key) {
            self.diffs.remove(idx);
        }
    }

    // ----------------------------------------------------------------

    fn tracker_a_ref(&self) -> Option<&MidiStateTracker> {
        if self.tracker_a.is_null() {
            None
        } else {
            Some(unsafe { &*self.tracker_a })
        }
    }

    fn tracker_b_ref(&self) -> Option<&MidiStateTracker> {
        if self.tracker_b.is_null() {
            None
        } else {
            Some(unsafe { &*self.tracker_b })
        }
    }

    /// Reset with two trackers.
    /// Matches C++ order: unsubscribe old, assign new, subscribe new, action.
    pub unsafe fn reset_with_ptrs(
        &mut self,
        a: *mut MidiStateTracker,
        b: *mut MidiStateTracker,
        action: i32,
    ) {
        // Unsubscribe from old trackers
        if !self.tracker_a.is_null() {
            unsafe {
                (*self.tracker_a).unsubscribe(self as *mut dyn Subscriber);
            }
        }
        if !self.tracker_b.is_null() {
            unsafe {
                (*self.tracker_b).unsubscribe(self as *mut dyn Subscriber);
            }
        }

        // Assign new trackers
        self.tracker_a = a;
        self.tracker_b = b;

        // Cache IDs
        self.tracker_a_id = if a.is_null() {
            0
        } else {
            unsafe { (*a).get_id() }
        };
        self.tracker_b_id = if b.is_null() {
            0
        } else {
            unsafe { (*b).get_id() }
        };

        // Subscribe to new trackers
        if !self.tracker_a.is_null() {
            unsafe {
                (*self.tracker_a).subscribe(self as *mut dyn Subscriber);
            }
        }
        if !self.tracker_b.is_null() {
            unsafe {
                (*self.tracker_b).subscribe(self as *mut dyn Subscriber);
            }
        }

        // Action
        match action {
            0 => self.rescan_diff(),
            1 => self.clear_diff(),
            _ => {}
        }
    }

    pub fn add_diff(&mut self, d0: u8, d1: u8) {
        self.diffs_insert([d0, d1]);
    }

    pub fn delete_diff(&mut self, d0: u8, d1: u8) {
        self.diffs_remove(&[d0, d1]);
    }

    pub fn check_note(&mut self, channel: u8, note: u8) {
        let a = match self.tracker_a_ref() {
            Some(t) => t,
            None => return,
        };
        let b = match self.tracker_b_ref() {
            Some(t) => t,
            None => return,
        };
        if !a.tracking_notes() || !b.tracking_notes() {
            return;
        }
        let av = a.maybe_current_note_velocity(channel, note);
        let bv = b.maybe_current_note_velocity(channel, note);
        if av != bv {
            self.diffs_insert([0x90 | channel, note]);
        } else {
            self.diffs_remove(&[0x90 | channel, note]);
        }
    }

    pub fn check_cc(&mut self, channel: u8, controller: u8) {
        let a = match self.tracker_a_ref() {
            Some(t) => t,
            None => return,
        };
        let b = match self.tracker_b_ref() {
            Some(t) => t,
            None => return,
        };
        if !a.tracking_controls() || !b.tracking_controls() {
            return;
        }
        let av = a.maybe_cc_value(channel, controller);
        let bv = b.maybe_cc_value(channel, controller);
        if av != bv {
            self.diffs_insert([0xB0 | channel, controller]);
        } else {
            self.diffs_remove(&[0xB0 | channel, controller]);
        }
    }

    pub fn check_program(&mut self, channel: u8) {
        let a = match self.tracker_a_ref() {
            Some(t) => t,
            None => return,
        };
        let b = match self.tracker_b_ref() {
            Some(t) => t,
            None => return,
        };
        if !a.tracking_programs() || !b.tracking_programs() {
            return;
        }
        let av = a.maybe_program_value(channel);
        let bv = b.maybe_program_value(channel);
        if av != bv {
            self.diffs_insert([0xC0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xC0 | channel, 0]);
        }
    }

    pub fn check_channel_pressure(&mut self, channel: u8) {
        let a = match self.tracker_a_ref() {
            Some(t) => t,
            None => return,
        };
        let b = match self.tracker_b_ref() {
            Some(t) => t,
            None => return,
        };
        if !a.tracking_controls() || !b.tracking_controls() {
            return;
        }
        let av = a.maybe_channel_pressure_value(channel);
        let bv = b.maybe_channel_pressure_value(channel);
        if av != bv {
            self.diffs_insert([0xD0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xD0 | channel, 0]);
        }
    }

    pub fn check_pitch_wheel(&mut self, channel: u8) {
        let a = match self.tracker_a_ref() {
            Some(t) => t,
            None => return,
        };
        let b = match self.tracker_b_ref() {
            Some(t) => t,
            None => return,
        };
        if !a.tracking_controls() || !b.tracking_controls() {
            return;
        }
        let av = a.maybe_pitch_wheel_value(channel);
        let bv = b.maybe_pitch_wheel_value(channel);
        if av != bv {
            self.diffs_insert([0xE0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xE0 | channel, 0]);
        }
    }

    pub fn rescan_diff(&mut self) {
        self.diffs.clear();
        let a_notes = self
            .tracker_a_ref()
            .map(|t| t.tracking_notes())
            .unwrap_or(false);
        let b_notes = self
            .tracker_b_ref()
            .map(|t| t.tracking_notes())
            .unwrap_or(false);
        let a_ctrls = self
            .tracker_a_ref()
            .map(|t| t.tracking_controls())
            .unwrap_or(false);
        let b_ctrls = self
            .tracker_b_ref()
            .map(|t| t.tracking_controls())
            .unwrap_or(false);
        let a_progs = self
            .tracker_a_ref()
            .map(|t| t.tracking_programs())
            .unwrap_or(false);
        let b_progs = self
            .tracker_b_ref()
            .map(|t| t.tracking_programs())
            .unwrap_or(false);

        if a_notes && b_notes {
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    self.check_note(channel, note);
                }
            }
        }
        if a_ctrls && b_ctrls {
            for channel in 0..16u8 {
                for controller in 0..128u8 {
                    self.check_cc(channel, controller);
                }
                self.check_pitch_wheel(channel);
                self.check_channel_pressure(channel);
            }
        }
        if a_progs && b_progs {
            for channel in 0..16u8 {
                self.check_program(channel);
            }
        }
    }

    pub fn clear_diff(&mut self) {
        self.diffs.clear();
    }

    pub fn resolve_to_wrapper(
        &mut self,
        to: &mut MidiStateTracker,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        let to_id = to.get_id();

        if self.tracker_a.is_null() || self.tracker_b.is_null() {
            return Vec::new();
        }

        let from_tracker = if to_id == self.tracker_a_id {
            self.tracker_b_ref()
        } else if to_id == self.tracker_b_id {
            self.tracker_a_ref()
        } else {
            return Vec::new();
        };

        let from = match from_tracker {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut out = Vec::new();

        for diff in &self.diffs {
            let kind_part = diff[0] & 0xF0;
            let channel_part = diff[0] & 0x0F;

            match kind_part {
                0x90 => {
                    if notes {
                        let v = from.maybe_current_note_velocity(channel_part, diff[1]);
                        let status_byte = if v != NOTE_INACTIVE {
                            diff[0]
                        } else {
                            0x80 | channel_part
                        };
                        let velocity = if v != NOTE_INACTIVE { v } else { 64 };
                        out.push(status_byte);
                        out.push(diff[1]);
                        out.push(velocity);
                    }
                }
                0xB0 => {
                    if controls {
                        let v = from.maybe_cc_value(channel_part, diff[1]);
                        let out_val = if v != CC_VALUE_UNKNOWN { v } else { 0 };
                        out.push(diff[0]);
                        out.push(diff[1]);
                        out.push(out_val);
                    }
                }
                0xC0 => {
                    if programs {
                        let v = from.maybe_program_value(channel_part);
                        if v != PROGRAM_UNKNOWN {
                            out.push(diff[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xD0 => {
                    if controls {
                        let v = from.maybe_channel_pressure_value(channel_part);
                        if v != CHANNEL_PRESSURE_UNKNOWN {
                            out.push(diff[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xE0 => {
                    if controls {
                        let v = from.maybe_pitch_wheel_value(channel_part);
                        if v != PITCH_WHEEL_UNKNOWN {
                            let lsb = (v & 0x7F) as u8;
                            let msb = ((v >> 7) & 0x7F) as u8;
                            out.push(diff[0]);
                            out.push(lsb);
                            out.push(msb);
                        }
                    }
                }
                _ => {}
            }
        }

        out
    }

    pub fn get_diff_flat(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.diffs.len() * 2);
        for d in &self.diffs {
            out.push(d[0]);
            out.push(d[1]);
        }
        out
    }

    pub fn set_diff_flat(&mut self, data: &[u8]) {
        self.diffs.clear();
        for chunk in data.chunks_exact(2) {
            self.diffs_insert([chunk[0], chunk[1]]);
        }
    }

    pub fn get_a_id(&self) -> Option<u64> {
        if self.tracker_a_id != 0 {
            Some(self.tracker_a_id)
        } else {
            None
        }
    }

    pub fn get_b_id(&self) -> Option<u64> {
        if self.tracker_b_id != 0 {
            Some(self.tracker_b_id)
        } else {
            None
        }
    }
}

impl Subscriber for MidiStateDiffTracker {
    fn note_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        note: u8,
        _maybe_velocity: Option<u8>,
    ) {
        let (sender, other) = if from as *const _ == self.tracker_a {
            (self.tracker_a_ref(), self.tracker_b_ref())
        } else if from as *const _ == self.tracker_b {
            (self.tracker_b_ref(), self.tracker_a_ref())
        } else {
            return;
        };

        let sender = match sender {
            Some(t) => t,
            None => return,
        };
        let other = match other {
            Some(t) => t,
            None => return,
        };

        if !sender.tracking_notes() || !other.tracking_notes() {
            self.diffs_remove(&[0x90 | channel, note]);
            return;
        }

        let sender_vel = sender.maybe_current_note_velocity(channel, note);
        let other_vel = other.maybe_current_note_velocity(channel, note);

        if sender_vel != other_vel {
            self.diffs_insert([0x90 | channel, note]);
        } else {
            self.diffs_remove(&[0x90 | channel, note]);
        }
    }

    fn cc_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        cc: u8,
        _maybe_value: Option<u8>,
    ) {
        let (sender, other) = if from as *const _ == self.tracker_a {
            (self.tracker_a_ref(), self.tracker_b_ref())
        } else if from as *const _ == self.tracker_b {
            (self.tracker_b_ref(), self.tracker_a_ref())
        } else {
            return;
        };

        let sender = match sender {
            Some(t) => t,
            None => return,
        };
        let other = match other {
            Some(t) => t,
            None => return,
        };

        if !sender.tracking_controls() || !other.tracking_controls() {
            self.diffs_remove(&[0xB0 | channel, cc]);
            return;
        }

        let sender_val = sender.maybe_cc_value(channel, cc);
        let other_val = other.maybe_cc_value(channel, cc);

        if sender_val != other_val && sender_val != CC_VALUE_UNKNOWN {
            self.diffs_insert([0xB0 | channel, cc]);
        } else {
            self.diffs_remove(&[0xB0 | channel, cc]);
        }
    }

    fn program_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        _maybe_program: Option<u8>,
    ) {
        let (sender, other) = if from as *const _ == self.tracker_a {
            (self.tracker_a_ref(), self.tracker_b_ref())
        } else if from as *const _ == self.tracker_b {
            (self.tracker_b_ref(), self.tracker_a_ref())
        } else {
            return;
        };

        let sender = match sender {
            Some(t) => t,
            None => return,
        };
        let other = match other {
            Some(t) => t,
            None => return,
        };

        if !sender.tracking_programs() || !other.tracking_programs() {
            self.diffs_remove(&[0xC0 | channel, 0]);
            return;
        }

        let sender_val = sender.maybe_program_value(channel);
        let other_val = other.maybe_program_value(channel);

        if sender_val != other_val && sender_val != PROGRAM_UNKNOWN {
            self.diffs_insert([0xC0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xC0 | channel, 0]);
        }
    }

    fn pitch_wheel_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        _maybe_pitch: Option<u16>,
    ) {
        let (sender, other) = if from as *const _ == self.tracker_a {
            (self.tracker_a_ref(), self.tracker_b_ref())
        } else if from as *const _ == self.tracker_b {
            (self.tracker_b_ref(), self.tracker_a_ref())
        } else {
            return;
        };

        let sender = match sender {
            Some(t) => t,
            None => return,
        };
        let other = match other {
            Some(t) => t,
            None => return,
        };

        if !sender.tracking_controls() || !other.tracking_controls() {
            self.diffs_remove(&[0xE0 | channel, 0]);
            return;
        }

        let sender_val = sender.maybe_pitch_wheel_value(channel);
        let other_val = other.maybe_pitch_wheel_value(channel);

        if sender_val != other_val && sender_val != PITCH_WHEEL_UNKNOWN {
            self.diffs_insert([0xE0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xE0 | channel, 0]);
        }
    }

    fn channel_pressure_changed(
        &mut self,
        from: &MidiStateTracker,
        channel: u8,
        _maybe_pressure: Option<u8>,
    ) {
        let (sender, other) = if from as *const _ == self.tracker_a {
            (self.tracker_a_ref(), self.tracker_b_ref())
        } else if from as *const _ == self.tracker_b {
            (self.tracker_b_ref(), self.tracker_a_ref())
        } else {
            return;
        };

        let sender = match sender {
            Some(t) => t,
            None => return,
        };
        let other = match other {
            Some(t) => t,
            None => return,
        };

        if !sender.tracking_controls() || !other.tracking_controls() {
            self.diffs_remove(&[0xD0 | channel, 0]);
            return;
        }

        let sender_val = sender.maybe_channel_pressure_value(channel);
        let other_val = other.maybe_channel_pressure_value(channel);

        if sender_val != other_val && sender_val != CHANNEL_PRESSURE_UNKNOWN {
            self.diffs_insert([0xD0 | channel, 0]);
        } else {
            self.diffs_remove(&[0xD0 | channel, 0]);
        }
    }
}

impl Drop for MidiStateDiffTracker {
    fn drop(&mut self) {
        if !self.tracker_a.is_null() {
            unsafe {
                (*self.tracker_a).unsubscribe(self as *mut dyn Subscriber);
            }
        }
        if !self.tracker_b.is_null() {
            unsafe {
                (*self.tracker_b).unsubscribe(self as *mut dyn Subscriber);
            }
        }
    }
}

// C++-callable module-level functions

pub fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker> {
    Box::new(MidiStateDiffTracker::new())
}

pub unsafe fn reset_with_ptrs(
    this: &mut MidiStateDiffTracker,
    a: *mut MidiStateTracker,
    b: *mut MidiStateTracker,
    action: i32,
) {
    this.reset_with_ptrs(a, b, action);
}

pub fn add_diff(this: &mut MidiStateDiffTracker, d0: u8, d1: u8) {
    this.add_diff(d0, d1);
}

pub fn delete_diff(this: &mut MidiStateDiffTracker, d0: u8, d1: u8) {
    this.delete_diff(d0, d1);
}

pub fn check_note(this: &mut MidiStateDiffTracker, channel: u8, note: u8) {
    this.check_note(channel, note);
}

pub fn check_cc(this: &mut MidiStateDiffTracker, channel: u8, controller: u8) {
    this.check_cc(channel, controller);
}

pub fn check_program(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_program(channel);
}

pub fn check_channel_pressure(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_channel_pressure(channel);
}

pub fn check_pitch_wheel(this: &mut MidiStateDiffTracker, channel: u8) {
    this.check_pitch_wheel(channel);
}

pub fn rescan_diff(this: &mut MidiStateDiffTracker) {
    this.rescan_diff();
}

pub fn clear_diff(this: &mut MidiStateDiffTracker) {
    this.clear_diff();
}

pub fn resolve_to_wrapper(
    this: &mut MidiStateDiffTracker,
    to: &mut MidiStateTracker,
    notes: bool,
    controls: bool,
    programs: bool,
) -> Vec<u8> {
    this.resolve_to_wrapper(to, notes, controls, programs)
}

pub fn get_diff_flat(this: &MidiStateDiffTracker) -> Vec<u8> {
    this.get_diff_flat()
}

pub fn set_diff_flat(this: &mut MidiStateDiffTracker, data: &[u8]) {
    this.set_diff_flat(data);
}

pub fn get_id(this: &MidiStateDiffTracker) -> u64 {
    this.get_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_msg(tracker: &mut MidiStateTracker, data: &[u8]) {
        unsafe { tracker.process_msg_raw(data.as_ptr()) }
    }

    #[test]
    fn test_channel_pressure_diff_uses_correct_status_byte() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_NONE,
            );
        }

        process_msg(&mut tracker_a, &[0xD0, 64]);
        process_msg(&mut tracker_b, &[0xD0, 64]);
        assert!(
            diff.diffs.is_empty(),
            "no diffs expected after identical state"
        );

        process_msg(&mut tracker_a, &[0xD0, 100]);

        let diffs = diff.get_diff_flat();
        assert_eq!(diffs.len(), 2, "expected exactly one diff");
        assert_eq!(
            diffs[0], 0xD0,
            "status byte should be 0xD0 (channel pressure)"
        );
        assert_eq!(diffs[1], 0);
    }

    #[test]
    fn test_channel_pressure_independent_from_pitch_wheel() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_NONE,
            );
        }

        process_msg(&mut tracker_a, &[0xD0, 50]);
        process_msg(&mut tracker_b, &[0xD0, 50]);
        process_msg(&mut tracker_a, &[0xE0, 0x00, 0x40]);
        process_msg(&mut tracker_b, &[0xE0, 0x00, 0x40]);
        assert!(diff.diffs.is_empty());

        process_msg(&mut tracker_a, &[0xD0, 75]);

        let diffs = diff.get_diff_flat();
        assert_eq!(diffs.len(), 2, "expected exactly one diff");
        assert_eq!(diffs[0], 0xD0, "diff should be for channel pressure (0xD0)");
        assert_eq!(diffs[1], 0);
    }

    #[test]
    fn test_check_channel_pressure_uses_correct_byte() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_NONE,
            );
        }

        process_msg(&mut tracker_a, &[0xD5, 42]);
        process_msg(&mut tracker_b, &[0xD5, 0]);

        diff.check_channel_pressure(5);

        let diffs = diff.get_diff_flat();
        assert_eq!(diffs.len(), 2, "expected exactly one diff");
        assert_eq!(
            diffs[0], 0xD5,
            "status byte should be 0xD5 (channel pressure on ch 5)"
        );
        assert_eq!(diffs[1], 0);
    }

    #[test]
    fn test_diff_tracks_cc_changes() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_SCAN_DIFF,
            );
        }

        assert!(diff.diffs.is_empty());

        process_msg(&mut tracker_a, &[0xB0, 7, 64]);
        let diffs = diff.get_diff_flat();
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0], 0xB0);
        assert_eq!(diffs[1], 7);

        process_msg(&mut tracker_b, &[0xB0, 7, 64]);
        assert!(diff.diffs.is_empty());
    }

    #[test]
    fn test_resolve_to_generates_messages() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_SCAN_DIFF,
            );
        }

        process_msg(&mut tracker_a, &[0xB0, 7, 64]);
        process_msg(&mut tracker_a, &[0xD0, 50]);

        let msgs = diff.resolve_to_wrapper(&mut tracker_b, false, true, false);

        // Resolve to tracker_b means: generate messages so tracker_b matches tracker_a.
        // Tracker_a has CC7=64 and channel_pressure=50, so those values are output.
        assert_eq!(msgs.len(), 6);
        assert_eq!(msgs[0], 0xB0);
        assert_eq!(msgs[1], 7);
        assert_eq!(msgs[2], 64);
        assert_eq!(msgs[3], 0xD0);
        assert_eq!(msgs[4], 0);
        assert_eq!(msgs[5], 50);
    }

    #[test]
    fn test_reset_clears_old_diffs() {
        let mut tracker_a = MidiStateTracker::new(false, true, false);
        let mut tracker_b = MidiStateTracker::new(false, true, false);
        let mut tracker_c = MidiStateTracker::new(false, true, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_SCAN_DIFF,
            );
        }

        process_msg(&mut tracker_a, &[0xB0, 7, 64]);
        assert!(!diff.diffs.is_empty());

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_c as *mut _,
                ACTION_CLEAR_DIFF,
            );
        }
        assert!(diff.diffs.is_empty());
    }

    #[test]
    fn test_note_diff_tracked_correctly() {
        let mut tracker_a = MidiStateTracker::new(true, false, false);
        let mut tracker_b = MidiStateTracker::new(true, false, false);
        let mut diff = MidiStateDiffTracker::new();

        unsafe {
            diff.reset_with_ptrs(
                &mut tracker_a as *mut _,
                &mut tracker_b as *mut _,
                ACTION_SCAN_DIFF,
            );
        }

        process_msg(&mut tracker_a, &[0x90, 60, 100]);
        let diffs = diff.get_diff_flat();
        assert_eq!(diffs.len(), 2);
        assert_eq!(diffs[0], 0x90);
        assert_eq!(diffs[1], 60);

        process_msg(&mut tracker_b, &[0x90, 60, 100]);
        assert!(diff.diffs.is_empty());
    }
}
