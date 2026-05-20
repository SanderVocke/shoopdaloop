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
    auto buf = PROC_get_buffer(nframes);
    if (nframes == 0 || !buf) {
        return;
    }
    if (!m_rust.has_value()) {
        return;
    }
    backend_rust::audio_port_process(**m_rust, buf, nframes);
}

unsigned InternalAudioPort::input_connectability() const {
    return m_rust_internal->input_connectability();
}

unsigned InternalAudioPort::output_connectability() const {
    return m_rust_internal->output_connectability();
}