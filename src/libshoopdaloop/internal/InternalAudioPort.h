#pragma once
#include "AudioPort.h"
#include <vector>

template<typename SampleT>
class InternalAudioPort : public AudioPort<SampleT> {
    std::string m_name;
    shoop_port_direction_t m_direction;
    std::vector<SampleT> m_buffer;

public:
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
    InternalAudioPort(
        std::string name,
        shoop_port_direction_t direction,
        uint32_t n_frames
    );
    
    SampleT *PROC_get_buffer(uint32_t n_frames) const override;

    const char* name() const override;
    shoop_port_direction_t direction() const override;
    void reallocate_buffer(uint32_t n_frames);
    uint32_t buffer_size() const;
    void close() override;
    PortDataType type() const override;
    void zero();
    void* maybe_driver_handle() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    void PROC_change_buffer_size(uint32_t buffer_size) override;
};

extern template class InternalAudioPort<float>;
extern template class InternalAudioPort<int>;