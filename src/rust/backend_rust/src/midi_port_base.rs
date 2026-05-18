//! MidiPortBase - Core implementation of MIDI port logic in Rust.
//!
//! This class contains the core state tracking, ringbuffer management,
//! and event counting logic that can be composed into different MIDI port
//! implementations (MidiPort, DummyMidiPort, etc.).
//!
//! Ported from C++ MidiPortBase.h/cpp

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use crate::midi_state_tracker::{new_midi_state_tracker, MidiStateTracker};
use crate::midi_storage::{MidiStorageCore, MidiTimeWindow};
use crate::midi_traits::{MidiRingbufferOps, MidiStateTracking};

/// Default MIDI storage size in bytes (65536 * 8 = 524288)
const DEFAULT_MIDI_STORAGE_SIZE: u32 = 65536 * 8;

/// Configuration for what to track in MIDI state.
#[derive(Debug, Clone)]
pub struct TrackingConfig {
    pub track_notes: bool,
    pub track_controls: bool,
    pub track_programs: bool,
}

impl Default for TrackingConfig {
    fn default() -> Self {
        TrackingConfig {
            track_notes: true,
            track_controls: true,
            track_programs: true,
        }
    }
}

impl TrackingConfig {
    pub fn new(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        TrackingConfig {
            track_notes,
            track_controls,
            track_programs,
        }
    }
}

/// MidiPortBase - Core implementation of MIDI port logic.
///
/// Contains the core state tracking, ringbuffer management, and event
/// counting logic. This is thread-safe for atomic members (event counts, muted state).
pub struct MidiPortBase {
    // Tracking configuration
    tracking_config: TrackingConfig,

    // The current MIDI state on the port.
    maybe_midi_state: Option<Box<MidiStateTracker>>,

    // The MIDI state at the tail of the ringbuffer. This is basically a time-delayed
    // version of the current state.
    ringbuffer_tail_state: Option<Box<MidiStateTracker>>,

    // The MIDI ringbuffer for retroactive recording
    // We use MidiTimeWindow + MidiStorageCore for the ringbuffer
    midi_time_window: Option<Box<MidiTimeWindow>>,
    midi_storage: Option<Box<MidiStorageCore>>,

    // Mute state
    muted: AtomicBool,

    // Event counters
    n_input_events: AtomicU32,
    n_output_events: AtomicU32,
}

impl MidiPortBase {
    /// Create a new MidiPortBase with configuration
    pub fn new(config: &TrackingConfig) -> Self {
        let maybe_midi_state =
            if config.track_notes || config.track_controls || config.track_programs {
                Some(new_midi_state_tracker(
                    config.track_notes,
                    config.track_controls,
                    config.track_programs,
                ))
            } else {
                None
            };

        let ringbuffer_tail_state = Some(new_midi_state_tracker(
            config.track_notes,
            config.track_controls,
            config.track_programs,
        ));

        MidiPortBase {
            tracking_config: config.clone(),
            maybe_midi_state,
            ringbuffer_tail_state,
            midi_time_window: None,
            midi_storage: None,
            muted: AtomicBool::new(false),
            n_input_events: AtomicU32::new(0),
            n_output_events: AtomicU32::new(0),
        }
    }

    /// Create a new MidiPortBase with individual tracking flags (backward compatible)
    pub fn new_with_flags(track_notes: bool, track_controls: bool, track_programs: bool) -> Self {
        Self::new(&TrackingConfig::new(
            track_notes,
            track_controls,
            track_programs,
        ))
    }

    // State tracker accessors
    pub fn maybe_midi_state_tracker(&mut self) -> Option<&mut MidiStateTracker> {
        self.maybe_midi_state.as_mut().map(|b| b.as_mut())
    }

    pub fn maybe_ringbuffer_tail_state_tracker(&mut self) -> Option<&mut MidiStateTracker> {
        self.ringbuffer_tail_state.as_mut().map(|b| b.as_mut())
    }

    // Ringbuffer accessors
    pub fn maybe_midi_ringbuffer_time_window(&mut self) -> Option<&mut MidiTimeWindow> {
        self.midi_time_window.as_mut().map(|b| b.as_mut())
    }

    pub fn maybe_midi_storage(&mut self) -> Option<&mut MidiStorageCore> {
        self.midi_storage.as_mut().map(|b| b.as_mut())
    }

    /// Initialize the ringbuffer if not already initialized
    fn ensure_ringbuffer_initialized(&mut self) {
        if self.midi_time_window.is_none() {
            self.midi_storage = Some(Box::new(MidiStorageCore::new(DEFAULT_MIDI_STORAGE_SIZE)));
            self.midi_time_window = Some(Box::new(MidiTimeWindow::new()));
        }
    }

    // Mute state accessors
    pub fn set_muted(&self, muted: bool) {
        self.muted.store(muted, Ordering::SeqCst);
    }

    pub fn get_muted(&self) -> bool {
        self.muted.load(Ordering::SeqCst)
    }

    // Event counter management
    pub fn reset_n_input_events(&self) {
        self.n_input_events.store(0, Ordering::SeqCst);
    }

    pub fn get_n_input_events(&self) -> u32 {
        self.n_input_events.load(Ordering::SeqCst)
    }

    pub fn increment_input_events(&self, count: u32) {
        self.n_input_events.fetch_add(count, Ordering::SeqCst);
    }

    pub fn reset_n_output_events(&self) {
        self.n_output_events.store(0, Ordering::SeqCst);
    }

    pub fn get_n_output_events(&self) -> u32 {
        self.n_output_events.load(Ordering::SeqCst)
    }

    pub fn increment_output_events(&self, count: u32) {
        self.n_output_events.fetch_add(count, Ordering::SeqCst);
    }

    // State processing helpers
    pub fn process_msg_to_state(&mut self, data: &[u8]) {
        if let Some(ref mut state) = self.maybe_midi_state {
            unsafe {
                crate::midi_state_tracker::process_msg_raw(state.as_mut(), data.as_ptr());
            }
        }
    }

    /// Process a raw MIDI message (for FFI boundary)
    pub unsafe fn process_msg_to_state_raw(&mut self, data: *const u8, size: usize) {
        let slice = std::slice::from_raw_parts(data, size);
        self.process_msg_to_state(slice);
    }

    /// Snapshot ringbuffer contents into a target storage
    pub fn snapshot_ringbuffer_into(&self, target: &mut MidiStorageCore) {
        if let (Some(ref tw), Some(ref ms)) = (&self.midi_time_window, &self.midi_storage) {
            tw.snapshot(ms, target, 0);
        } else {
            target.clear();
        }
    }

    /// Get the tracking config
    pub fn tracking_config(&self) -> &TrackingConfig {
        &self.tracking_config
    }

    // Direct method accessors (delegates to trait implementations for CXX bridge)
    pub fn n_notes_active_direct(&self) -> u32 {
        MidiStateTracking::n_notes_active(self)
    }

    pub fn input_event_count_direct(&self) -> u32 {
        MidiStateTracking::input_event_count(self)
    }

    pub fn output_event_count_direct(&self) -> u32 {
        MidiStateTracking::output_event_count(self)
    }

    pub fn set_n_samples_direct(&mut self, n: u32) {
        MidiRingbufferOps::set_n_samples(self, n);
    }

    pub fn get_n_samples_direct(&self) -> u32 {
        MidiRingbufferOps::get_n_samples(self)
    }

    pub fn get_current_n_samples_direct(&self) -> u32 {
        MidiRingbufferOps::get_current_n_samples(self)
    }
}

// Implement MidiStateTracking trait for MidiPortBase
impl MidiStateTracking for MidiPortBase {
    fn n_notes_active(&self) -> u32 {
        if let Some(ref state) = self.maybe_midi_state {
            crate::midi_state_tracker::n_notes_active(state.as_ref())
        } else {
            0
        }
    }

    fn input_event_count(&self) -> u32 {
        self.n_input_events.load(Ordering::SeqCst)
    }

    fn output_event_count(&self) -> u32 {
        self.n_output_events.load(Ordering::SeqCst)
    }
}

// Implement MidiRingbufferOps trait for MidiPortBase
impl MidiRingbufferOps for MidiPortBase {
    fn set_n_samples(&mut self, n: u32) {
        if n > 0 {
            self.ensure_ringbuffer_initialized();
            if let Some(ref mut tw) = self.midi_time_window {
                tw.set_n_samples(n);
            }
        }
    }

    fn get_n_samples(&self) -> u32 {
        if let Some(ref tw) = self.midi_time_window {
            tw.get_n_samples()
        } else {
            0
        }
    }

    fn snapshot(&self, target: &mut MidiStorageCore, start_offset_from_end: Option<u32>) {
        if let (Some(ref tw), Some(ref ms)) = (&self.midi_time_window, &self.midi_storage) {
            let offset = start_offset_from_end.unwrap_or(0);
            tw.snapshot(ms, target, offset);
        } else {
            target.clear();
        }
    }

    fn get_current_n_samples(&self) -> u32 {
        self.get_n_samples()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracking_config() {
        let config = TrackingConfig::default();
        assert!(config.track_notes);
        assert!(config.track_controls);
        assert!(config.track_programs);

        let config2 = TrackingConfig::new(false, true, false);
        assert!(!config2.track_notes);
        assert!(config2.track_controls);
        assert!(!config2.track_programs);
    }

    #[test]
    fn test_midi_port_base_creation() {
        let config = TrackingConfig::new(true, true, true);
        let port = MidiPortBase::new(&config);

        assert!(!port.get_muted());
        assert_eq!(port.get_n_input_events(), 0);
        assert_eq!(port.get_n_output_events(), 0);
    }

    #[test]
    fn test_midi_port_base_with_flags() {
        let port = MidiPortBase::new_with_flags(true, false, false);
        assert_eq!(port.tracking_config.track_notes, true);
        assert_eq!(port.tracking_config.track_controls, false);
        assert_eq!(port.tracking_config.track_programs, false);
    }

    #[test]
    fn test_mute_state() {
        let port = MidiPortBase::new_with_flags(true, true, true);

        assert!(!port.get_muted());
        port.set_muted(true);
        assert!(port.get_muted());
        port.set_muted(false);
        assert!(!port.get_muted());
    }

    #[test]
    fn test_event_counters() {
        let port = MidiPortBase::new_with_flags(true, true, true);

        assert_eq!(port.get_n_input_events(), 0);
        assert_eq!(port.get_n_output_events(), 0);

        port.increment_input_events(5);
        port.increment_output_events(10);

        assert_eq!(port.get_n_input_events(), 5);
        assert_eq!(port.get_n_output_events(), 10);

        port.reset_n_input_events();
        assert_eq!(port.get_n_input_events(), 0);

        port.reset_n_output_events();
        assert_eq!(port.get_n_output_events(), 0);
    }

    #[test]
    fn test_ringbuffer_operations() {
        let mut port = MidiPortBase::new_with_flags(true, true, true);

        // Initially no ringbuffer
        assert_eq!(port.get_n_samples(), 0);

        // Set n_samples should initialize ringbuffer
        port.set_n_samples(1024);
        assert_eq!(port.get_n_samples(), 1024);

        // Get n_samples should return 1024
        assert_eq!(port.get_n_samples(), 1024);
    }

    #[test]
    fn test_midi_state_tracking_trait() {
        let port = MidiPortBase::new_with_flags(true, true, true);

        // Should implement MidiStateTracking
        assert_eq!(port.n_notes_active(), 0); // No notes yet
        assert_eq!(port.input_event_count(), 0);
        assert_eq!(port.output_event_count(), 0);
    }

    #[test]
    fn test_midi_ringbuffer_ops_trait() {
        let mut port = MidiPortBase::new_with_flags(true, true, true);

        // Should implement MidiRingbufferOps
        port.set_n_samples(512);
        assert_eq!(port.get_n_samples(), 512);
        assert_eq!(port.get_current_n_samples(), 512);
    }
}
