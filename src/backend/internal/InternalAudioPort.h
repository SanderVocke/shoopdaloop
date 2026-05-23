#pragma once
#include "RustAudioPort.h"
#include "backend_rust/src/audio_port_cxx.rs.h"
#include "backend_rust/src/internal_audio_port_cxx.rs.h"
#include <vector>
#include "shoop_shared_ptr.h"

class InternalAudioPort : public RustAudioPortF32 {
    rust::Box<backend_rust::InternalAudioPort> m_rust_internal;
    std::string m_name;  // Cached name to avoid dangling pointer from Rust String

public:
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
    InternalAudioPort(
        std::string name,
        uint32_t n_frames,
        unsigned input_connectability,
        unsigned output_connectability,
        shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> maybe_ringbuffer_buffer_pool
    );
    
    float* PROC_get_buffer(uint32_t n_frames) override;

    const char* name() const override;
    void close() override;
    PortDataType type() const override;
    void* maybe_driver_handle() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return false; }
    bool has_implicit_output_sink() const override { return false; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
    
    // Gain/mute/peak control - delegate to Rust InternalAudioPort
    void set_gain(float gain) override;
    float get_gain() const override;
    void set_muted(bool muted) override;
    bool get_muted() const override;
    float get_input_peak() const override;
    void reset_input_peak() override;
    float get_output_peak() const override;
    void reset_output_peak() override;
    
    // Ringbuffer config - delegate to Rust InternalAudioPort
    void set_ringbuffer_n_samples(unsigned n) override;
    unsigned get_ringbuffer_n_samples() const override;
    
    // Ringbuffer access - delegate to Rust InternalAudioPort
    RingbufferSnapshot PROC_get_ringbuffer_contents();
};