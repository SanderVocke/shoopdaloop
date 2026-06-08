//! CXX bridge declarations for C++ AudioMidiDriver bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriver.h");

        type AudioMidiDriver;
        type AudioMidiDriverBridgeStrong;
        type AudioMidiDriverBridgeWeak;

        fn downgrade(self: &AudioMidiDriverBridgeStrong) -> UniquePtr<AudioMidiDriverBridgeWeak>;
        fn upgrade(self: &AudioMidiDriverBridgeWeak) -> UniquePtr<AudioMidiDriverBridgeStrong>;
        fn clone(self: &AudioMidiDriverBridgeWeak) -> UniquePtr<AudioMidiDriverBridgeWeak>;
        fn valid(self: &AudioMidiDriverBridgeStrong) -> bool;
        fn get_ref(self: &AudioMidiDriverBridgeStrong) -> &AudioMidiDriver;
        unsafe fn get_pin_mut(
            self: Pin<&mut AudioMidiDriverBridgeStrong>,
        ) -> Pin<&mut AudioMidiDriver>;

        fn request_close_decoupled_midi_port(
            self: Pin<&mut AudioMidiDriver>,
            registry_handle: u64,
        );
        #[namespace = "backend_rust"]
        fn audiomididriver_request_close_decoupled_midi_port(driver_ptr: usize, registry_handle: u64);
    }
}
