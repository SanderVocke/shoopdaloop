#pragma once 
#include "CustomProcessingChain.h"
#include <cstddef>
#include <cstdint>
#include <memory>
#include <functional>

template<typename SampleT> class AudioBuffer;
template<typename Obj> class ObjectPool;
template<typename A, typename B> class AudioSystemInterface;
template<typename A, typename B> class DummyAudioSystem;
class AudioMidiLoop;
class ChannelInterface;
class PortInterface;
template<typename A> class AudioChannel;
template<typename A, typename B> class MidiChannel;
template<typename A> class AudioPortInterface;
class MidiPortInterface;
class ConnectedPort;
class ConnectedLoop;
class ConnectedChannel;
class ConnectedFXChain;
class ConnectedDecoupledMidiPort;

template<typename A, typename B> class MidiMessage;
template<typename A, typename B> class DecoupledMidiPort;

#ifdef SHOOP_HAVE_LV2
template<typename A, typename B> class CarlaLV2ProcessingChain;
#endif

namespace shoop_constants {

constexpr size_t initial_max_loops = 512;
constexpr size_t initial_max_ports = 1024;
constexpr size_t initial_max_fx_chains = 128;
constexpr size_t initial_max_decoupled_midi_ports = 512;
constexpr size_t n_buffers_in_pool = 128;
constexpr size_t audio_buffer_size = 32768;
constexpr size_t command_queue_size = 2048;
constexpr size_t audio_channel_initial_buffers = 128;
constexpr size_t midi_storage_size = 65536;
constexpr size_t default_max_port_mappings = 8;
constexpr size_t default_max_midi_channels = 8;
constexpr size_t default_max_audio_channels = 8;
constexpr size_t decoupled_midi_port_queue_size = 256;
constexpr size_t default_audio_dummy_buffer_size = 16384;

}

namespace shoop_types {

using audio_sample_t = float;

using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = ObjectPool<DefaultAudioBuffer>;
using Time = uint32_t;
using Size = uint16_t;
using AudioSystem = AudioSystemInterface<size_t, size_t>;
using _DummyAudioSystem = DummyAudioSystem<size_t, size_t>;
using LoopAudioChannel = AudioChannel<audio_sample_t>;
using LoopMidiChannel = MidiChannel<uint32_t, uint16_t>;
using AudioPort = AudioPortInterface<audio_sample_t>;
using MidiPort = MidiPortInterface;
using Command = std::function<void()>;
using _MidiMessage = MidiMessage<Time, Size>;
using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;
using FXChain = ProcessingChainInterface<Time, Size>;

enum class ProcessWhen {
    BeforeFXChains, // Process before FX chains have processed.
    AfterFXChains   // Process only after FX chains have processed.
};

}