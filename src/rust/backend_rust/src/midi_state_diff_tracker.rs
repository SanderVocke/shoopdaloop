use crate::midi_state_tracker::MidiStateTracker;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Mutex;

pub struct MidiStateDiffTracker {
    diffs: Mutex<BTreeSet<[u8; 2]>>,
    a: AtomicPtr<MidiStateTracker>,
    b: AtomicPtr<MidiStateTracker>,
}

impl MidiStateDiffTracker {
    pub fn new() -> Self {
        Self {
            diffs: Mutex::new(BTreeSet::new()),
            a: AtomicPtr::new(std::ptr::null_mut()),
            b: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    fn get_a(&self) -> Option<&mut MidiStateTracker> {
        let ptr = self.a.load(Ordering::Relaxed);
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }

    fn get_b(&self) -> Option<&mut MidiStateTracker> {
        let ptr = self.b.load(Ordering::Relaxed);
        if ptr.is_null() {
            None
        } else {
            Some(unsafe { &mut *ptr })
        }
    }

    fn other_tracker<'a>(
        &'a self,
        tracker: &MidiStateTracker,
    ) -> Option<&'a mut MidiStateTracker> {
        let a = self.a.load(Ordering::Relaxed);
        let b = self.b.load(Ordering::Relaxed);
        let ptr = tracker as *const MidiStateTracker as *mut MidiStateTracker;
        if ptr == a {
            if b.is_null() {
                None
            } else {
                Some(unsafe { &mut *b })
            }
        } else if ptr == b {
            if a.is_null() {
                None
            } else {
                Some(unsafe { &mut *a })
            }
        } else {
            None
        }
    }

    pub fn reset(&mut self, a: *mut MidiStateTracker, b: *mut MidiStateTracker) {
        self.a.store(a, Ordering::Relaxed);
        self.b.store(b, Ordering::Relaxed);
        self.diffs.lock().unwrap().clear();
    }

    pub fn add_diff(&self, d0: u8, d1: u8) {
        self.diffs.lock().unwrap().insert([d0, d1]);
    }

    pub fn delete_diff(&self, d0: u8, d1: u8) {
        self.diffs.lock().unwrap().remove(&[d0, d1]);
    }

    pub fn check_note(&self, channel: u8, note: u8) {
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_notes() && b.tracking_notes() {
            let av = a.maybe_current_note_velocity(channel, note);
            let bv = b.maybe_current_note_velocity(channel, note);
            let mut diffs = self.diffs.lock().unwrap();
            if av == bv {
                diffs.remove(&[0x90 | channel, note]);
            } else {
                diffs.insert([0x90 | channel, note]);
            }
        }
    }

    pub fn check_cc(&self, channel: u8, controller: u8) {
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_controls() && b.tracking_controls() {
            let av = a.maybe_cc_value(channel, controller);
            let bv = b.maybe_cc_value(channel, controller);
            let mut diffs = self.diffs.lock().unwrap();
            if av == bv {
                diffs.remove(&[0xB0 | channel, controller]);
            } else {
                diffs.insert([0xB0 | channel, controller]);
            }
        }
    }

    pub fn check_program(&self, channel: u8) {
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_programs() && b.tracking_programs() {
            let av = a.maybe_program_value(channel);
            let bv = b.maybe_program_value(channel);
            let mut diffs = self.diffs.lock().unwrap();
            if av == bv {
                diffs.remove(&[0xC0 | channel, 0]);
            } else {
                diffs.insert([0xC0 | channel, 0]);
            }
        }
    }

    pub fn check_channel_pressure(&self, channel: u8) {
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_controls() && b.tracking_controls() {
            let av = a.maybe_channel_pressure_value(channel);
            let bv = b.maybe_channel_pressure_value(channel);
            let mut diffs = self.diffs.lock().unwrap();
            if av == bv {
                diffs.remove(&[0xD0 | channel, 0]);
            } else {
                diffs.insert([0xD0 | channel, 0]);
            }
        }
    }

    pub fn check_pitch_wheel(&self, channel: u8) {
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_controls() && b.tracking_controls() {
            let av = a.maybe_pitch_wheel_value(channel);
            let bv = b.maybe_pitch_wheel_value(channel);
            let mut diffs = self.diffs.lock().unwrap();
            if av == bv {
                diffs.remove(&[0xE0 | channel, 0]);
            } else {
                diffs.insert([0xE0 | channel, 0]);
            }
        }
    }

    pub fn rescan_diff(&self) {
        self.clear_diff();
        let Some(a) = self.get_a() else { return };
        let Some(b) = self.get_b() else { return };
        if a.tracking_notes() && b.tracking_notes() {
            for channel in 0..16u8 {
                for note in 0..128u8 {
                    self.check_note(channel, note);
                }
            }
        }
        if a.tracking_controls() && b.tracking_controls() {
            for channel in 0..16u8 {
                for controller in 0..128u8 {
                    self.check_cc(channel, controller);
                }
                self.check_pitch_wheel(channel);
                self.check_channel_pressure(channel);
            }
        }
        if a.tracking_programs() && b.tracking_programs() {
            for channel in 0..16u8 {
                self.check_program(channel);
            }
        }
    }

    pub fn clear_diff(&self) {
        self.diffs.lock().unwrap().clear();
    }

    pub fn resolve_to(
        &self,
        to: *mut MidiStateTracker,
        notes: bool,
        controls: bool,
        programs: bool,
    ) -> Vec<u8> {
        let to = unsafe { &mut *to };
        let Some(a) = self.get_a() else { return Vec::new() };
        let Some(b) = self.get_b() else { return Vec::new() };
        let from = if std::ptr::eq(to, a) {
            b
        } else if std::ptr::eq(to, b) {
            a
        } else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let diffs = self.diffs.lock().unwrap();
        for d in diffs.iter() {
            let kind_part = d[0] & 0xF0;
            let channel_part = d[0] & 0x0F;
            match kind_part {
                0x80 | 0x90 => {
                    if notes {
                        let v = from.maybe_current_note_velocity(d[0], d[1]);
                        if v != 0x80 {
                            out.push(0x90 | channel_part);
                            out.push(d[1]);
                            out.push(v);
                        } else {
                            out.push(0x80 | channel_part);
                            out.push(d[1]);
                            out.push(64);
                        }
                    }
                }
                0xB0 => {
                    if controls {
                        let v = from.maybe_cc_value(channel_part, d[1]);
                        if v != 0x80 {
                            out.push(d[0]);
                            out.push(d[1]);
                            out.push(v);
                        }
                    }
                }
                0xC0 => {
                    if programs {
                        let v = from.maybe_program_value(channel_part);
                        if v != 0x80 {
                            out.push(d[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xD0 => {
                    if controls {
                        let v = from.maybe_channel_pressure_value(channel_part);
                        if v != 0x80 {
                            out.push(d[0]);
                            out.push(0);
                            out.push(v);
                        }
                    }
                }
                0xE0 => {
                    if controls {
                        let v = from.maybe_pitch_wheel_value(channel_part);
                        if v != 0x8000 {
                            out.push(d[0]);
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

    pub fn get_diff_flat(&self) -> Vec<u8> {
        let diffs = self.diffs.lock().unwrap();
        let mut out = Vec::with_capacity(diffs.len() * 2);
        for d in diffs.iter() {
            out.push(d[0]);
            out.push(d[1]);
        }
        out
    }

    pub fn set_diff_flat(&self, data: &[u8]) {
        let mut diffs = self.diffs.lock().unwrap();
        diffs.clear();
        for chunk in data.chunks_exact(2) {
            diffs.insert([chunk[0], chunk[1]]);
        }
    }

    // Callbacks from MidiStateTracker
    pub fn note_changed(
        &self,
        tracker: &MidiStateTracker,
        channel: u8,
        note: u8,
        maybe_velocity: Option<u8>,
    ) {
        let Some(other) = self.other_tracker(tracker) else { return };
        let other_vel = other.maybe_current_note_velocity(channel, note);
        let other_vel_opt = if other_vel == 0x80 { None } else { Some(other_vel) };
        if other.tracking_notes() && other_vel_opt != maybe_velocity {
            self.diffs.lock().unwrap().insert([0x90 | channel, note]);
        } else {
            self.diffs.lock().unwrap().remove(&[0x90 | channel, note]);
        }
    }

    pub fn cc_changed(
        &self,
        tracker: &MidiStateTracker,
        channel: u8,
        cc: u8,
        maybe_value: Option<u8>,
    ) {
        let Some(other) = self.other_tracker(tracker) else { return };
        let other_val = other.maybe_cc_value(channel, cc);
        let other_val_opt = if other_val == 0x80 { None } else { Some(other_val) };
        if other.tracking_controls() && other_val_opt != maybe_value && maybe_value.is_some() {
            self.diffs.lock().unwrap().insert([0xB0 | channel, cc]);
        } else {
            self.diffs.lock().unwrap().remove(&[0xB0 | channel, cc]);
        }
    }

    pub fn program_changed(
        &self,
        tracker: &MidiStateTracker,
        channel: u8,
        maybe_program: Option<u8>,
    ) {
        let Some(other) = self.other_tracker(tracker) else { return };
        let other_val = other.maybe_program_value(channel);
        let other_val_opt = if other_val == 0x80 { None } else { Some(other_val) };
        if other.tracking_programs() && other_val_opt != maybe_program && maybe_program.is_some() {
            self.diffs.lock().unwrap().insert([0xC0 | channel, 0]);
        } else {
            self.diffs.lock().unwrap().remove(&[0xC0 | channel, 0]);
        }
    }

    pub fn pitch_wheel_changed(
        &self,
        tracker: &MidiStateTracker,
        channel: u8,
        maybe_pitch: Option<u16>,
    ) {
        let Some(other) = self.other_tracker(tracker) else { return };
        let other_val = other.maybe_pitch_wheel_value(channel);
        let other_val_opt = if other_val == 0x8000 { None } else { Some(other_val) };
        if other.tracking_controls() && other_val_opt != maybe_pitch && maybe_pitch.is_some() {
            self.diffs.lock().unwrap().insert([0xE0 | channel, 0]);
        } else {
            self.diffs.lock().unwrap().remove(&[0xE0 | channel, 0]);
        }
    }

    pub fn channel_pressure_changed(
        &self,
        tracker: &MidiStateTracker,
        channel: u8,
        maybe_pressure: Option<u8>,
    ) {
        let Some(other) = self.other_tracker(tracker) else { return };
        let other_val = other.maybe_channel_pressure_value(channel);
        let other_val_opt = if other_val == 0x80 { None } else { Some(other_val) };
        if other.tracking_controls() && other_val_opt != maybe_pressure && maybe_pressure.is_some() {
            self.diffs.lock().unwrap().insert([0xE0 | channel, 0]);
        } else {
            self.diffs.lock().unwrap().remove(&[0xD0 | channel, 0]);
        }
    }
}

pub fn new_midi_state_diff_tracker() -> Box<MidiStateDiffTracker> {
    Box::new(MidiStateDiffTracker::new())
}

pub fn reset(
    this: &mut MidiStateDiffTracker,
    a: *mut MidiStateTracker,
    b: *mut MidiStateTracker,
) {
    this.reset(a, b);
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

pub fn resolve_to(
    this: &MidiStateDiffTracker,
    to: *mut MidiStateTracker,
    notes: bool,
    controls: bool,
    programs: bool,
) -> Vec<u8> {
    this.resolve_to(to, notes, controls, programs)
}

pub fn get_diff_flat(this: &MidiStateDiffTracker) -> Vec<u8> {
    this.get_diff_flat()
}

pub fn set_diff_flat(this: &mut MidiStateDiffTracker, data: &[u8]) {
    this.set_diff_flat(data);
}
