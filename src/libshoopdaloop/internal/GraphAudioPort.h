#pragma once
#include "GraphPort.h"

class GraphAudioPort : public GraphPort {
    const std::shared_ptr<shoop_types::_AudioPort> port;
public:
    GraphAudioPort (std::shared_ptr<shoop_types::_AudioPort> const& port,
                        std::shared_ptr<BackendSession> const& backend);

    std::shared_ptr<PortInterface> &get_port() const override;
    void PROC_passthrough(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};