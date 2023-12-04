#pragma once
#include <vector>
#include <memory>
#include "shoop_globals.h"

class ConnectedFXChain : public std::enable_shared_from_this<ConnectedFXChain> {
public:
    const std::shared_ptr<shoop_types::FXChain> chain;
    std::weak_ptr<BackendSession> backend;

    std::vector<std::shared_ptr<ConnectedPort>> mc_audio_input_ports;
    std::vector<std::shared_ptr<ConnectedPort>> mc_audio_output_ports;
    std::vector<std::shared_ptr<ConnectedPort>> mc_midi_input_ports;

    ConnectedFXChain(std::shared_ptr<shoop_types::FXChain> chain, std::shared_ptr<BackendSession> backend);

    BackendSession &get_backend();
    std::vector<std::shared_ptr<ConnectedPort>> const& audio_input_ports() const;
    std::vector<std::shared_ptr<ConnectedPort>> const& audio_output_ports() const;
    std::vector<std::shared_ptr<ConnectedPort>> const& midi_input_ports() const;
};