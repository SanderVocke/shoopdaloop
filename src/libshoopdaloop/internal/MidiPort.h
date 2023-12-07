#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"
#include <atomic>
#include "MidiStateTracker.h"
#include "MidiSortingBuffer.h"
#include "MidiBufferInterfaces.h"

class MidiPort : public virtual PortInterface {
    std::atomic<MidiWriteableBufferInterface *> ma_write_data_into_port_buffer;
    std::atomic<MidiReadWriteBufferInterface *> ma_processing_buffer;
    std::atomic<MidiReadableBufferInterface *> ma_read_output_data_buffer;
    std::atomic<MidiReadableBufferInterface *> ma_internal_read_input_data_buffer;
    std::atomic<MidiWriteableBufferInterface *> ma_internal_write_output_data_to_buffer;
    std::atomic<bool> ma_muted;
    std::shared_ptr<MidiStateTracker> m_maybe_midi_state;
    std::atomic<uint32_t> n_events_processed;
public:

    // Midi ports can have buffering or not. Multiple buffers are defined, although they
    // don't have to exist (nullptr) or can point to the same buffer.
    virtual MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer  (uint32_t n_frames) = 0;
    virtual MidiReadWriteBufferInterface *PROC_get_processing_buffer (uint32_t n_frames) = 0;
    virtual MidiReadableBufferInterface *PROC_get_read_output_data_buffer (uint32_t n_frames) = 0;
    // Below buffers are for internal use
    virtual MidiReadableBufferInterface *PROC_internal_read_input_data_buffer (uint32_t n_frames) = 0;
    virtual MidiWriteableBufferInterface *PROC_internal_write_output_data_to_buffer (uint32_t n_frames) = 0;

    // Buffer which the port will push messages into during the process step (e.g. to send MIDI
    // messages out to JACK)
    virtual MidiWriteableBufferInterface *PROC_maybe_get_send_out_buffer (uint32_t n_frames) = 0;

    PortDataType type() const override { return PortDataType::Midi; }

    void set_muted(bool muted) override { ma_muted = muted; }
    bool get_muted() const override { return ma_muted; }

    void PROC_prepare(uint32_t nframes) override {
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

    void PROC_process(uint32_t nframes) override {
        auto muted = ma_muted.load();
        
        // Get buffers
        auto write_in_buf = ma_write_data_into_port_buffer.load();
        auto read_out_buf = ma_read_output_data_buffer.load();
        auto read_in_buf = ma_internal_read_input_data_buffer.load();
        auto write_out_buf = ma_internal_write_output_data_to_buffer.load();
        auto procbuf = ma_processing_buffer.load();
        auto procbuf_inbuf = static_cast<MidiReadableBufferInterface*>(procbuf);
        auto procbuf_outbuf = static_cast<MidiWriteableBufferInterface*>(procbuf);

        if (!muted && read_in_buf) {
            auto n_events = read_in_buf->PROC_get_n_events();
            // Count events
            n_events_processed += n_events;
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
                        procbuf->PROC_event_write_by_reference(msg);
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
            for(uint32_t i=0; i<n_events; i++) {
                auto &msg = source->PROC_get_event_reference(i);
                write_out_buf->PROC_event_write_by_reference(msg);
            }
        }
    }

    MidiPort(
        bool track_notes,
        bool track_controls,
        bool track_programs
    ) {
        if (track_notes || track_controls || track_programs) {
            m_maybe_midi_state = MidiStateTracker(
                track_notes, track_controls, track_programs
            );
        }
    }
    virtual ~MidiPort() {};
};