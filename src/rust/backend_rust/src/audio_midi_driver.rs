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

use std::sync::atomic::{AtomicBool, AtomicPtr, AtomicU32, Ordering};
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
/// Holds atomic state and lists of processors/decoupled ports.
/// Processors and ports are stored as usize pointers; actual iteration
/// and processing happens via callbacks to C++.
///
/// Note: CommandQueue handling is done in C++ via the C++ AudioMidiDriver
/// which owns the CommandQueue (as a wrapper around a Rust CommandQueue).
pub struct AudioMidiDriverCore {
    state: AudioMidiDriverState,
    processors: RwLock<Vec<usize>>,
    decoupled_ports: RwLock<Vec<usize>>,
}

impl AudioMidiDriverCore {
    /// Create a new AudioMidiDriverCore.
    pub fn new() -> Self {
        AudioMidiDriverCore {
            state: AudioMidiDriverState::new(),
            processors: RwLock::new(Vec::new()),
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

    /// Add a processor pointer.
    /// The pointer is stored as usize; C++ is responsible for the actual object.
    pub fn add_processor(&self, ptr: usize) {
        if let Ok(mut guard) = self.processors.write() {
            // Check if already present
            if !guard.contains(&ptr) {
                guard.push(ptr);
            }
        }
    }

    /// Remove a processor pointer.
    pub fn remove_processor(&self, ptr: usize) {
        if let Ok(mut guard) = self.processors.write() {
            guard.retain(|&p| p != ptr);
        }
    }

    /// Get a copy of the processor pointers list.
    /// Used by C++ to iterate and call processors.
    pub fn get_processors(&self) -> Vec<usize> {
        self.processors
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    // ========================================================================
    // Decoupled port management
    // ========================================================================

    /// Register a decoupled MIDI port pointer.
    pub fn register_decoupled_port(&self, ptr: usize) {
        if let Ok(mut guard) = self.decoupled_ports.write() {
            if !guard.contains(&ptr) {
                guard.push(ptr);
            }
        }
    }

    /// Unregister a decoupled MIDI port pointer.
    pub fn unregister_decoupled_port(&self, ptr: usize) {
        if let Ok(mut guard) = self.decoupled_ports.write() {
            guard.retain(|&p| p != ptr);
        }
    }

    /// Get a copy of the decoupled port pointers list.
    /// Used by C++ to iterate and process decoupled ports.
    pub fn get_decoupled_ports(&self) -> Vec<usize> {
        self.decoupled_ports
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
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
        assert!(core.get_processors().is_empty());

        // Add processors
        core.add_processor(100);
        core.add_processor(200);
        core.add_processor(300);

        let procs = core.get_processors();
        assert_eq!(procs.len(), 3);
        assert!(procs.contains(&100));
        assert!(procs.contains(&200));
        assert!(procs.contains(&300));

        // Adding duplicate should not increase count
        core.add_processor(100);
        assert_eq!(core.get_processors().len(), 3);

        // Remove processor
        core.remove_processor(200);
        let procs = core.get_processors();
        assert_eq!(procs.len(), 2);
        assert!(!procs.contains(&200));
    }

    #[test]
    fn test_decoupled_port_management() {
        let core = AudioMidiDriverCore::new();

        // Initially empty
        assert!(core.get_decoupled_ports().is_empty());

        // Add ports
        core.register_decoupled_port(1000);
        core.register_decoupled_port(2000);

        let ports = core.get_decoupled_ports();
        assert_eq!(ports.len(), 2);
        assert!(ports.contains(&1000));
        assert!(ports.contains(&2000));

        // Adding duplicate should not increase count
        core.register_decoupled_port(1000);
        assert_eq!(core.get_decoupled_ports().len(), 2);

        // Remove port
        core.unregister_decoupled_port(1000);
        let ports = core.get_decoupled_ports();
        assert_eq!(ports.len(), 1);
        assert!(!ports.contains(&1000));
    }
}
