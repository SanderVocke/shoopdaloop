#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"

template<typename TimeType, typename SizeType>
class _MidiPortInterface : public PortInterface {
public:
    _MidiPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual void* get_buffer(size_t n_frames) = 0;
    virtual size_t get_n_events(void* buffer) const = 0;
    virtual void get_event(void* buffer,
                           size_t idx,
                           SizeType &size_out,
                           TimeType &time_out,
                           uint8_t* &data_out) const = 0;

    _MidiPortInterface() {}
    virtual ~_MidiPortInterface() {}
};

typedef _MidiPortInterface<uint32_t, uint32_t> MidiPortInterface;