#include "JackAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

template class GenericJackAudioPort<JackApi>;
template class GenericJackAudioPort<JackTestApi>;

template<typename API>
GenericJackAudioPort<API>::GenericJackAudioPort(std::string name, PortDirection direction,
                             jack_client_t *client, std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker)
    : AudioPortInterface<float>(name, direction), GenericJackPort<API>(name, direction, PortType::Audio, client, all_ports_tracker) {}

template<typename API>
float *GenericJackAudioPort<API>::PROC_get_buffer(size_t n_frames, bool do_zero) {
    auto rval =
        (jack_default_audio_sample_t *)API::port_get_buffer(m_port, n_frames);
    if (do_zero) {
        memset((void *)rval, 0, sizeof(jack_default_audio_sample_t) * n_frames);
    }
    return rval;
}
