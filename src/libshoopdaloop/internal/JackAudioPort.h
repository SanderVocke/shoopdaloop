#pragma once
#include <jack/types.h>
#include <string>
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <cstring>
#include <iostream>

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
    );
    
    float *PROC_get_buffer(size_t n_frames) override;
    const char* name() const override;
    PortDirection direction() const override;
    void close() override;
    jack_port_t *get_jack_port() const;
    ~JackAudioPort() override;
};