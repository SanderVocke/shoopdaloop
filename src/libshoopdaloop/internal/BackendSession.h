#pragma once
#include <memory>
#include <vector>
#include "LoggingEnabled.h"
#include "CommandQueue.h"
#include "AudioMidiDriver.h"
#include "shoop_globals.h"
#include "types.h"

namespace profiling {
class Profiler;
class ProfilingItem;
}

class ConnectedLoop;
class ConnectedPort;
class ConnectedFXChain;
class ConnectedDecoupledMidiPort;

using namespace shoop_types;

class BackendSession : public std::enable_shared_from_this<BackendSession>,
                 public ModuleLoggingEnabled<"Backend.Session"> {

public:
    enum class State {
        NotStarted,
        Running,
        Terminated
    };
    std::atomic<State> ma_state;

    // Graph nodes
    std::vector<std::shared_ptr<ConnectedLoop>> loops;
    std::vector<std::shared_ptr<ConnectedPort>> ports;
    std::vector<std::shared_ptr<ConnectedFXChain>> fx_chains;
    std::vector<std::shared_ptr<ConnectedDecoupledMidiPort>> decoupled_midi_ports;

    // Infrastructure
    CommandQueue cmd_queue;
    std::shared_ptr<AudioBufferPool> audio_buffer_pool;

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

    ProcessFunctionResult PROC_process(uint32_t nframes) override;

    void PROC_process_decoupled_midi_ports(uint32_t nframes);
    void terminate();
    void* maybe_jack_client_handle();
    const char *get_client_name();
    unsigned get_sample_rate();
    unsigned get_buffer_size();
    shoop_backend_session_state_info_t get_state();
    std::shared_ptr<ConnectedLoop> create_loop();
    std::shared_ptr<ConnectedFXChain> create_fx_chain(shoop_fx_chain_type_t type, const char *title);
    void start();
};