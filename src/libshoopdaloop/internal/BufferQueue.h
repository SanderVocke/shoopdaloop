#pragma once
#include <deque>
#include <memory>
#include <vector>

#include "AudioBuffer.h"
#include "ObjectPool.h"
#include "WithCommandQueue.h"
#include "LoggingEnabled.h"
#include "shoop_shared_ptr.h"

template<typename SampleT>
struct BufferQueue : private WithCommandQueue,
                     private ModuleLoggingEnabled<"Backend.BufferQueue"> {
public:
    typedef AudioBuffer<SampleT> BufferObj;
    typedef ObjectPool<BufferObj> BufferPool;
    typedef shoop_shared_ptr<BufferObj> Buffer;

private:
    shoop_shared_ptr<std::deque<Buffer>> buffers;
    shoop_shared_ptr<BufferPool> pool = nullptr;
    std::atomic<uint32_t> ma_active_buffer_pos = 0;
    std::atomic<uint32_t> ma_max_buffers = 0;

public:
    BufferQueue(shoop_shared_ptr<BufferPool> pool, uint32_t max_buffers);

    uint32_t n_samples() const;
    uint32_t single_buffer_size() const;

    // Put extra data in. A buffer object will be added if needed.
    // If so, the back buffer may also be dropped from the queue
    // as a result.
    void PROC_put(const SampleT *data, uint32_t length);
    void PROC_put(const std::initializer_list<SampleT>& list);

    // The buffer objects are shared pointers.
    // This function returns a vector of them. The buffer queue won't overwrite
    // samples in the buffers, so the caller can safely read them.
    // Note though that the front buffer which is still being filled may have
    // vacant samples written after this function returns.
    struct Snapshot {
        shoop_shared_ptr<std::vector<Buffer>> data;
        uint32_t n_samples;
        uint32_t buffer_size;
    };
    Snapshot PROC_get();

    // Change the amount of buffers allowed in the queue.
    // This will replace the queue with an empty one which will need to be filled
    // again. Result will be seen after the next process thread cycle.
    void set_max_buffers(uint32_t max_buffers);
    unsigned get_max_buffers() const;

    // Helper to set the amount of samples this buffer queue should hold
    // at a minimum.
    void set_min_n_samples(uint32_t n);

    void PROC_process();
};

#ifndef IMPLEMENT_BUFFERQUEUE_H
extern template class BufferQueue<float>;
extern template class BufferQueue<int>;
#endif