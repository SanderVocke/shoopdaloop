#include "BackendSession.h"
#include "ConnectedChannel.h"
#include "Backend.h"
#include "GraphNode.h"
#include "LoggingBackend.h"
#include "ProcessProfiling.h"
#include "ConnectedPort.h"
#include "ConnectedFXChain.h"
#include "ProcessingChainInterface.h"
#include "ConnectedLoop.h"
#include "AudioBuffer.h"
#include "ObjectPool.h"
#include "AudioMidiLoop.h"
#include "graph_dot.h"
#include "shoop_globals.h"
#include "types.h"
#include "graph_processing_order.h"
#include <chrono>

#ifdef SHOOP_HAVE_BACKEND_JACK
#include <jack_wrappers.h>
#endif

#ifdef SHOOP_HAVE_LV2
#include "LV2.h"
#endif

using namespace logging;
using namespace shoop_types;
using namespace shoop_constants;

BackendSession::BackendSession() :
          WithCommandQueue(),
          profiler(std::make_shared<profiling::Profiler>()),
          top_profiling_item(profiler->maybe_get_profiling_item("Process")),
          ports_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports")),
          ports_prepare_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports.Prepare")),
          ports_finalize_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports.Finalize")),
          ports_prepare_fx_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports.PrepareFX")),
          ports_passthrough_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports.Passthrough")),
          ports_passthrough_input_profiling_item(
              profiler->maybe_get_profiling_item(
                  "Process.Ports.Passthrough.Input")),
          ports_passthrough_output_profiling_item(
              profiler->maybe_get_profiling_item(
                  "Process.Ports.Passthrough.Output")),
          ports_decoupled_midi_profiling_item(
              profiler->maybe_get_profiling_item("Process.Ports.MidiControl")),
          loops_profiling_item(
              profiler->maybe_get_profiling_item("Process.Loops")),
          fx_profiling_item(profiler->maybe_get_profiling_item("Process.FX")),
          cmds_profiling_item(
              profiler->maybe_get_profiling_item("Process.Commands")),
          ma_state(State::Active)
{
    audio_buffer_pool = std::make_shared<ObjectPool<AudioBuffer<shoop_types::audio_sample_t>>>(
        n_buffers_in_pool, audio_buffer_size);
    loops.reserve(initial_max_loops);
    ports.reserve(initial_max_ports);
    fx_chains.reserve(initial_max_fx_chains);
}

shoop_backend_session_state_info_t BackendSession::get_state() {
    shoop_backend_session_state_info_t rval;
    return rval;
}

//TODO delete destroyed ports
void BackendSession::PROC_process (uint32_t nframes) {
    log<log_level_trace>("Process {}: start", nframes);
    profiling::stopwatch(
        [this, &nframes]() {
            
            profiler->next_iteration();
            
            // Execute queued commands
            log<log_level_trace>("Process: execute commands");
            profiling::stopwatch(
                [this]() {
                    PROC_handle_command_queue();
                },
                cmds_profiling_item
            );
            
            log<trace>("Process: process graph");
            auto processing_schedule = m_processing_schedule;
            for(auto &step: processing_schedule->steps) {
                profiling::stopwatch(
                    [this, &nframes, &step]() {
                        if(step.nodes.size() == 1) {
                            log<trace>("Processing node: {}", step.nodes.begin()->get()->graph_node_name());
                            step.nodes.begin()->get()->graph_node_process(nframes);
                        } else if(step.nodes.size() > 1) {
                            log<trace>("Co-processing {} nodes, first: {}", step.nodes.size(), step.nodes.begin()->get()->graph_node_name());
                            step.nodes.begin()->get()->graph_node_co_process(step.nodes, nframes);
                        }
                    },
                    step.profiling_item
                );
            }
        },
        top_profiling_item
    );
}

void BackendSession::destroy() {
    if (ma_state != State::Destroyed) {
        log<log_level_debug>("Destroying backend");
        ma_queue.passthrough_on();
        for (auto &p : ports) {
            if (p) {
                p->port->close();
            }
        }
        ma_state = State::Destroyed;
    } else {
        log<log_level_debug>("Backend already destroyed");
    }
}

std::shared_ptr<ConnectedLoop> BackendSession::create_loop() {
    auto loop = std::make_shared<AudioMidiLoop>(profiler);
    auto r = std::make_shared<ConnectedLoop>(shared_from_this(), loop);
    loops.push_back(r);
    recalculate_processing_schedule();
    return r;
}

std::shared_ptr<ConnectedFXChain> BackendSession::create_fx_chain(shoop_fx_chain_type_t type, const char* title) {
#ifdef SHOOP_HAVE_LV2
    static LV2 lv2;
#endif
    std::shared_ptr<ProcessingChainInterface<Time, Size>> chain;
    switch(type) {
#ifdef SHOOP_HAVE_LV2
        case Carla_Rack:
        case Carla_Patchbay:
        case Carla_Patchbay_16x:
            try {
                chain = lv2.create_carla_chain<Time, Size>(
                    type, m_sample_rate, std::string(title), profiler
                );
            } catch (const std::exception &exp) {
                throw_error<std::runtime_error>("Failed to create a Carla LV2 instance: " + std::string(exp.what()));
            } catch (...) {
                throw_error<std::runtime_error>("Failed to create a Carla LV2 instance (unknown exception)");
            }
            break;
#else
        case Carla_Rack:
        case Carla_Patchbay:
        case Carla_Patchbay_16x:
            throw_error<std::runtime_error>("LV2 plugin hosting not available");
            break;
#endif
        case Test2x2x1:
            chain = std::make_shared<CustomProcessingChain<Time, Size>>(
                2, 2, 1, [this, &chain](uint32_t n, auto &ins, auto &outs, auto &midis) {
                    static std::vector<audio_sample_t> out_buf_1, out_buf_2;
                    out_buf_1.resize(std::max((size_t)n, out_buf_1.size()));
                    out_buf_2.resize(std::max((size_t)n, out_buf_2.size()));

                    // Audio: for any audio sample, divide it by 2.
                    auto in_1 = ins[0]->PROC_get_buffer(n);
                    auto in_2 = ins[1]->PROC_get_buffer(n);
                    for (uint32_t i = 0; i < n; i++) {
                        out_buf_1[i] = in_1[i] / 2.0f;
                        out_buf_2[i] = in_2[i] / 2.0f;
                    }

                    // Midi: for any MIDI message, synthesize a single sample on the timestamp of the message.
                    // Its value will be (3rd msg byte / 0xFF).
                    auto &midi = midis[0];
                    auto &readbuf = midi->PROC_get_read_buffer(n);
                    auto n_msgs = readbuf.PROC_get_n_events();
                    for (uint32_t i = 0; i < n_msgs; i++) {
                        auto &msg = readbuf.PROC_get_event_reference(i);
                        auto time = msg.get_time();
                        auto data = msg.get_data();
                        auto size = msg.get_size();
                        if (size >= 3) {
                            auto val = data[2] / 255.0f;
                            out_buf_1[time] += val;
                            out_buf_2[time] += val;
                        }
                    }
                    memcpy((void*)outs[0]->PROC_get_buffer(n), (void*)out_buf_1.data(), n*sizeof(float));
                    memcpy((void*)outs[1]->PROC_get_buffer(n), (void*)out_buf_2.data(), n*sizeof(float));
                }
            );
        break;
    };

    chain->ensure_buffers(m_buffer_size);
    auto info = std::make_shared<ConnectedFXChain>(chain, shared_from_this());
    fx_chains.push_back(info);
    recalculate_processing_schedule();
    return info;
}

BackendSession::~BackendSession() {
    destroy();
}

void BackendSession::set_sample_rate(uint32_t sr) {
    m_sample_rate = sr;
}

void BackendSession::set_buffer_size(uint32_t bs) {
    auto notify = [&, this](auto &item) {
        auto as_node = dynamic_cast<ProcessingNodeInterface*>(item.get());
        if (as_node) {
            as_node->PROC_change_buffer_size(bs);
        }
    };
    exec_process_thread_command([&, this]() {
        for (auto &i : loops) { notify(i); }
        for (auto &i : ports) { notify(i); }
        for (auto &i : fx_chains) { notify(i); }
        m_buffer_size = bs;
    });
}

void Backend::recalculate_processing_schedule(bool thread_safe) {
    using std::chrono::high_resolution_clock;
    using std::chrono::microseconds;
    auto result = std::make_shared<ProcessingSchedule>();

    logging::log<"Backend.ProcessGraph", debug>(std::nullopt, std::nullopt, "Recalculating process graph");

    auto start = high_resolution_clock::now();

    // Gather all the nodes
    std::set<std::shared_ptr<GraphNode>> nodes;
    auto insert_all = [&nodes](auto container) {
        for(auto &item: container->all_graph_nodes()) {
            nodes.insert(item->shared_from_this());
        }
    };
    for(auto &p: ports) {
        insert_all(p);
    }
    for(auto &l: loops) {
        result->loop_graph_nodes.insert(l->graph_node());
        insert_all(l);
        for(auto &c: l->mp_audio_channels) { insert_all(c); }
        for(auto &c: l->mp_midi_channels) { insert_all(c); }
    }
    for(auto &c: fx_chains) {
        insert_all(c);
        for (auto &p : c->audio_input_ports()) { insert_all(p); }
        for (auto &p : c->audio_output_ports()) { insert_all(p); }
        for (auto &p : c->midi_input_ports()) { insert_all(p); }
    }
    for(auto &p: decoupled_midi_ports) { insert_all(p); }

    // Make raw pointers
    std::set<GraphNode*> raw_nodes;
    for(auto &n: nodes) { raw_nodes.insert(n.get()); }

    // Schedule
    std::vector<std::set<GraphNode*>> schedule = graph_processing_order(raw_nodes);

    // Convert
    // TODO: add profiling
    for(auto &s: schedule) {
        ProcessingStep step;
        for(auto &n: s) { step.nodes.insert(n->shared_from_this()); }
        result->steps.push_back(step);
    }

    auto end = high_resolution_clock::now();
    float us = duration_cast<microseconds>(end - start).count();
    logging::log<"Backend.ProcessGraph", debug>(std::nullopt, std::nullopt, "Calculation took {} us", us);

    if(logging::should_log("Backend.ProcessGraph", trace)) {
        auto dot = graph_dot(raw_nodes);
        logging::log<"Backend.ProcessGraph", trace>(std::nullopt, std::nullopt, "DOT graph:\n{}", dot);
    }

    auto me = shared_from_this();
    auto finish_fn = [me, result]() {
        me->log<trace>("Applying updated process graph");
        me->m_processing_schedule = result;
    };
    if (!thread_safe) { finish_fn(); }
    else { cmd_queue.queue(finish_fn); }
}

WeakGraphNodeSet const& Backend::get_loop_graph_nodes() {
    return m_processing_schedule->loop_graph_nodes;
}