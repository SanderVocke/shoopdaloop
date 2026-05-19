#include "RustAudioPort.h"

#include <cstring>
#include <stdexcept>
#include <algorithm>

RustAudioPortF32::RustAudioPortF32()
{
    // Default constructor: use moderate ringbuffer settings
    size_t pool_capacity = 36;  // Some headroom
    size_t low_water_mark = 18;  // Half
    size_t buffer_size = 128;    // Default buffer size
    
    m_rust = backend_rust::new_audio_port(
        pool_capacity,
        low_water_mark,
        buffer_size,
        32  // max_buffers
    );
    
    m_buffer.resize(128);
    m_buffer_size = 128;
}

RustAudioPortF32::RustAudioPortF32(shoop_shared_ptr<UsedBufferPool> buffer_pool, uint32_t max_buffers)
{
    // Determine buffer size from pool
    size_t buffer_size = buffer_pool ? buffer_pool->elems_per_buffer() : 128;
    if (buffer_size == 0) {
        buffer_size = 128;
    }
    
    // Pool configuration: capacity = max_buffers + some headroom
    size_t pool_capacity = max_buffers + 4;
    size_t low_water_mark = pool_capacity / 2;
    
    m_rust = backend_rust::new_audio_port(
        pool_capacity,
        low_water_mark,
        buffer_size,
        max_buffers
    );
    
    // Initialize buffer with given buffer_size
    m_buffer.resize(buffer_size);
    m_buffer_size = static_cast<uint32_t>(buffer_size);
}

RustAudioPortF32::~RustAudioPortF32() = default;

float* RustAudioPortF32::PROC_get_buffer(uint32_t n_frames) {
    // Resize internal buffer if needed
    if (n_frames > m_buffer_size) {
        m_buffer.resize(n_frames);
        m_buffer_size = n_frames;
    }
    
    // Return pointer to buffer
    return m_buffer.data();
}

void RustAudioPortF32::PROC_process(uint32_t n_frames) {
    if (n_frames == 0 || m_buffer.empty()) {
        return;
    }
    
    // Verify m_rust is valid
    if (!m_rust.has_value()) {
        return;
    }
    
    // Call Rust process with our buffer pointer
    backend_rust::audio_port_process(**m_rust, m_buffer.data(), n_frames);
}

void RustAudioPortF32::set_gain(float gain) {
    backend_rust::audio_port_set_gain(**m_rust, gain);
}

float RustAudioPortF32::get_gain() const {
    return backend_rust::audio_port_get_gain(**m_rust);
}

void RustAudioPortF32::set_muted(bool muted) {
    backend_rust::audio_port_set_muted(**m_rust, muted);
}

bool RustAudioPortF32::get_muted() const {
    return backend_rust::audio_port_get_muted(**m_rust);
}

float RustAudioPortF32::get_input_peak() const {
    return backend_rust::audio_port_get_input_peak(**m_rust);
}

void RustAudioPortF32::reset_input_peak() {
    backend_rust::audio_port_reset_input_peak(**m_rust);
}

float RustAudioPortF32::get_output_peak() const {
    return backend_rust::audio_port_get_output_peak(**m_rust);
}

void RustAudioPortF32::reset_output_peak() {
    backend_rust::audio_port_reset_output_peak(**m_rust);
}

void RustAudioPortF32::set_ringbuffer_n_samples(unsigned n) {
    backend_rust::audio_port_set_ringbuffer_n_samples(**m_rust, n);
}

unsigned RustAudioPortF32::get_ringbuffer_n_samples() const {
    return backend_rust::audio_port_get_ringbuffer_n_samples(**m_rust);
}

RustAudioPortF32::RingbufferSnapshot RustAudioPortF32::PROC_get_ringbuffer_contents() {
    RingbufferSnapshot s;
    s.data = shoop_make_shared<std::vector<shoop_shared_ptr<BufferObj>>>();
    
    // Get snapshot from Rust - returns Vec<AudioBufferInfo>
    auto buffer_infos = backend_rust::audio_port_get_ringbuffer_contents(**m_rust);
    
    s.n_samples = backend_rust::audio_port_get_ringbuffer_n_samples(**m_rust);
    s.buffer_size = backend_rust::audio_port_get_single_buffer_size(**m_rust);
    
    // Process each buffer info
    for (const auto& info : buffer_infos) {
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