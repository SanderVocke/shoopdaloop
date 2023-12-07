#pragma once
#include "MidiBufferInterfaces.h"
#include "MidiPort.h"
#include <stdint.h>
#include <vector>

// A MIDI buffer that can be read from. It buffers messages from another
// input source so that they can be passed on as references and sorted
// downstream.
class MidiBufferingInputPort : public MidiPort,
                               protected MidiReadableBufferInterface {
protected:
    struct ReadMessage : public MidiSortableMessageInterface {
        uint32_t time;
	    uint32_t size;
	    void *buffer;

        ReadMessage(uint32_t t, uint32_t s, void* b) :
            time(t), size(s), buffer(b) {}

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
    MidiSortableMessageInterface const& PROC_get_event_reference(uint32_t idx) override;
    
    MidiBufferingInputPort(uint32_t reserve_size=1024);
    virtual ~MidiBufferingInputPort() {}

    MidiReadableBufferInterface *PROC_get_read_output_data_buffer (uint32_t nframes) override;
    MidiReadWriteBufferInterface *PROC_get_processing_buffer (uint32_t nframes) override { return nullptr; }
    void PROC_process(uint32_t nframes) override;
};