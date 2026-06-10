#pragma once

#include "MidiPort.h"
#include "MidiBuffer.h"

inline void midi_port_prepare(MidiPort &port, uint32_t nframes) {
    port.PROC_prepare(nframes);
}

inline void midi_port_process(MidiPort &port, uint32_t nframes) {
    port.PROC_process(nframes);
}

inline void midi_port_close(MidiPort &port) {
    port.close();
}

inline size_t midi_port_get_read_output_data_buffer(MidiPort &port, uint32_t nframes) {
    return reinterpret_cast<size_t>(port.PROC_get_read_output_data_buffer(nframes));
}

inline size_t midi_port_get_write_data_into_port_buffer(MidiPort &port, uint32_t nframes) {
    return reinterpret_cast<size_t>(port.PROC_get_write_data_into_port_buffer(nframes));
}

inline uint32_t midi_readable_buffer_n_events(size_t buffer_ptr) {
    auto *buffer = reinterpret_cast<MidiReadableBuffer *>(buffer_ptr);
    return buffer ? buffer->n_events() : 0;
}

inline bool midi_readable_buffer_get_event(
    size_t buffer_ptr,
    uint32_t idx,
    uint32_t &out_time,
    uint16_t &out_size,
    uint8_t *out_data
) {
    auto *buffer = reinterpret_cast<MidiReadableBuffer *>(buffer_ptr);
    if (!buffer || !out_data || idx >= buffer->n_events()) {
        return false;
    }
    auto event = buffer->get_event(idx);
    out_time = event.time;
    out_size = event.size;
    memcpy(out_data, event.bytes, event.size);
    return true;
}

inline bool midi_writeable_buffer_write_event(
    size_t buffer_ptr,
    uint32_t time,
    uint16_t size,
    const uint8_t *data
) {
    auto *buffer = reinterpret_cast<MidiWriteableBuffer *>(buffer_ptr);
    if (!buffer || !data || size > 4) {
        return false;
    }
    MidiStorageElem event;
    event.time = time;
    event.size = size;
    memcpy(event.bytes, data, size);
    buffer->write_event(event);
    return true;
}
