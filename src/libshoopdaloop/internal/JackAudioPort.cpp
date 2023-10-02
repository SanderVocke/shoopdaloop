#include "JackAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

JackAudioPort::JackAudioPort(std::string name, PortDirection direction,
                             jack_client_t *client, std::shared_ptr<JackAllPorts> all_ports_tracker)
    : AudioPortInterface<float>(name, direction), JackPort(name, direction, PortType::Audio, client, all_ports_tracker) {}

float *JackAudioPort::PROC_get_buffer(size_t n_frames, bool do_zero) {
    auto rval =
        (jack_default_audio_sample_t *)jack_port_get_buffer(m_port, n_frames);
    if (do_zero) {
        memset((void *)rval, 0, sizeof(jack_default_audio_sample_t) * n_frames);
    }
    return rval;
}
