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
        eprintln!("[RUST] MidiStateDiffTracker::new() id={}", id);
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
        eprintln!("[RUST] MidiStateDiffTracker::get_other_state(tracker_id={})", tracker_id);
        eprintln!("[RUST]   tracker_a={:?} tracker_b={:?}", 
            self.tracker_a.as_ref().map(|(id, _)| id),
            self.tracker_b.as_ref().map(|(id, _)| id)
        );
        
        if let Some((a_id, _)) = &self.tracker_a {
            if *a_id == tracker_id {
                eprintln!("[RUST]   -> returning tracker_b");
                if let Some((_, state)) = &self.tracker_b {
                    eprintln!("[RUST]   tracker_b state: notes[0][100]={} pitch_wheel[0]={}",
                        state.notes_active.get(100).copied().unwrap_or(0),
                        state.pitch_wheel.get(0).copied().unwrap_or(0));
                }
                return self.tracker_b.as_ref().map(|(_, s)| Arc::clone(s));
            }
        }
        if let Some((b_id, _)) = &self.tracker_b {
            if *b_id == tracker_id {
                eprintln!("[RUST]   -> returning tracker_a");
                if let Some((_, state)) = &self.tracker_a {
                    eprintln!("[RUST]   tracker_a state: notes[0][100]={} pitch_wheel[0]={}",
                        state.notes_active.get(100).copied().unwrap_or(0),
                        state.pitch_wheel.get(0).copied().unwrap_or(0));
                }
                return self.tracker_a.as_ref().map(|(_, s)| Arc::clone(s));
            }
        }
        eprintln!("[RUST]   -> returning None (tracker not found)");
        None
    }

    /// Drain all event channels and process events
    fn sync(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::sync() id={}", self.id);
        
        // Collect events first to avoid borrow issues
        let mut events: Vec<(Arc<TrackerStateData>, Event)> = Vec::new();

        for sub in &self.subscriptions {
            eprintln!("[RUST]   checking receiver for tracker_id={}", sub.tracker_id);
            while let Ok(event) = sub.receiver.try_recv() {
                eprintln!("[RUST]   -> collected event: tracker_id={} type={} ch={} p1={} p2={}", 
                    event.tracker_id, event.event_type, event.channel, event.param1, event.param2);
                events.push((Arc::clone(&sub.state), event));
            }
        }

        eprintln!("[RUST]   sync: collected {} events, diffs before = {}", events.len(), self.diffs.len());
        for d in &self.diffs {
            eprintln!("[RUST]     diff before: [{:#x}, {:#x}]", d[0], d[1]);
        }

        // Process events
        for (state, event) in events {
            self.process_event_internal(&state, event);
        }

        eprintln!("[RUST]   sync: diffs after = {}", self.diffs.len());
        for d in &self.diffs {
            eprintln!("[RUST]     diff after: [{:#x}, {:#x}]", d[0], d[1]);
        }
    }

    /// Process a single event from a tracker (internal, no &mut self borrow)
    fn process_event_internal(&mut self, sender_state: &Arc<TrackerStateData>, event: Event) {
        eprintln!("[RUST] MidiStateDiffTracker::process_event_internal() event: tracker_id={} type={} ch={} note={} vel={}", 
            event.tracker_id, event.event_type, event.channel, event.param1, event.param2);
        
        let other_state = self.get_other_state(event.tracker_id);
        eprintln!("[RUST]   other_state present: {}", other_state.is_some());

        match event.event_type {
            EVENT_NOTE => {
                let ch = event.channel;
                let note = event.param1;
                let vel = event.param2;

                let sender_vel = if sender_state.tracking_notes() {
                    sender_state.maybe_current_note_velocity(ch, note)
                } else {
                    NOTE_INACTIVE
                };

                let other_vel = other_state.as_ref().map(|s| s.maybe_current_note_velocity(ch, note)).unwrap_or(NOTE_INACTIVE);
                
                eprintln!("[RUST]   EVENT_NOTE: ch={} note={} sender_vel={} (0x{:02x}) other_vel={} (0x{:02x})", 
                    ch, note, sender_vel, sender_vel, other_vel, other_vel);

                // C++ behavior: only track NoteOn (0x90) diffs when states differ
                if sender_vel != other_vel {
                    eprintln!("[RUST]     -> inserting diff [0x90 | ch, note] = [{:#x}, {:#x}]", 0x90 | ch, note);
                    self.diffs.insert([0x90 | ch, note]);
                } else {
                    eprintln!("[RUST]     -> removing diff [0x90 | ch, note]");
                    self.diffs.remove(&[0x90 | ch, note]);
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
                
                eprintln!("[RUST]   EVENT_CC: ch={} cc={} sender_val={} other_val={}", ch, cc, sender_val, other_val);

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

                eprintln!("[RUST]   EVENT_PITCH: ch={} sender_val={} (0x{:04x}) other_val={} (0x{:04x})", 
                    ch, sender_val, sender_val, other_val, other_val);

                // C++ behavior: if sender_val differs from other_val and sender is not unknown, add diff
                if sender_val != other_val && sender_val != PITCH_WHEEL_UNKNOWN {
                    eprintln!("[RUST]     -> inserting pitch diff [0xE0 | ch, 0]");
                    self.diffs.insert([0xE0 | ch, 0]);
                } else {
                    eprintln!("[RUST]     -> removing pitch diff");
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
        eprintln!("[RUST] MidiStateDiffTracker::reset() id={}", self.id);
        eprintln!("[RUST]   a_id={} b_id={}", a.get_id(), b.get_id());
        
        // Show current state of both trackers
        let state_a = a.get_state_data();
        let state_b = b.get_state_data();
        eprintln!("[RUST]   a state: notes[0][100]={} (0x{:02x}), controls len={}", 
            state_a.notes_active.get(100).copied().unwrap_or(0), 
            state_a.notes_active.get(100).copied().unwrap_or(0),
            state_a.controls.len());
        eprintln!("[RUST]   b state: notes[0][100]={} (0x{:02x}), controls len={}", 
            state_b.notes_active.get(100).copied().unwrap_or(0),
            state_b.notes_active.get(100).copied().unwrap_or(0),
            state_b.controls.len());
        
        // Debug: show diffs before clearing
        eprintln!("[RUST]   diffs before clear = {}:", self.diffs.len());
        for d in &self.diffs {
            eprintln!("[RUST]     diff: [{:#x}, {:#x}]", d[0], d[1]);
        }
        
        // Clear old subscriptions
        self.subscriptions.clear();
        
        // Create subscriptions for both trackers
        let rx_a = a.create_subscription();
        let rx_b = b.create_subscription();
        
        // Store tracker info BEFORE sync
        self.tracker_a = Some((a.get_id(), Arc::new(a.get_state_data())));
        self.tracker_b = Some((b.get_id(), Arc::new(b.get_state_data())));
        
        self.subscriptions.push(Subscription {
            tracker_id: a.get_id(),
            receiver: rx_a,
            state: self.tracker_a.as_ref().unwrap().1.clone(),
        });
        
        self.subscriptions.push(Subscription {
            tracker_id: b.get_id(),
            receiver: rx_b,
            state: self.tracker_b.as_ref().unwrap().1.clone(),
        });
        
        eprintln!("[RUST]   subscriptions created: {} total", self.subscriptions.len());
        
        // Initial sync to populate diffs
        self.sync();
        
        // CRITICAL: After sync, also compare static state to populate diffs.
        // This is what the C++ implementation's sync_internal() does.
        // The sync() above only processes events from channels (none at reset time).
        // We need to compare the current tracker states and add any diffs.
        eprintln!("[RUST]   comparing static states to populate diffs...");
        if let (Some((_, sa)), Some((_, sb))) = (&self.tracker_a, &self.tracker_b) {
            // Check notes
            if sa.tracking_notes() && sb.tracking_notes() {
                let av = sa.maybe_current_note_velocity(0, 100);
                let bv = sb.maybe_current_note_velocity(0, 100);
                eprintln!("[RUST]   note ch=0 note=100: av={} bv={}", av, bv);
                
                if av != bv {
                    eprintln!("[RUST]   -> adding note diff [0x90, 100] because states differ");
                    self.diffs.insert([0x90 | 0, 100]);
                }
            }
            
            // Check CC values (specifically CC1 which is often used for modulation)
            if sa.tracking_controls() && sb.tracking_controls() {
                let av = sa.maybe_cc_value(0, 1);
                let bv = sb.maybe_cc_value(0, 1);
                eprintln!("[RUST]   CC ch=0 cc=1: av={} bv={}", av, bv);
                
                if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
                    eprintln!("[RUST]   -> adding CC diff [0xB1, 1]");
                    self.diffs.insert([0xB0 | 0, 1]);
                }
            }
            
            // Check pitch wheel
            if sa.tracking_controls() && sb.tracking_controls() {
                let av = sa.maybe_pitch_wheel_value(0);
                let bv = sb.maybe_pitch_wheel_value(0);
                eprintln!("[RUST]   pitch wheel ch=0: av={} bv={}", av, bv);
                
                // C++ behavior: if av differs from bv and av is not unknown, add diff
                if av != bv && av != PITCH_WHEEL_UNKNOWN {
                    eprintln!("[RUST]   -> adding pitch diff [0xE0, 0]");
                    self.diffs.insert([0xE0 | 0, 0]);
                }
            }
        }
        
        eprintln!("[RUST]   reset complete: diffs = {}", self.diffs.len());
        for d in &self.diffs {
            eprintln!("[RUST]     final diff: [{:#x}, {:#x}]", d[0], d[1]);
        }
    }

    /// Add a diff (for manual diff management)
    pub fn add_diff(&mut self, d0: u8, d1: u8) {
        eprintln!("[RUST] MidiStateDiffTracker::add_diff([{:#x}, {:#x}])", d0, d1);
        self.diffs.insert([d0, d1]);
        eprintln!("[RUST]   diffs now: {}", self.diffs.len());
    }

    /// Remove a diff (for manual diff management)
    pub fn delete_diff(&mut self, d0: u8, d1: u8) {
        eprintln!("[RUST] MidiStateDiffTracker::delete_diff([{:#x}, {:#x}])", d0, d1);
        self.diffs.remove(&[d0, d1]);
        eprintln!("[RUST]   diffs now: {}", self.diffs.len());
    }

    /// Check a specific note and update diffs
    pub fn check_note(&mut self, channel: u8, note: u8) {
        eprintln!("[RUST] MidiStateDiffTracker::check_note(ch={} note={})", channel, note);
        
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { 
            eprintln!("[RUST]   missing tracker state");
            return; 
        };

        let av = state_a.maybe_current_note_velocity(channel, note);
        let bv = state_b.maybe_current_note_velocity(channel, note);
        
        eprintln!("[RUST]   av={} bv={}", av, bv);

        // C++ behavior: if states differ, add diff; if same, remove diff
        if av != bv {
            eprintln!("[RUST]   -> inserting diff [0x90 | ch, note]");
            self.diffs.insert([0x90 | channel, note]);
        } else {
            eprintln!("[RUST]   -> removing diff");
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
        eprintln!("[RUST] MidiStateDiffTracker::check_pitch_wheel(ch={})", channel);
        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else { return };

        let av = state_a.maybe_pitch_wheel_value(channel);
        let bv = state_b.maybe_pitch_wheel_value(channel);

        eprintln!("[RUST]   av={} bv={}", av, bv);

        // C++ behavior: if av differs from bv and av is not unknown, add diff
        if av != bv && av != PITCH_WHEEL_UNKNOWN {
            eprintln!("[RUST]   -> inserting pitch diff");
            self.diffs.insert([0xE0 | channel, 0]);
        } else {
            eprintln!("[RUST]   -> removing pitch diff");
            self.diffs.remove(&[0xE0 | channel, 0]);
        }
    }

    /// Rescan all differences between trackers
    pub fn rescan_diff(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::rescan_diff() id={}", self.id);
        
        self.diffs.clear();

        let (Some((_, state_a)), Some((_, state_b))) = (&self.tracker_a, &self.tracker_b) else {
            eprintln!("[RUST]   missing tracker state");
            return;
        };

        eprintln!("[RUST]   state_a tracking_notes={} tracking_controls={} tracking_programs={}", 
            state_a.tracking_notes(), state_a.tracking_controls(), state_a.tracking_programs());
        eprintln!("[RUST]   state_b tracking_notes={} tracking_controls={} tracking_programs={}", 
            state_b.tracking_notes(), state_b.tracking_controls(), state_b.tracking_programs());

        // Scan ALL notes
        if state_a.tracking_notes() && state_b.tracking_notes() {
            eprintln!("[RUST]   scanning notes...");
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    let av = state_a.maybe_current_note_velocity(channel, note);
                    let bv = state_b.maybe_current_note_velocity(channel, note);
                    
                    if av != bv {
                        if channel == 0 && note == 100 {
                            eprintln!("[RUST]   note ch=0 note=100 av={} bv={} -> adding diff", av, bv);
                        }
                        self.diffs.insert([0x90 | channel, note]);
                    }
                }
            }
        }

        // Scan CC values
        if state_a.tracking_controls() && state_b.tracking_controls() {
            eprintln!("[RUST]   scanning CCs...");
            for channel in 0..16u8 {
                for cc in 0..128u8 {
                    let av = state_a.maybe_cc_value(channel, cc);
                    let bv = state_b.maybe_cc_value(channel, cc);
                    
                    if av != bv && av != CC_VALUE_UNKNOWN && bv != CC_VALUE_UNKNOWN {
                        if channel == 0 && cc == 1 {
                            eprintln!("[RUST]   CC ch=0 cc=1 av={} bv={} -> adding diff", av, bv);
                        }
                        self.diffs.insert([0xB0 | channel, cc]);
                    }
                }
            }
        }

        // Scan pitch wheel
        if state_a.tracking_controls() && state_b.tracking_controls() {
            eprintln!("[RUST]   scanning pitch wheel...");
            for channel in 0..16u8 {
                let av = state_a.maybe_pitch_wheel_value(channel);
                let bv = state_b.maybe_pitch_wheel_value(channel);
                
                // C++ behavior: if values differ and sender (av) is not unknown, add diff
                // Note: We don't check if bv is unknown because we want to resolve from
                // a known value to an unknown/default value
                if av != bv && av != PITCH_WHEEL_UNKNOWN {
                    if channel == 0 {
                        eprintln!("[RUST]   pitch ch=0 av={} (0x{:04x}) bv={} (0x{:04x}) -> adding diff", av, av, bv, bv);
                    }
                    self.diffs.insert([0xE0 | channel, 0]);
                }
            }
        }

        eprintln!("[RUST]   final diffs count = {}", self.diffs.len());
        if self.diffs.len() > 0 {
            eprintln!("[RUST]   first few diffs:");
            for d in self.diffs.iter().take(10) {
                eprintln!("[RUST]     [{:#x}, {:#x}]", d[0], d[1]);
            }
        }
    }

    /// Clear all diffs
    pub fn clear_diff(&mut self) {
        eprintln!("[RUST] MidiStateDiffTracker::clear_diff() id={} diffs before={}", self.id, self.diffs.len());
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
        eprintln!("[RUST] MidiStateDiffTracker::resolve_to_by_id() target_id={} notes={} controls={} programs={}", 
            target_id, notes, controls, programs);
        
        // CRITICAL: Always sync first to ensure fresh diffs
        self.sync();

        // Get the "from" tracker state (the one we're resolving FROM)
        let from_state = self.get_other_state(target_id);
        
        eprintln!("[RUST]   from_state is_some: {}", from_state.is_some());

        let Some(from) = from_state else { 
            eprintln!("[RUST]   -> returning empty (no from state)");
            return Vec::new(); 
        };

        eprintln!("[RUST]   from state: notes[0][100]={}", from.maybe_current_note_velocity(0, 100));

        let mut out = Vec::new();

        eprintln!("[RUST]   iterating over {} diffs:", self.diffs.len());
        for diff in &self.diffs {
            let kind_part = diff[0] & 0xF0;
            let channel_part = diff[0] & 0x0F;
            eprintln!("[RUST]     diff [{:#x}, {:#x}] kind={:#x} ch={}", diff[0], diff[1], kind_part, channel_part);

            match kind_part {
                0x90 => {
                    if notes {
                        let v = from.maybe_current_note_velocity(channel_part, diff[1]);
                        eprintln!("[RUST]       velocity from 'from': {} (0x{:02x})", v, v);
                        if v != NOTE_INACTIVE {
                            // Use the diff status byte (0x90 or 0x80) to determine NoteOn vs NoteOff
                            // If diff says NoteOn (0x90), output NoteOn with velocity
                            // If diff says NoteOff (0x80), output NoteOff with velocity
                            out.push(diff[0]);  // 0x90 | channel or 0x80 | channel
                            out.push(diff[1]);
                            out.push(v);
                            eprintln!("[RUST]       -> output [{:#x}, {}, {}]", diff[0], diff[1], v);
                        } else {
                            eprintln!("[RUST]       -> skip (velocity inactive)");
                        }
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
                        eprintln!("[RUST]       pitch from 'from': {} (0x{:04x})", v, v);
                        if v != PITCH_WHEEL_UNKNOWN {
                            out.push(diff[0]);
                            out.push((v & 0x7F) as u8);
                            out.push(((v >> 7) & 0x7F) as u8);
                            eprintln!("[RUST]       -> output pitch [{:#x}, {}, {}]", diff[0], (v & 0x7F) as u8, ((v >> 7) & 0x7F) as u8);
                        } else {
                            eprintln!("[RUST]       -> skip (pitch unknown)");
                        }
                    }
                }
                _ => {}
            }
        }
        eprintln!("[RUST]   resolve_to_by_id returning: {:?}", out);
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

        eprintln!("[RUST] MidiStateDiffTracker::resolve_to_wrapper() id={} to_id={} notes={} controls={} programs={}", 
            self.id, to_id, notes, controls, programs);

        // Debug: log current tracker states
        eprintln!("[RUST]   tracker_a={:?} tracker_b={:?}", 
            self.tracker_a.as_ref().map(|(id, _)| id),
            self.tracker_b.as_ref().map(|(id, _)| id)
        );

        // Get current state from "to" tracker
        let to_state = to.get_state_data();

        // Log note state for channel 0, note 100
        let note_idx = (0usize) * 128 + 100usize;
        if note_idx < to_state.notes_active.len() {
            eprintln!("[RUST]   to tracker note[0][100] = {} (0x{:02x})", 
                to_state.notes_active[note_idx], to_state.notes_active[note_idx]);
        }
        
        // Log CC state
        if to_state.tracking_controls() {
            let cc_idx = (0usize) * 128 + 1usize;
            if cc_idx < to_state.controls.len() {
                eprintln!("[RUST]   to tracker CC[0][1] = {} (0x{:02x})", 
                    to_state.controls[cc_idx], to_state.controls[cc_idx]);
            }
            eprintln!("[RUST]   to tracker pitch_wheel[0] = {}", 
                to_state.pitch_wheel.get(0).copied().unwrap_or(0));
        }

        // Log diffs before resolve
        eprintln!("[RUST]   diffs count = {}:", self.diffs.len());
        for d in self.diffs.iter().take(10) {
            eprintln!("[RUST]     diff = [{:#x}, {:#x}]", d[0], d[1]);
        }

        let result = self.resolve_to_by_id(to_id, notes, controls, programs);

        eprintln!("[RUST]   resolve_to_wrapper result len = {}, result = {:?}", result.len(), result);

        result
    }

    /// Get diffs as flat byte array
    pub fn get_diff_flat(&self) -> Vec<u8> {
        eprintln!("[RUST] MidiStateDiffTracker::get_diff_flat() id={}", self.id);
        let mut out = Vec::with_capacity(self.diffs.len() * 2);
        for d in &self.diffs {
            out.push(d[0]);
            out.push(d[1]);
        }
        eprintln!("[RUST]   returning {} bytes", out.len());
        out
    }

    /// Set diffs from flat byte array
    pub fn set_diff_flat(&mut self, data: &[u8]) {
        eprintln!("[RUST] MidiStateDiffTracker::set_diff_flat() id={} data_len={}", self.id, data.len());
        self.diffs.clear();
        for chunk in data.chunks_exact(2) {
            eprintln!("[RUST]   inserting diff [{:#x}, {:#x}]", chunk[0], chunk[1]);
            self.diffs.insert([chunk[0], chunk[1]]);
        }
        eprintln!("[RUST]   diffs now: {}", self.diffs.len());
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
    eprintln!("[RUST] new_midi_state_diff_tracker()");
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