use crate::event::{Event, TrackerStateData};
use crate::midi_helpers;
use crossbeam_channel::{bounded, Sender};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

const NOTE_INACTIVE: u8 = 0x80;
const CC_VALUE_UNKNOWN: u8 = 0x80;
const PROGRAM_UNKNOWN: u8 = 0x80;
const CHANNEL_PRESSURE_UNKNOWN: u8 = 0x80;
const PITCH_WHEEL_UNKNOWN: u16 = 0x8000;
const PITCH_WHEEL_DEFAULT: u16 = 0x2000;

/// Capacity for event channels - pre-allocated to avoid allocation during processing
const EVENT_CHANNEL_CAPACITY: usize = 64;

static NEXT_TRACKER_ID: AtomicU64 = AtomicU64::new(1);

/// MidiStateTracker tracks MIDI state (notes, CC, pitch wheel, channel pressure, programs)
/// and communicates state changes via channels to subscribed diff trackers.
pub struct MidiStateTracker {
    id: u64,
    
    // Tracking configuration
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    
    // Owned mutable state - mutated during process_msg
    notes_active: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    n_notes_active: AtomicI32,
    
    // Subscription senders - one per diff tracker
    // When process_msg happens, we send to ALL of these
    subscribers: Vec<Sender<Event>>,
}

impl MidiStateTracker {
    /// Create a new MidiStateTracker
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        let id = NEXT_TRACKER_ID.fetch_add(1, Ordering::Relaxed);
        
        let mut s = Self {
            id,
            track_notes,
            track_controls,
            track_programs,
            n_notes_active: AtomicI32::new(0),
            notes_active: if track_notes { vec![NOTE_INACTIVE; 16 * 128] } else { vec![] },
            controls: if track_controls { vec![CC_VALUE_UNKNOWN; 16 * 128] } else { vec![] },
            programs: if track_programs { vec![PROGRAM_UNKNOWN; 16] } else { vec![] },
            pitch_wheel: if track_controls { vec![PITCH_WHEEL_DEFAULT; 16] } else { vec![] },
            channel_pressure: if track_controls { vec![CHANNEL_PRESSURE_UNKNOWN; 16] } else { vec![] },
            subscribers: Vec::new(),
        };
        s
    }

    #[inline]
    fn note_index(channel: u8, note: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (note as usize)
    }

    #[inline]
    fn cc_index(channel: u8, cc: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (cc as usize)
    }

    fn default_cc(_channel: u8, controller: u8) -> u8 {
        if controller == 64 || controller == 69 {
            0
        } else {
            CC_VALUE_UNKNOWN
        }
    }

    /// Send event notification to all subscribers (non-blocking, lock-free)
    fn notify(&self, event: Event) {
        // try_send: atomic CAS, returns immediately
        // If channel full, event dropped (acceptable - sync will rescan)
        eprintln!("[RUST] MidiStateTracker::notify() id={} n_subscribers={}", self.id, self.subscribers.len());
        for (i, sender) in self.subscribers.iter().enumerate() {
            let result = sender.try_send(event);
            eprintln!("[RUST]   -> subscriber[{}] send result: {:?}", i, result);
        }
    }

    fn process_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        eprintln!("[RUST] MidiStateTracker::process_note_on() id={} ch={} note={} vel={}", 
                   self.id, channel, note, velocity);
        if self.notes_active.is_empty() {
            eprintln!("[RUST]   -> ignored (not tracking notes)");
            return;
        }
        let idx = Self::note_index(channel, note);
        let was_inactive = self.notes_active[idx] == NOTE_INACTIVE;
        let old_vel = self.notes_active[idx];
        
        // Update state first
        self.notes_active[idx] = velocity;
        if was_inactive {
            self.n_notes_active.fetch_add(1, Ordering::Relaxed);
        }
        
        // Notify via channel (non-blocking, lock-free)
        // Only if state actually changed
        if was_inactive || old_vel != velocity {
            let event = Event::note(self.id, channel, note, velocity);
            self.notify(event);
        }
    }

    fn process_note_off(&mut self, channel: u8, note: u8) {
        eprintln!("[RUST] MidiStateTracker::process_note_off() id={} ch={} note={}", 
                   self.id, channel, note);
        if self.notes_active.is_empty() {
            eprintln!("[RUST]   -> ignored (not tracking notes)");
            return;
        }
        let idx = Self::note_index(channel, note);
        let was_active = self.notes_active[idx] != NOTE_INACTIVE;
        
        // Update state first
        self.notes_active[idx] = NOTE_INACTIVE;
        if was_active {
            self.n_notes_active.fetch_sub(1, Ordering::Relaxed);
        }
        
        // Notify via channel (only if state changed)
        if was_active {
            let event = Event::note_off(self.id, channel, note);
            self.notify(event);
        }
    }

    fn process_all_notes_off(&mut self, channel: u8) {
        if self.notes_active.is_empty() {
            return;
        }
        for note in 0..128u8 {
            let idx = Self::note_index(channel, note);
            if self.notes_active[idx] != NOTE_INACTIVE {
                self.n_notes_active.fetch_sub(1, Ordering::Relaxed);
                self.notes_active[idx] = NOTE_INACTIVE;
                let event = Event::note_off(self.id, channel, note);
                self.notify(event);
            }
        }
    }

    fn process_cc(&mut self, channel: u8, controller: u8, value: u8) {
        if self.controls.is_empty() {
            return;
        }
        let idx = Self::cc_index(channel, controller);
        let current = self.controls[idx];
        let def = Self::default_cc(channel, controller);
        
        if current != value || current == def {
            self.controls[idx] = value;
            let event = Event::cc(self.id, channel, controller, value);
            self.notify(event);
        }
    }

    fn process_program(&mut self, channel: u8, value: u8) {
        if self.programs.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        if self.programs[ch] != value {
            self.programs[ch] = value;
            let event = Event::program(self.id, channel, value);
            self.notify(event);
        }
    }

    fn process_pitch_wheel(&mut self, channel: u8, value: u16) {
        if self.pitch_wheel.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        eprintln!("[RUST] MidiStateTracker::process_pitch_wheel() id={} ch={} value={} (0x{:04x}) -> was={}", 
                   self.id, channel, value, value, self.pitch_wheel[ch]);
        if self.pitch_wheel[ch] != value {
            self.pitch_wheel[ch] = value;
            let event = Event::pitch(self.id, channel, value);
            eprintln!("[RUST]   -> sending event");
            self.notify(event);
        } else {
            eprintln!("[RUST]   -> no change, not sending event");
        }
    }

    fn process_channel_pressure(&mut self, channel: u8, value: u8) {
        if self.channel_pressure.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        if self.channel_pressure[ch] != value {
            self.channel_pressure[ch] = value;
            let event = Event::pressure(self.id, channel, value);
            self.notify(event);
        }
    }

    /// Copy relevant state from another tracker (used for loop start state)
    pub fn copy_relevant_state(&mut self, other: &MidiStateTracker) {
        if self.track_notes && other.track_notes {
            let n_notes = other.n_notes_active.load(Ordering::Relaxed);
            self.n_notes_active.store(n_notes, Ordering::Relaxed);
            self.notes_active.copy_from_slice(&other.notes_active);
        }
        if self.track_controls && other.track_controls {
            self.controls.copy_from_slice(&other.controls);
            self.pitch_wheel.copy_from_slice(&other.pitch_wheel);
            self.channel_pressure.copy_from_slice(&other.channel_pressure);
        }
        if self.track_programs && other.track_programs {
            self.programs.copy_from_slice(&other.programs);
        }
    }

    /// Reset state to defaults
    pub fn clear(&mut self) {
        for v in &mut self.notes_active {
            *v = NOTE_INACTIVE;
        }
        self.n_notes_active.store(0, Ordering::Relaxed);
        for v in &mut self.pitch_wheel {
            *v = PITCH_WHEEL_DEFAULT;
        }
        for v in &mut self.programs {
            *v = PROGRAM_UNKNOWN;
        }
        for v in &mut self.channel_pressure {
            *v = CHANNEL_PRESSURE_UNKNOWN;
        }
        for i in 0..self.controls.len() {
            let channel = (i / 128) as u8;
            let controller = (i % 128) as u8;
            self.controls[i] = Self::default_cc(channel, controller);
        }
    }

    /// Process a MIDI message
    pub fn process_msg(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        eprintln!("[RUST] MidiStateTracker::process_msg() id={} data={:?}", self.id, data);
        if let Some(ch) = midi_helpers::is_all_notes_off_for_channel(data) {
            self.process_all_notes_off(ch as u8);
        } else if let Some(ch) = midi_helpers::is_all_sound_off_for_channel(data) {
            self.process_all_notes_off(ch as u8);
        } else if midi_helpers::is_note_on(data) {
            self.process_note_on(
                midi_helpers::channel(data) as u8,
                data[1],
                data[2],
            );
        } else if midi_helpers::is_note_off(data) {
            self.process_note_off(midi_helpers::channel(data) as u8, data[1]);
        } else if midi_helpers::is_pitch_wheel(data) {
            let value = (data[1] as u16) | ((data[2] as u16) << 7);
            self.process_pitch_wheel(midi_helpers::channel(data) as u8, value);
        } else if midi_helpers::is_channel_pressure(data) {
            self.process_channel_pressure(midi_helpers::channel(data) as u8, data[1]);
        } else if midi_helpers::is_program(data) {
            self.process_program(midi_helpers::channel(data) as u8, data[1]);
        } else if midi_helpers::is_cc(data) {
            self.process_cc(midi_helpers::channel(data) as u8, data[1], data[2]);
        }
    }

    // Public accessors

    pub fn tracking_notes(&self) -> bool {
        !self.notes_active.is_empty()
    }

    pub fn n_notes_active(&self) -> u32 {
        self.n_notes_active.load(Ordering::Relaxed) as u32
    }

    pub fn maybe_current_note_velocity(&self, channel: u8, note: u8) -> u8 {
        let idx = Self::note_index(channel, note);
        self.notes_active[idx]
    }

    pub fn tracking_controls(&self) -> bool {
        !self.controls.is_empty()
    }

    pub fn maybe_cc_value(&self, channel: u8, controller: u8) -> u8 {
        let idx = Self::cc_index(channel, controller);
        self.controls[idx]
    }

    pub fn maybe_pitch_wheel_value(&self, channel: u8) -> u16 {
        self.pitch_wheel[(channel & 0x0F) as usize]
    }

    pub fn maybe_channel_pressure_value(&self, channel: u8) -> u8 {
        self.channel_pressure[(channel & 0x0F) as usize]
    }

    pub fn tracking_programs(&self) -> bool {
        !self.programs.is_empty()
    }

    pub fn maybe_program_value(&self, channel: u8) -> u8 {
        self.programs[(channel & 0x0F) as usize]
    }

    pub fn tracking_anything(&self) -> bool {
        self.tracking_programs() || self.tracking_controls() || self.tracking_notes()
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    /// Create a subscription for a diff tracker
    /// Returns a receiver that will receive events from this tracker
    pub fn create_subscription(&mut self) -> crossbeam_channel::Receiver<Event> {
        eprintln!("[RUST] MidiStateTracker::create_subscription() id={} current_subscribers={}", self.id, self.subscribers.len());
        let (tx, rx) = bounded::<Event>(EVENT_CHANNEL_CAPACITY);
        self.subscribers.push(tx);
        eprintln!("[RUST]   -> created subscription, total now: {}", self.subscribers.len());
        rx
    }

    /// Get current state data as TrackerStateData
    pub fn get_state_data(&self) -> TrackerStateData {
        TrackerStateData {
            notes_active: self.notes_active.clone(),
            controls: self.controls.clone(),
            programs: self.programs.clone(),
            pitch_wheel: self.pitch_wheel.clone(),
            channel_pressure: self.channel_pressure.clone(),
        }
    }

    pub fn state_as_messages_flat(&self) -> Vec<u8> {
        let mut flat = Vec::new();
        
        if self.tracking_programs() {
            for channel in 0..self.programs.len() {
                let v = self.programs[channel];
                if v != PROGRAM_UNKNOWN {
                    let msg = midi_helpers::program_change(channel as u8, v);
                    flat.push(msg.len() as u8);
                    flat.extend_from_slice(&msg);
                }
            }
        }
        if self.tracking_controls() {
            for channel in 0..16u8 {
                let pw = self.pitch_wheel[channel as usize];
                if pw != PITCH_WHEEL_UNKNOWN && pw != PITCH_WHEEL_DEFAULT {
                    let msg = midi_helpers::pitch_wheel_change(channel, pw);
                    flat.push(msg.len() as u8);
                    flat.extend_from_slice(&msg);
                }
                let cp = self.channel_pressure[channel as usize];
                if cp != CHANNEL_PRESSURE_UNKNOWN {
                    let msg = midi_helpers::channel_pressure(channel, cp);
                    flat.push(msg.len() as u8);
                    flat.extend_from_slice(&msg);
                }
                for controller in 0..128u8 {
                    let v = self.controls[Self::cc_index(channel, controller)];
                    let def = Self::default_cc(channel, controller);
                    if v != def {
                        let msg = midi_helpers::cc(channel, controller, v);
                        flat.push(msg.len() as u8);
                        flat.extend_from_slice(&msg);
                    }
                }
            }
        }
        if self.tracking_notes() {
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    let v = self.notes_active[Self::note_index(channel, note)];
                    if v != NOTE_INACTIVE {
                        let msg = midi_helpers::note_on(channel, note, v);
                        flat.push(msg.len() as u8);
                        flat.extend_from_slice(&msg);
                    }
                }
            }
        }
        flat
    }

    pub unsafe fn process_msg_raw(&mut self, data: *const u8) {
        if data.is_null() {
            return;
        }
        let status = unsafe { *data };
        let len = midi_helpers::midi_message_len(status);
        let slice = unsafe { std::slice::from_raw_parts(data, len) };
        self.process_msg(slice);
    }
}

// C++-callable constructors and methods

pub fn new_midi_state_tracker(
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
) -> Box<MidiStateTracker> {
    Box::new(MidiStateTracker::new(track_notes, track_controls, track_programs))
}

pub fn copy_relevant_state(this: &mut MidiStateTracker, other: &MidiStateTracker) {
    this.copy_relevant_state(other);
}

pub fn clear(this: &mut MidiStateTracker) {
    this.clear();
}

pub unsafe fn process_msg_raw(this: &mut MidiStateTracker, data: *const u8) {
    if data.is_null() {
        return;
    }
    let status = unsafe { *data };
    let len = midi_helpers::midi_message_len(status);
    let slice = unsafe { std::slice::from_raw_parts(data, len) };
    this.process_msg(slice);
}

pub fn tracking_notes(this: &MidiStateTracker) -> bool {
    this.tracking_notes()
}

pub fn n_notes_active(this: &MidiStateTracker) -> u32 {
    this.n_notes_active()
}

pub fn maybe_current_note_velocity(this: &MidiStateTracker, channel: u8, note: u8) -> u8 {
    this.maybe_current_note_velocity(channel, note)
}

pub fn tracking_controls(this: &MidiStateTracker) -> bool {
    this.tracking_controls()
}

pub fn maybe_cc_value(this: &MidiStateTracker, channel: u8, controller: u8) -> u8 {
    this.maybe_cc_value(channel, controller)
}

pub fn maybe_pitch_wheel_value(this: &MidiStateTracker, channel: u8) -> u16 {
    this.maybe_pitch_wheel_value(channel)
}

pub fn maybe_channel_pressure_value(this: &MidiStateTracker, channel: u8) -> u8 {
    this.maybe_channel_pressure_value(channel)
}

pub fn tracking_programs(this: &MidiStateTracker) -> bool {
    this.tracking_programs()
}

pub fn maybe_program_value(this: &MidiStateTracker, channel: u8) -> u8 {
    this.maybe_program_value(channel)
}

pub fn tracking_anything(this: &MidiStateTracker) -> bool {
    this.tracking_anything()
}

pub fn get_id(this: &MidiStateTracker) -> u64 {
    this.get_id()
}

pub fn state_as_messages_flat(this: &MidiStateTracker) -> Vec<u8> {
    this.state_as_messages_flat()
}