use crate::midi_helpers;
use std::sync::Mutex;

const UNKNOWN_8: u8 = 0x80;
const UNKNOWN_16: u16 = 0x8000;
const PITCH_WHEEL_DEFAULT: u16 = 0x2000;
const NOTE_INACTIVE: u8 = UNKNOWN_8;
const CC_VALUE_UNKNOWN: u8 = UNKNOWN_8;
const PROGRAM_UNKNOWN: u8 = UNKNOWN_8;
const CHANNEL_PRESSURE_UNKNOWN: u8 = UNKNOWN_8;
const PITCH_WHEEL_UNKNOWN: u16 = UNKNOWN_16;

pub struct MidiStateTracker {
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
    n_notes_active: std::sync::atomic::AtomicI32,
    notes_active_velocities: Vec<u8>,
    controls: Vec<u8>,
    programs: Vec<u8>,
    pitch_wheel: Vec<u16>,
    channel_pressure: Vec<u8>,
    subscribers: Mutex<Vec<*mut crate::midi_state_diff_tracker::MidiStateDiffTracker>>,
}

impl MidiStateTracker {
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        let mut s = Self {
            track_notes,
            track_controls,
            track_programs,
            n_notes_active: std::sync::atomic::AtomicI32::new(0),
            notes_active_velocities: if track_notes { vec![NOTE_INACTIVE; 16 * 128] } else { vec![] },
            controls: if track_controls { vec![CC_VALUE_UNKNOWN; 16 * 128] } else { vec![] },
            programs: if track_programs { vec![PROGRAM_UNKNOWN; 16] } else { vec![] },
            pitch_wheel: if track_controls { vec![PITCH_WHEEL_DEFAULT; 16] } else { vec![] },
            channel_pressure: if track_controls { vec![CHANNEL_PRESSURE_UNKNOWN; 16] } else { vec![] },
            subscribers: Mutex::new(Vec::new()),
        };
        s.clear();
        s
    }

    fn cc_index(channel: u8, cc: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (cc as usize)
    }

    fn note_index(channel: u8, note: u8) -> usize {
        ((channel & 0x0F) as usize) * 128 + (note as usize)
    }

    fn default_cc(_channel: u8, controller: u8) -> u8 {
        if controller == 64 || controller == 69 {
            0
        } else {
            CC_VALUE_UNKNOWN
        }
    }

    fn notify_note_changed(&self, channel: u8, note: u8, maybe_velocity: Option<u8>) {
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            unsafe {
                (*sub).note_changed(self, channel, note, maybe_velocity);
            }
        }
    }

    fn notify_cc_changed(&self, channel: u8, cc: u8, value: Option<u8>) {
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            unsafe {
                (*sub).cc_changed(self, channel, cc, value);
            }
        }
    }

    fn notify_program_changed(&self, channel: u8, program: Option<u8>) {
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            unsafe {
                (*sub).program_changed(self, channel, program);
            }
        }
    }

    fn notify_pitch_wheel_changed(&self, channel: u8, pitch: Option<u16>) {
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            unsafe {
                (*sub).pitch_wheel_changed(self, channel, pitch);
            }
        }
    }

    fn notify_channel_pressure_changed(&self, channel: u8, pressure: Option<u8>) {
        let subs = self.subscribers.lock().unwrap().clone();
        for sub in subs {
            unsafe {
                (*sub).channel_pressure_changed(self, channel, pressure);
            }
        }
    }

    fn process_note_on(&mut self, channel: u8, note: u8, velocity: u8) {
        if self.notes_active_velocities.is_empty() {
            return;
        }
        let idx = Self::note_index(channel, note);
        if self.notes_active_velocities[idx] == NOTE_INACTIVE {
            self.n_notes_active.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if self.notes_active_velocities[idx] != velocity {
            self.notify_note_changed(channel, note, Some(velocity));
        }
        self.notes_active_velocities[idx] = velocity;
    }

    fn process_note_off(&mut self, channel: u8, note: u8) {
        if self.notes_active_velocities.is_empty() {
            return;
        }
        let idx = Self::note_index(channel, note);
        if self.notes_active_velocities[idx] != NOTE_INACTIVE {
            self.n_notes_active.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
            self.notify_note_changed(channel, note, None);
        }
        self.notes_active_velocities[idx] = NOTE_INACTIVE;
    }

    fn process_all_notes_off(&mut self, channel: u8) {
        if self.notes_active_velocities.is_empty() {
            return;
        }
        for note in 0..128u8 {
            let idx = Self::note_index(channel, note);
            if self.notes_active_velocities[idx] != NOTE_INACTIVE {
                self.n_notes_active.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
                self.notify_note_changed(channel, note, None);
            }
            self.notes_active_velocities[idx] = NOTE_INACTIVE;
        }
    }

    fn process_cc(&mut self, channel: u8, controller: u8, value: u8) {
        if self.controls.is_empty() {
            return;
        }
        let idx = Self::cc_index(channel & 0x0F, controller);
        let current = self.controls[idx];
        if current != value {
            self.notify_cc_changed(channel, controller, Some(value));
        }
        self.controls[idx] = value;
    }

    fn process_program(&mut self, channel: u8, value: u8) {
        if self.programs.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        if self.programs[ch] != value {
            self.notify_program_changed(channel, Some(value));
        }
        self.programs[ch] = value;
    }

    fn process_pitch_wheel(&mut self, channel: u8, value: u16) {
        if self.pitch_wheel.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        if self.pitch_wheel[ch] != value {
            self.notify_pitch_wheel_changed(channel, Some(value));
        }
        self.pitch_wheel[ch] = value;
    }

    fn process_channel_pressure(&mut self, channel: u8, value: u8) {
        if self.channel_pressure.is_empty() {
            return;
        }
        let ch = (channel & 0x0F) as usize;
        if self.channel_pressure[ch] != value {
            self.notify_channel_pressure_changed(channel, Some(value));
        }
        self.channel_pressure[ch] = value;
    }

    pub fn copy_relevant_state(&mut self, other: &MidiStateTracker) {
        if self.track_notes && other.track_notes {
            self.n_notes_active.store(
                other.n_notes_active.load(std::sync::atomic::Ordering::Relaxed),
                std::sync::atomic::Ordering::Relaxed,
            );
            self.notes_active_velocities.copy_from_slice(&other.notes_active_velocities);
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

    pub fn clear(&mut self) {
        for v in &mut self.notes_active_velocities {
            *v = NOTE_INACTIVE;
        }
        self.n_notes_active.store(0, std::sync::atomic::Ordering::Relaxed);
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

    pub fn process_msg(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
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

    pub fn tracking_notes(&self) -> bool {
        !self.notes_active_velocities.is_empty()
    }

    pub fn n_notes_active(&self) -> u32 {
        self.n_notes_active.load(std::sync::atomic::Ordering::Relaxed) as u32
    }

    pub fn maybe_current_note_velocity(&self, channel: u8, note: u8) -> u8 {
        let idx = Self::note_index(channel, note);
        self.notes_active_velocities[idx]
    }

    pub fn tracking_controls(&self) -> bool {
        !self.controls.is_empty()
    }

    pub fn maybe_cc_value(&self, channel: u8, controller: u8) -> u8 {
        let idx = Self::cc_index(channel & 0x0F, controller);
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

    pub fn subscribe(&mut self, subscriber: *mut crate::midi_state_diff_tracker::MidiStateDiffTracker) {
        self.subscribers.lock().unwrap().push(subscriber);
    }

    pub fn unsubscribe(&mut self, subscriber: *mut crate::midi_state_diff_tracker::MidiStateDiffTracker) {
        let mut subs = self.subscribers.lock().unwrap();
        subs.retain(|&s| s != subscriber);
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
                    if v != Self::default_cc(channel, controller) {
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
                    let v = self.notes_active_velocities[Self::note_index(channel, note)];
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

pub fn process_msg_raw(this: &mut MidiStateTracker, data: *const u8) {
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

pub fn subscribe(this: &mut MidiStateTracker, subscriber: *mut crate::midi_state_diff_tracker::MidiStateDiffTracker) {
    this.subscribe(subscriber);
}

pub fn unsubscribe(this: &mut MidiStateTracker, subscriber: *mut crate::midi_state_diff_tracker::MidiStateDiffTracker) {
    this.unsubscribe(subscriber);
}

pub fn state_as_messages_flat(this: &MidiStateTracker) -> Vec<u8> {
    this.state_as_messages_flat()
}
