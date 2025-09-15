#pragma once
#include <type_traits>
#include "refilling_pool/src/cxx.rs.h"
#include <stddef.h>
#include <span>
#include <memory>
#include <stdexcept>
#include <stdint.h>

template<typename Elem> class BufferPool;

template<typename Elem> struct AudioBuffer {
    using RustBufferHandle = refilling_pool::BufferHandle;
    using RustBuffer = refilling_pool::Buffer;
    using RustBufferPool = refilling_pool::BufferPool;
    
    RustBufferHandle* buffer_handle;
    RustBuffer        buffer;
    std::weak_ptr<BufferPool<Elem>> pool;

    AudioBuffer(RustBufferHandle *handle,
                std::weak_ptr<BufferPool<Elem>> pool) :
        buffer_handle(handle),
        buffer(get_buffer_data(handle)),
        pool(pool) {
            std::fill(span().begin(), span().end(), Elem{0});
        }

    AudioBuffer(size_t n_elems) :
        buffer_handle(nullptr),
        buffer({(uint8_t*)malloc(n_elems * sizeof(Elem)), n_elems * sizeof(Elem)}),
        pool(std::weak_ptr<BufferPool<Elem>>()) {
            std::fill(span().begin(), span().end(), Elem{0});
        }

    size_t len() const {
        if (!buffer.ptr) { return 0; }
        return buffer.len / sizeof(Elem);
    }

    Elem* data() const {
        if (!buffer.ptr) { return nullptr; }
        return (Elem*) buffer.ptr;
    }

    Elem &at(size_t idx) const {
        if (!buffer.ptr) {
            throw std::runtime_error("no buffer handle set");
        }
        if (idx >= len()) {
            throw std::runtime_error("out of bounds");
        }
        return data()[idx];
    }

    std::span<Elem> span() const {
        if (!buffer.ptr) { return std::span<Elem>(); }
        return std::span<Elem>(data(), len());
    }

    ~AudioBuffer() {
        if (buffer_handle) {
            // Allocated from pool
            if (auto shared = pool.lock()) {
                shared->release_buffer(buffer_handle);
            }
        } else {
            // Allocated directly
            free(buffer.ptr);
        }
    }
};