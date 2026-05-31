#include "InternalMidiPort.h"
#include <cstring>
#include <stdexcept>

#ifdef _WIN32
#undef min
#undef max
#endif

InternalMidiPort::InternalMidiPort(
    std::string name,
    unsigned input_connectability,
    unsigned output_connectability,
    unsigned /*ringbuffer_n_samples_hint*/
)
    : MidiPort(true, true, true),
      m_name(name),
      m_rust_internal(backend_rust::new_internal_midi_port(name, input_connectability, output_connectability)) {}

uint32_t InternalMidiPort::n_events() const {
    return m_rust_internal->n_events();
}

MidiStorageElem InternalMidiPort::get_event(uint32_t idx) const {
    uint32_t time = m_rust_internal->get_event_time(idx);
    uint16_t size = m_rust_internal->get_event_size(idx);

    MidiStorageElem elem;
    elem.time = time;
    elem.size = size;

    if (size > 0 && size <= 4) {
        m_rust_internal->get_event_bytes(idx, elem.bytes, 4);
    }

    return elem;
}

void InternalMidiPort::write_event(MidiStorageElem event) {
    std::array<uint8_t, 4> data = {0, 0, 0, 0};
    std::memcpy(data.data(), event.bytes, std::min(static_cast<size_t>(event.size), sizeof(data)));
    m_rust_internal->write_event(event.time, event.size, data);
}

MidiWriteableBuffer *InternalMidiPort::PROC_get_write_data_into_port_buffer(uint32_t /*n_frames*/) {
    return static_cast<MidiWriteableBuffer*>(this);
}

MidiReadableBuffer *InternalMidiPort::PROC_get_read_output_data_buffer(uint32_t /*n_frames*/) {
    return static_cast<MidiReadableBuffer*>(this);
}

MidiReadableBuffer *InternalMidiPort::PROC_internal_read_input_data_buffer(uint32_t /*n_frames*/) {
    return static_cast<MidiReadableBuffer*>(this);
}

MidiWriteableBuffer *InternalMidiPort::PROC_internal_write_output_data_to_buffer(uint32_t /*n_frames*/) {
    return static_cast<MidiWriteableBuffer*>(this);
}

void InternalMidiPort::PROC_prepare(uint32_t nframes) {
    m_rust_internal->proc_prepare(nframes);
    MidiPort::PROC_prepare(nframes);
}

void InternalMidiPort::PROC_process(uint32_t nframes) {
    MidiPort::PROC_process(nframes);
}

const char* InternalMidiPort::name() const {
    return m_name.c_str();
}

PortDataType InternalMidiPort::type() const {
    return PortDataType::Midi;
}

void InternalMidiPort::close() {}

void* InternalMidiPort::maybe_driver_handle() const {
    return (void*)this;
}

PortExternalConnectionStatus InternalMidiPort::get_external_connection_status() const {
    return PortExternalConnectionStatus();
}

void InternalMidiPort::connect_external(std::string name) {
    (void)name;
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

void InternalMidiPort::disconnect_external(std::string name) {
    (void)name;
    throw std::runtime_error("Internal ports cannot be externally connected.");
}

unsigned InternalMidiPort::input_connectability() const {
    return m_rust_internal->input_connectability();
}

unsigned InternalMidiPort::output_connectability() const {
    return m_rust_internal->output_connectability();
}
