#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "MidiPortInterface.h"
#include <string>
#include <map>
#include "AudioPortInterface.h"
#include "InternalAudioPort.h"
#include <functional>

template<typename TimeType, typename SizeType>
class ProcessingChainInterface {
public:
    using _InternalAudioPort = InternalAudioPort<float>;
    using SharedInternalAudioPort = std::shared_ptr<InternalAudioPort<float>>;
    using MidiPort = MidiPortInterface;
    using SharedMidiPort = std::shared_ptr<MidiPort>;

    ProcessingChainInterface() {}
    virtual ~ProcessingChainInterface() {}

    virtual
    std::vector<SharedInternalAudioPort> const& input_audio_ports() const = 0;

    virtual
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const = 0;

    virtual
    std::vector<SharedMidiPort> const& input_midi_ports() const = 0;

    // TODO: support MIDI output.

    virtual bool is_freewheeling() const = 0;
    virtual void set_freewheeling(bool enabled) = 0;

    virtual void process(size_t frames) = 0;
};