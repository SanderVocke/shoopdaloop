#pragma once
#include <boost/lockfree/spsc_queue.hpp>
#include <functional>
#include <chrono>
#include <ratio>
#include <stdexcept>
#include <thread>
#include <atomic>

// A lock-free queue suitable for passing functors to be executed in another thread.
// Typical use is for executing things on the processing thread of the audio back-end.
// There is also a fallback mechanism which will ensure the functors are called if
// the process thread does not seem to have executed for a while.
class CommandQueue : private boost::lockfree::spsc_queue<std::function<void()>> {
    using Functor = std::function<void()>;
    using Queue = boost::lockfree::spsc_queue<Functor>;

    const size_t ma_timeout_ms;
    const size_t ma_poll_interval_us;
    std::atomic<uint64_t> ma_last_processed; // Milliseconds since epoch

    uint64_t millis_since_epoch();

public:
    CommandQueue(size_t fixed_size,
                 size_t timeout_ms,
                 size_t poll_interval_us);

    void queue(std::function<void()> fn);
    void queue_and_wait(std::function<void()> fn);

    void PROC_exec_all();

    void clear();
};