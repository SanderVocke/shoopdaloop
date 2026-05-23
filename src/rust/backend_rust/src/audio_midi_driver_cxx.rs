//! CXX bridge for AudioMidiDriverCore to expose to C++.
//!
//! Exposes the AudioMidiDriverCore type and its methods for use from C++.
//! Note: CommandQueue handling is done in C++ directly.

#![allow(dead_code)]

use crate::audio_midi_driver::AudioMidiDriverCore;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type AudioMidiDriverCore;

        // Constructor
        fn new_audio_midi_driver_core() -> Box<AudioMidiDriverCore>;

        // State getters
        fn get_xruns(self: &AudioMidiDriverCore) -> u32;
        fn get_sample_rate(self: &AudioMidiDriverCore) -> u32;
        fn get_buffer_size(self: &AudioMidiDriverCore) -> u32;
        fn get_dsp_load(self: &AudioMidiDriverCore) -> f32;
        fn get_active(self: &AudioMidiDriverCore) -> bool;
        fn get_last_processed(self: &AudioMidiDriverCore) -> u32;
        fn get_client_name(self: &AudioMidiDriverCore) -> String;
        fn get_client_handle(self: &AudioMidiDriverCore) -> usize;

        // State setters
        fn set_xruns(self: &AudioMidiDriverCore, val: u32);
        fn set_sample_rate(self: &AudioMidiDriverCore, val: u32);
        fn set_buffer_size(self: &AudioMidiDriverCore, val: u32);
        fn set_dsp_load(self: &AudioMidiDriverCore, val: f32);
        fn set_active(self: &AudioMidiDriverCore, val: bool);
        fn set_last_processed(self: &AudioMidiDriverCore, val: u32);
        fn set_client_name(self: &AudioMidiDriverCore, name: &str);
        fn set_client_handle(self: &AudioMidiDriverCore, handle: usize);

        // Xrun management
        fn report_xrun(self: &AudioMidiDriverCore);
        fn reset_xruns(self: &AudioMidiDriverCore);

        // Processor management (ptrs as usize)
        fn add_processor(self: &AudioMidiDriverCore, ptr: usize);
        fn remove_processor(self: &AudioMidiDriverCore, ptr: usize);
        fn get_processors(self: &AudioMidiDriverCore) -> Vec<usize>;

        // Decoupled port management
        fn register_decoupled_port(self: &AudioMidiDriverCore, ptr: usize);
        fn unregister_decoupled_port(self: &AudioMidiDriverCore, ptr: usize);
        fn get_decoupled_ports(self: &AudioMidiDriverCore) -> Vec<usize>;
    }
}

// Constructor
fn new_audio_midi_driver_core() -> Box<AudioMidiDriverCore> {
    Box::new(AudioMidiDriverCore::new())
}

// State getters
fn get_xruns(core: &AudioMidiDriverCore) -> u32 {
    core.get_xruns()
}

fn get_sample_rate(core: &AudioMidiDriverCore) -> u32 {
    core.get_sample_rate()
}

fn get_buffer_size(core: &AudioMidiDriverCore) -> u32 {
    core.get_buffer_size()
}

fn get_dsp_load(core: &AudioMidiDriverCore) -> f32 {
    core.get_dsp_load()
}

fn get_active(core: &AudioMidiDriverCore) -> bool {
    core.get_active()
}

fn get_last_processed(core: &AudioMidiDriverCore) -> u32 {
    core.get_last_processed()
}

fn get_client_name(core: &AudioMidiDriverCore) -> String {
    core.get_client_name()
}

fn get_client_handle(core: &AudioMidiDriverCore) -> usize {
    core.get_client_handle()
}

// State setters
fn set_xruns(core: &AudioMidiDriverCore, val: u32) {
    core.set_xruns(val);
}

fn set_sample_rate(core: &AudioMidiDriverCore, val: u32) {
    core.set_sample_rate(val);
}

fn set_buffer_size(core: &AudioMidiDriverCore, val: u32) {
    core.set_buffer_size(val);
}

fn set_dsp_load(core: &AudioMidiDriverCore, val: f32) {
    core.set_dsp_load(val);
}

fn set_active(core: &AudioMidiDriverCore, val: bool) {
    core.set_active(val);
}

fn set_last_processed(core: &AudioMidiDriverCore, val: u32) {
    core.set_last_processed(val);
}

fn set_client_name(core: &AudioMidiDriverCore, name: &str) {
    core.set_client_name(name);
}

fn set_client_handle(core: &AudioMidiDriverCore, handle: usize) {
    core.set_client_handle(handle);
}

// Xrun management
fn report_xrun(core: &AudioMidiDriverCore) {
    core.report_xrun();
}

fn reset_xruns(core: &AudioMidiDriverCore) {
    core.reset_xruns();
}

// Processor management
fn add_processor(core: &AudioMidiDriverCore, ptr: usize) {
    core.add_processor(ptr);
}

fn remove_processor(core: &AudioMidiDriverCore, ptr: usize) {
    core.remove_processor(ptr);
}

fn get_processors(core: &AudioMidiDriverCore) -> Vec<usize> {
    core.get_processors()
}

// Decoupled port management
fn register_decoupled_port(core: &AudioMidiDriverCore, ptr: usize) {
    core.register_decoupled_port(ptr);
}

fn unregister_decoupled_port(core: &AudioMidiDriverCore, ptr: usize) {
    core.unregister_decoupled_port(ptr);
}

fn get_decoupled_ports(core: &AudioMidiDriverCore) -> Vec<usize> {
    core.get_decoupled_ports()
}
