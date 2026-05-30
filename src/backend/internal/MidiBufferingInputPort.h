#pragma once
#include "MidiBuffer.h"
#include "MidiPort.h"
#include <stdint.h>
#include <vector>
#include "TracingRegistry.h"

// A MIDI buffer that can be read from. It buffers messages from another
// input source so that they can be passed on and sorted downstream.
class MidiBufferingInputPort : public MidiPort,
                               protected MidiReadableBuffer,
                               private ModuleLoggingEnabled<"Backend.MidiBufferingInputPort"> {
protected:
    std::vector<MidiStorageElem> m_temp_midi_storage;
    TracyPlotter m_plot_buffered_messages{"buffered_messages"};
public:

    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;
    
    MidiBufferingInputPort(uint32_t reserve_size=1024);
    virtual ~MidiBufferingInputPort() {}

    MidiReadableBuffer *PROC_get_read_output_data_buffer (uint32_t nframes) override;

    void PROC_process(uint32_t nframes) override;
    void PROC_prepare(uint32_t nframes) override;
};