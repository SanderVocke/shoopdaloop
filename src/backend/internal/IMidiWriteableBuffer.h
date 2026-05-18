#pragma once
#include "MidiStorageElem.h"

/**
 * IMidiWriteableBuffer - Pure virtual interface for writeable MIDI buffers.
 */
class IMidiWriteableBuffer {
public:
    virtual ~IMidiWriteableBuffer() {}
    
    /// Writes a MIDI event to the buffer. The event is typically appended.
    virtual void write_event(MidiStorageElem event) = 0;
};