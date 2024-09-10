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
#include "shoop_shared_ptr.h"

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

class BackendSession : public shoop_enable_shared_from_this<BackendSession>,
                       public HasAudioProcessingFunction,
                       public WithCommandQueue,
                       public ModuleLoggingEnabled<"Backend.Session"> {
    void recalculate_processing_schedule(unsigned update_id);

    enum class State {
        Active,
        Destroyed
    };
    std::atomic<State> ma_state = State::Active;

    struct RecalculateGraphThread;
    friend class RecalculateGraphThread;
    std::unique_ptr<RecalculateGraphThread> m_recalculate_graph_thread;

public:
    // Graph nodes
    std::vector<shoop_shared_ptr<GraphLoop>> loops;
    std::vector<shoop_shared_ptr<GraphPort>> ports;
    std::vector<shoop_shared_ptr<GraphFXChain>> fx_chains;
    // Infrastructure
    shoop_shared_ptr<AudioBufferPool> audio_buffer_pool = nullptr;

    // Metadata
    std::atomic<uint32_t> m_sample_rate = 1;
    std::atomic<uint32_t> m_buffer_size = 0;

    // Profiling
    shoop_shared_ptr<profiling::Profiler> profiler = nullptr;
    shoop_shared_ptr<profiling::ProfilingItem> top_profiling_item = nullptr;
    shoop_shared_ptr<profiling::ProfilingItem> graph_profiling_item = nullptr;
    shoop_shared_ptr<profiling::ProfilingItem> cmds_profiling_item = nullptr;

    // For updating the graph. When node changes are pending, change
    // the update_id. The process thread will trigger a recalculation
    // of the graph on a dedicated thread, after which the schedule
    // is applied and the current id is set to the new id.
    std::atomic<unsigned> ma_graph_request_id = 0;
    std::atomic<unsigned> ma_graph_id = 0;

    BackendSession();
    ~BackendSession();

    struct ProcessingStep {
        std::set<shoop_shared_ptr<GraphNode>> nodes;
    };
    struct ProcessingSchedule : public shoop_enable_shared_from_this<ProcessingSchedule> {
        std::vector<shoop_shared_ptr<GraphLoop>> loops;
        std::vector<shoop_shared_ptr<GraphPort>> ports;
        std::vector<shoop_shared_ptr<GraphFXChain>> fx_chains;
        std::vector<ProcessingStep> steps;
        WeakGraphNodeSet loop_graph_nodes;
    };
    shoop_shared_ptr<ProcessingSchedule> m_processing_schedule = shoop_make_shared<ProcessingSchedule>();

    void PROC_process(uint32_t nframes) override;

    shoop_backend_session_state_info_t get_state();

    shoop_shared_ptr<GraphLoop> create_loop();
    shoop_shared_ptr<GraphFXChain> create_fx_chain(shoop_fx_chain_type_t type, const char *title);
    shoop_shared_ptr<GraphAudioPort> add_audio_port(shoop_shared_ptr<shoop_types::_AudioPort> port);
    shoop_shared_ptr<GraphMidiPort> add_midi_port(shoop_shared_ptr<MidiPort> port);
    shoop_shared_ptr<GraphLoopChannel> add_loop_channel(shoop_shared_ptr<GraphLoop> loop, shoop_shared_ptr<ChannelInterface> channel);

    void set_sample_rate(uint32_t sr);
    void set_buffer_size(uint32_t bs);

    void destroy();

    void set_graph_node_changes_pending();
    void wait_graph_up_to_date();
};