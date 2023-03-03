#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <jack/jack.h>
#include <jack/midiport.h>
#include <stdexcept>
#include <vector>

class JackMidiPort : public MidiPortInterface {
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;
    static constexpr size_t n_event_storage = 1024;
    
    struct ReadMessage : public MidiSortableMessageInterface {
        jack_nframes_t    time;
	    size_t            size;
	    jack_midi_data_t *buffer;

        ReadMessage(jack_midi_event_t e) {
            time = e.time;
            size = e.size;
            buffer = e.buffer;
        }

        uint32_t get_time() const override {
            return time;
        }
        void get(uint32_t &size_out,
                 uint32_t &time_out,
                 const uint8_t* &data_out) const override {
            size_out = size;
            time_out = time;
            data_out = buffer;
        }
    };
    std::vector<ReadMessage> m_temp_midi_storage;

    friend class JackReadMidiBuf;
public:
    struct JackReadMidiBuf : public MidiReadableBufferInterface {
        void* jack_buf;
        JackMidiPort *port;

        JackReadMidiBuf(void* buf, JackMidiPort *port) : jack_buf(buf), port(port) {}

        size_t PROC_get_n_events() const override { return jack_midi_get_event_count(jack_buf); }
        MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) const override
        {
            jack_midi_event_t evt;
            jack_midi_event_get(&evt, jack_buf, idx);
            port->m_temp_midi_storage.push_back(ReadMessage(evt));
            return port->m_temp_midi_storage.back();
        }
    };

    struct JackWriteMidiBuf : public MidiWriteableBufferInterface {
        void* jack_buf;
        JackWriteMidiBuf(void* buf) : jack_buf(buf) {}

        void PROC_write_event_value(uint32_t size,
                         uint32_t time,
                         const uint8_t* data) override
        {
            jack_midi_event_write(jack_buf, time, (uint8_t*) data, size);
        }

        void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {
            throw std::runtime_error("Write by reference not supported");
        }

        bool write_by_reference_supported() const override { return false; }
        bool write_by_value_supported() const override { return true; }
    };

    JackMidiPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client
    ) : MidiPortInterface(name, direction),
        m_client(client),
        m_direction(direction) {

        m_temp_midi_storage.reserve(n_event_storage);

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

    const char* name() const override {
        return m_name.c_str();
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
        m_temp_midi_storage.clear();
        return std::make_unique<JackReadMidiBuf>(jack_port_get_buffer(m_port, n_frames), this);
    }

    std::unique_ptr<MidiWriteableBufferInterface> PROC_get_write_buffer (size_t n_frames) override {
        return std::make_unique<JackWriteMidiBuf>(jack_port_get_buffer(m_port, n_frames));
    }

    ~JackMidiPort() override {
        close();
    }
};