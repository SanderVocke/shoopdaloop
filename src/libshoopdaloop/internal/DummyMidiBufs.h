#pragma once
#include "MidiPortInterface.h"

struct DummyReadMidiBuf : public MidiReadableBufferInterface {
    uint32_t PROC_get_n_events() const override;
    MidiSortableMessageInterface &PROC_get_event_reference(uint32_t idx) override;
};


struct DummyWriteMidiBuf : public MidiWriteableBufferInterface {
    void PROC_write_event_value(uint32_t size,
                        uint32_t time,
                        const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_value_supported() const override;
    bool write_by_reference_supported() const override;
};
