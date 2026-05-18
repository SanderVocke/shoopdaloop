#include "DummyMidiPort.h"

#ifdef _WIN32
#undef min
#undef max
#endif

MidiStorageElem DummyMidiPort::get_event(uint32_t idx) const {
    // CXX bridge generates these as member functions, not free functions
    uint32_t time = m_rust->get_event_time(idx);
    uint16_t size = m_rust->get_event_size(idx);
    
    MidiStorageElem elem;
    elem.time = time;
    elem.size = size;
    
    if (size > 0 && size <= 4) {
        m_rust->get_event_bytes(idx, elem.bytes, 4);
    }
    
    return elem;
}

void DummyMidiPort::write_event(MidiStorageElem event) {
    ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug>("Write midi message value to internal buffer @ {}", event.time);
    
    // Convert to fixed-size array for CXX bridge (max 4 bytes)
    std::array<uint8_t, 4> data = {0, 0, 0, 0};
    std::memcpy(data.data(), event.bytes, std::min(static_cast<size_t>(event.size), sizeof(data)));
    
    m_rust->dummy_write_event(event.time, event.size, data);
}

DummyMidiPort::DummyMidiPort(
    std::string name,
    shoop_port_direction_t direction,
    shoop_weak_ptr<DummyExternalConnections> external_connections
) : MidiPort(true, true, true),
    DummyPort(name, direction, PortDataType::Midi, external_connections),
    WithCommandQueue(100, 1000, 1000),
    m_rust(backend_rust::new_dummy_midi_port(direction == ShoopPortDirection_Output)) {}

unsigned DummyMidiPort::input_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_External : ShoopPortConnectability_Internal;
}

unsigned DummyMidiPort::output_connectability() const {
    return (m_direction == ShoopPortDirection_Input) ? ShoopPortConnectability_Internal : ShoopPortConnectability_External;
}

void DummyMidiPort::clear_queues() {
    m_rust->clear_queues();
}

void DummyMidiPort::queue_msg(uint32_t size, uint32_t time, uint8_t const *data) {
    // Convert to fixed-size array for CXX bridge (max 4 bytes)
    std::array<uint8_t, 4> data_arr = {0, 0, 0, 0};
    std::memcpy(data_arr.data(), data, std::min(static_cast<size_t>(size), sizeof(data_arr)));
    
    m_rust->queue_msg(time, static_cast<uint16_t>(size), data_arr);
}

bool DummyMidiPort::get_queue_empty() {
    return m_rust->get_queue_empty();
}

void DummyMidiPort::request_data(uint32_t n_frames) {
    m_rust->request_data(n_frames);
}

void DummyMidiPort::PROC_prepare(uint32_t nframes) {
    PROC_handle_command_queue();
    m_rust->prepare(nframes);
    MidiPort::PROC_prepare(nframes);
}

void DummyMidiPort::PROC_process(uint32_t nframes) {
    if (nframes > 0) {
        ModuleLoggingEnabled<"Backend.DummyMidiPort">::log<log_level_debug_trace>("Process {}", nframes);
    }
    
    m_rust->process(nframes);
    
    // Track events through base class
    uint32_t n_written = m_rust->get_n_written_requested_msgs();
    if (n_written > 0) {
        MidiPort::increment_output_events(n_written);
    }
    
    MidiPort::PROC_process(nframes);
}

MidiReadableBuffer *
DummyMidiPort::PROC_get_read_output_data_buffer(uint32_t n_frames) {
    return this;
}

IMidiReadableBuffer *
DummyMidiPort::get_readable_buffer() {
    return this;
}

MidiWriteableBuffer *
DummyMidiPort::PROC_get_write_data_into_port_buffer(uint32_t n_frames) {
    return this;
}

IMidiWriteableBuffer *
DummyMidiPort::get_writeable_buffer() {
    return this;
}

uint32_t DummyMidiPort::n_events() const {
    return m_rust->n_events();
}

std::vector<DummyMidiPort::StoredMessage> DummyMidiPort::get_written_requested_msgs() {
    std::vector<DummyMidiPort::StoredMessage> result;
    
    uint32_t n = m_rust->get_n_written_requested_msgs();
    result.reserve(n);
    
    for (uint32_t i = 0; i < n; i++) {
        MidiStorageElem elem;
        elem.time = m_rust->get_written_msg_time(i);
        elem.size = m_rust->get_written_msg_size(i);
        
        if (elem.size > 0 && elem.size <= 4) {
            m_rust->get_written_msg_bytes(i, elem.bytes, 4);
        }
        
        result.push_back(elem);
    }
    
    m_rust->clear_written_msgs();
    
    return result;
}

DummyMidiPort::~DummyMidiPort() { 
    DummyPort::close();
}

void DummyMidiPort::set_muted(bool muted) {
    // Call the base MidiPort::set_muted() for the MidiPort base class state
    MidiPort::set_muted(muted);
    // Also set muted on our own Rust DummyMidiPort (which has its own MidiPort base)
    m_rust->set_muted(muted);
}

bool DummyMidiPort::get_muted() const {
    // Get mute state from our own Rust DummyMidiPort's base MidiPort
    return m_rust->get_muted();
}