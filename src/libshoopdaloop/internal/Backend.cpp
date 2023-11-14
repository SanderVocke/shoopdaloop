#include "Backend.h"
#include "LoggingBackend.h"
#include "ProcessProfiling.h"
#include "DummyAudioSystem.h"
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
#include "JackAudioSystem.h"
#include <jack_wrappers.h>
#endif

#ifdef SHOOP_HAVE_LV2
#include "LV2.h"
#include "CarlaLV2ProcessingChain.h"
namespace {
LV2 g_lv2;
}
#endif

using namespace logging;
using namespace shoop_types;
using namespace shoop_constants;

Backend::Backend(audio_system_type_t audio_system_type, std::string client_name_hint, std::string argstring)
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
          m_argstring(argstring)
{
}

std::vector<audio_system_type_t> Backend::get_supported_audio_system_types() {
    std::vector<audio_system_type_t> rval = { Dummy };
    #ifdef SHOOP_HAVE_BACKEND_JACK
    rval.push_back(Jack);
    rval.push_back(JackTest);
    #endif
    return rval;
}

void Backend::start() {
        auto weak_self = weak_from_this();
        try {
            switch (m_audio_system_type) {
#ifdef SHOOP_HAVE_BACKEND_JACK
            case Jack:
                log<debug>("Initializing JACK audio system.");
                audio_system = std::unique_ptr<AudioSystemInterface>(
                    dynamic_cast<AudioSystemInterface *>(new JackAudioSystem(
                        std::string(m_client_name_hint),
                        [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<trace>("Jack process: {}", n_frames);
                            self->PROC_process(n_frames);
                        }
                    })));
                break;
            case JackTest:
                log<debug>("Initializing JackTest mock audio system.");
                audio_system = std::unique_ptr<AudioSystemInterface>(
                    dynamic_cast<AudioSystemInterface *>(new JackTestAudioSystem(
                        std::string(m_client_name_hint),
                        [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<trace>("Jack test process: {}", n_frames);
                            self->PROC_process(n_frames);
                        }
                    })));
                break;
#else
            case Jack:
            case JackTest:
                log<warning>("JACK backend requested but not supported.");
#endif
            case Dummy:
                // Dummy is also the fallback option - intiialized below
                break;
            default:
                throw_error<std::runtime_error>("Unimplemented backend type");
            }

            if (m_audio_system_type != Dummy && !audio_system) {
                log<warning>("Failed to initialize primary audio system.");
            }
        } catch (std::exception &e) {
            log<error>("Failed to initialize audio system.");
            log<info>("Failure info: " + std::string(e.what()));
            log<info>("Attempting fallback to dummy audio system.");
        } catch (...) {
            log<error>(
                "Failed to initialize audio system: unknown exception");
            log<info>("Attempting fallback to dummy audio system.");
        }

        if (!audio_system || m_audio_system_type == Dummy) {
            m_audio_system_type = Dummy;
            log<debug>("Initializing dummy audio system.");
            audio_system = std::unique_ptr<AudioSystemInterface>(
                dynamic_cast<AudioSystemInterface *>(new _DummyAudioSystem(
                    std::string(m_client_name_hint),
                    [weak_self] (uint32_t n_frames) {
                        if (auto self = weak_self.lock()) {
                            self->log<trace>("dummy process: {}", n_frames);
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
    }

backend_state_info_t Backend::get_state() {
    backend_state_info_t rval;
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

#warning delete destroyed ports
// MEMBER FUNCTIONS
void Backend::PROC_process (uint32_t nframes) {
    log<trace>("Process {}: start", nframes);
    profiling::stopwatch(
        [this, &nframes]() {
            
            profiler->next_iteration();
            
            // Execute queued commands
            log<trace>("Process: execute commands");
            profiling::stopwatch(
                [this]() {
                    cmd_queue.PROC_exec_all();
                },
                cmds_profiling_item
            );

            log<trace>("Process: decoupled MIDI");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Send/receive decoupled midi
                    PROC_process_decoupled_midi_ports(nframes);
                }, ports_decoupled_midi_profiling_item, ports_profiling_item);
            

            log<trace>("Process: prepare");
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
            
            log<trace>("Process: pre-FX passthrough");
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
            
            log<trace>("Process: process loops");
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

            log<trace>("Process: finish pre-FX ports");
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

            log<trace>("Process: FX");
            profiling::stopwatch(
                [this, &nframes]() {
                    // Process the FX chains.
                    for (auto & chain: fx_chains) {
                        chain->chain->process(nframes);
                    }
                },
                fx_profiling_item
            );

            log<trace>("Process: post-FX passthrough");
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

            log<trace>("Process: finish post-FX channels");
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

            log<trace>("Process: finish post-FX ports");
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

            log<trace>("Process: finish loops");
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

void Backend::PROC_process_decoupled_midi_ports(uint32_t nframes) {
    for (auto &elem : decoupled_midi_ports) {
        elem->port->PROC_process(nframes);
    }
}

void Backend::terminate() {
    for (auto &p : ports) {
        if (p) {
            p->port->close();
        }
    }
    if(audio_system) {
        audio_system->close();
        audio_system.reset(nullptr);
    }
}

void *Backend::maybe_jack_client_handle() {
    if (!audio_system || m_audio_system_type != Jack) {
        return nullptr;
    }
    return audio_system->maybe_client_handle();
}

const char* Backend::get_client_name() {
    if (!audio_system) {
        throw std::runtime_error("get_client_name() called before intialization");
    }
    return audio_system->client_name();
}

unsigned Backend::get_sample_rate() {
    if (!audio_system) {
        throw std::runtime_error("get_sample_rate() called before initialization");
    }
    return audio_system->get_sample_rate();
}

unsigned Backend::get_buffer_size() {
    if (!audio_system) {
        throw std::runtime_error("get_buffer_size() called before initialization");
    }
    return audio_system->get_buffer_size();
}

std::shared_ptr<ConnectedLoop> Backend::create_loop() {
    auto loop = std::make_shared<AudioMidiLoop>(profiler);
    auto r = std::make_shared<ConnectedLoop>(shared_from_this(), loop);
    cmd_queue.queue([this, r]() {
        loops.push_back(r);
    });
    return r;
}

std::shared_ptr<ConnectedFXChain> Backend::create_fx_chain(fx_chain_type_t type, const char* title) {
    std::shared_ptr<ProcessingChainInterface<Time, Size>> chain;
    switch(type) {
#ifdef SHOOP_HAVE_LV2
        case Carla_Rack:
        case Carla_Patchbay:
        case Carla_Patchbay_16x:
            chain = g_lv2.create_carla_chain<Time, Size>(
                type, get_sample_rate(), std::string(title), profiler
            );
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
    auto info = std::make_shared<ConnectedFXChain>(chain, shared_from_this());
    cmd_queue.queue([this, info]() {
        fx_chains.push_back(info);
    });
    return info;
}

Backend::~Backend() {
    terminate();
}