#include "CommandQueue.h"
#include <cstddef>

// Command executor - called from Rust when a queued command is executed
// user_data is a pointer to a std::function<void()>
static void command_executor_callback(std::size_t user_data) {
    auto fn = reinterpret_cast<std::function<void()>*>(user_data);
    if (fn) {
        (*fn)();
        // Delete the heap allocation after execution
        delete fn;
    }
}

// Static initialization to register the callback before any CommandQueue is used
namespace {
    struct CallbackRegistrar {
        CallbackRegistrar() {
            // Register the callback with Rust - cast function pointer to size_t
            backend_rust::register_command_callback_ptr(
                reinterpret_cast<std::size_t>(command_executor_callback)
            );
        }
    };
    static CallbackRegistrar registrar;
}

CommandQueue::CommandQueue(uint32_t fixed_size,
                           uint32_t timeout_ms,
                           uint32_t poll_interval_us)
    : m_rust(backend_rust::new_command_queue(fixed_size, timeout_ms, poll_interval_us)) {}

void CommandQueue::queue(std::function<void()> fn) {
    if (!fn) return;
    
    // Create a new std::function on the heap - Rust will call back to execute it
    auto fn_ptr = new std::function<void()>(std::move(fn));
    
    // Pass the pointer to Rust
    // Rust will store a wrapper that calls command_executor_callback with this pointer
    // When executed, callback will invoke the function and delete the heap allocation
    m_rust->cxx_queue(reinterpret_cast<std::size_t>(fn_ptr));
}

void CommandQueue::queue_and_wait(std::function<void()> fn) {
    if (!fn) return;
    
    auto fn_ptr = new std::function<void()>(std::move(fn));
    m_rust->cxx_queue_and_wait(reinterpret_cast<std::size_t>(fn_ptr));
}

void CommandQueue::passthrough_on() {
    m_rust->passthrough_on();
}

void CommandQueue::PROC_exec_all() {
    m_rust->exec_all();
}

void CommandQueue::clear() {
    m_rust->clear();
}