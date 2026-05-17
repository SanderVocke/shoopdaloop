#pragma once
#include "MidiStorage.h"
#include "IMidiReadableBuffer.h"
#include "IMidiWriteableBuffer.h"

/**
 * MidiReadableBuffer - Base class for readable MIDI buffers.
 */
class MidiReadableBuffer : public IMidiReadableBuffer {
public:
    MidiReadableBuffer() = default;
    virtual ~MidiReadableBuffer() {}
    
    /// Default implementation returns 0 (empty buffer).
    virtual uint32_t n_events() const override { return 0; }
    
    /// Default implementation returns null event (should not be called on empty buffer).
    virtual MidiStorageElem get_event(uint32_t idx) const override { (void)idx; return MidiStorageElem{}; }
};

/**
 * MidiWriteableBuffer - Base class for writeable MIDI buffers.
 */
class MidiWriteableBuffer : public IMidiWriteableBuffer {
public:
    MidiWriteableBuffer() = default;
    virtual ~MidiWriteableBuffer() {}
    
    /// Default empty implementation (no-op).
    virtual void write_event(MidiStorageElem event) override { (void)event; }
};