#include "JackAudioPort.h"
#include <string>
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

JackAudioPort::JackAudioPort(std::string name, PortDirection direction,
                             jack_client_t *client)
    : AudioPortInterface<float>(name, direction), m_client(client),
      m_direction(direction) {

    m_port = jack_port_register(
        m_client, name.c_str(), JACK_DEFAULT_AUDIO_TYPE,
        direction == PortDirection::Input ? JackPortIsInput : JackPortIsOutput,
        0);

    if (m_port == nullptr) {
        throw std::runtime_error("Unable to open port.");
    }
    m_name = std::string(jack_port_name(m_port));
}

float *JackAudioPort::PROC_get_buffer(size_t n_frames, bool do_zero) {
    auto rval =
        (jack_default_audio_sample_t *)jack_port_get_buffer(m_port, n_frames);
    if (do_zero) {
        memset((void *)rval, 0, sizeof(jack_default_audio_sample_t) * n_frames);
    }
    return rval;
}

const char *JackAudioPort::name() const { return m_name.c_str(); }

PortDirection JackAudioPort::direction() const { return m_direction; }

void JackAudioPort::close() { jack_port_unregister(m_client, m_port); }

jack_port_t *JackAudioPort::get_jack_port() const { return m_port; }

JackAudioPort::~JackAudioPort() { close(); }