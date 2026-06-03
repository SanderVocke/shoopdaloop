//! CXX bridge declarations for C++ decoupled MIDI port bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");

        #[namespace = ""]
        type DecoupledMidiPort;

        #[namespace = "bridge_object"]
        type DecoupledMidiPortBridgeStrong;
        #[namespace = "bridge_object"]
        type DecoupledMidiPortBridgeWeak;

        #[namespace = "bridge_object"]
        fn decoupled_midi_port_bridge_downgrade(
            strong: &DecoupledMidiPortBridgeStrong,
        ) -> UniquePtr<DecoupledMidiPortBridgeWeak>;
        #[namespace = "bridge_object"]
        fn decoupled_midi_port_bridge_upgrade(
            weak: &DecoupledMidiPortBridgeWeak,
        ) -> UniquePtr<DecoupledMidiPortBridgeStrong>;
        #[namespace = "bridge_object"]
        fn decoupled_midi_port_bridge_proc_process(
            port: &DecoupledMidiPortBridgeWeak,
            nframes: u32,
        );
        #[namespace = "bridge_object"]
        fn decoupled_midi_port_bridge_close(port: &DecoupledMidiPortBridgeWeak);
    }
}
