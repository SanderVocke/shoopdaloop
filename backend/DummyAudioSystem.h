#pragma once
#include "AudioSystemInterface.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include "types.h"
#include <chrono>
#include <ratio>
#include <stdexcept>
#include <memory>
#include <iostream>
#include <memory>
#include <set>
#include <thread>
#include <cstring>

class DummyAudioPort : public AudioPortInterface<audio_sample_t> {
    std::string m_name;
    PortDirection m_direction;
public:
    DummyAudioPort(
        std::string name,
        PortDirection direction) : AudioPortInterface<audio_sample_t>(name, direction),
            m_name(name),
            m_direction(direction) {}
    
    float *PROC_get_buffer(size_t n_frames) override {
        auto rval = (audio_sample_t*) malloc(n_frames * sizeof(audio_sample_t));
        memset((void*)rval, 0, sizeof(audio_sample_t) * n_frames);
        return rval;
    }

    std::string name() const override {
        return m_name;
    }

    PortDirection direction() const override {
        return m_direction;
    }

    void close() override {}

    void *get_jack_port() const {
        return nullptr;
    }

    ~DummyAudioPort() override {
        close();
    }
};

class DummyMidiPort : public MidiPortInterface {
    std::string m_name;
    PortDirection m_direction;

public:
    struct DummyMidiReadBuf : public MidiReadableBufferInterface {
        void* buf;
        DummyMidiReadBuf(void* buf) : buf(buf) {}

        size_t PROC_get_n_events() const override { return 0; }
        void PROC_get_event(size_t idx,
                       uint32_t &size_out,
                       uint32_t &time_out,
                       uint8_t* &data_out) const override
        {
            throw std::runtime_error("Qt proxy midi port cannot read messages");
        }
    };

    struct DummyMidiWriteBuf : public MidiWriteableBufferInterface {
        void* buf;
        DummyMidiWriteBuf(void* buf) : buf(buf) {}

        void PROC_write_event(uint32_t size,
                         uint32_t time,
                         uint8_t* data)
        {}
    };

    DummyMidiPort(
        std::string name,
        PortDirection direction
    ) : MidiPortInterface(name, direction),
        m_direction(direction), m_name(name) {}

    std::string name() const override {
        return m_name;
    }

    PortDirection direction() const override {
        return m_direction;
    }

    void close() override {}

    void *get_jack_port() const {
        return nullptr;
    }

    std::unique_ptr<MidiReadableBufferInterface> PROC_get_read_buffer (size_t n_frames) override {
        return std::make_unique<DummyMidiReadBuf>(nullptr);
    }

    std::unique_ptr<MidiWriteableBufferInterface> PROC_get_write_buffer (size_t n_frames) override {
        return std::make_unique<DummyMidiWriteBuf>(nullptr);
    }

    ~DummyMidiPort() override {
        close();
    }
};

template<typename Time, typename Size>
class DummyAudioSystem : public AudioSystemInterface<Time, Size> {

    std::function<void(size_t)> m_process_cb;
    const size_t mc_sample_rate;
    const size_t mc_buffer_size;
    std::atomic<bool> m_finish;
    std::thread m_proc_thread;
    std::set<std::shared_ptr<DummyAudioPort>> m_audio_ports;
    std::set<std::shared_ptr<DummyMidiPort>> m_midi_ports;
    std::string m_client_name;

    std::function<void(std::string, PortDirection)> m_audio_port_opened_cb, m_midi_port_opened_cb;
    std::function<void(std::string)> m_audio_port_closed_cb, m_midi_port_closed_cb;

public:
    DummyAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb
    ) : AudioSystemInterface<Time, Size>(client_name, process_cb),
        m_process_cb(process_cb),
        mc_buffer_size(1024),
        mc_sample_rate(48000),
        m_finish(false),
        m_client_name(client_name),
        m_audio_port_closed_cb(nullptr),
        m_audio_port_opened_cb(nullptr),
        m_midi_port_closed_cb(nullptr),
        m_midi_port_opened_cb(nullptr)
    {
        m_audio_ports.clear();
        m_midi_ports.clear();
    }

    void start() override {
        m_proc_thread = std::thread([this]{
            std::cout << "DummyAudioSystem: starting process thread" << std::endl;
            auto bufs_per_second = mc_sample_rate / mc_buffer_size;
            auto interval = 1.0f / ((float)bufs_per_second);
            auto micros = size_t(interval * 1000000.0f);
            while(!this->m_finish) {
                std::this_thread::sleep_for(std::chrono::microseconds(micros));
                m_process_cb(mc_buffer_size);
            }
            std::cout << "DummyAudioSystem: ending process thread" << std::endl;
        });
    }

    ~DummyAudioSystem() override {
        close();
    }

    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override {
        auto rval = std::make_shared<DummyAudioPort>(name, direction);
        m_audio_ports.insert(rval);
        return rval;
    }

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override {
        auto rval = std::make_shared<DummyMidiPort>(name, direction);
        m_midi_ports.insert(rval);
        return rval;
    }

    size_t get_sample_rate() const override {
        return mc_sample_rate;
    }

    void* maybe_client_handle() const override {
        return (void*)this;
    }

    const char* client_name() const override {
        return m_client_name.c_str();
    }

    void close() override {
        m_finish = true;
        if (m_proc_thread.joinable()) {
            m_proc_thread.join();
        }
    }
};