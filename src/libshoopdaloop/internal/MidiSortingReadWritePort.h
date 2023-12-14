#pragma once
#include "MidiBufferInterfaces.h"
#include "MidiSortingBuffer.h"
#include "MidiPort.h"
#include <stdint.h>
#include <vector>

// A MIDI buffer that can written to and read from. It sorts messages
// during the process step.
class MidiSortingReadWritePort : public MidiPort {
    std::shared_ptr<MidiSortingBuffer> m_sorting_buffer;
public:
    MidiSortingReadWritePort(
        bool track_notes, bool track_controls, bool track_programs
    ) : MidiPort(track_notes, track_controls, track_programs),
        m_sorting_buffer(std::make_shared<MidiSortingBuffer>()) {}
    
    virtual ~MidiSortingReadWritePort() {}

    MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer (uint32_t nframes) override {
        return m_sorting_buffer.get();
    }
    MidiReadableBufferInterface *PROC_get_read_output_data_buffer (uint32_t nframes) override {
        return m_sorting_buffer.get();
    }
    MidiReadWriteBufferInterface *PROC_get_processing_buffer (uint32_t nframes) override {
        return m_sorting_buffer.get();
    }
};