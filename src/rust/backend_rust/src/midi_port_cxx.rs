//! CXX bridge for MidiPort to expose to C++.

#![allow(dead_code)]

use crate::midi_port::MidiPort;
use crate::midi_state_tracker::MidiStateTracker;
use crate::midi_port_base::MidiPortBase;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiPort;

        // Constructors
        fn new_midi_port(
            track_notes: bool,
            track_controls: bool,
            track_programs: bool,
        ) -> Box<MidiPort>;

        // State tracking methods
        fn n_notes_active(self: &MidiPort) -> u32;
        fn input_event_count(self: &MidiPort) -> u32;
        fn output_event_count(self: &MidiPort) -> u32;

        // Event counter management
        fn reset_n_input_events(self: &MidiPort);
        fn reset_n_output_events(self: &MidiPort);

        // Event counter access (new - for C++ interop)
        fn get_n_input_events(port: &MidiPort) -> u32;
        fn get_n_output_events(port: &MidiPort) -> u32;
        fn increment_input_events(port: &MidiPort, count: u32);
        fn increment_output_events(port: &MidiPort, count: u32);

        // Mute state
        fn set_muted(self: &MidiPort, muted: bool);
        fn get_muted(self: &MidiPort) -> bool;

        // Ringbuffer methods
        fn set_ringbuffer_n_samples(self: &mut MidiPort, n: u32);
        fn get_ringbuffer_n_samples(self: &MidiPort) -> u32;
        fn get_current_n_samples(port: &MidiPort) -> u32;

        // Direct state tracker access (returns raw pointer as usize, 0 = None)
        // C++ is responsible for lifetime management (same lifetime as MidiPort)
        // Note: Accepts immutable reference since this is just reading a pointer
        unsafe fn get_maybe_midi_state_tracker(port: &MidiPort) -> usize;
        unsafe fn get_maybe_ringbuffer_tail_state_tracker(port: &MidiPort) -> usize;

        // State processing for C++ callers
        unsafe fn process_msg_to_state(port: &mut MidiPort, data: *const u8, size: usize);
    }
}

fn new_midi_port(track_notes: bool, track_controls: bool, track_programs: bool) -> Box<MidiPort> {
    Box::new(MidiPort::new(track_notes, track_controls, track_programs))
}

fn n_notes_active(port: &MidiPort) -> u32 {
    port.n_notes_active()
}

fn input_event_count(port: &MidiPort) -> u32 {
    port.input_event_count()
}

fn output_event_count(port: &MidiPort) -> u32 {
    port.output_event_count()
}

fn reset_n_input_events(port: &MidiPort) {
    port.reset_n_input_events();
}

fn reset_n_output_events(port: &MidiPort) {
    port.reset_n_output_events();
}

fn set_muted(port: &MidiPort, muted: bool) {
    port.set_muted(muted);
}

fn get_muted(port: &MidiPort) -> bool {
    port.get_muted()
}

fn set_ringbuffer_n_samples(port: &mut MidiPort, n: u32) {
    port.set_ringbuffer_n_samples(n);
}

fn get_ringbuffer_n_samples(port: &MidiPort) -> u32 {
    port.get_ringbuffer_n_samples()
}

fn get_n_input_events(port: &MidiPort) -> u32 {
    port.get_n_input_events()
}

fn get_n_output_events(port: &MidiPort) -> u32 {
    port.get_n_output_events()
}

fn increment_input_events(port: &MidiPort, count: u32) {
    port.increment_input_events(count);
}

fn increment_output_events(port: &MidiPort, count: u32) {
    port.increment_output_events(count);
}

fn get_current_n_samples(port: &MidiPort) -> u32 {
    port.get_current_n_samples()
}

// Direct state tracker access (returns raw pointer as usize, 0 = None)
// C++ is responsible for lifetime management (same lifetime as MidiPort)
// Note: Takes immutable reference since this is just reading a pointer
unsafe fn get_maybe_midi_state_tracker(port: &MidiPort) -> usize {
    // SAFETY: Caller ensures MidiPort lifetime covers the returned pointer
    // We need to get mutable access to base's state tracker from an immutable reference
    let base_ptr = port.base() as *const MidiPortBase as *mut MidiPortBase;
    (*base_ptr).maybe_midi_state_tracker()
        .map(|t| t as *mut MidiStateTracker as usize)
        .unwrap_or(0)
}

unsafe fn get_maybe_ringbuffer_tail_state_tracker(port: &MidiPort) -> usize {
    // SAFETY: Caller ensures MidiPort lifetime covers the returned pointer
    let base_ptr = port.base() as *const MidiPortBase as *mut MidiPortBase;
    (*base_ptr).maybe_ringbuffer_tail_state_tracker()
        .map(|t| t as *mut MidiStateTracker as usize)
        .unwrap_or(0)
}

// State processing for C++ callers
unsafe fn process_msg_to_state(port: &mut MidiPort, data: *const u8, size: usize) {
    port.process_msg_to_state(data, size);
}
