//! CXX bridge for AudioMidiDriverCore to expose to C++.
//!
//! Exposes the AudioMidiDriverCore type and its methods for use from C++.
//! Note: CommandQueue handling is done in C++ directly.

#![allow(dead_code)]

use crate::audio_midi_driver::AudioMidiDriverCore;

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/AudioMidiDriverCxxTrampolines.h");

        unsafe fn audiomididriver_invoke_maybe_process_callback(maybe_fn_ptr: usize);
        unsafe fn audiomididriver_exec_command_queue(command_queue_ptr: usize);
        unsafe fn audiomididriver_process_processor(processor_ptr: usize, nframes: u32);
        unsafe fn audiomididriver_process_decoupled_port(decoupled_port_ptr: usize, nframes: u32);
        unsafe fn audiomididriver_close_decoupled_port(decoupled_port_ptr: usize);
    }

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
        fn add_processor(self: &AudioMidiDriverCore, ptr: usize) -> u64;
        fn remove_processor(self: &AudioMidiDriverCore, handle: u64);
        fn get_processor_handles(self: &AudioMidiDriverCore) -> Vec<u64>;

        // Decoupled port management
        fn register_decoupled_port(self: &AudioMidiDriverCore, ptr: usize) -> u64;
        fn unregister_decoupled_port(self: &AudioMidiDriverCore, handle: u64);
        fn process_decoupled_port(self: &AudioMidiDriverCore, handle: u64, nframes: u32) -> bool;
        fn close_decoupled_port(self: &AudioMidiDriverCore, handle: u64) -> bool;
        fn get_decoupled_ports(self: &AudioMidiDriverCore) -> Vec<usize>;

        unsafe fn process_cycle(
            self: &AudioMidiDriverCore,
            maybe_process_callback_ptr: usize,
            command_queue_ptr: usize,
            nframes: u32,
        );
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
fn add_processor(core: &AudioMidiDriverCore, ptr: usize) -> u64 {
    core.add_processor(ptr)
}

fn remove_processor(core: &AudioMidiDriverCore, handle: u64) {
    core.remove_processor(handle);
}

fn get_processor_handles(core: &AudioMidiDriverCore) -> Vec<u64> {
    core.get_processor_handles()
}

// Decoupled port management
fn register_decoupled_port(core: &AudioMidiDriverCore, ptr: usize) -> u64 {
    core.register_decoupled_port(ptr)
}

fn unregister_decoupled_port(core: &AudioMidiDriverCore, handle: u64) {
    core.unregister_decoupled_port(handle);
}

fn process_decoupled_port(core: &AudioMidiDriverCore, handle: u64, nframes: u32) -> bool {
    core.process_decoupled_port(handle, nframes)
}

fn close_decoupled_port(core: &AudioMidiDriverCore, handle: u64) -> bool {
    core.close_decoupled_port(handle)
}

fn get_decoupled_ports(core: &AudioMidiDriverCore) -> Vec<usize> {
    core.get_decoupled_ports()
}

unsafe fn process_cycle(
    core: &AudioMidiDriverCore,
    maybe_process_callback_ptr: usize,
    command_queue_ptr: usize,
    nframes: u32,
) {
    core.process_cycle(maybe_process_callback_ptr, command_queue_ptr, nframes);
}
