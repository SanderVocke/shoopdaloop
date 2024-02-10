#pragma once
#include "GraphPort.h"

class GraphAudioPort : public GraphPort {
    const std::shared_ptr<shoop_types::_AudioPort> port = nullptr;
public:
    GraphAudioPort (std::shared_ptr<shoop_types::_AudioPort> const& port,
                        std::shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    shoop_types::_AudioPort *maybe_audio_port() const override;
    void PROC_passthrough(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};