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

    uint64_t millis_since_epoch() {
        return std::chrono::system_clock::now().time_since_epoch() / 
               std::chrono::milliseconds(1);
    }

public:
    CommandQueue(size_t fixed_size,
                 size_t timeout_ms,
                 size_t poll_interval_us) :
        Queue(fixed_size),
        ma_timeout_ms(timeout_ms),
        ma_poll_interval_us(poll_interval_us),
        ma_last_processed(0) {}

    void queue(std::function<void()> fn) {
        auto now = millis_since_epoch();
        if ((now - ma_last_processed) > ma_timeout_ms) {
            // Processing is apparently not active. Just execute the command directly.
            fn();
        } else {
            auto start = std::chrono::high_resolution_clock::now();
            while (!push(fn)) {
                auto now = std::chrono::high_resolution_clock::now();
                if ((now - start) > std::chrono::milliseconds(ma_timeout_ms)) {
                    throw std::runtime_error("Command queue: queue timeout");
                }
                std::this_thread::sleep_for(std::chrono::microseconds(ma_poll_interval_us));
            }
        }
    }

    void queue_and_wait(std::function<void()> fn) {
        auto now = millis_since_epoch();
        if ((now - ma_last_processed) > ma_timeout_ms) {
            // Processing is apparently not active. Just execute the command directly.
            fn();
        } else {
            std::atomic<bool> finished = false;
            std::atomic<bool> failed = false;
            auto _fn = [fn, &finished, &failed]() {
                try {
                    fn();
                } catch (...) {
                    failed = true;
                }
                finished = true;
            };
            queue(_fn);
            auto start = std::chrono::high_resolution_clock::now();
            while (!finished) {
                auto now = std::chrono::high_resolution_clock::now();
                if ((now - start) > std::chrono::milliseconds(ma_timeout_ms)) {
                    throw std::runtime_error("Command queue: exec wait timeout");
                }
                std::this_thread::sleep_for(std::chrono::microseconds(ma_poll_interval_us));
            }
            if (failed) {
                throw std::runtime_error("Command in queue threw an exception.");
            }
        }
    }

    void PROC_exec_all() {
        std::function<void()> cmd;
        while(pop(cmd)) {
            cmd();
        }
        ma_last_processed = millis_since_epoch();
    }

    void clear() { reset(); }
};