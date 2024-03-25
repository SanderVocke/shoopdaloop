#pragma once
#include "GraphPort.h"

class GraphMidiPort : public GraphPort {
    const std::shared_ptr<MidiPort> port = nullptr;
public:
    GraphMidiPort (std::shared_ptr<MidiPort> const& port,
                   std::shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    MidiPort *maybe_midi_port() const override;
    std::shared_ptr<PortInterface> maybe_shared_port() const override;
    std::shared_ptr<MidiPort> maybe_shared_midi_port() const override;
    void PROC_internal_connections(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};