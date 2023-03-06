#include "shoopdaloop_backend.h"

// Internal
#include "AudioBuffer.h"
#include "AudioChannel.h"
#include "AudioMidiLoop.h"
#include "AudioPortInterface.h"
#include "AudioSystemInterface.h"
#include "JackAudioPort.h"
#include "JackAudioSystem.h"
#include "DummyAudioSystem.h"
#include "MidiChannel.h"
#include "MidiMessage.h"
#include "MidiPortInterface.h"
#include "MidiMergingBuffer.h"
#include "ObjectPool.h"
#include "PortInterface.h"
#include "ChannelInterface.h"
#include "DecoupledMidiPort.h"
#include "CommandQueue.h"
#include "DummyMidiBufs.h"
#include "process_loops.h"
#include "types.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <cmath>
#include <jack/types.h>
#include <math.h>
#include <memory>
#include <stdexcept>
#include <map>
#include <set>

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
constexpr size_t gc_default_audio_dummy_buffer_size = 16384;
}

// FORWARD DECLARATIONS
struct LoopInfo;
struct PortInfo;
struct ChannelInfo;
struct Backend;
struct DecoupledMidiPortInfo;

// TYPE ALIASES
using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = ObjectPool<DefaultAudioBuffer>;
using Time = uint32_t;
using Size = uint16_t;
using AudioSystem = AudioSystemInterface<jack_nframes_t, size_t>;
using _DummyAudioSystem = DummyAudioSystem<jack_nframes_t, size_t>;
using Loop = AudioMidiLoop;
using SharedLoop = std::shared_ptr<Loop>;
using SharedLoopChannel = std::shared_ptr<ChannelInterface>;
using LoopAudioChannel = AudioChannel<audio_sample_t>;
using LoopMidiChannel = MidiChannel<uint32_t, uint16_t>;
using SharedLoopAudioChannel = std::shared_ptr<LoopAudioChannel>;
using SharedLoopMidiChannel = std::shared_ptr<LoopMidiChannel>;
using SharedPort = std::shared_ptr<PortInterface>;
using AudioPort = AudioPortInterface<audio_sample_t>;
using MidiPort = MidiPortInterface;
using SharedPortInfo = std::shared_ptr<PortInfo>;
using SharedLoopInfo = std::shared_ptr<LoopInfo>;
using WeakPortInfo = std::weak_ptr<PortInfo>;
using WeakLoopInfo = std::weak_ptr<LoopInfo>;
using SharedChannelInfo = std::shared_ptr<ChannelInfo>;
using Command = std::function<void()>;
using _MidiMessage = MidiMessage<Time, Size>;
using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;
using SharedDecoupledMidiPort = std::shared_ptr<_DecoupledMidiPort>;
using SharedDecoupledMidiPortInfo = std::shared_ptr<DecoupledMidiPortInfo>;
using SharedBackend = std::shared_ptr<Backend>;
using WeakBackend = std::weak_ptr<Backend>;

// GLOBALS
namespace {
std::vector<audio_sample_t> g_dummy_audio_input_buffer (gc_default_audio_dummy_buffer_size);
std::vector<audio_sample_t> g_dummy_audio_output_buffer (gc_default_audio_dummy_buffer_size);
std::shared_ptr<DummyReadMidiBuf> g_dummy_midi_input_buffer = std::make_shared<DummyReadMidiBuf>();
std::shared_ptr<DummyWriteMidiBuf> g_dummy_midi_output_buffer = std::make_shared<DummyWriteMidiBuf>();
std::set<SharedBackend> g_active_backends;
}

// TYPES
struct Backend : public std::enable_shared_from_this<Backend> {

    std::vector<SharedLoopInfo> loops;
    std::vector<SharedPortInfo> ports;
    std::vector<SharedDecoupledMidiPortInfo> decoupled_midi_ports;
    CommandQueue cmd_queue;
    std::shared_ptr<AudioBufferPool> audio_buffer_pool;
    std::unique_ptr<AudioSystem> audio_system;

    Backend (audio_system_type_t audio_system_type,
             std::string client_name_hint) :
        cmd_queue (gc_command_queue_size, 1000, 1000) {
        
        using namespace std::placeholders;

        switch (audio_system_type) {
        case Jack:
            audio_system = std::unique_ptr<AudioSystem>(dynamic_cast<AudioSystem*>(new JackAudioSystem(
                std::string(client_name_hint), std::bind(&Backend::PROC_process, this, _1)
            )));
            break;
        case Dummy:
            audio_system = std::unique_ptr<AudioSystem>(dynamic_cast<AudioSystem*>(new _DummyAudioSystem(
                std::string(client_name_hint), std::bind(&Backend::PROC_process, this, _1)
            )));
            break;
        default:
            throw std::runtime_error("Unimplemented backend type");
        }

        audio_buffer_pool = std::make_shared<AudioBufferPool>(gc_n_buffers_in_pool, gc_audio_buffer_size);
        loops.reserve(gc_initial_max_loops);
        ports.reserve(gc_initial_max_ports);
        decoupled_midi_ports.reserve(gc_initial_max_decoupled_midi_ports);
        audio_system->start();
    }

    void PROC_process (jack_nframes_t nframes);
    void PROC_process_decoupled_midi_ports(size_t nframes);
    void terminate();
    jack_client_t *maybe_jack_client_handle();
    const char* get_client_name();
    unsigned get_sample_rate();
    std::shared_ptr<LoopInfo> create_loop();
};

struct DecoupledMidiPortInfo : public std::enable_shared_from_this<DecoupledMidiPortInfo> {
    const SharedDecoupledMidiPort port;
    const WeakBackend backend;

    DecoupledMidiPortInfo(std::shared_ptr<_DecoupledMidiPort> port, SharedBackend backend) :
        port(port), backend(backend) {}

    Backend &get_backend();
};

struct PortInfo : public std::enable_shared_from_this<PortInfo> {
    const SharedPort port;
    WeakBackend backend;

    std::vector<WeakPortInfo> mp_passthrough_to;

    // Audio only
    audio_sample_t* maybe_audio_buffer;
    std::atomic<float> volume;
    std::atomic<float> passthrough_volume;
    std::atomic<float> peak;

    // Midi only
    std::shared_ptr<MidiReadableBufferInterface> maybe_midi_input_buffer;
    std::shared_ptr<MidiWriteableBufferInterface> maybe_midi_output_buffer;
    std::shared_ptr<MidiMergingBuffer> maybe_midi_output_merging_buffer;
    std::atomic<size_t> n_events_processed;

    // Both
    std::atomic<bool> muted;
    std::atomic<bool> passthrough_muted;

    PortInfo (SharedPort const& port, SharedBackend const& backend) : port(port),
        maybe_audio_buffer(nullptr),
        maybe_midi_input_buffer(nullptr),
        maybe_midi_output_buffer(nullptr),
        volume(1.0f),
        passthrough_volume(1.0f),
        muted(false),
        passthrough_muted(false),
        backend(backend),
        peak(0.0f),
        n_events_processed(0) {
        if (auto m = dynamic_cast<MidiPort*>(port.get())) {
            if(m->direction() == PortDirection::Output) {
                maybe_midi_output_merging_buffer = std::make_shared<MidiMergingBuffer>();
            }
        }
    }

    void PROC_reset_buffers();
    void PROC_ensure_buffer(size_t n_frames);
    void PROC_passthrough(size_t n_frames);
    void PROC_passthrough_audio(size_t n_frames, PortInfo &to);
    void PROC_passthrough_midi(size_t n_frames, PortInfo &to);
    void PROC_check_buffer();
    void PROC_finalize_process(size_t n_frames);

    void connect_passthrough(SharedPortInfo const& other);

    AudioPort &audio();
    MidiPort &midi();
    Backend &get_backend();
};

struct ChannelInfo : public std::enable_shared_from_this<ChannelInfo> {
    SharedLoopChannel channel;
    WeakLoopInfo loop;
    WeakPortInfo mp_input_port_mapping;
    WeakBackend backend;
    std::vector<WeakPortInfo> mp_output_port_mappings;

    ChannelInfo(SharedLoopChannel chan,
                SharedLoopInfo loop,
                SharedBackend backend) :
        channel(chan),
        loop(loop),
        backend(backend) {
            mp_output_port_mappings.reserve(gc_default_max_port_mappings);
    }

    // NOTE: only use on process thread
    ChannelInfo &operator= (ChannelInfo const& other) {
        if (backend.lock() != other.backend.lock()) {
            throw std::runtime_error("Cannot copy channels between back-ends");
        }
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
    Backend &get_backend();
};

struct LoopInfo : public std::enable_shared_from_this<LoopInfo> {
    const SharedLoop loop;
    std::vector<SharedChannelInfo> mp_audio_channels;
    std::vector<SharedChannelInfo>  mp_midi_channels;
    WeakBackend backend;

    LoopInfo(SharedBackend backend) :
        loop(std::make_shared<Loop>()),
        backend(backend) {
        mp_audio_channels.reserve(gc_default_max_audio_channels);
        mp_midi_channels.reserve(gc_default_max_midi_channels);
    }

    void delete_audio_channel_idx(size_t idx);
    void delete_midi_channel_idx(size_t idx);
    void delete_audio_channel(SharedChannelInfo chan);
    void delete_midi_channel(SharedChannelInfo chan);
    void PROC_prepare_process(size_t n_frames);
    void PROC_finalize_process();
    Backend &get_backend();
};

// MEMBER FUNCTIONS
void Backend::PROC_process (jack_nframes_t nframes) {
    // Execute queued commands
    cmd_queue.PROC_exec_all();

    // Send/receive decoupled midi
    PROC_process_decoupled_midi_ports(nframes);

    // Prepare:
    // Get buffers and process passthrough
    for (auto & port: ports) {
        port->PROC_reset_buffers();
        port->PROC_ensure_buffer(nframes);
    }
    for (auto & port: ports) {
        port->PROC_passthrough(nframes);
    }

    // Prepare:
    // Connect port buffers to loop channels
    for (auto & loop: loops) {
        loop->PROC_prepare_process(nframes);
    }
    
    // Process the loops.
    process_loops<LoopInfo>
        (loops, [](LoopInfo &l) { return (LoopInterface*)l.loop.get(); }, nframes);

    // Prepare state for next round.
    for (auto &port : ports) {
        port->PROC_finalize_process(nframes);
    }
    for (auto &loop : loops) {
        loop->PROC_finalize_process();
    }
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

std::shared_ptr<LoopInfo> Backend::create_loop() {
    auto r = std::make_shared<LoopInfo>(shared_from_this());
    cmd_queue.queue([this, r]() {
        loops.push_back(r);
    });
    return r;
}

void PortInfo::PROC_reset_buffers() {
    maybe_audio_buffer = nullptr;
    maybe_midi_input_buffer = nullptr;
    maybe_midi_output_buffer = nullptr;
}

void PortInfo::PROC_ensure_buffer(size_t n_frames) {
    auto maybe_midi = dynamic_cast<MidiPort*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPort*>(port.get());

    if (maybe_midi) {
        if(maybe_midi->direction() == PortDirection::Input) {
            if (maybe_midi_input_buffer) { return; } // already there
            maybe_midi_input_buffer = maybe_midi->PROC_get_read_buffer(n_frames);
            if (!muted) {
                n_events_processed += maybe_midi_input_buffer->PROC_get_n_events();
            }
        } else {
            if (maybe_midi_output_buffer) { return; } // already there
            maybe_midi_output_buffer = maybe_midi->PROC_get_write_buffer(n_frames);
        }
    } else if (maybe_audio) {
        if (maybe_audio_buffer) { return; } // already there
        maybe_audio_buffer = maybe_audio->PROC_get_buffer(n_frames);
        if (port->direction() == PortDirection::Input) {
            float max = 0.0f;
            for(size_t i=0; i<n_frames; i++) {
                // TODO: allowed to write to input buffer? test it
                maybe_audio_buffer[i] *= muted.load() ? 0.0f : volume.load();
                max = std::max(max, abs(maybe_audio_buffer[i]));
            }
            peak = std::max(peak.load(), max);
        }
    } else {
        throw std::runtime_error("Invalid port");
    }
}

void PortInfo::PROC_check_buffer() {
    auto maybe_midi = dynamic_cast<MidiPort*>(port.get());
    auto maybe_audio = dynamic_cast<AudioPort*>(port.get());
    bool result;

    if (maybe_midi) {
        if(maybe_midi->direction() == PortDirection::Input) {
            result = (bool) maybe_midi_input_buffer;
        } else {
            result = (bool) maybe_midi_output_buffer;
        }
    } else if (maybe_audio) {
        result = (maybe_audio_buffer != nullptr);
    } else {
        throw std::runtime_error("Invalid port");
    }

    if (!result) {
        throw std::runtime_error("No buffer available.");
    }
}

void PortInfo::PROC_passthrough(size_t n_frames) {
    if (port->direction() == PortDirection::Input) {
        for(auto & other : mp_passthrough_to) {
            auto o = other.lock();
            if(o) {
                if (o->port->direction() != PortDirection::Output) {
                    throw std::runtime_error("Cannot passthrough to input");
                }
                o->PROC_check_buffer();
                if (dynamic_cast<AudioPort*>(port.get())) { PROC_passthrough_audio(n_frames, *o); }
                else if (dynamic_cast<MidiPort*>(port.get())) { PROC_passthrough_midi(n_frames, *o); }
                else { throw std::runtime_error("Invalid port"); }
            }
        }
    }
}

void PortInfo::PROC_passthrough_audio(size_t n_frames, PortInfo &to) {
    if (!muted && !passthrough_muted) {
        for (size_t i=0; i<n_frames; i++) {
            to.maybe_audio_buffer[i] += passthrough_volume * maybe_audio_buffer[i];
        }
    }
}

void PortInfo::PROC_passthrough_midi(size_t n_frames, PortInfo &to) {
    if(!muted && !passthrough_muted) {
        for(size_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
            to.maybe_midi_output_buffer->PROC_write_event_reference(
                maybe_midi_input_buffer->PROC_get_event_reference(i)
                );
        }
    }
}

void PortInfo::PROC_finalize_process(size_t n_frames) {
    if (auto a = dynamic_cast<AudioPort*>(port.get())) {
        if (a->direction() == PortDirection::Output) {
            float max = 0.0f;
            for (size_t i=0; i<n_frames; i++) {
                maybe_audio_buffer[i] *= muted.load() ? 0.0f : volume.load();
                max = std::max(abs(maybe_audio_buffer[i]), max);
            }
            peak = std::max(peak.load(), max);
        }
    } else if (auto m = dynamic_cast<MidiPort*>(port.get())) {
        if (m->direction() == PortDirection::Output) {
            if (!muted) {
                maybe_midi_output_merging_buffer->PROC_sort();
                size_t n_events = maybe_midi_output_merging_buffer->PROC_get_n_events();
                for(size_t i=0; i<n_events; i++) {
                    uint32_t size, time;
                    const uint8_t* data;
                    maybe_midi_output_merging_buffer->PROC_get_event_reference(i).get(size, time, data);
                    maybe_midi_output_buffer->PROC_write_event_value(size, time, data);
                }
                n_events_processed += n_events;
            }
        }
    }
}

AudioPort &PortInfo::audio() {
    return *dynamic_cast<AudioPort*>(port.get());
}

MidiPort &PortInfo::midi() {
    return *dynamic_cast<MidiPort*>(port.get());
}

Backend &PortInfo::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void PortInfo::connect_passthrough(const SharedPortInfo &other) {
    get_backend().cmd_queue.queue([=]() {
        for (auto &_other : mp_passthrough_to) {
            if(auto __other = _other.lock()) {
                if (__other.get() == other.get()) { return; } // already connected
            }
        }
        mp_passthrough_to.push_back(other);
    });
}

Backend &DecoupledMidiPortInfo::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

void ChannelInfo::connect_output_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [this, port]() {
        for (auto const& elem : mp_output_port_mappings) {
            if (elem.lock() == port) { return; }
        }
        mp_output_port_mappings.push_back(port);
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::connect_input_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [this, port]() {
        mp_input_port_mapping = port;
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_output_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [this, port]() {
        for (auto it = mp_output_port_mappings.rbegin(); it != mp_output_port_mappings.rend(); it++) {
            mp_output_port_mappings.erase(
                std::remove_if(mp_output_port_mappings.begin(), mp_output_port_mappings.end(),
                    [port](auto const& e) {
                        auto l = e.lock();
                        return !l || l == port;
                    }), mp_output_port_mappings.end());
        }
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_output_ports(bool thread_safe) {
    auto fn = [this]() {
        mp_output_port_mappings.clear();
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_input_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [this, port]() {
        auto locked = mp_input_port_mapping.lock();
        if (locked) {
            if (port != locked) {
                throw std::runtime_error("Attempting to disconnect unconnected input");
            }
        }
        mp_input_port_mapping.reset();
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::disconnect_input_port(bool thread_safe) {
    auto fn = [this]() {
        mp_input_port_mapping.reset();
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

#warning This does not deal with multiple output channels properly
void ChannelInfo::PROC_prepare_process_audio(size_t n_frames) {
    auto in_locked = mp_input_port_mapping.lock();
    if (in_locked) {
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_audio_buffer, n_frames);
    } else {
        if (g_dummy_audio_input_buffer.size() < n_frames*sizeof(audio_sample_t)) {
            g_dummy_audio_input_buffer.resize(n_frames*sizeof(audio_sample_t));
            memset((void*)g_dummy_audio_input_buffer.data(), 0, n_frames*sizeof(audio_sample_t));
        }
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_recording_buffer(g_dummy_audio_input_buffer.data(), n_frames);
    }
    size_t n_outputs_mapped = 0;
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        if (locked) {
            auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
            chan->PROC_set_playback_buffer(locked->maybe_audio_buffer, n_frames);
            n_outputs_mapped++;
        }
    }
    if (n_outputs_mapped == 0) {
        if (g_dummy_audio_output_buffer.size() < n_frames*sizeof(audio_sample_t)) {
            g_dummy_audio_output_buffer.resize(n_frames*sizeof(audio_sample_t));
        }
        auto chan = dynamic_cast<LoopAudioChannel*>(channel.get());
        chan->PROC_set_playback_buffer(g_dummy_audio_output_buffer.data(), n_frames);
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
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(in_locked->maybe_midi_input_buffer.get(), n_frames);
    } else {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(g_dummy_midi_input_buffer.get(), n_frames);
    }
    size_t n_outputs_mapped = 0;
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        if (locked) {
            chan->PROC_set_playback_buffer(locked->maybe_midi_output_buffer.get(), n_frames);
        }
    }
    if (n_outputs_mapped == 0) {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_playback_buffer(g_dummy_midi_output_buffer.get(), n_frames);
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

Backend &ChannelInfo::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
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
    get_backend().cmd_queue.queue([this, idx]() {
        auto _chan = loop->audio_channel<audio_sample_t>(idx);
        auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [&_chan](auto const& e) { return _chan.get() == e->channel.get(); });
        if (r == mp_audio_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent audio channel.");
        }
        mp_audio_channels.erase(r);
        loop->delete_audio_channel(idx, false);
    });
}

void LoopInfo::delete_midi_channel_idx(size_t idx) {
    get_backend().cmd_queue.queue([this, idx]() {
        auto _chan = loop->midi_channel<Time, Size>(idx);
        auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [&_chan](auto const& e) { return _chan.get() == e->channel.get(); });
        if (r == mp_midi_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent midi channel.");
        }
        mp_midi_channels.erase(r);
        loop->delete_midi_channel(idx, false);
    });
}

void LoopInfo::delete_audio_channel(SharedChannelInfo chan) {
    get_backend().cmd_queue.queue([this, chan]() {
        auto r = std::find_if(mp_audio_channels.begin(), mp_audio_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_audio_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent audio channel.");
        }
        auto idx = r - mp_audio_channels.begin();
        mp_audio_channels.erase(r);
        loop->delete_audio_channel(idx, false);
    });
}

void LoopInfo::delete_midi_channel(SharedChannelInfo chan) {
    get_backend().cmd_queue.queue([this, chan]() {
        auto r = std::find_if(mp_midi_channels.begin(), mp_midi_channels.end(), [chan](auto const& e) { return chan->channel.get() == e->channel.get(); });
        if (r == mp_midi_channels.end()) {
            throw std::runtime_error("Attempting to delete non-existent midi channel.");
        }
        auto idx = r - mp_midi_channels.begin();
        mp_midi_channels.erase(r);
        loop->delete_midi_channel(idx, false);
    });
}

Backend &LoopInfo::get_backend() {
    auto b = backend.lock();
    if(!b) {
        throw std::runtime_error("Back-end no longer exists");
    }
    return *b;
}

// HELPER FUNCTIONS
SharedBackend internal_backend(shoopdaloop_backend_instance_t *backend) {
    return ((Backend*)backend)->shared_from_this();
}

SharedPortInfo internal_audio_port(shoopdaloop_audio_port_t *port) {
    return ((PortInfo*)port)->shared_from_this();
}

SharedPortInfo internal_midi_port(shoopdaloop_midi_port_t *port) {
    return ((PortInfo*)port)->shared_from_this();
}

SharedChannelInfo internal_audio_channel(shoopdaloop_loop_audio_channel_t *chan) {
    return ((ChannelInfo*)chan)->shared_from_this();
}

SharedChannelInfo internal_midi_channel(shoopdaloop_loop_midi_channel_t *chan) {
    return ((ChannelInfo*)chan)->shared_from_this();
}

#warning make the handles point to globally stored weak pointers to avoid trying to access deleted shared object
SharedLoopInfo internal_loop(shoopdaloop_loop_t *loop) {
    return ((LoopInfo*)loop)->shared_from_this();
}

shoopdaloop_backend_instance_t *external_backend(SharedBackend backend) {
    return (shoopdaloop_backend_instance_t*) backend.get();
}

shoopdaloop_audio_port_t* external_audio_port(SharedPortInfo port) {
    return (shoopdaloop_audio_port_t*) port.get();
}

shoopdaloop_midi_port_t* external_midi_port(SharedPortInfo port) {
    return (shoopdaloop_midi_port_t*) port.get();
}

shoopdaloop_loop_audio_channel_t* external_audio_channel(SharedChannelInfo port) {
    return (shoopdaloop_loop_audio_channel_t*) port.get();
}

shoopdaloop_loop_midi_channel_t* external_midi_channel(SharedChannelInfo port) {
    return (shoopdaloop_loop_midi_channel_t*) port.get();
}

shoopdaloop_loop_t* external_loop(SharedLoopInfo loop) {
    return (shoopdaloop_loop_t*)loop.get();
}

audio_channel_data_t *external_audio_data(std::vector<audio_sample_t> f) {
    auto d = new audio_channel_data_t;
    d->n_samples = f.size();
    d->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * f.size());
    memcpy((void*)d->data, (void*)f.data(), sizeof(audio_sample_t) * f.size());
    return d;
}

midi_channel_data_t *external_midi_data(std::vector<_MidiMessage> m) {
    auto d = new midi_channel_data_t;
    d->n_events = m.size();
    d->events = (midi_event_t**) malloc(sizeof(midi_event_t*) * m.size());
    for (size_t idx=0; idx < m.size(); idx++) {
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

std::vector<_MidiMessage> internal_midi_data(midi_channel_data_t const& d) {
    auto r = std::vector<_MidiMessage>(d.n_events);
    for (size_t idx=0; idx < d.n_events; idx++) {
        _MidiMessage m(
            d.events[idx]->size,
            d.events[idx]->time,
            std::vector<uint8_t>(m.size));
        memcpy((void*)m.data.data(), (void*)d.events[idx]->data, m.size);
        r[idx] = m;
    }
    return r;
}

shoopdaloop_decoupled_midi_port_t *external_decoupled_midi_port(SharedDecoupledMidiPortInfo port) {
    return (shoopdaloop_decoupled_midi_port_t*) port.get();
}

SharedDecoupledMidiPortInfo internal_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
    return ((DecoupledMidiPortInfo*)port)->shared_from_this();
}

PortDirection internal_port_direction(port_direction_t d) {
    return d == Input ? PortDirection::Input : PortDirection::Output;
}

std::optional<audio_system_type_t> audio_system_type(AudioSystem *sys) {
    if (!sys) {
        return std::nullopt;
    } else if (dynamic_cast<JackAudioSystem*>(sys)) {
        return Jack;
    } else if (dynamic_cast<_DummyAudioSystem*>(sys)) {
        return Dummy;
    } else {
        throw std::runtime_error("Unimplemented");
    }
}

// API FUNCTIONS

shoopdaloop_backend_instance_t *initialize (
    audio_system_type_t audio_system,
    const char* client_name_hint) {
    
    auto backend = std::make_shared<Backend>(audio_system, client_name_hint);
    g_active_backends.insert(backend);

    return external_backend(backend);
}

void terminate(shoopdaloop_backend_instance_t *backend) {
    auto _backend = internal_backend(backend);
    _backend->terminate();
    g_active_backends.erase(_backend);
}

jack_client_t *maybe_jack_client_handle(shoopdaloop_backend_instance_t* backend) {
    return internal_backend(backend)->maybe_jack_client_handle();
}

const char *get_client_name(shoopdaloop_backend_instance_t *backend) {
    return internal_backend(backend)->get_client_name();
}

unsigned get_sample_rate(shoopdaloop_backend_instance_t *backend) {
    return internal_backend(backend)->get_sample_rate();
}

shoopdaloop_loop_t *create_loop(shoopdaloop_backend_instance_t *backend) {
    return external_loop(internal_backend(backend)->create_loop());
}

shoopdaloop_loop_audio_channel_t *add_audio_channel (shoopdaloop_loop_t *loop, channel_mode_t mode) {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    SharedLoopInfo loop_info = internal_loop(loop);
    auto &backend = loop_info->get_backend();
    auto r = std::make_shared<ChannelInfo> (nullptr, loop_info, backend.shared_from_this());
    backend.cmd_queue.queue([r, loop_info, mode]() {
        auto chan = loop_info->loop->add_audio_channel<audio_sample_t>(loop_info->backend.lock()->audio_buffer_pool,
                                                        gc_audio_channel_initial_buffers,
                                                        mode,
                                                        false);
        auto replacement = std::make_shared<ChannelInfo> (chan, loop_info, loop_info->backend.lock());
        loop_info->mp_audio_channels.push_back(r);
        *r = *replacement;
    });
    return external_audio_channel(r);
}

shoopdaloop_loop_midi_channel_t *add_midi_channel (shoopdaloop_loop_t *loop, channel_mode_t mode) {
    // Note: we jump through hoops here to pre-create a shared ptr and then
    // queue a copy-assignment of its value. This allows us to return before
    // the channel has really been created, without altering the pointed-to
    // address later.
    SharedLoopInfo loop_info = internal_loop(loop);
    auto &backend = loop_info->get_backend();
    auto r = std::make_shared<ChannelInfo> (nullptr, nullptr, backend.shared_from_this());
    backend.cmd_queue.queue([loop_info, mode, r]() {
        auto chan = loop_info->loop->add_midi_channel<Time, Size>(gc_midi_storage_size, mode, false);
        auto replacement = std::make_shared<ChannelInfo> (chan, loop_info, loop_info->backend.lock());
        loop_info->mp_midi_channels.push_back(r);
        *r = *replacement;
    });
    return external_midi_channel(r);
}

shoopdaloop_loop_audio_channel_t *get_audio_channel (shoopdaloop_loop_t *loop, size_t idx) {
    throw std::runtime_error("get_audio_channel not implemented\n");
    //return external_audio_channel(internal_loop(loop)->loop->audio_channel<audio_sample_t>(idx));
}

shoopdaloop_loop_midi_channel_t *get_midi_channel (shoopdaloop_loop_t *loop, size_t idx) {
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
    auto &loop_info = *internal_loop(loop);
    size_t idx;
    loop_info.delete_audio_channel(internal_audio_channel(channel));
}

void delete_midi_channel(shoopdaloop_loop_t *loop, shoopdaloop_loop_midi_channel_t *channel) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel(internal_midi_channel(channel));
}

void delete_audio_channel_idx(shoopdaloop_loop_t *loop, size_t idx) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_audio_channel_idx(idx);
}

void delete_midi_channel_idx(shoopdaloop_loop_t *loop, size_t idx) {
    auto &loop_info = *internal_loop(loop);
    loop_info.delete_midi_channel_idx(idx);
}

void connect_audio_output(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_midi_output(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_output_port(_port, false);
    });
}

void connect_audio_input(shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t *port) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void connect_midi_input(shoopdaloop_loop_midi_channel_t *channel, shoopdaloop_midi_port_t *port) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->connect_input_port(_port, false);
    });
}

void disconnect_audio_output (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_midi_output (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_port(_port, false);
    });
}

void disconnect_audio_outputs (shoopdaloop_loop_audio_channel_t *channel) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

void disconnect_midi_outputs  (shoopdaloop_loop_midi_channel_t  *channel) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_output_ports(false);
    });
}

audio_channel_data_t *get_audio_channel_data (shoopdaloop_loop_audio_channel_t *channel) {
    auto &chan = *internal_audio_channel(channel);
    return external_audio_data(chan.audio().get_data());
}

audio_channel_data_t *get_audio_rms_data (shoopdaloop_loop_audio_channel_t *channel,
                                        unsigned from_sample,
                                        unsigned to_sample,
                                        unsigned samples_per_bin) {
    auto &chan = *internal_audio_channel(channel);
    auto data = chan.audio().get_data();
    auto n_samples = to_sample - from_sample;
    if (n_samples == 0) {
        return external_audio_data(std::vector<audio_sample_t>(0));
    } else {
        std::vector<audio_sample_t> bins (std::ceil((float)n_samples / (float)(samples_per_bin)));
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
}

midi_channel_data_t *get_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel) {
    auto &chan = *internal_midi_channel(channel);
    return external_midi_data(chan.midi().retrieve_contents());
}

void load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t *data) {
    auto &chan = *internal_audio_channel(channel);
    chan.audio().load_data(data->data, data->n_samples);
}

void load_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel, midi_channel_data_t  *data) {
    auto &chan = *internal_midi_channel(channel);
    chan.midi().set_contents(internal_midi_data(*data));
}

void loop_transition(shoopdaloop_loop_t *loop,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_sync) {
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &loop_info = *internal_loop(loop);
        loop_info.loop->plan_transition(mode, delay, wait_for_sync, false);
    });
}

void loops_transition(unsigned int n_loops,
                      shoopdaloop_loop_t **loops,
                      loop_mode_t mode,
                      size_t delay, // In # of triggers
                      unsigned wait_for_sync) {
    internal_loop(loops[0])->get_backend().cmd_queue.queue([=]() {
        for (size_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *internal_loop(loops[idx]);
            loop_info.loop->plan_transition(mode, delay, wait_for_sync, false);
        }
    });
}

void clear_loop (shoopdaloop_loop_t *loop, size_t length) {
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
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

void set_loop_length (shoopdaloop_loop_t *loop, size_t length) {
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        _loop.loop->set_length(length, false);
    });
}

void set_loop_position (shoopdaloop_loop_t *loop, size_t position) {
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        _loop.loop->set_position(position, false);
    });
}

void clear_audio_channel (shoopdaloop_loop_audio_channel_t *channel, size_t length) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        internal_audio_channel(channel)->audio().PROC_clear(length);
    });
}

void clear_midi_channel (shoopdaloop_loop_midi_channel_t *channel) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        internal_midi_channel(channel)->midi().PROC_clear();
    });
}

shoopdaloop_audio_port_t *open_audio_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_audio_port
        (name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<PortInfo>(port, _backend);
    _backend->cmd_queue.queue([pi, _backend]() {
        _backend->ports.push_back(pi);
    });
    return external_audio_port(pi);
}

void close_audio_port (shoopdaloop_backend_instance_t *backend, shoopdaloop_audio_port_t *port) {
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

jack_port_t *get_audio_port_jack_handle(shoopdaloop_audio_port_t *port) {
    auto pi = internal_audio_port(port);
    auto pp = &pi->audio();
    return dynamic_cast<JackAudioPort*>(pp)->get_jack_port();
}

shoopdaloop_midi_port_t *open_midi_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto pi = std::make_shared<PortInfo>(port, _backend);
    _backend->cmd_queue.queue([pi, _backend]() {
        _backend->ports.push_back(pi);
    });
    return external_midi_port(pi);
}

void close_midi_port (shoopdaloop_midi_port_t *port) {
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
jack_port_t *get_midi_port_jack_handle(shoopdaloop_midi_port_t *port) {
    auto pi = internal_midi_port(port);
    auto pp = &pi->midi();
    return dynamic_cast<JackMidiPort*>(pp)->get_jack_port();
}

void add_audio_port_passthrough(shoopdaloop_audio_port_t *from, shoopdaloop_audio_port_t *to) {
    auto _from = internal_audio_port(from);
    auto _to = internal_audio_port(to);
    _from->connect_passthrough(_to);
}

void add_midi_port_passthrough(shoopdaloop_midi_port_t *from, shoopdaloop_midi_port_t *to) {
    auto _from = internal_midi_port(from);
    auto _to = internal_midi_port(to);
    _from->connect_passthrough(_to);
}

void set_audio_port_muted(shoopdaloop_audio_port_t *port, unsigned int muted) {
    internal_audio_port(port)->muted = (bool)muted;
}

void set_audio_port_passthroughMuted(shoopdaloop_audio_port_t *port, unsigned int muted) {
    internal_audio_port(port)->passthrough_muted = (bool)muted;
}

void set_audio_port_volume(shoopdaloop_audio_port_t *port, float volume) {
    internal_audio_port(port)->volume = volume;
}

void set_audio_port_passthroughVolume(shoopdaloop_audio_port_t *port, float passthroughVolume) {
    internal_audio_port(port)->passthrough_volume = passthroughVolume;
}

void set_midi_port_muted(shoopdaloop_midi_port_t *port, unsigned int muted) {
    internal_midi_port(port)->muted = (bool)muted;
}

void set_midi_port_passthroughMuted(shoopdaloop_midi_port_t *port, unsigned int muted) {
    internal_midi_port(port)->passthrough_muted = (bool)muted;
}

shoopdaloop_decoupled_midi_port_t *open_decoupled_midi_port(shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
    auto _backend = internal_backend(backend);
    auto port = _backend->audio_system->open_midi_port(name_hint, internal_port_direction(direction));
    auto decoupled = std::make_shared<_DecoupledMidiPort>(port,
        gc_decoupled_midi_port_queue_size,
        direction == Input ? PortDirection::Input : PortDirection::Output);
    auto r = std::make_shared<DecoupledMidiPortInfo>(decoupled, _backend);
    _backend->cmd_queue.queue_and_wait([=]() {
        _backend->decoupled_midi_ports.push_back(r);
    });
    return external_decoupled_midi_port(r);
}

midi_event_t *maybe_next_message(shoopdaloop_decoupled_midi_port_t *port) {
    auto &_port = *internal_decoupled_midi_port(port);
    auto m = _port.port->pop_incoming();
    if (m.has_value()) {
        auto r = alloc_midi_event(m->size);
        r->time = 0;
        r->size = m->size;
        r->data = (uint8_t*)malloc(r->size);
        memcpy((void*)r->data, (void*)m->data.data(), r->size);
        return r;
    } else {
        return nullptr;
    }
}

void close_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *port) {
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

void send_decoupled_midi(shoopdaloop_decoupled_midi_port_t *port, unsigned length, unsigned char *data) {
    auto &_port = *internal_decoupled_midi_port(port);
    DecoupledMidiMessage m;
    m.data.resize(length);
    memcpy((void*)m.data.data(), (void*)data, length);
    _port.port->push_outgoing(m);
}

midi_event_t *alloc_midi_event(size_t data_bytes) {
    auto r = new midi_event_t;
    r->size = data_bytes;
    r->time = 0;
    r->data = (unsigned char*) malloc(data_bytes);
    return r;
}

midi_channel_data_t *alloc_midi_channel_data(size_t n_events) {
    auto r = new midi_channel_data_t;
    r->n_events = n_events;
    r->events = (midi_event_t**)malloc (sizeof(midi_event_t*) * n_events);
    return r;
}

audio_channel_data_t *alloc_audio_channel_data(size_t n_samples) {
    auto r = new audio_channel_data_t;
    r->n_samples = n_samples;
    r->data = (audio_sample_t*) malloc(sizeof(audio_sample_t) * n_samples);
    return r;
}

void set_audio_channel_volume (shoopdaloop_loop_audio_channel_t *channel, float volume) {
    auto r = new audio_channel_state_info_t;
    auto &_channel = *dynamic_cast<LoopAudioChannel*>(internal_audio_channel(channel)->channel.get());
    _channel.set_volume(volume);
}

#warning state getters incomplete
audio_channel_state_info_t *get_audio_channel_state (shoopdaloop_loop_audio_channel_t *channel) {
    auto r = new audio_channel_state_info_t;
    auto &_channel = *dynamic_cast<LoopAudioChannel*>(internal_audio_channel(channel)->channel.get());
    r->output_peak = _channel.get_output_peak();
    r->volume = _channel.get_volume();
    r->mode = _channel.get_mode();
    _channel.reset_output_peak();
    return r;
}

midi_channel_state_info_t *get_midi_channel_state   (shoopdaloop_loop_midi_channel_t  *channel) {
    auto r = new midi_channel_state_info_t;
    auto &_channel = *dynamic_cast<LoopMidiChannel*>(internal_midi_channel(channel)->channel.get());
    r->n_events_triggered = 0;
    r->mode = _channel.get_mode();
    return r;
}

void set_audio_channel_mode (shoopdaloop_loop_audio_channel_t * channel, channel_mode_t mode) {
    internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->channel->set_mode(mode);
    });
}

void set_midi_channel_mode (shoopdaloop_loop_midi_channel_t * channel, channel_mode_t mode) {
    internal_midi_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->channel->set_mode(mode);
    });
}

audio_port_state_info_t *get_audio_port_state(shoopdaloop_audio_port_t *port) {
    auto r = new audio_port_state_info_t;
    auto p = internal_audio_port(port);
    r->peak = p->peak;
    r->volume = p->volume;
    r->passthrough_volume = p->passthrough_volume;
    r->muted = p->muted;
    r->passthrough_muted = p->passthrough_muted;
    r->name = p->port->name();
    p->peak = 0.0f;
    return r;
}

midi_port_state_info_t *get_midi_port_state(shoopdaloop_midi_port_t *port) {
    auto r = new midi_port_state_info_t;
    auto p = internal_midi_port(port);
    r->n_events_triggered = p->n_events_processed;
    r->muted = p->muted;
    r->passthrough_muted = p->passthrough_muted;
    r->name = p->port->name();
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
    size_t next_delay;
    _loop->loop->get_first_planned_transition(next_mode, next_delay);
    r->maybe_next_mode = next_mode;
    r->maybe_next_mode_delay = next_delay;
    return r;
}

void set_loop_sync_source (shoopdaloop_loop_t *loop, shoopdaloop_loop_t *sync_source) {
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

void destroy_midi_event(midi_event_t *e) {
    free(e->data);
    free(e);
}

void destroy_midi_channel_data(midi_channel_data_t *d) {
    for(size_t idx=0; idx<d->n_events; idx++) {
        destroy_midi_event(d->events[idx]);
    }
    free(d->events);
    free(d);
}

void destroy_audio_channel_data(audio_channel_data_t *d) {
    free(d->data);
    free(d);
}

void destroy_audio_channel_state_info(audio_channel_state_info_t *d) {
    free(d);
}

void destroy_midi_channel_state_info(midi_channel_state_info_t *d) {
    free(d);
}

void destroy_loop(shoopdaloop_loop_t *d) {
    std::cerr << "Warning: destroying loops is unimplemented" << std::endl;
}

void destroy_audio_port(shoopdaloop_audio_port_t *d) {
    auto port = internal_audio_port(d);
    auto backend = port->backend.lock();
    backend->cmd_queue.queue([port, backend]() {
        // Remove port, which should stop anything from accessing it
        bool found = false;
        for(auto &elem : backend->ports) {
            if(elem == port) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find audio port to destroy");
        }
        // Now also close the port for the audio back-end
        port->port->close();
    });
}

void destroy_midi_port(shoopdaloop_midi_port_t *d) {
    auto port = internal_midi_port(d);
    auto backend = port->backend.lock();
    backend->cmd_queue.queue([port, backend]() {
        // Remove port, which should stop anything from accessing it
        bool found = false;
        for(auto &elem : backend->ports) {
            if(elem == port) { elem = nullptr; found = true; }
        }
        if (!found) {
            throw std::runtime_error("Did not find audio port to destroy");
        }
        // Now also close the port for the audio back-end
        port->port->close();
    });
}

void destroy_midi_port_state_info(midi_port_state_info_t *d) {
    free(d);
}

void destroy_audio_port_state_info(audio_port_state_info_t *d) {
    free(d);
}

void destroy_audio_channel(shoopdaloop_loop_audio_channel_t *d) {
    std::cerr << "Warning: destroying audio channels is unimplemented" << std::endl;
}

void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d) {
    std::cerr << "Warning: destroying midi channels is unimplemented" << std::endl;
}

void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d) {
    throw std::runtime_error("unimplemented");
}

void destroy_loop_state_info(loop_state_info_t *state) {
    free(state);
}