#include "MidiBufferingInputPort.h"
#include "MidiBufferInterfaces.h"

uint32_t MidiBufferingInputPort::PROC_get_n_events() const {
    return m_temp_midi_storage.size();
}

MidiSortableMessageInterface const&
MidiBufferingInputPort::PROC_get_event_reference(uint32_t idx) {
    return m_temp_midi_storage[idx];
}

MidiBufferingInputPort::MidiBufferingInputPort(uint32_t reserve_size) : MidiPort(false, false, false) {
    m_temp_midi_storage.reserve(reserve_size);
}

MidiReadableBufferInterface *MidiBufferingInputPort::PROC_get_read_output_data_buffer (uint32_t nframes) {
    return static_cast<MidiReadableBufferInterface*>(this);
}

void MidiBufferingInputPort::PROC_process(uint32_t nframes) {
    // Copy data from the wrapped input to our buffer.
    auto inbuf = PROC_internal_read_input_data_buffer(nframes);
    for(uint32_t i=0; i<inbuf->PROC_get_n_events(); i++) {
        auto &msg = inbuf->PROC_get_event_reference(i);
        m_temp_midi_storage.push_back(ReadMessage(msg.get_time(), msg.get_size(), (void*)msg.get_data()));
    }
}