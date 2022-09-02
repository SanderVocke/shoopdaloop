#pragma once
#include <vector>
#include <stdint.h>
#include "MidiPortInterface.h"

template<typename TimeType, typename SizeType>
struct MidiMessage : public MidiSortableMessageInterface {
    TimeType time;
    SizeType size;
    std::vector<uint8_t> data;

    MidiMessage(decltype(time) time, decltype(size) size, decltype(data) data) :
        time(time), size(size), data(data) {}
    MidiMessage() {}

    uint32_t get_time() const override {
        return time;
    }
    uint32_t get_size() const override {
        return size;
    }
    const uint8_t* get_data() const override {
        return data.data();
    }
    void     get(uint32_t &size_out,
                         uint32_t &time_out,
                         const uint8_t* &data_out) const override {
        size_out = size;
        time_out = time;
        data_out = data.data();
    }
};

template<typename TimeType, typename SizeType, unsigned MaxDataSize>
struct MaxSizeMidiMessage : public MidiSortableMessageInterface {
    TimeType time;
    SizeType size;
    uint8_t data[MaxDataSize];

    MaxSizeMidiMessage(decltype(time) time, decltype(size) size, const uint8_t *data) :
        time(time),
        size(size) {
            if (size > MaxDataSize) {
                throw std::runtime_error("Attempt to store message in insufficiently sized MIDI message container.");
            }
            memcpy((void*) this->data, (void*) data, size);
        }

    uint32_t get_time() const override {
        return time;
    }
    uint32_t get_size() const override {
        return size;
    }
    const uint8_t* get_data() const override {
        return data;
    }
    void     get(uint32_t &size_out,
                         uint32_t &time_out,
                         const uint8_t* &data_out) const override {
        size_out = size;
        time_out = time;
        data_out = data;
    }
};