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
#include "fmt/format.h"
#include "fmt/ranges.h"
#include "graph_dot.h"
#include "shoop_globals.h"
#include "types.h"
#include "graph_processing_order.h"
#include <chrono>
#include <condition_variable>
#include "CustomProcessingChain.h"

#ifdef _WIN32
#undef min
#undef max
#endif

#ifdef SHOOP_HAVE_BACKEND_JACK
#include <jack_wrappers.h>
#endif

#ifdef SHOOP_HAVE_LV2
#include "LV2.h"
#endif

using namespace logging;
using namespace shoop_types;
using namespace shoop_constants;

struct BackendSession::RecalculateGraphThread {
    std::thread thread;
    std::condition_variable cv;
    std::mutex mutex;
    bool finish;
    std::atomic<unsigned> m_req_id;
    shoop_weak_ptr<BackendSession> backend;
    BackendSession *tmp_backend;

    RecalculateGraphThread(BackendSession &_backend) :
        finish(false),
        m_req_id(0),
        tmp_backend(&_backend)
    {
        thread = std::thread([this]() { this->thread_fn(); });
    }

    void thread_fn() {
        unsigned rid = 0;
        while(true) {
            std::unique_lock lk(mutex);
            unsigned new_rid;
            cv.wait(lk, [this, &rid, &new_rid]() { new_rid = m_req_id.load(); return finish || rid != new_rid; });
            if(finish) { break; }
            rid = new_rid;
            lk.unlock();
            if (auto sh_backend = backend.lock()) {
                sh_backend->log<log_level_debug>("Recalculate graph {}", rid);
                sh_backend->recalculate_processing_schedule(rid);
            } else {
                logging::log<"Backend.Session", log_level_debug>(std::nullopt, std::nullopt,
                "Backend session not available");
            }
        }
    }

    void update_request_id(unsigned req_id) {
        if (tmp_backend) {
            backend = tmp_backend->weak_from_this();
            tmp_backend = nullptr;
        }
        if (req_id != m_req_id) {
            std::lock_guard lk(mutex);
            m_req_id = req_id;
        }
        cv.notify_one();
    }

    ~RecalculateGraphThread() {
        finish = true;
        cv.notify_one();
        thread.join();
    }
};

BackendSession::BackendSession() :
          WithCommandQueue(),
          profiler(shoop_make_shared<profiling::Profiler>()),
          top_profiling_item(profiler->maybe_get_profiling_item("Process")),
          graph_profiling_item(profiler->maybe_get_profiling_item("Process.Graph")),
          cmds_profiling_item(
              profiler->maybe_get_profiling_item("Process.Commands")),
          ma_state(State::Active),
          ma_graph_id(0),
          ma_graph_request_id(0),
          m_recalculate_graph_thread(std::make_unique<RecalculateGraphThread>(*this))
{
    audio_buffer_pool = shoop_static_pointer_cast<AudioBufferPool>(
        shoop_make_shared<ObjectPool<AudioBuffer<shoop_types::audio_sample_t>>>(
            "Session audio buffer pool", n_buffers_in_pool, audio_buffer_size)
    );
    loops.reserve(initial_max_loops);
    ports.reserve(initial_max_ports);
    fx_chains.reserve(initial_max_fx_chains);
}

shoop_backend_session_state_info_t BackendSession::get_state() {
    shoop_backend_session_state_info_t rval;
    rval.n_audio_buffers_available = audio_buffer_pool->n_available();
    rval.n_audio_buffers_created = audio_buffer_pool->n_created_since_last_checked();
    return rval;
}

//TODO delete destroyed ports
void BackendSession::PROC_process (uint32_t nframes) {
    auto weak_self = weak_from_this();
    log<log_level_debug_trace>("Process {}: start", nframes);
    profiling::stopwatch(
        [this, &nframes]() {
            
            profiler->next_iteration();
            
            // Execute queued commands
            // (Graph changes applied here too)
            log<log_level_debug_trace>("Process: execute commands and MIDI control");
            profiling::stopwatch(
                [this, nframes]() {
                    PROC_handle_command_queue();
                },
                cmds_profiling_item
            );

            auto graph_id = ma_graph_id.load();
            auto graph_request_id = ma_graph_request_id.load();
            if (graph_id != graph_request_id) {
                log<log_level_debug_trace>("Notify graph recalculate thread");
                m_recalculate_graph_thread->update_request_id(graph_request_id);
            } 
            
            log<log_level_debug_trace>("Process: process graph");
            auto processing_schedule = m_processing_schedule;

            profiling::stopwatch(
                [this, &nframes, &processing_schedule]() {
                    size_t n_steps = processing_schedule->steps.size();
                    for(size_t i=0; i<processing_schedule->steps.size(); i++) {
                        auto &step = processing_schedule->steps[i];
                        try {
                            if(step.nodes.size() == 1) {
                                log<log_level_debug_trace>("[{}/{}] Processing node: {}", i, n_steps, step.nodes.begin()->get()->graph_node_name());
                                step.nodes.begin()->get()->PROC_process(nframes);
                            } else if(step.nodes.size() > 1) {
                                log<log_level_debug_trace>("[{}/{}] Co-processing {} nodes, first: {}", i, n_steps, step.nodes.size(), step.nodes.begin()->get()->graph_node_name());
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

shoop_shared_ptr<GraphLoop> BackendSession::create_loop() {
    auto loop = shoop_make_shared<AudioMidiLoop>();
    auto r = shoop_make_shared<GraphLoop>(shared_from_this(), loop);

    // Setup profiling
    auto loops_item = profiler->maybe_get_profiling_item("Process.Graph.Loops");
    auto loops_control_item = profiler->maybe_get_profiling_item("Process.Graph.Loops.Control");
    r->graph_node()->set_processed_cb([loops_item, loops_control_item](uint32_t us) {
        loops_item->log_time(us);
        loops_control_item->log_time(us);
    });

    loops.push_back(r);
    set_graph_node_changes_pending();
    return r;
}

shoop_shared_ptr<GraphFXChain> BackendSession::create_fx_chain(shoop_fx_chain_type_t type, const char* title) {
#ifdef SHOOP_HAVE_LV2
    static LV2 lv2;
#endif
    shoop_shared_ptr<ProcessingChainInterface<Time, Size>> chain;
    switch(type) {
#ifdef SHOOP_HAVE_LV2
        case Carla_Rack:
        case Carla_Patchbay:
        case Carla_Patchbay_16x:
            try {
                chain = shoop_static_pointer_cast<ProcessingChainInterface<Time, Size>>(
                lv2.create_carla_chain<Time, Size>(
                    type, m_sample_rate, m_buffer_size, std::string(title), audio_buffer_pool
                ));
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
            chain = shoop_static_pointer_cast<ProcessingChainInterface<Time, Size>>(
                shoop_make_shared<CustomProcessingChain<Time, Size>>(
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
                    
                    if (should_log<log_level_debug_trace>()) {
                        log<log_level_debug_trace>("FX output port 1: {}", out_buf_1);
                        log<log_level_debug_trace>("FX output port 2: {}", out_buf_2);
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
                }, audio_buffer_pool
            ));
        break;
    };

    auto info = shoop_make_shared<GraphFXChain>(chain, shared_from_this());

    // Setup profiling
    auto fx_item = profiler->maybe_get_profiling_item("Process.Graph.FX");
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto audio_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Audio");
    auto midi_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Midi");
    info->graph_node()->set_processed_cb([fx_item](uint32_t us) {
        fx_item->log_time(us);
    });
    for (auto &p : info->audio_input_ports()) {
        p->first_graph_node()->set_processed_cb([ports_item, audio_ports_item](uint32_t us) {
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([ports_item, audio_ports_item](uint32_t us) {
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
    }
    for (auto &p : info->audio_output_ports()) {
        p->first_graph_node()->set_processed_cb([ ports_item, audio_ports_item](uint32_t us) {
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([ports_item, audio_ports_item](uint32_t us) {
            ports_item->log_time(us);
            audio_ports_item->log_time(us);
        });
    }
    for (auto &p : info->midi_input_ports()) {
        p->first_graph_node()->set_processed_cb([ports_item, midi_ports_item](uint32_t us) {
            ports_item->log_time(us);
            midi_ports_item->log_time(us);
        });
        p->second_graph_node()->set_processed_cb([ports_item, midi_ports_item](uint32_t us) {
            ports_item->log_time(us);
            midi_ports_item->log_time(us);
        });
    }

    fx_chains.push_back(info);

    set_graph_node_changes_pending();
    return info;
}

shoop_shared_ptr<GraphAudioPort> BackendSession::add_audio_port(shoop_shared_ptr<shoop_types::_AudioPort> port) {
    auto rval = shoop_make_shared<GraphAudioPort>(port, shared_from_this());
    ports.push_back(shoop_static_pointer_cast<GraphPort>(rval));

    // Setup profiling
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto audio_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Audio");
    auto cb = [ports_item, audio_ports_item](uint32_t us) {
        ports_item->log_time(us);
        audio_ports_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    set_graph_node_changes_pending();

    return rval;
}

shoop_shared_ptr<GraphMidiPort> BackendSession::add_midi_port(shoop_shared_ptr<MidiPort> port) {
    auto rval = shoop_make_shared<GraphMidiPort>(port, shared_from_this());
    ports.push_back(shoop_static_pointer_cast<GraphPort>(rval));

    // Setup profiling
    auto ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports");
    auto midi_ports_item = profiler->maybe_get_profiling_item("Process.Graph.Ports.Midi");
    auto cb = [ports_item, midi_ports_item](uint32_t us) {
        ports_item->log_time(us);
        midi_ports_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    set_graph_node_changes_pending();

    return rval;
}

shoop_shared_ptr<GraphLoopChannel> BackendSession::add_loop_channel(shoop_shared_ptr<GraphLoop> loop, shoop_shared_ptr<ChannelInterface> channel) {
    auto rval = shoop_make_shared<GraphLoopChannel>(channel, loop, shared_from_this());

    // Setup profiling
    auto loops_item = profiler->maybe_get_profiling_item("Process.Graph.Loops");
    auto loop_channels_item = profiler->maybe_get_profiling_item("Process.Graph.Loops.Channels");
    auto cb = [loops_item, loop_channels_item](uint32_t us) {
        loops_item->log_time(us);
        loop_channels_item->log_time(us);
    };
    rval->first_graph_node()->set_processed_cb(cb);
    rval->second_graph_node()->set_processed_cb(cb);

    set_graph_node_changes_pending();

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

void BackendSession::set_graph_node_changes_pending() {
    log<log_level_debug>("Set graph node changes pending");
    ma_graph_request_id++;
}

void BackendSession::recalculate_processing_schedule(unsigned req_id) {
    using std::chrono::high_resolution_clock;
    using std::chrono::microseconds;
    auto result = shoop_make_shared<ProcessingSchedule>();
    result->loops = loops;
    result->ports = ports;
    result->fx_chains = fx_chains;

    // Set up a callback to get all loop nodes
    auto weak_self = weak_from_this();
    WeakGraphNodeSet weak_loop_nodes;
    for (auto &l : result->loops) {
        if (l) {
            weak_loop_nodes.insert(shoop_weak_ptr<GraphNode>(l->graph_node()));
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
    std::set<shoop_shared_ptr<GraphNode>> nodes;
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
    for(auto &n: nodes) { if (n) { raw_nodes.insert(n.get()); } }

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

    if (logging::should_log<"Backend.ProcessGraph", log_level_debug_trace>()) {
        auto dot = graph_dot(raw_nodes);
        logging::log<"Backend.ProcessGraph", log_level_debug_trace>(std::nullopt, std::nullopt, "DOT graph:\n{}", dot);
        if(logging::should_log("Backend.ProcessGraph", log_level_debug)) {
            // Print the processing order
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
            auto _ptr = result.get();
            logging::log<"Backend.ProcessGraph", log_level_debug>(std::nullopt, std::nullopt, "Processing schedule @ {}:\n{}", fmt::ptr(_ptr), schedule_str);
        }
    }

    auto maybe_me = weak_from_this();
    if(auto me = maybe_me.lock()) {
        auto finish_fn = [me, result, req_id]() {
            me->log<log_level_debug>("Applying updated process graph {}", req_id);
            me->m_processing_schedule = result;
            me->ma_graph_id = req_id;
        };
        queue_process_thread_command(finish_fn);
    }
}

void BackendSession::wait_graph_up_to_date() {
    while(ma_graph_id.load() != ma_graph_request_id.load()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
}