#include "Backend.h"
#include "ProcessProfiling.h"
#include "JackAudioSystem.h"
#include "DummyAudioSystem.h"
#include <jack_wrappers.h>
#include "ConnectedPort.h"
#include "ConnectedFXChain.h"
#include "process_loops.h"
#include "ConnectedLoop.h"
#include "ConnectedChannel.h"
#include "ConnectedDecoupledMidiPort.h"
#include "LV2.h"
#include "ChannelInterface.h"
#include "CarlaLV2ProcessingChain.h"
#include "DecoupledMidiPort.h"
#include "AudioBuffer.h"
#include "ObjectPool.h"
#include "AudioMidiLoop.h"

using namespace logging;
using namespace shoop_types;
using namespace shoop_constants;

namespace {
LV2 g_lv2;
}

std::string Backend::log_module_name() const { return "Backend"; }

Backend::Backend(audio_system_type_t audio_system_type, std::string client_name_hint)
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
              profiler->maybe_get_profiling_item("Process.Commands")) {
        log_init();

        using namespace std::placeholders;

        try {
            switch (audio_system_type) {
            case Jack:
                log<LogLevel::debug>("Initializing JACK audio system.");
                audio_system = std::unique_ptr<AudioSystem>(
                    dynamic_cast<AudioSystem *>(new JackAudioSystem(
                        std::string(client_name_hint),
                        std::bind(&Backend::PROC_process, this, _1))));
                break;
            case Dummy:
                // Dummy is also the fallback option - intiialized below
                break;
            default:
                throw_error<std::runtime_error>("Unimplemented backend type");
            }
        } catch (std::exception &e) {
            log<LogLevel::err>("Failed to initialize audio system.");
            log<LogLevel::info>(fmt::runtime("Failure info: " + std::string(e.what())));
            log<LogLevel::info>("Attempting fallback to dummy audio system.");
        } catch (...) {
            log<LogLevel::err>(
                "Failed to initialize audio system: unknown exception");
            log<LogLevel::info>("Attempting fallback to dummy audio system.");
        }

        if (!audio_system || audio_system_type == Dummy) {
            log<LogLevel::debug>("Initializing dummy audio system.");
            audio_system = std::unique_ptr<AudioSystem>(
                dynamic_cast<AudioSystem *>(new _DummyAudioSystem(
                    std::string(client_name_hint),
                    std::bind(&Backend::PROC_process, this, _1))));
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
    rval.dsp_load_percent = maybe_jack_client_handle() ? jack_cpu_load(maybe_jack_client_handle()) : 0.0f;
    rval.xruns_since_last = audio_system->get_xruns();
    audio_system->reset_xruns();
    return rval;
}

#warning delete destroyed ports
// MEMBER FUNCTIONS
void Backend::PROC_process (jack_nframes_t nframes) {
    profiling::stopwatch(
        [this, &nframes]() {
            
            profiler->next_iteration();
            
            // Execute queued commands
            profiling::stopwatch(
                [this]() {
                    cmd_queue.PROC_exec_all();
                },
                cmds_profiling_item
            );


            profiling::stopwatch(
                [this, &nframes]() {
                    // Send/receive decoupled midi
                    PROC_process_decoupled_midi_ports(nframes);
                }, ports_decoupled_midi_profiling_item, ports_profiling_item);
            
            profiling::stopwatch(
                [this, &nframes]() {
                    // Prepare ports:
                    // Get buffers and process passthrough
                    auto prepare_port_fn = [&](auto & p) {
                        if(p) {
                            p->PROC_reset_buffers();
                            p->PROC_ensure_buffer(nframes);
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
                        for (auto & channel: loop->mp_audio_channels) { before_fx_channel_process(channel); }
                        for (auto & channel: loop->mp_midi_channels)  { before_fx_channel_process(channel); }
                    }
                },
                loops_profiling_item
            );

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

            profiling::stopwatch(
                [this, &nframes]() {
                    // Process the FX chains.
                    for (auto & chain: fx_chains) {
                        chain->chain->process(nframes);
                    }
                },
                fx_profiling_item
            );

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

            profiling::stopwatch(
                [this, &nframes]() {
                    // Finish processing any channels that come after FX.
                    auto after_fx_channel_process = [&](auto &c) {
                        if (c && c->ma_process_when == ProcessWhen::AfterFXChains) { c->channel->PROC_finalize_process(); }
                    };
                    for (auto & loop: loops) {
                        for (auto & channel: loop->mp_audio_channels) { after_fx_channel_process(channel); }
                        for (auto & channel: loop->mp_midi_channels)  { after_fx_channel_process(channel); }
                    }
                },
                loops_profiling_item
            );

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

void Backend::PROC_process_decoupled_midi_ports(size_t nframes) {
    for (auto &elem : decoupled_midi_ports) {
        elem->port->PROC_process(nframes);
    }
}

void Backend::terminate() {
    if(audio_system) {
        audio_system->close();
        audio_system.reset(nullptr);
    }
}

jack_client_t *Backend::maybe_jack_client_handle() {
    if (!audio_system || !dynamic_cast<JackAudioSystem*>(audio_system.get())) {
        return nullptr;
    }
    return (jack_client_t*)audio_system->maybe_client_handle();
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
    auto chain = g_lv2.create_carla_chain<Time, Size>(
        type, get_sample_rate(), std::string(title), profiler
    );
    chain->ensure_buffers(get_buffer_size());
    auto info = std::make_shared<ConnectedFXChain>(chain, shared_from_this());
    cmd_queue.queue([this, info]() {
        fx_chains.push_back(info);
    });
    return info;
}