#include "BackendSession.h"
#include "GraphLoopChannel.h"
#include "GraphNode.h"
#include "GraphAudioPort.h"
#include "GraphMidiPort.h"
#include "LoggingBackend.h"
#include "ProcessProfiling.h"
#include "GraphPort.h"
#include "GraphFXChain.h"
#include "ProcessingChainInterface.h"
#include "GraphLoop.h"
#include "AudioBuffer.h"
#include "ObjectPool.h"
#include "AudioMidiLoop.h"
#include "graph_dot.h"
#include "shoop_globals.h"
#include "types.h"
#include "graph_processing_order.h"
#include <chrono>
#include "CustomProcessingChain.h"

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
          graph_profiling_item(profiler->maybe_get_profiling_item("Process.Graph")),
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
            
            log<log_level_trace>("Process: process graph");
            auto processing_schedule = m_processing_schedule;

            profiling::stopwatch(
                [this, &nframes, &processing_schedule]() {
                    size_t n_steps = processing_schedule->steps.size();
                    for(size_t i=0; i<processing_schedule->steps.size(); i++) {
                        auto &step = processing_schedule->steps[i];
                        try {
                            if(step.nodes.size() == 1) {
                                log<log_level_trace>("[{}/{}] Processing node: {}", i, n_steps, step.nodes.begin()->get()->graph_node_name());
                                step.nodes.begin()->get()->PROC_process(nframes);
                            } else if(step.nodes.size() > 1) {
                                log<log_level_trace>("[{}/{}] Co-processing {} nodes, first: {}", i, n_steps, step.nodes.size(), step.nodes.begin()->get()->graph_node_name());
                                step.nodes.begin()->get()->PROC_co_process(step.nodes, nframes);
                            }
                        } catch (const std::exception &exp) {
                            log<log_level_error>("Exception while processing graph step: {}", exp.what());
                        } catch (...) {
                            log<log_level_error>("Unknown exception while processing graph step");
                        }
                    }
                }, graph_profiling_item
            );
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
                p->get_port().close();
            }
        }
        ma_state = State::Destroyed;
    } else {
        log<log_level_debug>("Backend already destroyed");
    }
}

std::shared_ptr<GraphLoop> BackendSession::create_loop() {
    auto loop = std::make_shared<AudioMidiLoop>();
    auto r = std::make_shared<GraphLoop>(shared_from_this(), loop);

    // Setup profiling
    auto top_item = top_profiling_item;
    auto loops_item = profiler->maybe_get_profiling_item("Process.Graph.Loops");
    auto loops_control_item = profiler->maybe_get_profiling_item("Process.Graph.Loops.Control");
    r->graph_node()->set_processed_cb([top_item, loops_item, loops_control_item](uint32_t us) {
        top_item->log_time(us);
        loops_item->log_time(us);
        loops_control_item->log_time(us);
    });

    loops.push_back(r);
    recalculate_processing_schedule();
    return r;
}

std::shared_ptr<GraphFXChain> BackendSession::create_fx_chain(shoop_fx_chain_type_t type, const char* title) {
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
                    type, m_sample_rate, std::string(title)
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
                    auto readbuf = midi->PROC_get_read_output_data_buffer(n);
                    auto n_msgs = readbuf->PROC_get_n_events();
                    for (uint32_t i = 0; i < n_msgs; i++) {
                        auto &msg = readbuf->PROC_get_event_reference(i);
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

    auto info = std::make_shared<GraphFXChain>(chain, shared_from_this());

    // Setup profiling
    auto top_item = top_profiling_item;
    auto fx_item = profiler->maybe_get_profiling_item("Process.Graph.FX");
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto audio_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Audio");
    auto midi_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Midi");
    info->graph_node()->set_processed_cb([top_item, fx_item](uint32_t us) {
        top_item->log_time(us);
        fx_item->log_time(us);
    });
    for (auto &p : info->audio_input_ports()) {
        p->first_graph_node()->set_processed_cb([top_item, ports_item, audio_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([top_item, ports_item, audio_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
    }
    for (auto &p : info->audio_output_ports()) {
        p->first_graph_node()->set_processed_cb([top_item, ports_item, audio_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([top_item, ports_item, audio_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
    }
    for (auto &p : info->midi_input_ports()) {
        p->first_graph_node()->set_processed_cb([top_item, ports_item, midi_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            midi_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([top_item, ports_item, midi_ports_item](uint32_t us) {
            top_item->log_time(us);
            ports_item->log_time(us);
            midi_ports_item->log_time(us);
        });
    }

    fx_chains.push_back(info);

    recalculate_processing_schedule();
    return info;
}

std::shared_ptr<GraphAudioPort> BackendSession::add_audio_port(std::shared_ptr<shoop_types::_AudioPort> port) {
    auto rval = std::make_shared<GraphAudioPort>(port, shared_from_this());
    ports.push_back(rval);

    // Setup profiling
    auto top_item = top_profiling_item;
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto audio_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Audio");
    auto cb = [top_item, ports_item, audio_ports_item](uint32_t us) {
        top_item->log_time(us);
        ports_item->log_time(us);
        audio_ports_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    recalculate_processing_schedule();

    return rval;
}

std::shared_ptr<GraphMidiPort> BackendSession::add_midi_port(std::shared_ptr<MidiPort> port) {
    auto rval = std::make_shared<GraphMidiPort>(port, shared_from_this());
    ports.push_back(rval);

    // Setup profiling
    auto top_item = top_profiling_item;
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto midi_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Midi");
    auto cb = [top_item, ports_item, midi_ports_item](uint32_t us) {
        top_item->log_time(us);
        ports_item->log_time(us);
        midi_ports_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    recalculate_processing_schedule();

    return rval;
}

std::shared_ptr<GraphLoopChannel> BackendSession::add_loop_channel(std::shared_ptr<GraphLoop> loop, std::shared_ptr<ChannelInterface> channel) {
    auto rval = std::make_shared<GraphLoopChannel>(channel, loop, shared_from_this());

    // Setup profiling
    auto top_item = top_profiling_item;
    auto loops_item = profiler->maybe_get_profiling_item("Process.Graph.Loops");
    auto loop_channels_item = profiler->maybe_get_profiling_item("Process.Graph.Loops.Channels");
    auto cb = [top_item, loops_item, loop_channels_item](uint32_t us) {
        top_item->log_time(us);
        loops_item->log_time(us);
        loop_channels_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    recalculate_processing_schedule();

    return rval;
}

BackendSession::~BackendSession() {
    destroy();
}

void BackendSession::set_sample_rate(uint32_t sr) {
    m_sample_rate = sr;
}

void BackendSession::set_buffer_size(uint32_t bs) {
    auto notify = [&, this](auto &item) {
        auto as_node = dynamic_cast<NotifyProcessParametersInterface*>(item.get());
        if (as_node) {
            as_node->PROC_notify_changed_buffer_size(bs);
        }
    };
    exec_process_thread_command([&, this]() {
        for (auto &i : loops) { notify(i); }
        for (auto &i : ports) { notify(i); }
        for (auto &i : fx_chains) { notify(i); }
        m_buffer_size = bs;
    });
}

void BackendSession::recalculate_processing_schedule(bool thread_safe) {
    using std::chrono::high_resolution_clock;
    using std::chrono::microseconds;
    auto result = std::make_shared<ProcessingSchedule>();
    result->loops = loops;
    result->ports = ports;
    result->fx_chains = fx_chains;

    // Set up a callback to get all loop nodes
    auto weak_self = weak_from_this();
    WeakGraphNodeSet weak_loop_nodes;
    for (auto &l : result->loops) {
        if (l) {
            weak_loop_nodes.insert(l->graph_node());
        }
    }
    auto get_loop_nodes = [weak_self, weak_loop_nodes]() {
        auto self = weak_self.lock();
        if (!self) { return WeakGraphNodeSet(); }
        return weak_loop_nodes;
    };

    logging::log<"Backend.ProcessGraph", log_level_debug>(std::nullopt, std::nullopt, "Recalculating process graph");

    auto start = high_resolution_clock::now();

    // Gather all the nodes
    std::set<std::shared_ptr<GraphNode>> nodes;
    auto insert_all = [&nodes](auto container) {
        for(auto &item: container->all_graph_nodes()) {
            nodes.insert(item->shared_from_this());
        }
    };
    for(auto &p: result->ports) {
        if (p) {
            insert_all(p);
        }
    }
    for(auto &l: result->loops) {
        if (l) {
            insert_all(l);
            l->set_get_co_process_nodes_cb(get_loop_nodes);
            for(auto &c: l->mp_audio_channels) { if(c) { insert_all(c); } }
            for(auto &c: l->mp_midi_channels) { if(c) { insert_all(c); } }
        }
    }
    for(auto &c: result->fx_chains) {
        if (c) {
            insert_all(c);
            for (auto &p : c->audio_input_ports()) { if(p) { insert_all(p); } }
            for (auto &p : c->audio_output_ports()) { if(p) { insert_all(p); } }
            for (auto &p : c->midi_input_ports()) { if(p) { insert_all(p); } }
        }
    }

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
    logging::log<"Backend.ProcessGraph", log_level_debug>(std::nullopt, std::nullopt, "Calculation took {} us", us);

    if(logging::should_log("Backend.ProcessGraph", log_level_trace)) {
        auto dot = graph_dot(raw_nodes);
        logging::log<"Backend.ProcessGraph", log_level_trace>(std::nullopt, std::nullopt, "DOT graph:\n{}", dot);
        if(logging::should_log("Backend.ProcessGraph", log_level_debug)) {
            std::vector<std::string> schedule_names;
            for(auto &step: schedule) {
                std::string step_name;
                for(auto &n: step) {
                    if(!step_name.empty()) { step_name += ", "; }
                    step_name += n->graph_node_name();
                }
                schedule_names.push_back(step_name);
            }
            std::string schedule_str;
            for(auto &s: schedule_names) {
                if(!schedule_str.empty()) { schedule_str += "\n"; }
                schedule_str += s;
            }
            logging::log<"Backend.ProcessGraph", log_level_debug>(std::nullopt, std::nullopt, "Processing schedule:\n{}", schedule_str);
        }
    }

    auto me = shared_from_this();
    auto finish_fn = [me, result]() {
        me->log<log_level_trace>("Applying updated process graph");
        me->m_processing_schedule = result;
    };
    if (!thread_safe) { finish_fn(); }
    else { queue_process_thread_command(finish_fn); }
}