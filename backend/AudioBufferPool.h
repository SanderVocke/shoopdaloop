#pragma once
#include <boost/lockfree/queue.hpp>
#include <memory>
#include <thread>
#include <atomic>
#include <chrono>
#include "AudioBuffer.h"

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

public:
    template<class Rep, class Period> AudioBufferPool(size_t target_n_buffers,
                    size_t buffers_size,
                    std::chrono::duration<Rep, Period> const& replenishment_delay) :
        m_target_n_buffers(target_n_buffers),
        m_buffers_size(buffers_size),
        m_actual_n_buffers(0),
        m_finish(false),
        m_queue(target_n_buffers)
    {
        fill();

        // Start auto-replenishment
        m_replenish_thread = std::thread(
            [replenishment_delay, this]() { this->replenish_thread_fn(replenishment_delay); });
    }

    ~AudioBufferPool() {
        Buffer *buf;
        while(m_queue.pop(buf)) {
            delete buf;
        }
        m_finish = true;
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
            return buf;
        }
        else {
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

    template<class Rep, class Period>
    void replenish_thread_fn(std::chrono::duration<Rep, Period> const& replenishment_delay) {
        while(true) {
            if (this->m_finish) { break; }
            this->fill();
            std::this_thread::sleep_for(replenishment_delay);
        }
    }

    Buffer *allocate() {
        return new Buffer(m_buffers_size);
    }
};