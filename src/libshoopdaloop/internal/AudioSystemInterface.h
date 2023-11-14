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
class AudioSystemInterface {
public:

    AudioSystemInterface(
        std::string client_name,
        std::function<void(uint32_t)> process_cb
    ) {}

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

    virtual void start() = 0;
    virtual uint32_t get_sample_rate() const = 0;
    virtual uint32_t get_buffer_size() const = 0;
    virtual void* maybe_client_handle() const = 0;
    virtual const char* client_name() const = 0;

    virtual void close() = 0;

    virtual uint32_t get_xruns() const = 0;
    virtual void reset_xruns() = 0;

    AudioSystemInterface() {}
    virtual ~AudioSystemInterface() {}
};