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

    template<typename SampleT>
    std::shared_ptr<AudioPortInterface<SampleT>> open_audio_port(
        std::string name,
        PortDirection direction
    );

    AudioSystemInterface() = 0;
    virtual ~AudioPortInterface() {}
};