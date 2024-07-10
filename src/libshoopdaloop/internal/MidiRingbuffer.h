#pragma once

#include "MidiStorage.h"

class MidiRingbuffer : public MidiStorage {
public:
    using Storage = MidiStorage;

private:
    std::atomic<uint32_t> n_samples = 0;
    std::atomic<uint32_t> current_buffer_start_time = 0;
    std::atomic<uint32_t> current_buffer_end_time = 0;

public:
    MidiRingbuffer(uint32_t data_size);

    // Set N samples. Also truncates the tail such that older data is erased.
    void set_n_samples(uint32_t n);

    uint32_t get_n_samples() const;
    uint32_t get_current_start_time() const;
    uint32_t get_current_end_time() const;

    // Increment the current time. Also truncates the tail such that out-of-range data is erased.
    void next_buffer(uint32_t n_frames);

    bool put(uint32_t frame_in_current_buffer, uint16_t size,  const uint8_t* data);

    // Copy the current state of the ringbuffer to the target storage.
    // The timestamps of the messages are modified such that they are relative
    // to the current start point of the buffer.
    void snapshot(MidiStorage &target) const;
};