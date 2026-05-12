use crate::event::{Event, TrackerStateData, EVENT_NOTE, EVENT_CC, EVENT_PROGRAM, EVENT_PITCH, EVENT_PRESSURE};
use crate::midi_state_tracker::MidiStateTracker;
use crossbeam_channel::Receiver;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const NOTE_INACTIVE: u8 = 0x80;
const CC_VALUE_UNKNOWN: u8 = 0x80;
const PROGRAM_UNKNOWN: u8 = 0x80;
const CHANNEL_PRESSURE_UNKNOWN: u8 = 0x80;
const PITCH_WHEEL_UNKNOWN: u16 = 0x8000;

static NEXT_DIFF_TRACKER_ID: AtomicU64 = AtomicU64::new(1);

/// Subscription to a tracker's events and state
struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    state: Arc<TrackerStateData>,
}

/// MidiStateDiffTracker tracks differences between two MidiStateTracker instances.
/// It uses channel-based communication to receive state change events from trackers.
pub struct MidiStateDiffTracker {
    id: u64,
    
    // Per-subscribed-tracker: receiver + shared state
    subscriptions: Vec<Subscription>,
    
    // Diff state
    diffs: BTreeSet<[u8; 2]>,
    
    // Cached tracker info for comparison
    tracker_a: Option<(u64, Arc<TrackerStateData>)>,
    tracker_b: Option<(u64, Arc<TrackerStateData>)>,
}

impl MidiStateDiffTracker {
    /// Create a new MidiStateDiffTracker
    pub fn new() -> Self {
        let id = NEXT_DIFF_TRACKER_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            id,
            subscriptions: Vec::new(),
            diffs: BTreeSet::new(),
            tracker_a: None,
            tracker_b: None,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Get the "other" tracker's state given a tracker_id
    fn get_other_state(&self, tracker_id: u64) -> Option<Arc<TrackerStateData>> {
        if let Some((a_id, _)) = &self.tracker_a {
            if *a_id == tracker_id {
                return self.tracker_b.as_ref().map(|(_, s)| Arc::clone(s));
            }
        }
        if let Some((b_id, _)) = &self.tracker_b {
            if *b_id == tracker_id {
                return self.tracker_a.as_ref().map(|(_, s)| Arc::clone(s));
            }
        }
        None
    }

    /// Drain all event channels and process events
    fn sync(&mut self) {
        // Collect events first to avoid borrow issues
        let mut events: Vec<(Arc<TrackerStateData>, Event)> = Vec::new();
        
        for sub in &self.subscriptions {
            while let Ok(event) = sub.receiver.try_recv() {
                events.push((Arc::clone(&sub.state), event));
            }
        }
        
        // Process events
        for (state, event) in events {
            self.process_event_internal(&state, event);
        }
    }

    /// Process a single event from a tracker (internal, no &mut self borrow)
    fn process_event_internal(&mut self, sender_state: &Arc<TrackerStateData>, event: Event) {
        let other_state = self.get_other_state(event.tracker_id);
        
        match event.event_type {
            EVENT_NOTE => {
                let ch = event.channel;
                let note = event.param1;
                
                let sender_vel = if sender_state.tracking_notes() {
                    sender_state.maybe_current_note_velocity(ch, note)
                } else {
                    NOTE_INACTIVE
                };
                
                let other_vel = other_state.as_ref().map(|s| s.maybe_current_note_velocity(ch, note)).unwrap_or(NOTE_INACTIVE);
                
                // A diff exists if sender has note active with different velocity than other
                if sender_vel != NOTE_INACTIVE && sender_vel != other_vel {
                    self.diffs.insert([0x90 | ch, note]);
                } else if sender_vel == NOTE_INACTIVE && other_vel != NOTE_INACTIVE {
                    // Need to turn off the note
                    self.diffs.insert([0x80 | ch, note]);
                } else {
                    self.diffs.remove(&[0x90 | ch, note]);
                    self.diffs.remove(&[0x80 | ch, note]);
                }
            }
            EVENT_CC => {
                let ch = event.channel;
                let cc = event.param1;
                
                let sender_val = if sender_state.tracking_controls() {
                    sender_state.maybe_cc_value(ch, cc)
                } else {
                    CC_VALUE_UNKNOWN
                };
                
                let other_val = other_state.as_ref().map(|s| s.maybe_cc_value(ch, cc)).unwrap_or(CC_VALUE_UNKNOWN);
                
                if sender_val != CC_VALUE_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xB0 | ch, cc]);
                } else {
                    self.diffs.remove(&[0xB0 | ch, cc]);
                }
            }
            EVENT_PROGRAM => {
                let ch = event.channel;
                let _new_prog = event.param1;
                
                let sender_val = if sender_state.tracking_programs() {
                    sender_state.maybe_program_value(ch)
                } else {
                    PROGRAM_UNKNOWN
                };
                
                let other_val = other_state.as_ref().map(|s| s.maybe_program_value(ch)).unwrap_or(PROGRAM_UNKNOWN);
                
                if sender_val != PROGRAM_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xC0 | ch, 0]);
                } else {
                    self.diffs.remove(&[0xC0 | ch, 0]);
                }
            }
            EVENT_PITCH => {
                let ch = event.channel;
                
                let sender_val = if sender_state.tracking_controls() {
                    sender_state.maybe_pitch_wheel_value(ch)
                } else {
                    PITCH_WHEEL_UNKNOWN
                };
                
                let other_val = other_state.as_ref().map(|s| s.maybe_pitch_wheel_value(ch)).unwrap_or(PITCH_WHEEL_UNKNOWN);
                
                if sender_val != PITCH_WHEEL_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xE0 | ch, 0]);
                } else {
                    self.diffs.remove(&[0xE0 | ch, 0]);
                }
            }
            EVENT_PRESSURE => {
                let ch = event.channel;
                let _new_pressure = event.param1;
                
                let sender_val = if sender_state.tracking_controls() {
                    sender_state.maybe_channel_pressure_value(ch)
                } else {
                    CHANNEL_PRESSURE_UNKNOWN
                };
                
                let other_val = other_state.as_ref().map(|s| s.maybe_channel_pressure_value(ch)).unwrap_or(CHANNEL_PRESSURE_UNKNOWN);
                
                if sender_val != CHANNEL_PRESSURE_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xD0 | ch, 0]);
                } else {
                    self.diffs.remove(&[0xD0 | ch, 0]);
                }
            }
            _ => {}
        }
    }

    /// Reset with two trackers - creates channel-based subscriptions
    pub fn reset(&mut self, a: &mut MidiStateTracker, b: &mut MidiStateTracker) {
        // Clear old subscriptions
        self.subscriptions.clear();
        
        // Create subscriptions for both trackers
        let rx_a = a.create_subscription();
        let rx_b = b.create_subscription();
        
        // Get current state from both trackers using the public method
        let state_a = Arc::new(a.get_state_data());
        let state_b = Arc::new(b.get_state_data());
        
        self.subscriptions.push(Subscription {
            tracker_id: a.get_id(),
            receiver: rx_a,
            state: state_a.clone(),
        });
        
        self.subscriptions.push(Subscription {
            tracker_id: b.get_id(),
            receiver: rx_b,
            state: state_b.clone(),
        });
        
        // Store tracker info
        self.tracker_a = Some((a.get_id(), state_a));
        self.tracker_b = Some((b.get_id(), state_b));
        
        // Initial sync to populate diffs
        self.sync();
    }

    /// Add a diff (for manual diff management)
    pub fn add_diff(&mut self, d0: u8, d1: u8) {
        self.diffs.insert([d0, d1]);
    }

    /// Remove a diff (for manual diff management)
    pub fn delete_diff(&mut self, d0: u8, d1: u8) {
        self.diffs.remove(&[d0, d1]);
    }

    /// Check a specific note and update diffs
    pub fn check_note(&mut self, channel: u8, note: u8) {
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        let av = state_a.maybe_current_note_velocity(channel, note);
        let bv = state_b.maybe_current_note_velocity(channel, note);
        
        if av != bv && av != NOTE_INACTIVE && bv != NOTE_INACTIVE {
            self.diffs.insert([0x90 | channel, note]);
        } else {
            self.diffs.remove(&[0x90 | channel, note]);
        }
    }

    /// Check a specific CC and update diffs
    pub fn check_cc(&mut self, channel: u8, controller: u8) {
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        let av = state_a.maybe_cc_value(channel, controller);
        let bv = state_b.maybe_cc_value(channel, controller);
        
        if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
            self.diffs.insert([0xB0 | channel, controller]);
        } else {
            self.diffs.remove(&[0xB0 | channel, controller]);
        }
    }

    /// Check a specific program and update diffs
    pub fn check_program(&mut self, channel: u8) {
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        let av = state_a.maybe_program_value(channel);
        let bv = state_b.maybe_program_value(channel);
        
        if av != bv && av != PROGRAM_UNKNOWN && bv != PROGRAM_UNKNOWN {
            self.diffs.insert([0xC0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xC0 | channel, 0]);
        }
    }

    /// Check channel pressure and update diffs
    pub fn check_channel_pressure(&mut self, channel: u8) {
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        let av = state_a.maybe_channel_pressure_value(channel);
        let bv = state_b.maybe_channel_pressure_value(channel);
        
        if av != bv && av != CHANNEL_PRESSURE_UNKNOWN && bv != CHANNEL_PRESSURE_UNKNOWN {
            self.diffs.insert([0xD0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xD0 | channel, 0]);
        }
    }

    /// Check pitch wheel and update diffs
    pub fn check_pitch_wheel(&mut self, channel: u8) {
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        let av = state_a.maybe_pitch_wheel_value(channel);
        let bv = state_b.maybe_pitch_wheel_value(channel);
        
        if av != bv && av != PITCH_WHEEL_UNKNOWN && bv != PITCH_WHEEL_UNKNOWN {
            self.diffs.insert([0xE0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xE0 | channel, 0]);
        }
    }

    /// Rescan all differences between trackers
    pub fn rescan_diff(&mut self) {
        self.diffs.clear();
        
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };
        
        // Check notes
        if state_a.tracking_notes() && state_b.tracking_notes() {
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    let av = state_a.maybe_current_note_velocity(channel, note);
                    let bv = state_b.maybe_current_note_velocity(channel, note);
                    if av != bv && av != NOTE_INACTIVE && bv != NOTE_INACTIVE {
                        self.diffs.insert([0x90 | channel, note]);
                    } else if av != NOTE_INACTIVE && bv == NOTE_INACTIVE {
                        self.diffs.insert([0x80 | channel, note]);
                    }
                }
            }
        }
        
        // Check CCs
        if state_a.tracking_controls() && state_b.tracking_controls() {
            for channel in 0..16u8 {
                for cc in 0..128u8 {
                    let av = state_a.maybe_cc_value(channel, cc);
                    let bv = state_b.maybe_cc_value(channel, cc);
                    if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
                        self.diffs.insert([0xB0 | channel, cc]);
                    }
                }
            }
        }
        
        // Check pitch wheel
        if state_a.tracking_controls() && state_b.tracking_controls() {
            for channel in 0..16u8 {
                let av = state_a.maybe_pitch_wheel_value(channel);
                let bv = state_b.maybe_pitch_wheel_value(channel);
                if av != bv && av != PITCH_WHEEL_UNKNOWN && bv != PITCH_WHEEL_UNKNOWN {
                    self.diffs.insert([0xE0 | channel, 0]);
                }
            }
        }
        
        // Check channel pressure
        if state_a.tracking_controls() && state_b.tracking_controls() {
            for channel in 0..16u8 {
                let av = state_a.maybe_channel_pressure_value(channel);
                let bv = state_b.maybe_channel_pressure_value(channel);
                if av != bv && av != CHANNEL_PRESSURE_UNKNOWN && bv != CHANNEL_PRESSURE_UNKNOWN {
                    self.diffs.insert([0xD0 | channel, 0]);
                }
            }
        }
        
        // Check programs
        if state_a.tracking_programs() && state_b.tracking_programs() {
            for channel in 0..16u8 {
                let av = state_a.maybe_program_value(channel);
                let bv = state_b.maybe_program_value(channel);
                if av != bv && av != PROGRAM_UNKNOWN && bv != PROGRAM_UNKNOWN {
                    self.diffs.insert([0xC0 | channel, 0]);
                }
            }
        }
    }

    /// Clear all diffs
    pub fn clear_diff(&mut self) {
        self.diffs.clear();
    }

    /// Resolve diffs to a target tracker - generates messages to make target match source
    /// Takes target_id directly (used by internal wrapper)
    fn resolve_to_by_id(
        &mut self,
        target_id: u64,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        // CRITICAL: Always sync first to ensure fresh diffs
        self.sync();
        
        // Get the "from" tracker state (the one we're resolving FROM)
        let from_state = self.get_other_state(target_id);
        
        let Some(from) = from_state else { return Vec::new() };
        
        let mut out = Vec::new();
        
        for diff in &self.diffs {
            let kind_part = diff[0] & 0xF0;
            let channel_part = diff[0] & 0x0F;
            
            match kind_part {
                0x90 => {
                    if notes {
                        let v = from.maybe_current_note_velocity(channel_part, diff[1]);
                        if v != NOTE_INACTIVE {
                            out.push(0x90 | channel_part);
                            out.push(diff[1]);
                            out.push(v);
                        }
                    }
                }
                0x80 => {
                    if notes {
                        // Note off
                        out.push(0x80 | channel_part);
                        out.push(diff[1]);
                        out.push(64);
                    }
                }
                0xB0 => {
                    if controls {
                        let v = from.maybe_cc_value(channel_part, diff[1]);
                        if v != CC_VALUE_UNKNOWN {
                            out.push(diff[0]);
                            out.push(diff[1]);
                            out.push(v);
                        }
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
                            out.push(diff[0]);
                            out.push((v & 0x7F) as u8);
                            out.push(((v >> 7) & 0x7F) as u8);
                        }
                    }
                }
                _ => {}
            }
        }
        out
    }

    /// Resolve diffs - wrapper that takes &mut MidiStateTracker (for C++ FFI)
    pub fn resolve_to_wrapper(
        &mut self,
        to: &mut MidiStateTracker,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        let to_id = to.get_id();
        self.resolve_to_by_id(to_id, notes, controls, programs)
    }

    /// Get diffs as flat byte array
    pub fn get_diff_flat(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.diffs.len() * 2);
        for d in &self.diffs {
            out.push(d[0]);
            out.push(d[1]);
        }
        out
    }

    /// Set diffs from flat byte array
    pub fn set_diff_flat(&mut self, data: &[u8]) {
        self.diffs.clear();
        for chunk in data.chunks_exact(2) {
            self.diffs.insert([chunk[0], chunk[1]]);
        }
    }

    /// Get tracker A id
    pub fn get_a_id(&self) -> Option<u64> {
        self.tracker_a.as_ref().map(|(id, _)| *id)
    }

    /// Get tracker B id
    pub fn get_b_id(&self) -> Option<u64> {
        self.tracker_b.as_ref().map(|(id, _)| *id)
    }
}

// C++-callable module-level functions

pub fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker> {
    Box::new(MidiStateDiffTracker::new())
}

pub fn reset(
    this: &mut MidiStateDiffTracker,
    a: &mut MidiStateTracker,
    b: &mut MidiStateTracker,
) {
    this.reset(a, b);
}

pub fn setup_trackers(
    _this: &mut MidiStateDiffTracker,
    _a: *mut MidiStateTracker,
    _b: *mut MidiStateTracker,
) {
    // No-op: reset handles tracker setup
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

pub fn get_diff_flat(this: &MidiStateDiffTracker) -> Vec<u8> {
    this.get_diff_flat()
}

pub fn set_diff_flat(this: &mut MidiStateDiffTracker, data: &[u8]) {
    this.set_diff_flat(data);
}

pub fn get_id(this: &MidiStateDiffTracker) -> u64 {
    this.get_id()
}