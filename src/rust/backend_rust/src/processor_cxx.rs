//! CXX bridge declarations for C++ processor bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/HasAudioProcessingFunction.h");

        type HasAudioProcessingFunction;

        type ProcessorBridgeStrong;
        type ProcessorBridgeWeak;

        fn processor_bridge_downgrade(
            strong: &ProcessorBridgeStrong,
        ) -> UniquePtr<ProcessorBridgeWeak>;

        fn processor_bridge_upgrade(weak: &ProcessorBridgeWeak)
            -> UniquePtr<ProcessorBridgeStrong>;

        fn processor_bridge_clone_weak(
            processor: &ProcessorBridgeWeak,
        ) -> UniquePtr<ProcessorBridgeWeak>;
        fn processor_bridge_proc_process(processor: &ProcessorBridgeWeak, nframes: u32);
    }
}
