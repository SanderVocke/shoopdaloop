#pragma once
#include <string>
#include <stdint.h>
#include "PortInterface.h"
#include "types.h"

template<typename SampleT>
class AudioPortInterface : public virtual PortInterface {
public:
    AudioPortInterface(
        std::string name,
        shoop_port_direction_t direction
    ) : PortInterface() {}

    virtual SampleT *PROC_get_buffer(uint32_t n_frames, bool do_zero=false) = 0;

    AudioPortInterface() {}
    virtual ~AudioPortInterface() {}
};
