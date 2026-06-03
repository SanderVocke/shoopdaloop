//! CXX bridge declarations for C++ decoupled MIDI port bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");
        type DecoupledMidiPort;

        type DecoupledMidiPortBridgeStrong;
        type DecoupledMidiPortBridgeWeak;

        fn downgrade(
            self: &DecoupledMidiPortBridgeStrong,
        ) -> UniquePtr<DecoupledMidiPortBridgeWeak>;
        fn upgrade(self: &DecoupledMidiPortBridgeWeak) -> UniquePtr<DecoupledMidiPortBridgeStrong>;
        fn clone(
            self: &DecoupledMidiPortBridgeWeak,
        ) -> UniquePtr<DecoupledMidiPortBridgeWeak>;

        fn decoupled_midi_port_bridge_proc_process(
            port: &DecoupledMidiPortBridgeWeak,
            nframes: u32,
        );
        fn decoupled_midi_port_bridge_close(port: &DecoupledMidiPortBridgeWeak);
    }
}
