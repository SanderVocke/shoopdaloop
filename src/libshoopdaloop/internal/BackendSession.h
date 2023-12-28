#pragma once
#include <memory>
#include <vector>
#include "LoggingEnabled.h"
#include "AudioMidiDriver.h"
#include "WithCommandQueue.h"
#include "shoop_globals.h"
#include "types.h"
#include <set>
#include "GraphNode.h"

namespace profiling {
class Profiler;
class ProfilingItem;
}

class GraphLoop;
class GraphPort;
class GraphFXChain;
class GraphAudioPort;
class GraphMidiPort;

using namespace shoop_types;

class BackendSession : public std::enable_shared_from_this<BackendSession>,
                       public HasAudioProcessingFunction,
                       public WithCommandQueue,
                       public ModuleLoggingEnabled<"Backend.Session"> {

    void PROC_process_decoupled_midi_ports(uint32_t nframes);
    void recalculate_processing_schedule(unsigned update_id);

    enum class State {
        Active,
        Destroyed
    };
    std::atomic<State> ma_state;

    struct RecalculateGraphThread;
    friend class RecalculateGraphThread;
    std::unique_ptr<RecalculateGraphThread> m_recalculate_graph_thread;

public:
    // Graph nodes
    std::vector<std::shared_ptr<GraphLoop>> loops;
    std::vector<std::shared_ptr<GraphPort>> ports;
    std::vector<std::shared_ptr<GraphFXChain>> fx_chains;
    // Infrastructure
    std::shared_ptr<AudioBufferPool> audio_buffer_pool;

    // Metadata
    std::atomic<uint32_t> m_sample_rate;
    std::atomic<uint32_t> m_buffer_size;

    // Profiling
    std::shared_ptr<profiling::Profiler> profiler;
    std::shared_ptr<profiling::ProfilingItem> top_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> graph_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> cmds_profiling_item;

    // For updating the graph. When node changes are pending, change
    // the update_id. The process thread will trigger a recalculation
    // of the graph on a dedicated thread, after which the schedule
    // is applied and the current id is set to the new id.
    std::atomic<unsigned> ma_graph_request_id;
    std::atomic<unsigned> ma_graph_id;

    BackendSession();
    ~BackendSession();

    struct ProcessingStep {
        std::set<std::shared_ptr<GraphNode>> nodes;
    };
    struct ProcessingSchedule : public std::enable_shared_from_this<ProcessingSchedule> {
        std::vector<std::shared_ptr<GraphLoop>> loops;
        std::vector<std::shared_ptr<GraphPort>> ports;
        std::vector<std::shared_ptr<GraphFXChain>> fx_chains;
        std::vector<ProcessingStep> steps;
        WeakGraphNodeSet loop_graph_nodes;
    };
    std::shared_ptr<ProcessingSchedule> m_processing_schedule = std::make_shared<ProcessingSchedule>();

    void PROC_process(uint32_t nframes) override;

    shoop_backend_session_state_info_t get_state();

    std::shared_ptr<GraphLoop> create_loop();
    std::shared_ptr<GraphFXChain> create_fx_chain(shoop_fx_chain_type_t type, const char *title);
    std::shared_ptr<GraphAudioPort> add_audio_port(std::shared_ptr<shoop_types::_AudioPort> port);
    std::shared_ptr<GraphMidiPort> add_midi_port(std::shared_ptr<MidiPort> port);
    std::shared_ptr<GraphLoopChannel> add_loop_channel(std::shared_ptr<GraphLoop> loop, std::shared_ptr<ChannelInterface> channel);

    void set_sample_rate(uint32_t sr);
    void set_buffer_size(uint32_t bs);

    void destroy();

    void set_graph_node_changes_pending();
    void wait_graph_up_to_date();
};