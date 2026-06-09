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
        include!("backend_rust/src/decoupled_midi_port_cxx.rs.h");

        unsafe fn audiomididriver_invoke_maybe_process_callback(maybe_fn_ptr: usize);
        #[namespace = ""]
        type ProcessorBridgeStrong = crate::processor_cxx::ffi::ProcessorBridgeStrong;
        #[namespace = ""]
        type ProcessorBridgeWeak = crate::processor_cxx::ffi::ProcessorBridgeWeak;
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

        // Command queue
        fn command_queue_ptr(self: &AudioMidiDriverCore) -> usize;
        fn queue_process_thread_command(self: &AudioMidiDriverCore, user_data: usize);
        fn exec_process_thread_command(self: &AudioMidiDriverCore, user_data: usize);
        fn exec_all_commands_for_process_thread(self: &AudioMidiDriverCore);

        // Processor management (ptrs as usize)
        fn add_processor(
            self: &AudioMidiDriverCore,
            cpp_identity: usize,
            weak: UniquePtr<ProcessorBridgeWeak>,
            strong: UniquePtr<ProcessorBridgeStrong>,
        ) -> u64;
        fn remove_processor_by_cpp_identity(self: &AudioMidiDriverCore, cpp_identity: usize);
        fn get_processor_handles(self: &AudioMidiDriverCore) -> Vec<u64>;
        fn get_processor_bridge_weak_handle(
            self: &AudioMidiDriverCore,
            handle: u64,
        ) -> UniquePtr<ProcessorBridgeWeak>;

        // Decoupled port management
        unsafe fn add_decoupled_port_raw(core: &AudioMidiDriverCore, weak_ptr: usize);

        unsafe fn process_cycle(
            self: &AudioMidiDriverCore,
            maybe_process_callback_ptr: usize,
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
fn add_processor(
    core: &AudioMidiDriverCore,
    cpp_identity: usize,
    weak: cxx::UniquePtr<ffi::ProcessorBridgeWeak>,
    strong: cxx::UniquePtr<ffi::ProcessorBridgeStrong>,
) -> u64 {
    core.add_processor(cpp_identity, weak, strong)
}

fn remove_processor_by_cpp_identity(core: &AudioMidiDriverCore, cpp_identity: usize) {
    core.remove_processor_by_cpp_identity(cpp_identity);
}

fn get_processor_handles(core: &AudioMidiDriverCore) -> Vec<u64> {
    core.get_processor_handles()
}

// Decoupled port management
unsafe fn add_decoupled_port_raw(core: &AudioMidiDriverCore, weak_ptr: usize) {
    let weak = unsafe {
        Box::from_raw(weak_ptr as *mut crate::decoupled_midi_port_cxx::DecoupledMidiPortBridgeWeak)
    };
    core.add_decoupled_port(weak);
}

fn command_queue_ptr(core: &AudioMidiDriverCore) -> usize {
    core.command_queue_ptr()
}

fn queue_process_thread_command(core: &AudioMidiDriverCore, user_data: usize) {
    core.queue_process_thread_command(user_data);
}

fn exec_process_thread_command(core: &AudioMidiDriverCore, user_data: usize) {
    core.exec_process_thread_command(user_data);
}

fn exec_all_commands_for_process_thread(core: &AudioMidiDriverCore) {
    core.exec_all_commands_for_process_thread();
}

unsafe fn process_cycle(
    core: &AudioMidiDriverCore,
    maybe_process_callback_ptr: usize,
    nframes: u32,
) {
    core.process_cycle(maybe_process_callback_ptr, nframes);
}
