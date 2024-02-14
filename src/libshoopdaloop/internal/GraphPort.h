#pragma once
#include <cstdint>
#include <memory>
#include <vector>
#include <atomic>
#include "LoggingEnabled.h"
#include "MidiPort.h"
#include "shoop_globals.h"
#include "GraphNode.h"
#include "PortInterface.h"

class GraphPort :     public HasTwoGraphNodes,
                      protected ModuleLoggingEnabled<"Backend.GraphPort"> {
public:
    
    std::weak_ptr<BackendSession> backend;
    std::vector<std::weak_ptr<GraphPort>> mp_passthrough_to;
    std::atomic<bool> m_passthrough_enabled = false;

    GraphPort (std::shared_ptr<BackendSession> const& backend);

    virtual PortInterface &get_port() const = 0;
    virtual shoop_types::_AudioPort *maybe_audio_port() const { return nullptr; };
    virtual MidiPort *maybe_midi_port() const { return nullptr; }

    virtual void PROC_passthrough(uint32_t n_frames) = 0;
    virtual void PROC_prepare(uint32_t n_frames) = 0;
    virtual void PROC_process(uint32_t n_frames) = 0;

    void set_passthrough_enabled(bool enabled) { m_passthrough_enabled = enabled; }
    bool get_passthrough_enabled() const { return m_passthrough_enabled; }

    void PROC_notify_changed_buffer_size(uint32_t buffer_size) override;

    void connect_passthrough(std::shared_ptr<GraphPort> const& other);
    BackendSession &get_backend();

    // The first graph node we encapsulate is for preparing/creating
    // our buffers and setting up externally sourced data.
    // CONNECTIONS:
    //  - Out: to our own second node (handled below in second node)
    //  - In: from any port that wants to passthrough to us (2nd node).
    //        Handled by those ports, not here.
    void graph_node_0_process(uint32_t nframes) override;
    std::string graph_node_0_name() const override {
        auto &port = get_port();
        return std::string(port.name()) + "::prepare";
    }
    
    // The second graph node we represent is for processing the buffer
    // contents and passing it through to any passthrough targets.
    // CONNECTIONS:
    //  - In: from our first node. Handled in our first node.
    //  - In: from the first nodes of all passthrough targets (buffers should
    //        be ready before we can pass through to them)
    //  - Out: to the process nodes of all passthrough targets.
    void graph_node_1_process(uint32_t nframes) override;
    WeakGraphNodeSet graph_node_1_incoming_edges() override;
    WeakGraphNodeSet graph_node_1_outgoing_edges() override;
    std::string graph_node_1_name() const override {
        auto &port = get_port();
        return std::string(port.name()) + "::process_and_passthrough";
    }
};