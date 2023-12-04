#include "BackendSession.h"
#include "LoggingBackend.h"
#include "ProcessProfiling.h"
#include "DummyAudioMidiDriver.h"
#include "ConnectedPort.h"
#include "ConnectedFXChain.h"
#include "ProcessingChainInterface.h"
#include "process_loops.h"
#include "ConnectedLoop.h"
#include "ConnectedChannel.h"
#include "ConnectedDecoupledMidiPort.h"
#include "ChannelInterface.h"
#include "DecoupledMidiPort.h"
#include "AudioBuffer.h"
#include "ObjectPool.h"
#include "AudioMidiLoop.h"
#include "shoop_globals.h"
#include "types.h"
#include "process_when.h"

#ifdef SHOOP_HAVE_BACKEND_JACK
#include "JackAudioMidiDriver.h"
#include <jack_wrappers.h>
#endif

#ifdef SHOOP_HAVE_LV2
#include "LV2.h"
#include "CarlaLV2ProcessingChain.h"
#endif

using namespace logging;
using namespace shoop_types;
using namespace shoop_constants;

BackendSession::BackendSession(shoop_audio_driver_type_t audio_system_type, std::string client_name_hint, std::string argstring)
        : cmd_queue(shoop_constants::command_queue_size, 1000, 1000),
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
          m_audio_system_type(audio_system_type),
          m_client_name_hint(client_name_hint),
          m_argstring(argstring),
          ma_state(State::NotStarted)
{
}

std::vector<shoop_audio_driver_type_t> BackendSession::get_supported_audio_system_types() {
    std::vector<shoop_audio_driver_type_t> rval = { Dummy };
    #ifdef SHOOP_HAVE_BACKEND_JACK
    rval.push_back(Jack);
    rval.push_back(JackTest);
    #endif
    return rval;
}

void BackendSession::start() {
        auto weak_self = weak_from_this();
        try {
            switch (m_audio_system_type) {
#ifdef SHOOP_HAVE_BACKEND_JACK
            case Jack:
                log<log_debug>("Initializing JACK audio system.");
                audio_system = std::unique_ptr<AudioMidiDriver>(
                    dynamic_cast<AudioMidiDriver *>(new JackAudioMidiDriver(
                        std::string(m_client_name_hint),
                        [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<log_trace>("Jack process: {}", n_frames);
                            self->PROC_process(n_frames);
                        }
                    })));
                break;
            case JackTest:
                log<log_debug>("Initializing JackTest mock audio system.");
                audio_system = std::unique_ptr<AudioMidiDriver>(
                    dynamic_cast<AudioMidiDriver *>(new JackTestAudioMidiDriver(
                        std::string(m_client_name_hint),
                        [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<log_trace>("Jack test process: {}", n_frames);
                            self->PROC_process(n_frames);
                        }
                    })));
                break;
#else
            case Jack:
            case JackTest:
                log<log_warning>("JACK backend requested but not supported.");
#endif
            case Dummy:
                // Dummy is also the fallback option - intiialized below
                break;
            default:
                throw_error<std::runtime_error>("Unimplemented backend type");
            }

            if (m_audio_system_type != Dummy && !audio_system) {
                log<log_warning>("Failed to initialize primary audio system.");
            }
        } catch (std::exception &e) {
            log<log_error>("Failed to initialize audio system.");
            log<log_info>("Failure log_info: " + std::string(e.what()));
            log<log_info>("Attempting fallback to dummy audio system.");
        } catch (...) {
            log<log_error>(
                "Failed to initialize audio system: unknown exception");
            log<log_info>("Attempting fallback to dummy audio system.");
        }

        if (!audio_system || m_audio_system_type == Dummy) {
            m_audio_system_type = Dummy;
            log<log_debug>("Initializing dummy audio system.");
            audio_system = std::unique_ptr<AudioMidiDriver>(
                dynamic_cast<AudioMidiDriver *>(new _DummyAudioMidiDriver(
                    std::string(m_client_name_hint),
                    [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<log_trace>("dummy process: {}", n_frames);
                            self->PROC_process(n_frames);
                        }
                    })));
        }

        audio_buffer_pool = std::make_shared<ObjectPool<AudioBuffer<shoop_types::audio_sample_t>>>(
            n_buffers_in_pool, audio_buffer_size);
        loops.reserve(initial_max_loops);
        ports.reserve(initial_max_ports);
        fx_chains.reserve(initial_max_fx_chains);
        decoupled_midi_ports.reserve(initial_max_decoupled_midi_ports);
        audio_system->start();
        ma_state = State::Running;
    }

shoop_backend_session_state_info_t BackendSession::get_state() {
    shoop_backend_session_state_info_t rval;
#ifdef SHOOP_HAVE_BACKEND_JACK
    rval.dsp_load_percent = (maybe_jack_client_handle() && m_audio_system_type == Jack) ? JackApi::cpu_load((jack_client_t*)maybe_jack_client_handle()) : 0.0f;
#else
    rval.dsp_load_percent = 0.0f;
#endif
    rval.xruns_since_last = audio_system->get_xruns();
    rval.actual_type = m_audio_system_type;
    audio_system->reset_xruns();
    return rval;
}

//TODO delete destroyed ports
// MEMBER FUNCTIONS
void BackendSession::PROC_process (uint32_t nframes) {
    log<log_trace>("Process {}: start", nframes);
    profiling::stopwatch(
        [this, &nframes]() {
            
            profiler->next_iteration();
            
            // Execute queued commands
            log<log_trace>("Process: execute commands");
            profiling::stopwatch(
                [this]() {
                    cmd_queue.PROC_exec_all();
                },
                cmds_profiling_item
            );

            log<log_trace>("Process: decoupled MIDI");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Send/receive decoupled midi
                    PROC_process_decoupled_midi_ports(nframes);
                }, ports_decoupled_midi_profiling_item, ports_profiling_item);
            

            log<log_trace>("Process: prepare");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Prepare ports:
                    // Get buffers, reset them, and ensure they are the right size
                    auto prepare_port_fn = [&](auto & p) {
                        if(p) {
                            p->PROC_reset_buffers();
                            p->PROC_ensure_buffer(nframes, p->port->direction() == PortDirection::Output);
                        }
                    };
                    for (auto & port: ports) { prepare_port_fn(port); }
                    for (auto & chain : fx_chains) {
                        for (auto & p : chain->mc_audio_input_ports) { prepare_port_fn(p); }
                        for (auto & p : chain->mc_audio_output_ports){ prepare_port_fn(p); }
                        for (auto & p : chain->mc_midi_input_ports)  { prepare_port_fn(p); }
                    }

                    // Prepare loops:
                    // Connect port buffers to loop channels
                    for (auto & loop: loops) {
                        if (loop) {
                            loop->PROC_prepare_process(nframes);
                        }
                    }
                }, ports_prepare_profiling_item, ports_profiling_item);
            
            log<log_trace>("Process: pre-FX passthrough");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Process passthrough on the input side (before FX chains)
                    auto before_fx_port_passthrough_fn = [&](auto &p) {
                        if (p && p->ma_process_when == ProcessWhen::BeforeFXChains) { p->PROC_passthrough(nframes); }
                    };
                    for (auto & port: ports) { before_fx_port_passthrough_fn(port); }
                    for (auto & chain : fx_chains) {
                        for (auto & p : chain->mc_audio_input_ports) { before_fx_port_passthrough_fn(p); }
                        for (auto & p : chain->mc_midi_input_ports)  { before_fx_port_passthrough_fn(p); }
                    }
                },
                ports_profiling_item, ports_passthrough_profiling_item, ports_passthrough_input_profiling_item
            );
            
            log<log_trace>("Process: process loops");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Process the loops. This queues deferred audio operations for post-FX channels.
                    process_loops<ConnectedLoop>
                        (loops, nframes, [](ConnectedLoop &l) { return (LoopInterface*)l.loop.get(); });
            
                    // Finish processing any channels that come before FX.
                    auto before_fx_channel_process = [&](auto &c) {
                        if (c && c->ma_process_when == ProcessWhen::BeforeFXChains) { c->channel->PROC_finalize_process(); }
                    };
                    for (auto & loop: loops) {
                        if (loop) {
                            for (auto & channel: loop->mp_audio_channels) { before_fx_channel_process(channel); }
                            for (auto & channel: loop->mp_midi_channels)  { before_fx_channel_process(channel); }
                        }
                    }
                },
                loops_profiling_item
            );

            log<log_trace>("Process: finish pre-FX ports");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Finish processing any ports that come before FX.
                    auto before_fx_port_process = [&](auto &p) {
                        if (p && p->ma_process_when == ProcessWhen::BeforeFXChains) { p->PROC_finalize_process(nframes); }
                    };
                    for (auto &port : ports) { before_fx_port_process(port); }
                    for (auto & chain : fx_chains) {
                        for (auto & p : chain->mc_audio_input_ports) { before_fx_port_process(p); }
                        for (auto & p : chain->mc_midi_input_ports)  { before_fx_port_process(p); }
                    }
                },
                ports_profiling_item, ports_prepare_fx_profiling_item
            );

            log<log_trace>("Process: FX");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Process the FX chains.
                    for (auto & chain: fx_chains) {
                        chain->chain->process(nframes);
                    }
                },
                fx_profiling_item
            );

            log<log_trace>("Process: post-FX passthrough");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Process passthrough on the output side (after FX chains)
                    auto after_fx_port_passthrough_fn = [&](auto &p) {
                        if (p && p->ma_process_when == ProcessWhen::AfterFXChains) { p->PROC_passthrough(nframes); }
                    };
                    for (auto & port: ports) { after_fx_port_passthrough_fn(port); }
                    for (auto & chain : fx_chains) {
                        for (auto & p : chain->mc_audio_output_ports) { after_fx_port_passthrough_fn(p); }
                    }
                },
                ports_profiling_item, ports_passthrough_profiling_item, ports_passthrough_output_profiling_item
            );

            log<log_trace>("Process: finish post-FX channels");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Finish processing any channels that come after FX.
                    auto after_fx_channel_process = [&](auto &c) {
                        if (c && c->ma_process_when == ProcessWhen::AfterFXChains) { c->channel->PROC_finalize_process(); }
                    };
                    for (auto & loop: loops) {
                        if (loop) {
                            for (auto & channel: loop->mp_audio_channels) { after_fx_channel_process(channel); }
                            for (auto & channel: loop->mp_midi_channels)  { after_fx_channel_process(channel); }
                        }
                    }
                },
                loops_profiling_item
            );

            log<log_trace>("Process: finish post-FX ports");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Finish processing any ports that come after FX.
                    auto after_fx_port_process = [&](auto &p) {
                        if (p && p->ma_process_when == ProcessWhen::AfterFXChains) { p->PROC_finalize_process(nframes); }
                    };
                    for (auto &port : ports) { after_fx_port_process(port); }
                    for (auto & chain : fx_chains) {
                        for (auto & p : chain->mc_audio_output_ports) { after_fx_port_process(p); }
                    }
                },
                ports_profiling_item, ports_finalize_profiling_item
            );

            log<log_trace>("Process: finish loops");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Finish processing loops.
                    for (auto &loop : loops) {
                        if (loop) {
                            loop->PROC_finalize_process();
                        }
                    }
                },
                loops_profiling_item
            );
        },
        top_profiling_item
    );
}

void BackendSession::PROC_process_decoupled_midi_ports(uint32_t nframes) {
    for (auto &elem : decoupled_midi_ports) {
        elem->port->PROC_process(nframes);
    }
}

void BackendSession::terminate() {
    if (ma_state != State::Terminated) {
        log<log_debug>("Terminating backend");
        cmd_queue.passthrough_on();
        for (auto &p : ports) {
            if (p) {
                p->port->close();
            }
        }
        for (auto &p : decoupled_midi_ports) {
            if (p) {
                p->port->close();
            }
        }
        if(audio_system) {
            audio_system->close();
            audio_system.reset(nullptr);
        }
        ma_state = State::Terminated;
    } else {
        log<log_debug>("Backend already terminated");
    }
}

void *BackendSession::maybe_jack_client_handle() {
    if (!audio_system || m_audio_system_type != Jack) {
        return nullptr;
    }
    return audio_system->maybe_client_handle();
}

const char* BackendSession::get_client_name() {
    if (!audio_system) {
        throw std::runtime_error("get_client_name() called before intialization");
    }
    return audio_system->client_name();
}

unsigned BackendSession::get_sample_rate() {
    if (!audio_system) {
        throw std::runtime_error("get_sample_rate() called before initialization");
    }
    return audio_system->get_sample_rate();
}

unsigned BackendSession::get_buffer_size() {
    if (!audio_system) {
        throw std::runtime_error("get_buffer_size() called before initialization");
    }
    return audio_system->get_buffer_size();
}

std::shared_ptr<ConnectedLoop> BackendSession::create_loop() {
    auto loop = std::make_shared<AudioMidiLoop>(profiler);
    auto r = std::make_shared<ConnectedLoop>(shared_from_this(), loop);
    cmd_queue.queue([this, r]() {
        loops.push_back(r);
    });
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
                    type, get_sample_rate(), std::string(title), profiler
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
    chain->ensure_buffers(get_buffer_size());
    auto log_info = std::make_shared<ConnectedFXChain>(chain, shared_from_this());
    cmd_queue.queue([this, log_info]() {
        fx_chains.push_back(log_info);
    });
    return log_info;
}

BackendSession::~BackendSession() {
    terminate();
}