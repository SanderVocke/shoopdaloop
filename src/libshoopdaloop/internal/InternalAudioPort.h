#pragma once
#include "AudioPortInterface.h"
#include <vector>

template<typename SampleT>
class InternalAudioPort : public AudioPortInterface<SampleT> {
    std::string m_name;
    PortDirection m_direction;
    std::vector<SampleT> m_buffer;

public:
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
    InternalAudioPort(
        std::string name,
        PortDirection direction,
        size_t n_frames
    );
    
    SampleT *PROC_get_buffer(size_t n_frames, bool do_zero=false) override;

    const char* name() const override;
    PortDirection direction() const override;
    void reallocate_buffer(size_t n_frames);
    size_t buffer_size() const;
    void close() override;
    PortType type() const override;
    void zero();

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
};

#ifndef IMPLEMENT_INTERNALAUDIOPORT_H
extern template class InternalAudioPort<float>;
extern template class InternalAudioPort<int>;
#endif