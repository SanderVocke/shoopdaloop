#pragma once
#include "MidiPortInterface.h"
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
    );

    MidiReadableBufferInterface &PROC_get_read_buffer  (size_t n_frames) override;

    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override;

    const char* name() const override;
    PortDirection direction() const override;
    void close() override;
    PortType type() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    void PROC_write_event_value(uint32_t size,
                         uint32_t time,
                         const uint8_t* data) override;

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;

    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    LV2_Evbuf *internal_evbuf() const;

    ~InternalLV2MidiOutputPort();
};