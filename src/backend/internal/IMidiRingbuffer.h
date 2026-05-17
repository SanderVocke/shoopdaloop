#pragma once
#include <cstdint>
#include <memory>
#include <optional>

// Forward declarations
class IMidiStorage;

/**
 * IMidiRingbuffer - Pure virtual interface for ringbuffer operations.
 * 
 * This interface provides access to MIDI ringbuffer functionality,
 * including sample count management and snapshot operations.
 */
class IMidiRingbuffer {
public:
    virtual ~IMidiRingbuffer() = default;

    /// Sets the number of samples to track in the ringbuffer.
    virtual void set_n_samples(uint32_t n) = 0;

    /// Returns the number of samples being tracked.
    virtual uint32_t get_n_samples() const = 0;

    /// Takes a snapshot of the ringbuffer contents into the provided storage.
    virtual void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const = 0;

    /// Returns the current number of samples in the ringbuffer.
    virtual uint32_t get_current_n_samples() const = 0;
};