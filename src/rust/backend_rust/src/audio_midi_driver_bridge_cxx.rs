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
    }
}
