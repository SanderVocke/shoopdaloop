#pragma once
#include "GraphPort.h"

class GraphMidiPort : public GraphPort {
    const std::shared_ptr<MidiPort> port;
public:
    GraphMidiPort (std::shared_ptr<MidiPort> const& port,
                   std::shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    MidiPort *maybe_midi_port() const override;
    void PROC_passthrough(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};