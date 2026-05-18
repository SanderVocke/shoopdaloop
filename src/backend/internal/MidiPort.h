#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"
#include "IMidiStateTracking.h"
#include "IMidiRingbuffer.h"
#include <atomic>
#include "MidiStateTracker.h"
#include "MidiBuffer.h"
#include "IMidiReadableBuffer.h"
#include "IMidiWriteableBuffer.h"
#include "MidiStorageElem.h"
#include "LoggingBackend.h"
#include "shoop_shared_ptr.h"
#include "MidiRingbuffer.h"
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

    // Rust implementation for core logic (single source of truth)
    rust::Box<backend_rust::MidiPort> m_rust_port;

    // Ringbuffer access - owned by MidiPort but wraps Rust storage
    // This is needed for tests and some internal operations
    shoop_shared_ptr<MidiRingbuffer> m_midi_ringbuffer;

public:
    friend class MidiPortTestHelper;
    template<typename API> friend class GenericJackMidiInputPort;
    friend class MidiChannel;

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

    // State tracking methods
    void reset_n_input_events();
    uint32_t get_n_input_events() const;
    uint32_t get_input_event_count() const override;
    uint32_t n_notes_active() const override;

    void reset_n_output_events();
    uint32_t get_n_output_events() const;
    uint32_t get_output_event_count() const override;

    uint32_t get_n_input_notes_active();
    uint32_t get_n_output_notes_active();

    // Ringbuffer methods
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    void set_n_samples(uint32_t n) override { set_ringbuffer_n_samples(n); }
    uint32_t get_n_samples() const override { return get_ringbuffer_n_samples(); }
    void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const override;
    uint32_t get_current_n_samples() const override { return get_ringbuffer_n_samples(); }
    void PROC_snapshot_ringbuffer_into(IMidiStorage &s) const;

    // Ringbuffer accessors for tests and internal use
    shoop_shared_ptr<MidiRingbuffer> &maybe_midi_ringbuffer();

    // State tracker accessors via Rust bridge
    // Returns raw pointer as size_t (0 = None)
    // C++ callers can use this or get shared_ptr wrapper below
    size_t get_maybe_midi_state_tracker_raw();
    size_t get_maybe_ringbuffer_tail_state_tracker_raw();

    // Get state tracker as shared_ptr (for convenience)
    // Note: lifetime is tied to MidiPort's m_rust_port
    shoop_shared_ptr<MidiStateTracker> maybe_midi_state_tracker();
    shoop_shared_ptr<MidiStateTracker> maybe_ringbuffer_tail_state_tracker();

    // Increment event counters (for internal use)
    void increment_input_events(uint32_t count);
    void increment_output_events(uint32_t count);

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
    static shoop_shared_ptr<MidiRingbuffer> &get_ringbuffer(MidiPort &port) {
        return port.m_midi_ringbuffer;
    }
};