#pragma once
#include <boost/lockfree/spsc_queue.hpp>
#include <functional>
#include "LoggingEnabled.h"

// A lock-free queue suitable for passing functors to be executed in another thread.
// Typical use is for executing things on the processing thread of the audio back-end.
// There is also a fallback mechanism which will ensure the functors are called if
// the process thread does not seem to have executed for a while.
class CommandQueue : private boost::lockfree::spsc_queue<std::function<void()>>,
                     private ModuleLoggingEnabled<"Backend.CommandQueue"> {
    using Functor = std::function<void()>;
    using Queue = boost::lockfree::spsc_queue<Functor>;

    const uint32_t ma_timeout_ms;
    const uint32_t ma_poll_interval_us;
    std::atomic<bool> ma_passthrough_all = false;
    std::atomic<uint64_t> ma_last_processed = 0; // Milliseconds since epoch

    uint64_t millis_since_epoch();

public:
    CommandQueue(uint32_t fixed_size,
                 uint32_t timeout_ms,
                 uint32_t poll_interval_us);

    void queue(std::function<void()> fn);
    void queue_and_wait(std::function<void()> fn);

    void passthrough_on();

    void PROC_exec_all();

    void clear();
};