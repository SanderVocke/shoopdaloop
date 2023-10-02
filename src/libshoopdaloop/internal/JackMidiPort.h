#pragma once
#include <jack/types.h>
#include "MidiPortInterface.h"
#include "JackPort.h"
#include <jack_wrappers.h>
#include <vector>

class JackMidiPort :
    public MidiPortInterface,
    public MidiReadableBufferInterface,
    public MidiWriteableBufferInterface,
    public JackPort
{
    void * m_jack_read_buf;
    void * m_jack_write_buf;
    static constexpr size_t n_event_storage = 1024;
    
    struct ReadMessage : public MidiSortableMessageInterface {
        jack_nframes_t    time;
	    size_t            size;
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

    friend class JackReadMidiBuf;
public:

    size_t PROC_get_n_events() const override;
    MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) override;
    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    JackMidiPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker
    );

    MidiReadableBufferInterface &PROC_get_read_buffer (size_t n_frames) override;
    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override;
};