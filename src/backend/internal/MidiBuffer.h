#pragma once
#include "MidiStorage.h"

class MidiReadableBuffer {
public:
    virtual uint32_t n_events() const = 0;
    virtual MidiStorageElem get_event(uint32_t idx) const = 0;
    virtual ~MidiReadableBuffer() {}
};

class MidiWriteableBuffer {
public:
    virtual void write_event(MidiStorageElem event) = 0;
    virtual ~MidiWriteableBuffer() {}
};