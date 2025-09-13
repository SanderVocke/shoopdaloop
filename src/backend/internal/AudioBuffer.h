#pragma once
#include <type_traits>
#include "refilling_pool/src/cxx.rs.h"
#include <stddef.h>
#include <span>
#include <memory>
#include <stdexcept>

template<typename Elem> class BufferPool;

template<typename Elem> struct AudioBuffer {
    using RustBufferHandle = refilling_pool::BufferHandle;
    using RustBuffer = refilling_pool::Buffer;
    using RustBufferPool = refilling_pool::BufferPool;
    
    RustBufferHandle* buffer_handle;
    RustBuffer        buffer;
    std::weak_ptr<BufferPool<Elem>> pool;

    AudioBuffer(RustBufferHandle *handle,
                std::weak_ptr<RustBufferPool> pool) :
        buffer_handle(handle),
        buffer(get_buffer_data(handle)),
        pool(pool) {}

    size_t len() const {
        if (!buffer_handle) { return 0; }
        return buffer.len;
    }

    Elem* data() const {
        if (!buffer_handle) { return nullptr; }
        return (Elem*) buffer.ptr;
    }

    Elem &at(size_t idx) const {
        if (!buffer_handle) {
            throw std::runtime_error("no buffer handle set");
        }
        if (idx >= len()) {
            throw std::runtime_error("out of bounds");
        }
        return data()[idx];
    }

    std::span<Elem> span() const {
        if (!buffer_handle) { return std::span<Elem>(); }
        return std::span<Elem>(data(), len());
    }

    ~AudioBuffer() {
        if (buffer_handle) {
            if (auto shared = pool.lock()) {
                shared->release_buffer(buffer_handle);
            }
        }
    }
};