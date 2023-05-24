#pragma once
#include <vector>
#include <stdio.h>
#include <functional>
#include "LoopInterface.h"
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
    if(n < 0) { n = chan.get_length() - start; }
    
    auto elems = chan.get_data(false);
    for(size_t idx=start; idx < (start+n); idx++) {
        fn(idx, elems[idx]);
    }
}

inline void process_loop(LoopInterface *lp, size_t n_samples) {
    size_t n_left = n_samples;
    while (n_left > 0) {
        lp->PROC_handle_sync();
        lp->PROC_handle_poi();
        size_t n = std::min(lp->PROC_get_next_poi().value_or(n_left), n_left);
        if (n == 0) { throw std::runtime_error("0 proc in test"); }
        lp->PROC_process(n);
        n_left -= n;
    }
    lp->PROC_handle_sync();
    lp->PROC_handle_poi();
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

    MidiSortableMessageInterface const& PROC_get_event_reference(size_t idx) override
    {
        return *dynamic_cast<const MidiSortableMessageInterface*>(&read.at(idx));
    }

    void PROC_write_event_value(uint32_t size,
                                uint32_t time,
                                const uint8_t* data) override
    {
        written.push_back(Msg(time, size, std::vector<uint8_t>(size)));
        memcpy((void*)written.back().data.data(), (void*) data, size);
    }

    bool write_by_value_supported() const override { return true; }
    bool write_by_reference_supported() const override { return false; }
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {}
};