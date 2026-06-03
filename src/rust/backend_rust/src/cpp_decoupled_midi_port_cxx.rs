//! CXX bridge for the C++ DecoupledMidiPort object itself.
//!
//! This is distinct from `decoupled_midi_port_cxx.rs`, which exposes the
//! Rust-native backend_rust::DecoupledMidiPort queue implementation to C++.
//! Bridge-object wrappers are declared separately in
//! `decoupled_midi_port_bridge_cxx.rs`.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/DecoupledMidiPort.h");

        type DecoupledMidiPort;

        fn PROC_process(self: Pin<&mut DecoupledMidiPort>, nframes: u32);
        fn close(self: Pin<&mut DecoupledMidiPort>);
    }
}
