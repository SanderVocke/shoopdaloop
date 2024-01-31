#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include "MidiSortingReadWritePort.h"
#include <stdexcept>
#include <iostream>

template<typename API>
GenericJackMidiInputPort<API>::JackMidiReadBuffer::JackMidiReadBuffer() : m_jack_buffer(nullptr) {}
        
template<typename API>
bool GenericJackMidiInputPort<API>::JackMidiReadBuffer::read_by_reference_supported() const {
    return false;
}

template<typename API>
uint32_t GenericJackMidiInputPort<API>::JackMidiReadBuffer::PROC_get_n_events() const {
    if (!m_jack_buffer) { return 0; }
    auto rval = API::midi_get_event_count(m_jack_buffer);
    return rval;
}

template<typename API>
MidiSortableMessageInterface const& GenericJackMidiInputPort<API>::JackMidiReadBuffer::PROC_get_event_reference(uint32_t idx) {
    throw std::runtime_error("Attempt to read from jack midi port by reference");
}

template<typename API>
void GenericJackMidiInputPort<API>::JackMidiReadBuffer::PROC_get_event_value(uint32_t idx,
                                uint32_t &size_out,
                                uint32_t &time_out,
                                const uint8_t* &data_out)
{
    jack_midi_event_t event;
    API::midi_event_get(&event, m_jack_buffer, idx);
    size_out = event.size;
    time_out = event.time;
    data_out = event.buffer;
}

template<typename API>
GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::JackMidiWriteBuffer() : m_jack_buffer(nullptr) {}

template<typename API>
bool GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::write_by_reference_supported() const {
    return true;
}

template<typename API>
bool GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::write_by_value_supported() const {
    return true;
}

template<typename API>
void GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::PROC_write_event_reference(MidiSortableMessageInterface const& m) {
    PROC_write_event_value(m.get_size(), m.get_time(), m.get_data());
}

template<typename API>
void GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::PROC_write_event_value(uint32_t size,
                            uint32_t time,
                            const uint8_t* data) {
    if(!m_jack_buffer) { return; }
    API::midi_event_write(m_jack_buffer, time, data, size);
}

template<typename API>
GenericJackMidiInputPort<API>::GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, Input, client, all_ports_tracker),
        MidiBufferingInputPort()
{}

template<typename API>
void GenericJackMidiInputPort<API>::PROC_prepare(uint32_t nframes) {
    // sets m_port
    GenericJackMidiPort<API>::PROC_prepare(nframes);
    m_read_buffer.m_jack_buffer = m_buffer;
    MidiBufferingInputPort::PROC_prepare(nframes);
}

template<typename API>
void GenericJackMidiInputPort<API>::PROC_process(uint32_t nframes) {
    MidiBufferingInputPort::PROC_process(nframes);
}

template<typename API>
MidiReadableBufferInterface *GenericJackMidiInputPort<API>::PROC_internal_read_input_data_buffer(uint32_t nframes) {
    return &m_read_buffer;
}

template<typename API>
MidiWriteableBufferInterface *GenericJackMidiOutputPort<API>::PROC_internal_write_output_data_to_buffer(uint32_t nframes) {
    return &m_write_buffer;
}

template<typename API>
void GenericJackMidiOutputPort<API>::PROC_prepare(uint32_t nframes) {
    GenericJackMidiPort<API>::PROC_prepare(nframes);
    m_write_buffer.m_jack_buffer = m_buffer;
    API::midi_clear_buffer(m_buffer.load());
    MidiSortingReadWritePort::PROC_prepare(nframes);
}

template<typename API>
void GenericJackMidiOutputPort<API>::PROC_process(uint32_t nframes) {
    MidiSortingReadWritePort::PROC_process(nframes);
}

template class GenericJackMidiInputPort<JackApi>;
template class GenericJackMidiInputPort<JackTestApi>;
template class GenericJackMidiOutputPort<JackApi>;
template class GenericJackMidiOutputPort<JackTestApi>;