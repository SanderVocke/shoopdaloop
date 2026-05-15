#pragma once
#include "MidiBuffer.h"

struct DummyReadMidiBuf : public MidiReadableBuffer {
    uint32_t n_events() const override;
    MidiStorageElem get_event(uint32_t idx) const override;
};


struct DummyWriteMidiBuf : public MidiWriteableBuffer {
    void write_event(MidiStorageElem event) override;
};