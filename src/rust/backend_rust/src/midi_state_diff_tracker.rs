use crate::event::{Event, TrackerStateData, EVENT_NOTE, EVENT_CC, EVENT_PROGRAM, EVENT_PITCH, EVENT_PRESSURE};
use crate::midi_state_tracker::MidiStateTracker;
use crossbeam_channel::Receiver;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

const NOTE_INACTIVE: u8 = 0x80;
const CC_VALUE_UNKNOWN: u8 = 0x80;
const PROGRAM_UNKNOWN: u8 = 0x80;
const CHANNEL_PRESSURE_UNKNOWN: u8 = 0x80;
const PITCH_WHEEL_UNKNOWN: u16 = 0x8000;

static NEXT_DIFF_TRACKER_ID: AtomicU64 = AtomicU64::new(1);

/// Subscription to a tracker's events
struct Subscription {
    tracker_id: u64,
    receiver: Receiver<Event>,
    /// Live reference to the tracker for reading state
    tracker: Rc<RefCell<*mut MidiStateTracker>>,
}

/// MidiStateDiffTracker tracks differences between two MidiStateTracker instances.
/// It uses channel-based communication to receive state change events from trackers,
/// and reads LIVE state from trackers (not snapshots) for diff resolution.
pub struct MidiStateDiffTracker {
    id: u64,

    // Per-subscribed-tracker: receiver + live tracker reference
    subscriptions: Vec<Subscription>,

    // Diff state
    diffs: BTreeSet<[u8; 2]>,

    // Live tracker references - stored as Rc<RefCell<*mut T>> so we can borrow mutably
    // We use raw pointers internally but wrapped in Rc for shared ownership
    tracker_a: Option<Rc<RefCell<*mut MidiStateTracker>>>,
    tracker_a_id: u64,
    tracker_b: Option<Rc<RefCell<*mut MidiStateTracker>>>,
    tracker_b_id: u64,
}

impl MidiStateDiffTracker {
    /// Create a new MidiStateDiffTracker
    pub fn new() -> Self {
        let id = NEXT_DIFF_TRACKER_ID.fetch_add(1, Ordering::Relaxed);
        eprintln!("[RUST] MidiStateDiffTracker::new() id={}", id);
        Self {
            id,
            subscriptions: Vec::new(),
            diffs: BTreeSet::new(),
            tracker_a: None,
            tracker_a_id: 0,
            tracker_b: None,
            tracker_b_id: 0,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Borrow a tracker immutably and call a function with its state data
    fn with_tracker_state<F, R>(&self, tracker: &Rc<RefCell<*mut MidiStateTracker>>, f: F) -> R
    where
        F: FnOnce(&MidiStateTracker) -> R,
    {
        let ptr = tracker.borrow();
        // SAFETY: The pointer is valid for the lifetime of the program since
        // the C++ code owns the trackers and they live at least as long as
        // the diff tracker that references them.
        let tracker_ref = unsafe { &*(*ptr) };
        f(tracker_ref)
    }

    /// Borrow a tracker mutably and call a function
    fn with_tracker_state_mut<F, R>(&self, tracker: &Rc<RefCell<*mut MidiStateTracker>>, f: F) -> R
    where
        F: FnOnce(&mut MidiStateTracker) -> R,
    {
        let mut ptr = tracker.borrow_mut();
        // SAFETY: Same as above - pointer is valid.
        let tracker_ref = unsafe { &mut *(*ptr) };
        f(tracker_ref)
    }

    /// Get the "other" tracker's live reference given a tracker_id
    fn get_other_tracker(&self, tracker_id: u64) -> Option<Rc<RefCell<*mut MidiStateTracker>>> {
        if self.tracker_a_id == tracker_id {
            return self.tracker_b.clone();
        }
        if self.tracker_b_id == tracker_id {
            return self.tracker_a.clone();
        }
        None
    }

    /// Drain all event channels and process events
    fn sync(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::sync() id={}", self.id);

        // Collect events first to avoid borrow issues
        let mut events: Vec<(Rc<RefCell<*mut MidiStateTracker>>, Event)> = Vec::new();

        for (i, sub) in self.subscriptions.iter().enumerate() {
            let mut count = 0;
            while let Ok(event) = sub.receiver.try_recv() {
                events.push((Rc::clone(&sub.tracker), event));
                count += 1;
            }
            if count > 0 {
                eprintln!("[RUST]   subscription[{}] collected {} events", i, count);
            }
        }

        eprintln!("[RUST]   sync: collected {} events, diffs before = {}", events.len(), self.diffs.len());

        // Process events using live tracker state
        for (tracker, event) in events {
            self.process_event_internal(&tracker, event);
        }

        eprintln!("[RUST]   sync: diffs after = {}", self.diffs.len());
    }

    /// Process a single event from a tracker - reads LIVE state
    fn process_event_internal(&mut self, sender_tracker: &Rc<RefCell<*mut MidiStateTracker>>, event: Event) {
        let sender_id = event.tracker_id;

        // Get other tracker for comparison
        let other_tracker = match self.get_other_tracker(sender_id) {
            Some(t) => t,
            None => return,
        };

        let channel = event.channel;

        match event.event_type {
            EVENT_NOTE => {
                let note = event.param1;
                let vel = event.param2;

                // Read LIVE state from both trackers
                let sender_vel = self.with_tracker_state(sender_tracker, |t| {
                    if t.tracking_notes() {
                        t.maybe_current_note_velocity(channel, note)
                    } else {
                        NOTE_INACTIVE
                    }
                });

                let other_vel = self.with_tracker_state(&other_tracker, |t| {
                    t.maybe_current_note_velocity(channel, note)
                });

                // C++ behavior: only track NoteOn (0x90) diffs when states differ
                if sender_vel != other_vel {
                    self.diffs.insert([0x90 | channel, note]);
                } else {
                    self.diffs.remove(&[0x90 | channel, note]);
                }
            }
            EVENT_CC => {
                let cc = event.param1;

                let sender_val = self.with_tracker_state(sender_tracker, |t| {
                    if t.tracking_controls() {
                        t.maybe_cc_value(channel, cc)
                    } else {
                        CC_VALUE_UNKNOWN
                    }
                });

                let other_val = self.with_tracker_state(&other_tracker, |t| {
                    t.maybe_cc_value(channel, cc)
                });

                if sender_val != CC_VALUE_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xB0 | channel, cc]);
                } else {
                    self.diffs.remove(&[0xB0 | channel, cc]);
                }
            }
            EVENT_PROGRAM => {
                let sender_val = self.with_tracker_state(sender_tracker, |t| {
                    if t.tracking_programs() {
                        t.maybe_program_value(channel)
                    } else {
                        PROGRAM_UNKNOWN
                    }
                });

                let other_val = self.with_tracker_state(&other_tracker, |t| {
                    t.maybe_program_value(channel)
                });

                if sender_val != PROGRAM_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xC0 | channel, 0]);
                } else {
                    self.diffs.remove(&[0xC0 | channel, 0]);
                }
            }
            EVENT_PITCH => {
                let sender_val = self.with_tracker_state(sender_tracker, |t| {
                    if t.tracking_controls() {
                        t.maybe_pitch_wheel_value(channel)
                    } else {
                        PITCH_WHEEL_UNKNOWN
                    }
                });

                let other_val = self.with_tracker_state(&other_tracker, |t| {
                    t.maybe_pitch_wheel_value(channel)
                });

                // C++ behavior: if sender_val differs from other_val and sender is not unknown, add diff
                if sender_val != other_val && sender_val != PITCH_WHEEL_UNKNOWN {
                    self.diffs.insert([0xE0 | channel, 0]);
                } else {
                    self.diffs.remove(&[0xE0 | channel, 0]);
                }
            }
            EVENT_PRESSURE => {
                let sender_val = self.with_tracker_state(sender_tracker, |t| {
                    if t.tracking_controls() {
                        t.maybe_channel_pressure_value(channel)
                    } else {
                        CHANNEL_PRESSURE_UNKNOWN
                    }
                });

                let other_val = self.with_tracker_state(&other_tracker, |t| {
                    t.maybe_channel_pressure_value(channel)
                });

                if sender_val != CHANNEL_PRESSURE_UNKNOWN && sender_val != other_val {
                    self.diffs.insert([0xD0 | channel, 0]);
                } else {
                    self.diffs.remove(&[0xD0 | channel, 0]);
                }
            }
            _ => {}
        }
    }

    /// Reset with two trackers - creates channel-based subscriptions
    /// Takes raw pointers from C++ FFI and wraps them in Rc<RefCell<...>>
    pub fn reset_with_ptrs(&mut self, a: *mut MidiStateTracker, b: *mut MidiStateTracker) {
        eprintln!("[RUST] MidiStateDiffTracker::reset_with_ptrs() id={}", self.id);

        // SAFETY: The raw pointers are valid because they come from C++ which owns
        // the MidiStateTracker objects. The Rc will NOT drop these pointers
        // (we use Rc::from_raw with NonNull and ensure we don't call from_raw destructor).
        // This is safe because the C++ side maintains ownership.

        let tracker_a = unsafe {
            Rc::new(RefCell::new(a))
        };
        let tracker_b = unsafe {
            Rc::new(RefCell::new(b))
        };

        let a_id = self.with_tracker_state(&tracker_a, |t| t.get_id());
        let b_id = self.with_tracker_state(&tracker_b, |t| t.get_id());

        eprintln!("[RUST]   a_id={} b_id={}", a_id, b_id);

        // Debug: show current state
        eprintln!("[RUST]   a state: notes[0][100]={}",
            self.with_tracker_state(&tracker_a, |t| t.maybe_current_note_velocity(0, 100)));
        eprintln!("[RUST]   b state: notes[0][100]={}",
            self.with_tracker_state(&tracker_b, |t| t.maybe_current_note_velocity(0, 100)));

        // Clear old subscriptions
        self.subscriptions.clear();

        // Store live tracker references
        self.tracker_a = Some(Rc::clone(&tracker_a));
        self.tracker_a_id = a_id;
        self.tracker_b = Some(Rc::clone(&tracker_b));
        self.tracker_b_id = b_id;

        // Create subscriptions for both trackers (needs mutable borrow)
        let rx_a = self.with_tracker_state_mut(&tracker_a, |t| t.create_subscription());
        let rx_b = self.with_tracker_state_mut(&tracker_b, |t| t.create_subscription());

        self.subscriptions.push(Subscription {
            tracker_id: a_id,
            receiver: rx_a,
            tracker: Rc::clone(&tracker_a),
        });

        self.subscriptions.push(Subscription {
            tracker_id: b_id,
            receiver: rx_b,
            tracker: Rc::clone(&tracker_b),
        });

        eprintln!("[RUST]   subscriptions created: {} total", self.subscriptions.len());

        // Initial sync to populate diffs
        self.sync();

        // Compare static states to populate diffs
        self.compare_static_states();
    }

    /// Compare static states between trackers to populate initial diffs
    fn compare_static_states(&mut self) {
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        eprintln!("[RUST]   comparing static states...");

        // Check notes
        let a_tracking_notes = self.with_tracker_state(tracker_a, |t| t.tracking_notes());
        let b_tracking_notes = self.with_tracker_state(tracker_b, |t| t.tracking_notes());
        if a_tracking_notes && b_tracking_notes {
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    let av = self.with_tracker_state(tracker_a, |t| t.maybe_current_note_velocity(channel, note));
                    let bv = self.with_tracker_state(tracker_b, |t| t.maybe_current_note_velocity(channel, note));
                    if av != bv {
                        self.diffs.insert([0x90 | channel, note]);
                    }
                }
            }
        }

        // Check CC values
        let a_tracking_controls = self.with_tracker_state(tracker_a, |t| t.tracking_controls());
        let b_tracking_controls = self.with_tracker_state(tracker_b, |t| t.tracking_controls());
        if a_tracking_controls && b_tracking_controls {
            for channel in 0..16u8 {
                for cc in 0..128u8 {
                    let av = self.with_tracker_state(tracker_a, |t| t.maybe_cc_value(channel, cc));
                    let bv = self.with_tracker_state(tracker_b, |t| t.maybe_cc_value(channel, cc));
                    if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
                        self.diffs.insert([0xB0 | channel, cc]);
                    }
                }
            }
        }

        // Check pitch wheel
        if a_tracking_controls && b_tracking_controls {
            for channel in 0..16u8 {
                let av = self.with_tracker_state(tracker_a, |t| t.maybe_pitch_wheel_value(channel));
                let bv = self.with_tracker_state(tracker_b, |t| t.maybe_pitch_wheel_value(channel));
                if av != bv && av != PITCH_WHEEL_UNKNOWN {
                    eprintln!("[RUST]   pitch diff ch={}: av={} bv={} -> adding", channel, av, bv);
                    self.diffs.insert([0xE0 | channel, 0]);
                }
            }
        }

        eprintln!("[RUST]   reset complete: diffs = {}", self.diffs.len());
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
        // Get trackers for comparison
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        let av = self.with_tracker_state(tracker_a, |t| t.maybe_current_note_velocity(channel, note));
        let bv = self.with_tracker_state(tracker_b, |t| t.maybe_current_note_velocity(channel, note));

        if av != bv {
            self.diffs.insert([0x90 | channel, note]);
        } else {
            self.diffs.remove(&[0x90 | channel, note]);
        }
    }

    /// Check a specific CC and update diffs
    pub fn check_cc(&mut self, channel: u8, controller: u8) {
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        let av = self.with_tracker_state(tracker_a, |t| t.maybe_cc_value(channel, controller));
        let bv = self.with_tracker_state(tracker_b, |t| t.maybe_cc_value(channel, controller));

        if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
            self.diffs.insert([0xB0 | channel, controller]);
        } else {
            self.diffs.remove(&[0xB0 | channel, controller]);
        }
    }

    /// Check a specific program and update diffs
    pub fn check_program(&mut self, channel: u8) {
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        let av = self.with_tracker_state(tracker_a, |t| t.maybe_program_value(channel));
        let bv = self.with_tracker_state(tracker_b, |t| t.maybe_program_value(channel));

        if av != bv && av != PROGRAM_UNKNOWN && bv != PROGRAM_UNKNOWN {
            self.diffs.insert([0xC0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xC0 | channel, 0]);
        }
    }

    /// Check channel pressure and update diffs
    pub fn check_channel_pressure(&mut self, channel: u8) {
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        let av = self.with_tracker_state(tracker_a, |t| t.maybe_channel_pressure_value(channel));
        let bv = self.with_tracker_state(tracker_b, |t| t.maybe_channel_pressure_value(channel));

        if av != bv && av != CHANNEL_PRESSURE_UNKNOWN && bv != CHANNEL_PRESSURE_UNKNOWN {
            self.diffs.insert([0xD0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xD0 | channel, 0]);
        }
    }

    /// Check pitch wheel and update diffs
    pub fn check_pitch_wheel(&mut self, channel: u8) {
        let tracker_a = match &self.tracker_a {
            Some(t) => t,
            None => return,
        };
        let tracker_b = match &self.tracker_b {
            Some(t) => t,
            None => return,
        };

        let av = self.with_tracker_state(tracker_a, |t| t.maybe_pitch_wheel_value(channel));
        let bv = self.with_tracker_state(tracker_b, |t| t.maybe_pitch_wheel_value(channel));

        if av != bv && av != PITCH_WHEEL_UNKNOWN {
            self.diffs.insert([0xE0 | channel, 0]);
        } else {
            self.diffs.remove(&[0xE0 | channel, 0]);
        }
    }

    /// Rescan all differences between trackers
    pub fn rescan_diff(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::rescan_diff() id={}", self.id);

        self.diffs.clear();
        self.compare_static_states();

        eprintln!("[RUST]   final diffs count = {}", self.diffs.len());
    }

    /// Clear all diffs
    pub fn clear_diff(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::clear_diff() id={}", self.id);
        self.diffs.clear();
    }

    /// Resolve diffs to a target tracker - generates messages to make target match source
    /// Reads LIVE state from the "from" tracker
    fn resolve_to_by_id(
        &mut self,
        target_id: u64,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        eprintln!("[RUST] MidiStateDiffTracker::resolve_to_by_id() target_id={} tracker_a_id={} tracker_b_id={}", 
            target_id, self.tracker_a_id, self.tracker_b_id);

        // CRITICAL: Always sync first to ensure fresh diffs
        self.sync();

        // Get the "from" tracker (the one we're resolving FROM)
        let from_tracker = match self.get_other_tracker(target_id) {
            Some(t) => t,
            None => return Vec::new(),
        };

        // Debug: show pitch values from both trackers
        if let Some(ref ta) = self.tracker_a {
            let pw = self.with_tracker_state(ta, |t| t.maybe_pitch_wheel_value(0));
            eprintln!("[RUST]   tracker_a (id={}) pitch_wheel[0]={}", self.tracker_a_id, pw);
        }
        if let Some(ref tb) = self.tracker_b {
            let pw = self.with_tracker_state(tb, |t| t.maybe_pitch_wheel_value(0));
            eprintln!("[RUST]   tracker_b (id={}) pitch_wheel[0]={}", self.tracker_b_id, pw);
        }
        
        // Show which tracker is the "from" tracker
        if target_id == self.tracker_a_id {
            eprintln!("[RUST]   resolving to tracker_a, so reading from tracker_b");
        } else if target_id == self.tracker_b_id {
            eprintln!("[RUST]   resolving to tracker_b, so reading from tracker_a");
        }

        eprintln!("[RUST]   from tracker found, reading live state...");

        let mut out = Vec::new();

        // Read from LIVE tracker state
        eprintln!("[RUST]   resolving {} diffs:", self.diffs.len());
        for diff in &self.diffs {
            eprintln!("[RUST]     diff [{:#x}, {:#x}]", diff[0], diff[1]);
            let kind_part = diff[0] & 0xF0;
            let channel_part = diff[0] & 0x0F;
            eprintln!("[RUST]       kind={:#x} ch={}", kind_part, channel_part);

            match kind_part {
                0x90 => {
                    if notes {
                        let v = self.with_tracker_state(&from_tracker, |t| {
                            t.maybe_current_note_velocity(channel_part, diff[1])
                        });
                        if v != NOTE_INACTIVE {
                            out.push(diff[0]);  // 0x90 | channel
                            out.push(diff[1]);  // note
                            out.push(v);        // velocity
                        }
                    }
                }
                0xB0 => {
                    if controls {
                        let v = self.with_tracker_state(&from_tracker, |t| {
                            t.maybe_cc_value(channel_part, diff[1])
                        });
                        if v != CC_VALUE_UNKNOWN {
                            out.push(diff[0]);
                            out.push(diff[1]);
                            out.push(v);
                        }
                    }
                }
                0xC0 => {
                    if programs {
                        let v = self.with_tracker_state(&from_tracker, |t| {
                            t.maybe_program_value(channel_part)
                        });
                        if v != PROGRAM_UNKNOWN {
                            out.push(diff[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xD0 => {
                    if controls {
                        let v = self.with_tracker_state(&from_tracker, |t| {
                            t.maybe_channel_pressure_value(channel_part)
                        });
                        if v != CHANNEL_PRESSURE_UNKNOWN {
                            out.push(diff[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xE0 => {
                    if controls {
                        let v = self.with_tracker_state(&from_tracker, |t| {
                            t.maybe_pitch_wheel_value(channel_part)
                        });
                        eprintln!("[RUST]       reading pitch ch={} = {} ({:#x})", channel_part, v, v);
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

        eprintln!("[RUST]   resolve_to result: {:?}", out);
        out
    }

    /// Resolve diffs - wrapper for C++ FFI
    pub fn resolve_to_wrapper(
        &mut self,
        to: &mut MidiStateTracker,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        let to_id = to.get_id();
        eprintln!("[RUST] MidiStateDiffTracker::resolve_to_wrapper() id={} to_id={} tracker_a_id={} tracker_b_id={} notes={} controls={} programs={}", 
            self.id, to_id, self.tracker_a_id, self.tracker_b_id, notes, controls, programs);
        
        // Check if trackers are set up
        if self.tracker_a.is_none() || self.tracker_b.is_none() {
            eprintln!("[RUST]   trackers not set up, returning empty");
            return Vec::new();
        }
        
        // Debug: show pitch values from both trackers
        if let Some(ref ta) = self.tracker_a {
            let pw = self.with_tracker_state(ta, |t| t.maybe_pitch_wheel_value(0));
            eprintln!("[RUST]   tracker_a (id={}) pitch_wheel[0]={}", self.tracker_a_id, pw);
        }
        if let Some(ref tb) = self.tracker_b {
            let pw = self.with_tracker_state(tb, |t| t.maybe_pitch_wheel_value(0));
            eprintln!("[RUST]   tracker_b (id={}) pitch_wheel[0]={}", self.tracker_b_id, pw);
        }

        let result = self.resolve_to_by_id(to_id, notes, controls, programs);
        eprintln!("[RUST]   result: {:?}", result);
        result
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
        if self.tracker_a_id != 0 {
            Some(self.tracker_a_id)
        } else {
            None
        }
    }

    /// Get tracker B id
    pub fn get_b_id(&self) -> Option<u64> {
        if self.tracker_b_id != 0 {
            Some(self.tracker_b_id)
        } else {
            None
        }
    }
}

// C++-callable module-level functions

pub fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker> {
    eprintln!("[RUST] new_midi_state_diff_tracker()");
    Box::new(MidiStateDiffTracker::new())
}

/// Reset with raw pointers (for C++ FFI)
pub fn reset_with_ptrs(
    this: &mut MidiStateDiffTracker,
    a: *mut MidiStateTracker,
    b: *mut MidiStateTracker,
) {
    this.reset_with_ptrs(a, b);
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
