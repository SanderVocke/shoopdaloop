#pragma once
#include <memory>
#include <vector>
#include "LoggingEnabled.h"
#include "CommandQueue.h"
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

class ConnectedLoop;
class ConnectedPort;
class ConnectedFXChain;

using namespace shoop_types;

class BackendSession : public std::enable_shared_from_this<BackendSession>,
                       public HasAudioProcessingFunction,
                       public WithCommandQueue,
                       public ModuleLoggingEnabled<"Backend.Session"> {

    void PROC_process_decoupled_midi_ports(uint32_t nframes);

    enum class State {
        Active,
        Destroyed
    };
    std::atomic<State> ma_state;

public:
    // Graph nodes
    std::vector<std::shared_ptr<ConnectedLoop>> loops;
    std::vector<std::shared_ptr<ConnectedPort>> ports;
    std::vector<std::shared_ptr<ConnectedFXChain>> fx_chains;
    // Infrastructure
    std::shared_ptr<AudioBufferPool> audio_buffer_pool;

    // Metadata
    std::atomic<uint32_t> m_sample_rate;
    std::atomic<uint32_t> m_buffer_size;

    // Profiling
    std::shared_ptr<profiling::Profiler> profiler;
    std::shared_ptr<profiling::ProfilingItem> top_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> ports_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> ports_prepare_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> ports_finalize_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> ports_prepare_fx_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> ports_passthrough_profiling_item;
    std::shared_ptr<profiling::ProfilingItem>
        ports_passthrough_input_profiling_item;
    std::shared_ptr<profiling::ProfilingItem>
        ports_passthrough_output_profiling_item;
    std::shared_ptr<profiling::ProfilingItem>
        ports_decoupled_midi_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> loops_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> fx_profiling_item;
    std::shared_ptr<profiling::ProfilingItem> cmds_profiling_item;

    BackendSession();
    ~BackendSession();

    struct ProcessingStep {
        std::set<std::shared_ptr<GraphNode>> nodes;
        std::shared_ptr<profiling::ProfilingItem> profiling_item;
    };
    struct ProcessingSchedule {
        std::vector<ProcessingStep> steps;
        WeakGraphNodeSet loop_graph_nodes;
    };
    std::shared_ptr<ProcessingSchedule> m_processing_schedule = std::make_shared<ProcessingSchedule>();

    void PROC_process(uint32_t nframes) override;

    shoop_backend_session_state_info_t get_state();
    WeakGraphNodeSet const& get_loop_graph_nodes();

    std::shared_ptr<ConnectedLoop> create_loop();
    std::shared_ptr<ConnectedFXChain> create_fx_chain(shoop_fx_chain_type_t type, const char *title);

    void set_sample_rate(uint32_t sr);
    void set_buffer_size(uint32_t bs);

    void destroy();
    void recalculate_processing_schedule(bool thread_safe=true);
};