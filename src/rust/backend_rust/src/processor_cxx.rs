//! CXX bridge declarations for C++ processor bridge-object handles.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/HasAudioProcessingFunction.h");

        type HasAudioProcessingFunction;

        type ProcessorBridgeStrong;
        type ProcessorBridgeWeak;

        fn downgrade(self: &ProcessorBridgeStrong) -> UniquePtr<ProcessorBridgeWeak>;
        fn upgrade(self: &ProcessorBridgeWeak) -> UniquePtr<ProcessorBridgeStrong>;
        fn clone_unique(self: &ProcessorBridgeWeak) -> UniquePtr<ProcessorBridgeWeak>;

        fn processor_bridge_proc_process(processor: &ProcessorBridgeWeak, nframes: u32);
    }
}
