#pragma once
#include <cstdint>
#include <cstring>
#include <functional>

/**
 * MidiStorageElem - MIDI message stored in the ringbuffer.
 * 
 * This struct represents a single MIDI message with an inline
 * 4-byte payload. The time field represents the position in the
 * ringbuffer (absolute in storage, relative in output buffers).
 */
struct MidiStorageElem {
    uint32_t time = 0;        // Position: absolute in storage, relative in buffers
    uint16_t size = 0;        // Size of the data portion (1..4)
    uint8_t bytes[4] = {0};   // Inline 4-byte payload

    uint8_t* data();
    const uint8_t* data() const;
    // Alias for data() for backward compatibility
    const uint8_t* get_data() const { return bytes; }
    uint32_t get_size() const;
    void get(uint32_t &size_out,
                uint32_t &time_out,
                const uint8_t* &data_out) const;

    bool operator==(MidiStorageElem const& other) const {
        return time == other.time &&
               size == other.size &&
               memcmp(bytes, other.bytes, 4) == 0;
    }

    // Compare only the MIDI data bytes (ignores time field)
    bool contents_equal(MidiStorageElem const& other) const {
        if (size != other.size) return false;
        return memcmp(bytes, other.bytes, size) == 0;
    }
};

// Inline implementations for performance
inline uint8_t* MidiStorageElem::data() {
    return bytes;
}

inline const uint8_t* MidiStorageElem::data() const {
    return bytes;
}

inline uint32_t MidiStorageElem::get_size() const {
    return size;
}

inline void MidiStorageElem::get(uint32_t &size_out,
                                              uint32_t &time_out,
                                              const uint8_t* &data_out) const {
    size_out = size;
    time_out = time;
    data_out = data();
}

// Callback type for dropped messages
typedef std::function<void(uint32_t time, uint16_t size, const uint8_t* data)> DroppedMsgCallback;