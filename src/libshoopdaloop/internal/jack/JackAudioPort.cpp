#include "JackAudioPort.h"
#include <string>
#include "JackPort.h"
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

template<typename API>
GenericJackAudioPort<API>::GenericJackAudioPort(std::string name, shoop_port_direction_t direction,
                             jack_client_t *client, std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker)
    : AudioPort<float>(name, direction), GenericJackPort<API>(name, direction, PortType::Audio, client, all_ports_tracker) {}

template<typename API>
void GenericJackAudioPort<API>::PROC_prepare(uint32_t nframes) {
    AudioPort<SampleT>::PROC_prepare();
    GenericJackPort<API>::PROC_prepare();
    auto buf = (jack_default_audio_sample_t *)API::port_get_buffer(m_port, nframes);
    if (output_connectivity() == PortConnectivityType::External) {
        // JACK output buffers should be zero'd
        memset((void*) buf, 0, sizeof(jack_default_audio_sample_t) * nframes);
    }
    ma_buffer = buf;
}

template<typename API>
float *GenericJackAudioPort<API>::PROC_get_buffer(uint32_t n_frames) const {
    return ma_buffer.load();
}

template<typename API>
void GenericJackAudioPort<API>::PROC_process() {
    AudioPort<SampleT>::PROC_process();
    GenericJackPort<API>::PROC_process();
}

template class GenericJackAudioPort<JackApi>;
template class GenericJackAudioPort<JackTestApi>;