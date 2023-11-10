// This file implements the libshoopdaloop C API. It is also where
// internal C++ objects are tied together at the highest level.

#include "libshoopdaloop.h"

// Internal
#include "AudioBuffer.h"
#include "AudioChannel.h"
#include "SerializeableStateInterface.h"
#include "AudioMidiLoop.h"
#include "ExternalUIInterface.h"
#include "AudioPortInterface.h"
#include "AudioSystemInterface.h"

#include "LoggingBackend.h"
#include "MidiChannel.h"
#include "MidiMessage.h"
#include "MidiPortInterface.h"
#include "MidiMergingBuffer.h"
#include "ObjectPool.h"
#include "PortInterface.h"
#include "ChannelInterface.h"
#include "DecoupledMidiPort.h"
#include "CommandQueue.h"
#include "MidiStateTracker.h"
#include "types.h"
#include "Backend.h"
#include "ConnectedPort.h"
#include "ConnectedChannel.h"
#include "ConnectedFXChain.h"
#include "ConnectedDecoupledMidiPort.h"
#include "ConnectedLoop.h"
#include "DummyAudioSystem.h"

#ifdef SHOOP_HAVE_BACKEND_JACK
#include "JackAudioSystem.h"
#include "JackMidiPort.h"
#include "JackAudioPort.h"
#endif

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <chrono>
#include <cmath>
#include <math.h>
#include <memory>
#include <stdexcept>
#include <map>
#include <set>
#include <thread>

#include "libshoopdaloop_test_if.h"

using namespace logging;
using namespace shoop_constants;
using namespace shoop_types;



// GLOBALS
namespace {
std::set<std::shared_ptr<Backend>> g_active_backends;
}

std::set<std::shared_ptr<Backend>> &get_active_backends() {
    return g_active_backends;
}

// HELPER FUNCTIONS
std::shared_ptr<Backend> internal_backend(shoopdaloop_backend_instance_t *backend) {
    return ((Backend*)backend)->shared_from_this();
}

std::shared_ptr<ConnectedPort> internal_audio_port(shoopdaloop_audio_port_t *port) {
    return ((ConnectedPort*)port)->shared_from_this();
}

std::shared_ptr<ConnectedPort> internal_midi_port(shoopdaloop_midi_port_t *port) {
    return ((ConnectedPort*)port)->shared_from_this();
}

std::shared_ptr<ConnectedChannel> internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan) {
    return ((ConnectedChannel*)chan)->shared_from_this();
}

std::shared_ptr<ConnectedChannel> internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan) {
    return ((ConnectedChannel*)chan)->shared_from_this();
}

#warning make the handles point to globally stored weak pointers to avoid trying to access deleted shared object
std::shared_ptr<ConnectedLoop> internal_loop(shoopdaloop_loop_t *loop) {
    return ((ConnectedLoop*)loop)->shared_from_this();
}

std::shared_ptr<ConnectedFXChain> internal_fx_chain(shoopdaloop_fx_chain_t *chain) {
    return ((ConnectedFXChain*)chain)->shared_from_this();
}

shoopdaloop_backend_instance_t *external_backend(std::shared_ptr<Backend> backend) {
    return (shoopdaloop_backend_instance_t*) backend.get();
}

shoopdaloop_audio_port_t* external_audio_port(std::shared_ptr<ConnectedPort> port) {
    return (shoopdaloop_audio_port_t*) port.get();
}

shoopdaloop_midi_port_t* external_midi_port(std::shared_ptr<ConnectedPort> port) {
    return (shoopdaloop_midi_port_t*) port.get();
}

shoopdaloop_loop_audio_channel_t* external_audio_channel(std::shared_ptr<ConnectedChannel> port) {
    return (shoopdaloop_loop_audio_channel_t*) port.get();
}

shoopdaloop_loop_midi_channel_t* external_midi_channel(std::shared_ptr<ConnectedChannel> port) {
    return (shoopdaloop_loop_midi_channel_t*) port.get();
}

shoopdaloop_loop_t* external_loop(std::shared_ptr<ConnectedLoop> loop) {
    return (shoopdaloop_loop_t*)loop.get();
}

shoopdaloop_fx_chain_t* external_fx_chain(std::shared_ptr<ConnectedFXChain> chain) {
    return (shoopdaloop_fx_chain_t*)chain.get();
}

audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f) {
    auto d = new audio_channel_data_t;
    d->n_samples = f.size();
    d->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * f.size());
    memcpy((void*)d->data, (void*)f.data(), sizeof(audio_sample_t) * f.size());
    return d;
}

midi_sequence_t *external_midi_data(std::vector<_MidiMessage> m) {
    auto d = new midi_sequence_t;
    d->n_events = m.size();
    d->events = (midi_event_t**) malloc(sizeof(midi_event_t*) * m.size());
    for (uint32_t idx=0; idx < m.size(); idx++) {
        auto e = alloc_midi_event(m[idx].size);
        e->size = m[idx].size;
        e->time = m[idx].time;
        memcpy((void*)e->data, (void*)m[idx].data.data(), m[idx].size);
        d->events[idx] = e;
    }
    return d;
}

std::vector<float> internal_audio_data(audio_channel_data_t const& d) {
    auto r = std::vector<float>(d.n_samples);
    memcpy((void*)r.data(), (void*)d.data, sizeof(audio_sample_t)*d.n_samples);
    return r;
}

std::vector<_MidiMessage> internal_midi_data(midi_sequence_t const& d) {
    auto r = std::vector<_MidiMessage>(d.n_events);
    for (uint32_t idx=0; idx < d.n_events; idx++) {
        auto &from = *d.events[idx];
        _MidiMessage m(
            from.time,
            from.size,
            std::vector<uint8_t>(from.size));
        memcpy((void*)m.data.data(), (void*)from.data, from.size);
        r[idx] = m;
    }
    return r;
}

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(std::shared_ptr<ConnectedDecoupledMidiPort> port) {
    return (shoopdaloop_decoupled_midi_port_t*) port.get();
}

std::shared_ptr<ConnectedDecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
    return ((ConnectedDecoupledMidiPort*)port)->shared_from_this();
}

PortDirection internal_port_direction(port_direction_t d) {
    return d == Input ? PortDirection::Input : PortDirection::Output;
}

std::optional<audio_system_type_t> audio_system_type(AudioSystemInterface *sys) {
    if (!sys) {
        return std::nullopt;
    }
#ifdef SHOOP_HAVE_BACKEND_JACK
    else if (dynamic_cast<JackAudioSystem*>(sys)) {
        return Jack;
    } 
#endif
    else if (dynamic_cast<_DummyAudioSystem*>(sys)) {
        return Dummy;
    } else {
        throw std::runtime_error("Unimplemented");
    }
}

// A lot of the API functions queue operations to be executed in the process thread.
// That means that in turn, other APIs cannot always rely on members of structures
// to already be fully initialized.
// This helper is useful for getting a result if it is ready, or if it is not,
// waiting for one process iteration and then getting it.
// The use-case is if you think some result may already be there to use, but
// if it isn't, you are sure it will be there after the next process thread iteration.
template<typename RType>
RType evaluate_before_or_after_process(std::function<RType()> fn, bool predicate, CommandQueue &queue) {
    if (predicate) { return fn(); }
    else {
        queue.queue_and_wait([](){});
        return fn();
    }
}

// API FUNCTIONS

shoopdaloop_backend_instance_t *initialize (
    audio_system_type_t audio_system,
    const char* client_name_hint,
    const char* argstring) {
    
    auto backend = std::make_shared<Backend>(audio_system, client_name_hint, argstring);
    backend->start();
    g_active_backends.insert(backend);

    auto rval = external_backend(backend);
    logging::log<"Backend.API", debug>("initialize"); // TODO fmt fix to print pointer
    return rval;
}

void terminate_backend(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("terminate");
    auto _backend = internal_backend(backend);
    _backend->terminate();
    g_active_backends.erase(_backend);
}

void* maybe_jack_client_handle(shoopdaloop_backend_instance_t* backend) {
    return internal_backend(backend)->maybe_jack_client_handle();
}

const char *get_client_name(shoopdaloop_backend_instance_t *backend) {
    return internal_backend(backend)->get_client_name();
}

unsigned get_sample_rate(shoopdaloop_backend_instance_t *backend) {
    return internal_backend(backend)->get_sample_rate();
}

unsigned get_buffer_size(shoopdaloop_backend_instance_t *backend) {
    return internal_backend(backend)->get_buffer_size();
}

backend_state_info_t *get_backend_state(shoopdaloop_backend_instance_t *backend) {
    auto val = internal_backend(backend)->get_state();
    auto rval = new backend_state_info_t;
    *rval = val;
    return rval;
}

profiling_report_t *get_profiling_report(shoopdaloop_backend_instance_t *backend) {
    auto internal = internal_backend(backend);

    if (!profiling::g_ProfilingEnabled) {
        internal->log<error>("Profiling support was disabled at compile-time.");
    }

    auto r = internal->profiler->report();    

    auto rval = new profiling_report_t;
    auto items = new profiling_report_item_t[r.size()];
    for (uint32_t idx=0; idx<r.size(); idx++) {
        auto name_str = (char*) malloc(r[idx].key.size() + 1);
        strcpy(name_str, r[idx].key.c_str());
        items[idx].key = name_str;
        items[idx].average = r[idx].avg;
        items[idx].most_recent = r[idx].most_recent;
        items[idx].n_samples = r[idx].n_samples;
        items[idx].worst = r[idx].worst;
    }

    rval->items = items;
    rval->n_items = r.size();
    return rval;
}

shoopdaloop_loop_t *create_loop(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("create_loop");
    auto rval = external_loop(internal_backend(backend)->create_loop());
    return rval;
}

shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, channel_mode_t mode) {
    logging::log<"Backend.API", debug>("add_audio_channel");
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    std::shared_ptr<ConnectedLoop> loop_info = internal_loop(loop);
    auto &backend = loop_info->get_backend();
    auto r = std::make_shared<ConnectedChannel> (nullptr, loop_info, backend.shared_from_this());
    backend.cmd_queue.queue([r, loop_info, mode]() {
        auto chan = loop_info->loop->add_audio_channel<audio_sample_t>(loop_info->backend.lock()->audio_buffer_pool,
                                                        audio_channel_initial_buffers,
                                                        mode,
                                                        false);
        r->channel = chan;
        loop_info->mp_audio_channels.push_back(r);
        logging::log<"Backend.API", debug>("add_audio_channel: executed on process thread");
    });
    return external_audio_channel(r);
}

shoopdaloop_loop_midi_channel_t *add_midi_channel (shoopdaloop_loop_t *loop, channel_mode_t mode) {
    logging::log<"Backend.API", debug>("add_midi_channel");
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    std::shared_ptr<ConnectedLoop> loop_info = internal_loop(loop);
    auto &backend = loop_info->get_backend();
    auto r = std::make_shared<ConnectedChannel> (nullptr, loop_info, backend.shared_from_this());
    backend.cmd_queue.queue([loop_info, mode, r]() {
        auto chan = loop_info->loop->add_midi_channel<Time, Size>(midi_storage_size, mode, false);
        r->channel = chan;
        loop_info->mp_midi_channels.push_back(r);
        logging::log<"Backend.API", debug>("add_midi_channel: executed on process thread");
    });
    return external_midi_channel(r);
}

shoopdaloop_loop_audio_channel_t *get_audio_channel (shoopdaloop_loop_t *loop, unsigned idx) {
    throw std::runtime_error("get_audio_channel not implemented\n");
    //return external_audio_channel(internal_loop(loop)->loop->audio_channel<audio_sample_t>(idx));
}

shoopdaloop_loop_midi_channel_t *get_midi_channel (shoopdaloop_loop_t *loop, unsigned idx) {
    throw std::runtime_error("get_midi_channel not implemented\n");
    //return external_midi_channel(internal_loop(loop)->loop->midi_channel<Time, Size>(idx));
}

unsigned get_n_audio_channels (shoopdaloop_loop_t *loop) {
    return internal_loop(loop)->loop->n_audio_channels();
}

unsigned get_n_midi_channels (shoopdaloop_loop_t *loop) {
    return internal_loop(loop)->loop->n_midi_channels();
}

void delete_audio_channel(shoopdaloop_loop_t *loop, shoopdaloop_loop_audio_channel_t *channel) {    
    logging::log<"Backend.API", debug>("delete_audio_channel");
    auto &loop_info = *internal_loop(loop);
    uint32_t idx;
    loop_info.delete_audio_channel(internal_audio_channel(channel));
}

void delete_midi_channel(shoopdaloop_loop_t *loop, shoopdaloop_loop_midi_channel_t *channel) {
    logging::log<"Backend.API", debug>("delete_midi_channel");
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel(internal_midi_channel(channel));
}

void delete_audio_channel_idx(shoopdaloop_loop_t *loop, unsigned idx) {
    logging::log<"Backend.API", debug>("delete_audio_channel_idx");
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_audio_channel_idx(idx);
}

void delete_midi_channel_idx(shoopdaloop_loop_t *loop, unsigned idx) {
    logging::log<"Backend.API", debug>("delete_midi_channel_idx");
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel_idx(idx);
}

void connect_audio_output(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
    logging::log<"Backend.API", debug>("connect_audio_output");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_midi_output(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
    logging::log<"Backend.API", debug>("connect_midi_output");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_audio_input(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
    logging::log<"Backend.API", debug>("connect_audio_input");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void connect_midi_input(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
    logging::log<"Backend.API", debug>("connect_midi_input");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void disconnect_audio_output (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
    logging::log<"Backend.API", debug>("disconnect_audio_output");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_midi_output (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
    logging::log<"Backend.API", debug>("disconnect_midi_output");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel) {
    logging::log<"Backend.API", debug>("disconnect_audio_outputs");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

void disconnect_midi_outputs  (shoopdaloop_loop_midi_channel_t  *channel) {
    logging::log<"Backend.API", debug>("disconnect_midi_outputs");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

void disconnect_audio_input (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
    logging::log<"Backend.API", debug>("disconnect_audio_input");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_input_port(_port, false);
    });
}

void disconnect_midi_input (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
    logging::log<"Backend.API", debug>("disconnect_midi_input");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_input_port(_port, false);
    });
}

void disconnect_audio_inputs (shoopdaloop_loop_audio_channel_t *channel) {
    logging::log<"Backend.API", debug>("disconnect_audio_inputs");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_input_ports(false);
    });
}

void disconnect_midi_inputs  (shoopdaloop_loop_midi_channel_t  *channel) {
    logging::log<"Backend.API", debug>("disconnect_midi_inputs");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_input_ports(false);
    });
}

audio_channel_data_t *get_audio_channel_data (shoopdaloop_loop_audio_channel_t *channel) {
    logging::log<"Backend.API", debug>("get_audio_channel_data");
    auto &chan = *internal_audio_channel(channel);
    return evaluate_before_or_after_process<audio_channel_data_t*>(
        [&chan]() { return external_audio_data(chan.maybe_audio()->get_data()); },
        chan.maybe_audio(),
        chan.backend.lock()->cmd_queue);
}

midi_sequence_t *get_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel) {
    logging::log<"Backend.API", debug>("get_midi_channel_data");
    auto &chan = *internal_midi_channel(channel);
    auto data = evaluate_before_or_after_process<std::vector<_MidiMessage>>(
        [&chan]() { return chan.maybe_midi()->retrieve_contents(); },
        chan.maybe_midi(),
        chan.backend.lock()->cmd_queue);

    return external_midi_data(data);
}

void load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t *data) {
    logging::log<"Backend.API", debug>("load_audio_channel_data");
    auto &chan = *internal_audio_channel(channel);
    evaluate_before_or_after_process<void>(
        [&chan, &data]() { chan.maybe_audio()->load_data(data->data, data->n_samples); },
        chan.maybe_audio(),
        chan.backend.lock()->cmd_queue);
}

void load_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel, midi_sequence_t  *data) {
    logging::log<"Backend.API", debug>("load_midi_channel_data");
    auto &chan = *internal_midi_channel(channel);
    auto _data = internal_midi_data(*data);
    evaluate_before_or_after_process<void>(
        [&]() { chan.maybe_midi()->set_contents(_data, data->length_samples); },
        chan.maybe_midi(),
        chan.backend.lock()->cmd_queue);
}

void clear_audio_channel_data_dirty (shoopdaloop_loop_audio_channel_t * channel) {
    auto &chan = *internal_audio_channel(channel);
    chan.clear_data_dirty();
}

void clear_midi_channel_data_dirty (shoopdaloop_loop_midi_channel_t * channel) {
    auto &chan = *internal_midi_channel(channel);
    chan.clear_data_dirty();
}

void loop_transition(shoopdaloop_loop_t *loop,
                      loop_mode_t mode,
                      unsigned delay, // In # of triggers
                      unsigned wait_for_sync) {
    logging::log<"Backend.API", debug>("loop_transition");
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &loop_info = *internal_loop(loop);
        loop_info.loop->plan_transition(mode, delay, wait_for_sync, false);
    });
}

void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      loop_mode_t mode,
                      unsigned delay, // In # of triggers
                      unsigned wait_for_sync) {
    logging::log<"Backend.API", debug>("loops_transition");
    auto internal_loops = std::make_shared<std::vector<std::shared_ptr<ConnectedLoop>>>(n_loops);
    for(uint32_t idx=0; idx<n_loops; idx++) {
        (*internal_loops)[idx] = internal_loop(loops[idx]);
    }
    internal_loop(loops[0])->get_backend().cmd_queue.queue([=]() {
        for (uint32_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *(*internal_loops)[idx];
            loop_info.loop->plan_transition(mode, delay, wait_for_sync, false);
        }
        // Ensure that sync is fully propagated
        for (uint32_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *(*internal_loops)[idx];
            loop_info.loop->PROC_handle_sync();
        }
    });
}

void clear_loop (shoopdaloop_loop_t *loop, unsigned length) {
    logging::log<"Backend.API", debug>("clear_loop");
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        _loop.loop->clear_planned_transitions(false);
        _loop.loop->plan_transition(Stopped, 0, false, false);
        for (auto &chan : _loop.mp_audio_channels) {
            chan->maybe_audio()->PROC_clear(length);
        }
        for (auto &chan : _loop.mp_midi_channels) {
            chan->maybe_midi()->clear(false);
        }
        _loop.loop->set_length(length, false);
    });
}

void set_loop_length (shoopdaloop_loop_t *loop, unsigned length) {
    logging::log<"Backend.API", debug>("set_loop_length");
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        _loop.loop->set_length(length, false);
    });
}

void set_loop_position (shoopdaloop_loop_t *loop, unsigned position) {
    logging::log<"Backend.API", debug>("set_loop_position");
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        _loop.loop->set_position(position, false);
    });
}

void clear_audio_channel (shoopdaloop_loop_audio_channel_t *channel, unsigned length) {
    logging::log<"Backend.API", debug>("clear_audio_channel");
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        internal_audio_channel(channel)->maybe_audio()->PROC_clear(length);
    });
}

void clear_midi_channel (shoopdaloop_loop_midi_channel_t *channel) {
    logging::log<"Backend.API", debug>("clear_midi_channel");
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        internal_midi_channel(channel)->maybe_midi()->clear(false);
    });
}

shoopdaloop_audio_port_t *open_audio_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    logging::log<"Backend.API", debug>("open_audio_port");
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_audio_port
        (name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<ConnectedPort>(port, _backend,
        internal_port_direction(direction) == PortDirection::Input ? shoop_types::ProcessWhen::BeforeFXChains : shoop_types::ProcessWhen::AfterFXChains
    );
    _backend->cmd_queue.queue([pi, _backend]() {
        _backend->ports.push_back(pi);
    });
    return external_audio_port(pi);
}

void close_audio_port (shoopdaloop_backend_instance_t *backend, shoopdaloop_audio_port_t *port) {
    logging::log<"Backend.API", debug>("close_audio_port");
    auto _backend = internal_backend(backend);
    auto pi = internal_audio_port(port);
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    _backend->cmd_queue.queue([=]() {
        _backend->ports.erase(
            std::remove_if(_backend->ports.begin(), _backend->ports.end(),
                [pi](auto const& e) { return e == pi; }),
            _backend->ports.end()
        );
    });
}

port_connections_state_t *get_audio_port_connections_state(shoopdaloop_audio_port_t *port) {
    auto connections = internal_audio_port(port)->maybe_audio()->get_external_connection_status();

    auto rval = new port_connections_state_t;
    rval->n_ports = connections.size();
    rval->ports = new port_maybe_connection_t[rval->n_ports];
    uint32_t idx = 0;
    for (auto &pair : connections) {
        rval->ports[idx].name = strdup(pair.first.c_str());
        rval->ports[idx].connected = pair.second;
        idx++;
    }
    return rval;
}

void connect_external_audio_port(shoopdaloop_audio_port_t *ours, const char* external_port_name) {
    internal_audio_port(ours)->maybe_audio()->connect_external(std::string(external_port_name));
}

void disconnect_external_audio_port(shoopdaloop_audio_port_t *ours, const char* external_port_name) {
    internal_audio_port(ours)->maybe_audio()->disconnect_external(std::string(external_port_name));
}

port_connections_state_t *get_midi_port_connections_state(shoopdaloop_midi_port_t *port) {
    auto connections = internal_midi_port(port)->maybe_midi()->get_external_connection_status();

    auto rval = new port_connections_state_t;
    rval->n_ports = connections.size();
    rval->ports = new port_maybe_connection_t[rval->n_ports];
    uint32_t idx = 0;
    for (auto &pair : connections) {
        rval->ports[idx].name = strdup(pair.first.c_str());
        rval->ports[idx].connected = pair.second;
        idx++;
    }
    return rval;
}

void destroy_port_connections_state(port_connections_state_t *d) {
    for (uint32_t idx=0; idx<d->n_ports; idx++) {
        free((void*)d->ports[idx].name);
    }
    delete[] d->ports;
    delete d;
}

void connect_external_midi_port(shoopdaloop_midi_port_t *ours, const char* external_port_name) {
    internal_midi_port(ours)->maybe_midi()->connect_external(std::string(external_port_name));
}

void disconnect_external_midi_port(shoopdaloop_midi_port_t *ours, const char* external_port_name) {
    internal_midi_port(ours)->maybe_midi()->disconnect_external(std::string(external_port_name));
}

void *get_audio_port_jack_handle(shoopdaloop_audio_port_t *port) {
#ifdef SHOOP_HAVE_BACKEND_JACK
    auto pi = internal_audio_port(port);
    auto _audio = pi->maybe_audio();
    if (pi->get_backend().m_audio_system_type != Jack) { return nullptr; }
    return evaluate_before_or_after_process<jack_port_t*>(
        [pi, _audio]() { return std::dynamic_pointer_cast<JackAudioPort>(_audio)->get_jack_port(); },
        _audio != nullptr,
        pi->backend.lock()->cmd_queue);
#else
    logging::log<"Backend.API", warning>("Requesting JACK handle but no JACK support built in.");
    return nullptr;
#endif
}

shoopdaloop_midi_port_t *open_midi_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    logging::log<"Backend.API", debug>("open_midi_port");
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<ConnectedPort>(port, _backend,
        internal_port_direction(direction) == PortDirection::Input ? shoop_types::ProcessWhen::BeforeFXChains : shoop_types::ProcessWhen::AfterFXChains);
    _backend->cmd_queue.queue([pi, _backend]() {
        _backend->ports.push_back(pi);
    });
    return external_midi_port(pi);
}

void close_midi_port (shoopdaloop_midi_port_t *port) {
    logging::log<"Backend.API", debug>("close_midi_port");
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    auto _port = internal_midi_port(port);
    _port->get_backend().cmd_queue.queue([=]() {
        auto &backend = _port->get_backend();
        backend.ports.erase(
            std::remove_if(backend.ports.begin(), backend.ports.end(),
                [_port](auto const& e) { return e == _port; }),
            backend.ports.end()
        );
    });
}

void *get_midi_port_jack_handle(shoopdaloop_midi_port_t *port) {
#ifdef SHOOP_HAVE_BACKEND_JACK
    auto pi = internal_midi_port(port);
    auto _midi = pi->maybe_midi();
    return evaluate_before_or_after_process<jack_port_t*>(
        [pi, _midi]() { return std::dynamic_pointer_cast<JackMidiPort>(_midi)->get_jack_port(); },
        _midi != nullptr,
        pi->backend.lock()->cmd_queue);
#else
    logging::log<"Backend.API", warning>("Requesting JACK handle but no JACK support built in.");
    return nullptr;
#endif
}

void add_audio_port_passthrough(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to) {
    logging::log<"Backend.API", debug>("add_audio_port_passthrough");
    auto _from = internal_audio_port(from);
    auto _to = internal_audio_port(to);
    _from->connect_passthrough(_to);
}

void add_midi_port_passthrough(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to) {
    logging::log<"Backend.API", debug>("add_midi_port_passthrough");
    auto _from = internal_midi_port(from);
    auto _to = internal_midi_port(to);
    _from->connect_passthrough(_to);
}

void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned int muted) {
    logging::log<"Backend.API", debug>("set_audio_port_muted {}", muted);
    internal_audio_port(port)->muted = (bool)muted;
}

void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned int muted) {
    logging::log<"Backend.API", debug>("set_audio_port_passthroughMuted {}", muted);
    internal_audio_port(port)->passthrough_muted = (bool)muted;
}

void set_audio_port_volume(shoopdaloop_audio_port_t *port, float volume) {
    logging::log<"Backend.API", debug>("set_audio_port_volume {}", volume);
    internal_audio_port(port)->volume = volume;
}

void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned int muted) {
    logging::log<"Backend.API", debug>("set_midi_port_muted {}", muted);
    internal_midi_port(port)->muted = (bool)muted;
}

void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned int muted) {
    logging::log<"Backend.API", debug>("set_midi_port_passthroughMuted {}", muted);
    internal_midi_port(port)->passthrough_muted = (bool)muted;
}

shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    logging::log<"Backend.API", debug>("open_decoupled_midi_port");
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto decoupled = std::make_shared<_DecoupledMidiPort>(port,
        decoupled_midi_port_queue_size,
        direction == Input ? PortDirection::Input : PortDirection::Output);
    auto r = std::make_shared<ConnectedDecoupledMidiPort>(decoupled, _backend);
    _backend->cmd_queue.queue_and_wait([=]() {
        _backend->decoupled_midi_ports.push_back(r);
    });
    return external_decoupled_midi_port(r);
}

midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port) {
    auto &_port = *internal_decoupled_midi_port(port);
    auto m = _port.port->pop_incoming();
    if (m.has_value()) {
        auto r = alloc_midi_event(m->data.size());
        r->time = 0;
        r->size = m->data.size();
        r->data = (uint8_t*)malloc(r->size);
        memcpy((void*)r->data, (void*)m->data.data(), r->size);
        return r;
    } else {
        return nullptr;
    }
}

void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
    logging::log<"Backend.API", debug>("close_decoupled_midi_port");
    internal_decoupled_midi_port(port)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_decoupled_midi_port(port);
        auto &ports = _port->backend.lock()->decoupled_midi_ports;
        ports.erase(
            std::remove_if(ports.begin(), ports.end(),
                [&_port](auto const& e) { return e.get() == _port.get(); }),
            ports.end()
        );
    });
}

const char* get_decoupled_midi_port_name(shoopdaloop_decoupled_midi_port_t *port) {
    logging::log<"Backend.API", debug>("get_decoupled_midi_port_name");
    return internal_decoupled_midi_port(port)->port->name();
}

void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data) {
    logging::log<"Backend.API", debug>("send_decoupled_midi");
    auto &_port = *internal_decoupled_midi_port(port);
    DecoupledMidiMessage m;
    m.data.resize(length);
    memcpy((void*)m.data.data(), (void*)data, length);
    _port.port->push_outgoing(m);
}

midi_event_t *alloc_midi_event(unsigned data_bytes) {
    auto r = new midi_event_t;
    r->size = data_bytes;
    r->time = 0;
    r->data = (unsigned char*) malloc(data_bytes);
    return r;
}

midi_sequence_t *alloc_midi_sequence(unsigned n_events) {
    auto r = new midi_sequence_t;
    r->n_events = n_events;
    r->events = (midi_event_t**)malloc (sizeof(midi_event_t*) * n_events);
    r->length_samples = 0;
    return r;
}

audio_channel_data_t *alloc_audio_channel_data(unsigned n_samples) {
    auto r = new audio_channel_data_t;
    r->n_samples = n_samples;
    r->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * n_samples);
    return r;
}

void set_audio_channel_volume (shoopdaloop_loop_audio_channel_t *channel, float volume) {
    logging::log<"Backend.API", debug>("set_audio_channel_volume");
    auto internal = internal_audio_channel(channel);
    // Perform atomically if possible, or queue if channel not yet initialized.
    if (internal->channel) {
        dynamic_cast<LoopAudioChannel*>(internal->channel.get())->set_volume(volume);
    } else {
        internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
            auto _channel = internal_audio_channel(channel);
            dynamic_cast<LoopAudioChannel*>(internal->channel.get())->set_volume(volume);
        });
    }
}

audio_channel_state_info_t *get_audio_channel_state (shoopdaloop_loop_audio_channel_t *channel) {
    auto r = new audio_channel_state_info_t;
    auto chan = internal_audio_channel(channel);
    auto audio = evaluate_before_or_after_process<LoopAudioChannel*>(
        [&]() { return chan->maybe_audio(); },
        chan->maybe_audio(),
        chan->backend.lock()->cmd_queue);
    r->output_peak = audio->get_output_peak();
    r->volume = audio->get_volume();
    r->mode = audio->get_mode();
    r->length = audio->get_length();
    r->start_offset = audio->get_start_offset();
    r->data_dirty = chan->get_data_dirty();
    r->n_preplay_samples = chan->channel->get_pre_play_samples();
    auto p = chan->channel->get_played_back_sample();
    if (p.has_value()) { r->played_back_sample = p.value(); } else { r->played_back_sample = -1; }
    audio->reset_output_peak();
    return r;
}

midi_channel_state_info_t *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel) {
    auto r = new midi_channel_state_info_t;
    auto chan = internal_midi_channel(channel);
    auto midi = evaluate_before_or_after_process<LoopMidiChannel*>(
        [&]() { return chan->maybe_midi(); },
        chan->maybe_midi(),
        chan->backend.lock()->cmd_queue);
    r->n_events_triggered = midi->get_n_events_triggered();
    r->n_notes_active = midi->get_n_notes_active();
    r->mode = midi->get_mode();
    r->length = midi->get_length();
    r->start_offset = midi->get_start_offset();
    r->data_dirty = chan->get_data_dirty();
    r->n_preplay_samples = chan->channel->get_pre_play_samples();
    auto p = chan->channel->get_played_back_sample();
    if (p.has_value()) { r->played_back_sample = p.value(); } else { r->played_back_sample = -1; }
    return r;
}

void set_audio_channel_mode (shoopdaloop_loop_audio_channel_t * channel, channel_mode_t mode) {
    logging::log<"Backend.API", debug>("set_audio_channel_mode");
    internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->channel->set_mode(mode);
    });
}

void set_midi_channel_mode (shoopdaloop_loop_midi_channel_t * channel, channel_mode_t mode) {
    logging::log<"Backend.API", debug>("set_midi_channel_mode");
    internal_midi_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->channel->set_mode(mode);
    });
}

void set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t * channel, int start_offset) {
    logging::log<"Backend.API", debug>("set_audio_channel_start_offset");
    internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->channel->set_start_offset(start_offset);
    });
}

void set_midi_channel_start_offset (shoopdaloop_loop_midi_channel_t * channel, int start_offset) {
    logging::log<"Backend.API", debug>("set_midi_channel_start_offset");
    internal_midi_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->channel->set_start_offset(start_offset);
    });
}

void set_audio_channel_n_preplay_samples (shoopdaloop_loop_audio_channel_t *channel, unsigned n) {
    logging::log<"Backend.API", debug>("set_audio_channel_n_preplay_samples");
    internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->channel->set_pre_play_samples(n);
    });
}

void set_midi_channel_n_preplay_samples  (shoopdaloop_loop_midi_channel_t *channel, unsigned n) {
    logging::log<"Backend.API", debug>("set_midi_channel_n_preplay_samples");
    internal_midi_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->channel->set_pre_play_samples(n);
    });
}

audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port) {
    auto r = new audio_port_state_info_t;
    auto p = internal_audio_port(port);
    r->peak = p->peak;
    r->volume = p->volume;
    r->muted = p->muted;
    r->passthrough_muted = p->passthrough_muted;
    r->name = strdup(p->maybe_audio()->name());
    p->peak = 0.0f;
    return r;
}

midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port) {
    auto r = new midi_port_state_info_t;
    auto p = internal_midi_port(port);
    r->n_events_triggered = p->n_events_processed;
    r->n_notes_active = p->maybe_midi_state->n_notes_active();
    r->muted = p->muted;
    r->passthrough_muted = p->passthrough_muted;
    r->name = strdup(p->maybe_midi()->name());
    p->n_events_processed = 0;
    return r;
}

loop_state_info_t *get_loop_state(shoopdaloop_loop_t *loop) {
    auto r = new loop_state_info_t;
    auto _loop = internal_loop(loop);
    r->mode = _loop->loop->get_mode();
    r->position = _loop->loop->get_position();
    r->length = _loop->loop->get_length();
    loop_mode_t next_mode;
    uint32_t next_delay;
    _loop->loop->get_first_planned_transition(next_mode, next_delay);
    r->maybe_next_mode = next_mode;
    r->maybe_next_mode_delay = next_delay;
    return r;
}

void set_loop_sync_source (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source) {
    logging::log<"Backend.API", debug>("set_loop_sync_source");
    internal_loop(loop)->backend.lock()->cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        if (sync_source) {
            auto &src = *internal_loop(sync_source);
            _loop.loop->set_sync_source(src.loop, false);
        } else {
            _loop.loop->set_sync_source(nullptr, false);
        }
    });
}

shoopdaloop_fx_chain_t *create_fx_chain(shoopdaloop_backend_instance_t *backend, fx_chain_type_t type, const char* title) {
    logging::log<"Backend.API", debug>("create_fx_chain");
    return external_fx_chain(internal_backend(backend)->create_fx_chain(type, title));
}

void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible) {
    logging::log<"Backend.API", debug>("fx_chain_set_ui_visible");
    auto maybe_ui = dynamic_cast<ExternalUIInterface*>(internal_fx_chain(chain)->chain.get());
    if(maybe_ui) {
        if (visible) {
            maybe_ui->show();
        } else {
            maybe_ui->hide();
        }
    } else {
        logging::log<"Backend.API", debug>("fx_chain_set_ui_visible: chain does not support UI, ignoring");
    }
}

fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain) {
    auto r = new fx_chain_state_info_t;
    auto c = internal_fx_chain(chain);
    r->ready = (unsigned) c->chain->is_ready();
    r->active = (unsigned) c->chain->is_active();
    auto maybe_ui = dynamic_cast<ExternalUIInterface*>(internal_fx_chain(chain)->chain.get());
    if(maybe_ui) {
        r->visible = (unsigned) maybe_ui->visible();
    } else {
        r->visible = 0;
    }
    return r;
}

void set_fx_chain_active(shoopdaloop_fx_chain_t *chain, unsigned active) {
    logging::log<"Backend.API", debug>("set_fx_chain_active {}", active);
    internal_fx_chain(chain)->chain->set_active(active);
}

const char* get_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain) {
    logging::log<"Backend.API", debug>("get_fx_chain_internal_state");
    auto c = internal_fx_chain(chain);
    auto maybe_serializeable = dynamic_cast<SerializeableStateInterface*>(c->chain.get());
    if(maybe_serializeable) {
        auto str = maybe_serializeable->serialize_state();
        char * rval = (char*) malloc(str.size() + 1);
        memcpy((void*)rval, str.data(), str.size());
        rval[str.size()] = 0;
        return rval;
    } else {
        return "";
    }
}

void restore_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain, const char* state) {
    logging::log<"Backend.API", debug>("restore_fx_chain_internal_state");
    auto c = internal_fx_chain(chain);
    auto maybe_serializeable = dynamic_cast<SerializeableStateInterface*>(c->chain.get());
    if (maybe_serializeable) {
        maybe_serializeable->deserialize_state(std::string(state));
    }
}

 


unsigned n_fx_chain_audio_input_ports(shoopdaloop_fx_chain_t *chain) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->audio_input_ports();
    return ports.size();
}

shoopdaloop_audio_port_t *fx_chain_audio_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
    auto _chain = internal_fx_chain(chain);
    auto port = _chain->audio_input_ports()[idx];
    return external_audio_port(port);
}

unsigned n_fx_chain_audio_output_ports(shoopdaloop_fx_chain_t *chain) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->audio_output_ports();
    return ports.size();
}

unsigned n_fx_chain_midi_input_ports(shoopdaloop_fx_chain_t *chain) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->midi_input_ports();
    return ports.size();
}
;
shoopdaloop_audio_port_t *fx_chain_audio_output_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
    auto _chain = internal_fx_chain(chain);
    auto port = _chain->audio_output_ports()[idx];
    return external_audio_port(port);
}

shoopdaloop_midi_port_t *fx_chain_midi_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
    auto _chain = internal_fx_chain(chain);
    auto port = _chain->midi_input_ports()[idx];
    return external_midi_port(port);
}

void destroy_midi_event(midi_event_t *e) {
    free(e->data);
    delete e;
}

void destroy_midi_sequence(midi_sequence_t *d) {
    for(uint32_t idx=0; idx<d->n_events; idx++) {
        destroy_midi_event(d->events[idx]);
    }
    free(d->events);
    delete d;
}

void destroy_audio_channel_data(audio_channel_data_t *d) {
    free(d->data);
    delete d;
}

void destroy_audio_channel_state_info(audio_channel_state_info_t *d) {
    delete d;
}

void destroy_midi_channel_state_info(midi_channel_state_info_t *d) {
    delete d;
}

void destroy_loop(shoopdaloop_loop_t *d) {
    logging::log<"Backend.API", debug>("destroy_loop");
    auto loop = internal_loop(d);
    auto backend = loop->backend.lock();
    backend->cmd_queue.queue_and_wait([loop, backend]() {
        loop->delete_all_channels(false);

        bool found = false;
        for(auto &elem : backend->loops) {
            if(elem == loop) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find loop to destroy");
        }
    });
}

void destroy_audio_port(shoopdaloop_audio_port_t *d) {
    logging::log<"Backend.API", debug>("destroy_audio_port");
    auto port = internal_audio_port(d);
    auto backend = port->backend.lock();
    backend->cmd_queue.queue_and_wait([port, backend]() {
        // Remove port, which should stop anything from accessing it
        bool found = false;
        for(auto &elem : backend->ports) {
            if(elem == port) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find audio port to destroy");
        }
    });
    // Now also close the port for the audio back-end
    port->port->close();
}

void destroy_midi_port(shoopdaloop_midi_port_t *d) {
    logging::log<"Backend.API", debug>("destroy_midi_port");
    auto port = internal_midi_port(d);
    auto backend = port->backend.lock();
    backend->cmd_queue.queue_and_wait([port, backend]() {
        // Remove port, which should stop anything from accessing it
        bool found = false;
        for(auto &elem : backend->ports) {
            if(elem == port) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find audio port to destroy");
        }
    });
    // Now also close the port for the audio back-end
    port->port->close();
}

void destroy_midi_port_state_info(midi_port_state_info_t *d) {
    delete d;
}

void destroy_audio_port_state_info(audio_port_state_info_t *d) {
    delete d;
}

void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d) {
    logging::log<"Backend.API", debug>("destroy_audio_channel");
    auto chan = internal_audio_channel(d);
    if(auto l = chan->loop.lock()) {
        l->delete_audio_channel(chan, true);
    }
}

void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d) {
    logging::log<"Backend.API", debug>("destroy_midi_channel");
    auto chan = internal_midi_channel(d);
    if(auto l = chan->loop.lock()) {
        l->delete_midi_channel(chan, true);
    }
}

void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d) {
    logging::log<"Backend.API", error>("destroy_shoopdaloop_decoupled_midi_port");
    throw std::runtime_error("unimplemented");
}

void destroy_loop_state_info(loop_state_info_t *state) {
    delete state;
}

void destroy_fx_chain(shoopdaloop_fx_chain_t *chain) {
    logging::log<"Backend.API", debug>("destroy_fx_chain");
    std::cerr << "Warning: destroying FX chains is unimplemented. Stopping only." << std::endl;
    internal_fx_chain(chain)->chain->stop();
}

void destroy_fx_chain_state(fx_chain_state_info_t *d) {
    delete d;
}

void destroy_string(const char* s) {
    free((void*)s);
}

void destroy_backend_state_info(backend_state_info_t *d) {
    delete d;
}

void destroy_profiling_report(profiling_report_t *d) {
    for(uint32_t idx=0; idx < d->n_items; idx++) {
        free ((void*)d->items[idx].key);
    }
    free(d->items);
    free(d);
}

shoopdaloop_logger_t *get_logger(const char* name) {
    static bool first = true;
    if (first) {
        logging::parse_conf_from_env();
        first = false;
    }
    return (shoopdaloop_logger_t*) nullptr; //TODO
    //(&logging::get_logger(std::string(name)));
}

const std::map<log_level_t, log_level_t> level_convert = {
    {trace, trace},
    {debug, debug},
    {info, info},
    {warning, warning},
    {error, error}
};

void set_global_logging_level(log_level_t level) {
    logging::log<"Backend.API", debug>("set_global_logging_level");
    logging::set_filter_level(level_convert.at(level));
}

void reset_logger_level_override(shoopdaloop_logger_t *logger) {
    logging::log<"Backend.API", debug>("reset_logger_level_override");
    #warning TODO
    //auto name = ((logging::logger*)logger)->name();
    //set_module_filter_level(std::string(name), std::nullopt);
}

void set_logger_level_override(shoopdaloop_logger_t *logger, log_level_t level) {
    logging::log<"Backend.API", debug>("set_logger_level_override");
    //auto name = ((logging::logger*)logger)->name();
    //set_module_filter_level(std::string(name), level_convert.at(level));
}

void shoopdaloop_log(shoopdaloop_logger_t *logger, log_level_t level, const char *msg) {
    // ((logging::logger*)logger)->log_no_filter(
    //     level_convert.at(level), msg
    // );
    #warning TODO
    logging::log<"TODOUNKNOWN", warning>(msg);
}

unsigned shoopdaloop_should_log(shoopdaloop_logger_t *logger, log_level_t level) {
    // return ((logging::logger*)logger)->should_log(
    //     level_convert.at(level)
    // ) ? 1 : 0;
    return true;
}

void dummy_audio_port_queue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t const* data) {
    logging::log<"Backend.API", debug>("dummy_audio_port_queue_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyAudioPort>(internal_audio_port(port)->maybe_audio());
    if (maybe_dummy) {
        maybe_dummy->queue_data(n_frames, data);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_port_queue_data called on non-dummy-audio port");
    }
}

void dummy_audio_port_dequeue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t *store_in) {
    logging::log<"Backend.API", debug>("dummy_audio_port_dequeue_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyAudioPort>(internal_audio_port(port)->maybe_audio());
    if (maybe_dummy) {
        auto data = maybe_dummy->dequeue_data(n_frames);
        memcpy((void*)store_in, (void*)data.data(), sizeof(audio_sample_t) * n_frames);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_port_queue_data called on non-dummy-audio port");
    }
}
void dummy_midi_port_queue_data(shoopdaloop_midi_port_t *port, midi_sequence_t* events) {
    logging::log<"Backend.API", debug>("dummy_midi_port_queue_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyMidiPort>(internal_midi_port(port)->maybe_midi());
    if (maybe_dummy) {
        for(uint32_t i=0; i<events->n_events; i++) {
            auto &e = events->events[i];
            maybe_dummy->queue_msg(
                e->size, e->time, e->data
            );
        }
    } else {
        logging::log<"Backend.API", error>("dummy_midi_port_queue_data called on non-dummy0midi port");
    }
}

midi_sequence_t *dummy_midi_port_dequeue_data(shoopdaloop_midi_port_t *port) {
    logging::log<"Backend.API", debug>("dummy_midi_port_dequeue_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyMidiPort>(internal_midi_port(port)->maybe_midi());
    if (maybe_dummy) {
        auto msgs = maybe_dummy->get_written_requested_msgs();
        midi_sequence_t *rval = alloc_midi_sequence(msgs.size());
        for (uint32_t i=0; i<msgs.size(); i++) {
            auto &e = msgs[i];
            rval->events[i] = alloc_midi_event(e.get_size());
            rval->events[i]->size = e.get_size();
            rval->events[i]->time = e.get_time();
            memcpy((void*)rval->events[i]->data, (void*)e.get_data(), e.get_size());
        }
        rval->n_events = msgs.size();
        rval->length_samples = msgs.size()? msgs.back().time+1 : 0;
        return rval;
    } else {
        logging::log<"Backend.API", error>("dummy_midi_port_dequeue_data called on non-dummy-midi port");
        return nullptr;
    }
}
void dummy_midi_port_request_data(shoopdaloop_midi_port_t* port, unsigned n_frames) {
    logging::log<"Backend.API", debug>("dummy_midi_port_queue_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyMidiPort>(internal_midi_port(port)->maybe_midi());
    if (maybe_dummy) {
        maybe_dummy->request_data(n_frames);
    }
}

void dummy_midi_port_clear_queues(shoopdaloop_midi_port_t* port) {
    logging::log<"Backend.API", debug>("dummy_midi_port_clear_queues");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyMidiPort>(internal_midi_port(port)->maybe_midi());
    if (maybe_dummy) {
        maybe_dummy->clear_queues();
    }
}

void dummy_audio_port_request_data(shoopdaloop_audio_port_t* port, unsigned n_frames) {
    logging::log<"Backend.API", debug>("dummy_audio_port_request_data");
    auto maybe_dummy = std::dynamic_pointer_cast<DummyAudioPort>(internal_audio_port(port)->maybe_audio());
    if (maybe_dummy) {
        maybe_dummy->request_data(n_frames);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_port_request_data called on non-dummy port");
    }
}

void dummy_audio_enter_controlled_mode(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("dummy_audio_enter_controlled_mode");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        maybe_dummy->enter_mode(DummyAudioSystemMode::Controlled);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_enter_controlled_mode called on non-dummy backend");
    }
}

void dummy_audio_enter_automatic_mode(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("dummy_audio_enter_automatic_mode");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        maybe_dummy->enter_mode(DummyAudioSystemMode::Automatic);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_enter_automatic_mode called on non-dummy backend");
    }
}

unsigned dummy_audio_is_in_controlled_mode(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("dummy_audio_is_in_controlled_mode");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        return (unsigned) (maybe_dummy->get_mode() == DummyAudioSystemMode::Controlled);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_is_in_controlled_mode called on non-dummy backend");
        return 0;
    }
}

void dummy_audio_request_controlled_frames(shoopdaloop_backend_instance_t *backend, unsigned n_frames) {
    logging::log<"Backend.API", debug>("dummy_audio_request_controlled_frames");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        maybe_dummy->controlled_mode_request_samples(n_frames);
    } else {
        logging::log<"Backend.API", error>("dummy_audio_request_controlled_frames called on non-dummy backend");
    }
}

unsigned dummy_audio_n_requested_frames(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("dummy_audio_n_requested_frames");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        return maybe_dummy->get_controlled_mode_samples_to_process();
    } else {
        logging::log<"Backend.API", error>("dummy_audio_n_requested_frames called on non-dummy backend");
        return 0;
    }
}

void dummy_audio_wait_process(shoopdaloop_backend_instance_t *backend) {
    logging::log<"Backend.API", debug>("dummy_audio_wait_process");
    auto _backend = internal_backend(backend);
    if (auto maybe_dummy = dynamic_cast<_DummyAudioSystem*>(_backend->audio_system.get())) {
        maybe_dummy->wait_process();
    } else {
        logging::log<"Backend.API", error>("dummy_audio_wait_process called on non-dummy backend");
    }
}
