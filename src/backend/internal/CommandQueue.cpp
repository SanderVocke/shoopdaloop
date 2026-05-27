#include "CommandQueue.h"
#include "CommandToken.h"

using backend_rust::make_command_token_ptr;

CommandQueue::CommandQueue(uint32_t fixed_size,
                           uint32_t timeout_ms,
                           uint32_t poll_interval_us)
    : m_rust(backend_rust::new_command_queue(fixed_size, timeout_ms, poll_interval_us)) {}

void CommandQueue::queue(std::function<void()> fn) {
    if (!fn) return;
    m_rust->cxx_queue(make_command_token_ptr(std::move(fn)));
}

void CommandQueue::queue_and_wait(std::function<void()> fn) {
    if (!fn) return;
    m_rust->cxx_queue_and_wait(make_command_token_ptr(std::move(fn)));
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
