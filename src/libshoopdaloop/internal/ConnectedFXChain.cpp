#include "ConnectedFXChain.h"
#include "ConnectedPort.h"
#include "ConnectedChannel.h"
#include "CarlaLV2ProcessingChain.h"
#include "shoop_globals.h"

using namespace shoop_types;
using namespace shoop_constants;

    ConnectedFXChain::ConnectedFXChain(std::shared_ptr<FXChain> chain, std::shared_ptr<Backend> backend) :
        chain(chain), backend(backend) {
        for (auto const& port : chain->input_audio_ports()) {
            mc_audio_input_ports.push_back(std::make_shared<ConnectedPort>(port, backend));
        }
        for (auto const& port : chain->output_audio_ports()) {
            mc_audio_output_ports.push_back(std::make_shared<ConnectedPort>(port, backend));
        }
        for (auto const& port : chain->input_midi_ports()) {
            mc_midi_input_ports.push_back(std::make_shared<ConnectedPort>(port, backend));
        }
    }

    std::vector<std::shared_ptr<ConnectedPort>> const& ConnectedFXChain::audio_input_ports() const { return mc_audio_input_ports; }
    std::vector<std::shared_ptr<ConnectedPort>> const& ConnectedFXChain::audio_output_ports() const { return mc_audio_output_ports; }
    std::vector<std::shared_ptr<ConnectedPort>> const& ConnectedFXChain::midi_input_ports() const { return mc_midi_input_ports; }

Backend &ConnectedFXChain::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}