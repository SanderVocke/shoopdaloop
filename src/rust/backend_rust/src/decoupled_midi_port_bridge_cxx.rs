//! CXX bridge declarations for C++ decoupled MIDI port bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");
        type DecoupledMidiPort;

        type DecoupledMidiPortBridgeStrong;
        type DecoupledMidiPortBridgeWeak;

        fn decoupled_midi_port_bridge_downgrade(
            strong: &DecoupledMidiPortBridgeStrong,
        ) -> UniquePtr<DecoupledMidiPortBridgeWeak>;
        fn decoupled_midi_port_bridge_upgrade(
            weak: &DecoupledMidiPortBridgeWeak,
        ) -> UniquePtr<DecoupledMidiPortBridgeStrong>;

        fn decoupled_midi_port_bridge_proc_process(
            port: &DecoupledMidiPortBridgeWeak,
            nframes: u32,
        );
        fn decoupled_midi_port_bridge_close(port: &DecoupledMidiPortBridgeWeak);
    }
}
