#pragma once
#include "GraphPort.h"
#include "RustAudioPort.h"
#include <memory>

class GraphAudioPort : public GraphPort {
    const std::shared_ptr<RustAudioPortF32> port = nullptr;
public:
    GraphAudioPort (std::shared_ptr<RustAudioPortF32> const& port,
                    std::shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    RustAudioPortF32 *maybe_audio_port() const override;
    std::shared_ptr<PortInterface> maybe_shared_port() const override;
    std::shared_ptr<RustAudioPortF32> maybe_shared_audio_port() const override;
    void PROC_internal_connections(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};