#pragma once
#include <sstream>
#include <vector>
#include <stdio.h>
#include <functional>
#include "LoopInterface.h"
#include "MidiBufferInterfaces.h"
#include "MidiPort.h"
#include "MidiMessage.h"
#include <cstring>
#include "LoggingBackend.h"
#include "types.h"
#include "libshoopdaloop.h"
#include <catch2/catch_test_macros.hpp>

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

template<typename Message>
inline Message create_noteOn(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    Message rval;
    rval.time = time;
    rval.data = { static_cast<unsigned char>(0x90 + channel), note, velocity };
    rval.size = rval.data.size();
    return rval;
}

template<typename Message>
inline Message create_noteOff(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    Message rval;
    rval.time = time;
    rval.data = { static_cast<unsigned char>(0x80 + channel), note, velocity };
    rval.size = rval.data.size();
    return rval;
}

inline void queue_midi_msgs(shoopdaloop_midi_port_t *port, std::vector<Msg> &msgs) {
    auto sequence = alloc_midi_sequence(msgs.size());
    sequence->length_samples = msgs.back().time + 1;
    for(size_t i=0; i<msgs.size(); i++) {
        Msg & msg = msgs.at(i);
        auto pEvent = alloc_midi_event(msg.size);
        sequence->events[i] = pEvent;
        auto & event = *pEvent;
        event.time = msg.time;
        event.size = msg.size;
        for (auto j=0; j<msg.size && j<msg.data.size(); j++) {
            event.data[j] = msg.data[j];
        }
    }

    dummy_midi_port_queue_data(port, sequence);
    destroy_midi_sequence(sequence);
}

inline std::vector<Msg> convert_api_midi_msgs(shoop_midi_sequence_t *sequence, bool destroy=true) {
    std::vector<Msg> rval;
    for (size_t i=0; i<sequence->n_events; i++) {
        auto &ev = sequence->events[i];
        Msg m;
        m.time = ev->time;
        m.size = ev->size;
        for(size_t j=0; j<ev->size; j++) {
            m.data.push_back(ev->data[j]);
        }
        rval.push_back(m);
    }

    if (destroy) { destroy_midi_sequence(sequence); }

    return rval;
}

inline std::string stringify_msg(MidiSortableMessageInterface &m) {
    std::ostringstream s;
    s << "{ t=" << m.get_time() << ", s=" << m.get_size() << ", d={";
    for (size_t i=0; i<m.get_size(); i++) {
        if (i>0) { s << ", "; }
        s << m.get_data()[i];
    }
    s << "} }";
    return s.str();
}

inline void CHECK_MSGS_EQUAL(MidiSortableMessageInterface &a, MidiSortableMessageInterface &b) {
    CHECK(stringify_msg(a) == stringify_msg(b));
}