#pragma once
#include <cstdint>
#include <memory>
#include <vector>
#include <atomic>
#include "LoggingEnabled.h"
#include "shoop_globals.h"
#include "GraphNode.h"
#include "PortInterface.h"

class ConnectedPort : public std::enable_shared_from_this<ConnectedPort>,
                      public HasTwoGraphNodes,
                      private ModuleLoggingEnabled<"Backend.ConnectedPort"> {
public:
    
    const std::shared_ptr<PortInterface> port;
    std::weak_ptr<BackendSession> backend;

    std::vector<std::weak_ptr<ConnectedPort>> mp_passthrough_to;

    // Audio only
    shoop_types::audio_sample_t* maybe_audio_buffer;
    std::atomic<float> volume;
    std::atomic<float> peak;

    // Dummy audio only (testing purposes)
    std::vector<float> stored_dummy_data;

    // Midi only
    MidiWriteableBufferInterface *maybe_midi_input_buffer;
    MidiReadableBufferInterface *maybe_midi_output_buffer;
    std::shared_ptr<MidiSortingBuffer> maybe_midi_output_merging_buffer;
    std::atomic<uint32_t> n_events_processed;
    std::shared_ptr<MidiStateTracker> maybe_midi_state;

    // Both
    std::atomic<bool> muted;
    std::atomic<bool> passthrough_muted;

    ConnectedPort (std::shared_ptr<PortInterface> const& port,
                   std::shared_ptr<BackendSession> const& backend);

    void PROC_reset_buffers();
    void PROC_passthrough(uint32_t n_frames);
    void PROC_passthrough_audio(uint32_t n_frames, ConnectedPort &to);
    void PROC_passthrough_midi(uint32_t n_frames, ConnectedPort &to);
    bool PROC_check_buffer(bool raise_if_absent=true);
    void PROC_process_buffer(uint32_t n_frames);

    void PROC_notify_changed_buffer_size(uint32_t buffer_size) override;

    void connect_passthrough(std::shared_ptr<ConnectedPort> const& other);

    std::shared_ptr<shoop_types::_AudioPort> maybe_audio();
    std::shared_ptr<MidiPort> maybe_midi();
    BackendSession &get_backend();

    // The first graph node we encapsulate is for preparing/creating
    // our buffer.
    // PRECONDITION:
    //  - None
    // POSTCONDITION:
    //  - maybe_audio_buffer is set to a valid buffer
    // CONNECTIONS:
    //  - Out: to our own second node (handled below in second node)
    //  - In: from any port that wants to passthrough to us (2nd node).
    //        Handled by those ports, not here.
    void graph_node_0_process(uint32_t nframes) override;
    std::string graph_node_0_name() const override {
        if (port) { return std::string(port->name()) + "::prepare"; }
        return "unknown_port::prepare";
    }
    
    // The second graph node we represent is for processing the buffer
    // contents and passing it through to any passthrough targets.
    // PRECONDITION:
    //  - maybe_audio_buffer is set to a valid buffer
    //  - anything producing data into our buffer should be finished
    //    (loops, channels, fx...)
    // POSTCONDITION:
    //  - data is ready to consume by others
    //  - data has been pushed to passthrough targets (they can also
    //    call their 2nd graph node).
    // CONNECTIONS:
    //  - In: from our first node. Handled in our first node.
    //  - In: from the first nodes of all passthrough targets (buffers should
    //        be ready before we can pass through to them)
    //  - Out: to the process nodes of all passthrough targets.
    void graph_node_1_process(uint32_t nframes) override;
    WeakGraphNodeSet graph_node_1_incoming_edges() override;
    WeakGraphNodeSet graph_node_1_outgoing_edges() override;
    std::string graph_node_1_name() const override {
        if (port) { return std::string(port->name()) + "::process_and_passthrough"; }
        return "unknown_port::process_and_passthrough";
    }
};