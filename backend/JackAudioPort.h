#pragma once
#include <jack/types.h>
#include <string>
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include <jack/jack.h>
#include <stdexcept>

class JackAudioPort : public AudioPortInterface<jack_default_audio_sample_t> {
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;

public:
    JackAudioPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client
    ) : AudioPortInterface<float>(name, direction),
        m_client(client),
        m_direction(direction) {

        m_port = jack_port_register(
            m_client,
            name.c_str(),
            JACK_DEFAULT_AUDIO_TYPE,
            direction == PortDirection::Input ? JackPortIsInput : JackPortIsOutput,
            0
        );

        if (m_port == nullptr) {
            throw std::runtime_error("Unable to open port.");
        }
        m_name = std::string(jack_port_name(m_port));
    }
    
    float *get_buffer(size_t n_frames) override {
        return (jack_default_audio_sample_t*) jack_port_get_buffer(m_port, n_frames);
    }

    std::string const& name() const override {
        return m_name;
    }

    PortDirection direction() const override {
        return m_direction;
    }

    void close() override {
        jack_port_unregister(m_client, m_port);
    }

    jack_port_t *get_jack_port() const {
        return m_port;
    }

    ~JackAudioPort() override {
        close();
    }
};