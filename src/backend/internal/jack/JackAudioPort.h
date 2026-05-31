#pragma once
#include <jack/types.h>
#include "JackApi.h"
#include "JackTestApi.h"
#include "JackPort.h"
#include "RustAudioPort.h"
#include "types.h"
#include <atomic>

template<typename API>
class GenericJackAudioPort : public virtual RustAudioPortF32, public virtual GenericJackPort<API> {
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;
    using GenericJackPort<API>::m_direction;

    std::vector<jack_default_audio_sample_t> m_fallback_buffer;
public:
    GenericJackAudioPort(
        std::string name,
        shoop_port_direction_t direction,
        jack_client_t *client,
        shoop_shared_ptr<GenericJackAllPorts<API>> all_ports_tracker,
        shoop_shared_ptr<RustAudioPortF32::UsedBufferPool> maybe_ringbuffer_buffer_pool
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
    const char* name() const override { return GenericJackPort<API>::name(); }
    void close() override { GenericJackPort<API>::close(); }
    void* maybe_driver_handle() const override { return GenericJackPort<API>::maybe_driver_handle(); }
    bool has_internal_read_access() const override { return GenericJackPort<API>::has_internal_read_access(); }
    bool has_internal_write_access() const override { return GenericJackPort<API>::has_internal_write_access(); }
    bool has_implicit_input_source() const override { return GenericJackPort<API>::has_implicit_input_source(); }
    bool has_implicit_output_sink() const override { return GenericJackPort<API>::has_implicit_output_sink(); }
    PortExternalConnectionStatus get_external_connection_status() const override { return GenericJackPort<API>::get_external_connection_status(); }
    void connect_external(std::string name) override { GenericJackPort<API>::connect_external(name); }
    void disconnect_external(std::string name) override { GenericJackPort<API>::disconnect_external(name); }
};

using JackAudioPort = GenericJackAudioPort<JackApi>;
using JackTestAudioPort = GenericJackAudioPort<JackTestApi>;

extern template class GenericJackAudioPort<JackApi>;
extern template class GenericJackAudioPort<JackTestApi>;