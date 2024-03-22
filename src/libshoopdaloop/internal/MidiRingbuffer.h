#pragma once

#include "MidiStorage.h"

template<typename TimeType, typename SizeType>
class MidiRingbuffer : public MidiStorage<TimeType, SizeType> {
    std::atomic<uint32_t> n_samples;
    std::atomic<uint32_t> head_time;
    SharedCursor head_cursor;
    SharedCursor tail_cursor;

public:
    MidiRingbuffer(uint32_t data_size);

    void set_n_samples(uint32_t n);
}