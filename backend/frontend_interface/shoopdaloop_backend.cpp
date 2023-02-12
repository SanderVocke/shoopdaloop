#include "shoopdaloop_backend.h"

// Internal
#include "AudioBuffer.h"
#include "AudioChannelSubloop.h"
#include "AudioMidiLoop.h"
#include "JackAudioSystem.h"
#include "MidiChannelSubloop.h"
#include "MidiPortInterface.h"
#include "ObjectPool.h"
#include "PortInterface.h"
#include "SubloopInterface.h"
#include "CommandQueue.h"
#include "types.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <stdexcept>
#include <map>

// CONSTANTS
constexpr size_t gc_n_buffers_in_pool = 100;
constexpr size_t gc_audio_buffer_size = 24000;
constexpr size_t gc_command_queue_size = 256;
constexpr size_t gc_audio_channel_initial_buffers = 100;
constexpr size_t gc_midi_storage_size = 10000;

// FORWARD DECLARATIONS
struct LoopInfo;
struct PortInfo;

// TYPE ALIASES
using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = ObjectPool<DefaultAudioBuffer>;
using AudioSystem = JackAudioSystem;
using Time = uint32_t;
using Size = uint16_t;
using Loop = AudioMidiLoop;
using SharedLoop = std::shared_ptr<Loop>;
using SharedLoopChannel = std::shared_ptr<SubloopInterface>;
using SharedLoopAudioChannel = std::shared_ptr<AudioChannelSubloop<float>>;
using SharedLoopMidiChannel = std::shared_ptr<MidiChannelSubloop<uint32_t, uint16_t>>;
using SharedPort = std::shared_ptr<PortInterface>;
using SharedPortInfo = std::shared_ptr<PortInfo>;
using SharedLoopInfo = std::shared_ptr<LoopInfo>;
using Command = std::function<void()>;

// TYPES
struct LoopInfo {
    SharedLoop loop;
    std::map<SubloopInterface, std::vector<SharedPortInfo>> input_port_mappings;
    std::map<SubloopInterface, std::vector<SharedPortInfo>> output_port_mappings;

    shoopdaloop_audio_port *c_ptr () const { return (shoopdaloop_audio_port*)this; }

    LoopInfo() : loop(std::make_shared<Loop>()) {}
};

struct PortInfo {
    SharedPort port;
    audio_sample_t *maybe_audio_buffer;
    std::shared_ptr<MidiReadableBufferInterface> maybe_midi_input_buffer;
    std::shared_ptr<MidiWriteableBufferInterface> maybe_midi_output_buffer;

    shoopdaloop_audio_port *c_audio_port () const { return (shoopdaloop_audio_port *)this; }
    shoopdaloop_midi_port  *c_midi_port  () const { return (shoopdaloop_midi_port *) this; }

    PortInfo (SharedPort const& port) : port(port) {}
};

// GLOBALS
std::unique_ptr<AudioSystem> g_audio_system;
std::shared_ptr<AudioBufferPool> g_audio_buffer_pool;
std::vector<SharedLoopInfo> g_loops;
std::vector<SharedPortInfo> g_ports;
CommandQueue g_cmd_queue (gc_command_queue_size, 1000, 1000);

// HELPER FUNCTIONS
SharedLoopInfo find_loop(shoopdaloop_loop* c_ptr) {
    auto r = std::find_if(g_loops.begin(), g_loops.end(),
        [&](auto const& e) { return (shoopdaloop_loop*)e.get() == c_ptr; });
    if (r == g_loops.end()) {
        throw std::runtime_error("Attempting to find non-existent loop.");
    }
    return *r;
}

SharedLoopAudioChannel find_audio_channel(Loop &loop,
                                          shoopdaloop_loop_audio_channel *channel) {
    for (size_t idx=0; idx < loop.n_audio_channels(); idx++) {
        SharedLoopAudioChannel chan = loop.audio_channel<float>(idx);
        if ((shoopdaloop_loop_audio_channel*)chan.get() == channel) {
            return chan;
        }
    }
    throw std::runtime_error("Attempting to find non-existent autio channel.");
}

// API FUNCTIONS

void process(size_t n_frames) {}

void initialize (const char* client_name_hint) {
    if (!g_audio_system) {
        g_audio_system = std::make_unique<JackAudioSystem>(std::string(client_name_hint), process);
    }
    if (!g_audio_buffer_pool) {
        g_audio_buffer_pool = std::make_shared<AudioBufferPool>(
            gc_n_buffers_in_pool, gc_audio_buffer_size
        );
    }
    g_cmd_queue.clear();
    g_loops.clear();
    g_ports.clear();

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

state_info_t request_state_update() {
    throw std::runtime_error("request_state_update() not implemented");
}

unsigned get_sample_rate() {
    if (!g_audio_system) {
        throw std::runtime_error("get_sample_rate() called before initialization");
    }
    return g_audio_system->get_sample_rate();
}

shoopdaloop_loop *create_loop() {
    std::cerr << "Warning: create_loop should be made thread-safe" << std::endl;
    auto r = std::make_shared<LoopInfo>();
    g_loops.push_back(r);
    return (shoopdaloop_loop*) r.get();
}

shoopdaloop_loop_audio_channel *add_audio_channel (shoopdaloop_loop *loop) {
    SharedLoopInfo loop_info = find_loop(loop);
    std::cerr << "Warning: add_audio_channel should be made thread-safe" << std::endl;
    auto r = loop_info->loop->add_audio_channel<float>(g_audio_buffer_pool,
                                                       gc_audio_channel_initial_buffers,
                                                       AudioOutputType::Copy);
    return (shoopdaloop_loop_audio_channel *)r.get();
}

shoopdaloop_loop_midi_channel  *add_midi_channel  (shoopdaloop_loop *loop) {
    SharedLoopInfo loop_info = find_loop(loop);
    std::cerr << "Warning: add_midi_channel should be made thread-safe" << std::endl;
    auto r = loop_info->loop->add_midi_channel<Time, Size>(gc_midi_storage_size);
    return (shoopdaloop_loop_midi_channel *)r.get();
}

shoopdaloop_loop_audio_channel *get_audio_channel (shoopdaloop_loop *loop, size_t idx) {
    return (shoopdaloop_loop_audio_channel *)find_loop(loop)->loop->audio_channel<float>(idx).get();
}

shoopdaloop_loop_midi_channel *get_midi_channel (shoopdaloop_loop *loop, size_t idx) {
    return (shoopdaloop_loop_midi_channel *)find_loop(loop)->loop->midi_channel<Time, Size>(idx).get();
}

unsigned get_n_audio_channels (shoopdaloop_loop *loop) {
    return find_loop(loop)->loop->n_audio_channels();
}

unsigned get_n_midi_channels (shoopdaloop_loop *loop) {
    return find_loop(loop)->loop->n_midi_channels();
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
    auto &loop_info = *find_loop(loop);
    audio_data_t a;
    a.n_channels = loop_info.loop->n_audio_channels();
    a.channels_data = (audio_channel_data_t*)malloc(sizeof(audio_channel_data_t) * a.n_channels);
    for (size_t idx=0; idx < a.n_channels; idx++) {
        a.channels_data[idx] = get_audio_channel_data(get_audio_channel(loop, idx));
    }
    return a;
}

midi_data_t get_loop_midi_data (shoopdaloop_loop *loop) {
    auto &loop_info = *find_loop(loop);
    midi_data_t a;
    a.n_channels = loop_info.loop->n_midi_channels();
    a.channels_data = (midi_channel_data_t*)malloc(sizeof(midi_channel_data_t) * a.n_channels);
    for (size_t idx=0; idx < a.n_channels; idx++) {
        a.channels_data[idx] = get_midi_channel_data(get_midi_channel(loop, idx));
    }
    return a;
}

void load_loop_audio_data(shoopdaloop_loop *loop, audio_data_t data) {
    auto &loop_info = *find_loop(loop);
    for (size_t idx=0; idx < data.n_channels; idx++) {
        audio_channel_data_t &d = data.channels_data[idx];
        load_audio_channel_data(get_audio_channel(loop, idx), d);
    }
}

void load_loop_midi_data(shoopdaloop_loop *loop, midi_data_t data) {
    auto &loop_info = *find_loop(loop);
    for (size_t idx=0; idx < data.n_channels; idx++) {
        midi_channel_data_t &d = data.channels_data[idx];
        load_midi_channel_data(get_midi_channel(loop, idx), d);
    }
}

loop_data_t get_loop_data (shoopdaloop_loop *loop) {
    auto &loop_info = *find_loop(loop);

    loop_data_t r;
    r.audio_data = get_loop_audio_data(loop);
    r.midi_data = get_loop_midi_data(loop);
    r.length = loop_info.loop->get_length();

    return r;
}

void load_loop_data (shoopdaloop_loop *loop, loop_data_t data) {
    load_loop_audio_data(loop, data.audio_data);
    load_loop_midi_data(loop, data.midi_data);
    find_loop(loop)->loop->set_length(data.length);
}

void delete_audio_channel(shoopdaloop_loop *loop, shoopdaloop_loop_audio_channel *channel) {
    auto &loop_info = *find_loop(loop);
    auto _channel = find_audio_channel(*loop_info.loop, channel);
    throw BLABi23592u3```;
}

void                  delete_midi_channel      (shoopdaloop_loop_midi_channel  *channel);
void                  connect_audio_output     (shoopdaloop_loop_audio_channel *channel, shoopdaloop_audio_port *port);
void                  connect_midi_output      (shoopdaloop_loop_midi_channel  *channel, shoopdaloop_midi_port *port);
void                  disconnect_audio_outputs (shoopdaloop_loop_audio_channel *channel);
void                  disconnect_midi_outputs  (shoopdaloop_loop_midi_channel  *channel);
audio_channel_data_t  get_audio_channel_data   (shoopdaloop_loop_audio_channel *channel);
audio_channel_data_t  get_audio_rms_data       (shoopdaloop_loop_audio_channel *channel,
                                                unsigned from_sample,
                                                unsigned to_sample,
                                                unsigned samples_per_bin);
midi_channel_data_t   get_midi_channel_data    (shoopdaloop_loop_midi_channel  *channel);
void                  load_audio_channel_data  (shoopdaloop_loop_audio_channel *channel, audio_channel_data_t data);
void                  load_midi_channel_data   (shoopdaloop_loop_midi_channel  *channel, midi_channel_data_t  data);