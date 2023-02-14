#pragma once
#include <vector>
#include <stdint.h>

template<typename TimeType, typename SizeType>
struct MidiMessage {
    TimeType time;
    SizeType size;
    std::vector<uint8_t> data;
};