#include "MidiPort.h"

MidiWriteableBufferInterface *MidiPort::PROC_get_write_data_into_port_buffer  (uint32_t n_frames) { return nullptr; }
    
MidiReadWriteBufferInterface *MidiPort::PROC_get_processing_buffer (uint32_t n_frames) { return nullptr; }

MidiReadableBufferInterface *MidiPort::PROC_get_read_output_data_buffer (uint32_t n_frames) { return nullptr; }

MidiReadableBufferInterface *MidiPort::PROC_internal_read_input_data_buffer (uint32_t n_frames) { return nullptr; }

MidiWriteableBufferInterface *MidiPort::PROC_internal_write_output_data_to_buffer (uint32_t n_frames) { return nullptr; }

MidiWriteableBufferInterface *MidiPort::PROC_maybe_get_send_out_buffer (uint32_t n_frames) { return nullptr; }

PortDataType MidiPort::type() const { return PortDataType::Midi; }

void MidiPort::set_muted(bool muted) { ma_muted = muted; }

bool MidiPort::get_muted() const { return ma_muted; }

void MidiPort::reset_n_events_processed() { n_events_processed = 0; }

uint32_t MidiPort::get_n_events_processed() const { return n_events_processed; }

std::shared_ptr<MidiStateTracker> &MidiPort::maybe_midi_state_tracker() {
    return m_maybe_midi_state;
}

void MidiPort::PROC_prepare(uint32_t nframes) {
    ma_write_data_into_port_buffer = PROC_get_write_data_into_port_buffer(nframes);
    ma_read_output_data_buffer = PROC_get_read_output_data_buffer(nframes);
    ma_internal_read_input_data_buffer = PROC_internal_read_input_data_buffer(nframes);
    ma_internal_write_output_data_to_buffer = PROC_internal_write_output_data_to_buffer(nframes);

    auto procbuf = PROC_get_processing_buffer(nframes);
    ma_processing_buffer = procbuf;

    if (procbuf) {
        procbuf->PROC_prepare(nframes);
    }
}

void MidiPort::PROC_process(uint32_t nframes) {
    auto muted = ma_muted.load();
    
    // Get buffers
    auto write_in_buf = ma_write_data_into_port_buffer.load();
    auto read_out_buf = ma_read_output_data_buffer.load();
    auto read_in_buf = ma_internal_read_input_data_buffer.load();
    auto write_out_buf = ma_internal_write_output_data_to_buffer.load();
    auto procbuf = ma_processing_buffer.load();
    auto procbuf_inbuf = static_cast<MidiReadableBufferInterface*>(procbuf);
    auto procbuf_outbuf = static_cast<MidiWriteableBufferInterface*>(procbuf);

    uint32_t n_events_processed_here = 0;

    if (!muted && read_in_buf) {
        auto n_events = read_in_buf->PROC_get_n_events();
        // Count events
        n_events_processed_here = n_events;
        // Process state
        if(m_maybe_midi_state) {
            for(uint32_t i=0; i<n_events; i++) {
                auto &msg = read_in_buf->PROC_get_event_reference(i);
                uint32_t size, time;
                const uint8_t *data;
                msg.get(size, time, data);
                m_maybe_midi_state->process_msg(data);
                if (procbuf && procbuf_inbuf && procbuf_inbuf != read_in_buf) {
                    // Our processing buffer is separate from our input buffer.
                    // We need to move data into the processing buffer manually.
                    procbuf->PROC_write_event_reference(msg);
                }
            }
        }
    }
    if (!muted && procbuf) {
        // Sort the processing buffer.
        procbuf->PROC_process(nframes);
    }
    if (!muted && write_out_buf) {
        // We need to actively write data to the output.
        // Take it from the processing buffer if we have one, otherwise from the input.
        MidiReadableBufferInterface *source =
            procbuf ? procbuf : read_in_buf;
        auto n_events = source->PROC_get_n_events();
        n_events_processed_here = std::max(n_events_processed_here, n_events);
        for(uint32_t i=0; i<n_events; i++) {
            auto &msg = source->PROC_get_event_reference(i);
            write_out_buf->PROC_write_event_reference(msg);
        }
    }

    n_events_processed += n_events_processed_here;
}

MidiPort::MidiPort(
    bool track_notes,
    bool track_controls,
    bool track_programs
) {
    if (track_notes || track_controls || track_programs) {
        m_maybe_midi_state = std::make_shared<MidiStateTracker>(
            track_notes, track_controls, track_programs
        );
    }
}

MidiPort::~MidiPort() {};