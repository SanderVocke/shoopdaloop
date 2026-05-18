#pragma once
#include "AudioMidiDriver.h"
#include "MidiPort.h"
#include "DummyPort.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "WithCommandQueue.h"
#include "MidiBuffer.h"
#include "IMidiReadableBuffer.h"
#include "IMidiWriteableBuffer.h"
#include "types.h"
#include "backend_rust/src/dummy_midi_port_cxx.rs.h"  // Rust CXX bridge for DummyMidiPort
#include <memory>
#include <set>
#include <thread>
#include <vector>
#include <stdint.h>
#include <mutex>

/**
 * DummyMidiPort - A MIDI port implementation for testing and development.
 * 
 * This is a thin C++ wrapper that delegates to the Rust implementation
 * in src/rust/backend_rust/src/dummy_midi_port.rs via the CXX bridge.
 */
class DummyMidiPort : public MidiPort,
                      public DummyPort,
                      public MidiReadableBuffer,
                      public MidiWriteableBuffer,
                      public WithCommandQueue,
                      private ModuleLoggingEnabled<"Backend.DummyMidiPort"> {
public:
    using StoredMessage = MidiStorageElem;

private:

    // Rust implementation (delegates all logic here)
    rust::Box<backend_rust::DummyMidiPort> m_rust;

public:
    // MidiReadableBuffer interface implementation
    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;
    
    // MidiWriteableBuffer interface implementation
    void write_event(MidiStorageElem event) override;
    
    // Buffer interface accessors
    IMidiReadableBuffer *get_readable_buffer() override;
    IMidiWriteableBuffer *get_writeable_buffer() override;
    
    // Buffer accessors for port
    MidiWriteableBuffer *PROC_get_write_data_into_port_buffer(uint32_t n_frames) override;
    MidiReadableBuffer *PROC_get_read_output_data_buffer(uint32_t n_frames) override;

    DummyMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>()
    );

    void queue_msg(uint32_t size, uint32_t time, const uint8_t* data);
    bool get_queue_empty();

    void clear_queues();

    // Request a certain number of frames to be stored.
    // Not allowed if previous request was not yet completed.
    void request_data(uint32_t n_frames);
    // Dequeue messages written during the requested period.
    // Timestamps are relative to when the request was made.
    std::vector<StoredMessage> get_written_requested_msgs();

    ~DummyMidiPort() override;

    // PortInterface methods (from DummyPort base, but need explicit override due to MidiPort also having these)
    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return true; }
    bool has_implicit_output_sink() const override { return true; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;

    // Override from MidiPort to delegate to our Rust implementation
    void set_muted(bool muted) override;
    bool get_muted() const override;
};
