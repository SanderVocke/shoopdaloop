#include "JackMidiPort.h"
#include <string>
#include "PortInterface.h"
#include <stdexcept>

JackMidiPort::ReadMessage::ReadMessage(jack_midi_event_t e) {
    time = e.time;
    size = e.size;
    buffer = e.buffer;
}

uint32_t JackMidiPort::ReadMessage::get_time() const { return time; }
uint32_t JackMidiPort::ReadMessage::get_size() const { return size; }
const uint8_t *JackMidiPort::ReadMessage::get_data() const {
    return (const uint8_t *)buffer;
}
void JackMidiPort::ReadMessage::get(uint32_t &size_out, uint32_t &time_out,
                                    const uint8_t *&data_out) const {
    size_out = size;
    time_out = time;
    data_out = buffer;
}

size_t JackMidiPort::PROC_get_n_events() const {
    return jack_midi_get_event_count(m_jack_read_buf);
}

MidiSortableMessageInterface &
JackMidiPort::PROC_get_event_reference(size_t idx) {
    jack_midi_event_t evt;
    jack_midi_event_get(&evt, m_jack_read_buf, idx);
    m_temp_midi_storage.push_back(ReadMessage(evt));
    return m_temp_midi_storage.back();
}

void JackMidiPort::PROC_write_event_value(uint32_t size, uint32_t time,
                                          const uint8_t *data) {
    jack_midi_event_write(m_jack_write_buf, time, (uint8_t *)data, size);
}

void JackMidiPort::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {
    throw std::runtime_error("Write by reference not supported");
}

bool JackMidiPort::write_by_reference_supported() const { return false; }
bool JackMidiPort::write_by_value_supported() const { return true; }

JackMidiPort::JackMidiPort(std::string name, PortDirection direction,
                           jack_client_t *client, std::shared_ptr<JackAllPorts> all_ports_tracker)
    : MidiPortInterface(name, direction), JackPort(name, direction, PortType::Midi, client, all_ports_tracker) {

    m_temp_midi_storage.reserve(n_event_storage);
}

MidiReadableBufferInterface &
JackMidiPort::PROC_get_read_buffer(size_t n_frames) {
    m_temp_midi_storage.clear();
    m_jack_read_buf = jack_port_get_buffer(m_port, n_frames);
    return *(static_cast<MidiReadableBufferInterface *>(this));
}

MidiWriteableBufferInterface &
JackMidiPort::PROC_get_write_buffer(size_t n_frames) {
    m_jack_write_buf = jack_port_get_buffer(m_port, n_frames);
    jack_midi_clear_buffer(m_jack_write_buf);
    return *(static_cast<MidiWriteableBufferInterface *>(this));
}