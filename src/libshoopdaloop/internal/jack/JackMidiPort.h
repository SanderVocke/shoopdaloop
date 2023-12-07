#pragma once
#include <jack/types.h>
#include "JackTestApi.h"
#include "MidiPortInterface.h"
#include "JackPort.h"
#include <jack_wrappers.h>
#include <vector>

template<typename API>
class GenericJackMidiPort :
    public virtual MidiPortInterface,
    public MidiReadableBufferInterface,
    public MidiWriteableBufferInterface,
    public GenericJackPort<API>
{
    using GenericJackPort<API>::m_port;
    
    void * m_jack_read_buf;
    void * m_jack_write_buf;
    static constexpr uint32_t n_event_storage = 1024;
    
    struct ReadMessage : public MidiSortableMessageInterface {
        jack_nframes_t    time;
	    uint32_t            size;
	    jack_midi_data_t *buffer;

        ReadMessage(jack_midi_event_t e);

        uint32_t get_time() const override;
        uint32_t get_size() const override;
        const uint8_t *get_data() const override;
        void get(uint32_t &size_out,
                 uint32_t &time_out,
                 const uint8_t* &data_out) const override;
    };
    std::vector<ReadMessage> m_temp_midi_storage;
public:

    uint32_t PROC_get_n_events() const override;
    MidiSortableMessageInterface &PROC_get_event_reference(uint32_t idx) override;
    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    GenericJackMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    );

    MidiReadableBufferInterface &PROC_get_read_buffer (uint32_t n_frames) override;
    MidiWriteableBufferInterface &PROC_get_write_buffer (uint32_t n_frames) override;
};

using JackMidiPort = GenericJackMidiPort<JackApi>;
using JackTestMidiPort = GenericJackMidiPort<JackTestApi>;

extern template class GenericJackMidiPort<JackApi>;
extern template class GenericJackMidiPort<JackTestApi>;