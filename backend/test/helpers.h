#pragma once
#include <vector>
#include <stdio.h>
#include <functional>
#include "MidiPortInterface.h"
#include "MidiMessage.h"
#include <cstring>

template<typename S>
std::vector<S> create_audio_buf(size_t size, std::function<S(size_t)> elem_fn) {
    std::vector<S> buf(size);
    for(size_t idx=0; idx < buf.size(); idx++) {
        buf[idx] = elem_fn(idx);
    }
    return buf;
}

template<typename Buf, typename S>
void for_buf_elems(Buf const& buf, std::function<void(size_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = buf.size() - start; }
    for(size_t idx=start; idx < (start+n); idx++) {
        fn(idx, buf.at(idx));
    }
}

template<typename Channel, typename S>
void for_channel_elems(Channel &chan, std::function<void(size_t,S const&)> fn,
                   int start=0, int n=-1) {
    if(n < 0) { n = chan.data_length() - start; }
    
    auto elems = chan.get_data(false);
    for(size_t idx=start; idx < (start+n); idx++) {
        fn(idx, elems[idx]);
    }
}

typedef MidiMessage<uint32_t, uint32_t> Msg;
class MidiTestBuffer : public MidiReadableBufferInterface,
                       public MidiWriteableBufferInterface {
public:
    std::vector<Msg> read;
    std::vector<Msg> written;
    size_t length;


    size_t PROC_get_n_events() const override {
        return read.size();
    }

    void   PROC_get_event(size_t idx,
                             uint32_t &size_out,
                             uint32_t &time_out,
                             uint8_t* &data_out) const override
    {
        size_out = read[idx].size;
        time_out = read[idx].time;
        data_out = (uint8_t*)read[idx].data.data();
    }

    void PROC_write_event(uint32_t size,
                             uint32_t time,
                             uint8_t* data) override
    {
        written.push_back(Msg{
            .time = time,
            .size = size,
            .data = std::vector<uint8_t>(size)
        });

        memcpy((void*)written.back().data.data(), (void*) data, size);
    }
};