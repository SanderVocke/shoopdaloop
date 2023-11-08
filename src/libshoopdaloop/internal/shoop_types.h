#pragma once 
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
template<typename A, typename B> class ProcessingChainInterface;
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

}
