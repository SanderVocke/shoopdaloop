#pragma once
#include "AudioMidiDriver.h"
#include "MidiPort.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "MidiBuffer.h"
#include "IMidiReadableBuffer.h"
#include "IMidiWriteableBuffer.h"
#include "types.h"
#include "backend_rust/src/internal_midi_port_cxx.rs.h"
#include <memory>
#include <set>
#include <thread>
#include <vector>
#include <stdint.h>
#include <mutex>

class InternalMidiPort : public MidiPort,
                         public MidiReadableBuffer,
                         public MidiWriteableBuffer,
                         private ModuleLoggingEnabled<"Backend.InternalMidiPort"> {
    rust::Box<backend_rust::InternalMidiPort> m_rust_internal;

public:
    InternalMidiPort(
        std::string name,
        unsigned input_connectability,
        unsigned output_connectability,
        unsigned ringbuffer_n_samples_hint = 0
    );

    // MidiReadableBuffer
    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;

    // MidiWriteableBuffer
    void write_event(MidiStorageElem event) override;

    // MidiPort buffer accessors — return this so MidiPort::PROC_prepare stores our buffer pointers
    MidiWriteableBuffer *PROC_get_write_data_into_port_buffer(uint32_t n_frames) override;
    MidiReadableBuffer *PROC_get_read_output_data_buffer(uint32_t n_frames) override;
    MidiReadableBuffer *PROC_internal_read_input_data_buffer(uint32_t n_frames) override;
    MidiWriteableBuffer *PROC_internal_write_output_data_to_buffer(uint32_t n_frames) override;

    // Processing
    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    // PortInterface
    const char* name() const override;
    PortDataType type() const override;
    void close() override;
    void* maybe_driver_handle() const override;
    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return false; }
    bool has_implicit_output_sink() const override { return false; }
    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
};
