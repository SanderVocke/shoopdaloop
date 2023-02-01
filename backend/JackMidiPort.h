#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <jack/jack.h>
#include <jack/midiport.h>
#include <stdexcept>

class JackMidiPort : public MidiPortInterface<jack_nframes_t, size_t> {
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;

public:
    JackMidiPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client
    ) : MidiPortInterface<jack_nframes_t, size_t>(name, direction),
        m_client(client),
        m_direction(direction) {

        m_port = jack_port_register(
            m_client,
            name.c_str(),
            JACK_DEFAULT_MIDI_TYPE,
            direction == PortDirection::Input ? JackPortIsInput : JackPortIsOutput,
            0
        );

        if (m_port == nullptr) {
            throw std::runtime_error("Unable to open port.");
        }
        m_name = std::string(jack_port_name(m_port));
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

    void* get_buffer(size_t n_frames) override {
        return jack_port_get_buffer(m_port, n_frames);
    }

    size_t get_n_events(void* buffer) const override {
        return jack_midi_get_event_count(buffer);
    }

    void get_event(void* buffer,
                    size_t idx,
                    size_t &size_out,
                    jack_nframes_t &time_out,
                    uint8_t* &data_out) const override {
        jack_midi_event_t evt;
        jack_midi_event_get(&evt, buffer, idx);
        size_out = evt.size;
        time_out = evt.time;
        data_out = evt.buffer;
    }

    ~JackMidiPort() override {
        close();
    }
};