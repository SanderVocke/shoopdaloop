//! CXX bridge for MidiPortBase to expose to C++.

#![allow(dead_code)]

use crate::midi_port_base::{MidiPortBase, TrackingConfig};

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiPortBase;
        type TrackingConfig;

        // Constructors
        fn new_midi_port_base(track_notes: bool, track_controls: bool, track_programs: bool) -> Box<MidiPortBase>;

        // IMidiStateTracking interface
        fn n_notes_active(port: &MidiPortBase) -> u32;
        fn get_input_event_count(port: &MidiPortBase) -> u32;
        fn get_output_event_count(port: &MidiPortBase) -> u32;

        // IMidiRingbuffer interface
        fn set_n_samples(port: &mut MidiPortBase, n: u32);
        fn get_n_samples(port: &MidiPortBase) -> u32;
        fn get_current_n_samples(port: &MidiPortBase) -> u32;

        // Event counter management
        fn reset_n_input_events(port: &MidiPortBase);
        fn increment_input_events(port: &MidiPortBase, count: u32);

        fn reset_n_output_events(port: &MidiPortBase);
        fn increment_output_events(port: &MidiPortBase, count: u32);

        // Mute state
        fn set_muted(port: &MidiPortBase, muted: bool);
        fn get_muted(port: &MidiPortBase) -> bool;

        // State processing
        unsafe fn process_msg_to_state(port: &mut MidiPortBase, data: *const u8, size: usize);

        // Tracking config accessors
        fn tracking_config_track_notes(config: &TrackingConfig) -> bool;
        fn tracking_config_track_controls(config: &TrackingConfig) -> bool;
        fn tracking_config_track_programs(config: &TrackingConfig) -> bool;
    }
}

// Constructors
fn new_midi_port_base(
    track_notes: bool,
    track_controls: bool,
    track_programs: bool,
) -> Box<MidiPortBase> {
    Box::new(MidiPortBase::new_with_flags(track_notes, track_controls, track_programs))
}

// IMidiStateTracking interface
fn n_notes_active(port: &MidiPortBase) -> u32 {
    port.n_notes_active_direct()
}

fn get_input_event_count(port: &MidiPortBase) -> u32 {
    port.get_n_input_events()
}

fn get_output_event_count(port: &MidiPortBase) -> u32 {
    port.get_n_output_events()
}

// IMidiRingbuffer interface
fn set_n_samples(port: &mut MidiPortBase, n: u32) {
    port.set_n_samples_direct(n);
}

fn get_n_samples(port: &MidiPortBase) -> u32 {
    port.get_n_samples_direct()
}

fn get_current_n_samples(port: &MidiPortBase) -> u32 {
    port.get_current_n_samples_direct()
}

// Event counter management
fn reset_n_input_events(port: &MidiPortBase) {
    port.reset_n_input_events();
}

fn increment_input_events(port: &MidiPortBase, count: u32) {
    port.increment_input_events(count);
}

fn reset_n_output_events(port: &MidiPortBase) {
    port.reset_n_output_events();
}

fn increment_output_events(port: &MidiPortBase, count: u32) {
    port.increment_output_events(count);
}

// Mute state
fn set_muted(port: &MidiPortBase, muted: bool) {
    port.set_muted(muted);
}

fn get_muted(port: &MidiPortBase) -> bool {
    port.get_muted()
}

// State processing
unsafe fn process_msg_to_state(port: &mut MidiPortBase, data: *const u8, size: usize) {
    port.process_msg_to_state_raw(data, size);
}

// Tracking config accessors
fn tracking_config_track_notes(config: &TrackingConfig) -> bool {
    config.track_notes
}

fn tracking_config_track_controls(config: &TrackingConfig) -> bool {
    config.track_controls
}

fn tracking_config_track_programs(config: &TrackingConfig) -> bool {
    config.track_programs
}