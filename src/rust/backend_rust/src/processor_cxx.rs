//! CXX bridge declarations for C++ processor bridge-object handles.
//!
//! The bridge handles only expose generic lifetime/access operations. Methods
//! of the contained C++ `IProcessor` object are exposed by `i_processor_cxx.rs`.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/IProcessor.h");

        type IProcessor = crate::i_processor_cxx::ffi::IProcessor;

        type ProcessorBridgeStrong;
        type ProcessorBridgeWeak;

        fn downgrade(self: &ProcessorBridgeStrong) -> UniquePtr<ProcessorBridgeWeak>;
        fn upgrade(self: &ProcessorBridgeWeak) -> UniquePtr<ProcessorBridgeStrong>;
        fn clone(self: &ProcessorBridgeWeak) -> UniquePtr<ProcessorBridgeWeak>;

        fn get_ref(self: &ProcessorBridgeStrong) -> &IProcessor;
        unsafe fn get_pin_mut(self: Pin<&mut ProcessorBridgeStrong>) -> Pin<&mut IProcessor>;
    }
}
