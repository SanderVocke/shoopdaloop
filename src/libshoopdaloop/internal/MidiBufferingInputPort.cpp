#include "MidiBufferingInputPort.h"
#include "MidiBufferInterfaces.h"

uint32_t MidiBufferingInputPort::PROC_get_n_events() const {
    return m_temp_midi_storage.size();
}

MidiSortableMessageInterface const&
MidiBufferingInputPort::PROC_get_event_reference(uint32_t idx) {
    return m_temp_midi_storage[idx];
}

MidiBufferingInputPort::MidiBufferingInputPort(uint32_t reserve_size) : MidiPort(true, false, false) {
    m_temp_midi_storage.reserve(reserve_size);
}

MidiReadableBufferInterface *MidiBufferingInputPort::PROC_get_read_output_data_buffer (uint32_t nframes) {
    return static_cast<MidiReadableBufferInterface*>(this);
}

bool MidiBufferingInputPort::read_by_reference_supported() const { return true; }

void MidiBufferingInputPort::PROC_get_event_value(uint32_t idx,
                                                  uint32_t &size_out,
                                                  uint32_t &time_out,
                                                  const uint8_t* &data_out) {
    auto &msg = PROC_get_event_reference(idx);
    size_out = msg.get_size();
    time_out = msg.get_time();
    data_out = msg.get_data();
}

void MidiBufferingInputPort::PROC_prepare(uint32_t nframes) {
    // Clear the internal buffer
    m_temp_midi_storage.clear();

    MidiPort::PROC_prepare(nframes);
}

void MidiBufferingInputPort::PROC_process(uint32_t nframes) {
    // Copy data from the wrapped input to our buffer.
    if (!get_muted()) {
        auto inbuf = PROC_internal_read_input_data_buffer(nframes);
        bool use_references = inbuf->read_by_reference_supported();
        for(uint32_t i=0; i<inbuf->PROC_get_n_events(); i++) {
            if (use_references) {
                auto &msg = inbuf->PROC_get_event_reference(i);
                m_temp_midi_storage.push_back(ReadMessage(msg.get_time(), msg.get_size(), (void*)msg.get_data()));
            } else {
                uint32_t size, time;
                const uint8_t *data;
                inbuf->PROC_get_event_value(i, size, time, data);
                m_temp_midi_storage.push_back(ReadMessage(time, size, (void*)data));
            }
        }
    }

    MidiPort::PROC_process(nframes);
}