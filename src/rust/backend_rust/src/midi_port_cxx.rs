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
        unsafe fn get_maybe_midi_state_tracker(port: &mut MidiPort) -> usize;
        unsafe fn get_maybe_ringbuffer_tail_state_tracker(port: &mut MidiPort) -> usize;

        // Get n_notes_active directly from the state tracker
        // This avoids the raw pointer issue when calling from C++
        fn get_state_tracker_n_notes_active(port: &mut MidiPort) -> u32;

        // State processing for C++ callers - calls Rust directly
        unsafe fn process_msg_raw_to_state(port: &mut MidiPort, data: *const u8);

        // Ringbuffer tail state processing - calls Rust directly
        unsafe fn process_msg_raw_to_tail_state(port: &mut MidiPort, data: *const u8);

        // Copy ringbuffer tail state to a target MidiStateTracker by pointer
        // target_ptr is a raw pointer to the target MidiStateTracker
        // Returns true if the tail state was copied, false if no tail state exists
        unsafe fn copy_tail_state_to_tracker_by_ptr(port: &mut MidiPort, target_ptr: usize) -> bool;
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
unsafe fn get_maybe_midi_state_tracker(port: &mut MidiPort) -> usize {
    // SAFETY: Caller ensures MidiPort lifetime covers the returned pointer
    port.base_mut().maybe_midi_state_tracker()
        .map(|t| t as *mut MidiStateTracker as usize)
        .unwrap_or(0)
}

unsafe fn get_maybe_ringbuffer_tail_state_tracker(port: &mut MidiPort) -> usize {
    // SAFETY: Caller ensures MidiPort lifetime covers the returned pointer
    port.base_mut().maybe_ringbuffer_tail_state_tracker()
        .map(|t| t as *mut MidiStateTracker as usize)
        .unwrap_or(0)
}

// State processing for C++ callers - calls Rust directly
unsafe fn process_msg_raw_to_state(port: &mut MidiPort, data: *const u8) {
    if let Some(tracker) = port.base_mut().maybe_midi_state_tracker() {
        tracker.process_msg_raw(data);
    }
}

unsafe fn process_msg_raw_to_tail_state(port: &mut MidiPort, data: *const u8) {
    if let Some(tracker) = port.base_mut().maybe_ringbuffer_tail_state_tracker() {
        tracker.process_msg_raw(data);
    }
}

unsafe fn copy_tail_state_to_tracker_by_ptr(port: &mut MidiPort, target_ptr: usize) -> bool {
    if target_ptr == 0 {
        return false;
    }
    // SAFETY: Caller ensures target_ptr is a valid pointer to a MidiStateTracker
    let target = unsafe { &mut *(target_ptr as *mut MidiStateTracker) };
    if let Some(tail_state) = port.base_mut().maybe_ringbuffer_tail_state_tracker() {
        target.copy_relevant_state(tail_state);
        true
    } else {
        false
    }
}

fn get_state_tracker_n_notes_active(port: &mut MidiPort) -> u32 {
    port.base_mut().maybe_midi_state_tracker()
        .map(|t| t.n_notes_active())
        .unwrap_or(0)
}
