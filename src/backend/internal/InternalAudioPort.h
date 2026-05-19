#pragma once
#include "RustAudioPort.h"
#include "backend_rust/src/audio_port_cxx.rs.h"
#include <vector>
#include "shoop_shared_ptr.h"

class InternalAudioPort : public RustAudioPortF32 {
    std::string m_name = "";
    std::vector<float> m_buffer;
    unsigned m_input_connectability = 0;
    unsigned m_output_connectability = 0;

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
};