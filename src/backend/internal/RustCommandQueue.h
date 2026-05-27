#pragma once

#include <functional>
#include <cstdint>

#include "CommandToken.h"
#include "backend_rust/src/command_queue_cxx.rs.h"

namespace rust_command_queue {
inline rust::Box<backend_rust::CommandQueue> make(uint32_t fixed_size, uint32_t timeout_ms, uint32_t poll_interval_us) {
    return backend_rust::new_command_queue(fixed_size, timeout_ms, poll_interval_us);
}

inline void queue(backend_rust::CommandQueue &queue, std::function<void()> fn) {
    if (!fn) return;
    queue.cxx_queue(backend_rust::make_command_token_ptr(std::move(fn)));
}

inline void queue_and_wait(backend_rust::CommandQueue &queue, std::function<void()> fn) {
    if (!fn) return;
    queue.cxx_queue_and_wait(backend_rust::make_command_token_ptr(std::move(fn)));
}

inline void exec_all(backend_rust::CommandQueue &queue) {
    queue.exec_all();
}

inline void passthrough_on(backend_rust::CommandQueue &queue) {
    queue.passthrough_on();
}

inline void clear(backend_rust::CommandQueue &queue) {
    queue.clear();
}
}
