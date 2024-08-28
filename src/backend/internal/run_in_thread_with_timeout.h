#pragma once

#include <future>
#include <chrono>
#include <functional>
#include <stdexcept>


// Note: "unsafe" is in the name because detaching a thread during program termination
// might not clean up resources properly.
inline void run_in_thread_with_timeout_unsafe(std::function<void()> fn, size_t timeout_ms) {
    std::promise<void> promise;
    std::future<void> future = promise.get_future();

    // Creating a thread to execute the function
    std::thread thread([&]() {
        fn();
        promise.set_value(); // Signal completion
    });

    // Wait for either the function to finish or the timeout to occur
    if (future.wait_for(std::chrono::milliseconds(timeout_ms)) == std::future_status::timeout) {
        // If the function call takes longer than the timeout, detach the thread
        thread.detach();
        throw std::runtime_error("Execution timed out");
    } else {
        thread.join();
    }

    return;
}
