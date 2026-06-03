//! CXX bridge declarations for C++ processor bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");

        #[namespace = ""]
        type HasAudioProcessingFunction;

        #[namespace = "bridge_object"]
        type ProcessorBridgeStrong;
        #[namespace = "bridge_object"]
        type ProcessorBridgeWeak;

        #[namespace = "bridge_object"]
        fn processor_bridge_downgrade(
            strong: &ProcessorBridgeStrong,
        ) -> UniquePtr<ProcessorBridgeWeak>;
        #[namespace = "bridge_object"]
        fn processor_bridge_upgrade(weak: &ProcessorBridgeWeak)
            -> UniquePtr<ProcessorBridgeStrong>;
        #[namespace = "bridge_object"]
        fn processor_bridge_clone_weak(
            processor: &ProcessorBridgeWeak,
        ) -> UniquePtr<ProcessorBridgeWeak>;
        #[namespace = "bridge_object"]
        fn processor_bridge_proc_process(processor: &ProcessorBridgeWeak, nframes: u32);
    }
}
