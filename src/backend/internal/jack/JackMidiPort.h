#pragma once
#include <atomic>
#include <jack/types.h>
#include "JackPort.h"
#include "../MidiBuffer.h"
#include "../MidiPort.h"
#include "../MidiSortingBuffer.h"
#include "../PortInterface.h"
#include "types.h"
#include "backend_rust/src/jack_api_cxx.rs.h"
#include <vector>

class JackMidiPort : public JackPort {
public:
    JackMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        uintptr_t client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        rust::Box<backend_rust::JackApiBridgeStrong> api
    ) : JackPort(name, direction, PortDataType::Midi, client, std::move(all_ports_tracker), std::move(api)) {}
};

class JackMidiInputPort :
    public JackMidiPort,
    private MidiPort,
    private MidiReadableBuffer
{
    std::vector<MidiStorageElem> m_messages;
    std::atomic<bool> m_muted = false;
    unsigned m_ringbuffer_n_samples = 0;
public:
    JackMidiInputPort(
        std::string name,
        uintptr_t client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        rust::Box<backend_rust::JackApiBridgeStrong> api
    );

    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;

    MidiReadableBuffer *PROC_internal_read_input_data_buffer (uint32_t nframes);
    MidiReadableBuffer *PROC_get_read_output_data_buffer(uint32_t nframes) override;
    MidiWriteableBuffer *PROC_internal_write_output_data_to_buffer (uint32_t nframes) { return nullptr; }
    MidiWriteableBuffer *PROC_get_write_data_into_port_buffer (uint32_t nframes) { return nullptr; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
    PortDataType type() const override { return PortDataType::Midi; }

    void set_muted(bool muted) override;
    bool get_muted() const override;
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
};

class JackMidiOutputPort :
    public JackMidiPort,
    private MidiPort,
    private MidiWriteableBuffer
{
    std::shared_ptr<MidiSortingBuffer> m_sorting_buffer = nullptr;
    std::atomic<bool> m_muted = false;
    unsigned m_ringbuffer_n_samples = 0;
public:
    JackMidiOutputPort(
        std::string name,
        uintptr_t client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        rust::Box<backend_rust::JackApiBridgeStrong> api
    );

    void write_event(MidiStorageElem event) override;

    MidiReadableBuffer *PROC_internal_read_input_data_buffer (uint32_t nframes) { return nullptr; }
    MidiWriteableBuffer *PROC_internal_write_output_data_to_buffer (uint32_t nframes);
    MidiWriteableBuffer *PROC_get_write_data_into_port_buffer (uint32_t nframes);

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
    PortDataType type() const override { return PortDataType::Midi; }

    void set_muted(bool muted) override;
    bool get_muted() const override;
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
};

using JackTestMidiInputPort = JackMidiInputPort;
using JackTestMidiOutputPort = JackMidiOutputPort;
