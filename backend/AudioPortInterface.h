#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "types.h"
#include <string>

enum class PortDirection {
    Input,
    Output
};

template<typename SampleT>
class AudioPortInterface {
public:
    AudioPortInterface(
        std::string name,
        PortDirection direction
    ) {}

    virtual SampleT *get_buffer(size_t n_frames) = 0;
    virtual void close() = 0;

    AudioPortInterface() = 0;
    virtual ~AudioPortInterface() {}
};