#include "InternalAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>
#include "types.h"

template <typename SampleT>
InternalAudioPort<SampleT>::InternalAudioPort(std::string name,
                                              uint32_t n_frames)
    : AudioPort<SampleT>(), m_name(name), m_buffer(n_frames) {}

template <typename SampleT>
SampleT *InternalAudioPort<SampleT>::PROC_get_buffer(uint32_t n_frames) {
    if (n_frames > m_buffer.size() || m_buffer.size() == 0) {
        m_buffer.resize(std::max(n_frames, (uint32_t)1));
    }
    return m_buffer.data();
}

template <typename SampleT>
const char *InternalAudioPort<SampleT>::name() const {
    return m_name.c_str();
}

template <typename SampleT>
PortExternalConnectionStatus InternalAudioPort<SampleT>::get_external_connection_status() const {
    return PortExternalConnectionStatus();
}

template <typename SampleT>
void InternalAudioPort<SampleT>::connect_external(std::string name) {
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

template <typename SampleT>
void InternalAudioPort<SampleT>::disconnect_external(std::string name) {
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

template <typename SampleT>
PortDataType InternalAudioPort<SampleT>::type() const {
    return PortDataType::Audio;
}

template <typename SampleT> void InternalAudioPort<SampleT>::close() {}

template <typename SampleT>
void* InternalAudioPort<SampleT>::maybe_driver_handle() const {
    return (void*) this;
}

template <typename SampleT>
void InternalAudioPort<SampleT>::PROC_prepare(uint32_t nframes) {
    PROC_get_buffer(nframes);
    memset((void*) m_buffer.data(), 0, nframes * sizeof(SampleT));
}

template <typename SampleT>
void InternalAudioPort<SampleT>::PROC_process(uint32_t nframes) {
    AudioPort<SampleT>::PROC_process(nframes);
}

template <typename SampleT>
unsigned InternalAudioPort<SampleT>::input_connectability() const {
    return ShoopPortConnectability_Internal;
}

template <typename SampleT>
unsigned InternalAudioPort<SampleT>::output_connectability() const {
    return ShoopPortConnectability_Internal;
}

template class InternalAudioPort<float>;
template class InternalAudioPort<int>;