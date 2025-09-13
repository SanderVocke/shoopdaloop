#pragma once
#include <cstdint>
#include <functional>

template<typename SampleT> class AudioBuffer;
template<typename SampleT> class BufferPool;
template<typename A, typename B> class DummyAudioMidiDriver;
class AudioMidiLoop;
class ChannelInterface;
class PortInterface;
template<typename A> class AudioChannel;
class MidiChannel;
template<typename A> class AudioPort;
class MidiPort;
class GraphPort;
class GraphLoop;
class GraphLoopChannel;
class GraphFXChain;
class BackendSession;
class MidiReadableBufferInterface;
class MidiWriteableBufferInterface;
class MidiSortingBuffer;
class MidiStateTracker;

template<typename A, typename B> class MidiMessage;
template<typename A, typename B> class DecoupledMidiPort;
template<typename A, typename B> class ProcessingChainInterface;

#ifdef SHOOP_HAVE_LV2
template<typename A, typename B> class CarlaLV2ProcessingChain;
#endif

namespace shoop_constants {

constexpr uint32_t initial_max_loops = 512;
constexpr uint32_t initial_max_ports = 1024;
constexpr uint32_t initial_max_fx_chains = 128;
constexpr uint32_t initial_max_decoupled_midi_ports = 512;
constexpr uint32_t n_buffers_in_pool = 2048;
constexpr uint32_t audio_buffer_size = 16384;
constexpr uint32_t command_queue_size = 2048;
constexpr uint32_t audio_channel_initial_buffers = 128;
constexpr uint32_t midi_storage_size = 65536 * 8;
constexpr uint32_t default_max_port_mappings = 8;
constexpr uint32_t default_max_midi_channels = 8;
constexpr uint32_t default_max_audio_channels = 8;
constexpr uint32_t default_audio_dummy_buffer_size = 16384;

}

namespace shoop_types {

using audio_sample_t = float;

using DefaultAudioBuffer = AudioBuffer<audio_sample_t>;
using AudioBufferPool = BufferPool<audio_sample_t>;
using Time = uint32_t;
using Size = uint16_t;
using _DummyAudioMidiDriver = DummyAudioMidiDriver<uint32_t, uint32_t>;
using LoopAudioChannel = AudioChannel<audio_sample_t>;
using LoopMidiChannel = MidiChannel;
using _AudioPort = AudioPort<audio_sample_t>;
using Command = std::function<void()>;
using _MidiMessage = MidiMessage<Time, Size>;
using _DecoupledMidiPort = DecoupledMidiPort<Time, Size>;
using FXChain = ProcessingChainInterface<Time, Size>;

}