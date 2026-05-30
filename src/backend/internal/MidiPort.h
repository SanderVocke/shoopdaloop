#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"
#include <atomic>
#include "MidiStateTracker.h"
#include "MidiBuffer.h"
#include "MidiRingbuffer.h"
#include "LoggingBackend.h"
#include "shoop_shared_ptr.h"
#include "TracyPlotter.h"
#include "Checksum.h"

struct MidiPortTestHelper;

class MidiPort : public virtual PortInterface, private ModuleLoggingEnabled<"Backend.MidiPort"> {
    std::atomic<MidiWriteableBuffer*> ma_write_data_into_port_buffer = nullptr;
    std::atomic<MidiReadableBuffer*> ma_read_output_data_buffer = nullptr;
    std::atomic<MidiReadableBuffer*> ma_internal_read_input_data_buffer = nullptr;
    std::atomic<MidiWriteableBuffer*> ma_internal_write_output_data_to_buffer = nullptr;
    std::atomic<bool> ma_muted = false;

    // The current MIDI state on the port.
    shoop_shared_ptr<MidiStateTracker> m_maybe_midi_state;

    // The MIDI state at the tail of the ringbuffer. This is basically a time-delayed
    // version of the current state.
    shoop_shared_ptr<MidiStateTracker> m_ringbuffer_tail_state;

    shoop_shared_ptr<MidiRingbuffer> m_midi_ringbuffer;
    std::atomic<uint32_t> n_input_events = 0;
    std::atomic<uint32_t> n_output_events = 0;

    // Checksum tracking for data consistency verification
    std::atomic<double> ma_input_checksum{0.0};
    std::atomic<double> ma_output_checksum{0.0};

    // Tracy plotters for MIDI port debugging (suffixes only, base identifier from name())
    TracyPlotter m_plot_input_events{"input_events"};
    TracyPlotter m_plot_output_events{"output_events"};
    TracyPlotter m_plot_notes_active{"notes_active"};
    TracyPlotter m_plot_muted{"muted"};
    TracyPlotter m_plot_frames_processed{"frames_processed"};
    TracyPlotter m_plot_input_checksum{"input_checksum"};
    TracyPlotter m_plot_output_checksum{"output_checksum"};
public:
    friend class MidiPortTestHelper;

    // Midi ports can have buffering or not. Multiple buffers are defined, although they
    // don't have to exist (nullptr) or can point to the same buffer.
    virtual MidiWriteableBuffer *PROC_get_write_data_into_port_buffer  (uint32_t n_frames);
    virtual MidiReadableBuffer *PROC_get_read_output_data_buffer (uint32_t n_frames);
    // Below buffers are for internal use
    virtual MidiReadableBuffer *PROC_internal_read_input_data_buffer (uint32_t n_frames);
    virtual MidiWriteableBuffer *PROC_internal_write_output_data_to_buffer (uint32_t n_frames);

    // Buffer which the port will push messages into during the process step (e.g. to send MIDI
    // messages out to JACK)
    virtual MidiWriteableBuffer *PROC_maybe_get_send_out_buffer (uint32_t n_frames);

    PortDataType type() const override;

    void set_muted(bool muted) override;
    bool get_muted() const override;

    void reset_n_input_events();
    uint32_t get_n_input_events() const;

    void reset_n_output_events();
    uint32_t get_n_output_events() const;

    double get_input_checksum() const { return ma_input_checksum.load(); }
    double get_output_checksum() const { return ma_output_checksum.load(); }

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