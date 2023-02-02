#include <jack/midiport.h>
#include <vector>
#include <optional>
#include <cstring>
#include <iostream>
#include "Ringbuffer.h"

template<typename TimeType, typename SizeType>
class MidiRingBuffer {
public:
    struct Metadata {
        TimeType time;
        SizeType size;
    };

private:
    struct EventInfo {
        Metadata metadata;
        uint32_t data_offset;
    };
    Ringbuffer<EventInfo> info_ringbuf;
    Ringbuffer<uint8_t> data_ringbuf;

public:
    size_t size() const {
        return info_ringbuf.size();
    }

    void at(size_t idx,
            TimeType &time_out,
            SizeType &size_out,
            uint8_t* &data_out) const {
        if (idx >= size()) { throw std::runtime_error("MIDI buffer access out of bounds"); }
        EventInfo &info = info_ringbuf.at(idx);
        time_out = info.metadata.time;
        size_out = info.metadata.size;
        // FIXME: what if data crosses ringbuf boundary?
        data_out = &data_ringbuf.at(info.data_offset);
        std::cerr << "FIXME: ringbuffer safety\n";
    }
 
    void clear() {
        info_ringbuf.clear();
        data_ringbuf.clear();
    }
};