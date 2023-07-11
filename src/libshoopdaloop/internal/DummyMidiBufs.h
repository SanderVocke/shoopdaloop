#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include <jack_wrappers.h>
#include <stdexcept>

struct DummyReadMidiBuf : public MidiReadableBufferInterface {
    size_t PROC_get_n_events() const override { return 0; }
    MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) override
    { throw std::runtime_error("Attempt to read from dummy buffer");}
};


struct DummyWriteMidiBuf : public MidiWriteableBufferInterface {
    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override
    {}
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {}
    bool write_by_value_supported() const override { return true; }
    bool write_by_reference_supported() const override { return true; }
};
