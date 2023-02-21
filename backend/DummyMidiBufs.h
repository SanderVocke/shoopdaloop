#pragma once
#include <jack/types.h>
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <jack/jack.h>
#include <jack/midiport.h>
#include <stdexcept>

struct DummyReadMidiBuf : public MidiReadableBufferInterface {
    size_t PROC_get_n_events() const override { return 0; }
    void PROC_get_event(size_t idx,
                    uint32_t &size_out,
                    uint32_t &time_out,
                    uint8_t* &data_out) const override
    { throw std::runtime_error("Attempt to read from dummy buffer");}
};


struct DummyWriteMidiBuf : public MidiWriteableBufferInterface {
    void PROC_write_event(uint32_t size,
                        uint32_t time,
                        uint8_t* data)
    {}
};
