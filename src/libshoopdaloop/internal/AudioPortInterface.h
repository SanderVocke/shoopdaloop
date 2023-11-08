#pragma once
#include <string>
#include "PortInterface.h"

template<typename SampleT>
class AudioPortInterface : public virtual PortInterface {
public:
    AudioPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual SampleT *PROC_get_buffer(size_t n_frames, bool do_zero=false) = 0;

    AudioPortInterface() {}
    virtual ~AudioPortInterface() {}
};
