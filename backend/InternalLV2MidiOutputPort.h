#pragma once
#include <string>
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <stdexcept>
#include <cstring>
#include <iostream>
#include <vector>
#include <lv2_evbuf.h>

class InternalLV2MidiOutputPort : public MidiPortInterface, public MidiWriteableBufferInterface {
    std::string m_name;
    PortDirection m_direction;
    LV2_Evbuf *m_evbuf;
    uint32_t m_midi_event_type_urid;
    LV2_Evbuf_Iterator m_iter;
public:
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
    InternalLV2MidiOutputPort(
        std::string name,
        PortDirection direction,
        size_t capacity,
        uint32_t atom_chunk_urid,
        uint32_t atom_sequence_urid,
        uint32_t midi_event_type_urid
    ) : MidiPortInterface(name, direction),
        m_name(name),
        m_direction(direction),
        m_evbuf(lv2_evbuf_new(capacity, atom_chunk_urid, atom_sequence_urid)),
        m_midi_event_type_urid(midi_event_type_urid) {}

    MidiReadableBufferInterface &PROC_get_read_buffer  (size_t n_frames) override {
        throw std::runtime_error("Internal LV2 MIDI output port does not support reading.");
    }

    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override {
        lv2_evbuf_reset(m_evbuf, true);
        m_iter = lv2_evbuf_begin(m_evbuf);
        return *(static_cast<MidiWriteableBufferInterface*>(this));
    }

    const char* name() const override {
        return m_name.c_str();
    }

    PortDirection direction() const override {
        return m_direction;
    }

    void close() override {}

    void PROC_write_event_value(uint32_t size,
                         uint32_t time,
                         const uint8_t* data) override
    {
        if (!lv2_evbuf_is_valid(m_iter)) {
            throw std::runtime_error("LV2 EvBuf iterator not valid");
        }
        bool result = lv2_evbuf_write(
            &m_iter,
            time,
            0,
            m_midi_event_type_urid,
            size,
            (void*)data
        );
        if (!result) {
            throw std::runtime_error("Failed to write MIDI event into LV2 evbuf");
        }
    }

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override {
        throw std::runtime_error("Write by reference not supported");
    }

    bool write_by_reference_supported() const override { return false; }
    bool write_by_value_supported() const override { return true; }

    LV2_Evbuf *internal_evbuf() const { return m_evbuf; }

    ~InternalLV2MidiOutputPort() {
        lv2_evbuf_free(m_evbuf);
    }
};