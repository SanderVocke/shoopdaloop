// This file implements the libshoopdaloop_backend C API. It is also where
// internal C++ objects are tied together at the highest level.

#include "libshoopdaloop_backend.h"
#include "ExternalUIInterface.h"
#include "GraphAudioPort.h"
#include "GraphMidiPort.h"
#include "SerializeableStateInterface.h"
#include "WithCommandQueue.h"
#include "shoop_globals.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <cmath>
#include <math.h>
#include <memory>
#include <stdexcept>
#include <set>

#ifdef SHOOP_HAVE_BACKEND_JACK
#include "JackAudioMidiDriver.h"
#endif

// Internal
#include "AudioMidiLoop.h"
#include "AudioMidiDriver.h"
#include "ProcessingChainInterface.h"
#include "LoggingBackend.h"
#include "MidiMessage.h"
#include "MidiPort.h"
#include "MidiSortingBuffer.h"
#include "PortInterface.h"
#include "DecoupledMidiPort.h"
#include "CommandQueue.h"
#include "types.h"
#include "BackendSession.h"
#include "GraphPort.h"
#include "GraphLoopChannel.h"
#include "GraphFXChain.h"
#include "GraphLoop.h"
#include "DummyAudioMidiDriver.h"
#include "AudioMidiDrivers.h"
#include "Resample.h"

#include "libshoopdaloop_test_if.h"

using namespace logging;
using namespace shoop_constants;
using namespace shoop_types;

#ifdef _strdup
  #define strdup _strdup
#endif

// GLOBALS
namespace {
std::set<shoop_shared_ptr<BackendSession>> g_active_backends;
std::set<shoop_shared_ptr<AudioMidiDriver>> g_active_drivers;
}

std::set<shoop_shared_ptr<BackendSession>> &get_active_backends() {
    return g_active_backends;
}

// HELPER FUNCTIONS
shoop_shared_ptr<BackendSession> internal_backend_session(shoop_backend_session_t *backend) {
    if (!backend) { return nullptr; }
    return ((shoop_weak_ptr<BackendSession> *)backend)->lock();
}

shoop_shared_ptr<AudioMidiDriver> internal_audio_driver(shoop_audio_driver_t *driver) {
    if (!driver) { return nullptr; }
    return ((shoop_weak_ptr<AudioMidiDriver> *)driver)->lock();
}

shoop_shared_ptr<GraphPort> internal_audio_port(shoopdaloop_audio_port_t *port) {
    if (!port) { return nullptr; }
    return ((shoop_weak_ptr<GraphPort> *)port)->lock();
}

shoop_shared_ptr<GraphPort> internal_midi_port(shoopdaloop_midi_port_t *port) {
    if (!port) { return nullptr; }
    return ((shoop_weak_ptr<GraphPort> *)port)->lock();
}

shoop_shared_ptr<GraphLoopChannel> internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan) {
    if (!chan) { return nullptr; }
    return ((shoop_weak_ptr<GraphLoopChannel> *)chan)->lock();
}

shoop_shared_ptr<GraphLoopChannel> internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan) {
    if (!chan) { return nullptr; }
    return ((shoop_weak_ptr<GraphLoopChannel> *)chan)->lock();
}

//TODO: make the handles point to globally stored weak pointers to avoid trying to access deleted shared object
shoop_shared_ptr<GraphLoop> internal_loop(shoopdaloop_loop_t *loop) {
    if (!loop) { return nullptr; }
    return ((shoop_weak_ptr<GraphLoop> *)loop)->lock();
}

shoop_shared_ptr<GraphFXChain> internal_fx_chain(shoopdaloop_fx_chain_t *chain) {
    if (!chain) { return nullptr; }
    return ((shoop_weak_ptr<GraphFXChain> *)chain)->lock();
}

shoop_backend_session_t *external_backend_session(shoop_shared_ptr<BackendSession> backend) {
    auto weak = new shoop_weak_ptr<BackendSession>(backend);
    return (shoop_backend_session_t*) weak;
}

shoop_audio_driver_t *external_audio_driver(shoop_shared_ptr<AudioMidiDriver> driver) {
    auto weak = new shoop_weak_ptr<AudioMidiDriver>(driver);
    return (shoop_audio_driver_t*) weak;
}

shoopdaloop_audio_port_t* external_audio_port(shoop_shared_ptr<GraphPort> port) {
    auto weak = new shoop_weak_ptr<GraphPort>(port);
    return (shoopdaloop_audio_port_t*) weak;
}

shoopdaloop_midi_port_t* external_midi_port(shoop_shared_ptr<GraphPort> port) {
    auto weak = new shoop_weak_ptr<GraphPort>(port);
    return (shoopdaloop_midi_port_t*) weak;
}

shoopdaloop_loop_audio_channel_t* external_audio_channel(shoop_shared_ptr<GraphLoopChannel> port) {
    auto weak = new shoop_weak_ptr<GraphLoopChannel>(port);
    return (shoopdaloop_loop_audio_channel_t*) weak;
}

shoopdaloop_loop_midi_channel_t* external_midi_channel(shoop_shared_ptr<GraphLoopChannel> port) {
    auto weak = new shoop_weak_ptr<GraphLoopChannel>(port);
    return (shoopdaloop_loop_midi_channel_t*) weak;
}

shoopdaloop_loop_t* external_loop(shoop_shared_ptr<GraphLoop> loop) {
    auto weak = new shoop_weak_ptr<GraphLoop>(loop);
    return (shoopdaloop_loop_t*) weak;
}

shoopdaloop_fx_chain_t* external_fx_chain(shoop_shared_ptr<GraphFXChain> chain) {
    auto weak = new shoop_weak_ptr<GraphFXChain>(chain);
    return (shoopdaloop_fx_chain_t*) weak;
}

shoop_audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f) {
    auto d = new shoop_audio_channel_data_t;
    d->n_samples = f.size();
    d->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * f.size());
    memcpy((void*)d->data, (void*)f.data(), sizeof(audio_sample_t) * f.size());
    return d;
}

shoop_midi_sequence_t *external_midi_data(shoop_types::LoopMidiChannel::Contents const& m) {
    auto d = new shoop_midi_sequence_t;

    // The sequence will include state-defining messages and recorded messages.
    d->n_events = m.starting_state_msg_datas.size() + m.recorded_msgs.size();
    d->events = (shoop_midi_event_t**) malloc(sizeof(shoop_midi_event_t*) * d->n_events);

    // State-defining messages stored first at time=-1
    for (uint32_t idx=0; idx < m.starting_state_msg_datas.size(); idx++) {
        auto e = alloc_midi_event(m.starting_state_msg_datas[idx].size());
        e->size = m.starting_state_msg_datas[idx].size();
        e->time = -1;
        memcpy((void*)e->data, (void*)m.starting_state_msg_datas[idx].data(), m.starting_state_msg_datas[idx].size());
        d->events[idx] = e;
    }
    // Rest is recorded messages
    for (uint32_t idx=0; idx < m.recorded_msgs.size(); idx++) {
        auto e = alloc_midi_event(m.recorded_msgs[idx].size);
        e->size = m.recorded_msgs[idx].size;
        e->time = m.recorded_msgs[idx].time;
        memcpy((void*)e->data, (void*)m.recorded_msgs[idx].data.data(), m.recorded_msgs[idx].size);
        d->events[idx + m.starting_state_msg_datas.size()] = e;
    }

    return d;
}

std::vector<float> internal_audio_data(shoop_audio_channel_data_t const& d) {
    auto r = std::vector<float>(d.n_samples);
    memcpy((void*)r.data(), (void*)d.data, sizeof(audio_sample_t)*d.n_samples);
    return r;
}

shoop_types::LoopMidiChannel::Contents internal_midi_data(shoop_midi_sequence_t const& d) {
    shoop_types::LoopMidiChannel::Contents rval;
    for (uint32_t idx=0; idx < d.n_events; idx++) {
        auto &from = *d.events[idx];
        if (from.time < 0) {
          std::vector<uint8_t> data(from.size);
          memcpy((void*)data.data(), (void*)from.data, from.size);
          rval.starting_state_msg_datas.push_back(data);
        } else {
          _MidiMessage m(
            from.time,
            from.size,
            std::vector<uint8_t>(from.size));
          memcpy((void*)m.data.data(), (void*)from.data, from.size);
          rval.recorded_msgs.push_back(m);
        }
    }
    return rval;
}

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(shoop_shared_ptr<_DecoupledMidiPort> port) {
    auto weak = new shoop_weak_ptr<_DecoupledMidiPort>(port);
    return (shoopdaloop_decoupled_midi_port_t*) weak;
}

shoop_shared_ptr<_DecoupledMidiPort> internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
    auto rval = ((shoop_weak_ptr<_DecoupledMidiPort>*)port)->lock();
    if (!rval) {
        throw std::runtime_error("Attempt to access an invalid/expired decoupled midi port.");
    }
    return rval;
}

std::optional<shoop_audio_driver_type_t> audio_system_type(AudioMidiDriver *sys) {
    if (!sys) {
        return std::nullopt;
    }
#ifdef SHOOP_HAVE_BACKEND_JACK
    else if (dynamic_cast<JackAudioMidiDriver*>(sys)) {
        return Jack;
    }
#endif
    else if (dynamic_cast<_DummyAudioMidiDriver*>(sys)) {
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
template<typename RType>
RType evaluate_before_or_after_process(std::function<RType()> fn, bool predicate, WithCommandQueue &queue) {
    if (predicate) { return fn(); }
    else {
        queue.exec_process_thread_command([](){});
        return fn();
    }
}

template<typename ReturnType, shoop_log_level_t TraceLevel=log_level_debug, shoop_log_level_t ErrorLevel=log_level_error>
ReturnType api_impl(const char* name, std::function<ReturnType()> fn, ReturnType return_on_error) {
    try {
        ReturnType rval = fn();
        if constexpr (fmt::is_formattable<ReturnType>::value) {
            log<"Backend.API", TraceLevel>(std::nullopt, std::nullopt, "API called: {} -> {}", name, rval);
        } else {
            log<"Backend.API", TraceLevel>(std::nullopt, std::nullopt, "API called: {}", name);
        }
        return rval;
    } catch (std::exception &e) {
        log<"Backend.API", ErrorLevel>(std::nullopt, std::nullopt, "{} failed: {}", name, e.what());
    } catch (...) {
        log<"Backend.API", ErrorLevel>(std::nullopt, std::nullopt, "{} failed with unknown log_level_error", name);
    }
    return return_on_error;
}

typedef struct { } dummy_struct_t;

template<typename ReturnType=void, shoop_log_level_t TraceLevel=log_level_debug, shoop_log_level_t ErrorLevel=log_level_error>
void api_impl(const char* name, std::function<void()> fn) {
    api_impl<dummy_struct_t*, TraceLevel, ErrorLevel>(name, [&]() { fn(); return (dummy_struct_t*) nullptr; }, (dummy_struct_t*) nullptr);
}

// API FUNCTIONS

void initialize_logging() {
  return api_impl<void>("initialize_logging", [&]() {
    logging::parse_conf_from_env();
  });
}

shoop_backend_session_t *create_backend_session() {
  return api_impl<shoop_backend_session_t*>("create_backend_session", [&]() {
    auto rval = shoop_make_shared<BackendSession>();
    g_active_backends.insert(rval);
    return external_backend_session(rval);
  }, nullptr);
}

shoop_audio_driver_t *create_audio_driver (
    shoop_audio_driver_type_t type
) {
  return api_impl<shoop_audio_driver_t*>("create_audio_driver", [&]() {
    auto rval = create_audio_midi_driver(type);
    g_active_drivers.insert(rval);
    return external_audio_driver(rval);
  }, nullptr);
}

shoop_audio_driver_state_t *get_audio_driver_state(shoop_audio_driver_t *driver) {
  return api_impl<shoop_audio_driver_state_t*, log_level_debug_trace>("get_audio_driver_state", [&]() -> shoop_audio_driver_state_t* {
    auto rval = new shoop_audio_driver_state_t;
    auto d = internal_audio_driver(driver);
    if (!d) {
      return nullptr;
    }
    rval->maybe_driver_handle = d->get_maybe_client_handle();
    rval->maybe_instance_name = d->get_client_name() ? strdup(d->get_client_name()) : "(unknown)";
    rval->buffer_size = d->get_buffer_size();
    rval->sample_rate = d->get_sample_rate();
    rval->dsp_load_percent = d->get_dsp_load();
    rval->xruns_since_last = d->get_xruns();
    d->reset_xruns();
    rval->active = d->get_active();
    rval->last_processed = d->get_last_processed();
    return rval;
  }, (shoop_audio_driver_state_t*) nullptr);
}

void destroy_backend_session(shoop_backend_session_t *backend) {
    return api_impl<void>("destroy_backend_session", [&]() {
        auto _backend = internal_backend_session(backend);
        if (!_backend) { return; }
        for (auto &driver : g_active_drivers) {
          driver->remove_processor(shoop_static_pointer_cast<HasAudioProcessingFunction>(_backend));
        }
        _backend->destroy();
        g_active_backends.erase(_backend);
    });
}

void* maybe_driver_handle(shoop_audio_driver_t* driver) {
  return api_impl<void*>("maybe_driver_handle", [&]() -> void* {
    auto internal = internal_audio_driver(driver);
    if (!internal) { return nullptr; }
    return internal->get_maybe_client_handle();
  }, (void*) nullptr);
}

const char* maybe_driver_instance_name(shoop_audio_driver_t* driver) {
  return api_impl<const char*>("maybe_driver_instance_name", [&]() {
    auto internal = internal_audio_driver(driver);
    if (!internal) { return "unknown"; }
    return internal->get_client_name();
  }, (const char*) nullptr);
}

unsigned driver_type_supported(shoop_audio_driver_type_t type) {
  return api_impl<unsigned>("driver_type_supported", [&]() {
        return (unsigned) audio_midi_driver_type_supported(type);
  }, 0);
}

shoop_external_port_descriptors_t *find_external_ports(
  shoop_audio_driver_t *driver,
  const char* maybe_name_regex,
  shoop_port_direction_t maybe_port_direction_filter,
  shoop_port_data_type_t maybe_data_type_filter)
{
  std::string name_regex = maybe_name_regex ? maybe_name_regex : "";
  std::string fn_name = "find_external_ports (regex \"" + name_regex + "\")";
  return api_impl<shoop_external_port_descriptors_t*>(fn_name.c_str(), [&]() -> shoop_external_port_descriptors_t* {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return nullptr; }
    auto ports = _driver->find_external_ports(
      maybe_name_regex,
      maybe_port_direction_filter,
      maybe_data_type_filter
    );
    auto rval = new shoop_external_port_descriptors_t;
    rval->n_ports = ports.size();
    rval->ports = (shoop_external_port_descriptor_t*) malloc(sizeof(shoop_external_port_descriptor_t) * ports.size());
    for(size_t i=0; i<ports.size(); i++) {
      rval->ports[i].name = strdup(ports[i].name.c_str());
      rval->ports[i].direction = ports[i].direction;
      rval->ports[i].data_type = ports[i].data_type;
    }
    return rval;
  }, (shoop_external_port_descriptors_t*) nullptr);
}

shoop_result_t set_audio_driver(shoop_backend_session_t *backend, shoop_audio_driver_t *driver) {
  return api_impl<shoop_result_t>("set_audio_driver", [&]() -> shoop_result_t {
    auto _backend = internal_backend_session(backend);
    auto _driver = internal_audio_driver(driver);
    if (!_backend || !_driver) { return Failure; }
    _driver->queue_process_thread_command([_backend, _driver]() {
        _backend->set_buffer_size(_driver->get_buffer_size());
        _backend->set_sample_rate(_driver->get_sample_rate());
        _driver->add_processor(shoop_static_pointer_cast<HasAudioProcessingFunction>(_backend));
    });
    return Success;
  }, Failure);
}

shoop_backend_session_state_info_t *get_backend_session_state(shoop_backend_session_t *backend) {
  return api_impl<shoop_backend_session_state_info_t*, log_level_debug_trace, log_level_warning>("get_backend_session_state", [&]() -> shoop_backend_session_state_info_t* {
    auto _backend = internal_backend_session(backend);
    if (_backend) {
      auto rval = new shoop_backend_session_state_info_t;
      *rval = _backend->get_state();
      return rval;
    }
    return nullptr;
  }, new shoop_backend_session_state_info_t);
}

shoop_profiling_report_t *get_profiling_report(shoop_backend_session_t *backend) {
  return api_impl<shoop_profiling_report_t*>("get_profiling_report", [&]() -> shoop_profiling_report_t* {
    auto rval = new shoop_profiling_report_t;
    auto internal = internal_backend_session(backend);
    if (!internal) {
      return nullptr;
    }

    if (!profiling::g_ProfilingEnabled) {
        internal->log<log_level_error>("Profiling support was disabled at compile-time.");
    }

    auto r = internal->profiler->report();

    auto items = new shoop_profiling_report_item_t[r.size()];
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
  }, new shoop_profiling_report_t);
}

shoopdaloop_loop_t *create_loop(shoop_backend_session_t *backend) {
  return api_impl<shoopdaloop_loop_t*>("create_loop", [&]() -> shoopdaloop_loop_t* {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return nullptr; }
    auto rval = external_loop(_backend->create_loop());
    return rval;
  }, nullptr);
}

shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, shoop_channel_mode_t mode) {
  return api_impl<shoopdaloop_loop_audio_channel_t*>("add_audio_channel", [&]() -> shoopdaloop_loop_audio_channel_t* {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    shoop_shared_ptr<GraphLoop> loop_info = internal_loop(loop);
    if (!loop_info) { return nullptr; }
    auto &backend = loop_info->get_backend();
    auto r = backend.add_loop_channel(loop_info, nullptr);
    backend.queue_process_thread_command([r, loop_info, mode]() {
        auto _backend = loop_info->backend.lock();
        if (!_backend) { return; }
        auto chan = loop_info->loop->add_audio_channel<audio_sample_t>(_backend->audio_buffer_pool,
                                                        audio_channel_initial_buffers,
                                                        mode,
                                                        false);
        r->channel = shoop_static_pointer_cast<ChannelInterface>(chan);
        loop_info->mp_audio_channels.push_back(r);
        logging::log<"Backend.API", log_level_debug>(std::nullopt, std::nullopt, "add_audio_channel: executed on process thread");
    });
    backend.set_graph_node_changes_pending();
    return external_audio_channel(r);
  }, nullptr);
}

shoopdaloop_loop_midi_channel_t *add_midi_channel (shoopdaloop_loop_t *loop, shoop_channel_mode_t mode) {
  return api_impl<shoopdaloop_loop_midi_channel_t*>("add_midi_channel", [&]() -> shoopdaloop_loop_midi_channel_t* {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    shoop_shared_ptr<GraphLoop> loop_info = internal_loop(loop);
    if (!loop_info) { return nullptr; }
    auto &backend = loop_info->get_backend();
    auto r = backend.add_loop_channel(loop_info, nullptr);
    backend.queue_process_thread_command([loop_info, mode, r]() {
        auto chan = loop_info->loop->add_midi_channel(midi_storage_size, mode, false);
        r->channel = shoop_static_pointer_cast<ChannelInterface>(chan);
        loop_info->mp_midi_channels.push_back(r);
        logging::log<"Backend.API", log_level_debug>(std::nullopt, std::nullopt, "add_midi_channel: executed on process thread");
    });
    backend.set_graph_node_changes_pending();
    return external_midi_channel(r);
  }, nullptr);
}

unsigned get_n_audio_channels (shoopdaloop_loop_t *loop) {
  return api_impl<unsigned>("get_n_audio_channels", [&]() -> unsigned {
    auto _loop = internal_loop(loop);
    if (!_loop) { return 0; }
    return _loop->loop->n_audio_channels();
  }, 0);
}

unsigned get_n_midi_channels (shoopdaloop_loop_t *loop) {
  return api_impl<unsigned>("get_n_midi_channels", [&]() -> unsigned {
    auto _loop = internal_loop(loop);
    if (!_loop) { return 0; }
    return _loop->loop->n_midi_channels();
  }, 0);
}

void delete_audio_channel(shoopdaloop_loop_t *loop, shoopdaloop_loop_audio_channel_t *channel) {
  return api_impl<void>("delete_audio_channel", [&]() {
    auto loop_info = internal_loop(loop);
    if (!loop_info) { return; }
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    loop_info->delete_audio_channel(_chan);
  });
}

void delete_midi_channel(shoopdaloop_loop_t *loop, shoopdaloop_loop_midi_channel_t *channel) {
  return api_impl<void>("delete_midi_channel", [&]() {
    auto loop_info = internal_loop(loop);
    if (!loop_info) { return; }
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    loop_info->delete_midi_channel(_chan);
  });
}

void delete_audio_channel_idx(shoopdaloop_loop_t *loop, unsigned idx) {
  return api_impl<void>("delete_audio_channel_idx", [&]() {
    auto loop_info = internal_loop(loop);
    if (!loop_info) { return; }
    loop_info->delete_audio_channel_idx(idx);
  });
}

void delete_midi_channel_idx(shoopdaloop_loop_t *loop, unsigned idx) {
  return api_impl<void>("delete_midi_channel_idx", [&]() {
    auto loop_info = internal_loop(loop);
    if (!loop_info) { return; }
    loop_info->delete_midi_channel_idx(idx);
  });
}

void connect_audio_output(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
  return api_impl<void>("connect_audio_output", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto &backend = _chan->get_backend();
    backend.queue_process_thread_command([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        if (_port && _channel) {
          _channel->connect_output_port(_port, false);
        }
    });
    backend.set_graph_node_changes_pending();
  });
}

void connect_midi_output(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
  return api_impl<void>("connect_midi_output", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto &backend = _chan->get_backend();
    backend.queue_process_thread_command([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        if (_port && _channel) {
          _channel->connect_output_port(_port, false);
        }
    });
    backend.set_graph_node_changes_pending();
  });
}

void connect_audio_input(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
  return api_impl<void>("connect_audio_input", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto &backend = _chan->get_backend();
    backend.queue_process_thread_command([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        if (_port && _channel) {
          _channel->connect_input_port(_port, false);
        }
    });
    backend.set_graph_node_changes_pending();
  });
}

void connect_midi_input(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
  return api_impl<void>("connect_midi_input", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto &backend = _chan->get_backend();
    backend.queue_process_thread_command([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        if (_port && _channel) {
          _channel->connect_input_port(_port, false);
        }
    });
    backend.set_graph_node_changes_pending();
  });
}

void disconnect_audio_output (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
  return api_impl<void>("disconnect_audio_output", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_output_port(_port, false);
        }
    });
  });
}

void disconnect_audio_port (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
  return api_impl<void>("disconnect_audio_output", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_port(_port, false);
        }
    });
  });
}

void disconnect_midi_output (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
  return api_impl<void>("disconnect_midi_output", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_output_port(_port, false);
        }
    });
  });
}

void disconnect_midi_port (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
  return api_impl<void>("disconnect_midi_output", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_port(_port, false);
        }
    });
  });
}

void disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel) {
  return api_impl<void>("disconnect_audio_outputs", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (_channel) {
          _channel->disconnect_output_ports(false);
        }
    });
  });
}

void disconnect_midi_outputs (shoopdaloop_loop_midi_channel_t  *channel) {
  return api_impl<void>("disconnect_midi_outputs", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (_channel) {
          _channel->disconnect_output_ports(false);
        }
    });
  });
}

void disconnect_audio_input (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
  return api_impl<void>("disconnect_audio_input", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_input_port(_port, false);
        }
    });
  });
}

void disconnect_midi_input (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
  return api_impl<void>("disconnect_midi_input", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        if (_port && _channel) {
          _channel->disconnect_input_port(_port, false);
        }
    });
  });
}

void disconnect_audio_inputs (shoopdaloop_loop_audio_channel_t *channel) {
  return api_impl<void>("disconnect_audio_inputs", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (_channel) {
          _channel->disconnect_input_ports(false);
        }
    });
  });
}

void disconnect_midi_inputs  (shoopdaloop_loop_midi_channel_t  *channel) {
  return api_impl<void>("disconnect_midi_inputs", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (_channel) {
          _channel->disconnect_input_ports(false);
        }
    });
  });
}

shoop_audio_channel_data_t *get_audio_channel_data (shoopdaloop_loop_audio_channel_t *channel) {
  return api_impl<shoop_audio_channel_data_t*>("get_audio_channel_data", [&]() -> shoop_audio_channel_data_t* {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return nullptr; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return nullptr; }
    return evaluate_before_or_after_process<shoop_audio_channel_data_t*>(
        [=]() -> shoop_audio_channel_data_t* {
          auto _channel = internal_audio_channel(channel);
          if (!_channel) { return nullptr; }
          return external_audio_data(_channel->maybe_audio()->get_data());
        },
        _chan->maybe_audio(),
        *_backend);
  }, nullptr);
}

shoop_midi_sequence_t *get_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel) {
  return api_impl<shoop_midi_sequence_t*>("get_midi_channel_data", [&]() -> shoop_midi_sequence_t* {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return nullptr; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return nullptr; }
    auto contents = evaluate_before_or_after_process<shoop_types::LoopMidiChannel::Contents>(
        [=]() -> shoop_types::LoopMidiChannel::Contents {
          auto _channel = internal_midi_channel(channel);
          if (!_channel) { return {}; }
          return _channel->maybe_midi()->retrieve_contents();
        },
        _chan->maybe_midi(),
        *_backend);

    return external_midi_data(contents);
  }, nullptr);
}

void load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, shoop_audio_channel_data_t *data) {
  return api_impl<void>("load_audio_channel_data", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    evaluate_before_or_after_process<void>(
        [=, &data]() {
          auto _channel = internal_audio_channel(channel);
          if (!_channel) { return; }
          _channel->maybe_audio()->load_data(data->data, data->n_samples);
        },
        _chan->maybe_audio(),
        *_backend);
  });
}

void load_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel, shoop_midi_sequence_t  *data) {
  return api_impl<void>("load_midi_channel_data", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    auto contents = internal_midi_data(*data);
    evaluate_before_or_after_process<void>(
        [&]() {
          auto _channel = internal_midi_channel(channel);
          if (!_channel) { return; }
          _channel->maybe_midi()->set_contents(contents, data->length_samples);
        },
        _chan->maybe_midi(),
        *_backend);
  });
}

void clear_audio_channel_data_dirty (shoopdaloop_loop_audio_channel_t * channel) {
  return api_impl<void>("clear_audio_channel_data_dirty", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->clear_data_dirty();
  });
}

void clear_midi_channel_data_dirty (shoopdaloop_loop_midi_channel_t * channel) {
  return api_impl<void>("clear_midi_channel_data_dirty", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->clear_data_dirty();
  });
}

void set_audio_port_ringbuffer_n_samples (shoopdaloop_audio_port_t* port, unsigned n) {
  return api_impl<void>("set_audio_port_ringbuffer_n_samples", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    _port->get_port().set_ringbuffer_n_samples(n);
  });
}

void set_midi_port_ringbuffer_n_samples (shoopdaloop_midi_port_t* port, unsigned n) {
  return api_impl<void>("set_midi_port_ringbuffer_n_samples", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    _port->get_port().set_ringbuffer_n_samples(n);
  });
}

void loop_transition(shoopdaloop_loop_t *loop,
                      shoop_loop_mode_t mode,
                      int maybe_delay,
                      int maybe_to_sync_at_cycle)
{
  return api_impl<void>("loop_transition", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    _loop->get_backend().queue_process_thread_command([=]() {
        auto loop_info = internal_loop(loop);
        if (!loop_info) { return; }
        loop_info->loop->plan_transition(
          mode,
          maybe_delay >= 0 ? std::optional<uint32_t>(maybe_delay) : std::nullopt,
          maybe_to_sync_at_cycle >= 0 ? std::optional<uint32_t>(maybe_to_sync_at_cycle) : std::nullopt,
          false);
    });
  });
}

void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      shoop_loop_mode_t mode,
                      int maybe_delay,
                      int maybe_to_sync_at_cycle)
{
  return api_impl<void>("loops_transition", [&]() {
    auto internal_loops = shoop_make_shared<std::vector<shoop_shared_ptr<GraphLoop>>>(n_loops);
    for(uint32_t idx=0; idx<n_loops; idx++) {
        auto _loop = internal_loop(loops[idx]);
        if (_loop) {
          (*internal_loops)[idx] = _loop;
        }
    }
    auto _loop = internal_loop(loops[0]);
    if (!_loop) { return; }
    _loop->get_backend().queue_process_thread_command([=]() {
        for (uint32_t idx=0; idx<n_loops; idx++) {
            auto loop_info = (*internal_loops)[idx];
            if (loop_info) {
              loop_info->loop->plan_transition(
                mode,
                maybe_delay >= 0 ? std::optional<uint32_t>(maybe_delay) : std::nullopt,
                maybe_to_sync_at_cycle >= 0 ? std::optional<uint32_t>(maybe_to_sync_at_cycle) : std::nullopt,
                false);
            }
        }
        // Ensure that sync is fully propagated
        for (uint32_t idx=0; idx<n_loops; idx++) {
            auto loop_info = (*internal_loops)[idx];
            if (loop_info) {
              loop_info->loop->PROC_handle_sync();
            }
        }
    });
  });
}

void clear_loop (shoopdaloop_loop_t *loop, unsigned length) {
  return api_impl<void>("clear_loop", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    _loop->get_backend().queue_process_thread_command([=]() {
        auto __loop = internal_loop(loop);
        if (!__loop) { return; }
        __loop->loop->clear_planned_transitions(false);
        __loop->loop->plan_transition(LoopMode_Stopped, 0, false, false);
        for (auto &chan : __loop->mp_audio_channels) {
            chan->maybe_audio()->PROC_clear(length);
        }
        for (auto &chan : __loop->mp_midi_channels) {
            chan->maybe_midi()->clear(false);
        }
        __loop->loop->set_length(length, false);
    });
  });
}

void set_loop_length (shoopdaloop_loop_t *loop, unsigned length) {
  return api_impl<void>("set_loop_length", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    _loop->get_backend().queue_process_thread_command([=]() {
        auto __loop = internal_loop(loop);
        if (!__loop) { return; }
        __loop->loop->set_length(length, false);
    });
  });
}

void set_loop_position (shoopdaloop_loop_t *loop, unsigned position) {
  return api_impl<void>("set_loop_position", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    _loop->get_backend().queue_process_thread_command([=]() {
        auto __loop = internal_loop(loop);
        if (!__loop) { return; }
        __loop->loop->set_position(position, false);
    });
  });
}

void clear_audio_channel (shoopdaloop_loop_audio_channel_t *channel, unsigned length) {
  return api_impl<void>("clear_audio_channel", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (!_channel) { return; }
        _channel->maybe_audio()->PROC_clear(length);
    });
  });
}

void clear_midi_channel (shoopdaloop_loop_midi_channel_t *channel) {
  return api_impl<void>("clear_midi_channel", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (!_channel) { return; }
        _channel->maybe_midi()->clear(false);
    });
  });
}

void reset_midi_channel_state_tracking(shoopdaloop_loop_midi_channel_t *channel) {
  return api_impl<void>("reset_midi_channel_state_tracking", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    _chan->get_backend().queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (!_channel) { return; }
        _channel->maybe_midi()->PROC_reset_midi_state_tracking();
    });
  });
}

void do_segfault_on_process_thread(shoop_backend_session_t *backend) {
  return api_impl<void>("do_segfault_on_process_thread", [&]() {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
      volatile int* a = (int*)(NULL); *a = 1;
    });
  });
}

void do_abort_on_process_thread(shoop_backend_session_t *backend) {
  return api_impl<void>("do_abort_on_process_thread", [&]() {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
      abort();
    });
  });
}

shoopdaloop_audio_port_t *open_driver_audio_port (shoop_backend_session_t *backend, shoop_audio_driver_t *driver, const char* name_hint, shoop_port_direction_t direction, unsigned min_always_on_ringbuffer_samples) {
  return api_impl<shoopdaloop_audio_port_t*>("open_driver_audio_port", [&]() -> shoopdaloop_audio_port_t* {
    auto _backend = internal_backend_session(backend);
    auto _driver = internal_audio_driver(driver);
    if (!_backend || !_driver) { return nullptr; }
    auto port = _driver->open_audio_port
        (name_hint, direction,
         min_always_on_ringbuffer_samples > 0 ? _backend->audio_buffer_pool : nullptr);
    if (min_always_on_ringbuffer_samples > 0) {
      port->set_ringbuffer_n_samples(min_always_on_ringbuffer_samples);
    }
    auto pi = _backend->add_audio_port(port);
    _backend->set_graph_node_changes_pending();
    return external_audio_port(shoop_static_pointer_cast<GraphPort>(pi));
  }, (shoopdaloop_audio_port_t*) nullptr);
}

shoopdaloop_audio_port_t *open_internal_audio_port (shoop_backend_session_t *backend, const char* name_hint, unsigned min_always_on_ringbuffer_samples) {
  return api_impl<shoopdaloop_audio_port_t*>("open_internal_audio_port", [&]() -> shoopdaloop_audio_port_t* {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return nullptr; }
    auto port = shoop_make_shared<InternalAudioPort<audio_sample_t>>(
      std::string(name_hint),
      _backend->m_buffer_size,
      min_always_on_ringbuffer_samples > 0 ? _backend->audio_buffer_pool : nullptr
    );
    if (min_always_on_ringbuffer_samples > 0) {
      port->set_ringbuffer_n_samples(min_always_on_ringbuffer_samples);
    }
    auto pi = _backend->add_audio_port(shoop_static_pointer_cast<_AudioPort>(port));
    _backend->set_graph_node_changes_pending();
    return external_audio_port(shoop_static_pointer_cast<GraphPort>(pi));
  }, (shoopdaloop_audio_port_t*) nullptr);
}

void close_audio_port (shoop_backend_session_t *backend, shoopdaloop_audio_port_t *port) {
  return api_impl<void>("close_audio_port", [&]() {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return; }
    auto pi = internal_audio_port(port);
    if (!pi) { return; }
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    _backend->queue_process_thread_command([=]() {
        _backend->ports.erase(
            std::remove_if(_backend->ports.begin(), _backend->ports.end(),
                [pi](auto const& e) { return e == pi; }),
            _backend->ports.end()
        );
    });
  });
}

shoop_port_connections_state_t *get_audio_port_connections_state(shoopdaloop_audio_port_t *port) {
  return api_impl<shoop_port_connections_state_t*, log_level_debug_trace, log_level_warning>("get_audio_port_connections_state", [&]() {
    auto _port = internal_audio_port(port);
    auto rval = new shoop_port_connections_state_t;
    rval->n_ports = 0;
    if (!_port) {
      return rval;
    }

    auto connections = _port->get_port().get_external_connection_status();
    rval->n_ports = connections.size();
    rval->ports = new shoop_port_maybe_connection_t[rval->n_ports];
    uint32_t idx = 0;
    for (auto &pair : connections) {
        auto name = strdup(pair.first.c_str());
        auto connected = pair.second;
        rval->ports[idx].name = name;
        rval->ports[idx].connected = connected;
        logging::log<"Backend.API", log_level_debug_trace>(std::nullopt, std::nullopt, "--> {} connected: {}", name, connected);
        idx++;
    }
    return rval;
  }, new shoop_port_connections_state_t);
}

void connect_audio_port_external(shoopdaloop_audio_port_t *ours, const char* external_port_name) {
  return api_impl<void>("connect_audio_port_external", [&]() {
    auto _port = internal_audio_port(ours);
    if (!_port) { return; }
    _port->get_port().connect_external(std::string(external_port_name));
  });
}

void disconnect_audio_port_external(shoopdaloop_audio_port_t *ours, const char* external_port_name) {
  return api_impl<void>("disconnect_audio_port_external", [&]() {
    auto _port = internal_audio_port(ours);
    if (!_port) { return; }
    _port->get_port().disconnect_external(std::string(external_port_name));
  });
}

shoop_port_connections_state_t *get_midi_port_connections_state(shoopdaloop_midi_port_t *port) {
  return api_impl<shoop_port_connections_state_t*, log_level_debug_trace, log_level_warning>("get_midi_port_connections_state", [&]() -> shoop_port_connections_state_t* {
    auto _port = internal_midi_port(port);
    if (!port) { return nullptr; }
    auto connections = _port->get_port().get_external_connection_status();

    auto rval = new shoop_port_connections_state_t;
    rval->n_ports = connections.size();
    rval->ports = new shoop_port_maybe_connection_t[rval->n_ports];
    uint32_t idx = 0;
    for (auto &pair : connections) {
        rval->ports[idx].name = strdup(pair.first.c_str());
        rval->ports[idx].connected = pair.second;
        idx++;
    }
    return rval;
  }, new shoop_port_connections_state_t);
}

shoop_port_connections_state_t *get_decoupled_midi_port_connections_state(shoopdaloop_decoupled_midi_port_t *port) {
  return api_impl<shoop_port_connections_state_t*, log_level_debug_trace, log_level_warning>("get_decoupled_midi_port_connections_state", [&]() -> shoop_port_connections_state_t* {
    auto _port = internal_decoupled_midi_port(port);
    if (!port) { return nullptr; }
    auto connections = _port->get_port()->get_external_connection_status();

    auto rval = new shoop_port_connections_state_t;
    rval->n_ports = connections.size();
    rval->ports = new shoop_port_maybe_connection_t[rval->n_ports];
    uint32_t idx = 0;
    for (auto &pair : connections) {
        rval->ports[idx].name = strdup(pair.first.c_str());
        rval->ports[idx].connected = pair.second;
        idx++;
    }
    return rval;
  }, new shoop_port_connections_state_t);
}

void destroy_port_connections_state(shoop_port_connections_state_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_port_connections_state", [&]() {
    for (uint32_t idx=0; idx<d->n_ports; idx++) {
        free((void*)d->ports[idx].name);
    }
    delete[] d->ports;
    delete d;
  });
}

void connect_midi_port_external(shoopdaloop_midi_port_t *ours, const char* external_port_name) {
  return api_impl<void>("connect_midi_port_external", [&]() {
    auto _port = internal_midi_port(ours);
    if (!_port) { return; }
    _port->get_port().connect_external(std::string(external_port_name));
  });
}

void disconnect_midi_port_external(shoopdaloop_midi_port_t *ours, const char* external_port_name) {
  return api_impl<void>("disconnect_midi_port_external", [&]() {
    auto _port = internal_midi_port(ours);
    if (!_port) { return; }
    _port->get_port().disconnect_external(std::string(external_port_name));
  });
}

void connect_external_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *ours, const char* external_port_name) {
  return api_impl<void>("connect_external_decoupled_midi_port", [&]() {
    auto _port = internal_decoupled_midi_port(ours);
    if (!_port) { return; }
    _port->get_port()->connect_external(std::string(external_port_name));
  });
}

void disconnect_external_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *ours, const char* external_port_name) {
  return api_impl<void>("disconnect_external_decoupled_midi_port", [&]() {
    auto _port = internal_decoupled_midi_port(ours);
    if (!_port) { return; }
    _port->get_port()->disconnect_external(std::string(external_port_name));
  });
}

void *get_audio_port_driver_handle(shoopdaloop_audio_port_t *port) {
  return api_impl<void*>("get_audio_port_driver_handle", [&]() -> void* {
    auto _port = internal_audio_port(port);
    if (!_port) { return nullptr; }
    return _port->get_port().maybe_driver_handle();
  }, (void*)nullptr);
}

unsigned get_audio_port_input_connectability(shoopdaloop_audio_port_t *port) {
  return api_impl<unsigned>("get_audio_port_input_connectability", [&]() -> unsigned {
    auto _port = internal_audio_port(port);
    if (!_port) { return 0; }
    return _port->get_port().input_connectability();
  }, 0);
}

unsigned get_audio_port_output_connectability(shoopdaloop_audio_port_t *port) {
  return api_impl<unsigned>("get_audio_port_input_connectability", [&]() -> unsigned {
    auto _port = internal_audio_port(port);
    if (!_port) { return 0; }
    return _port->get_port().output_connectability();
  }, 0);
}

unsigned get_midi_port_input_connectability(shoopdaloop_midi_port_t *port) {
  return api_impl<unsigned>("get_midi_port_input_connectability", [&]() -> unsigned {
    auto _port = internal_midi_port(port);
    if (!_port) { return 0; }
    return _port->get_port().input_connectability();
  }, 0);
}

unsigned get_midi_port_output_connectability(shoopdaloop_midi_port_t *port) {
  return api_impl<unsigned>("get_midi_port_input_connectability", [&]() -> unsigned {
    auto _port = internal_midi_port(port);
    if (!_port) { return 0; }
    return _port->get_port().output_connectability();
  }, 0);
}

shoopdaloop_midi_port_t *open_driver_midi_port (shoop_backend_session_t *backend, shoop_audio_driver_t *driver, const char* name_hint, shoop_port_direction_t direction, unsigned min_always_on_ringbuffer_samples) {
  return api_impl<shoopdaloop_midi_port_t*>("open_driver_midi_port", [&]() -> shoopdaloop_midi_port_t* {
    auto _backend = internal_backend_session(backend);
    auto _driver = internal_audio_driver(driver);
    if (!_backend || !_driver) { return nullptr; }
    auto port = _driver->open_midi_port(name_hint, direction);
    if (min_always_on_ringbuffer_samples > 0) {
      port->set_ringbuffer_n_samples(min_always_on_ringbuffer_samples);
    }
    auto pi = _backend->add_midi_port(port);
    _backend->set_graph_node_changes_pending();
    return external_midi_port(shoop_static_pointer_cast<GraphPort>(pi));
  }, nullptr);
}

shoopdaloop_midi_port_t *open_internal_midi_port (shoop_backend_session_t *backend, const char* name_hint, unsigned min_always_on_ringbuffer_samples) {
  return api_impl<shoopdaloop_midi_port_t*>("open_internal_midi_port", [&]() -> shoopdaloop_midi_port_t* {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return nullptr; }
    auto port = nullptr;
    throw std::runtime_error("Creating internal MIDI ports not yet supported");
  }, (shoopdaloop_midi_port_t*) nullptr);
}

void close_midi_port (shoopdaloop_midi_port_t *port) {
  return api_impl<void>("close_midi_port", [&]() {
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    _port->get_backend().queue_process_thread_command([=]() {
        auto &backend = _port->get_backend();
        backend.set_graph_node_changes_pending();
        backend.ports.erase(
            std::remove_if(backend.ports.begin(), backend.ports.end(),
                [_port](auto const& e) { return e == _port; }),
            backend.ports.end()
        );
    });
  });
}

void *get_midi_port_driver_handle(shoopdaloop_midi_port_t *port) {
  return api_impl<void*>("get_midi_port_driver_handle", [&]() -> void* {
    auto _port = internal_midi_port(port);
    if (!_port) { return nullptr; }
    return _port->get_port().maybe_driver_handle();
  }, (void*)nullptr);
}

void connect_audio_port_internal(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to) {
  return api_impl<void>("connect_audio_port_internal", [&]() {
    auto _from = internal_audio_port(from);
    auto _to = internal_audio_port(to);
    if (_from && _to) {
      _from->connect_internal(_to);
    }
  });
}

void connect_midi_port_internal(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to) {
  return api_impl<void>("connect_midi_port_internal", [&]() {
    auto _from = internal_midi_port(from);
    auto _to = internal_midi_port(to);
    if (_from && _to) {
      _from->connect_internal(_to);
    }
  });
}

void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned int muted) {
  return api_impl<void>("set_audio_port_muted", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    _port->get_port().set_muted((bool)muted);
  });
}

void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned int muted) {
  return api_impl<void>("set_audio_port_passthroughMuted", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    _port->set_passthrough_enabled(!((bool)muted));
  });
}

void set_audio_port_gain(shoopdaloop_audio_port_t *port, float gain) {
  return api_impl<void>("set_audio_port_gain", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    _port->maybe_audio_port()->set_gain(gain);
  });
}

void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned int muted) {
  return api_impl<void>("set_midi_port_muted", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    _port->get_port().set_muted((bool)muted);
  });
}

void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned int muted) {
  return api_impl<void>("set_midi_port_passthroughMuted", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    _port->set_passthrough_enabled(!((bool)muted));
  });
}

shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoop_audio_driver_t *driver, const char* name_hint, shoop_port_direction_t direction) {
  return api_impl<shoopdaloop_decoupled_midi_port_t*>("open_decoupled_midi_port", [&]() -> shoopdaloop_decoupled_midi_port_t* {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return nullptr; }
    auto port = _driver->open_decoupled_midi_port(name_hint, direction);
    return external_decoupled_midi_port(port);
  }, nullptr);
}

shoop_midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port) {
  return api_impl<shoop_midi_event_t*, log_level_debug_trace, log_level_warning>("maybe_next_message", [&]() -> shoop_midi_event_t * {
    auto &_port = *internal_decoupled_midi_port(port);
    auto m = _port.pop_incoming();
    if (m.has_value()) {
        auto r = alloc_midi_event(m->data.size());
        r->time = 0;
        r->size = m->data.size();
        r->data = (uint8_t*)malloc(r->size);
        memcpy((void*)r->data, (void*)m->data.data(), r->size);
        return r;
    } else {
        return (shoop_midi_event_t*)nullptr;
    }
  }, (shoop_midi_event_t*)nullptr);
}

void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
  return api_impl<void>("close_decoupled_midi_port", [&]() {
    auto _port = internal_decoupled_midi_port(port);
    auto _driver = _port->get_maybe_driver();
    if (!_driver) {
       // Already closed
       return;
    }
    _driver->queue_process_thread_command([=]() {
        auto _port = internal_decoupled_midi_port(port);
        _port->get_maybe_driver()->unregister_decoupled_midi_port(_port);
        _port->close();
    });
  });
}

const char* get_decoupled_midi_port_name(shoopdaloop_decoupled_midi_port_t *port) {
  return api_impl<const char*>("get_decoupled_midi_port_name", [&]() {
    return internal_decoupled_midi_port(port)->name();
  }, "(invalid)");
}

void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data) {
  return api_impl<void>("send_decoupled_midi", [&]() {
    auto &_port = *internal_decoupled_midi_port(port);
    DecoupledMidiMessage m;
    m.data.resize(length);
    memcpy((void*)m.data.data(), (void*)data, length);
    _port.push_outgoing(m);
  });
}

shoop_midi_event_t *alloc_midi_event(unsigned data_bytes) {
  return api_impl<shoop_midi_event_t*>("alloc_midi_event", [&]() {
    auto r = new shoop_midi_event_t;
    r->size = data_bytes;
    r->time = 0;
    r->data = (unsigned char*) malloc(data_bytes);
    return r;
  }, nullptr);
}

shoop_midi_sequence_t *alloc_midi_sequence(unsigned n_events) {
  return api_impl<shoop_midi_sequence_t*>("alloc_midi_sequence", [&]() {
    auto r = new shoop_midi_sequence_t;
    r->n_events = n_events;
    r->events = (shoop_midi_event_t**)malloc (sizeof(shoop_midi_event_t*) * n_events);
    r->length_samples = 0;
    return r;
  }, nullptr);
}

shoop_audio_channel_data_t *alloc_audio_channel_data(unsigned n_samples) {
  return api_impl<shoop_audio_channel_data_t*>("alloc_audio_channel_data", [&]() {
    auto r = new shoop_audio_channel_data_t;
    r->n_samples = n_samples;
    r->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * n_samples);
    return r;
  }, nullptr);
}

void set_audio_channel_gain (shoopdaloop_loop_audio_channel_t *channel, float gain) {
  return api_impl<void>("set_audio_channel_gain", [&]() {
    auto internal = internal_audio_channel(channel);
    if (!internal) { return; }
    // Perform atomically if possible, or queue if channel not yet initialized.
    if (internal->channel) {
        dynamic_cast<LoopAudioChannel*>(internal->channel.get())->set_gain(gain);
    } else {
        auto _backend = internal->backend.lock();
        if (!_backend) { return; }
        _backend->queue_process_thread_command([=]() {
            dynamic_cast<LoopAudioChannel*>(internal->channel.get())->set_gain(gain);
        });
    }
  });
}

shoop_audio_channel_state_info_t *get_audio_channel_state (shoopdaloop_loop_audio_channel_t *channel) {
  return api_impl<shoop_audio_channel_state_info_t*, log_level_debug_trace, log_level_warning>("get_audio_channel_state", [&]() -> shoop_audio_channel_state_info_t* {
    auto r = new shoop_audio_channel_state_info_t;
    auto chan = internal_audio_channel(channel);
    if (!chan) { return nullptr; }
    auto _backend = chan->backend.lock();
    if (!_backend) { return nullptr; }
    auto audio = evaluate_before_or_after_process<LoopAudioChannel*>(
        [&]() { return chan->maybe_audio(); },
        chan->maybe_audio(),
        *_backend);
    r->output_peak = audio->get_output_peak();
    r->gain = audio->get_gain();
    r->mode = audio->get_mode();
    r->length = audio->get_length();
    r->start_offset = audio->get_start_offset();
    r->data_dirty = chan->get_data_dirty();
    r->n_preplay_samples = chan->channel->get_pre_play_samples();
    auto p = chan->channel->get_played_back_sample();
    if (p.has_value()) { r->played_back_sample = p.value(); } else { r->played_back_sample = -1; }
    audio->reset_output_peak();
    return r;
  }, new shoop_audio_channel_state_info_t);
}

shoop_midi_channel_state_info_t *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel) {
  return api_impl<shoop_midi_channel_state_info_t *, log_level_debug_trace, log_level_warning>("get_midi_channel_state", [&]() -> shoop_midi_channel_state_info_t* {
    auto r = new shoop_midi_channel_state_info_t;
    auto chan = internal_midi_channel(channel);
    if (!chan) { return nullptr; }
    auto _backend = chan->backend.lock();
    if (!_backend) { return nullptr; }
    auto midi = evaluate_before_or_after_process<LoopMidiChannel*>(
        [&]() { return chan->maybe_midi(); },
        chan->maybe_midi(),
        *_backend);
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
  }, new shoop_midi_channel_state_info_t);
}

void set_audio_channel_mode (shoopdaloop_loop_audio_channel_t * channel, shoop_channel_mode_t mode) {
  return api_impl<void>("set_audio_channel_mode", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_mode(mode);
    });
  });
}

void set_midi_channel_mode (shoopdaloop_loop_midi_channel_t * channel, shoop_channel_mode_t mode) {
  return api_impl<void>("set_midi_channel_mode", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_mode(mode);
    });
  });
}

void set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t * channel, int start_offset) {
  return api_impl<void>("set_audio_channel_start_offset", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_start_offset(start_offset);
    });
  });
}

void set_midi_channel_start_offset (shoopdaloop_loop_midi_channel_t * channel, int start_offset) {
  return api_impl<void>("set_midi_channel_start_offset", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_start_offset(start_offset);
    });
  });
}

void set_audio_channel_n_preplay_samples (shoopdaloop_loop_audio_channel_t *channel, unsigned n) {
  return api_impl<void>("set_audio_channel_n_preplay_samples", [&]() {
    auto _chan = internal_audio_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_audio_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_pre_play_samples(n);
    });
  });
}

void set_midi_channel_n_preplay_samples  (shoopdaloop_loop_midi_channel_t *channel, unsigned n) {
  return api_impl<void>("set_midi_channel_n_preplay_samples", [&]() {
    auto _chan = internal_midi_channel(channel);
    if (!_chan) { return; }
    auto _backend = _chan->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _channel = internal_midi_channel(channel);
        if (!_channel) { return; }
        _channel->channel->set_pre_play_samples(n);
    });
  });
}

shoop_audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port) {
  return api_impl<shoop_audio_port_state_info_t*, log_level_debug_trace, log_level_warning>("get_audio_port_state", [&]() -> shoop_audio_port_state_info_t* {
    auto r = new shoop_audio_port_state_info_t;
    auto pp = internal_audio_port(port);
    if (!pp) { return nullptr; }
    auto p = pp->maybe_audio_port();

    if (p) {
      r->input_peak = p->get_input_peak();
      r->output_peak = p->get_output_peak();
      r->gain = p->get_gain();
      r->muted = p->get_muted();
      r->passthrough_muted = !pp->get_passthrough_enabled();
      r->name = strdup(p->name());
      r->ringbuffer_n_samples = pp->get_port().get_ringbuffer_n_samples();
      p->reset_input_peak();
      p->reset_output_peak();
    }
    return r;
  }, new shoop_audio_port_state_info_t);
}

shoop_midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port) {
  return api_impl<shoop_midi_port_state_info_t*, log_level_debug_trace, log_level_warning>("get_midi_port_state", [&]() -> shoop_midi_port_state_info_t* {
    auto r = new shoop_midi_port_state_info_t;
    auto pp = internal_midi_port(port);
    if (!pp) {
      return nullptr;
    }
    auto p = pp->maybe_midi_port();

    if(p) {
      r->n_input_events = p->get_n_input_events();
      r->n_output_events = p->get_n_output_events();
      r->n_input_notes_active = p->get_n_input_notes_active();
      r->n_output_notes_active = p->get_n_output_notes_active();
      r->muted = p->get_muted();
      r->passthrough_muted = !pp->get_passthrough_enabled();
      r->name = strdup(p->name());
      r->ringbuffer_n_samples = pp->get_port().get_ringbuffer_n_samples();
      p->reset_n_input_events();
      p->reset_n_output_events();
    }
    return r;
  }, new shoop_midi_port_state_info_t);
}

shoop_loop_state_info_t *get_loop_state(shoopdaloop_loop_t *loop) {
  return api_impl<shoop_loop_state_info_t*, log_level_debug_trace, log_level_warning>("get_loop_state", [&]() -> shoop_loop_state_info_t* {
    auto r = new shoop_loop_state_info_t;
    auto _loop = internal_loop(loop);
    if (!_loop) { return nullptr; }
    r->mode = _loop->loop->get_mode();
    r->position = _loop->loop->get_position();
    r->length = _loop->loop->get_length();
    shoop_loop_mode_t next_mode;
    uint32_t next_delay;
    _loop->loop->get_first_planned_transition(next_mode, next_delay);
    r->maybe_next_mode = next_mode;
    r->maybe_next_mode_delay = next_delay;
    return r;
  }, new shoop_loop_state_info_t);
}

void set_loop_sync_source (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source) {
  return api_impl<void>("set_loop_sync_source", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    auto _backend = _loop->backend.lock();
    if (!_backend) { return; }
    _backend->queue_process_thread_command([=]() {
        auto _loop = internal_loop(loop);
        if (!_loop) { return; }
        if (sync_source) {
            auto _src = internal_loop(sync_source);
            if (!_src) { return ;}
            _loop->loop->set_sync_source(shoop_static_pointer_cast<LoopInterface>(_src->loop), false);
        } else {
            _loop->loop->set_sync_source(nullptr, false);
        }
    });
  });
}

void adopt_ringbuffer_contents(shoopdaloop_loop_t *loop, int reverse_cycles_start, int cycles_length, int go_to_cycle, shoop_loop_mode_t go_to_mode) {
  return api_impl<void>("adopt_ringbuffer_contents", [&]() {
    auto _loop = internal_loop(loop);
    if (!_loop) { return; }
    auto backend = _loop->backend.lock();
    if (!backend) { return; }

    std::optional<unsigned> _reverse_cycles_start = reverse_cycles_start >= 0 ? std::optional<unsigned>(reverse_cycles_start) : std::nullopt;
    std::optional<unsigned> _cycles_length = cycles_length >= 0 ? std::optional<unsigned>(cycles_length) : std::nullopt;
    std::optional<unsigned> _go_to_cycle = go_to_cycle >= 0 ? std::optional<unsigned>(go_to_cycle) : std::nullopt;

    backend->queue_process_thread_command([=]() {
      _loop->PROC_adopt_ringbuffer_contents(_reverse_cycles_start, _cycles_length, _go_to_cycle, go_to_mode);
    });
  });
}

shoopdaloop_fx_chain_t *create_fx_chain(shoop_backend_session_t *backend, shoop_fx_chain_type_t type, const char* title) {
  return api_impl<shoopdaloop_fx_chain_t*>("create_fx_chain", [&]() -> shoopdaloop_fx_chain_t* {
    auto _backend = internal_backend_session(backend);
    if (!_backend) { return nullptr; }
    return external_fx_chain(internal_backend_session(backend)->create_fx_chain(type, title));
  }, nullptr);
}

void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible) {
  return api_impl<void>("fx_chain_set_ui_visible", [&]() {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return; }
    auto maybe_ui = dynamic_cast<ExternalUIInterface*>(_chain->chain.get());
    if(maybe_ui) {
        if (visible) {
            maybe_ui->show();
        } else {
            maybe_ui->hide();
        }
    } else {
        logging::log<"Backend.API", log_level_debug>(std::nullopt, std::nullopt, "fx_chain_set_ui_visible: chain does not support UI, ignoring");
    }
  });
}

shoop_fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain) {
  return api_impl<shoop_fx_chain_state_info_t*, log_level_debug_trace, log_level_warning>("get_fx_chain_state", [&]() -> shoop_fx_chain_state_info_t* {
    auto r = new shoop_fx_chain_state_info_t;
    auto c = internal_fx_chain(chain);
    if (!c) { return nullptr; }
    r->ready = (unsigned) c->chain->is_ready();
    r->active = (unsigned) c->chain->is_active();
    auto maybe_ui = dynamic_cast<ExternalUIInterface*>(c->chain.get());
    if(maybe_ui) {
        r->visible = (unsigned) maybe_ui->visible();
    } else {
        r->visible = 0;
    }
    return r;
  }, new shoop_fx_chain_state_info_t);
}

void set_fx_chain_active(shoopdaloop_fx_chain_t *chain, unsigned active) {
  return api_impl<void>("set_fx_chain_active", [&]() {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return; }
    _chain->chain->set_active(active);
  });
}

const char* get_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain) {
  return api_impl<const char*, log_level_debug_trace, log_level_warning>("get_fx_chain_internal_state", [&]() -> const char* {
    auto c = internal_fx_chain(chain);
    if (!c) { return nullptr; }
    auto maybe_serializeable = dynamic_cast<SerializeableStateInterface*>(c->chain.get());
    if(maybe_serializeable) {
        auto str = maybe_serializeable->serialize_state(10000);
        char * rval = (char*) malloc(str.size() + 1);
        memcpy((void*)rval, str.data(), str.size());
        rval[str.size()] = 0;
        return (const char*)rval;
    } else {
        char * rval = (char*) malloc(1);
        rval[0] = 0;
        return (const char*)rval;
    }
  }, (const char*)"");
}

void restore_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain, const char* state) {
  return api_impl<void>("restore_fx_chain_internal_state", [&]() {
    auto c = internal_fx_chain(chain);
    if (!c) { return; }
    auto maybe_serializeable = dynamic_cast<SerializeableStateInterface*>(c->chain.get());
    if (maybe_serializeable) {
        maybe_serializeable->deserialize_state(std::string(state));
    }
  });
}

unsigned n_fx_chain_audio_input_ports(shoopdaloop_fx_chain_t *chain) {
  return api_impl<unsigned>("n_fx_chain_audio_input_ports", [&]() -> unsigned {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return 0; }
    auto const& ports = _chain->audio_input_ports();
    return ports.size();
  }, 0);
}

shoopdaloop_audio_port_t *fx_chain_audio_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
  return api_impl<shoopdaloop_audio_port_t*>("fx_chain_audio_input_port", [&]() -> shoopdaloop_audio_port_t* {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return nullptr; }
    auto port = _chain->audio_input_ports()[idx];
    return external_audio_port(port);
  }, nullptr);
}

unsigned n_fx_chain_audio_output_ports(shoopdaloop_fx_chain_t *chain) {
  return api_impl<unsigned>("n_fx_chain_audio_output_ports", [&]() -> unsigned {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return 0; }
    auto const& ports = _chain->audio_output_ports();
    return ports.size();
  }, 0);
}

unsigned n_fx_chain_midi_input_ports(shoopdaloop_fx_chain_t *chain) {
  return api_impl<unsigned>("n_fx_chain_midi_input_ports", [&]() -> unsigned {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return 0; }
    auto const& ports = _chain->midi_input_ports();
    return ports.size();
  }, 0);
}
;
shoopdaloop_audio_port_t *fx_chain_audio_output_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
  return api_impl<shoopdaloop_audio_port_t*>("fx_chain_audio_output_port", [&]() -> shoopdaloop_audio_port_t* {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return nullptr; }
    auto port = _chain->audio_output_ports()[idx];
    return external_audio_port(port);
  }, nullptr);
}

shoopdaloop_midi_port_t *fx_chain_midi_input_port(shoopdaloop_fx_chain_t *chain, unsigned int idx) {
  return api_impl<shoopdaloop_midi_port_t*>("fx_chain_midi_input_port", [&]() -> shoopdaloop_midi_port_t* {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return nullptr; }
    auto port = _chain->midi_input_ports()[idx];
    return external_midi_port(port);
  }, nullptr);
}

void destroy_midi_event(shoop_midi_event_t *e) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_midi_event", [&]() {
    free(e->data);
    delete e;
  });
}

void destroy_midi_sequence(shoop_midi_sequence_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_midi_sequence", [&]() {
    for(uint32_t idx=0; idx<d->n_events; idx++) {
        destroy_midi_event(d->events[idx]);
    }
    free(d->events);
    delete d;
  });
}

void destroy_audio_channel_data(shoop_audio_channel_data_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_audio_channel_data", [&]() {
    free(d->data);
    delete d;
  });
}

void destroy_audio_channel_state_info(shoop_audio_channel_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_audio_channel_state_info", [&]() {
    delete d;
  });
}

void destroy_midi_channel_state_info(shoop_midi_channel_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_midi_channel_state_info", [&]() {
    delete d;
  });
}

void destroy_loop(shoopdaloop_loop_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_loop", [&]() {
    auto loop = internal_loop(d);
    if (!loop) { return; }
    auto backend = loop->backend.lock();
    if (!backend) { return; }
    backend->exec_process_thread_command([loop, backend]() {
        loop->delete_all_channels(false);

        bool found = false;
        for(auto &elem : backend->loops) {
            if(elem == loop) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find loop to destroy");
        }
    });
  });
}

void destroy_audio_port(shoopdaloop_audio_port_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_audio_port", [&]() {
    auto port = internal_audio_port(d);
    if (!port) { return; }
    auto backend = port->backend.lock();
    if (!backend) { return; }
    backend->exec_process_thread_command([port, backend]() {
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
    port->get_port().close();
  });
}

void destroy_midi_port(shoopdaloop_midi_port_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_midi_port", [&]() {
    auto port = internal_midi_port(d);
    if (!port) { return; }
    auto backend = port->backend.lock();
    if (!backend) { return; }
    backend->exec_process_thread_command([port, backend]() {
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
    port->get_port().close();
  });
}

void destroy_midi_port_state_info(shoop_midi_port_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_midi_port_state_info", [&]() {
    delete d;
  });
}

void destroy_audio_port_state_info(shoop_audio_port_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_audio_port_state_info", [&]() {
    delete d;
  });
}

void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_audio_channel", [&]() {
    auto chan = internal_audio_channel(d);
    if (!chan) { return; }
    if(auto l = chan->loop.lock()) {
        l->delete_audio_channel(chan, true);
    }
  });
}

void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_midi_channel", [&]() {
    auto chan = internal_midi_channel(d);
    if (!chan) { return; }
    if(auto l = chan->loop.lock()) {
        l->delete_midi_channel(chan, true);
    }
  });
}

void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_shoopdaloop_decoupled_midi_port", [&]() {
    logging::log<"Backend.API", log_level_debug>(std::nullopt, std::nullopt, "destroy_shoopdaloop_decoupled_midi_port");
    throw std::runtime_error("unimplemented");
  });
}

void destroy_loop_state_info(shoop_loop_state_info_t *state) {
  return api_impl<void, log_level_debug_trace>("destroy_loop_state_info", [&]() {
    delete state;
  });
}

void destroy_fx_chain(shoopdaloop_fx_chain_t *chain) {
  return api_impl<void, log_level_debug_trace, log_level_warning>("destroy_fx_chain", [&]() {
    auto _chain = internal_fx_chain(chain);
    if (!_chain) { return; }
    logging::log<"Backend.API", log_level_warning>(std::nullopt, std::nullopt, "Warning: destroying FX chains is unimplemented. Stopping only.");
    _chain->chain->stop();
  });
}

void destroy_fx_chain_state(shoop_fx_chain_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_fx_chain_state", [&]() {
    delete d;
  });
}

void destroy_string(const char* s) {
  return api_impl<void, log_level_debug_trace>("destroy_string", [&]() {
    free((void*)s);
  });
}

void destroy_backend_state_info(shoop_backend_session_state_info_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_backend_state_info", [&]() {
    delete d;
  });
}

void destroy_profiling_report(shoop_profiling_report_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_profiling_report", [&]() {
    for(uint32_t idx=0; idx < d->n_items; idx++) {
        free ((void*)d->items[idx].key);
    }
    free(d->items);
    free(d);
  });
}

void destroy_external_port_descriptors(shoop_external_port_descriptors_t *d) {
  return api_impl<void, log_level_debug_trace>("destroy_profiling_report", [&]() {
    for(int i=0; i<d->n_ports; i++) {
      free((void*)d->ports[i].name);
    }
    free(d->ports);
    free(d);
  });
}

shoopdaloop_logger_t *get_logger(const char* name) {
  return api_impl<shoopdaloop_logger_t*, log_level_debug_trace>("get_logger", [&]() {
    return (shoopdaloop_logger_t*) strdup(name);
  }, nullptr);
}

void set_global_logging_level(shoop_log_level_t level) {
  return api_impl<void>("set_global_logging_level", [&]() {
    logging::set_filter_level(level);
  });
}

void reset_logger_level_override(shoopdaloop_logger_t *logger) {
  return api_impl<void>("reset_logger_level_override", [&]() {
    auto name = (const char*)logger;
    set_module_filter_level(name, std::nullopt);
  });
}

void set_logger_level_override(shoopdaloop_logger_t *logger, shoop_log_level_t level) {
  return api_impl<void>("set_logger_level_override", [&]() {
    auto name = (const char*)logger;
    set_module_filter_level(name, level);
  });
}

void shoopdaloop_log(shoopdaloop_logger_t *logger, shoop_log_level_t level, const char *msg) {
  return api_impl<void, log_level_debug_trace>("shoopdaloop_log", [&]() {
    std::string name((const char*)logger);
    std::string _msg(msg);
    logging::log(level, name, _msg);
  });
}

unsigned shoopdaloop_should_log(shoopdaloop_logger_t *logger, shoop_log_level_t level) {
  return api_impl<unsigned, log_level_debug_trace>("shoopdaloop_should_log", [&]() -> unsigned {
    auto name = (const char*)logger;
    return logging::should_log(
        name, level
    ) ? 1 : 0;
    return 1;
  }, (unsigned)0);
}

void dummy_audio_port_queue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t const* data) {
  return api_impl<void>("dummy_audio_port_queue_data", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyAudioPort*>(&_port->get_port());
    if (maybe_dummy) {
        maybe_dummy->queue_data(n_frames, data);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_port_queue_data called on non-dummy-audio port");
    }
  });
}

void dummy_audio_port_dequeue_data(shoopdaloop_audio_port_t *port, unsigned n_frames, audio_sample_t *store_in) {
  return api_impl<void>("dummy_audio_port_dequeue_data", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyAudioPort*>(&_port->get_port());
    if (maybe_dummy) {
        auto data = maybe_dummy->dequeue_data(n_frames);
        memcpy((void*)store_in, (void*)data.data(), sizeof(audio_sample_t) * n_frames);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_port_queue_data called on non-dummy-audio port");
    }
  });
}
void dummy_midi_port_queue_data(shoopdaloop_midi_port_t *port, shoop_midi_sequence_t* events) {
  return api_impl<void>("dummy_midi_port_queue_data", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyMidiPort*>(&_port->get_port());
    if (maybe_dummy) {
        for(uint32_t i=0; i<events->n_events; i++) {
            auto &e = events->events[i];
            maybe_dummy->queue_msg(
                e->size, e->time, e->data
            );
        }
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_midi_port_queue_data called on non-dummy0midi port");
    }
  });
}

shoop_midi_sequence_t *dummy_midi_port_dequeue_data(shoopdaloop_midi_port_t *port) {
  return api_impl<shoop_midi_sequence_t*>("dummy_midi_port_dequeue_data", [&]() -> shoop_midi_sequence_t*{
    auto _port = internal_midi_port(port);
    if (!_port) { return nullptr; }
    auto maybe_dummy = dynamic_cast<DummyMidiPort*>(&_port->get_port());
    if (maybe_dummy) {
        auto msgs = maybe_dummy->get_written_requested_msgs();
        shoop_midi_sequence_t *rval = alloc_midi_sequence(msgs.size());
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
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_midi_port_dequeue_data called on non-dummy-midi port");
        return nullptr;
    }
  }, (shoop_midi_sequence_t *)nullptr);
}
void dummy_midi_port_request_data(shoopdaloop_midi_port_t* port, unsigned n_frames) {
  return api_impl<void>("dummy_midi_port_request_data", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyMidiPort*>(&_port->get_port());
    if (maybe_dummy) {
        maybe_dummy->request_data(n_frames);
    }
  });
}

void dummy_midi_port_clear_queues(shoopdaloop_midi_port_t* port) {
  return api_impl<void>("dummy_midi_port_clear_queues", [&]() {
    auto _port = internal_midi_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyMidiPort*>(&_port->get_port());
    if (maybe_dummy) {
        maybe_dummy->clear_queues();
    }
  });
}

void dummy_audio_port_request_data(shoopdaloop_audio_port_t* port, unsigned n_frames) {
  return api_impl<void>("dummy_audio_port_request_data", [&]() {
    auto _port = internal_audio_port(port);
    if (!_port) { return; }
    auto maybe_dummy = dynamic_cast<DummyAudioPort*>(&_port->get_port());
    if (maybe_dummy) {
        maybe_dummy->request_data(n_frames);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_port_request_data called on non-dummy port");
    }
  });
}

void dummy_audio_enter_controlled_mode(shoop_audio_driver_t *driver) {
  return api_impl<void>("dummy_audio_enter_controlled_mode", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->enter_mode(DummyAudioMidiDriverMode::Controlled);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_enter_controlled_mode called on non-dummy backend");
    }
  });
}

void dummy_audio_enter_automatic_mode(shoop_audio_driver_t *driver) {
  return api_impl<void>("dummy_audio_enter_automatic_mode", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->enter_mode(DummyAudioMidiDriverMode::Automatic);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_enter_automatic_mode called on non-dummy backend");
    }
  });
}

unsigned dummy_audio_is_in_controlled_mode(shoop_audio_driver_t *driver) {
  return api_impl<unsigned>("dummy_audio_is_in_controlled_mode", [&]() -> unsigned {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return 0; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        return (unsigned) (maybe_dummy->get_mode() == DummyAudioMidiDriverMode::Controlled);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_is_in_controlled_mode called on non-dummy backend");
        return 0;
    }
  }, (unsigned)0);
}

void dummy_audio_request_controlled_frames(shoop_audio_driver_t *driver, unsigned n_frames) {
  return api_impl<void>("dummy_audio_request_controlled_frames", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->controlled_mode_request_samples(n_frames);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_request_controlled_frames called on non-dummy backend");
    }
  });
}

void dummy_audio_run_requested_frames(shoop_audio_driver_t *driver) {
  return api_impl<void>("dummy_audio_run_requested_frames", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->controlled_mode_run_request();
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_request_controlled_frames called on non-dummy backend");
    }
  });
}

unsigned dummy_audio_n_requested_frames(shoop_audio_driver_t *driver) {
  return api_impl<unsigned>("dummy_audio_n_requested_frames", [&]() -> unsigned  {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return 0; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        return maybe_dummy->get_controlled_mode_samples_to_process();
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_audio_n_requested_frames called on non-dummy backend");
        return 0;
    }
  }, (unsigned)0);
}

void dummy_driver_add_external_mock_port(shoop_audio_driver_t* driver, const char* name, shoop_port_direction_t direction, shoop_port_data_type_t data_type) {
  return api_impl<void>("dummy_driver_add_external_mock_port", [&]()  {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->add_external_mock_port(std::string(name), direction, data_type);
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_driver_add_external_mock_port called on non-dummy backend");
    }
  });
}

void dummy_driver_remove_external_mock_port(shoop_audio_driver_t* driver, const char* name) {
  return api_impl<void>("dummy_driver_remove_external_mock_port", [&]()  {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->remove_external_mock_port(std::string(name));
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_driver_remove_external_mock_port called on non-dummy backend");
    }
  });
}

void dummy_driver_remove_all_external_mock_ports(shoop_audio_driver_t* driver) {
  return api_impl<void>("dummy_driver_remove_all_external_mock_ports", [&]()  {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    if (auto maybe_dummy = shoop_dynamic_pointer_cast<_DummyAudioMidiDriver>(_driver)) {
        maybe_dummy->remove_all_external_mock_ports();
    } else {
        logging::log<"Backend.API", log_level_error>(std::nullopt, std::nullopt, "dummy_driver_remove_all_external_mock_ports called on non-dummy backend");
    }
  });
}

void wait_process(shoop_audio_driver_t *driver) {
  return api_impl<void>("wait_process", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    _driver->wait_process();
  });
}

void destroy_logger(shoopdaloop_logger_t* logger) {
  return api_impl<void, log_level_debug_trace>("destroy_logger", [&]() {
#ifndef _WIN32
    // free ((void*)logger);
#else
    //TODO Leaking loggers on Windows because of unsolved segmentation fault.
#endif
  });
}

unsigned get_sample_rate (shoop_audio_driver_t *driver) {
  return api_impl<unsigned>("get_sample_rate", [&]() -> unsigned {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return 48000; }
    return _driver->get_sample_rate();
  }, 0);
}

unsigned get_buffer_size (shoop_audio_driver_t *driver) {
  return api_impl<unsigned>("get_buffer_size", [&]() -> unsigned {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return 1024; }
    return _driver->get_buffer_size();
  }, 0);
}

void destroy_audio_driver_state(shoop_audio_driver_state_t *state) {
  return api_impl<void, log_level_debug_trace>("destroy_audio_driver_state", [&]() {
    if (state->maybe_instance_name) {
      free((void*)state->maybe_instance_name);
    }
    delete state;
  });
}

void destroy_audio_driver(shoop_audio_driver_t *driver) {
  return api_impl<void, log_level_debug_trace>("destroy_audio_driver", [&]() {
      auto _driver = internal_audio_driver(driver);
      if (!_driver) { return; }
      g_active_drivers.erase(_driver);
      _driver->close();
  });
}

unsigned get_driver_active(shoop_audio_driver_t *driver) {
  return api_impl<unsigned>("get_driver_active", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return false; }
    return _driver->get_active();
  }, 0);
}

void start_dummy_driver(shoop_audio_driver_t *driver, shoop_dummy_audio_driver_settings_t settings) {
  return api_impl<void>("start_dummy_driver", [&]() {
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    auto dummy = shoop_dynamic_pointer_cast<shoop_types::_DummyAudioMidiDriver>(_driver);
    if (!dummy) {
      throw std::runtime_error("Given driver is invalid or not of the correct type (Dummy).");
    }
    if (dummy->get_active()) {
      throw std::runtime_error("Driver to be started is already running.");
    }
    DummyAudioMidiDriverSettings s;
    s.buffer_size = settings.buffer_size;
    s.sample_rate = settings.sample_rate;
    s.client_name = settings.client_name;
    dummy->start(s);
  });
}

void start_jack_driver(shoop_audio_driver_t *driver, shoop_jack_audio_driver_settings_t settings) {
  return api_impl<void>("start_jack_driver", [&]() {
#ifdef SHOOP_HAVE_BACKEND_JACK
    auto _driver = internal_audio_driver(driver);
    if (!_driver) { return; }
    auto jack = shoop_dynamic_pointer_cast<JackAudioMidiDriver>(_driver);
    auto jacktest = shoop_dynamic_pointer_cast<JackTestAudioMidiDriver>(_driver);
    if (!jack && !jacktest) {
      throw std::runtime_error("Given driver is invalid or not of the correct type (Jack / JackTest).");
    }

    auto execute = [settings](auto &jack) {
      if (jack->get_active()) {
        throw std::runtime_error("Driver to be started is already running.");
      }
      JackAudioMidiDriverSettings s;
      s.client_name_hint = settings.client_name_hint;
      if(settings.maybe_server_name) {
          s.maybe_server_name_hint = settings.maybe_server_name;
      }
      jack->start(s);
    };

    if (jack) { execute(jack); }
    else if (jacktest) { execute(jacktest); }
#else
    throw std::runtime_error("Jack backend not available.");
#endif
  });
}

shoop_multichannel_audio_t *resample_audio(shoop_multichannel_audio_t *in, unsigned new_n_frames) {
  return api_impl<shoop_multichannel_audio_t*>("resample_audio", [&]() {
    float* result = resample_multi(in->data, in->n_channels, in->n_frames, new_n_frames);
    auto rval = new shoop_multichannel_audio_t;
    rval->data = result;
    rval->n_channels = in->n_channels;
    rval->n_frames = new_n_frames;
    return rval;
  }, new shoop_multichannel_audio_t);
}

shoop_multichannel_audio_t *alloc_multichannel_audio(unsigned n_channels, unsigned n_frames) {
  auto rval = new shoop_multichannel_audio_t;
  rval->n_channels = n_channels;
  rval->n_frames = n_frames;
  rval->data = (shoop_types::audio_sample_t*) malloc(sizeof(shoop_types::audio_sample_t) * n_channels * n_frames);
  return rval;
}

void destroy_multichannel_audio(shoop_multichannel_audio_t *audio) {
  free(audio->data);
  delete audio;
}
