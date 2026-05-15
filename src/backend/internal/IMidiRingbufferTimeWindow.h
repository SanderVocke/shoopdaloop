#pragma once
#include "IMidiStorageCore.h"
#include "MidiStorageElem.h"
#include <cstdint>
#include <functional>
#include <optional>

class IMidiStorage;

/**
 * IMidiRingbufferTimeWindow - Pure virtual interface for ringbuffer time-window operations.
 */
class IMidiRingbufferTimeWindow {
public:
    virtual ~IMidiRingbufferTimeWindow() = default;

    virtual void set_n_samples(uint32_t n) = 0;
    virtual uint32_t get_n_samples() const = 0;

    virtual uint32_t get_current_start_time() const = 0;
    virtual uint32_t get_current_end_time() const = 0;

    virtual void next_buffer(uint32_t n_frames, 
                            std::function<void(uint32_t, uint16_t, const uint8_t*)> dropped_msg_cb = nullptr) = 0;

    virtual bool put(uint32_t frame_in_current_buffer, uint16_t size, const uint8_t* data,
                    std::function<void(uint32_t, uint16_t, const uint8_t*)> dropped_msg_cb = nullptr) = 0;

    virtual void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const = 0;
};