#include "JackMidiPort.h"
#include <string>
#include "PortInterface.h"
#include "LoggingBackend.h"
#include <stdexcept>

template<typename API>
GenericJackMidiPort<API>::ReadMessage::ReadMessage(jack_midi_event_t e) {
    time = e.time;
    size = e.size;
    buffer = e.buffer;
}

template<typename API>
uint32_t GenericJackMidiPort<API>::ReadMessage::get_time() const { return time; }
template<typename API>
uint32_t GenericJackMidiPort<API>::ReadMessage::get_size() const { return size; }
template<typename API>
const uint8_t *GenericJackMidiPort<API>::ReadMessage::get_data() const {
    return (const uint8_t *)buffer;
}
template<typename API>
void GenericJackMidiPort<API>::ReadMessage::get(uint32_t &size_out, uint32_t &time_out,
                                    const uint8_t *&data_out) const {
    size_out = size;
    time_out = time;
    data_out = buffer;
}

template<typename API>
size_t GenericJackMidiPort<API>::PROC_get_n_events() const {
    return API::midi_get_event_count(m_jack_read_buf);
}

template<typename API>
MidiSortableMessageInterface &GenericJackMidiPort<API>::PROC_get_event_reference(size_t idx) {
    jack_midi_event_t evt;
    API::midi_event_get(&evt, m_jack_read_buf, idx);
    m_temp_midi_storage.push_back(ReadMessage(evt));
    return m_temp_midi_storage.back();
}

template<typename API>
void GenericJackMidiPort<API>::PROC_write_event_value(uint32_t size, uint32_t time,
                                          const uint8_t *data) {
    API::midi_event_write(m_jack_write_buf, time, (uint8_t *)data, size);
}

template<typename API>
void GenericJackMidiPort<API>::PROC_write_event_reference(
    MidiSortableMessageInterface const &m) {
    throw std::runtime_error("Write by reference not supported");
}

template<typename API>
bool GenericJackMidiPort<API>::write_by_reference_supported() const { return false; }
template<typename API>
bool GenericJackMidiPort<API>::write_by_value_supported() const { return true; }

template<typename API>
GenericJackMidiPort<API>::GenericJackMidiPort(std::string name, PortDirection direction,
                           jack_client_t *client, std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker)
    : MidiPortInterface(name, direction), GenericJackPort<API>(name, direction, PortType::Midi, client, all_ports_tracker) {

    m_temp_midi_storage.reserve(n_event_storage);
}

template<typename API>
MidiReadableBufferInterface &GenericJackMidiPort<API>::PROC_get_read_buffer(size_t n_frames) {
    m_temp_midi_storage.clear();
    m_jack_read_buf = API::port_get_buffer(m_port, n_frames);
    return *(static_cast<MidiReadableBufferInterface *>(this));
}

template<typename API>
MidiWriteableBufferInterface &GenericJackMidiPort<API>::PROC_get_write_buffer(size_t n_frames) {
    m_jack_write_buf = API::port_get_buffer(m_port, n_frames);
    API::midi_clear_buffer(m_jack_write_buf);
    return *(static_cast<MidiWriteableBufferInterface *>(this));
}

template class GenericJackMidiPort<JackApi>;
template class GenericJackMidiPort<JackTestApi>;