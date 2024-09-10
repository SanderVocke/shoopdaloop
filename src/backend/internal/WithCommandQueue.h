#pragma once
#include "CommandQueue.h"
#include <cstdint>
#include "shoop_globals.h"

// Helper class which defines a lock-free command queue. Commands in
// the form of functors can be inserted into the queue and executed
// in another thread.
// Typical use is with the audio back-end's process thread.
class WithCommandQueue {
protected:
    CommandQueue ma_queue;

    WithCommandQueue(
        uint32_t size = shoop_constants::command_queue_size,
        uint32_t timeout_ms = 1000,
        uint32_t poll_interval_us = 1000) :
        ma_queue(size, timeout_ms, poll_interval_us)
    {}

public:
    void exec_process_thread_command(std::function<void()> fn) {
        ma_queue.queue_and_wait(fn);
    }

    void queue_process_thread_command(std::function<void()> fn) {
        ma_queue.queue(fn);
    }

    void PROC_handle_command_queue() {
        ma_queue.PROC_exec_all();
    }
};