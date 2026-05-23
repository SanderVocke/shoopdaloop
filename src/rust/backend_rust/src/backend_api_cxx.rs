//! CXX bridge for the backend API layer.
//!
//! This module provides a Rust implementation layer between libshoopdaloop_backend.cpp
//! and the internal implementation objects. It allows gradual migration of API functions
//! from C++ to Rust.
//!
//! Initially, this layer will handle simple query functions that don't involve
//! object handles. More complex functions will be migrated incrementally.

/// Driver type enum values matching shoop_audio_driver_type_t in types.h
const DRIVER_TYPE_JACK: u32 = 0;
const DRIVER_TYPE_JACK_TEST: u32 = 1;
const DRIVER_TYPE_DUMMY: u32 = 2;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        /// Check if a driver type is supported.
        /// Takes the driver type as a u32 (matching shoop_audio_driver_type_t enum values).
        /// Returns true if the driver type is available in this build.
        fn driver_type_supported(driver_type: u32) -> bool;
    }
}

/// Check if a driver type is supported.
///
/// Matches the shoop_audio_driver_type_t enum values from types.h:
/// - Jack (0): JACK audio driver - supported on Linux (BACKEND_JACK is ON by default)
/// - JackTest (1): JACK test backend - supported on Linux
/// - Dummy (2): Dummy driver - always supported
///
/// Note: JACK support is typically enabled by default on Linux builds.
/// The actual availability depends on the SHOOP_HAVE_BACKEND_JACK compile flag
/// set by CMake. This Rust approximation assumes Linux builds have JACK available.
/// For precise detection, feature flag propagation from CMake would be needed.
fn driver_type_supported(driver_type: u32) -> bool {
    match driver_type {
        DRIVER_TYPE_DUMMY => true,
        DRIVER_TYPE_JACK | DRIVER_TYPE_JACK_TEST => {
            // JACK is typically available on Linux builds (BACKEND_JACK=ON by default)
            // For now, assume Linux builds have JACK support.
            // TODO: Add proper feature flag propagation from CMake build.
            cfg!(target_os = "linux")
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dummy_always_supported() {
        assert!(driver_type_supported(DRIVER_TYPE_DUMMY));
    }

    #[test]
    fn test_invalid_type_not_supported() {
        assert!(!driver_type_supported(999));
        assert!(!driver_type_supported(3));
    }
}
