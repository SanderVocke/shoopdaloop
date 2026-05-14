#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include <stdexcept>
#include <iostream>
#include "types.h"

template<typename API>
uint32_t GenericJackMidiInputPort<API>::n_events() const {
    return m_messages.size();
}

template<typename API>
MidiStorageElem GenericJackMidiInputPort<API>::get_event(uint32_t idx) const {
    return m_messages[idx];
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
void GenericJackMidiInputPort<API>::set_muted(bool muted) {
    m_muted = muted;
}

template<typename API>
bool GenericJackMidiInputPort<API>::get_muted() const {
    return m_muted;
}

template<typename API>
void GenericJackMidiInputPort<API>::set_ringbuffer_n_samples(unsigned n) {
    m_ringbuffer_n_samples = n;
}

template<typename API>
unsigned GenericJackMidiInputPort<API>::get_ringbuffer_n_samples() const {
    return m_ringbuffer_n_samples;
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
void GenericJackMidiOutputPort<API>::set_muted(bool muted) {
    m_muted = muted;
}

template<typename API>
bool GenericJackMidiOutputPort<API>::get_muted() const {
    return m_muted;
}

template<typename API>
void GenericJackMidiOutputPort<API>::set_ringbuffer_n_samples(unsigned n) {
    m_ringbuffer_n_samples = n;
}

template<typename API>
unsigned GenericJackMidiOutputPort<API>::get_ringbuffer_n_samples() const {
    return m_ringbuffer_n_samples;
}

template<typename API>
void GenericJackMidiOutputPort<API>::write_event(MidiStorageElem event) {
    m_sorting_buffer->write_event(event);
}

template<typename API>
GenericJackMidiOutputPort<API>::GenericJackMidiOutputPort(
        std::string name,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, ShoopPortDirection_Output, client, all_ports_tracker),
        m_sorting_buffer(shoop_make_shared<MidiSortingBuffer>())
{}

template<typename API>
GenericJackMidiInputPort<API>::GenericJackMidiInputPort(
        std::string name,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    ) : GenericJackMidiPort<API>(name, ShoopPortDirection_Input, client, all_ports_tracker),
        m_messages()
{
    m_messages.reserve(1024);
}

template<typename API>
void GenericJackMidiInputPort<API>::PROC_prepare(uint32_t nframes) {
    // sets m_port
    GenericJackMidiPort<API>::PROC_prepare(nframes);
    m_messages.clear();
}

template<typename API>
void GenericJackMidiInputPort<API>::PROC_process(uint32_t nframes) {
    // Read events directly from JACK buffer and copy into our vector
    auto* jack_buffer = m_buffer.load();
    if (!jack_buffer) return;

    uint32_t n_events = API::midi_get_event_count(jack_buffer);
    
    m_messages.reserve(m_messages.size() + n_events);
    
    for (uint32_t i = 0; i < n_events; ++i) {
        jack_midi_event_t jack_event;
        API::midi_event_get(&jack_event, jack_buffer, i);
        
        MidiStorageElem elem;
        elem.proc_time = jack_event.time;
        elem.storage_time = 0;
        elem.size = jack_event.size;
        memcpy(elem.bytes, jack_event.buffer, jack_event.size);
        m_messages.push_back(elem);
    }
    
    // Sort by time
    std::stable_sort(m_messages.begin(), m_messages.end(), [](const MidiStorageElem &a, const MidiStorageElem &b) {
        return a.proc_time < b.proc_time;
    });
}

template<typename API>
MidiReadableBuffer *GenericJackMidiInputPort<API>::PROC_internal_read_input_data_buffer(uint32_t nframes) {
    (void)nframes;
    return this;
}

template<typename API>
MidiWriteableBuffer *GenericJackMidiOutputPort<API>::PROC_internal_write_output_data_to_buffer(uint32_t nframes) {
    (void)nframes;
    return m_sorting_buffer.get();
}

template<typename API>
MidiWriteableBuffer *GenericJackMidiOutputPort<API>::PROC_get_write_data_into_port_buffer(uint32_t nframes) {
    (void)nframes;
    return m_sorting_buffer.get();
}

template<typename API>
void GenericJackMidiOutputPort<API>::PROC_prepare(uint32_t nframes) {
    GenericJackMidiPort<API>::PROC_prepare(nframes);
    API::midi_clear_buffer(m_buffer.load());
    m_sorting_buffer->PROC_prepare(nframes);
}

template<typename API>
void GenericJackMidiOutputPort<API>::PROC_process(uint32_t nframes) {
    m_sorting_buffer->PROC_process(nframes);
    
    // Write sorted messages to JACK output buffer
    auto* jack_buffer = m_buffer.load();
    if (!jack_buffer) return;

    uint32_t n_events = m_sorting_buffer->n_events();
    for (uint32_t i = 0; i < n_events; ++i) {
        MidiStorageElem event = m_sorting_buffer->get_event(i);
        API::midi_event_write(jack_buffer, event.proc_time, event.bytes, event.size);
    }
}

template class GenericJackMidiInputPort<JackApi>;
template class GenericJackMidiInputPort<JackTestApi>;
template class GenericJackMidiOutputPort<JackApi>;
template class GenericJackMidiOutputPort<JackTestApi>;