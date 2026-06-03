//! CXX bridge declarations for C++ decoupled MIDI port bridge-object handles.
//!
//! The bridge handles only expose generic lifetime/access operations. Methods
//! of the contained C++ `DecoupledMidiPort` object are exposed by
//! `cpp_decoupled_midi_port_cxx.rs`.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");

        type DecoupledMidiPort = crate::cpp_decoupled_midi_port_cxx::ffi::DecoupledMidiPort;

        type DecoupledMidiPortBridgeStrong;
        type DecoupledMidiPortBridgeWeak;

        fn downgrade(
            self: &DecoupledMidiPortBridgeStrong,
        ) -> UniquePtr<DecoupledMidiPortBridgeWeak>;
        fn upgrade(self: &DecoupledMidiPortBridgeWeak) -> UniquePtr<DecoupledMidiPortBridgeStrong>;
        fn clone(self: &DecoupledMidiPortBridgeWeak) -> UniquePtr<DecoupledMidiPortBridgeWeak>;

        fn get_ref(self: &DecoupledMidiPortBridgeStrong) -> &DecoupledMidiPort;
        unsafe fn get_pin_mut(
            self: Pin<&mut DecoupledMidiPortBridgeStrong>,
        ) -> Pin<&mut DecoupledMidiPort>;
    }
}
