#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "MidiPortInterface.h"
#include <string>
#include <map>
#include "AudioPortInterface.h"
#include <functional>

template<typename TimeType, typename SizeType>
class ProcessingChainInterface {
public:
    using SharedAudioPort = std::shared_ptr<AudioPortInterface<float>>;
    using SharedMidiPort = std::shared_ptr<MidiPortInterface>;

    ProcessingChainInterface() {}
    virtual ~ProcessingChainInterface() {}

    virtual
    std::vector<SharedAudioPort> input_audio_ports() const = 0;

    virtual
    std::vector<SharedAudioPort> output_audio_ports() const = 0;

    virtual
    std::vector<SharedMidiPort> input_midi_ports() const = 0;

    virtual
    std::vector<SharedMidiPort> output_midi_ports() const = 0;

    virtual bool is_freewheeling() const = 0;
    virtual void set_freewheeling(bool enabled) = 0;

    virtual void process(size_t frames) = 0;
};