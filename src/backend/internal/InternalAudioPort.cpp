#include "InternalAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>
#include "types.h"

#ifdef _WIN32
#undef min
#undef max
#endif

InternalAudioPort::InternalAudioPort(std::string name,
                                     uint32_t n_frames,
                                     unsigned input_connectability,
                                     unsigned output_connectability,
                                     shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool)
    : RustAudioPortF32(buffer_pool, 32),
      m_rust_internal(backend_rust::new_internal_audio_port(name, n_frames, input_connectability, output_connectability)) {}

float* InternalAudioPort::PROC_get_buffer(uint32_t n_frames) {
    return m_rust_internal->proc_get_buffer(n_frames);
}

const char* InternalAudioPort::name() const {
    return m_rust_internal->name().data();
}

PortExternalConnectionStatus InternalAudioPort::get_external_connection_status() const {
    return PortExternalConnectionStatus();
}

void InternalAudioPort::connect_external(std::string name) {
    (void)name;
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

void InternalAudioPort::disconnect_external(std::string name) {
    (void)name;
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

PortDataType InternalAudioPort::type() const {
    return PortDataType::Audio;
}

void InternalAudioPort::close() {}

void* InternalAudioPort::maybe_driver_handle() const {
    return (void*) this;
}

void InternalAudioPort::PROC_prepare(uint32_t nframes) {
    m_rust_internal->proc_prepare(nframes);
}

void InternalAudioPort::PROC_process(uint32_t nframes) {
    m_rust_internal->proc_process(nframes);
}

unsigned InternalAudioPort::input_connectability() const {
    return m_rust_internal->input_connectability();
}

unsigned InternalAudioPort::output_connectability() const {
    return m_rust_internal->output_connectability();
}

// Gain/mute/peak control - delegate to Rust InternalAudioPort
void InternalAudioPort::set_gain(float gain) {
    m_rust_internal->set_gain(gain);
}

float InternalAudioPort::get_gain() const {
    return m_rust_internal->get_gain();
}


void InternalAudioPort::set_muted(bool muted) {
    m_rust_internal->set_muted(muted);
}

bool InternalAudioPort::get_muted() const {
    return m_rust_internal->get_muted();
}

float InternalAudioPort::get_input_peak() const {
    return m_rust_internal->get_input_peak();
}

void InternalAudioPort::reset_input_peak() {
    m_rust_internal->reset_input_peak();
}

float InternalAudioPort::get_output_peak() const {
    return m_rust_internal->get_output_peak();
}

void InternalAudioPort::reset_output_peak() {
    m_rust_internal->reset_output_peak();
}
// Ringbuffer config - delegate to Rust InternalAudioPort
void InternalAudioPort::set_ringbuffer_n_samples(unsigned n) {
    m_rust_internal->set_ringbuffer_n_samples(n);
}

unsigned InternalAudioPort::get_ringbuffer_n_samples() const {
    return m_rust_internal->get_ringbuffer_n_samples();
}

// Ringbuffer access - delegate to Rust InternalAudioPort
RustAudioPortF32::RingbufferSnapshot InternalAudioPort::PROC_get_ringbuffer_contents() {
    RingbufferSnapshot s;
    s.data = shoop_make_shared<std::vector<shoop_shared_ptr<BufferObj>>>();
    
    // Get snapshot from Rust
    auto buffer_infos_vec = backend_rust::take_snapshot(*m_rust_internal);
    auto& buffer_infos = *buffer_infos_vec;
    
    s.n_samples = m_rust_internal->get_ringbuffer_n_samples();
    s.buffer_size = m_rust_internal->get_ringbuffer_single_buffer_size();
    
    // Process each buffer info
    for (const auto& info : buffer_infos) {
        // Allocate AudioBuffer of appropriate size
        auto ab = shoop_make_shared<BufferObj>(info.len);
        
        // Copy data from Rust buffer into AudioBuffer
        size_t to_copy = std::min(info.len, (size_t)s.buffer_size);
        if (info.data_ptr && ab->data() && to_copy > 0) {
            memcpy(ab->data(), info.data_ptr, to_copy * sizeof(float));
        }
        
        s.data->push_back(ab);
    }
    
    // Free the Rust-allocated Vec<BufferInfo>
    delete buffer_infos_vec;
    
    return s;
}