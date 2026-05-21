#pragma once
#include <boost/lockfree/spsc_queue.hpp>
#include <functional>
#include "shoop_shared_ptr.h"
#include "backend_rust/src/command_queue_cxx.rs.h"

// A lock-free queue suitable for passing functors to be executed in another thread.
// Typical use is for executing things on the processing thread of the audio back-end.
// There is also a fallback mechanism which will ensure the functors are called if
// the process thread does not seem to have executed for a while.
//
// This is a C++ wrapper around the Rust implementation via CXX bridge.
class CommandQueue {
    rust::Box<backend_rust::CommandQueue> m_rust;

public:
    CommandQueue(uint32_t fixed_size,
                 uint32_t timeout_ms,
                 uint32_t poll_interval_us);

    CommandQueue(const CommandQueue&) = delete;
    CommandQueue& operator=(const CommandQueue& other) = delete;

    void queue(std::function<void()> fn);
    void queue_and_wait(std::function<void()> fn);

    void passthrough_on();

    void PROC_exec_all();

    void clear();

    // Convenience methods (previously provided by WithCommandQueue mixin)
    void exec_process_thread_command(std::function<void()> fn) { queue_and_wait(std::move(fn)); }
    void queue_process_thread_command(std::function<void()> fn) { queue(std::move(fn)); }
    void PROC_handle_command_queue() { PROC_exec_all(); }

    backend_rust::CommandQueue* raw_ptr() { return m_rust.operator->(); }
    backend_rust::CommandQueue const* raw_ptr() const { return m_rust.operator->(); }
};