//! CXX bridge for the C++ IProcessor interface itself.
//!
//! Bridge-object wrappers are declared separately in `processor_cxx.rs`; this
//! module exposes the methods of the contained C++ object once a bridge strong
//! handle has yielded a reference to it.

#![allow(dead_code)]

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/IProcessor.h");

        type IProcessor;

        fn PROC_process(self: Pin<&mut IProcessor>, nframes: u32);
    }
}
