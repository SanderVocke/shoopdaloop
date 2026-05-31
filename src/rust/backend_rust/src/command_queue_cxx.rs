//! CXX bridge for CommandQueue to expose to C++.

#![allow(dead_code)]

use crate::command_queue::CommandQueue;

#[cxx::bridge(namespace = "backend_rust")]
pub mod ffi {
    unsafe extern "C++" {
        include!("internal/CommandToken.h");

        unsafe fn cxx_command_execute_ptr(user_data: usize);
        unsafe fn cxx_command_destroy_ptr(user_data: usize);
    }

    extern "Rust" {
        type CommandQueue;

        fn new_command_queue(
            fixed_size: u32,
            timeout_ms: u32,
            poll_interval_us: u32,
        ) -> Box<CommandQueue>;

        unsafe fn cxx_queue(self: &CommandQueue, user_data: usize);
        unsafe fn cxx_queue_and_wait(self: &CommandQueue, user_data: usize);
        fn passthrough_on(self: &CommandQueue);
        fn exec_all(self: &CommandQueue);
        fn clear(self: &CommandQueue);
        unsafe fn cxx_exec_process_thread_command(self: &CommandQueue, user_data: usize);
    }
}

fn new_command_queue(fixed_size: u32, timeout_ms: u32, poll_interval_us: u32) -> Box<CommandQueue> {
    Box::new(CommandQueue::new(fixed_size, timeout_ms, poll_interval_us))
}
