#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "types.h"
#include <string>
#include "AudioPortInterface.h"

class AudioSystemInterface {
public:

    AudioSystemInterface(
        std::string client_name
    ) {}

    virtual
    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) = 0;

    // Create a MIDI port.
    //
    // decoupled: specifies whether the port should be decoupled
    //   from real-time processing by having intermediate storage.
    //   Such ports are suitable for reading out only once in a while,
    //   at the cost of adding latency.
    typedef int MidiPortInterface;
    virtual
    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction,
        bool decoupled
    );

    virtual void start() = 0;

    virtual ~AudioSystemInterface() {}
};