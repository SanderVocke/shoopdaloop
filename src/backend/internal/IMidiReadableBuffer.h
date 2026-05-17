#pragma once
#include "MidiStorage.h"

/**
 * IMidiReadableBuffer - Pure virtual interface for readable MIDI buffers.
 * 
 * This interface defines the contract for reading MIDI events from a buffer.
 * Implementations must provide methods to query the number of events and
 * retrieve individual events by index.
 */
class IMidiReadableBuffer {
public:
    virtual ~IMidiReadableBuffer() {}
    
    /// Returns the number of MIDI events currently in the buffer.
    virtual uint32_t n_events() const = 0;
    
    /// Retrieves a MIDI event by index. The index must be less than n_events().
    virtual MidiStorageElem get_event(uint32_t idx) const = 0;
};