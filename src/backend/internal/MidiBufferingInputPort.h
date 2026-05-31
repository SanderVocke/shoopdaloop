#pragma once
#include "MidiPort.h"
#include "IMidiReadableBuffer.h"
#include "MidiBuffer.h"
#include "backend_rust/src/midi_buffering_input_port_cxx.rs.h"
#include <memory>
#include <stdint.h>

/**
 * MidiBufferingInputPort - A MIDI port that buffers messages from another input source.
 * 
 * This is a thin C++ wrapper that delegates to the Rust implementation
 * in src/rust/backend_rust/src/midi_buffering_input_port.rs via the CXX bridge.
 * 
 * Buffers MIDI messages so they can be passed on and sorted downstream.
 */
class MidiBufferingInputPort : public MidiPort,
                               protected MidiReadableBuffer,
                               private ModuleLoggingEnabled<"Backend.MidiBufferingInputPort"> {
    // Rust implementation for buffering logic
    rust::Box<backend_rust::MidiBufferingInputPort> m_rust;

public:
    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;
    
    IMidiReadableBuffer *get_readable_buffer() override;
    
    MidiBufferingInputPort(uint32_t reserve_size=1024);
    virtual ~MidiBufferingInputPort() {}

    MidiReadableBuffer *PROC_get_read_output_data_buffer(uint32_t nframes) override;

    void PROC_process(uint32_t nframes) override;
    void PROC_prepare(uint32_t nframes) override;
};