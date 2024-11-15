#pragma once
#include "GraphPort.h"
#include "shoop_shared_ptr.h"

class GraphMidiPort : public GraphPort {
    const shoop_shared_ptr<MidiPort> port = nullptr;
public:
    GraphMidiPort (shoop_shared_ptr<MidiPort> const& port,
                   shoop_shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    MidiPort *maybe_midi_port() const override;
    shoop_shared_ptr<PortInterface> maybe_shared_port() const override;
    shoop_shared_ptr<MidiPort> maybe_shared_midi_port() const override;
    void PROC_internal_connections(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};