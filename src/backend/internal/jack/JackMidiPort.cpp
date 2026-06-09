#include "JackMidiPort.h"
#include <string>
#include "MidiPort.h"
#include <stdexcept>
#include <iostream>
#include "types.h"

uint32_t JackMidiInputPort::n_events() const {
    return m_messages.size();
}

MidiStorageElem JackMidiInputPort::get_event(uint32_t idx) const {
    return m_messages[idx];
}

unsigned JackMidiInputPort::input_connectability() const {
    return ShoopPortConnectability_External;
}

unsigned JackMidiInputPort::output_connectability() const {
    return ShoopPortConnectability_Internal;
}

void JackMidiInputPort::set_muted(bool muted) {
    m_muted = muted;
}

bool JackMidiInputPort::get_muted() const {
    return m_muted;
}

void JackMidiInputPort::set_ringbuffer_n_samples(unsigned n) {
    m_ringbuffer_n_samples = n;
}

unsigned JackMidiInputPort::get_ringbuffer_n_samples() const {
    return m_ringbuffer_n_samples;
}

unsigned JackMidiOutputPort::input_connectability() const {
    return ShoopPortConnectability_Internal;
}

unsigned JackMidiOutputPort::output_connectability() const {
    return ShoopPortConnectability_External;
}

void JackMidiOutputPort::set_muted(bool muted) {
    m_muted = muted;
}

bool JackMidiOutputPort::get_muted() const {
    return m_muted;
}

void JackMidiOutputPort::set_ringbuffer_n_samples(unsigned n) {
    m_ringbuffer_n_samples = n;
}

unsigned JackMidiOutputPort::get_ringbuffer_n_samples() const {
    return m_ringbuffer_n_samples;
}

void JackMidiOutputPort::write_event(MidiStorageElem event) {
    m_sorting_buffer->write_event(event);
}

JackMidiOutputPort::JackMidiOutputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        std::shared_ptr<IJackApi> api
    ) : JackMidiPort(name, ShoopPortDirection_Output, client, std::move(all_ports_tracker), std::move(api)),
        MidiPort(true, true, true),
        m_sorting_buffer(std::make_shared<MidiSortingBuffer>())
{}

JackMidiInputPort::JackMidiInputPort(
        std::string name,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        std::shared_ptr<IJackApi> api
    ) : JackMidiPort(name, ShoopPortDirection_Input, client, std::move(all_ports_tracker), std::move(api)),
        MidiPort(true, true, true),
        m_messages()
{
    m_messages.reserve(1024);
}

void JackMidiInputPort::PROC_prepare(uint32_t nframes) {
    // sets m_port
    JackMidiPort::PROC_prepare(nframes);
    m_messages.clear();
}

void JackMidiInputPort::PROC_process(uint32_t nframes) {
    (void)nframes;
    // Read events directly from JACK buffer and copy into our vector
    auto* jack_buffer = m_buffer.load();
    if (!jack_buffer) return;

    if (m_muted.load()) {
        m_messages.clear();
        return;
    }

    uint32_t n_events = m_api->midi_get_event_count(jack_buffer);

    m_messages.reserve(m_messages.size() + n_events);

    for (uint32_t i = 0; i < n_events; ++i) {
        jack_midi_event_t jack_event;
        m_api->midi_event_get(&jack_event, jack_buffer, i);

        MidiStorageElem elem;
        elem.time = jack_event.time;
        elem.size = jack_event.size;
        memcpy(elem.bytes, jack_event.buffer, jack_event.size);
        m_messages.push_back(elem);

        // Process state changes for note tracking via Rust bridge
        backend_rust::process_msg_raw_to_state(*m_rust_port, elem.bytes);
    }

    // Sort by time
    std::stable_sort(m_messages.begin(), m_messages.end(), [](const MidiStorageElem &a, const MidiStorageElem &b) {
        return a.time < b.time;
    });
}

MidiReadableBuffer *JackMidiInputPort::PROC_internal_read_input_data_buffer(uint32_t nframes) {
    (void)nframes;
    return static_cast<MidiReadableBuffer*>(this);
}

MidiReadableBuffer *JackMidiInputPort::PROC_get_read_output_data_buffer(uint32_t nframes) {
    (void)nframes;
    return static_cast<MidiReadableBuffer*>(this);
}

MidiWriteableBuffer *JackMidiOutputPort::PROC_internal_write_output_data_to_buffer(uint32_t nframes) {
    (void)nframes;
    return static_cast<MidiWriteableBuffer*>(this);
}

MidiWriteableBuffer *JackMidiOutputPort::PROC_get_write_data_into_port_buffer(uint32_t nframes) {
    (void)nframes;
    return static_cast<MidiWriteableBuffer*>(m_sorting_buffer.get());
}

void JackMidiOutputPort::PROC_prepare(uint32_t nframes) {
    JackMidiPort::PROC_prepare(nframes);
    m_api->midi_clear_buffer(m_buffer.load());
    m_sorting_buffer->PROC_clear();
}

void JackMidiOutputPort::PROC_process(uint32_t nframes) {
    if (get_muted()) {
        m_sorting_buffer->PROC_clear();
        return;
    }

    m_sorting_buffer->PROC_process(nframes);

    // Write sorted messages to JACK output buffer
    auto* jack_buffer = m_buffer.load();
    if (!jack_buffer) return;

    uint32_t n_events = m_sorting_buffer->n_events();
    for (uint32_t i = 0; i < n_events; ++i) {
        MidiStorageElem event = m_sorting_buffer->get_event(i);
        m_api->midi_event_write(jack_buffer, event.time, event.bytes, event.size);
    }
}
