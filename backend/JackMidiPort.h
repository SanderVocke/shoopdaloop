#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <jack/jack.h>
#include <jack/midiport.h>
#include <stdexcept>

class JackMidiPort : public MidiPortInterface {
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;

public:
    struct JackReadMidiBuf : public MidiReadableBufferInterface {
        void* jack_buf;
        JackReadMidiBuf(void* buf) : jack_buf(buf) {}

        size_t PROC_get_n_events() const override { return jack_midi_get_event_count(jack_buf); }
        void PROC_get_event(size_t idx,
                       uint32_t &size_out,
                       uint32_t &time_out,
                       uint8_t* &data_out) const override
        {
            jack_midi_event_t evt;
            jack_midi_event_get(&evt, jack_buf, idx);
            size_out = evt.size;
            time_out = evt.time;
            data_out = (uint8_t*) evt.buffer;
        }
    };

    struct JackWriteMidiBuf : public MidiWriteableBufferInterface {
        void* jack_buf;
        JackWriteMidiBuf(void* buf) : jack_buf(buf) {}

        void PROC_write_event(uint32_t size,
                         uint32_t time,
                         uint8_t* data)
        {
            jack_midi_event_write(jack_buf, time, (uint8_t*) data, size);
        }
    };

    JackMidiPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client
    ) : MidiPortInterface(name, direction),
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

    std::unique_ptr<MidiReadableBufferInterface> PROC_get_read_buffer (size_t n_frames) override {
        return std::make_unique<JackReadMidiBuf>(jack_port_get_buffer(m_port, n_frames));
    }

    std::unique_ptr<MidiWriteableBufferInterface> PROC_get_write_buffer (size_t n_frames) override {
        return std::make_unique<JackWriteMidiBuf>(jack_port_get_buffer(m_port, n_frames));
    }

    ~JackMidiPort() override {
        close();
    }
};