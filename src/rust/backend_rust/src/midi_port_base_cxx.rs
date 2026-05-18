//! CXX bridge for MidiPortBase to expose to C++.
//!
//! Note: Most MidiPortBase methods are delegated through MidiPort bridge.
//! This bridge exposes only what's needed for direct C++ interop (tests).

#![allow(dead_code)]

use crate::midi_port_base::{MidiPortBase, TrackingConfig};

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type MidiPortBase;
        type TrackingConfig;

        // Constructors
        fn new_midi_port_base(
            track_notes: bool,
            track_controls: bool,
            track_programs: bool,
        ) -> Box<MidiPortBase>;

        // Mute state
        fn set_muted(port: &MidiPortBase, muted: bool);
        fn get_muted(port: &MidiPortBase) -> bool;

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
    Box::new(MidiPortBase::new_with_flags(
        track_notes,
        track_controls,
        track_programs,
    ))
}

// Mute state
fn set_muted(port: &MidiPortBase, muted: bool) {
    port.set_muted(muted);
}

fn get_muted(port: &MidiPortBase) -> bool {
    port.get_muted()
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
