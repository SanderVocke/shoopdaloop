#include "InternalAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>

template <typename SampleT>
InternalAudioPort<SampleT>::InternalAudioPort(std::string name,
                                              shoop_port_direction_t direction,
                                              uint32_t n_frames)
    : AudioPort<SampleT>(), m_name(name),
      m_direction(direction), m_buffer(n_frames) {}

template <typename SampleT>
SampleT *InternalAudioPort<SampleT>::PROC_get_buffer(uint32_t n_frames) {
    if (n_frames > m_buffer.size()) {
        throw std::runtime_error(
            "Requesting oversized buffer from internal port");
    }
    return m_buffer.data();
}

template <typename SampleT>
const char *InternalAudioPort<SampleT>::name() const {
    return m_name.c_str();
}

template <typename SampleT>
PortExternalConnectionStatus InternalAudioPort<SampleT>::get_external_connection_status() const { return PortExternalConnectionStatus(); }

template <typename SampleT>
void InternalAudioPort<SampleT>::connect_external(std::string name) {}

template <typename SampleT>
void InternalAudioPort<SampleT>::disconnect_external(std::string name) {}

template <typename SampleT>
PortDataType InternalAudioPort<SampleT>::type() const { return PortDataType::Audio; }

template <typename SampleT>
void InternalAudioPort<SampleT>::reallocate_buffer(uint32_t n_frames) {
    m_buffer = std::vector<SampleT>(n_frames);
}

template <typename SampleT>
uint32_t InternalAudioPort<SampleT>::buffer_size() const {
    return m_buffer.size();
}

template <typename SampleT> void InternalAudioPort<SampleT>::close() {}

template <typename SampleT> void InternalAudioPort<SampleT>::zero() {
    memset((void *)m_buffer.data(), 0, sizeof(float) * m_buffer.size());
}

template <typename SampleT>
void* InternalAudioPort<SampleT>::maybe_driver_handle() const {
    return (void*) this;
}

template <typename SampleT>
void InternalAudioPort<SampleT>::PROC_change_buffer_size(uint32_t buffer_size) {
    if (m_buffer.size() < buffer_size) {
        reallocate_buffer(buffer_size);
    }
}

template class InternalAudioPort<float>;
template class InternalAudioPort<int>;