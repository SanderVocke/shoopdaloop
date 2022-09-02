#pragma once
#include "CommandQueue.h"

// Helper class which defines a lock-free command queue. Commands in
// the form of functors can be inserted into the queue and executed
// in another thread.
// Typical use is with the audio back-end's process thread.
template<size_t QueueSize, size_t QueueTimeoutMs, size_t PollIntervalUs>
class WithCommandQueue {
    CommandQueue ma_queue;
    
protected:
    WithCommandQueue() :
        ma_queue(QueueSize, QueueTimeoutMs, PollIntervalUs)
    {}

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