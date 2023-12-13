#pragma once
#include <memory>
#include <vector>
#include <atomic>
#include "shoop_globals.h"
#include "GraphNode.h"

class GraphLoopChannel : public std::enable_shared_from_this<GraphLoopChannel>,
                         public HasTwoGraphNodes {
public:
    std::shared_ptr<ChannelInterface> channel;
    std::weak_ptr<GraphLoop> loop;
    std::weak_ptr<GraphPort> mp_input_port_mapping;
    std::weak_ptr<BackendSession> backend;
    std::weak_ptr<GraphPort> mp_output_port_mapping;
    std::atomic<unsigned> ma_data_sequence_nr;

    GraphLoopChannel(std::shared_ptr<ChannelInterface> chan,
                std::shared_ptr<GraphLoop> loop,
                std::shared_ptr<BackendSession> backend);

    // NOTE: only use on process thread
    GraphLoopChannel &operator= (GraphLoopChannel const& other);

    void connect_output_port(std::shared_ptr<GraphPort> port, bool thread_safe=true);
    void connect_input_port(std::shared_ptr<GraphPort> port, bool thread_safe=true);
    void disconnect_output_port(std::shared_ptr<GraphPort> port, bool thread_safe=true);
    void disconnect_output_ports(bool thread_safe=true);
    void disconnect_input_port(std::shared_ptr<GraphPort> port, bool thread_safe=true);
    void disconnect_input_ports(bool thread_safe=true);

    void PROC_prepare(uint32_t nframes);
    void PROC_process(uint32_t nframes);

    BackendSession &get_backend();
    void clear_data_dirty();
    bool get_data_dirty() const;
    
    shoop_types::LoopAudioChannel* maybe_audio();
    shoop_types::LoopMidiChannel* maybe_midi();

    // Our first graph node is for connecting to the prepared buffers of our
    // connected ports. It has incoming edges from the praparation steps of 
    // each connected port.
    void graph_node_0_process(uint32_t nframes) override;
    WeakGraphNodeSet graph_node_0_incoming_edges() override;
    WeakGraphNodeSet graph_node_0_outgoing_edges() override;
    std::string graph_node_0_name() const override { return "channel::prepare_buffers"; }

    // Our second graph node is for anything that needs to happen after loop
    // processing. Currently that is nothing, but the node is still useful for
    // e.g. ports that come after to connect to in the graph.
    // An incoming connection is made to the loop to ensure the loop processing happens first.
    // Also an incoming connection is made from our own first node.
    // Outgoing connections are made to the connected output port(s)' post-processing.
    void graph_node_1_process(uint32_t nframes) override;
    WeakGraphNodeSet graph_node_1_incoming_edges() override;
    WeakGraphNodeSet graph_node_1_outgoing_edges() override;
    std::string graph_node_1_name() const override { return "channel::process"; }
};