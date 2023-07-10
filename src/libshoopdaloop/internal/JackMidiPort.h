#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>
#include <vector>

class JackMidiPort : public MidiPortInterface, public MidiReadableBufferInterface, public MidiWriteableBufferInterface {
    jack_port_t* m_port;
    jack_client_t* m_client;
    std::string m_name;
    PortDirection m_direction;
    void * m_jack_read_buf;
    void * m_jack_write_buf;
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
        uint32_t get_size() const override {
            return size;
        }
        const uint8_t *get_data() const override {
            return (const uint8_t*)buffer;
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

    size_t PROC_get_n_events() const override { return jack_midi_get_event_count(m_jack_read_buf); }
    
    MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) override
    {
        jack_midi_event_t evt;
        jack_midi_event_get(&evt, m_jack_read_buf, idx);
        m_temp_midi_storage.push_back(ReadMessage(evt));
        return m_temp_midi_storage.back();
    }

    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override
    {
        jack_midi_event_write(m_jack_write_buf, time, (uint8_t*) data, size);
    }

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {
        throw std::runtime_error("Write by reference not supported");
    }

    bool write_by_reference_supported() const override { return false; }
    bool write_by_value_supported() const override { return true; }

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

    MidiReadableBufferInterface &PROC_get_read_buffer (size_t n_frames) override {
        m_temp_midi_storage.clear();
        m_jack_read_buf = jack_port_get_buffer(m_port, n_frames);
        return *(static_cast<MidiReadableBufferInterface*>(this));
    }

    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override {
        m_jack_write_buf = jack_port_get_buffer(m_port, n_frames);
        jack_midi_clear_buffer(m_jack_write_buf);
        return *(static_cast<MidiWriteableBufferInterface*>(this));
    }

    ~JackMidiPort() override {
        close();
    }
};