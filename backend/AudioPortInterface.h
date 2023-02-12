#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "types.h"
#include <string>
#include "PortInterface.h"

template<typename SampleT>
class AudioPortInterface : public PortInterface {
public:
    AudioPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual SampleT *PROC_get_buffer(size_t n_frames) = 0;

    AudioPortInterface() {}
    virtual ~AudioPortInterface() {}
};