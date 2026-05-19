#pragma once

/**
 * RustAudioBufferQueueF32 - C++ wrapper around Rust AudioBufferQueue for f32 samples
 * 
 * This provides a drop-in replacement for the C++ BufferQueue<float> template,
 * delegating all operations to the Rust implementation via the CXX bridge.
 * 
 * Uses pre-allocated buffers from RefillingPool - no heap allocations on audio thread.
 * Snapshot returns Vec<BufferPtrInfo> for thin-copy semantics.
 */

#include "AudioBuffer.h"
#include "BufferPool.h"
#include "shoop_shared_ptr.h"
#include "backend_rust/src/audio_buffer_queue_cxx.rs.h"

#include <cstdint>
#include <cstring>
#include <optional>
#include <vector>
#include <algorithm>

// Forward declaration of the C++ BufferQueue (defined in BufferQueue.h)
template<typename SampleT>
struct BufferQueue;

// Forward declaration of RustAudioBufferQueueF32
class RustAudioBufferQueueF32;

// Use Rust-backed BufferQueue for f32, C++ version for other types
template<typename SampleT>
struct BufferQueueSelector {
    // Default: use original BufferQueue (defined in BufferQueue.h)
    using Type = BufferQueue<SampleT>;
};

// Specialization for float samples - uses Rust implementation
template<>
struct BufferQueueSelector<float> {
    using Type = RustAudioBufferQueueF32;
};

// Concrete Rust-backed BufferQueue for f32 samples
class RustAudioBufferQueueF32 {
public:
    using BufferObj = AudioBuffer<float>;
    using UsedBufferPool = BufferPool<float>;
    using SharedBuffer = shoop_shared_ptr<BufferObj>;

    struct Snapshot {
        shoop_shared_ptr<std::vector<SharedBuffer>> data;
        uint32_t n_samples;
        uint32_t buffer_size;
    };

private:
    // Rust implementation - wrapped in optional since rust::Box has no default constructor
    std::optional<rust::Box<backend_rust::AudioBufferQueue>> m_rust;
    uint32_t m_buffer_size;

public:
    // Constructor with pool - main constructor for real use
    RustAudioBufferQueueF32(shoop_shared_ptr<UsedBufferPool> pool, uint32_t max_buffers)
    {
        m_buffer_size = pool ? pool->elems_per_buffer() : 0;
        if (m_buffer_size == 0) {
            m_buffer_size = 128;
        }
        
        // Pool configuration: capacity = max_buffers + some headroom
        // low_water_mark = half of capacity
        size_t pool_capacity = max_buffers + 4;
        size_t low_water_mark = pool_capacity / 2;
        
        m_rust = backend_rust::new_audio_buffer_queue_f32(
            pool_capacity, 
            low_water_mark,
            m_buffer_size,
            max_buffers
        );
    }

    // Default constructor - for use when not using ringbuffer
    RustAudioBufferQueueF32()
    {
        m_buffer_size = 128;
        m_rust = backend_rust::new_audio_buffer_queue_f32(8, 4, m_buffer_size, 4);
    }

    uint32_t n_samples() const {
        return backend_rust::buffer_queue_f32_n_samples(**m_rust);
    }

    uint32_t single_buffer_size() const {
        return m_buffer_size;
    }

    void PROC_put(const float* data, uint32_t length) {
        if (length == 0) return;
        backend_rust::buffer_queue_f32_put(**m_rust, data, length);
    }

    void PROC_put(std::initializer_list<float> list) {
        // This is harder to bridge - for now delegate to pointer version
        // C++ only uses this in tests typically
    }

    Snapshot PROC_get() {
        Snapshot s;
        s.data = shoop_make_shared<std::vector<SharedBuffer>>();
        
        // Get snapshot - returns Vec<BufferPtrInfo> with raw pointers
        auto buffer_infos = backend_rust::buffer_queue_f32_snapshot(**m_rust);
        
        s.n_samples = backend_rust::buffer_queue_f32_n_samples(**m_rust);
        s.buffer_size = m_buffer_size;
        
        // Process each buffer info
        for (size_t i = 0; i < buffer_infos.size(); i++) {
            const auto& info = buffer_infos[i];
            
            // Allocate AudioBuffer of appropriate size
            auto ab = shoop_make_shared<BufferObj>(s.buffer_size);
            
            // Copy data from Rust buffer into AudioBuffer
            size_t to_copy = std::min(info.len, (size_t)s.buffer_size);
            if (info.data_ptr && ab->data() && to_copy > 0) {
                memcpy(ab->data(), info.data_ptr, to_copy * sizeof(float));
            }
            
            s.data->push_back(ab);
        }
        
        return s;
    }

    void set_max_buffers(uint32_t max_buffers) {
        backend_rust::buffer_queue_f32_set_max_buffers(**m_rust, max_buffers);
    }

    unsigned get_max_buffers() const {
        return backend_rust::buffer_queue_f32_get_max_buffers(**m_rust);
    }

    void set_min_n_samples(uint32_t n) {
        backend_rust::buffer_queue_f32_set_min_n_samples(**m_rust, n);
    }

    void PROC_process() {
        // No-op for Rust version - no command queue needed
    }
};

// Type alias for convenience
using RustAudioBufferQueue = RustAudioBufferQueueF32;