//! CXX bridge for DummyAudioMidiDriver to expose to C++.

#![allow(dead_code)]

use crate::dummy_audio_midi_driver::DummyAudioMidiDriver;

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/DummyAudioMidiDriverCxxTrampolines.h");
        unsafe fn dummy_audiomididriver_exec_commands(owner_ptr: usize);
        unsafe fn dummy_audiomididriver_process(owner_ptr: usize, nframes: u32);
    }

    extern "Rust" {
        type DummyAudioMidiDriver;

        fn new_dummy_audio_midi_driver() -> Box<DummyAudioMidiDriver>;
        fn is_finish(self: &DummyAudioMidiDriver) -> bool;
        fn set_finish(self: &DummyAudioMidiDriver);
        fn enter_mode(self: &DummyAudioMidiDriver, mode: u32);
        fn get_mode(self: &DummyAudioMidiDriver) -> u32;
        fn pause(self: &DummyAudioMidiDriver);
        fn resume(self: &DummyAudioMidiDriver);
        fn is_paused(self: &DummyAudioMidiDriver) -> bool;
        fn controlled_mode_request_samples(self: &DummyAudioMidiDriver, samples: u32);
        fn get_controlled_mode_samples_to_process(self: &DummyAudioMidiDriver) -> u32;
        fn controlled_mode_advance(self: &DummyAudioMidiDriver, samples: u32);
        fn start_process_thread(self: &DummyAudioMidiDriver, owner_ptr: usize, sample_rate: u32, buffer_size: u32);
        fn stop_process_thread(self: &DummyAudioMidiDriver);
    }
}

fn new_dummy_audio_midi_driver() -> Box<DummyAudioMidiDriver> {
    Box::new(DummyAudioMidiDriver::new())
}

fn is_finish(driver: &DummyAudioMidiDriver) -> bool {
    driver.is_finish()
}

fn set_finish(driver: &DummyAudioMidiDriver) {
    driver.set_finish();
}

fn enter_mode(driver: &DummyAudioMidiDriver, mode: u32) {
    driver.enter_mode(mode);
}

fn get_mode(driver: &DummyAudioMidiDriver) -> u32 {
    driver.get_mode()
}

fn pause(driver: &DummyAudioMidiDriver) {
    driver.pause();
}

fn resume(driver: &DummyAudioMidiDriver) {
    driver.resume();
}

fn is_paused(driver: &DummyAudioMidiDriver) -> bool {
    driver.is_paused()
}

fn controlled_mode_request_samples(driver: &DummyAudioMidiDriver, samples: u32) {
    driver.controlled_mode_request_samples(samples);
}

fn get_controlled_mode_samples_to_process(driver: &DummyAudioMidiDriver) -> u32 {
    driver.get_controlled_mode_samples_to_process()
}

fn controlled_mode_advance(driver: &DummyAudioMidiDriver, samples: u32) {
    driver.controlled_mode_advance(samples);
}
