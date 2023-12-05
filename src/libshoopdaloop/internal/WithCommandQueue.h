#pragma once
#include "CommandQueue.h"

// Helper class which defines a lock-free command queue. Commands in
// the form of functors can be inserted into the queue and executed
// in another thread.
// Typical use is with the audio back-end's process thread.
template<uint32_t QueueSize, uint32_t QueueTimeoutMs, uint32_t PollIntervalUs>
class WithCommandQueue {
protected:
    CommandQueue ma_queue;

    WithCommandQueue() :
        ma_queue(QueueSize, QueueTimeoutMs, PollIntervalUs)
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