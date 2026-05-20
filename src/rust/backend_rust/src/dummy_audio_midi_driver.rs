//! DummyAudioMidiDriver - Rust core for the dummy audio/MIDI driver.
//!
//! Holds the mutable driver state (finish flag, mode, controlled samples, paused)
//! as atomics so the C++ wrapper can query and update them from multiple threads.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct DummyAudioMidiDriver {
    finish: AtomicBool,
    mode: AtomicU32, // 0 = Controlled, 1 = Automatic
    controlled_samples: AtomicU32,
    paused: AtomicBool,
}

impl DummyAudioMidiDriver {
    pub fn new() -> Self {
        DummyAudioMidiDriver {
            finish: AtomicBool::new(false),
            mode: AtomicU32::new(1),
            controlled_samples: AtomicU32::new(0),
            paused: AtomicBool::new(false),
        }
    }

    pub fn is_finish(&self) -> bool {
        self.finish.load(Ordering::SeqCst)
    }

    pub fn set_finish(&self) {
        self.finish.store(true, Ordering::SeqCst);
    }

    pub fn enter_mode(&self, mode: u32) {
        self.mode.store(mode, Ordering::SeqCst);
        self.controlled_samples.store(0, Ordering::SeqCst);
    }

    pub fn get_mode(&self) -> u32 {
        self.mode.load(Ordering::SeqCst)
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn controlled_mode_request_samples(&self, samples: u32) {
        self.controlled_samples.fetch_add(samples, Ordering::SeqCst);
    }

    pub fn get_controlled_mode_samples_to_process(&self) -> u32 {
        self.controlled_samples.load(Ordering::SeqCst)
    }

    pub fn controlled_mode_advance(&self, samples: u32) {
        let prev = self.controlled_samples.fetch_sub(samples, Ordering::SeqCst);
        if prev < samples {
            // Saturate at 0 if we underflowed
            self.controlled_samples.store(0, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_defaults() {
        let driver = DummyAudioMidiDriver::new();
        assert!(!driver.is_finish());
        assert_eq!(driver.get_mode(), 1); // Automatic = 1
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 0);
        assert!(!driver.is_paused());
    }

    #[test]
    fn test_set_finish() {
        let driver = DummyAudioMidiDriver::new();
        driver.set_finish();
        assert!(driver.is_finish());
    }

    #[test]
    fn test_enter_mode() {
        let driver = DummyAudioMidiDriver::new();
        driver.controlled_mode_request_samples(100);
        driver.enter_mode(1);
        assert_eq!(driver.get_mode(), 1);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 0);
    }

    #[test]
    fn test_pause_resume() {
        let driver = DummyAudioMidiDriver::new();
        assert!(!driver.is_paused());
        driver.pause();
        assert!(driver.is_paused());
        driver.resume();
        assert!(!driver.is_paused());
    }

    #[test]
    fn test_controlled_mode_request_samples() {
        let driver = DummyAudioMidiDriver::new();
        driver.controlled_mode_request_samples(50);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 50);
        driver.controlled_mode_request_samples(30);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 80);
    }

    #[test]
    fn test_controlled_mode_advance() {
        let driver = DummyAudioMidiDriver::new();
        driver.controlled_mode_request_samples(100);
        driver.controlled_mode_advance(30);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 70);
        driver.controlled_mode_advance(70);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 0);
    }

    #[test]
    fn test_controlled_mode_advance_saturates_at_zero() {
        let driver = DummyAudioMidiDriver::new();
        driver.controlled_mode_request_samples(10);
        driver.controlled_mode_advance(20);
        assert_eq!(driver.get_controlled_mode_samples_to_process(), 0);
    }
}
