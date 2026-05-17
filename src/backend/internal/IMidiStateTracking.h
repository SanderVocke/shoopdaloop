#pragma once
#include <cstdint>
#include <optional>
#include <vector>
#include "shoop_shared_ptr.h"

/**
 * IMidiStateTracking - Pure virtual interface for MIDI state of a port.
 */
class IMidiStateTracking {
public:
    virtual ~IMidiStateTracking() = default;

    virtual uint32_t n_notes_active() const = 0;

    virtual uint32_t get_input_event_count() const = 0;
    virtual uint32_t get_output_event_count() const = 0;
};