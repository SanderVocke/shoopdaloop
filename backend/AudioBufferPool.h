#pragma once
#include <boost/lockfree/queue.hpp>
#include <memory>
#include <thread>
#include <atomic>
#include "AudioBuffer.h"
#include <iostream>

// A class which manages a queue of audio buffers which can be
// consumed lock-free. The queue is continuously replenished with newly allocated
// buffers asynchronously.
template<typename SampleT>
class AudioBufferPool {
    typedef AudioBuffer<SampleT> Buffer;

    boost::lockfree::queue<Buffer*> m_queue;
    const unsigned m_target_n_buffers;
    unsigned m_buffers_size;
    std::atomic<unsigned> m_actual_n_buffers;
    std::atomic<bool> m_finish;
    std::thread m_replenish_thread;
    std::atomic_flag m_replenish_flag;
    std::atomic_flag m_no_bufs_available_flag;

public:
    AudioBufferPool(size_t target_n_buffers, size_t buffers_size) :
        m_target_n_buffers(target_n_buffers),
        m_buffers_size(buffers_size),
        m_actual_n_buffers(0),
        m_finish(false),
        m_queue(target_n_buffers)
    {
        fill();
        m_replenish_flag.clear();
        m_no_bufs_available_flag.clear();

        // Start auto-replenishment
        m_replenish_thread = std::thread(
            [this]() { this->replenish_thread_fn(); });
    }

    ~AudioBufferPool() {
        Buffer *buf;
        while(m_queue.pop(buf)) {
            delete buf;
        }
        m_finish = true;
        m_replenish_flag.test_and_set();
        m_replenish_flag.notify_all();
        m_replenish_thread.join();
    }

    // Get a buffer. It is a shared pointer but if gotten via this
    // method, there will be no other references to it anywhere.
    // The buffer is taken lock-free from the pool. Should the pool be
    // empty, it is allocated in-place.
    Buffer* get_buffer() {
        Buffer* buf = nullptr;
        if (m_queue.pop(buf)) {
            m_actual_n_buffers--;
            m_replenish_flag.test_and_set();
            m_replenish_flag.notify_all();
            return buf;
        }
        else {
            m_no_bufs_available_flag.test_and_set();
            return allocate();
        }
    }

    size_t buffer_size() const { return m_buffers_size; }

protected:
    void push() {
        m_queue.push(allocate());
        m_actual_n_buffers++;
    }

    void fill() {
        auto n_replenish = m_target_n_buffers - m_actual_n_buffers;
        for(size_t idx=0; idx<n_replenish; idx++) {
            push();
        }
    }

    void replenish_thread_fn() {
        while(true) {
            if (this->m_finish) { break; }
            m_replenish_flag.wait(false);
            m_replenish_flag.clear();
            if (m_no_bufs_available_flag.test()) {
                m_no_bufs_available_flag.clear();
                std::cerr << "Warning: one or more audio buffers were allocated on the processing thread because "
                             "no pre-allocated buffers were available from the pool." << std::endl;
            }
            this->fill();
        }
    }

    Buffer *allocate() {
        return new Buffer(m_buffers_size);
    }
};