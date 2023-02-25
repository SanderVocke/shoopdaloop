#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "PortInterface.h"
#include "MidiPortInterface.h"
#include <string>
#include <map>
#include "AudioPortInterface.h"
#include <functional>

template<typename TimeType, typename SizeType>
class AudioSystemInterface {
public:

    AudioSystemInterface(
        std::string client_name,
        std::function<void(size_t)> process_cb
    ) {}

    virtual
    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) = 0;

    virtual
    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) = 0;

    virtual void start() = 0;

    virtual size_t get_sample_rate() const = 0;

    AudioSystemInterface() {}
    virtual ~AudioSystemInterface() {}
};