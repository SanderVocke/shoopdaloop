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
      m_name(name), 
      m_buffer(n_frames), 
      m_input_connectability(input_connectability), 
      m_output_connectability(output_connectability) {}

float* InternalAudioPort::PROC_get_buffer(uint32_t n_frames) {
    if (n_frames > m_buffer.size() || m_buffer.size() == 0) {
        m_buffer.resize(std::max(n_frames, (uint32_t)1));
    }
    return m_buffer.data();
}

const char* InternalAudioPort::name() const {
    return m_name.c_str();
}

PortExternalConnectionStatus InternalAudioPort::get_external_connection_status() const {
    return PortExternalConnectionStatus();
}

void InternalAudioPort::connect_external(std::string name) {
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

void InternalAudioPort::disconnect_external(std::string name) {
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
    PROC_get_buffer(nframes);
    memset((void*) m_buffer.data(), 0, nframes * sizeof(float));
}

void InternalAudioPort::PROC_process(uint32_t nframes) {
    // Call base class process
    RustAudioPortF32::PROC_process(nframes);
}

unsigned InternalAudioPort::input_connectability() const {
    return m_input_connectability;
}

unsigned InternalAudioPort::output_connectability() const {
    return m_output_connectability;
}