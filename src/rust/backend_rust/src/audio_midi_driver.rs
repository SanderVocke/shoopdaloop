//! AudioMidiDriver - Core driver state management in Rust.
//!
//! Provides atomic state management for audio/MIDI drivers:
//! - Atomic state (xruns, sample_rate, buffer_size, dsp_load, active, last_processed, client_name, client_handle)
//! - Processor registration (stored as usize pointers, callbacks used for iteration)
//! - Decoupled MIDI port registration (stored as usize pointers)
//!
//! The actual audio processing and command queue handling happens in C++.
//! This struct only manages atomic state and processor/port registration.
//!
//! Ported from C++ AudioMidiDriver.h/cpp

use crate::command_queue::CommandQueue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};

/// Wrapper for atomic f32 operations using AtomicU32 internally.
#[derive(Debug)]
pub struct AtomicF32 {
    inner: AtomicU32,
}

impl AtomicF32 {
    pub fn new(val: f32) -> Self {
        AtomicF32 {
            inner: AtomicU32::new(val.to_bits()),
        }
    }

    pub fn load(&self, order: Ordering) -> f32 {
        f32::from_bits(self.inner.load(order))
    }

    pub fn store(&self, val: f32, order: Ordering) {
        self.inner.store(val.to_bits(), order);
    }
}

impl Default for AtomicF32 {
    fn default() -> Self {
        Self::new(0.0)
    }
}

/// Atomic state for AudioMidiDriver.
/// All fields are accessible from multiple threads (audio thread, control thread).
pub struct AudioMidiDriverState {
    xruns: AtomicU32,
    sample_rate: AtomicU32,
    buffer_size: AtomicU32,
    dsp_load: AtomicF32,
    active: AtomicBool,
    last_processed: AtomicU32,
    client_handle: AtomicPtr<()>,
    client_name: Mutex<String>,
}

impl AudioMidiDriverState {
    pub fn new() -> Self {
        AudioMidiDriverState {
            xruns: AtomicU32::new(0),
            sample_rate: AtomicU32::new(0),
            buffer_size: AtomicU32::new(0),
            dsp_load: AtomicF32::new(0.0),
            active: AtomicBool::new(false),
            last_processed: AtomicU32::new(1), // C++ initializes to 1
            client_handle: AtomicPtr::new(std::ptr::null_mut()),
            client_name: Mutex::new("unknown".to_string()),
        }
    }
}

impl Default for AudioMidiDriverState {
    fn default() -> Self {
        Self::new()
    }
}

/// Core audio/MIDI driver implementation.
///
/// Holds atomic state and registration records.
macro_rules! typed_bridge_registration {
    ($name:ident, $weak:ty, $strong:ty) => {
        struct $name {
            weak: cxx::UniquePtr<$weak>,
            _strong: cxx::UniquePtr<$strong>,
        }
    };
    ($name:ident, $weak:ty, $strong:ty, { $($extra_name:ident : $extra_ty:ty),+ $(,)? }) => {
        struct $name {
            $($extra_name: $extra_ty,)+
            weak: cxx::UniquePtr<$weak>,
            _strong: cxx::UniquePtr<$strong>,
        }
    };
}

typed_bridge_registration!(
    ProcessorRegistration,
    crate::processor_cxx::ffi::ProcessorBridgeWeak,
    crate::processor_cxx::ffi::ProcessorBridgeStrong,
    { cpp_identity: usize }
);

pub struct AudioMidiDriverCore {
    state: AudioMidiDriverState,
    command_queue: CommandQueue,
    processors: RwLock<HashMap<u64, ProcessorRegistration>>,
    processor_handle_by_cpp_identity: Mutex<HashMap<usize, u64>>,
    next_processor_handle: AtomicU64,
    decoupled_ports: RwLock<Vec<Box<crate::decoupled_midi_port_cxx::DecoupledMidiPortBridgeWeak>>>,
}

impl AudioMidiDriverCore {
    /// Create a new AudioMidiDriverCore.
    pub fn new() -> Self {
        AudioMidiDriverCore {
            state: AudioMidiDriverState::new(),
            command_queue: CommandQueue::new(1024, 1000, 1000),
            processors: RwLock::new(HashMap::new()),
            processor_handle_by_cpp_identity: Mutex::new(HashMap::new()),
            next_processor_handle: AtomicU64::new(1),
            decoupled_ports: RwLock::new(Vec::new()),
        }
    }

    // ========================================================================
    // State getters
    // ========================================================================

    pub fn get_xruns(&self) -> u32 {
        self.state.xruns.load(Ordering::SeqCst)
    }

    pub fn get_sample_rate(&self) -> u32 {
        self.state.sample_rate.load(Ordering::SeqCst)
    }

    pub fn get_buffer_size(&self) -> u32 {
        self.state.buffer_size.load(Ordering::SeqCst)
    }

    pub fn get_dsp_load(&self) -> f32 {
        self.state.dsp_load.load(Ordering::SeqCst)
    }

    pub fn get_active(&self) -> bool {
        self.state.active.load(Ordering::SeqCst)
    }

    pub fn get_last_processed(&self) -> u32 {
        self.state.last_processed.load(Ordering::SeqCst)
    }

    pub fn get_client_name(&self) -> String {
        self.state
            .client_name
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_else(|_| "unknown".to_string())
    }

    pub fn get_client_handle(&self) -> usize {
        self.state.client_handle.load(Ordering::SeqCst) as usize
    }

    // ========================================================================
    // State setters
    // ========================================================================

    pub fn set_xruns(&self, val: u32) {
        self.state.xruns.store(val, Ordering::SeqCst);
    }

    pub fn set_sample_rate(&self, val: u32) {
        self.state.sample_rate.store(val, Ordering::SeqCst);
    }

    pub fn set_buffer_size(&self, val: u32) {
        self.state.buffer_size.store(val, Ordering::SeqCst);
    }

    pub fn set_dsp_load(&self, val: f32) {
        self.state.dsp_load.store(val, Ordering::SeqCst);
    }

    pub fn set_active(&self, val: bool) {
        self.state.active.store(val, Ordering::SeqCst);
    }

    pub fn set_last_processed(&self, val: u32) {
        self.state.last_processed.store(val, Ordering::SeqCst);
    }

    pub fn set_client_name(&self, name: &str) {
        if let Ok(mut guard) = self.state.client_name.lock() {
            *guard = name.to_string();
        }
    }

    pub fn set_client_handle(&self, handle: usize) {
        self.state
            .client_handle
            .store(handle as *mut (), Ordering::SeqCst);
    }

    // ========================================================================
    // Xrun management
    // ========================================================================

    pub fn report_xrun(&self) {
        self.state.xruns.fetch_add(1, Ordering::SeqCst);
    }

    pub fn reset_xruns(&self) {
        self.state.xruns.store(0, Ordering::SeqCst);
    }

    // ========================================================================
    // Processor management
    // ========================================================================

    pub fn add_processor(
        &self,
        cpp_identity: usize,
        weak: cxx::UniquePtr<crate::processor_cxx::ffi::ProcessorBridgeWeak>,
        strong: cxx::UniquePtr<crate::processor_cxx::ffi::ProcessorBridgeStrong>,
    ) -> u64 {
        let handle = self.next_processor_handle.fetch_add(1, Ordering::SeqCst);
        let reg = ProcessorRegistration {
            cpp_identity,
            weak,
            _strong: strong,
        };
        if let Ok(mut guard) = self.processors.write() {
            guard.insert(handle, reg);
        }
        if let Ok(mut map) = self.processor_handle_by_cpp_identity.lock() {
            map.insert(cpp_identity, handle);
        }
        handle
    }

    pub fn remove_processor_by_cpp_identity(&self, cpp_identity: usize) {
        let handle = self
            .processor_handle_by_cpp_identity
            .lock()
            .ok()
            .and_then(|mut map| map.remove(&cpp_identity));
        if let Some(handle) = handle {
            self.remove_processor(handle);
        }
    }

    pub fn remove_processor(&self, handle: u64) {
        let reg = self
            .processors
            .write()
            .ok()
            .and_then(|mut guard| guard.remove(&handle));
        if let Some(reg) = reg {
            if let Ok(mut map) = self.processor_handle_by_cpp_identity.lock() {
                map.remove(&reg.cpp_identity);
            }
            drop(reg._strong);
        }
    }

    pub fn get_processor_handles(&self) -> Vec<u64> {
        self.processors
            .read()
            .map(|guard| guard.keys().copied().collect())
            .unwrap_or_default()
    }

    pub fn get_processor_bridge_weak_handle(
        &self,
        handle: u64,
    ) -> cxx::UniquePtr<crate::processor_cxx::ffi::ProcessorBridgeWeak> {
        self.processors
            .read()
            .ok()
            .and_then(|guard| guard.get(&handle).map(|reg| reg.weak.clone()))
            .unwrap_or_else(cxx::UniquePtr::null)
    }

    // ========================================================================
    // Decoupled port management
    // ========================================================================

    /// Add a decoupled MIDI bridge weak object to the process list.
    pub fn add_decoupled_port(
        &self,
        weak: Box<crate::decoupled_midi_port_cxx::DecoupledMidiPortBridgeWeak>,
    ) {
        if let Ok(mut guard) = self.decoupled_ports.write() {
            guard.push(weak);
        }
    }

    /// Process all live, non-closed decoupled ports and prune expired/closed weak objects.
    pub fn process_decoupled_ports(&self, nframes: u32) {
        if let Ok(mut guard) = self.decoupled_ports.write() {
            guard.retain(|weak| {
                let strong = weak.upgrade();
                if !strong.valid() || crate::decoupled_midi_port_cxx::decoupled_is_closed(&strong) {
                    return false;
                }
                crate::decoupled_midi_port_cxx::decoupled_process(&strong, nframes);
                !crate::decoupled_midi_port_cxx::decoupled_is_closed(&strong)
            });
        }
    }

    pub fn decoupled_port_count(&self) -> usize {
        self.decoupled_ports
            .read()
            .map(|guard| guard.len())
            .unwrap_or_default()
    }

    pub fn process_cycle(&self, maybe_process_callback_ptr: usize, nframes: u32) {
        unsafe {
            crate::audio_midi_driver_cxx::ffi::audiomididriver_invoke_maybe_process_callback(
                maybe_process_callback_ptr,
            );
        }
        self.command_queue.exec_all();

        self.process_decoupled_ports(nframes);

        for handle in self.get_processor_handles() {
            if let Ok(guard) = self.processors.read() {
                if let Some(reg) = guard.get(&handle) {
                    if !reg.weak.is_null() {
                        let mut strong = reg.weak.upgrade();
                        if let Some(strong) = strong.as_mut() {
                            // SAFETY: The bridge object only proves that the C++ object is alive.
                            // The audio driver registry/process-thread discipline is responsible
                            // for avoiding aliased mutable use of the contained C++ object.
                            let processor = unsafe { strong.get_pin_mut() };
                            processor.PROC_process(nframes);
                        }
                    }
                }
            }
        }

        self.set_last_processed(nframes);
    }

    pub fn queue_process_thread_command(&self, user_data: usize) {
        unsafe {
            self.command_queue.cxx_queue(user_data);
        }
    }

    pub fn exec_process_thread_command(&self, user_data: usize) {
        unsafe {
            self.command_queue
                .cxx_exec_process_thread_command(user_data);
        }
    }

    pub fn exec_all_commands_for_process_thread(&self) {
        self.command_queue.exec_all();
    }

    pub fn command_queue_ptr(&self) -> usize {
        (&self.command_queue as *const CommandQueue) as usize
    }
}

// =============================================================================
// Unit Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_defaults() {
        let state = AudioMidiDriverState::new();
        assert_eq!(state.xruns.load(Ordering::SeqCst), 0);
        assert_eq!(state.sample_rate.load(Ordering::SeqCst), 0);
        assert_eq!(state.buffer_size.load(Ordering::SeqCst), 0);
        assert!((state.dsp_load.load(Ordering::SeqCst) - 0.0).abs() < 0.0001);
        assert!(!state.active.load(Ordering::SeqCst));
        assert_eq!(state.last_processed.load(Ordering::SeqCst), 1);
        assert!(state.client_handle.load(Ordering::SeqCst).is_null());
    }

    #[test]
    fn test_atomic_f32() {
        let af = AtomicF32::new(0.0);
        assert!((af.load(Ordering::SeqCst) - 0.0).abs() < 0.0001);

        af.store(0.5, Ordering::SeqCst);
        assert!((af.load(Ordering::SeqCst) - 0.5).abs() < 0.0001);

        af.store(1.5, Ordering::SeqCst);
        assert!((af.load(Ordering::SeqCst) - 1.5).abs() < 0.0001);
    }

    #[test]
    fn test_core_creation() {
        let core = AudioMidiDriverCore::new();

        // Test xruns
        assert_eq!(core.get_xruns(), 0);
        core.set_xruns(5);
        assert_eq!(core.get_xruns(), 5);

        // Test sample_rate
        assert_eq!(core.get_sample_rate(), 0);
        core.set_sample_rate(48000);
        assert_eq!(core.get_sample_rate(), 48000);

        // Test buffer_size
        assert_eq!(core.get_buffer_size(), 0);
        core.set_buffer_size(256);
        assert_eq!(core.get_buffer_size(), 256);

        // Test dsp_load
        assert!((core.get_dsp_load() - 0.0).abs() < 0.0001);
        core.set_dsp_load(0.75);
        assert!((core.get_dsp_load() - 0.75).abs() < 0.0001);

        // Test active
        assert!(!core.get_active());
        core.set_active(true);
        assert!(core.get_active());

        // Test last_processed
        assert_eq!(core.get_last_processed(), 1);
        core.set_last_processed(128);
        assert_eq!(core.get_last_processed(), 128);

        // Test client_name
        assert_eq!(core.get_client_name(), "unknown");
        core.set_client_name("test_client");
        assert_eq!(core.get_client_name(), "test_client");

        // Test client_handle
        assert_eq!(core.get_client_handle(), 0);
        core.set_client_handle(0x1234);
        assert_eq!(core.get_client_handle(), 0x1234);
    }

    #[test]
    fn test_xrun_management() {
        let core = AudioMidiDriverCore::new();

        assert_eq!(core.get_xruns(), 0);

        core.report_xrun();
        assert_eq!(core.get_xruns(), 1);

        core.report_xrun();
        core.report_xrun();
        assert_eq!(core.get_xruns(), 3);

        core.reset_xruns();
        assert_eq!(core.get_xruns(), 0);
    }

    #[test]
    fn test_processor_management() {
        let core = AudioMidiDriverCore::new();

        // Initially empty
        assert!(core.get_processor_handles().is_empty());

        // Add processors
        let h1 = core.add_processor(100, cxx::UniquePtr::null(), cxx::UniquePtr::null());
        let h2 = core.add_processor(200, cxx::UniquePtr::null(), cxx::UniquePtr::null());
        let _h3 = core.add_processor(300, cxx::UniquePtr::null(), cxx::UniquePtr::null());

        let handles = core.get_processor_handles();
        assert_eq!(handles.len(), 3);
        assert!(handles.contains(&h1));
        assert!(handles.contains(&h2));

        // same pointer can be registered with another handle
        let _h4 = core.add_processor(100, cxx::UniquePtr::null(), cxx::UniquePtr::null());
        assert_eq!(core.get_processor_handles().len(), 4);

        // Remove processor by handle
        core.remove_processor(h2);
        let handles = core.get_processor_handles();
        assert_eq!(handles.len(), 3);
    }
}
