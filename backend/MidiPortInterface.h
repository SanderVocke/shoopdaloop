#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"

template<typename TimeType, typename SizeType>
class MidiPortInterface : public PortInterface {
public:
    MidiPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual void* get_buffer(size_t n_frames) = 0;
    virtual size_t get_n_events(void* buffer) const = 0;
    virtual void get_event(void* buffer,
                           size_t idx,
                           SizeType &size_out,
                           TimeType &time_out,
                           uint8_t* &data_out) const;

    MidiPortInterface() {}
    virtual ~MidiPortInterface() {}
};