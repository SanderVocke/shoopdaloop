#![allow(dead_code)]

use crate::midi_state_tracker::MidiStateTracker;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiStateTracker;
        fn new_midi_state_tracker(
            track_notes: bool,
            track_controls: bool,
            track_programs: bool,
        ) -> Box<MidiStateTracker>;
        fn copy_relevant_state(self: &mut MidiStateTracker, other: &MidiStateTracker);
        fn clear(self: &mut MidiStateTracker);
        unsafe fn process_msg_raw(self: &mut MidiStateTracker, data: *const u8);
        fn tracking_notes(self: &MidiStateTracker) -> bool;
        fn n_notes_active(self: &MidiStateTracker) -> u32;
        fn maybe_current_note_velocity(self: &MidiStateTracker, channel: u8, note: u8) -> u8;
        fn tracking_controls(self: &MidiStateTracker) -> bool;
        fn maybe_cc_value(self: &MidiStateTracker, channel: u8, controller: u8) -> u8;
        fn maybe_pitch_wheel_value(self: &MidiStateTracker, channel: u8) -> u16;
        fn maybe_channel_pressure_value(self: &MidiStateTracker, channel: u8) -> u8;
        fn tracking_programs(self: &MidiStateTracker) -> bool;
        fn maybe_program_value(self: &MidiStateTracker, channel: u8) -> u8;
        fn tracking_anything(self: &MidiStateTracker) -> bool;
        fn get_id(self: &MidiStateTracker) -> u64;
        fn state_as_messages_flat(self: &MidiStateTracker) -> Vec<u8>;
    }
}

fn new_midi_state_tracker(
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
) -> Box<MidiStateTracker> {
    crate::midi_state_tracker::new_midi_state_tracker(track_notes, track_controls, track_programs)
}

fn copy_relevant_state(this: &mut MidiStateTracker, other: &MidiStateTracker) {
    crate::midi_state_tracker::copy_relevant_state(this, other)
}

fn clear(this: &mut MidiStateTracker) {
    crate::midi_state_tracker::clear(this)
}

unsafe fn process_msg_raw(this: &mut MidiStateTracker, data: *const u8) {
    crate::midi_state_tracker::process_msg_raw(this, data)
}

fn tracking_notes(this: &MidiStateTracker) -> bool {
    crate::midi_state_tracker::tracking_notes(this)
}

fn n_notes_active(this: &MidiStateTracker) -> u32 {
    crate::midi_state_tracker::n_notes_active(this)
}

fn maybe_current_note_velocity(this: &MidiStateTracker, channel: u8, note: u8) -> u8 {
    crate::midi_state_tracker::maybe_current_note_velocity(this, channel, note)
}

fn tracking_controls(this: &MidiStateTracker) -> bool {
    crate::midi_state_tracker::tracking_controls(this)
}

fn maybe_cc_value(this: &MidiStateTracker, channel: u8, controller: u8) -> u8 {
    crate::midi_state_tracker::maybe_cc_value(this, channel, controller)
}

fn maybe_pitch_wheel_value(this: &MidiStateTracker, channel: u8) -> u16 {
    crate::midi_state_tracker::maybe_pitch_wheel_value(this, channel)
}

fn maybe_channel_pressure_value(this: &MidiStateTracker, channel: u8) -> u8 {
    crate::midi_state_tracker::maybe_channel_pressure_value(this, channel)
}

fn tracking_programs(this: &MidiStateTracker) -> bool {
    crate::midi_state_tracker::tracking_programs(this)
}

fn maybe_program_value(this: &MidiStateTracker, channel: u8) -> u8 {
    crate::midi_state_tracker::maybe_program_value(this, channel)
}

fn tracking_anything(this: &MidiStateTracker) -> bool {
    crate::midi_state_tracker::tracking_anything(this)
}

fn get_id(this: &MidiStateTracker) -> u64 {
    crate::midi_state_tracker::get_id(this)
}

fn state_as_messages_flat(this: &MidiStateTracker) -> Vec<u8> {
    crate::midi_state_tracker::state_as_messages_flat(this)
}
