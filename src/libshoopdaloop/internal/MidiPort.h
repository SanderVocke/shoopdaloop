#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"
#include <atomic>
#include "MidiStateTracker.h"
#include "MidiBufferInterfaces.h"
#include "MidiRingbuffer.h"
#include "LoggingBackend.h"
#include "shoop_shared_ptr.h"

struct MidiPortTestHelper;

class MidiPort : public virtual PortInterface, private ModuleLoggingEnabled<"Backend.MidiPort"> {
    std::atomic<MidiWriteableBufferInterface *> ma_write_data_into_port_buffer = nullptr;
    std::atomic<MidiReadWriteBufferInterface *> ma_processing_buffer = nullptr;
    std::atomic<MidiReadableBufferInterface *> ma_read_output_data_buffer = nullptr;
    std::atomic<MidiReadableBufferInterface *> ma_internal_read_input_data_buffer = nullptr;
    std::atomic<MidiWriteableBufferInterface *> ma_internal_write_output_data_to_buffer = nullptr;
    std::atomic<bool> ma_muted = false;

    // The current MIDI state on the port.
    shoop_shared_ptr<MidiStateTracker> m_maybe_midi_state;

    // The MIDI state at the tail of the ringbuffer. This is basically a time-delayed
    // version of the current state.
    shoop_shared_ptr<MidiStateTracker> m_ringbuffer_tail_state;

    shoop_shared_ptr<MidiRingbuffer> m_midi_ringbuffer;
    std::atomic<uint32_t> n_input_events = 0;
    std::atomic<uint32_t> n_output_events = 0;
public:
    friend class MidiPortTestHelper;

    // Midi ports can have buffering or not. Multiple buffers are defined, although they
    // don't have to exist (nullptr) or can point to the same buffer.
    virtual MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer  (uint32_t n_frames);
    virtual MidiReadWriteBufferInterface *PROC_get_processing_buffer (uint32_t n_frames);
    virtual MidiReadableBufferInterface *PROC_get_read_output_data_buffer (uint32_t n_frames);
    // Below buffers are for internal use
    virtual MidiReadableBufferInterface *PROC_internal_read_input_data_buffer (uint32_t n_frames);
    virtual MidiWriteableBufferInterface *PROC_internal_write_output_data_to_buffer (uint32_t n_frames);

    // Buffer which the port will push messages into during the process step (e.g. to send MIDI
    // messages out to JACK)
    virtual MidiWriteableBufferInterface *PROC_maybe_get_send_out_buffer (uint32_t n_frames);

    PortDataType type() const override;

    void set_muted(bool muted) override;
    bool get_muted() const override;

    void reset_n_input_events();
    uint32_t get_n_input_events() const;

    void reset_n_output_events();
    uint32_t get_n_output_events() const;

    uint32_t get_n_input_notes_active() const;
    uint32_t get_n_output_notes_active() const;

    shoop_shared_ptr<MidiStateTracker> &maybe_midi_state_tracker();
    shoop_shared_ptr<MidiStateTracker> &maybe_ringbuffer_tail_state_tracker();

    virtual void PROC_prepare(uint32_t nframes) override;
    virtual void PROC_process(uint32_t nframes) override;

    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    void PROC_snapshot_ringbuffer_into(MidiStorage &s) const;

    MidiPort(
        bool track_notes,
        bool track_controls,
        bool track_programs
    );
    virtual ~MidiPort();
};

struct MidiPortTestHelper {
    static shoop_shared_ptr<MidiRingbuffer> &get_ringbuffer(MidiPort &port) {
        return port.m_midi_ringbuffer;
    }
};