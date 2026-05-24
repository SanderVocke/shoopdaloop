//! CXX bridge for the backend API layer.
//!
//! This module provides a Rust implementation layer between libshoopdaloop_backend.cpp
//! and the internal implementation objects. It allows gradual migration of API functions
//! from C++ to Rust.
//!
//! Initially, this layer will handle simple query functions that don't involve
//! object handles. More complex functions will be migrated incrementally.

use std::ffi::c_char;
use std::mem::size_of;

/// Driver type enum values matching shoop_audio_driver_type_t in types.h
const DRIVER_TYPE_JACK: u32 = 0;
const DRIVER_TYPE_JACK_TEST: u32 = 1;
const DRIVER_TYPE_DUMMY: u32 = 2;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    // C structs that we manipulate from Rust
    // These match the definitions in types.h
    // Note: cxx automatically handles C-compatible layout
    struct ShoopMidiEvent {
        time: i32,
        size: u32,
        data: *mut u8,
    }

    struct ShoopAudioChannelData {
        n_samples: u32,
        data: *mut f32,
    }

    struct ShoopMultichannelAudio {
        n_channels: u32,
        n_frames: u32,
        data: *mut f32,
    }

    struct ShoopAudioChannelStateInfo {
        mode: u32, // shoop_channel_mode_t as u32
        gain: f32,
        output_peak: f32,
        length: u32,
        start_offset: i32,
        played_back_sample: i32, // -1 is none
        n_preplay_samples: u32,
        data_dirty: u32,
    }

    struct ShoopMidiChannelStateInfo {
        mode: u32, // shoop_channel_mode_t as u32
        n_events_triggered: u32,
        n_notes_active: u32,
        length: u32,
        start_offset: i32,
        played_back_sample: i32, // -1 is none
        n_preplay_samples: u32,
        data_dirty: u32,
    }

    struct ShoopLoopStateInfo {
        mode: u32, // shoop_loop_mode_t as u32
        length: u32,
        position: u32,
        maybe_next_mode: u32, // shoop_loop_mode_t as u32
        maybe_next_mode_delay: u32,
    }

    struct ShoopFxChainStateInfo {
        ready: u32,
        active: u32,
        visible: u32,
    }

    struct ShoopBackendSessionStateInfo {
        audio_driver: *mut u8, // opaque pointer (using u8 as void-like)
        n_audio_buffers_created: u32,
        n_audio_buffers_available: u32,
    }

    struct ShoopMidiSequence {
        n_events: u32,
        events: *mut *mut ShoopMidiEvent,
        length_samples: u32,
    }

    struct ShoopProfilingReportItem {
        key: *mut c_char,
        n_samples: f32,
        average: f32,
        worst: f32,
        most_recent: f32,
    }

    struct ShoopProfilingReport {
        n_items: u32,
        items: *mut ShoopProfilingReportItem,
    }

    struct ShoopExternalPortDescriptor {
        data_type: u32, // shoop_port_data_type_t as u32
        direction: u32, // shoop_port_direction_t as u32
        name: *mut c_char,
    }

    struct ShoopExternalPortDescriptors {
        n_ports: u32,
        ports: *mut ShoopExternalPortDescriptor,
    }

    struct ShoopAudioDriverState {
        dsp_load_percent: f32,
        xruns_since_last: u32,
        maybe_driver_handle: *mut u8, // void* as *mut u8
        maybe_instance_name: *mut c_char,
        sample_rate: u32,
        buffer_size: u32,
        active: u32,
        last_processed: u32,
    }

    struct ShoopMidiPortStateInfo {
        n_input_events: u32,
        n_input_notes_active: u32,
        n_output_events: u32,
        n_output_notes_active: u32,
        muted: u32,
        passthrough_muted: u32,
        ringbuffer_n_samples: u32,
        name: *mut c_char,
    }

    struct ShoopAudioPortStateInfo {
        input_peak: f32,
        output_peak: f32,
        gain: f32,
        muted: u32,
        passthrough_muted: u32,
        ringbuffer_n_samples: u32,
        name: *mut c_char,
    }

    extern "Rust" {
        /// Check if a driver type is supported.
        fn driver_type_supported(driver_type: u32) -> bool;

        // Allocation functions
        fn alloc_midi_event(data_bytes: u32) -> *mut ShoopMidiEvent;
        fn alloc_audio_channel_data(n_samples: u32) -> *mut ShoopAudioChannelData;
        fn alloc_multichannel_audio(n_channels: u32, n_frames: u32) -> *mut ShoopMultichannelAudio;

        // Destroy functions (unsafe due to pointer arguments)
        unsafe fn destroy_midi_event(e: *mut ShoopMidiEvent);
        unsafe fn destroy_audio_channel_data(d: *mut ShoopAudioChannelData);
        unsafe fn destroy_multichannel_audio(audio: *mut ShoopMultichannelAudio);
        unsafe fn destroy_string(s: *mut c_char);
        unsafe fn destroy_audio_channel_state_info(d: *mut ShoopAudioChannelStateInfo);
        unsafe fn destroy_midi_channel_state_info(d: *mut ShoopMidiChannelStateInfo);
        unsafe fn destroy_loop_state_info(state: *mut ShoopLoopStateInfo);
        unsafe fn destroy_fx_chain_state(d: *mut ShoopFxChainStateInfo);
        unsafe fn destroy_backend_state_info(d: *mut ShoopBackendSessionStateInfo);

        // New functions to migrate
        unsafe fn destroy_midi_sequence(d: *mut ShoopMidiSequence);
        unsafe fn destroy_profiling_report(d: *mut ShoopProfilingReport);
        unsafe fn destroy_external_port_descriptors(d: *mut ShoopExternalPortDescriptors);
        unsafe fn destroy_audio_driver_state(d: *mut ShoopAudioDriverState);
        unsafe fn destroy_midi_port_state_info(d: *mut ShoopMidiPortStateInfo);
        unsafe fn destroy_audio_port_state_info(d: *mut ShoopAudioPortStateInfo);
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

// =============================================================================
// Allocation Functions
// =============================================================================

/// Allocate a MIDI event struct with a data buffer.
///
/// Matches the C implementation in libshoopdaloop_backend.cpp.
/// The returned pointer should be freed with destroy_midi_event.
fn alloc_midi_event(data_bytes: u32) -> *mut ffi::ShoopMidiEvent {
    unsafe {
        let data = libc::malloc(data_bytes as usize) as *mut u8;
        let event = Box::new(ffi::ShoopMidiEvent {
            size: data_bytes,
            time: 0,
            data,
        });
        Box::into_raw(event)
    }
}

/// Allocate an audio channel data struct with a sample buffer.
///
/// Matches the C implementation in libshoopdaloop_backend.cpp.
/// The returned pointer should be freed with destroy_audio_channel_data.
fn alloc_audio_channel_data(n_samples: u32) -> *mut ffi::ShoopAudioChannelData {
    unsafe {
        let data = libc::malloc((n_samples as usize) * size_of::<f32>()) as *mut f32;
        let channel_data = Box::new(ffi::ShoopAudioChannelData { n_samples, data });
        Box::into_raw(channel_data)
    }
}

/// Allocate a multichannel audio struct.
///
/// Matches the C implementation in libshoopdaloop_backend.cpp.
/// Channels are not interleaved.
/// The returned pointer should be freed with destroy_multichannel_audio.
fn alloc_multichannel_audio(n_channels: u32, n_frames: u32) -> *mut ffi::ShoopMultichannelAudio {
    unsafe {
        let data = libc::malloc((n_channels as usize) * (n_frames as usize) * size_of::<f32>())
            as *mut f32;
        let audio = Box::new(ffi::ShoopMultichannelAudio {
            n_channels,
            n_frames,
            data,
        });
        Box::into_raw(audio)
    }
}

// =============================================================================
// Destroy Functions
// =============================================================================

/// Free a MIDI event struct and its data buffer.
///
/// Safe to call with null pointer.
unsafe fn destroy_midi_event(e: *mut ffi::ShoopMidiEvent) {
    if e.is_null() {
        return;
    }
    let event = Box::from_raw(e);
    libc::free(event.data as *mut libc::c_void);
}

/// Free an audio channel data struct and its sample buffer.
///
/// Safe to call with null pointer.
unsafe fn destroy_audio_channel_data(d: *mut ffi::ShoopAudioChannelData) {
    if d.is_null() {
        return;
    }
    let channel_data = Box::from_raw(d);
    libc::free(channel_data.data as *mut libc::c_void);
}

/// Free a multichannel audio struct and its data buffer.
///
/// Safe to call with null pointer.
unsafe fn destroy_multichannel_audio(audio: *mut ffi::ShoopMultichannelAudio) {
    if audio.is_null() {
        return;
    }
    let mc_audio = Box::from_raw(audio);
    libc::free(mc_audio.data as *mut libc::c_void);
}

/// Free a strdup'd string.
///
/// Safe to call with null pointer.
unsafe fn destroy_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    libc::free(s as *mut libc::c_void);
}

/// Free an audio channel state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_audio_channel_state_info(d: *mut ffi::ShoopAudioChannelStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

/// Free a MIDI channel state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_midi_channel_state_info(d: *mut ffi::ShoopMidiChannelStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

/// Free a loop state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_loop_state_info(state: *mut ffi::ShoopLoopStateInfo) {
    if state.is_null() {
        return;
    }
    let _ = Box::from_raw(state);
}

/// Free an FX chain state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_fx_chain_state(d: *mut ffi::ShoopFxChainStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

/// Free a backend session state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_backend_state_info(d: *mut ffi::ShoopBackendSessionStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

// =============================================================================
// New Functions Migrated from C++
// =============================================================================

// Note: alloc_midi_sequence is kept in C++ for now due to cxx struct layout complexities.
// The destroy_midi_sequence function is migrated since it operates on C++-allocated pointers.

/// Free a MIDI sequence struct and all its events.
///
/// Safe to call with null pointer.
/// Each event is freed using destroy_midi_event.
/// Uses ptr::read_unaligned to handle potential alignment issues with cxx structs.
unsafe fn destroy_midi_sequence(d: *mut ffi::ShoopMidiSequence) {
    if d.is_null() {
        return;
    }
    // Read the struct using ptr::read_unaligned to handle potential alignment issues
    let sequence = std::ptr::read_unaligned(d);
    // Free each event
    for i in 0..sequence.n_events {
        let event_ptr = std::ptr::read_unaligned(sequence.events.add(i as usize));
        destroy_midi_event(event_ptr);
    }
    // Free the events array (was allocated with malloc in C++)
    libc::free(sequence.events as *mut libc::c_void);
    // Free the struct itself (was allocated with new in C++)
    libc::free(d as *mut libc::c_void);
}

/// Free a profiling report struct.
///
/// Safe to call with null pointer.
/// Frees all item keys, the items array, and the struct itself.
unsafe fn destroy_profiling_report(d: *mut ffi::ShoopProfilingReport) {
    if d.is_null() {
        return;
    }
    // Read the struct using ptr::read_unaligned to handle potential alignment issues
    let report = std::ptr::read_unaligned(d);
    // Free each item's key string
    for i in 0..report.n_items {
        let item = std::ptr::read_unaligned(report.items.add(i as usize));
        libc::free(item.key as *mut libc::c_void);
    }
    // Free the items array
    libc::free(report.items as *mut libc::c_void);
    // Free the struct itself (was allocated with malloc/new in C++)
    libc::free(d as *mut libc::c_void);
}

/// Free external port descriptors struct.
///
/// Safe to call with null pointer.
/// Frees all port names, the ports array, and the struct itself.
unsafe fn destroy_external_port_descriptors(d: *mut ffi::ShoopExternalPortDescriptors) {
    if d.is_null() {
        return;
    }
    // Read the struct using ptr::read_unaligned to handle potential alignment issues
    let descriptors = std::ptr::read_unaligned(d);
    // Free each port's name string
    for i in 0..descriptors.n_ports {
        let port = std::ptr::read_unaligned(descriptors.ports.add(i as usize));
        libc::free(port.name as *mut libc::c_void);
    }
    // Free the ports array
    libc::free(descriptors.ports as *mut libc::c_void);
    // Free the struct itself (was allocated with malloc/new in C++)
    libc::free(d as *mut libc::c_void);
}

/// Free an audio driver state struct.
///
/// Safe to call with null pointer.
/// Frees the instance name string if present, and the struct itself.
unsafe fn destroy_audio_driver_state(d: *mut ffi::ShoopAudioDriverState) {
    if d.is_null() {
        return;
    }
    // Read the struct using ptr::read_unaligned to handle potential alignment issues
    let state = std::ptr::read_unaligned(d);
    // Free the instance name if present
    if !state.maybe_instance_name.is_null() {
        libc::free(state.maybe_instance_name as *mut libc::c_void);
    }
    // Free the struct itself (was allocated with new in C++)
    libc::free(d as *mut libc::c_void);
}

/// Free a MIDI port state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_midi_port_state_info(d: *mut ffi::ShoopMidiPortStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
}

/// Free an audio port state info struct.
///
/// Safe to call with null pointer.
unsafe fn destroy_audio_port_state_info(d: *mut ffi::ShoopAudioPortStateInfo) {
    if d.is_null() {
        return;
    }
    let _ = Box::from_raw(d);
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

    // Allocation/Destroy tests

    #[test]
    fn test_alloc_destroy_midi_event() {
        let event = alloc_midi_event(10);
        assert!(!event.is_null());
        unsafe {
            assert_eq!((*event).size, 10);
            assert_eq!((*event).time, 0);
            assert!(!(*event).data.is_null());
            destroy_midi_event(event);
        }
    }

    #[test]
    fn test_alloc_destroy_midi_event_zero_size() {
        let event = alloc_midi_event(0);
        assert!(!event.is_null());
        unsafe {
            assert_eq!((*event).size, 0);
            destroy_midi_event(event);
        }
    }

    #[test]
    fn test_destroy_midi_event_null() {
        unsafe { destroy_midi_event(std::ptr::null_mut()) };
    }

    #[test]
    fn test_alloc_destroy_audio_channel_data() {
        let data = alloc_audio_channel_data(100);
        assert!(!data.is_null());
        unsafe {
            assert_eq!((*data).n_samples, 100);
            assert!(!(*data).data.is_null());
        }
        unsafe { destroy_audio_channel_data(data) };
    }

    #[test]
    fn test_destroy_audio_channel_data_null() {
        unsafe { destroy_audio_channel_data(std::ptr::null_mut()) };
    }

    #[test]
    fn test_alloc_destroy_multichannel_audio() {
        let audio = alloc_multichannel_audio(2, 1024);
        assert!(!audio.is_null());
        unsafe {
            assert_eq!((*audio).n_channels, 2);
            assert_eq!((*audio).n_frames, 1024);
            assert!(!(*audio).data.is_null());
        }
        unsafe { destroy_multichannel_audio(audio) };
    }

    #[test]
    fn test_destroy_multichannel_audio_null() {
        unsafe { destroy_multichannel_audio(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_string_null() {
        unsafe { destroy_string(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_audio_channel_state_info_null() {
        unsafe { destroy_audio_channel_state_info(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_midi_channel_state_info_null() {
        unsafe { destroy_midi_channel_state_info(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_loop_state_info_null() {
        unsafe { destroy_loop_state_info(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_fx_chain_state_null() {
        unsafe { destroy_fx_chain_state(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_backend_state_info_null() {
        unsafe { destroy_backend_state_info(std::ptr::null_mut()) };
    }

    // Tests for new migrated functions

    #[test]
    fn test_destroy_midi_sequence_null() {
        unsafe { destroy_midi_sequence(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_profiling_report_null() {
        unsafe { destroy_profiling_report(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_external_port_descriptors_null() {
        unsafe { destroy_external_port_descriptors(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_audio_driver_state_null() {
        unsafe { destroy_audio_driver_state(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_midi_port_state_info_null() {
        unsafe { destroy_midi_port_state_info(std::ptr::null_mut()) };
    }

    #[test]
    fn test_destroy_audio_port_state_info_null() {
        unsafe { destroy_audio_port_state_info(std::ptr::null_mut()) };
    }
}
