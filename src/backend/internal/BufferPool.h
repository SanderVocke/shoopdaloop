#pragma once
#include <type_traits>
#include "refilling_pool/src/cxx.rs.h"
#include "AudioBuffer.h"
#include "shoop_shared_ptr.h"
#include <optional>

template<typename Elem>
class BufferPool : public shoop_enable_shared_from_this<BufferPool<Elem>> {
    ::rust::Box<refilling_pool::BufferPool> pool;
    size_t m_elems_per_buffer;
    
public:

    BufferPool(size_t capacity,
               size_t low_water_mark,
               size_t elems_per_buffer) :
        m_elems_per_buffer(elems_per_buffer),
        pool(refilling_pool::new_buffer_pool(
            capacity,
            low_water_mark,
            elems_per_buffer * sizeof(Elem)
        ))
    {}

    shoop_shared_ptr<AudioBuffer<Elem>> get_shared_buffer() {
        auto handle = pool->get_buffer();
        return shoop_make_shared<AudioBuffer<Elem>>(handle, this->shared_from_this());
    }

    AudioBuffer<Elem> get_buffer() {
        auto handle = pool->get_buffer();
        return AudioBuffer<Elem>(handle, this->shared_from_this());
    }

    size_t elems_per_buffer() const {
        return m_elems_per_buffer;
    }
};