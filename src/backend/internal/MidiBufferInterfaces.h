#pragma once 
#include <stdint.h>

class MidiSortableMessageInterface {
public:
    virtual uint32_t get_time() const = 0;
    virtual const uint8_t* get_data() const = 0;
    virtual uint32_t get_size() const = 0;
    virtual void     get(uint32_t &size_out,
                         uint32_t &time_out,
                         const uint8_t* &data_out) const = 0;
    
    bool contents_equal(MidiSortableMessageInterface const& other) const {
        if (get_time() != other.get_time()) { return false; }
        if (get_size() != other.get_size()) { return false; }
        auto *d1 = get_data();
        auto *d2 = other.get_data();
        for (uint32_t i = 0; i < get_size(); i++) {
            if (d1[i] != d2[i]) {
                return false;
            }
        }
        return true;
    }
};

class MidiReadableBufferInterface {
public:
    virtual uint32_t PROC_get_n_events() const = 0;

    virtual bool read_by_reference_supported() const = 0;

    virtual MidiSortableMessageInterface const& PROC_get_event_reference(uint32_t idx) = 0;
    virtual void PROC_get_event_value(uint32_t idx,
                                      uint32_t &size_out,
                                      uint32_t &time_out,
                                      const uint8_t* &data_out) = 0;
    
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

class MidiReadWriteBufferInterface : public virtual MidiWriteableBufferInterface,
                                     public virtual MidiReadableBufferInterface
{
public:
    // A read/write buffer will allow some kind of processing on the content.
    virtual void PROC_process(uint32_t nframes) {}
    virtual void PROC_prepare(uint32_t nframes) {}
};