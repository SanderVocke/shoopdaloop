#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include "MidiSortingReadWritePort.h"
#include <stdexcept>
#include <iostream>
#include "types.h"

template<typename API>
uint32_t GenericJackMidiInputPort<API>::JackMidiReadBuffer::n_events() const {
    if (!m_jack_buffer) { return 0; }
    auto rval = API::midi_get_event_count(m_jack_buffer);
    return rval;
}

template<typename API>
MidiStorageElem GenericJackMidiInputPort<API>::JackMidiReadBuffer::get_event(uint32_t idx) const {
    jack_midi_event_t event;
    API::midi_event_get(&event, m_jack_buffer, idx);
    MidiStorageElem elem;
    elem.size = event.size;
    elem.proc_time = event.time;
    memcpy(elem.bytes, event.buffer, event.size);
    return elem;
}

template<typename API>
unsigned GenericJackMidiInputPort<API>::input_connectability() const {
    return ShoopPortConnectability_External;
}

template<typename API>
unsigned GenericJackMidiInputPort<API>::output_connectability() const {
    return ShoopPortConnectability_Internal;
}

template<typename API>
unsigned GenericJackMidiOutputPort<API>::input_connectability() const {
    return ShoopPortConnectability_Internal;
}

template<typename API>
unsigned GenericJackMidiOutputPort<API>::output_connectability() const {
    return ShoopPortConnectability_External;
}

template<typename API>
void GenericJackMidiOutputPort<API>::JackMidiWriteBuffer::write_event(MidiStorageElem event) {
    if(!m_jack_buffer) { return; }
    API::midi_event_write(m_jack_buffer, event.proc_time, event.bytes, event.size);
}

template<typename API>
GenericJackMidiInputPort<API>::GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, ShoopPortDirection_Input, client, all_ports_tracker),
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
MidiReadableBuffer *GenericJackMidiInputPort<API>::PROC_internal_read_input_data_buffer(uint32_t nframes) {
    (void)nframes;
    return &m_read_buffer;
}

template<typename API>
MidiWriteableBuffer *GenericJackMidiOutputPort<API>::PROC_internal_write_output_data_to_buffer(uint32_t nframes) {
    (void)nframes;
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