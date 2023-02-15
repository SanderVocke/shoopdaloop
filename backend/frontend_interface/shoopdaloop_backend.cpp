#include "shoopdaloop_backend.h"

// Internal
#include "AudioBuffer.h"
#include "AudioChannelSubloop.h"
#include "AudioMidiLoop.h"
#include "AudioPortInterface.h"
#include "JackAudioPort.h"
#include "JackAudioSystem.h"
#include "MidiChannelSubloop.h"
#include "MidiMessage.h"
#include "MidiPortInterface.h"
#include "ObjectPool.h"
#include "PortInterface.h"
#include "SubloopInterface.h"
#include "DecoupledMidiPort.h"
#include "CommandQueue.h"
#include "process_loops.h"
#include "types.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <math.h>
#include <memory>
#include <stdexcept>
#include <map>

// CONSTANTS
namespace {
constexpr size_t gc_initial_max_loops = 512;
constexpr size_t gc_initial_max_ports = 1024;
constexpr size_t gc_initial_max_decoupled_midi_ports = 512;
constexpr size_t gc_n_buffers_in_pool = 128;
constexpr size_t gc_audio_buffer_size = 32768;
constexpr size_t gc_command_queue_size = 2048;
constexpr size_t gc_audio_channel_initial_buffers = 128;
constexpr size_t gc_midi_storage_size = 16384;
constexpr size_t gc_default_max_port_mappings = 8;
constexpr size_t gc_default_max_midi_channels = 8;
constexpr size_t gc_default_max_audio_channels = 8;
constexpr size_t gc_decoupled_midi_port_queue_size = 256;
}

// FORWARD DECLARATIONS
struct LoopInfo;
struct PortInfo;
struct ChannelInfo;

// TYPE ALIASES
using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = ObjectPool<DefaultAudioBuffer>;
using AudioSystem = JackAudioSystem;
using Time = uint32_t;
using Size = uint16_t;
using Loop = AudioMidiLoop;
using SharedLoop = std::shared_ptr<Loop>;
using SharedLoopChannel = std::shared_ptr<SubloopInterface>;
using LoopAudioChannel = AudioChannelSubloop<audio_sample_t>;
using LoopMidiChannel = MidiChannelSubloop<uint32_t, uint16_t>;
using SharedLoopAudioChannel = std::shared_ptr<LoopAudioChannel>;
using SharedLoopMidiChannel = std::shared_ptr<LoopMidiChannel>;
using SharedPort = std::shared_ptr<PortInterface>;
using AudioPort = AudioPortInterface<audio_sample_t>;
using MidiPort = MidiPortInterface;
using SharedPortInfo = std::shared_ptr<PortInfo>;
using SharedLoopInfo = std::shared_ptr<LoopInfo>;
using WeakPortInfo = std::weak_ptr<PortInfo>;
using SharedChannelInfo = std::shared_ptr<ChannelInfo>;
using Command = std::function<void()>;
using _MidiMessage = MidiMessage<Time, Size>;
using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;
using SharedDecoupledMidiPort = std::shared_ptr<_DecoupledMidiPort>;

// GLOBALS
namespace {
std::unique_ptr<AudioSystem> g_audio_system;
std::shared_ptr<AudioBufferPool> g_audio_buffer_pool;
std::vector<SharedLoopInfo> g_loops;
std::vector<SharedPortInfo> g_ports;
std::vector<SharedDecoupledMidiPort> g_decoupled_midi_ports;
CommandQueue g_cmd_queue (gc_command_queue_size, 1000, 1000);
}

// TYPES
struct PortInfo : public std::enable_shared_from_this<PortInfo> {
    const SharedPort port;
    audio_sample_t *maybe_audio_buffer;
    std::shared_ptr<MidiReadableBufferInterface> maybe_midi_input_buffer;
    std::shared_ptr<MidiWriteableBufferInterface> maybe_midi_output_buffer;

    PortInfo (SharedPort const& port) : port(port),
        maybe_audio_buffer(nullptr),
        maybe_midi_input_buffer(nullptr),
        maybe_midi_output_buffer(nullptr) {}

    void PROC_reset_buffers();
    void PROC_update_audio_buffer_if_none(size_t n_frames);
    void PROC_update_midi_input_buffer_if_none(size_t n_frames);
    void PROC_update_midi_output_buffer_if_none(size_t n_frames);
    AudioPort &audio();
    MidiPort &midi();
};

struct ChannelInfo : public std::enable_shared_from_this<ChannelInfo> {
    SharedLoopChannel channel;
    SharedLoopInfo loop;

    WeakPortInfo mp_input_port_mapping;
    std::vector<WeakPortInfo> mp_output_port_mappings;

    ChannelInfo(SharedLoopChannel chan,
                SharedLoopInfo loop) :
        channel(chan),
        loop(loop) {
            mp_output_port_mappings.reserve(gc_default_max_port_mappings);
    }

    // NOTE: only use on process thread
    ChannelInfo &operator= (ChannelInfo const& other) {
        loop = other.loop;
        channel = other.channel;
        mp_input_port_mapping = other.mp_input_port_mapping;
        mp_output_port_mappings = other.mp_output_port_mappings;
        return *this;
    }

    void connect_output_port(SharedPortInfo port, bool thread_safe=true);
    void connect_input_port(SharedPortInfo port, bool thread_safe=true);
    void disconnect_output_port(SharedPortInfo port, bool thread_safe=true);
    void disconnect_output_ports(bool thread_safe=true);
    void disconnect_input_port(SharedPortInfo port, bool thread_safe=true);
    void disconnect_input_port(bool thread_safe=true);
    void PROC_prepare_process_audio(size_t n_frames);
    void PROC_prepare_process_midi(size_t n_frames);
    void PROC_finalize_process_audio();
    void PROC_finalize_process_midi();
    LoopAudioChannel &audio();
    LoopMidiChannel &midi();
};

struct LoopInfo : public std::enable_shared_from_this<LoopInfo> {
    const SharedLoop loop;
    std::vector<SharedChannelInfo> mp_audio_channels;
    std::vector<SharedChannelInfo>  mp_midi_channels;

    LoopInfo() : loop(std::make_shared<Loop>()) {
        mp_audio_channels.reserve(gc_default_max_audio_channels);
        mp_midi_channels.reserve(gc_default_max_midi_channels);
    }

    void delete_audio_channel_idx(size_t idx);
    void delete_midi_channel_idx(size_t idx);
    void delete_audio_channel(SharedChannelInfo chan);
    void delete_midi_channel(SharedChannelInfo chan);
    void PROC_prepare_process(size_t n_frames);
    void PROC_finalize_process();

    // SharedLoopAudioChannel find_audio_channel(
    //                                       shoopdaloop_loop_audio_channel *channel,
    //                                       size_t &found_idx_out) {
    //     for (size_t idx=0; idx < loop->n_audio_channels(); idx++) {
    //         SharedLoopAudioChannel chan = loop->audio_channel<float>(idx);
    //         if ((shoopdaloop_loop_audio_channel*)chan.get() == channel) {
    //             found_idx_out = idx;
    //             return chan;
    //         }
    //     }
    //     throw std::runtime_error("Attempting to find non-existent audio channel.");
    // }

    // SharedLoopMidiChannel find_midi_channel(
    //                                         shoopdaloop_loop_midi_channel *channel,
    //                                         size_t &found_idx_out) {
    //     for (size_t idx=0; idx < loop->n_midi_channels(); idx++) {
    //         SharedLoopMidiChannel chan = loop->midi_channel<Time, Size>(idx);
    //         if ((shoopdaloop_loop_midi_channel*)chan.get() == channel) {
    //             found_idx_out = idx;
    //             return chan;
    //         }
    //     }
    //     throw std::runtime_error("Attempting to find non-existent midi channel.");
    // }
};

// MEMBER FUNCTIONS
void PortInfo::PROC_reset_buffers() {
    maybe_audio_buffer = nullptr;
    maybe_midi_input_buffer = nullptr;
    maybe_midi_output_buffer = nullptr;
}

void PortInfo::PROC_update_audio_buffer_if_none(size_t n_frames) {
    if (maybe_audio_buffer) { return; } // Only get a buffer if we don't have one yet
    auto _port = dynamic_cast<AudioPort*>(port.get());
    if (!_port) {
        throw std::runtime_error("Attempting to get audio buffer for non-audio port");
    }
    maybe_audio_buffer = _port->PROC_get_buffer(n_frames);
}

void PortInfo::PROC_update_midi_input_buffer_if_none(size_t n_frames) {
    if (maybe_midi_input_buffer) { return; } // Only get a buffer if we don't have one yet
    auto _port = dynamic_cast<MidiPort*>(port.get());
    if (!_port) {
        throw std::runtime_error("Attempting to get MIDI input buffer for non-MIDI port");
    }
    maybe_midi_input_buffer = _port->PROC_get_read_buffer(n_frames);
}

void PortInfo::PROC_update_midi_output_buffer_if_none(size_t n_frames) {
    if (maybe_midi_output_buffer) { return; } // Only get a buffer if we don't have one yet
    auto _port = dynamic_cast<MidiPort*>(port.get());
    if (!_port) {
        throw std::runtime_error("Attempting to get MIDI output buffer for non-MIDI port");
    }
    maybe_midi_output_buffer = _port->PROC_get_write_buffer(n_frames);
}

AudioPort &PortInfo::audio() {
    return *dynamic_cast<AudioPort*>(port.get());
}

MidiPort &PortInfo::midi() {
    return *dynamic_cast<MidiPort*>(port.get());
}

void ChannelInfo::connect_output_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [&]() {
        for (auto const& elem : mp_output_port_mappings) {
            if (elem.lock() == port) { return; }
        }
        mp_output_port_mappings.push_back(port);
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::connect_input_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [&]() {
        mp_input_port_mapping = port;
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_output_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [&]() {
        for (auto it = mp_output_port_mappings.rbegin(); it != mp_output_port_mappings.rend(); it++) {
            mp_output_port_mappings.erase(
                std::remove_if(mp_output_port_mappings.begin(), mp_output_port_mappings.end(),
                    [&](auto const& e) {
                        auto l = e.lock();
                        return !l || l == port;
                    }), mp_output_port_mappings.end());
        }
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_output_ports(bool thread_safe) {
    auto fn = [&]() {
        mp_output_port_mappings.clear();
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_input_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [&]() {
        auto locked = mp_input_port_mapping.lock();
        if (locked) {
            if (port != locked) {
                throw std::runtime_error("Attempting to disconnect unconnected input");
            }
        }
        mp_input_port_mapping.reset();
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_input_port(bool thread_safe) {
    auto fn = [&]() {
        mp_input_port_mapping.reset();
    };
    if (thread_safe) { g_cmd_queue.queue(fn); }
    else { fn(); }
}

#warning This does not deal with multiple output channels properly
void ChannelInfo::PROC_prepare_process_audio(size_t n_frames) {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        in_locked->PROC_update_audio_buffer_if_none(n_frames);
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_audio_buffer, n_frames);
    }
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        if (locked) {
            locked->PROC_update_audio_buffer_if_none(n_frames);
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            chan->PROC_set_playback_buffer(in_locked->maybe_audio_buffer, n_frames);
        }
    }
}

void ChannelInfo::PROC_finalize_process_audio() {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        in_locked->PROC_reset_buffers();
    }
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        if (locked) {
            locked->PROC_reset_buffers();
        }
    }
}

void ChannelInfo::PROC_prepare_process_midi(size_t n_frames) {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        in_locked->PROC_update_midi_input_buffer_if_none(n_frames);
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_midi_input_buffer.get(), n_frames);
    }
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        if (locked) {
            locked->PROC_update_midi_output_buffer_if_none(n_frames);
            auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
            chan->PROC_set_playback_buffer(in_locked->maybe_midi_output_buffer.get(), n_frames);
        }
    }
}


void ChannelInfo::PROC_finalize_process_midi() {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        in_locked->PROC_reset_buffers();
    }
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        if (locked) {
            locked->PROC_reset_buffers();
        }
    }
}

LoopAudioChannel &ChannelInfo::audio() {
    auto _chan = dynamic_cast<LoopAudioChannel*>(channel.get());
    return *_chan;
}

LoopMidiChannel &ChannelInfo::midi() {
    auto _chan = dynamic_cast<LoopMidiChannel*>(channel.get());
    return *_chan;
}

void LoopInfo::PROC_prepare_process(size_t n_frames) {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_prepare_process_audio(n_frames);
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_prepare_process_midi(n_frames);
    }
}

void LoopInfo::PROC_finalize_process() {
    for (auto &chan : mp_audio_channels) {
        chan->PROC_finalize_process_audio();
    }
    for (auto &chan : mp_midi_channels) {
        chan->PROC_finalize_process_midi();
    }
}

void LoopInfo::delete_audio_channel_idx(size_t idx) {
    g_cmd_queue.queue([&]() {
        auto _chan = loop->audio_channel<audio_sample_t>(idx);
        auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [&](auto const& e) { return _chan.get() == e->channel.get(); });
        if (r == mp_audio_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent audio channel.");
        }
        mp_audio_channels.erase(r);
        loop->delete_audio_channel(idx, false);
    });
}

void LoopInfo::delete_midi_channel_idx(size_t idx) {
    g_cmd_queue.queue([&]() {
        auto _chan = loop->midi_channel<Time, Size>(idx);
        auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [&](auto const& e) { return _chan.get() == e->channel.get(); });
        if (r == mp_midi_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent midi channel.");
        }
        mp_midi_channels.erase(r);
        loop->delete_midi_channel(idx, false);
    });
}

void LoopInfo::delete_audio_channel(SharedChannelInfo chan) {
    g_cmd_queue.queue([&]() {
        auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [&](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_audio_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent audio channel.");
        }
        auto idx = r - mp_audio_channels.begin();
        mp_audio_channels.erase(r);
        loop->delete_audio_channel(idx, false);
    });
}

void LoopInfo::delete_midi_channel(SharedChannelInfo chan) {
    g_cmd_queue.queue([&]() {
        auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [&](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_midi_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent midi channel.");
        }
        auto idx = r - mp_midi_channels.begin();
        mp_midi_channels.erase(r);
        loop->delete_midi_channel(idx, false);
    });
}

// HELPER FUNCTIONS
SharedPortInfo internal_audio_port(shoopdaloop_audio_port *port) {
    return ((PortInfo*)port)->shared_from_this();
}

SharedPortInfo internal_midi_port(shoopdaloop_midi_port *port) {
    return ((PortInfo*)port)->shared_from_this();
}

SharedChannelInfo internal_audio_channel(shoopdaloop_loop_audio_channel *chan) {
    return ((ChannelInfo*)chan)->shared_from_this();
}

SharedChannelInfo internal_midi_channel(shoopdaloop_loop_midi_channel *chan) {
    return ((ChannelInfo*)chan)->shared_from_this();
}

SharedLoopInfo internal_loop(shoopdaloop_loop *loop) {
    return ((LoopInfo*)loop)->shared_from_this();
}

shoopdaloop_audio_port* external_audio_port(SharedPortInfo port) {
    return (shoopdaloop_audio_port*) port.get();
}

shoopdaloop_midi_port* external_midi_port(SharedPortInfo port) {
    return (shoopdaloop_midi_port*) port.get();
}

shoopdaloop_loop_audio_channel* external_audio_channel(SharedChannelInfo port) {
    return (shoopdaloop_loop_audio_channel*) port.get();
}

shoopdaloop_loop_midi_channel* external_midi_channel(SharedChannelInfo port) {
    return (shoopdaloop_loop_midi_channel*) port.get();
}

shoopdaloop_loop* external_loop(SharedLoopInfo loop) {
    return (shoopdaloop_loop*)loop.get();
}

audio_channel_data_t external_audio_data(std::vector<audio_sample_t> f) {
    audio_channel_data_t d;
    d.n_samples = f.size();
    d.data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * f.size());
    memcpy((void*)d.data, (void*)f.data(), sizeof(audio_sample_t) * f.size());
    return d;
}

midi_channel_data_t external_midi_data(std::vector<_MidiMessage> m) {
    midi_channel_data_t d;
    d.n_events = m.size();
    d.events = (midi_event_t*) malloc(sizeof(midi_event_t) * m.size());
    for (size_t idx=0; idx < m.size(); idx++) {
        midi_event_t e;
        e.size = m[idx].size;
        e.time = m[idx].time;
        e.data = (unsigned char*) malloc(m[idx].size);
        memcpy((void*)e.data, (void*)m[idx].data.data(), m[idx].size);
        d.events[idx] = e;
    }
    return d;
}

std::vector<float> internal_audio_data(audio_channel_data_t const& d) {
    auto r = std::vector<float>(d.n_samples);
    memcpy((void*)r.data(), (void*)d.data, sizeof(audio_sample_t)*d.n_samples);
    return r;
}

std::vector<_MidiMessage> internal_midi_data(midi_channel_data_t const& d) {
    auto r = std::vector<_MidiMessage>(d.n_events);
    for (size_t idx=0; idx < d.n_events; idx++) {
        _MidiMessage m;
        m.size = d.events[idx].size;
        m.time = d.events[idx].time;
        m.data = std::vector<uint8_t>(m.size);
        memcpy((void*)m.data.data(), (void*)d.events[idx].data, m.size);
        r[idx] = m;
    }
    return r;
}

shoopdaloop_decoupled_midi_port *external_decoupled_midi_port(SharedDecoupledMidiPort port) {
    return (shoopdaloop_decoupled_midi_port*) port.get();
}

SharedDecoupledMidiPort internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port *port) {
    return ((_DecoupledMidiPort*)port)->shared_from_this();
}

PortDirection internal_port_direction(port_direction_t d) {
    return d == Input ? PortDirection::Input : PortDirection::Output;
}

void PROC_process_decoupled_midi_ports(size_t n_frames) {
    for (auto &elem : g_decoupled_midi_ports) {
        elem->PROC_process(n_frames);
    }
}

void PROC_process(size_t n_frames) {
    // Execute queued commands
    g_cmd_queue.PROC_exec_all();

    // Send/receive decoupled midi
    PROC_process_decoupled_midi_ports(n_frames);

    // Prepare:
    // Get and connect port buffers to loop channels
    for (size_t idx=0; idx < g_loops.size(); idx++) {
        g_loops[idx]->PROC_prepare_process(n_frames);
    }
    
    // Process the loops.
    process_loops<LoopInfo>
        (g_loops, [](LoopInfo &l) { return (LoopInterface*)l.loop.get(); }, n_frames);

    // Prepare state for next round.
    for (size_t idx=0; idx < g_loops.size(); idx++) {
        g_loops[idx]->PROC_finalize_process();
    }
}

// API FUNCTIONS

void initialize (const char* client_name_hint) {
    if (!g_audio_system) {
        g_audio_system = std::make_unique<JackAudioSystem>(std::string(client_name_hint), PROC_process);
    }
    if (!g_audio_buffer_pool) {
        g_audio_buffer_pool = std::make_shared<AudioBufferPool>(
            gc_n_buffers_in_pool, gc_audio_buffer_size
        );
    }
    g_cmd_queue.clear();
    g_loops.clear();
    g_ports.clear();
    g_decoupled_midi_ports.clear();

    g_loops.reserve(gc_initial_max_loops);
    g_ports.reserve(gc_initial_max_ports);
    g_decoupled_midi_ports.reserve(gc_initial_max_decoupled_midi_ports);

    g_audio_system->start();
}

void terminate() {
    throw std::runtime_error("terminate() not implemented");
}

jack_client_t *get_jack_client_handle() {
    if (!g_audio_system) {
        throw std::runtime_error("get_jack_client_handle() called before intialization");
    }
    return g_audio_system->get_client();
}

const char *get_jack_client_name() {
    if (!g_audio_system) {
        throw std::runtime_error("get_jack_client_name() called before intialization");
    }
    return g_audio_system->get_client_name();
}

unsigned get_sample_rate() {
    if (!g_audio_system) {
        throw std::runtime_error("get_sample_rate() called before initialization");
    }
    return g_audio_system->get_sample_rate();
}

shoopdaloop_loop *create_loop() {
    auto r = std::make_shared<LoopInfo>();
    g_cmd_queue.queue([r]() {
        g_loops.push_back(r);
    });
    return (shoopdaloop_loop*) r.get();
}

shoopdaloop_loop_audio_channel *add_audio_channel (shoopdaloop_loop *loop) {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    SharedLoopInfo loop_info = internal_loop(loop);
    auto r = std::make_shared<ChannelInfo> (nullptr, loop_info);
    g_cmd_queue.queue([r, loop_info]() {
        auto chan = loop_info->loop->add_audio_channel<audio_sample_t>(g_audio_buffer_pool,
                                                        gc_audio_channel_initial_buffers,
                                                        false);
        auto replacement = std::make_shared<ChannelInfo> (chan, loop_info);
        loop_info->mp_audio_channels.push_back(r);
        *r = *replacement;
    });
    return external_audio_channel(r);
}

shoopdaloop_loop_midi_channel *add_midi_channel (shoopdaloop_loop *loop) {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    auto r = std::make_shared<ChannelInfo> (nullptr, nullptr);
    g_cmd_queue.queue([=]() {
        SharedLoopInfo loop_info = internal_loop(loop);
        auto chan = loop_info->loop->add_midi_channel<Time, Size>(gc_midi_storage_size, false);
        auto replacement = std::make_shared<ChannelInfo> (chan, loop_info);
        loop_info->mp_midi_channels.push_back(r);
        *r = *replacement;
    });
    return external_midi_channel(r);
}

shoopdaloop_loop_audio_channel *get_audio_channel (shoopdaloop_loop *loop, size_t idx) {
    throw std::runtime_error("get_audio_channel not implemented\n");
    //return external_audio_channel(internal_loop(loop)->loop->audio_channel<audio_sample_t>(idx));
}

shoopdaloop_loop_midi_channel *get_midi_channel (shoopdaloop_loop *loop, size_t idx) {
    throw std::runtime_error("get_midi_channel not implemented\n");
    //return external_midi_channel(internal_loop(loop)->loop->midi_channel<Time, Size>(idx));
}

unsigned get_n_audio_channels (shoopdaloop_loop *loop) {
    return internal_loop(loop)->loop->n_audio_channels();
}

unsigned get_n_midi_channels (shoopdaloop_loop *loop) {
    return internal_loop(loop)->loop->n_midi_channels();
}

void delete_loop (shoopdaloop_loop *loop) {
    auto r = std::find_if(g_loops.begin(), g_loops.end(),
        [&](auto const& e) { return (shoopdaloop_loop*)e.get() == loop; });
    if (r == g_loops.end()) {
        throw std::runtime_error("Attempting to delete non-existent loop.");
    }
    g_loops.erase(r);
}

audio_data_t get_loop_audio_data (shoopdaloop_loop *loop) {
    auto &loop_info = *internal_loop(loop);
    audio_data_t a;
    a.n_channels = loop_info.loop->n_audio_channels();
    a.channels_data = (audio_channel_data_t*)malloc(sizeof(audio_channel_data_t) * a.n_channels);
    for (size_t idx=0; idx < a.n_channels; idx++) {
        a.channels_data[idx] = get_audio_channel_data(get_audio_channel(loop, idx));
    }
    return a;
}

midi_data_t get_loop_midi_data (shoopdaloop_loop *loop) {
    auto &loop_info = *internal_loop(loop);
    midi_data_t a;
    a.n_channels = loop_info.loop->n_midi_channels();
    a.channels_data = (midi_channel_data_t*)malloc(sizeof(midi_channel_data_t) * a.n_channels);
    for (size_t idx=0; idx < a.n_channels; idx++) {
        a.channels_data[idx] = get_midi_channel_data(get_midi_channel(loop, idx));
    }
    return a;
}

void load_loop_audio_data(shoopdaloop_loop *loop, audio_data_t data) {
    auto &loop_info = *internal_loop(loop);
    for (size_t idx=0; idx < data.n_channels; idx++) {
        audio_channel_data_t &d = data.channels_data[idx];
        load_audio_channel_data(get_audio_channel(loop, idx), d);
    }
}

void load_loop_midi_data(shoopdaloop_loop *loop, midi_data_t data) {
    auto &loop_info = *internal_loop(loop);
    for (size_t idx=0; idx < data.n_channels; idx++) {
        midi_channel_data_t &d = data.channels_data[idx];
        load_midi_channel_data(get_midi_channel(loop, idx), d);
    }
}

loop_data_t get_loop_data (shoopdaloop_loop *loop) {
    auto &loop_info = *internal_loop(loop);

    loop_data_t r;
    r.audio_data = get_loop_audio_data(loop);
    r.midi_data = get_loop_midi_data(loop);
    r.length = loop_info.loop->get_length();

    return r;
}

void load_loop_data (shoopdaloop_loop *loop, loop_data_t data) {
    load_loop_audio_data(loop, data.audio_data);
    load_loop_midi_data(loop, data.midi_data);
    internal_loop(loop)->loop->set_length(data.length);
}

void delete_audio_channel(shoopdaloop_loop *loop, shoopdaloop_loop_audio_channel *channel) {
    auto &loop_info = *internal_loop(loop);
    size_t idx;
    loop_info.delete_audio_channel(internal_audio_channel(channel));
}

void delete_midi_channel(shoopdaloop_loop *loop, shoopdaloop_loop_midi_channel *channel) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel(internal_midi_channel(channel));
}

void delete_audio_channel_idx(shoopdaloop_loop *loop, size_t idx) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_audio_channel_idx(idx);
}

void delete_midi_channel_idx(shoopdaloop_loop *loop, size_t idx) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel_idx(idx);
}

void connect_audio_output(shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port *port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_midi_output(shoopdaloop_loop_midi_channel *channel, shoopdaloop_midi_port *port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_audio_input(shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port *port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void connect_midi_input(shoopdaloop_loop_midi_channel *channel, shoopdaloop_midi_port *port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void disconnect_audio_output (shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port* port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_midi_output (shoopdaloop_loop_midi_channel  *channel, shoopdaloop_midi_port* port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_audio_outputs (shoopdaloop_loop_audio_channel *channel) {
    g_cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

void disconnect_midi_outputs  (shoopdaloop_loop_midi_channel  *channel) {
    g_cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

audio_channel_data_t get_audio_channel_data (shoopdaloop_loop_audio_channel *channel) {
    auto &chan = *internal_audio_channel(channel);
    return external_audio_data(chan.audio().get_data());
}

audio_channel_data_t get_audio_rms_data (shoopdaloop_loop_audio_channel *channel,
                                        unsigned from_sample,
                                        unsigned to_sample,
                                        unsigned samples_per_bin) {
    auto &chan = *internal_audio_channel(channel);
    auto data = chan.audio().get_data();
    auto n_samples = to_sample - from_sample;
    std::vector<audio_sample_t> bins (std::ceil((float)n_samples / (float)samples_per_bin));
    for (size_t idx = 0; idx < bins.size(); idx++) {
        bins[idx] = 0.0;
        size_t from = from_sample + samples_per_bin*idx;
        size_t to = std::min(from_sample + samples_per_bin, to_sample);
        for (size_t di = from; di < to; di++) {
            bins[idx] += sqrtf(data[di] * data[di]);
        }
    }
    return external_audio_data(bins);
}

midi_channel_data_t get_midi_channel_data (shoopdaloop_loop_midi_channel  *channel) {
    auto &chan = *internal_midi_channel(channel);
    return external_midi_data(chan.midi().retrieve_contents());
}

void load_audio_channel_data  (shoopdaloop_loop_audio_channel *channel, audio_channel_data_t data) {
    auto &chan = *internal_audio_channel(channel);
    chan.audio().load_data(data.data, data.n_samples);
}

void load_midi_channel_data (shoopdaloop_loop_midi_channel  *channel, midi_channel_data_t  data) {
    auto &chan = *internal_midi_channel(channel);
    chan.midi().set_contents(internal_midi_data(data));
}

void loop_transition(shoopdaloop_loop *loop,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync) {
    g_cmd_queue.queue([=]() {
        auto &loop_info = *internal_loop(loop);
        loop_info.loop->plan_transition(mode, delay, wait_for_soft_sync, false);
    });
}

void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop **loops,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_soft_sync) {
    g_cmd_queue.queue([=]() {
        for (size_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *internal_loop(loops[idx]);
            loop_info.loop->plan_transition(mode, delay, wait_for_soft_sync, false);
        }
    });
}

void clear_loop (shoopdaloop_loop *loop, size_t length) {
    g_cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        for (auto &chan : _loop.mp_audio_channels) {
            chan->audio().PROC_clear(length);
        }
        for (auto &chan : _loop.mp_midi_channels) {
            chan->midi().PROC_clear();
        }
        _loop.loop->set_length(length, false);
    });
}

void clear_audio_channel (shoopdaloop_loop_audio_channel *channel, size_t length) {
    g_cmd_queue.queue([=]() {
        internal_audio_channel(channel)->audio().PROC_clear(length);
    });
}

void clear_midi_channel (shoopdaloop_loop_midi_channel *channel) {
    g_cmd_queue.queue([=]() {
        internal_midi_channel(channel)->midi().PROC_clear();
    });
}

shoopdaloop_audio_port *open_audio_port (const char* name_hint, port_direction_t direction) {
    auto port = g_audio_system->open_audio_port
        (name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<PortInfo>(port);
    g_cmd_queue.queue([pi]() {
        g_ports.push_back(pi);
    });
    return external_audio_port(pi);
}

void close_audio_port (shoopdaloop_audio_port *port) {
    auto pi = internal_audio_port(port);
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    g_cmd_queue.queue([=]() {
        g_ports.erase(
            std::remove_if(g_ports.begin(), g_ports.end(),
                [&](auto const& e) { return e == pi; }),
            g_ports.end()
        );
    });
}

jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port *port) {
    auto pi = internal_audio_port(port);
    auto pp = &pi->audio();
    return dynamic_cast<JackAudioPort*>(pp)->get_jack_port();
}

shoopdaloop_midi_port *open_midi_port (const char* name_hint, port_direction_t direction) {
    auto port = g_audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<PortInfo>(port);
    g_cmd_queue.queue([pi]() {
        g_ports.push_back(pi);
    });
    return external_midi_port(pi);
}

void close_midi_port (shoopdaloop_midi_port *port) {
    // Removing from the list should be enough, as there
    // are only weak pointers elsewhere.
    g_cmd_queue.queue([=]() {
        auto pi = internal_midi_port(port);
        g_ports.erase(
            std::remove_if(g_ports.begin(), g_ports.end(),
                [&](auto const& e) { return e == pi; }),
            g_ports.end()
        );
    });
}
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port *port) {
    auto pi = internal_midi_port(port);
    auto pp = &pi->midi();
    return dynamic_cast<JackMidiPort*>(pp)->get_jack_port();
}

shoopdaloop_decoupled_midi_port *open_decoupled_midi_port(const char* name_hint, port_direction_t direction) {
    auto port = g_audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto decoupled = std::make_shared<_DecoupledMidiPort>(port,
        gc_decoupled_midi_port_queue_size,
        direction == Input ? PortDirection::Input : PortDirection::Output);
    g_cmd_queue.queue_and_wait([=]() {
        g_decoupled_midi_ports.push_back(decoupled);
    });
    return external_decoupled_midi_port(decoupled);
}

midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port *port) {
    auto &_port = *internal_decoupled_midi_port(port);
    auto m = _port.pop_incoming();
    if (m.has_value()) {
        auto r = (midi_event_t*) malloc(sizeof(midi_event_t));
        r->time = 0;
        r->size = m->size;
        r->data = (uint8_t*)malloc(r->size);
        memcpy((void*)r->data, (void*)m->data.data(), r->size);
        return r;
    } else {
        return nullptr;
    }
}

void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port *port) {
    g_cmd_queue.queue([=]() {
        auto _port = internal_decoupled_midi_port(port);
        g_decoupled_midi_ports.erase(
            std::remove_if(g_decoupled_midi_ports.begin(), g_decoupled_midi_ports.end(),
                [&](auto const& e) { return e.get() == _port.get(); }),
            g_decoupled_midi_ports.end()
        );
    });
}

void send_decoupled_midi(shoopdaloop_decoupled_midi_port *port, unsigned length, unsigned char *data) {
    auto &_port = *internal_decoupled_midi_port(port);
    DecoupledMidiMessage m;
    m.data.resize(length);
    memcpy((void*)m.data.data(), (void*)data, length);
    _port.push_outgoing(m);
}

void free_midi_event(midi_event_t e) {
    free(e.data);
}

void free_midi_channel_data(midi_channel_data_t d) {
    for(size_t idx=0; idx<d.n_events; idx++) {
        free_midi_event(d.events[idx]);
    }
    free(d.events);
}

void free_audio_channel_data(audio_channel_data_t d) {
    free(d.data);
}

midi_event_t alloc_midi_event(size_t data_bytes) {
    midi_event_t r;
    r.size = data_bytes;
    r.time = 0;
    r.data = (unsigned char*) malloc(data_bytes);
    return r;
}

midi_channel_data_t alloc_midi_channel_data(size_t n_events) {
    midi_channel_data_t r;
    r.n_events = n_events;
    r.events = (midi_event_t*)malloc (sizeof(midi_event_t) * n_events);
}

audio_channel_data_t alloc_audio_channel_data(size_t n_samples) {
    audio_channel_data_t r;
    r.n_samples = n_samples;
    r.data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * n_samples);
}
