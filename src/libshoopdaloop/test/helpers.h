#pragma once
#include <vector>
#include <stdio.h>
#include <functional>
#include "LoopInterface.h"
#include "MidiPort.h"
#include "MidiMessage.h"
#include <cstring>
#include "LoggingBackend.h"

template<typename S>
std::vector<S> create_audio_buf(uint32_t size, std::function<S(uint32_t)> elem_fn) {
    std::vector<S> buf(size);
    for(uint32_t idx=0; idx < buf.size(); idx++) {
        buf[idx] = elem_fn(idx);
    }
    return buf;
}

template<typename Buf, typename S>
void for_buf_elems(Buf const& buf, std::function<void(uint32_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = buf.size() - start; }
    for(uint32_t idx=start; idx < (start+n); idx++) {
        fn(idx, buf.at(idx));
    }
}

template<typename Channel, typename S>
void for_channel_elems(Channel &chan, std::function<void(uint32_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = chan.get_length() - start; }
    
    auto elems = chan.get_data(false);
    for(uint32_t idx=start; idx < (start+n); idx++) {
        fn(idx, elems[idx]);
    }
}

typedef MidiMessage<uint32_t, uint32_t> Msg;
class MidiTestBuffer : public MidiReadableBufferInterface,
                       public MidiWriteableBufferInterface {
public:
    std::vector<Msg> read;
    std::vector<Msg> written;
    uint32_t length;


    uint32_t PROC_get_n_events() const override {
        return read.size();
    }

    MidiSortableMessageInterface const& PROC_get_event_reference(uint32_t idx) override
    {
        return *dynamic_cast<const MidiSortableMessageInterface*>(&read.at(idx));
    }

    void PROC_get_event_value(uint32_t idx,
                              uint32_t &size_out,
                              uint32_t &time_out,
                              const uint8_t* &data_out) override
    {
        auto &msg = PROC_get_event_reference(idx);
        size_out = msg.get_size();
        time_out = msg.get_time();
        data_out = msg.get_data();
    }

    void PROC_write_event_value(uint32_t size,
                                uint32_t time,
                                const uint8_t* data) override
    {
        written.push_back(Msg(time, size, std::vector<uint8_t>(size)));
        memcpy((void*)written.back().data.data(), (void*) data, size);
    }

    bool read_by_reference_supported() const override { return true; }
    bool write_by_value_supported() const override { return true; }
    bool write_by_reference_supported() const override { return false; }
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {}
};