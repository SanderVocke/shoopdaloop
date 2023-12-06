#include "InternalLV2MidiOutputPort.h"
#include <lv2/atom/forge.h>
#include <string>
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>
#include <vector>

InternalLV2MidiOutputPort::InternalLV2MidiOutputPort(
    std::string name, shoop_port_direction_t direction, uint32_t capacity,
    uint32_t atom_chunk_urid, uint32_t atom_sequence_urid,
    uint32_t midi_event_type_urid)
    : MidiPortInterface(name, direction), m_name(name), m_direction(direction),
      m_evbuf(lv2_evbuf_new(capacity, atom_chunk_urid, atom_sequence_urid)),
      m_midi_event_type_urid(midi_event_type_urid) {}

MidiReadableBufferInterface &
InternalLV2MidiOutputPort::PROC_get_read_buffer(uint32_t n_frames) {
    throw std::runtime_error(
        "Internal LV2 MIDI output port does not support reading.");
}

MidiWriteableBufferInterface &
InternalLV2MidiOutputPort::PROC_get_write_buffer(uint32_t n_frames) {
    lv2_evbuf_reset(m_evbuf, true);
    m_iter = lv2_evbuf_begin(m_evbuf);
    return *(static_cast<MidiWriteableBufferInterface *>(this));
}

const char *InternalLV2MidiOutputPort::name() const { return m_name.c_str(); }

shoop_port_direction_t InternalLV2MidiOutputPort::direction() const {
    return m_direction;
}

void InternalLV2MidiOutputPort::close() {}

PortType InternalLV2MidiOutputPort::type() const { return PortType::Midi; }

PortExternalConnectionStatus InternalLV2MidiOutputPort::get_external_connection_status() const { return PortExternalConnectionStatus(); }

void InternalLV2MidiOutputPort::connect_external(std::string name) {}

void InternalLV2MidiOutputPort::disconnect_external(std::string name) {}

void InternalLV2MidiOutputPort::PROC_write_event_value(uint32_t size,
                                                       uint32_t time,
                                                       const uint8_t *data) {
    bool result = lv2_evbuf_write(&m_iter, time, 0, m_midi_event_type_urid,
                                  size, (void *)data);
    if (!result) {
        throw std::runtime_error("Failed to write MIDI event into LV2 evbuf");
    }
}

void InternalLV2MidiOutputPort::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {
    throw std::runtime_error("Write by reference not supported");
}

bool InternalLV2MidiOutputPort::write_by_reference_supported() const {
    return false;
}
bool InternalLV2MidiOutputPort::write_by_value_supported() const {
    return true;
}

void *InternalLV2MidiOutputPort::maybe_driver_handle() const {
    return (void*)this;
}

LV2_Evbuf *InternalLV2MidiOutputPort::internal_evbuf() const { return m_evbuf; }

InternalLV2MidiOutputPort::~InternalLV2MidiOutputPort() {
    lv2_evbuf_free(m_evbuf);
}