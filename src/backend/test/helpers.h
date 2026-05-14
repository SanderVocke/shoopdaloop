#pragma once
#include "LoopInterface.h"
#include "MidiBuffer.h"
#include "MidiPort.h"
#include "LoggingBackend.h"
#include "midi_helpers.h"
#include "types.h"
#include "libshoopdaloop_backend.h"

#include <sstream>
#include <vector>
#include <stdio.h>
#include <functional>
#include <cstring>

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

typedef MidiStorageElem TestMsg;
typedef MidiStorageElem Msg;

class MidiTestBuffer : public MidiReadableBuffer,
                       public MidiWriteableBuffer {
public:
    std::vector<TestMsg> read;
    std::vector<TestMsg> written;
    uint32_t length;


    uint32_t n_events() const override {
        return read.size();
    }

    MidiStorageElem get_event(uint32_t idx) const override
    {
        return read.at(idx);
    }

    void write_event(MidiStorageElem event) override
    {
        written.push_back(event);
    }
};

inline MidiStorageElem create_note_msg(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    MidiStorageElem rval;
    rval.proc_time = time;
    rval.bytes[0] = (uint8_t)(0x90 | (channel & 0x0F));
    rval.bytes[1] = note;
    rval.bytes[2] = velocity;
    rval.size = 3;
    return rval;
}

inline MidiStorageElem create_noteOn(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    return create_note_msg(time, channel, note, velocity);
}

inline MidiStorageElem create_noteOff(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    MidiStorageElem rval;
    rval.proc_time = time;
    rval.bytes[0] = (uint8_t)(0x80 | (channel & 0x0F));
    rval.bytes[1] = note;
    rval.bytes[2] = velocity;
    rval.size = 3;
    return rval;
}

inline MidiStorageElem create_cc(uint32_t time, uint8_t channel, uint8_t cc_num, uint8_t value) {
    MidiStorageElem rval;
    rval.proc_time = time;
    rval.bytes[0] = (uint8_t)(0xB0 | (channel & 0x0F));
    rval.bytes[1] = cc_num;
    rval.bytes[2] = value;
    rval.size = 3;
    return rval;
}

template<typename Message>
inline Message create_noteOn(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    return create_note_msg(time, channel, note, velocity);
}

template<typename Message>
inline Message create_noteOff(uint32_t time, uint8_t channel, uint8_t note, uint8_t velocity) {
    MidiStorageElem rval;
    rval.proc_time = time;
    rval.bytes[0] = (uint8_t)(0x80 | (channel & 0x0F));
    rval.bytes[1] = note;
    rval.bytes[2] = velocity;
    rval.size = 3;
    return rval;
}

template<typename Message>
inline Message create_cc(uint32_t time, uint8_t channel, uint8_t cc_num, uint8_t value) {
    MidiStorageElem rval;
    rval.proc_time = time;
    rval.bytes[0] = (uint8_t)(0xB0 | (channel & 0x0F));
    rval.bytes[1] = cc_num;
    rval.bytes[2] = value;
    rval.size = 3;
    return rval;
}

inline shoop_midi_sequence_t *convert_midi_msgs_to_api(std::vector<MidiStorageElem> &msgs) {
    auto sequence = alloc_midi_sequence(msgs.size());
    sequence->length_samples = msgs.back().proc_time + 1;
    for(size_t i=0; i<msgs.size(); i++) {
        auto &msg = msgs.at(i);
        auto pEvent = alloc_midi_event(msg.size);
        sequence->events[i] = pEvent;
        auto & event = *pEvent;
        event.time = msg.proc_time;
        event.size = msg.size;
        for (auto j=0; j<msg.size && j<4; j++) {
            event.data[j] = msg.bytes[j];
        }
    }
    return sequence;
}

template<typename Message>
inline void queue_midi_msgs(shoopdaloop_midi_port_t *port, std::vector<Message> &msgs) {
    std::vector<MidiStorageElem> converted;
    for (auto &m : msgs) {
        converted.push_back(m);
    }
    auto sequence = convert_midi_msgs_to_api(converted);
    dummy_midi_port_queue_data(port, sequence);
    destroy_midi_sequence(sequence);
}

inline std::vector<MidiStorageElem> convert_api_midi_msgs(shoop_midi_sequence_t *sequence, bool destroy=true) {
    std::vector<MidiStorageElem> rval;
    for (size_t i=0; i<sequence->n_events; i++) {
        auto &ev = sequence->events[i];
        MidiStorageElem m;
        m.proc_time = ev->time;
        m.size = ev->size;
        for(size_t j=0; j<ev->size && j<4; j++) {
            m.bytes[j] = ev->data[j];
        }
        rval.push_back(m);
    }

    if (destroy) { destroy_midi_sequence(sequence); }

    return rval;
}

inline std::string stringify_msg(MidiStorageElem &m) {
    std::ostringstream s;
    s << "{ t=" << m.proc_time << ", s=" << m.size << ", d={";
    for (size_t i=0; i<m.size; i++) {
        if (i>0) { s << ", "; }
        s << (int)m.bytes[i];
    }
    s << "} }";
    return s.str();
}

inline void CHECK_MSGS_EQUAL(MidiStorageElem &a, MidiStorageElem &b) {
    CHECK(stringify_msg(a) == stringify_msg(b));
}

namespace Catch {
    template<>
    struct StringMaker<std::vector<uint8_t>> {
        static std::string convert(const std::vector<uint8_t>& vec) {
            std::ostringstream oss;
            oss << "[";
            for (size_t i=0; i<vec.size(); i++) {
                if (i>0) { oss << ", "; }
                oss << (int)vec[i];
            }
            oss << "]";
            return oss.str();
        }
    };

    template<>
    struct StringMaker<MidiStorageElem> {
        static std::string convert(const MidiStorageElem& e) {
            std::ostringstream oss;
            oss << "{ t:" << e.proc_time << ", s:" << e.size << ", d:[";
            for (size_t i=0; i<e.size; i++) {
                if (i>0) { oss << ", "; }
                oss << (int)e.bytes[i];
            }
            oss << "] }";
            return oss.str();
        }
    };

    template<>
    struct StringMaker<std::vector<MidiStorageElem>> {
        static std::string convert(const std::vector<MidiStorageElem>& vec) {
            std::ostringstream oss;
            oss << "[\n";
            for (auto &e : vec) {
                oss << "  " << StringMaker<MidiStorageElem>::convert(e) << "\n";
            }
            oss << "]";
            return oss.str();
        }
    };
}