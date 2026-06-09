#pragma once
#include <atomic>
#include <jack/types.h>
#include "JackPort.h"
#include "../RustAudioPort.h"
#include "types.h"
#include "backend_rust/src/jack_api_cxx.rs.h"

class JackAudioPort : public virtual RustAudioPortF32, public virtual JackPort {
    using JackPort::m_buffer;
    using JackPort::m_direction;

    std::vector<jack_default_audio_sample_t> m_fallback_buffer;
public:
    JackAudioPort(
        std::string name,
        shoop_port_direction_t direction,
        uintptr_t client,
        std::shared_ptr<JackAllPorts> all_ports_tracker,
        rust::Box<backend_rust::JackApiBridgeStrong> api,
        std::shared_ptr<RustAudioPortF32::UsedBufferPool> maybe_ringbuffer_buffer_pool
    );

    // Prepare step is used to get the buffer from JACK
    void PROC_prepare(uint32_t) override;
    void PROC_process(uint32_t) override;

    // Access the cached buffer.
    float *PROC_get_buffer(uint32_t n_frames) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;

    // Override PortInterface methods to resolve diamond inheritance
    PortDataType type() const override { return RustAudioPortF32::type(); }
    const char* name() const override { return JackPort::name(); }
    void close() override { JackPort::close(); }
    void* maybe_driver_handle() const override { return JackPort::maybe_driver_handle(); }
    bool has_internal_read_access() const override { return JackPort::has_internal_read_access(); }
    bool has_internal_write_access() const override { return JackPort::has_internal_write_access(); }
    bool has_implicit_input_source() const override { return JackPort::has_implicit_input_source(); }
    bool has_implicit_output_sink() const override { return JackPort::has_implicit_output_sink(); }
    PortExternalConnectionStatus get_external_connection_status() const override { return JackPort::get_external_connection_status(); }
    void connect_external(std::string name) override { JackPort::connect_external(name); }
    void disconnect_external(std::string name) override { JackPort::disconnect_external(name); }
};

using JackTestAudioPort = JackAudioPort;
