#pragma once
#include <vector>
#include <memory>
#include "shoop_globals.h"
#include "GraphNode.h"
class GraphFXChain : public std::enable_shared_from_this<GraphFXChain>,
                         public HasGraphNode {
public:
    const std::shared_ptr<shoop_types::FXChain> chain;
    std::weak_ptr<BackendSession> backend;

    std::vector<std::shared_ptr<GraphPort>> mc_audio_input_ports;
    std::vector<std::shared_ptr<GraphPort>> mc_audio_output_ports;
    std::vector<std::shared_ptr<GraphPort>> mc_midi_input_ports;

    GraphFXChain(std::shared_ptr<shoop_types::FXChain> chain, std::shared_ptr<BackendSession> backend);

    BackendSession &get_backend();
    std::vector<std::shared_ptr<GraphPort>> const& audio_input_ports() const;
    std::vector<std::shared_ptr<GraphPort>> const& audio_output_ports() const;
    std::vector<std::shared_ptr<GraphPort>> const& midi_input_ports() const;

    // Our graph node sits in-between the processing stage of all input ports
    // and the processing stage of all output ports.
    void graph_node_process(uint32_t nframes) override;
    WeakGraphNodeSet graph_node_incoming_edges() override;
    WeakGraphNodeSet graph_node_outgoing_edges() override;
    std::string graph_node_name() const override { return "fxchain::process"; }
};