#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "PortInterface.h"
#include "MidiPortInterface.h"
#include <string>
#include "AudioPortInterface.h"
#include <functional>
#include <stdint.h>
#include "types.h"

struct AudioMidiDriverSettingsInterface {
    AudioMidiDriverSettingsInterface() {}
    virtual ~AudioMidiDriverSettingsInterface() {}
};

class AudioMidiDriverInterface {
public:

    AudioMidiDriverInterface() {}

    virtual void start(AudioMidiDriverSettingsInterface &settings,
                       std::function<void(uint32_t)> process_cb) = 0;
    virtual bool started() const = 0;

    virtual
    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        PortDirection direction
    ) = 0;

    virtual
    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) = 0;

    virtual shoop_audio_driver_state_t get_state() const;

    virtual void close() = 0;

    virtual uint32_t get_xruns() const = 0;
    virtual void reset_xruns() = 0;

    AudioMidiDriverInterface() {}
    virtual ~AudioMidiDriverInterface() {}
};