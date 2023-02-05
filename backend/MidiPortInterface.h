#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"

class MidiReadableBufferInterface {
public:
    virtual size_t get_n_events() const = 0;
    virtual void   get_event(size_t idx,
                             uint32_t &size_out,
                             uint32_t &time_out,
                             uint8_t* &data_out) const = 0;
};


class MidiWriteableBufferInterface {
public:
    virtual void write_event(uint32_t size,
                             uint32_t time,
                             uint8_t* data) = 0;
};

class MidiPortInterface : public PortInterface {
public:
MidiPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual std::unique_ptr<MidiReadableBufferInterface>  get_read_buffer  (size_t n_frames) = 0;
    virtual std::unique_ptr<MidiWriteableBufferInterface> get_write_buffer (size_t n_frames) = 0;

    MidiPortInterface() {}
    virtual ~MidiPortInterface() {}
};