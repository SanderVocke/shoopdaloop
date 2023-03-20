#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include <string>
#include "PortInterface.h"

class MidiSortableMessageInterface {
public:
    virtual uint32_t get_time() const = 0;
    virtual const uint8_t* get_data() const = 0;
    virtual uint32_t get_size() const = 0;
    virtual void     get(uint32_t &size_out,
                         uint32_t &time_out,
                         const uint8_t* &data_out) const = 0;
};

class MidiReadableBufferInterface {
public:
    virtual size_t PROC_get_n_events() const = 0;
    virtual MidiSortableMessageInterface const& PROC_get_event_reference(size_t idx) const = 0;
    
    MidiReadableBufferInterface() {}
    virtual ~MidiReadableBufferInterface() {};
};

class MidiWriteableBufferInterface {
public:
    virtual bool write_by_value_supported() const = 0;
    virtual void PROC_write_event_value(uint32_t size,
                                        uint32_t time,
                                        const uint8_t* data) = 0;
    
    virtual bool write_by_reference_supported() const = 0;
    virtual void PROC_write_event_reference(MidiSortableMessageInterface const& reference) = 0;
    
    MidiWriteableBufferInterface() {}
    virtual ~MidiWriteableBufferInterface() {};
};

class MidiPortInterface : public PortInterface {
public:
MidiPortInterface(
        std::string name,
        PortDirection direction
    ) : PortInterface() {}

    virtual std::unique_ptr<MidiReadableBufferInterface>  PROC_get_read_buffer  (size_t n_frames) = 0;
    virtual std::unique_ptr<MidiWriteableBufferInterface> PROC_get_write_buffer (size_t n_frames) = 0;

    MidiPortInterface() {}
    virtual ~MidiPortInterface() {};
};