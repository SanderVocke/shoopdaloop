#include "JackMidiPort.h"

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
                           jack_client_t *client)
    : MidiPortInterface(name, direction), m_client(client),
      m_direction(direction) {

    m_temp_midi_storage.reserve(n_event_storage);

    m_port = jack_port_register(
        m_client, name.c_str(), JACK_DEFAULT_MIDI_TYPE,
        direction == PortDirection::Input ? JackPortIsInput : JackPortIsOutput,
        0);

    if (m_port == nullptr) {
        throw std::runtime_error("Unable to open port.");
    }
    m_name = std::string(jack_port_name(m_port));
}

const char *JackMidiPort::name() const { return m_name.c_str(); }

PortDirection JackMidiPort::direction() const { return m_direction; }

void JackMidiPort::close() { jack_port_unregister(m_client, m_port); }

jack_port_t *JackMidiPort::get_jack_port() const { return m_port; }

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

JackMidiPort::~JackMidiPort() { close(); }