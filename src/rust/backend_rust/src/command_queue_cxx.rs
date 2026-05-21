//! CXX bridge for CommandQueue to expose to C++.
//!
//! The bridge provides method signatures for C++ to call.

#![allow(dead_code)]

use crate::command_queue::CommandQueue;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type CommandQueue;

        // Constructor
        fn new_command_queue(
            fixed_size: u32,
            timeout_ms: u32,
            poll_interval_us: u32,
        ) -> Box<CommandQueue>;

        // Methods on CommandQueue
        // SAFETY: user_data must be valid until command is executed or queue is cleared
        // user_data is passed as usize (cast from raw pointer)
        unsafe fn cxx_queue(self: &CommandQueue, user_data: usize);
        unsafe fn cxx_queue_and_wait(self: &CommandQueue, user_data: usize);
        fn passthrough_on(self: &CommandQueue);
        fn exec_all(self: &CommandQueue);
        fn clear(self: &CommandQueue);

        // Convenience method - queue and wait
        unsafe fn cxx_exec_process_thread_command(self: &CommandQueue, user_data: usize);

        // Callback registration - passes the function pointer as usize
        fn register_command_callback_ptr(callback_ptr: usize);
    }
}

fn new_command_queue(fixed_size: u32, timeout_ms: u32, poll_interval_us: u32) -> Box<CommandQueue> {
    Box::new(CommandQueue::new(fixed_size, timeout_ms, poll_interval_us))
}

// Register command callback from C++
// The callback pointer is passed as usize (function pointer cast to usize)
fn register_command_callback_ptr(callback_ptr: usize) {
    crate::command_queue::set_command_callback(callback_ptr);
}
