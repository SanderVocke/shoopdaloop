#pragma once
#include <boost/lockfree/spsc_queue.hpp>
#include <functional>
#include <chrono>
#include <stdexcept>
#include <thread>
#include <atomic>

class CommandQueue : private boost::lockfree::spsc_queue<std::function<void()>> {
    using Functor = std::function<void()>;
    using Queue = boost::lockfree::spsc_queue<Functor>;

    const size_t ma_queue_timeout_us;
    const size_t ma_exec_timeout_us;
    const size_t ma_poll_interval_us;
public:
    CommandQueue(size_t fixed_size,
                 size_t queue_timeout_us,
                 size_t exec_timeout_us,
                 size_t poll_interval_us) :
        Queue(fixed_size),
        ma_exec_timeout_us(exec_timeout_us),
        ma_queue_timeout_us(queue_timeout_us),
        ma_poll_interval_us(poll_interval_us) {}

    void queue(std::function<void()> fn) {
        auto start = std::chrono::high_resolution_clock::now();
        while (!push(fn)) {
            auto now = std::chrono::high_resolution_clock::now();
            if ((now - start) > std::chrono::microseconds(ma_queue_timeout_us)) {
                throw std::runtime_error("Command queue: queue timeout");
            }
            std::this_thread::sleep_for(std::chrono::microseconds(ma_poll_interval_us));
        }
    }

    void queue_and_wait(std::function<void()> fn) {
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
            if ((now - start) > std::chrono::microseconds(ma_exec_timeout_us)) {
                throw std::runtime_error("Command queue: exec wait timeout");
            }
            std::this_thread::sleep_for(std::chrono::microseconds(ma_poll_interval_us));
        }
        if (failed) {
            throw std::runtime_error("Command in queue threw an exception.");
        }
    }

    void PROC_exec_all() {
        std::function<void()> cmd;
        while(pop(cmd)) {
            cmd();
        }
    }

    void clear() { reset(); }
};