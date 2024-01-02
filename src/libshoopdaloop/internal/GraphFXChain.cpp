#include "GraphFXChain.h"
#include "GraphAudioPort.h"
#include "GraphMidiPort.h"
#include "GraphPort.h"
#include "GraphLoopChannel.h"
#include "ProcessingChainInterface.h"
#include "GraphNode.h"
#include "shoop_globals.h"

using namespace shoop_types;
using namespace shoop_constants;

    GraphFXChain::GraphFXChain(std::shared_ptr<FXChain> chain, std::shared_ptr<BackendSession> backend) :
        chain(chain), backend(backend) {
        for (auto const& port : chain->input_audio_ports()) {
            mc_audio_input_ports.push_back(std::make_shared<GraphAudioPort>(port, backend));
        }
        for (auto const& port : chain->output_audio_ports()) {
            mc_audio_output_ports.push_back(std::make_shared<GraphAudioPort>(port, backend));
        }
        for (auto const& port : chain->input_midi_ports()) {
            mc_midi_input_ports.push_back(std::make_shared<GraphMidiPort>(port, backend));
        }
    }

    std::vector<std::shared_ptr<GraphPort>> const& GraphFXChain::audio_input_ports() const { return mc_audio_input_ports; }
    std::vector<std::shared_ptr<GraphPort>> const& GraphFXChain::audio_output_ports() const { return mc_audio_output_ports; }
    std::vector<std::shared_ptr<GraphPort>> const& GraphFXChain::midi_input_ports() const { return mc_midi_input_ports; }

BackendSession &GraphFXChain::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void GraphFXChain::graph_node_process(uint32_t nframes) {
    if (chain) {
        chain->process(nframes);
    }
}

WeakGraphNodeSet GraphFXChain::graph_node_incoming_edges() {
    WeakGraphNodeSet rval;
    for (auto &p : mc_audio_input_ports) {
        rval.insert(p->second_graph_node());
    }
    for (auto &p : mc_midi_input_ports) {
        rval.insert(p->second_graph_node());
    }
    return rval;
}

WeakGraphNodeSet GraphFXChain::graph_node_outgoing_edges() {
    WeakGraphNodeSet rval;
    for (auto &p : mc_audio_output_ports) {
        rval.insert(p->second_graph_node());
    }
    return rval;
}