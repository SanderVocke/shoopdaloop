#pragma once
#include <jack/types.h>
#include "JackTestApi.h"
#include "MidiBufferingInputPort.h"
#include "JackPort.h"
#include "MidiSortingReadWritePort.h"
#include "PortInterface.h"
#include "types.h"
#include <jack_wrappers.h>
#include <vector>

template<typename API>
class GenericJackMidiPort : public GenericJackPort<API> {
public:
    GenericJackMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackPort<API>(name, direction, PortDataType::Midi, client, all_ports_tracker) {}
};

template<typename API>
class GenericJackMidiInputPort : 
    public virtual MidiBufferingInputPort,
    public GenericJackMidiPort<API>
{
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;
public:
    GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    );

    MidiReadableBufferInterface *PROC_internal_read_input_data_buffer (uint32_t nframes) override;
    MidiWriteableBufferInterface *PROC_internal_write_output_data_to_buffer (uint32_t nframes) override { return nullptr; }
    MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer (uint32_t nframes) override { return nullptr; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;
};

template<typename API>
class GenericJackMidiOutputPort : 
    public virtual MidiSortingReadWritePort,
    public GenericJackMidiPort<API>
{
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;
public:
    GenericJackMidiOutputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, Output, client, all_ports_tracker),
        MidiSortingReadWritePort(false, false, false) {}

    MidiReadableBufferInterface *PROC_internal_read_input_data_buffer (uint32_t nframes) override { return nullptr; }
    MidiWriteableBufferInterface *PROC_internal_write_output_data_to_buffer (uint32_t nframes) override;

    void PROC_prepare(uint32_t nframes) override;

    PortDataType type() const override { return PortDataType::Midi; }
    void PROC_process(uint32_t nframes) override {}
};

using JackMidiInputPort = GenericJackMidiInputPort<JackApi>;
using JackTestMidiInputPort = GenericJackMidiInputPort<JackTestApi>;
using JackMidiOutputPort = GenericJackMidiOutputPort<JackApi>;
using JackTestMidiOutputPort = GenericJackMidiOutputPort<JackTestApi>;

extern template class GenericJackMidiInputPort<JackApi>;
extern template class GenericJackMidiInputPort<JackTestApi>;
extern template class GenericJackMidiOutputPort<JackApi>;
extern template class GenericJackMidiOutputPort<JackTestApi>;