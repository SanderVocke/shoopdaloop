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
    if (muted != ma_muted.load()) {
        log<log_level_debug>("muted -> {}", muted);
    }
    ma_muted = muted;
}

bool MidiPort::get_muted() const { return ma_muted; }

void MidiPort::reset_n_input_events() { n_input_events = 0; }

uint32_t MidiPort::get_n_input_events() const { return n_input_events; }

void MidiPort::reset_n_output_events() { n_output_events = 0; }

uint32_t MidiPort::get_n_output_events() const { return n_output_events; }

uint32_t MidiPort::get_n_input_notes_active() const {
    auto &m = m_maybe_midi_state;
    return m ? m->n_notes_active() : 0;
}

uint32_t MidiPort::get_n_output_notes_active() const {
    auto &m = m_maybe_midi_state;
    return (m && !ma_muted) ? m->n_notes_active() : 0;
}

shoop_shared_ptr<MidiStateTracker> &MidiPort::maybe_midi_state_tracker() {
    return m_maybe_midi_state;
}

shoop_shared_ptr<MidiStateTracker> &MidiPort::maybe_ringbuffer_tail_state_tracker() {
    return m_ringbuffer_tail_state;
}

void MidiPort::PROC_prepare(uint32_t nframes) {
    ma_write_data_into_port_buffer = PROC_get_write_data_into_port_buffer(nframes);
    ma_read_output_data_buffer = PROC_get_read_output_data_buffer(nframes);
    ma_internal_read_input_data_buffer = PROC_internal_read_input_data_buffer(nframes);
    ma_internal_write_output_data_to_buffer = PROC_internal_write_output_data_to_buffer(nframes);
}

void MidiPort::PROC_process(uint32_t nframes) {
    auto muted = ma_muted.load();

    if (nframes > 0) {
        log<log_level_debug_trace>("process {}", nframes);
    }
    
    // Get buffers
    auto write_in_buf = ma_write_data_into_port_buffer.load();
    auto read_out_buf = ma_read_output_data_buffer.load();
    auto read_in_buf = ma_internal_read_input_data_buffer.load();
    auto write_out_buf = ma_internal_write_output_data_to_buffer.load();
    bool processed_state = false;

    auto put_in_ringbuf = [this](MidiStorageElem &elem) {
        if (m_midi_ringbuffer) {
            m_midi_ringbuffer->put(elem.time, elem.size, elem.bytes,
            [this](uint32_t time, uint16_t size, const uint8_t *data) {
                m_ringbuffer_tail_state->process_msg(data);
            });
        }
    };

    // Count input events from the read output buffer (for input ports) or internal input buffer (for others)
    MidiReadableBuffer *input_buf = read_out_buf ? read_out_buf : read_in_buf;
    uint32_t n_in_events = 0;
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->next_buffer(nframes,
            [this](uint32_t time, uint16_t size, const uint8_t *data) {
                m_ringbuffer_tail_state->process_msg(data);
            });
    }
    if (input_buf) {
        n_in_events = input_buf->n_events();
        // Count events
        n_input_events += n_in_events;
        log<log_level_debug_trace>("# events in input buf: {}", n_in_events);
    }
    if (input_buf) {
        // Process input events for state tracking
        for(uint32_t i=0; i<n_in_events; i++) {
            auto event = input_buf->get_event(i);
            if(m_maybe_midi_state) {
                m_maybe_midi_state->process_msg(event.bytes);
            }
            if (m_midi_ringbuffer) {
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
                n_output_events += n_events;
            }

            if (write_out_buf) {
                // We need to actively write data to the output.
                for(uint32_t i=0; i<n_events; i++) {
                    auto event = source->get_event(i);
                    write_out_buf->write_event(event);
                    if (!processed_state && m_maybe_midi_state) {
                        m_maybe_midi_state->process_msg(event.bytes);
                    }
                    if (!processed_state && m_midi_ringbuffer) {
                        put_in_ringbuf(event);
                    }
                }
                log<log_level_debug_trace>("processed state changes");
                processed_state = true;
            }
        }
    }
    if (!muted && !processed_state && m_maybe_midi_state && read_out_buf) {
        log<log_level_debug_trace>("processing msgs state from output read buffer");
        for(uint32_t i=0; i<read_out_buf->n_events(); i++) {
            auto event = read_out_buf->get_event(i);
            if (m_maybe_midi_state) {
                m_maybe_midi_state->process_msg(event.bytes);
            }
            if (m_midi_ringbuffer) {
                put_in_ringbuf(event);
            }
        }
    }
}

MidiPort::MidiPort(
    bool track_notes,
    bool track_controls,
    bool track_programs
) : ModuleLoggingEnabled<"Backend.MidiPort">() {
    if (track_notes || track_controls || track_programs) {
        m_maybe_midi_state = shoop_make_shared<MidiStateTracker>(
            track_notes, track_controls, track_programs
        );
    }
    m_ringbuffer_tail_state = shoop_make_shared<MidiStateTracker>(
        track_notes, track_controls, track_programs
    );
}

MidiPort::~MidiPort() {};

void MidiPort::set_ringbuffer_n_samples(unsigned n) {
    if (n > 0 && !m_midi_ringbuffer) {
        m_midi_ringbuffer = shoop_make_shared<MidiRingbuffer>(shoop_constants::midi_storage_size);
    }
    if (m_midi_ringbuffer) {
        m_midi_ringbuffer->set_n_samples(n);
    }
}

unsigned MidiPort::get_ringbuffer_n_samples() const {
    return m_midi_ringbuffer ? m_midi_ringbuffer->get_n_samples() : 0;
}

void MidiPort::PROC_snapshot_ringbuffer_into(MidiStorage &s) const {
    if (m_midi_ringbuffer) {
        auto n = m_midi_ringbuffer->n_events();
        log<log_level_debug_trace>("snapshot ringbuffer ({} msgs) into storage", n);
        m_midi_ringbuffer->snapshot(s);
    } else {
        log<log_level_debug_trace>("ringbuffer snapshot requested but no ringbuffer active");
        s.clear();
    }
}