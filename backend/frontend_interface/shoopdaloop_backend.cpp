#include "shoopdaloop_backend.h"

// Internal
#include "AudioBuffer.h"
#include "AudioChannel.h"
#include "AudioMidiLoop.h"
#include "AudioPortInterface.h"
#include "AudioSystemInterface.h"
#include "CarlaLV2ProcessingChain.h"
#include "InternalAudioPort.h"
#include "InternalLV2MidiOutputPort.h"
#include "JackAudioPort.h"
#include "JackAudioSystem.h"
#include "DummyAudioSystem.h"
#include "JackMidiPort.h"
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
#include "MidiNotesState.h"
#include "process_loops.h"
#include "types.h"
#include "LV2.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <cmath>
#include <jack/types.h>
#include <math.h>
#include <memory>
#include <stdexcept>
#include <map>
#include <set>
#include <thread>

// CONSTANTS
namespace {
constexpr size_t gc_initial_max_loops = 512;
constexpr size_t gc_initial_max_ports = 1024;
constexpr size_t gc_initial_max_fx_chains = 128;
constexpr size_t gc_initial_max_decoupled_midi_ports = 512;
constexpr size_t gc_n_buffers_in_pool = 128;
constexpr size_t gc_audio_buffer_size = 32768;
constexpr size_t gc_command_queue_size = 2048;
constexpr size_t gc_audio_channel_initial_buffers = 128;
constexpr size_t gc_midi_storage_size = 65536;
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
struct FXChainInfo;
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
using SharedFXChainInfo = std::shared_ptr<FXChainInfo>;
using SharedChannelInfo = std::shared_ptr<ChannelInfo>;
using Command = std::function<void()>;
using _MidiMessage = MidiMessage<Time, Size>;
using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;
using SharedDecoupledMidiPort = std::shared_ptr<_DecoupledMidiPort>;
using SharedDecoupledMidiPortInfo = std::shared_ptr<DecoupledMidiPortInfo>;
using SharedBackend = std::shared_ptr<Backend>;
using WeakBackend = std::weak_ptr<Backend>;
using SharedFXChain = std::shared_ptr<CarlaLV2ProcessingChain<Time, Size>>;

// GLOBALS
namespace {
std::vector<audio_sample_t> g_dummy_audio_input_buffer (gc_default_audio_dummy_buffer_size);
std::vector<audio_sample_t> g_dummy_audio_output_buffer (gc_default_audio_dummy_buffer_size);
std::shared_ptr<DummyReadMidiBuf> g_dummy_midi_input_buffer = std::make_shared<DummyReadMidiBuf>();
std::shared_ptr<DummyWriteMidiBuf> g_dummy_midi_output_buffer = std::make_shared<DummyWriteMidiBuf>();
std::set<SharedBackend> g_active_backends;
LV2 g_lv2;
}

// TYPES
enum class ProcessWhen {
    BeforeFXChains, // Process before FX chains have processed.
    AfterFXChains   // Process only after FX chains have processed.
};

struct Backend : public std::enable_shared_from_this<Backend> {

    std::vector<SharedLoopInfo> loops;
    std::vector<SharedPortInfo> ports;
    std::vector<SharedFXChainInfo> fx_chains;
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
        fx_chains.reserve(gc_initial_max_fx_chains);
        decoupled_midi_ports.reserve(gc_initial_max_decoupled_midi_ports);
        audio_system->start();
    }

    void PROC_process (jack_nframes_t nframes);
    void PROC_process_decoupled_midi_ports(size_t nframes);
    void terminate();
    jack_client_t *maybe_jack_client_handle();
    const char* get_client_name();
    unsigned get_sample_rate();
    unsigned get_buffer_size();
    SharedLoopInfo create_loop();
    SharedFXChainInfo create_fx_chain(fx_chain_type_t type, const char* title);
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
    MidiReadableBufferInterface *maybe_midi_input_buffer;
    MidiWriteableBufferInterface *maybe_midi_output_buffer;
    std::shared_ptr<MidiMergingBuffer> maybe_midi_output_merging_buffer;
    std::atomic<size_t> n_events_processed;
    std::unique_ptr<MidiNotesState> maybe_midi_notes_state;

    // Both
    std::atomic<bool> muted;
    std::atomic<bool> passthrough_muted;
    ProcessWhen ma_process_when;

    PortInfo (SharedPort const& port, SharedBackend const& backend) : port(port),
        maybe_audio_buffer(nullptr),
        maybe_midi_input_buffer(nullptr),
        maybe_midi_output_buffer(nullptr),
        maybe_midi_notes_state(nullptr),
        volume(1.0f),
        passthrough_volume(1.0f),
        muted(false),
        passthrough_muted(false),
        backend(backend),
        peak(0.0f),
        n_events_processed(0) {

        bool is_internal = (dynamic_cast<InternalAudioPort<float>*>(port.get()) ||
                            dynamic_cast<InternalLV2MidiOutputPort*>(port.get()));
        bool is_fx_in = is_internal && (port->direction() == PortDirection::Output);
        bool is_ext_in = !is_internal && (port->direction() == PortDirection::Input);

        ma_process_when = (is_ext_in || is_fx_in) ?
            ProcessWhen::BeforeFXChains : ProcessWhen::AfterFXChains;

        if (auto m = dynamic_cast<MidiPort*>(port.get())) {
            maybe_midi_notes_state = std::make_unique<MidiNotesState>();
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

    AudioPort *maybe_audio();
    MidiPort *maybe_midi();
    Backend &get_backend();
};

struct ChannelInfo : public std::enable_shared_from_this<ChannelInfo> {
    SharedLoopChannel channel;
    WeakLoopInfo loop;
    WeakPortInfo mp_input_port_mapping;
    WeakBackend backend;
    std::vector<WeakPortInfo> mp_output_port_mappings;
    ProcessWhen ma_process_when;

    ChannelInfo(SharedLoopChannel chan,
                SharedLoopInfo loop,
                SharedBackend backend) :
        channel(chan),
        loop(loop),
        backend(backend),
        ma_process_when(ProcessWhen::BeforeFXChains) {
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
    void disconnect_input_ports(bool thread_safe=true);
    void PROC_prepare_process_audio(size_t n_frames);
    void PROC_prepare_process_midi(size_t n_frames);
    void PROC_finalize_process_audio();
    void PROC_finalize_process_midi();
    LoopAudioChannel *maybe_audio();
    LoopMidiChannel *maybe_midi();
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

struct FXChainInfo : public std::enable_shared_from_this<FXChainInfo> {
    const SharedFXChain chain;
    WeakBackend backend;

    std::vector<SharedPortInfo> mc_audio_input_ports;
    std::vector<SharedPortInfo> mc_audio_output_ports;
    std::vector<SharedPortInfo> mc_midi_input_ports;

    FXChainInfo(SharedFXChain chain, SharedBackend backend) :
        chain(chain), backend(backend) {
        for (auto const& port : chain->input_audio_ports()) {
            mc_audio_input_ports.push_back(std::make_shared<PortInfo>(port, backend));
        }
        for (auto const& port : chain->output_audio_ports()) {
            mc_audio_output_ports.push_back(std::make_shared<PortInfo>(port, backend));
        }
        for (auto const& port : chain->input_midi_ports()) {
            mc_midi_input_ports.push_back(std::make_shared<PortInfo>(port, backend));
        }
    }

    Backend &get_backend();
    decltype(mc_audio_input_ports) const& audio_input_ports() const { return mc_audio_input_ports; }
    decltype(mc_audio_output_ports) const& audio_output_ports() const { return mc_audio_output_ports; }
    decltype(mc_midi_input_ports) const& midi_input_ports() const { return mc_midi_input_ports; }
};

#warning delete destroyed ports
// MEMBER FUNCTIONS
void Backend::PROC_process (jack_nframes_t nframes) {
    // Execute queued commands
    cmd_queue.PROC_exec_all();

    // Send/receive decoupled midi
    PROC_process_decoupled_midi_ports(nframes);

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

    // Process passthrough on the input side (before FX chains)
    auto before_fx_port_passthrough_fn = [&](auto &p) {
        if (p && p->ma_process_when == ProcessWhen::BeforeFXChains) { p->PROC_passthrough(nframes); }
    };
    for (auto & port: ports) { before_fx_port_passthrough_fn(port); }
    for (auto & chain : fx_chains) {
        for (auto & p : chain->mc_audio_input_ports) { before_fx_port_passthrough_fn(p); }
        for (auto & p : chain->mc_midi_input_ports)  { before_fx_port_passthrough_fn(p); }
    }
    
    // Process the loops. This queues deferred audio operations for post-FX channels.
    process_loops<LoopInfo>
        (loops, [](LoopInfo &l) { return (LoopInterface*)l.loop.get(); }, nframes);
    
    // Finish processing any channels that come before FX.
    auto before_fx_channel_process = [&](auto &c) {
        if (c->ma_process_when == ProcessWhen::BeforeFXChains) { c->channel->PROC_finalize_process(); }
    };
    for (auto & loop: loops) {
        for (auto & channel: loop->mp_audio_channels) { before_fx_channel_process(channel); }
        for (auto & channel: loop->mp_midi_channels)  { before_fx_channel_process(channel); }
    }

    // Process the FX chains.
    for (auto & chain: fx_chains) {
        chain->chain->process(nframes);
    }

    // Process passthrough on the output side (after FX chains)
    auto after_fx_port_passthrough_fn = [&](auto &p) {
        if (p && p->ma_process_when == ProcessWhen::AfterFXChains) { p->PROC_passthrough(nframes); }
    };
    for (auto & port: ports) { after_fx_port_passthrough_fn(port); }
    for (auto & chain : fx_chains) {
        for (auto & p : chain->mc_audio_output_ports) { after_fx_port_passthrough_fn(p); }
    }

    // Finish processing any channels that come after FX.
    auto after_fx_channel_process = [&](auto &c) {
        if (c->ma_process_when == ProcessWhen::AfterFXChains) { c->channel->PROC_finalize_process(); }
    };
    for (auto & loop: loops) {
        for (auto & channel: loop->mp_audio_channels) { after_fx_channel_process(channel); }
        for (auto & channel: loop->mp_midi_channels)  { after_fx_channel_process(channel); }
    }

    // Prepare state for next round.
    for (auto &port : ports) {
        if (port) {
            port->PROC_finalize_process(nframes);
        }
    }
    for (auto &loop : loops) {
        if (loop) {
            loop->PROC_finalize_process();
        }
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

unsigned Backend::get_buffer_size() {
    if (!audio_system) {
        throw std::runtime_error("get_buffer_size() called before initialization");
    }
    return audio_system->get_buffer_size();
}

SharedLoopInfo Backend::create_loop() {
    auto r = std::make_shared<LoopInfo>(shared_from_this());
    cmd_queue.queue([this, r]() {
        loops.push_back(r);
    });
    return r;
}

SharedFXChainInfo Backend::create_fx_chain(fx_chain_type_t type, const char* title) {
    auto chain = g_lv2.create_carla_chain<Time, Size>(
        type, get_sample_rate(), std::string(title)
    );
    chain->ensure_buffers(get_buffer_size());
    auto info = std::make_shared<FXChainInfo>(chain, shared_from_this());
    cmd_queue.queue([this, info]() {
        fx_chains.push_back(info);
    });
    return info;
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
            maybe_midi_input_buffer = &maybe_midi->PROC_get_read_buffer(n_frames);
            if (!muted) {
                n_events_processed += maybe_midi_input_buffer->PROC_get_n_events();
                for(size_t i=0; i<maybe_midi_input_buffer->PROC_get_n_events(); i++) {
                    auto &msg = maybe_midi_input_buffer->PROC_get_event_reference(i);
                    uint32_t size, time;
                    const uint8_t *data;
                    msg.get(size, time, data);
                    maybe_midi_notes_state->process_msg(data);
                }
            }
        } else {
            maybe_midi_output_merging_buffer->PROC_clear();
            if (maybe_midi_output_buffer) { return; } // already there
            maybe_midi_output_buffer = &maybe_midi->PROC_get_write_buffer(n_frames);
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
            auto &msg = maybe_midi_input_buffer->PROC_get_event_reference(i);
            to.maybe_midi_output_merging_buffer->PROC_write_event_reference(msg);
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
                    maybe_midi_notes_state->process_msg(data);
                }
                n_events_processed += n_events;
            }
        }
    }
}

AudioPort *PortInfo::maybe_audio() {
    return dynamic_cast<AudioPort*>(port.get());
}

MidiPort *PortInfo::maybe_midi() {
    return dynamic_cast<MidiPort*>(port.get());
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
        ma_process_when = port->ma_process_when; // Process in same phase as the connected port
    };
    if (thread_safe) { get_backend().cmd_queue.queue(fn); }
    else { fn(); }
}

void ChannelInfo::connect_input_port(SharedPortInfo port, bool thread_safe) {
    auto fn = [this, port]() {
        mp_input_port_mapping = port;
        ma_process_when = port->ma_process_when; // Process in same phase as the connected port
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

void ChannelInfo::disconnect_input_ports(bool thread_safe) {
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
        chan->PROC_set_recording_buffer(in_locked->maybe_midi_input_buffer, n_frames);
    } else {
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        chan->PROC_set_recording_buffer(g_dummy_midi_input_buffer.get(), n_frames);
    }
    size_t n_outputs_mapped = 0;
    for (auto &port : mp_output_port_mappings) {
        auto locked = port.lock();
        auto chan = dynamic_cast<LoopMidiChannel*>(channel.get());
        if (locked) {
            chan->PROC_set_playback_buffer(locked->maybe_midi_output_merging_buffer.get(), n_frames);
            n_outputs_mapped++;
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

LoopAudioChannel *ChannelInfo::maybe_audio() {
    return dynamic_cast<LoopAudioChannel*>(channel.get());
}

LoopMidiChannel *ChannelInfo::maybe_midi() {
    return dynamic_cast<LoopMidiChannel*>(channel.get());
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

Backend &FXChainInfo::get_backend() {
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

SharedFXChainInfo internal_fx_chain(shoopdaloop_fx_chain_t *chain) {
    return ((FXChainInfo*)chain)->shared_from_this();
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

shoopdaloop_fx_chain_t* external_fx_chain(SharedFXChainInfo chain) {
    return (shoopdaloop_fx_chain_t*)chain.get();
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
    auto rval = external_loop(internal_backend(backend)->create_loop());
    return rval;
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

void disconnect_audio_input (shoopdaloop_loop_audio_channel_t *channel, shoopdaloop_audio_port_t* port) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_audio_port(port);
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_input_port(_port, false);
    });
}

void disconnect_midi_input (shoopdaloop_loop_midi_channel_t  *channel, shoopdaloop_midi_port_t* port) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _port = internal_midi_port(port);
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_input_port(_port, false);
    });
}

void disconnect_audio_inputs (shoopdaloop_loop_audio_channel_t *channel) {
    internal_audio_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->disconnect_input_ports(false);
    });
}

void disconnect_midi_inputs  (shoopdaloop_loop_midi_channel_t  *channel) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->disconnect_input_ports(false);
    });
}

audio_channel_data_t *get_audio_channel_data (shoopdaloop_loop_audio_channel_t *channel) {
    auto &chan = *internal_audio_channel(channel);
    return evaluate_before_or_after_process<audio_channel_data_t*>(
        [&chan]() { return external_audio_data(chan.maybe_audio()->get_data()); },
        chan.maybe_audio(),
        chan.backend.lock()->cmd_queue);
}

midi_channel_data_t *get_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel) {
    auto &chan = *internal_midi_channel(channel);
    auto data = evaluate_before_or_after_process<std::vector<_MidiMessage>>(
        [&chan]() { return chan.maybe_midi()->retrieve_contents(); },
        chan.maybe_midi(),
        chan.backend.lock()->cmd_queue);

    return external_midi_data(data);
}

void load_audio_channel_data  (shoopdaloop_loop_audio_channel_t *channel, audio_channel_data_t *data) {
    auto &chan = *internal_audio_channel(channel);
    evaluate_before_or_after_process<void>(
        [&chan, &data]() { chan.maybe_audio()->load_data(data->data, data->n_samples); },
        chan.maybe_audio(),
        chan.backend.lock()->cmd_queue);
}

void load_midi_channel_data (shoopdaloop_loop_midi_channel_t  *channel, midi_channel_data_t  *data) {
    auto &chan = *internal_midi_channel(channel);
    auto _data = internal_midi_data(*data);
    evaluate_before_or_after_process<void>(
        [&]() { chan.maybe_midi()->set_contents(_data, data->length_samples); },
        chan.maybe_midi(),
        chan.backend.lock()->cmd_queue);
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
    auto internal_loops = std::make_shared<std::vector<SharedLoopInfo>>(n_loops);
    for(size_t idx=0; idx<n_loops; idx++) {
        (*internal_loops)[idx] = internal_loop(loops[idx]);
    }
    internal_loop(loops[0])->get_backend().cmd_queue.queue([=]() {
        for (size_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *(*internal_loops)[idx];
            loop_info.loop->plan_transition(mode, delay, wait_for_sync, false);
        }
        // Ensure that sync is fully propagated
        for (size_t idx=0; idx<n_loops; idx++) {
            auto &loop_info = *(*internal_loops)[idx];
            loop_info.loop->PROC_handle_sync();
        }
    });
}

void clear_loop (shoopdaloop_loop_t *loop, size_t length) {
    internal_loop(loop)->get_backend().cmd_queue.queue([=]() {
        auto &_loop = *internal_loop(loop);
        for (auto &chan : _loop.mp_audio_channels) {
            chan->maybe_audio()->PROC_clear(length);
        }
        for (auto &chan : _loop.mp_midi_channels) {
            chan->maybe_midi()->clear(false);
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
        internal_audio_channel(channel)->maybe_audio()->PROC_clear(length);
    });
}

void clear_midi_channel (shoopdaloop_loop_midi_channel_t *channel) {
    internal_midi_channel(channel)->get_backend().cmd_queue.queue([=]() {
        internal_midi_channel(channel)->maybe_midi()->clear(false);
    });
}

shoopdaloop_audio_port_t *open_jack_audio_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
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
    return evaluate_before_or_after_process<jack_port_t*>(
        [pi]() { return dynamic_cast<JackAudioPort*>(pi->maybe_audio())->get_jack_port(); },
        pi->maybe_audio(),
        pi->backend.lock()->cmd_queue);
}

shoopdaloop_midi_port_t *open_jack_midi_port (shoopdaloop_backend_instance_t *backend, const char* name_hint, port_direction_t direction) {
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
    return evaluate_before_or_after_process<jack_port_t*>(
        [pi]() { return dynamic_cast<JackMidiPort*>(pi->maybe_midi())->get_jack_port(); },
        pi->maybe_midi(),
        pi->backend.lock()->cmd_queue);
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
    auto &_channel = *dynamic_cast<LoopAudioChannel*>(internal_audio_channel(channel)->channel.get());
    _channel.set_volume(volume);
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

void set_audio_channel_start_offset (shoopdaloop_loop_audio_channel_t * channel, int start_offset) {
    internal_audio_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_audio_channel(channel);
        _channel->channel->set_start_offset(start_offset);
    });
}

void set_midi_channel_start_offset (shoopdaloop_loop_midi_channel_t * channel, int start_offset) {
    internal_midi_channel(channel)->backend.lock()->cmd_queue.queue([=]() {
        auto _channel = internal_midi_channel(channel);
        _channel->channel->set_start_offset(start_offset);
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
    r->n_notes_active = p->maybe_midi_notes_state->n_notes_active();
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

shoopdaloop_fx_chain_t *create_fx_chain(shoopdaloop_backend_instance_t *backend, fx_chain_type_t type, const char* title) {
    return external_fx_chain(internal_backend(backend)->create_fx_chain(type, title));
}

void fx_chain_set_ui_visible(shoopdaloop_fx_chain_t *chain, unsigned visible) {
    if(visible) {
        internal_fx_chain(chain)->chain->show();
    } else {
        internal_fx_chain(chain)->chain->hide();
    }
}

fx_chain_state_info_t *get_fx_chain_state(shoopdaloop_fx_chain_t *chain) {
    auto r = new fx_chain_state_info_t;
    auto c = internal_fx_chain(chain);
    r->ready = (unsigned) c->chain->is_ready();
    r->active = (unsigned) c->chain->is_active();
    r->visible = (unsigned) c->chain->visible();
    return r;
}

void set_fx_chain_active(shoopdaloop_fx_chain_t *chain, unsigned active) {
    internal_fx_chain(chain)->chain->set_active(active);
}

const char* get_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain) {
    auto c = internal_fx_chain(chain);
    auto str = c->chain->get_state();
    char * rval = (char*) malloc(str.size() + 1);
    memcpy((void*)rval, str.data(), str.size());
    rval[str.size()] = 0;
    return rval;
}

void restore_fx_chain_internal_state(shoopdaloop_fx_chain_t *chain, const char* state) {
    auto c = internal_fx_chain(chain);
    c->chain->restore_state(std::string(state));
}

shoopdaloop_audio_port_t **fx_chain_audio_input_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->audio_input_ports();
    shoopdaloop_audio_port_t **rval = (shoopdaloop_audio_port_t**) malloc(sizeof(shoopdaloop_audio_port_t*) * ports.size());
    for (size_t i=0; i<ports.size(); i++) {
        rval[i] = external_audio_port(ports[i]);
    }
    *n_out = ports.size();
    return rval;
}

shoopdaloop_audio_port_t **fx_chain_audio_output_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->audio_output_ports();
    shoopdaloop_audio_port_t **rval = (shoopdaloop_audio_port_t**) malloc(sizeof(shoopdaloop_audio_port_t*) * ports.size());
    for (size_t i=0; i<ports.size(); i++) {
        rval[i] = external_audio_port(ports[i]);
    }
    *n_out = ports.size();
    return rval;
}

shoopdaloop_midi_port_t **fx_chain_midi_input_ports(shoopdaloop_fx_chain_t *chain, unsigned int *n_out) {
    auto _chain = internal_fx_chain(chain);
    auto const& ports = _chain->midi_input_ports();
    shoopdaloop_midi_port_t **rval = (shoopdaloop_midi_port_t**) malloc(sizeof(shoopdaloop_midi_port_t*) * ports.size());
    for (size_t i=0; i<ports.size(); i++) {
        rval[i] = external_midi_port(ports[i]);
    }
    *n_out = ports.size();
    return rval;
}

void destroy_midi_event(midi_event_t *e) {
    free(e->data);
    delete e;
}

void destroy_midi_channel_data(midi_channel_data_t *d) {
    for(size_t idx=0; idx<d->n_events; idx++) {
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
    std::cerr << "Warning: destroying loops is unimplemented" << std::endl;
}

void destroy_audio_port(shoopdaloop_audio_port_t *d) {
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
    std::cerr << "Warning: destroying audio channels is unimplemented" << std::endl;
}

void destroy_midi_channel(shoopdaloop_loop_midi_channel_t *d) {
    std::cerr << "Warning: destroying midi channels is unimplemented" << std::endl;
}

void destroy_shoopdaloop_decoupled_midi_port(shoopdaloop_decoupled_midi_port_t *d) {
    throw std::runtime_error("unimplemented");
}

void destroy_loop_state_info(loop_state_info_t *state) {
    delete state;
}

void destroy_fx_chain(shoopdaloop_fx_chain_t *chain) {
    std::cerr << "Warning: destroying FX chains is unimplemented. Stopping only." << std::endl;
    internal_fx_chain(chain)->chain->stop();
}

void destroy_fx_chain_state(fx_chain_state_info_t *d) {
    delete d;
}

void destroy_string(const char* s) {
    free((void*)s);
}