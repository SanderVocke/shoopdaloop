#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"
#include "IMidiStateTracking.h"
#include <atomic>
#include "MidiStateTracker.h"
#include "MidiBuffer.h"
#include "IMidiReadableBuffer.h"
#include "IMidiWriteableBuffer.h"
#include "MidiStorage.h"
#include "LoggingBackend.h"
#include "shoop_shared_ptr.h"
#include "MidiPortBase.h"
#include "backend_rust/src/midi_storage_cxx.rs.h"  // For rust::Box
#include "backend_rust/src/midi_port_cxx.rs.h"  // Rust CXX bridge for MidiPort

struct MidiPortTestHelper;

/**
 * MidiPort - Main MIDI port implementation.
 */
class MidiPort : public virtual PortInterface,
                 public virtual IMidiStateTracking,
                 public virtual IMidiRingbuffer,
                 private ModuleLoggingEnabled<"Backend.MidiPort"> {
    std::atomic<MidiWriteableBuffer*> ma_write_data_into_port_buffer = nullptr;
    std::atomic<MidiReadableBuffer*> ma_read_output_data_buffer = nullptr;
    std::atomic<MidiReadableBuffer*> ma_internal_read_input_data_buffer = nullptr;
    std::atomic<MidiWriteableBuffer*> ma_internal_write_output_data_to_buffer = nullptr;

    // Rust implementation for core logic
    rust::Box<backend_rust::MidiPort> m_rust_port;

    // Ringbuffer access (still in C++ for tests and direct access)
    MidiPortBase m_base;

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

    // Interface accessors for IMidiReadableBuffer and IMidiWriteableBuffer
    virtual IMidiReadableBuffer *get_readable_buffer() { return nullptr; }
    virtual IMidiWriteableBuffer *get_writeable_buffer() { return nullptr; }
    virtual MidiReadableBuffer *get_internal_read_buffer() { return nullptr; }
    virtual MidiWriteableBuffer *get_internal_write_buffer() { return nullptr; }

    PortDataType type() const override;

    void set_muted(bool muted) override;
    bool get_muted() const override;

    // State tracking methods (delegated to composed MidiPortBase)
    void reset_n_input_events();
    uint32_t get_n_input_events() const;
    uint32_t get_input_event_count() const override;
    uint32_t n_notes_active() const override;

    void reset_n_output_events();
    uint32_t get_n_output_events() const;
    uint32_t get_output_event_count() const override;

    uint32_t get_n_input_notes_active() const;
    uint32_t get_n_output_notes_active() const;

    // Ringbuffer methods (delegated to composed MidiPortBase)
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    void set_n_samples(uint32_t n) override { set_ringbuffer_n_samples(n); }
    uint32_t get_n_samples() const override { return get_ringbuffer_n_samples(); }
    void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const override;
    uint32_t get_current_n_samples() const override { return get_ringbuffer_n_samples(); }
    void PROC_snapshot_ringbuffer_into(IMidiStorage &s) const;

    // Direct accessors for internal use
    shoop_shared_ptr<MidiStateTracker> &maybe_midi_state_tracker();
    shoop_shared_ptr<MidiStateTracker> &maybe_ringbuffer_tail_state_tracker();
    shoop_shared_ptr<MidiRingbuffer> &maybe_midi_ringbuffer();
    MidiPortBase& base() { return m_base; }
    const MidiPortBase& base() const { return m_base; }

    virtual void PROC_prepare(uint32_t nframes) override;
    virtual void PROC_process(uint32_t nframes) override;

    MidiPort(
        bool track_notes,
        bool track_controls,
        bool track_programs
    );
    virtual ~MidiPort();
};

struct MidiPortTestHelper {
    // Get ringbuffer from port - delegates to C++ MidiPortBase for test compatibility
    static shoop_shared_ptr<MidiRingbuffer> &get_ringbuffer(MidiPort &port) {
        return port.m_base.m_midi_ringbuffer;
    }
};