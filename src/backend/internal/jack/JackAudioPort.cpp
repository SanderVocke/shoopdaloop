#include "JackAudioPort.h"
#include <string>
#include "JackPort.h"
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

#ifdef _WIN32
#undef min
#undef max
#endif

template<typename API>
GenericJackAudioPort<API>::GenericJackAudioPort(std::string name, shoop_port_direction_t direction,
                             jack_client_t *client, shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker,
                             shoop_shared_ptr<typename AudioPort<jack_default_audio_sample_t>::UsedBufferPool> buffer_pool)
    : AudioPort<float>(buffer_pool), GenericJackPort<API>(name, direction, PortDataType::Audio, client, all_ports_tracker) {}

template<typename API>
void GenericJackAudioPort<API>::PROC_prepare(uint32_t nframes) {
    GenericJackPort<API>::PROC_prepare(nframes);
    auto buf = m_buffer.load();
    if (!buf) {
        // If JACK fails to give us a buffer, provide an internall fallback.
        m_fallback_buffer.resize(std::max(nframes, (uint32_t) m_fallback_buffer.size()));
        m_buffer = (void*)m_fallback_buffer.data();
    }
    if (!has_implicit_input_source()) {
        // JACK output buffers should be zero'd
        memset((void*) m_buffer.load(), 0, sizeof(jack_default_audio_sample_t) * nframes);
    }
}

template<typename API>
float *GenericJackAudioPort<API>::PROC_get_buffer(uint32_t n_frames) {
    auto rval = (float*) m_buffer.load();
    if (!rval) {
        if(m_fallback_buffer.size() < std::max(n_frames, (uint32_t)1)) {
            m_fallback_buffer.resize(std::max(n_frames, (uint32_t)1));
        }
        rval = m_fallback_buffer.data();
    }
    return rval;
}

template<typename API>
void GenericJackAudioPort<API>::PROC_process(uint32_t nframes) {
    AudioPort<jack_default_audio_sample_t>::PROC_process(nframes);
}

template<typename API>
unsigned GenericJackAudioPort<API>::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

template<typename API>
unsigned GenericJackAudioPort<API>::output_connectability() const {
    return (m_direction == ShoopPortDirection_Output) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

template class GenericJackAudioPort<JackApi>;
template class GenericJackAudioPort<JackTestApi>;