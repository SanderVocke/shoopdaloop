#pragma once

#include "MidiStorage.h"

template<typename TimeType, typename SizeType>
class MidiRingbuffer : public MidiStorage<TimeType, SizeType> {
public:
    using Storage = MidiStorage<TimeType, SizeType>;

private:
    std::atomic<uint32_t> n_samples = 0;
    std::atomic<TimeType> current_buffer_start_time = 0;
    std::atomic<TimeType> current_buffer_end_time = 0;

public:
    MidiRingbuffer(uint32_t data_size);

    // Set N samples. Also truncates the tail such that older data is erased.
    void set_n_samples(uint32_t n);

    // Increment the current time. Also truncates the tail such that out-of-range data is erased.
    void next_buffer(TimeType n_frames);

    bool put(TimeType frame_in_current_buffer, SizeType size,  const uint8_t* data);

    // Copy the current state of the ringbuffer to the target storage.
    // Returns the current head time as context for the timestamps in the
    // storage.
    TimeType snapshot(Storage &target) const;
};

extern template class MidiRingbuffer<uint32_t, uint16_t>;
extern template class MidiRingbuffer<uint32_t, uint32_t>;
extern template class MidiRingbuffer<uint16_t, uint16_t>;
extern template class MidiRingbuffer<uint16_t, uint32_t>;