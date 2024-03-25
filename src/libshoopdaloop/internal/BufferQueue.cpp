#include "BufferQueue.h"
#include "WithCommandQueue.h"
#include "types.h"
#include <iostream>

template<typename SampleT>
BufferQueue<SampleT>::BufferQueue(std::shared_ptr<BufferPool> pool, uint32_t max_buffers) : pool(pool) {
    buffers = std::make_shared<std::deque<Buffer>>();
    ma_active_buffer_pos.store(pool->object_size()); // put at end of a virtual buffer, ensures new buffer will be created immediately
    ma_max_buffers.store(max_buffers);
}

template<typename SampleT>
uint32_t BufferQueue<SampleT>::n_samples() const {
    if (buffers->size() == 0) { return 0; }
    return (buffers->size() - 1) * pool->object_size() + ma_active_buffer_pos.load();
}

template<typename SampleT>
void BufferQueue<SampleT>::PROC_put(const SampleT *data, uint32_t length) {
    auto capacity = buffers->size() * pool->object_size();
    auto remaining = (size_t) length;

    while (remaining > 0) {
        auto space = pool->object_size() - ma_active_buffer_pos.load();
        if (space == 0) {
            log<log_level_debug_trace>("add buffer -> {}", buffers->size() + 1);
            buffers->push_back(Buffer(pool->get_object()));
            ma_active_buffer_pos.store(0);
            space = pool->object_size();
            while (buffers->size() > ma_max_buffers.load()) {
                log<log_level_debug_trace>("Drop buffer ({} > {})", buffers->size(), ma_max_buffers.load());
                buffers->pop_front(); // FIFO behavior
            }
        }
        auto to_copy = std::min(space, remaining);
        auto &back = buffers->back(); 
        memcpy((void*)&back->at(ma_active_buffer_pos.load()), (void*)data, to_copy * sizeof(SampleT));
        remaining -= to_copy;
        data += to_copy;
        ma_active_buffer_pos.fetch_add(to_copy);
    }
}

template<typename SampleT>
void BufferQueue<SampleT>::PROC_put(const std::initializer_list<SampleT>& list) {
    PROC_put(list.begin(), list.size());
}

template<typename SampleT>
typename BufferQueue<SampleT>::Snapshot BufferQueue<SampleT>::PROC_get() {
    Snapshot s;
    s.data = std::make_shared<std::vector<Buffer>>();
    *s.data = std::vector<Buffer>(buffers->begin(), buffers->end());
    s.n_samples = n_samples();
    return s;
}

template<typename SampleT>
void BufferQueue<SampleT>::set_max_buffers(uint32_t max_buffers) {
    log<log_level_debug_trace>("queue set max buffers -> {}", max_buffers);
    auto new_buffers =
        std::make_shared<std::deque<Buffer>>();
    WithCommandQueue::queue_process_thread_command([this, new_buffers, max_buffers]() {
        log<log_level_debug_trace>("set max buffers -> {}", max_buffers);
        buffers = new_buffers;
        ma_max_buffers.store(max_buffers);
        ma_active_buffer_pos.store(pool->object_size());
    });
}

template<typename SampleT>
void BufferQueue<SampleT>::set_min_n_samples(uint32_t n) {
    unsigned bufs = (n + single_buffer_size() - 1) / single_buffer_size(); // Ceil
    set_max_buffers(bufs);
}

template<typename SampleT>
unsigned BufferQueue<SampleT>::get_max_buffers() const {
    return ma_max_buffers.load();
}

template<typename SampleT>
uint32_t BufferQueue<SampleT>::single_buffer_size() const {
    return pool->object_size();
}

template<typename SampleT>
void BufferQueue<SampleT>::PROC_process() {
    WithCommandQueue::PROC_handle_command_queue();
}

template class BufferQueue<float>;
template class BufferQueue<int>;