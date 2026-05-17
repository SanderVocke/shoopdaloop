#pragma once
#include "MidiStorage.h"

/**
 * IMidiWriteableBuffer - Pure virtual interface for writeable MIDI buffers.
 * 
 * This interface defines the contract for writing MIDI events to a buffer.
 * Implementations must provide methods to add new events to the buffer.
 */
class IMidiWriteableBuffer {
public:
    virtual ~IMidiWriteableBuffer() {}
    
    /// Writes a MIDI event to the buffer. The event is typically appended.
    virtual void write_event(MidiStorageElem event) = 0;
};