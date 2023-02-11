#include "shoopdaloop_backend.h"

// Internal
#include "AudioBuffer.h"
#include "AudioLoop.h"
#include "AudioPortInterface.h"
#include "JackAudioSystem.h"
#include "LoopInterface.h"
#include "MidiLoop.h"
#include "MidiPortInterface.h"
#include "ObjectPool.h"
#include "PortInterface.h"
#include "types.h"

// System
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <stdexcept>

// CONSTANTS
constexpr size_t gc_n_buffers_in_pool = 100;
constexpr size_t gc_audio_buffer_size = 24000;
constexpr size_t gc_command_queue_size = 256;

// FORWARD DECLARATIONS
struct LoopInfo;
struct PortInfo;

// TYPE ALIASES
using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = ObjectPool<DefaultAudioBuffer>;
using AudioSystem = JackAudioSystem;
using Time = uint32_t;
using Size = uint16_t;
using SharedLoop = std::shared_ptr<LoopInterface>;
using SharedPort = std::shared_ptr<PortInterface>;
using SharedPortInfo = std::shared_ptr<PortInfo>;
using SharedPortInfos = std::vector<SharedPortInfo>;
using SharedLoopInfo = std::shared_ptr<LoopInfo>;
using Command = std::function<void()>;
using CommandQueue = boost::lockfree::spsc_queue<Command>;

// TYPES
struct LoopInfo {
    SharedLoop loop;
    std::vector<SharedPortInfos> output_audio_ports;
    std::vector<SharedPortInfos> input_audio_ports;
    std::vector<SharedPortInfos> output_midi_ports;
    std::vector<SharedPortInfos> input_midi_ports;

    shoopdaloop_audio_port *c_ptr () const { return (shoopdaloop_audio_port*)this; }

    LoopInfo(SharedLoop const& l) : loop(loop) {}
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
CommandQueue g_cmd_queue (gc_command_queue_size);

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
    g_cmd_queue.reset();
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
}