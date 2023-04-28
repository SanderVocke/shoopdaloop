#pragma once
#include "AudioSystemInterface.h"
#include "JackMidiPort.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include "JackAudioPort.h"
#include <jack/jack.h>
#include <jack/types.h>
#include <stdexcept>
#include <memory>

class JackAudioSystem : public AudioSystemInterface<jack_nframes_t, size_t> {
    jack_client_t * m_client;
    std::string m_client_name;
    size_t m_sample_rate;
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::function<void(size_t)> m_process_cb;

    static int PROC_process_cb_static (jack_nframes_t nframes,
                                  void *arg) {
        auto &inst = *((JackAudioSystem *)arg);
        return inst.PROC_process_cb_inst(nframes);
    }

    int PROC_process_cb_inst (jack_nframes_t nframes) {
        if (m_process_cb) {
            m_process_cb((size_t) nframes);
        }
        return 0;
    }

public:
    JackAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb
    ) : AudioSystemInterface(client_name, process_cb),
        m_client_name(client_name),
        m_process_cb(process_cb)
    {
        jack_status_t status;

        m_client = jack_client_open(
            client_name.c_str(),
            JackNullOption,
            &status);
        
        if (m_client == nullptr) {
            throw std::runtime_error("Unable to open JACK client.");
        }

        if (status && JackNameNotUnique) {
            m_client_name = std::string(jack_get_client_name(m_client));
        }

        jack_set_process_callback(m_client,
                                  JackAudioSystem::PROC_process_cb_static,
                                  (void*)this);
    }

    void start() override {
        if (jack_activate(m_client)) {
            throw std::runtime_error("Could not activate JACK client.");
        }
    }

    ~JackAudioSystem() override {
        close();
        m_client = nullptr;
    }

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override {
        std::shared_ptr<PortInterface> port =
            std::make_shared<JackAudioPort>(name, direction, m_client);
        m_ports[port->name()] = port;
        return std::dynamic_pointer_cast<AudioPortInterface<float>>(port);
    }

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override {
        std::shared_ptr<PortInterface> port =
            std::make_shared<JackMidiPort>(name, direction, m_client);
        m_ports[port->name()] = port;
        return std::dynamic_pointer_cast<MidiPortInterface>(port);
    }

    size_t get_sample_rate() const override {
        return jack_get_sample_rate(m_client);
    }

    size_t get_buffer_size() const override {
        return jack_get_buffer_size(m_client);
    }

    void* maybe_client_handle() const override {
        return (void*)m_client;
    }

    const char* client_name() const override {
        return m_client_name.c_str();
    }

    void close() override {
        if (m_client) {
            jack_client_close(m_client);
        }
    }
};