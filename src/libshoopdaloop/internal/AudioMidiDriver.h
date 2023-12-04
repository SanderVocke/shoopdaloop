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
#include <set>

enum class ProcessFunctionResult {
    Continue,  // Continue processing next cycle
    Disconnect // Request to be disconnected from the audio processing thread
};

struct AudioMidiDriverSettingsInterface {
    AudioMidiDriverSettingsInterface() {}
    virtual ~AudioMidiDriverSettingsInterface() {}
};

class HasAudioProcessingFunction {
public:
    HasAudioProcessingFunction() {}
    virtual ~HasAudioProcessingFunction() {}

    virtual void PROC_process(uint32_t nframes) = 0;
};

class AudioMidiDriver {
    std::shared_ptr<std::set<HasAudioProcessingFunction*>> m_processors;

public:
    void add_processor(HasAudioProcessingFunction &p);
    void remove_processor(HasAudioProcessingFunction &p);
    std::set<HasAudioProcessingFunction*> processors() const;
    void PROC_process(uint32_t nframes);

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
    virtual void reset_xruns() = 0;

    

    AudioMidiDriver() {}
    virtual ~AudioMidiDriver() {}
};