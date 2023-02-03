#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"


template<typename TimeType, typename SizeType>
class MidiReadableBufferInterface {
public:
    virtual size_t get_n_events() const = 0;
    virtual void   get_event(size_t idx,
                             SizeType &size_out,
                             TimeType &time_out,
                             uint8_t* &data_out) const = 0;
};

template<typename TimeType, typename SizeType>
class MidiWriteableBufferInterface {
public:
    virtual void write_event(SizeType size,
                             TimeType time,
                             uint8_t* data) = 0;
};

template<typename TimeType, typename SizeType>
class _MidiPortInterface : public PortInterface {
public:
    _MidiPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    typedef MidiReadableBufferInterface <TimeType, SizeType>  ReadBuf;
    typedef MidiWriteableBufferInterface<TimeType, SizeType> WriteBuf;

    virtual std::unique_ptr<ReadBuf>  get_read_buffer (size_t n_frames) = 0;
    virtual std::unique_ptr<WriteBuf> get_write_buffer(size_t n_frames) = 0;

    _MidiPortInterface() {}
    virtual ~_MidiPortInterface() {}
};

typedef _MidiPortInterface<uint32_t, uint32_t> MidiPortInterface;