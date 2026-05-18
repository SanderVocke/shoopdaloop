#include "MidiPort.h"
#include "MidiStateTracker.h"
#include "shoop_globals.h"
#include "shoop_shared_ptr.h"
#include "types.h"

MidiWriteableBuffer *MidiPort::PROC_get_write_data_into_port_buffer  (uint32_t n_frames) { return nullptr; }
    
MidiReadableBuffer *MidiPort::PROC_get_read_output_data_buffer (uint32_t n_frames) { return nullptr; }

MidiReadableBuffer *MidiPort::PROC_internal_read_input_data_buffer (uint32_t n_frames) { return nullptr; }

MidiWriteableBuffer *MidiPort::PROC_internal_write_output_data_to_buffer (uint32_t n_frames) { return nullptr; }

MidiWriteableBuffer *MidiPort::PROC_maybe_get_send_out_buffer (uint32_t n_frames) { return nullptr; }

PortDataType MidiPort::type() const { return PortDataType::Midi; }

void MidiPort::set_muted(bool muted) { 
    if (muted != m_rust_port->get_muted()) {
        log<log_level_debug>("muted -> {}", muted);
    }
    m_rust_port->set_muted(muted);
}

bool MidiPort::get_muted() const { 
    return m_rust_port->get_muted(); 
}

void MidiPort::reset_n_input_events() { 
    m_rust_port->reset_n_input_events();
}

uint32_t MidiPort::get_n_input_events() const { 
    return m_rust_port->input_event_count(); 
}

uint32_t MidiPort::get_input_event_count() const { 
    return m_rust_port->input_event_count(); 
}

void MidiPort::reset_n_output_events() { 
    m_rust_port->reset_n_output_events();
}

uint32_t MidiPort::get_n_output_events() const { 
    return m_rust_port->output_event_count(); 
}

uint32_t MidiPort::get_output_event_count() const { 
    return m_rust_port->output_event_count(); 
}

uint32_t MidiPort::n_notes_active() const { 
    return m_rust_port->n_notes_active(); 
}

uint32_t MidiPort::get_n_input_notes_active() const {
    auto tracker_ptr = backend_rust::get_maybe_midi_state_tracker(*m_rust_port);
    if (!tracker_ptr) return 0;
    auto* tracker = (MidiStateTracker*)tracker_ptr;
    return tracker->n_notes_active();
}

uint32_t MidiPort::get_n_output_notes_active() const {
    auto tracker_ptr = backend_rust::get_maybe_midi_state_tracker(*m_rust_port);
    if (!tracker_ptr) return 0;
    auto* tracker = (MidiStateTracker*)tracker_ptr;
    return m_rust_port->get_muted() ? 0 : tracker->n_notes_active();
}

shoop_shared_ptr<MidiRingbuffer> &MidiPort::maybe_midi_ringbuffer() {
    return m_midi_ringbuffer;
}

size_t MidiPort::get_maybe_midi_state_tracker_raw() {
    return backend_rust::get_maybe_midi_state_tracker(*m_rust_port);
}

size_t MidiPort::get_maybe_ringbuffer_tail_state_tracker_raw() {
    return backend_rust::get_maybe_ringbuffer_tail_state_tracker(*m_rust_port);
}

shoop_shared_ptr<MidiStateTracker> MidiPort::maybe_midi_state_tracker() {
    auto ptr = backend_rust::get_maybe_midi_state_tracker(*m_rust_port);
    if (!ptr) return nullptr;
    return shoop_shared_ptr<MidiStateTracker>((MidiStateTracker*)ptr, [](MidiStateTracker*){});
}

shoop_shared_ptr<MidiStateTracker> MidiPort::maybe_ringbuffer_tail_state_tracker() {
    auto ptr = backend_rust::get_maybe_ringbuffer_tail_state_tracker(*m_rust_port);
    if (!ptr) return nullptr;
    return shoop_shared_ptr<MidiStateTracker>((MidiStateTracker*)ptr, [](MidiStateTracker*){});
}

void MidiPort::increment_input_events(uint32_t count) {
    backend_rust::increment_input_events(*m_rust_port, count);
}

void MidiPort::increment_output_events(uint32_t count) {
    backend_rust::increment_output_events(*m_rust_port, count);
}

void MidiPort::PROC_prepare(uint32_t nframes) {
    ma_write_data_into_port_buffer = PROC_get_write_data_into_port_buffer(nframes);
    ma_read_output_data_buffer = PROC_get_read_output_data_buffer(nframes);
    ma_internal_read_input_data_buffer = PROC_internal_read_input_data_buffer(nframes);
    ma_internal_write_output_data_to_buffer = PROC_internal_write_output_data_to_buffer(nframes);
}

void MidiPort::PROC_process(uint32_t nframes) {
    auto muted = m_rust_port->get_muted();

    if (nframes > 0) {
        log<log_level_debug_trace>("process {}", nframes);
    }
    
    // Get buffers
    auto write_in_buf = ma_write_data_into_port_buffer.load();
    auto read_out_buf = ma_read_output_data_buffer.load();
    auto read_in_buf = ma_internal_read_input_data_buffer.load();
    auto write_out_buf = ma_internal_write_output_data_to_buffer.load();
    bool processed_state = false;

    // Get ringbuffer and tail state references
    auto& ringbuffer = m_midi_ringbuffer;
    
    // Get tail state tracker via bridge
    auto tail_tracker_ptr = backend_rust::get_maybe_ringbuffer_tail_state_tracker(*m_rust_port);
    std::shared_ptr<MidiStateTracker> tail_state;
    if (tail_tracker_ptr) {
        // Wrap raw pointer in shared_ptr with no deleter (lifetime tied to MidiPort)
        tail_state = std::shared_ptr<MidiStateTracker>((MidiStateTracker*)tail_tracker_ptr, [](MidiStateTracker*){});
    }

    auto put_in_ringbuf = [&ringbuffer, &tail_state](MidiStorageElem &elem) {
        if (ringbuffer && tail_state) {
            ringbuffer->put(elem.time, elem.size, elem.bytes,
            [&tail_state](uint32_t time, uint16_t size, const uint8_t *data) {
                tail_state->process_msg(data);
            });
        }
    };

    // Count input events from the read output buffer (for input ports) or internal input buffer (for others)
    MidiReadableBuffer *input_buf = read_out_buf ? read_out_buf : read_in_buf;
    uint32_t n_in_events = 0;
    if (ringbuffer && tail_state) {
        ringbuffer->next_buffer(nframes,
            [&tail_state](uint32_t time, uint16_t size, const uint8_t *data) {
                tail_state->process_msg(data);
            });
    }
    if (input_buf) {
        n_in_events = input_buf->n_events();
        // Count events via Rust bridge
        backend_rust::increment_input_events(*m_rust_port, n_in_events);
        log<log_level_debug_trace>("# events in input buf: {}", n_in_events);
    }
    
    // Get state tracker via bridge for processing
    auto state_tracker_ptr = backend_rust::get_maybe_midi_state_tracker(*m_rust_port);
    std::shared_ptr<MidiStateTracker> state_tracker;
    if (state_tracker_ptr) {
        state_tracker = std::shared_ptr<MidiStateTracker>((MidiStateTracker*)state_tracker_ptr, [](MidiStateTracker*){});
    }
    
    if (input_buf && state_tracker) {
        // Process input events for state tracking
        for(uint32_t i=0; i<n_in_events; i++) {
            auto event = input_buf->get_event(i);
            state_tracker->process_msg(event.bytes);
            if (ringbuffer) {
                put_in_ringbuf(event);
            }
        }
        log<log_level_debug_trace>("processed state changes");
        processed_state = true;
    }
    if (!muted) {
        MidiReadableBuffer *source = input_buf;
        if (source) {
            auto n_events = source->n_events();
            log<log_level_debug_trace>("# output events: {}", n_events);
            // For output ports, count events being written to output
            // For input ports, source == read_out_buf, but we shouldn't count input as output
            // Only count to n_output_events if we have a write_out_buf (actual output)
            if (write_out_buf) {
                backend_rust::increment_output_events(*m_rust_port, n_events);
            }

            if (write_out_buf && state_tracker) {
                // We need to actively write data to the output.
                for(uint32_t i=0; i<n_events; i++) {
                    auto event = source->get_event(i);
                    write_out_buf->write_event(event);
                    if (!processed_state) {
                        state_tracker->process_msg(event.bytes);
                    }
                    if (!processed_state && ringbuffer) {
                        put_in_ringbuf(event);
                    }
                }
                log<log_level_debug_trace>("processed state changes");
                processed_state = true;
            }
        }
    }
    if (!muted && !processed_state && read_out_buf && state_tracker) {
        log<log_level_debug_trace>("processing msgs state from output read buffer");
        for(uint32_t i=0; i<read_out_buf->n_events(); i++) {
            auto event = read_out_buf->get_event(i);
            state_tracker->process_msg(event.bytes);
            if (ringbuffer) {
                put_in_ringbuf(event);
            }
        }
    }
}

MidiPort::MidiPort(
    bool track_notes,
    bool track_controls,
    bool track_programs
) : ModuleLoggingEnabled<"Backend.MidiPort">(),
    m_rust_port(backend_rust::new_midi_port(track_notes, track_controls, track_programs)),
    m_midi_ringbuffer(shoop_make_shared<MidiRingbuffer>(shoop_constants::midi_storage_size))
{
}

MidiPort::~MidiPort() {};

void MidiPort::set_ringbuffer_n_samples(unsigned n) {
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->set_n_samples(n);
    }
}

unsigned MidiPort::get_ringbuffer_n_samples() const {
    return m_midi_ringbuffer ? m_midi_ringbuffer->get_n_samples() : 0;
}

void MidiPort::snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end) const {
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->snapshot(target, start_offset_from_end);
    } else {
        target.clear();
    }
}

void MidiPort::PROC_snapshot_ringbuffer_into(IMidiStorage &s) const {
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->snapshot(s);
    } else {
        s.clear();
    }
}