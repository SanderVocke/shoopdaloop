#pragma once
#include <cstdint>
#include <optional>
#include <vector>
#include "shoop_shared_ptr.h"

/**
 * IMidiStateTracking - Pure virtual interface for MIDI state tracking.
 * 
 * This interface provides access to MIDI state information such as
 * currently active notes, control values, and event counts.
 */
class IMidiStateTracking {
public:
    virtual ~IMidiStateTracking() = default;

    /// Returns the number of notes currently active.
    virtual uint32_t n_notes_active() const = 0;

    /// Returns the number of input events processed.
    virtual uint32_t get_input_event_count() const = 0;

    /// Returns the number of output events processed.
    virtual uint32_t get_output_event_count() const = 0;
};