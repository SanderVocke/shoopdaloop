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

    class JackMidiReadBuffer : public MidiReadableBufferInterface {
    public:
        void* m_jack_buffer;

        JackMidiReadBuffer();
        
        bool read_by_reference_supported() const override;
        uint32_t PROC_get_n_events() const override;
        MidiSortableMessageInterface const& PROC_get_event_reference(uint32_t idx) override;
        void PROC_get_event_value(uint32_t idx,
                                uint32_t &size_out,
                                uint32_t &time_out,
                                const uint8_t* &data_out) override;
    };

    JackMidiReadBuffer m_read_buffer;
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

    class JackMidiWriteBuffer : public MidiWriteableBufferInterface {
    public:
        void* m_jack_buffer = nullptr;

        JackMidiWriteBuffer();
        
        bool write_by_reference_supported() const override;
        bool write_by_value_supported() const override;
        void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
        void PROC_write_event_value(uint32_t size,
                            uint32_t time,
                            const uint8_t* data) override;
    
    };

    JackMidiWriteBuffer m_write_buffer;
public:
    GenericJackMidiOutputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, Output, client, all_ports_tracker),
        MidiSortingReadWritePort(true, false, false),
        m_write_buffer(JackMidiWriteBuffer{}) {}

    MidiReadableBufferInterface *PROC_internal_read_input_data_buffer (uint32_t nframes) override { return nullptr; }
    MidiWriteableBufferInterface *PROC_internal_write_output_data_to_buffer (uint32_t nframes) override;

    PortDataType type() const override { return PortDataType::Midi; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;
};

using JackMidiInputPort = GenericJackMidiInputPort<JackApi>;
using JackTestMidiInputPort = GenericJackMidiInputPort<JackTestApi>;
using JackMidiOutputPort = GenericJackMidiOutputPort<JackApi>;
using JackTestMidiOutputPort = GenericJackMidiOutputPort<JackTestApi>;

extern template class GenericJackMidiInputPort<JackApi>;
extern template class GenericJackMidiInputPort<JackTestApi>;
extern template class GenericJackMidiOutputPort<JackApi>;
extern template class GenericJackMidiOutputPort<JackTestApi>;