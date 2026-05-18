#include "MidiBufferingInputPort.h"

uint32_t MidiBufferingInputPort::n_events() const {
    return m_rust->n_events();
}

MidiStorageElem MidiBufferingInputPort::get_event(uint32_t idx) const {
    MidiStorageElem elem;
    elem.time = m_rust->get_event_time(idx);
    elem.size = m_rust->get_event_size(idx);
    
    if (elem.size > 0 && elem.size <= 4) {
        m_rust->get_event_bytes(idx, elem.bytes, 4);
    }
    
    return elem;
}

MidiBufferingInputPort::MidiBufferingInputPort(uint32_t reserve_size)
    : MidiPort(true, true, true),
      m_rust(backend_rust::new_midi_buffering_input_port(reserve_size))
{}

MidiReadableBuffer *MidiBufferingInputPort::PROC_get_read_output_data_buffer(uint32_t nframes) {
    return this;
}

IMidiReadableBuffer *MidiBufferingInputPort::get_readable_buffer() {
    return this;
}

void MidiBufferingInputPort::PROC_prepare(uint32_t nframes) {
    // Clear the Rust buffer
    m_rust->clear();

    MidiPort::PROC_prepare(nframes);
}

void MidiBufferingInputPort::PROC_process(uint32_t nframes) {
    // Copy data from the wrapped input to our Rust buffer.
    if (!m_rust->get_muted()) {
        auto inbuf = PROC_internal_read_input_data_buffer(nframes);
        auto n = inbuf->n_events();
        if (n > 0) {
            ModuleLoggingEnabled<"Backend.MidiBufferingInputPort">::log<log_level_debug>("Buffer {} input messages", n);
        }
        for (uint32_t i = 0; i < n; i++) {
            auto event = inbuf->get_event(i);
            ModuleLoggingEnabled<"Backend.MidiBufferingInputPort">::log<log_level_debug>("Buffer message @ {}", event.time);
            
            // Convert to fixed-size array for CXX bridge
            std::array<uint8_t, 4> data = {0, 0, 0, 0};
            std::memcpy(data.data(), event.bytes, std::min(static_cast<size_t>(event.size), sizeof(data)));
            m_rust->buffer_event_with_params(event.time, event.size, data);
        }
    }

    MidiPort::PROC_process(nframes);
}