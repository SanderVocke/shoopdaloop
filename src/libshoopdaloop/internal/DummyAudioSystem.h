#pragma once
#include "AudioSystemInterface.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
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
        PortDirection direction);
    
    float *PROC_get_buffer(size_t n_frames) override;
    const char* name() const override;
    PortDirection direction() const override;
    void close() override;
    ~DummyAudioPort() override;
};

class DummyMidiPort : public MidiPortInterface, public MidiReadableBufferInterface, public MidiWriteableBufferInterface {
    std::string m_name;
    PortDirection m_direction;

public:

    size_t PROC_get_n_events() const override;
    virtual MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) override;
    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    DummyMidiPort(
        std::string name,
        PortDirection direction
    );

    const char* name() const override;

    PortDirection direction() const override;

    void close() override;

    MidiReadableBufferInterface &PROC_get_read_buffer (size_t n_frames) override;

    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override;

    ~DummyMidiPort() override;
};

template<typename Time, typename Size>
class DummyAudioSystem : public AudioSystemInterface<Time, Size>, private ModuleLoggingEnabled {

    std::string log_module_name() const override;

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
    );

    void start() override;

    ~DummyAudioSystem() override;

    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    size_t get_sample_rate() const override;
    size_t get_buffer_size() const override;
    void* maybe_client_handle() const override;
    const char* client_name() const override;
    void close() override;

    size_t get_xruns() const override;
    void reset_xruns() override;
};

#ifndef IMPLEMENT_DUMMYAUDIOSYSTEM_H
extern template class DummyAudioSystem<uint32_t, uint16_t>;
extern template class DummyAudioSystem<uint32_t, uint32_t>;
extern template class DummyAudioSystem<uint16_t, uint16_t>;
extern template class DummyAudioSystem<uint16_t, uint32_t>;
extern template class DummyAudioSystem<uint32_t, uint64_t>;
#endif