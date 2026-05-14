#pragma once
#include <jack/types.h>
#include "JackTestApi.h"
#include "JackPort.h"
#include "MidiBuffer.h"
#include "MidiSortingBuffer.h"
#include "PortInterface.h"
#include "types.h"
#include <jack_wrappers.h>
#include <vector>
#include <atomic>

template<typename API>
class GenericJackMidiPort : public GenericJackPort<API> {
public:
    GenericJackMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackPort<API>(name, direction, PortDataType::Midi, client, all_ports_tracker) {}
};

template<typename API>
class GenericJackMidiInputPort : 
    public GenericJackMidiPort<API>,
    private MidiReadableBuffer
{
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;

    std::vector<MidiStorageElem> m_messages;
    std::atomic<bool> m_muted = false;
    unsigned m_ringbuffer_n_samples = 0;
public:
    GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    );

    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;

    MidiReadableBuffer *PROC_internal_read_input_data_buffer (uint32_t nframes);
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

template<typename API>
class GenericJackMidiOutputPort : 
    public GenericJackMidiPort<API>,
    private MidiWriteableBuffer
{
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;

    shoop_shared_ptr<MidiSortingBuffer> m_sorting_buffer = nullptr;
    std::atomic<bool> m_muted = false;
    unsigned m_ringbuffer_n_samples = 0;
public:
    GenericJackMidiOutputPort(
        std::string name,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
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

using JackMidiInputPort = GenericJackMidiInputPort<JackApi>;
using JackTestMidiInputPort = GenericJackMidiInputPort<JackTestApi>;
using JackMidiOutputPort = GenericJackMidiOutputPort<JackApi>;
using JackTestMidiOutputPort = GenericJackMidiOutputPort<JackTestApi>;

extern template class GenericJackMidiInputPort<JackApi>;
extern template class GenericJackMidiInputPort<JackTestApi>;
extern template class GenericJackMidiOutputPort<JackApi>;
extern template class GenericJackMidiOutputPort<JackTestApi>;