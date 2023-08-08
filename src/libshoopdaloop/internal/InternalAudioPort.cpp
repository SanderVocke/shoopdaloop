#define IMPLEMENT_INTERNALAUDIOPORT_H
#include "InternalAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>

template class InternalAudioPort<float>;
template class InternalAudioPort<int>;

template <typename SampleT>
InternalAudioPort<SampleT>::InternalAudioPort(std::string name,
                                              PortDirection direction,
                                              size_t n_frames)
    : AudioPortInterface<SampleT>(name, direction), m_name(name),
      m_direction(direction), m_buffer(n_frames) {}

template <typename SampleT>
SampleT *InternalAudioPort<SampleT>::PROC_get_buffer(size_t n_frames, bool do_zero) {
    if (n_frames > m_buffer.size()) {
        throw std::runtime_error(
            "Requesting oversized buffer from internal port");
    }
    if (do_zero) {
        memset((void *)m_buffer.data(), 0, sizeof(float) * m_buffer.size());
    }
    return m_buffer.data();
}

template <typename SampleT>
const char *InternalAudioPort<SampleT>::name() const {
    return m_name.c_str();
}

template <typename SampleT>
PortDirection InternalAudioPort<SampleT>::direction() const {
    return m_direction;
}

template <typename SampleT>
void InternalAudioPort<SampleT>::reallocate_buffer(size_t n_frames) {
    m_buffer = std::vector<SampleT>(n_frames);
}

template <typename SampleT>
size_t InternalAudioPort<SampleT>::buffer_size() const {
    return m_buffer.size();
}

template <typename SampleT> void InternalAudioPort<SampleT>::close() {}

template <typename SampleT> void InternalAudioPort<SampleT>::zero() {
    memset((void *)m_buffer.data(), 0, sizeof(float) * m_buffer.size());
}