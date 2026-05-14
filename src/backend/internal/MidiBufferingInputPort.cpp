#include "MidiBufferingInputPort.h"

uint32_t MidiBufferingInputPort::n_events() const {
    return m_temp_midi_storage.size();
}

MidiStorageElem MidiBufferingInputPort::get_event(uint32_t idx) const {
    return m_temp_midi_storage[idx];
}

MidiBufferingInputPort::MidiBufferingInputPort(uint32_t reserve_size) : MidiPort(true, true, true) {
    m_temp_midi_storage.reserve(reserve_size);
}

MidiReadableBuffer *MidiBufferingInputPort::PROC_get_read_output_data_buffer (uint32_t nframes) {
    return static_cast<MidiReadableBuffer*>(this);
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
        auto n_events = inbuf->n_events();
        if (n_events > 0) {
            ModuleLoggingEnabled<"Backend.MidiBufferingInputPort">::log<log_level_debug>("Buffer {} input messages", n_events);
        }
        for(uint32_t i=0; i<n_events; i++) {
            auto event = inbuf->get_event(i);
            ModuleLoggingEnabled<"Backend.MidiBufferingInputPort">::log<log_level_debug>("Buffer message @ {}", event.proc_time);
            m_temp_midi_storage.push_back(event);
        }
    }

    MidiPort::PROC_process(nframes);
}