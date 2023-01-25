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
    boost::lockfree::queue<std::shared_ptr<AudioBuffer<SampleT>>> m_queue;
    const unsigned m_target_n_buffers;
    unsigned m_buffers_size;
    std::atomic<unsigned> m_actual_n_buffers;
    std::atomic<bool> m_finish;
    std::thread m_replenish_thread;
    const std::chrono::duration<double, std::micro> m_replenish_delay;

public:
    template<class Rep, class Period> AudioBufferPool(size_t target_n_buffers,
                    size_t buffers_size,
                    std::chrono::duration<Rep, Period> const& replenishment_delay) :
        m_target_n_buffers(target_n_buffers),
        m_buffers_size(buffers_size),
        m_actual_n_buffers(0),
        m_finish(false),
        m_replenish_delay(std::chrono::duration_cast<decltype(m_replenish_delay)>(replenishment_delay)
    {
        m_queue = decltype(m_queue)(m_target_n_buffers);
        fill();

        // Start auto-replenishment
        m_replenish_thread = std::thread(&AudioBufferPool<SampleT>::replenish_thread_fn, *this);
    }

    ~AudioBufferPool() {
        if (m_replenish_thread) {
            m_finish = true;
            m_replenish_thread.join();
        }
    }

    // Get a buffer. It is a shared pointer but if gotten via this
    // method, there will be no other references to it anywhere.
    // The buffer is taken lock-free from the pool. Should the pool be
    // empty, it is allocated in-place.
    std::shared_ptr<AudioBuffer<SampleT>> get() {
        std::shared_ptr<AudioBuffer<SampleT>> buf = nullptr;
        if (m_queue.pop(buf)) { return buf; }
        else { return allocate(); }
    }

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
            fill();
            std::this_thread::sleep_for(m_replenish_delay);
        }
    }

    std::shared_ptr<AudioBuffer<SampleT>> allocate() {
        return std::make_shared<AudioBuffer<SampleT>>(m_buffers_size);
    }
};