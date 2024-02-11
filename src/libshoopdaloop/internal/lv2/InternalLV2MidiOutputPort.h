#pragma once
#include "MidiPort.h"
#include <lv2_evbuf.h>

class InternalLV2MidiOutputPort : public MidiPort, public MidiWriteableBufferInterface {
    std::string m_name = "";
    shoop_port_direction_t m_direction = Input;
    LV2_Evbuf *m_evbuf = nullptr;
    uint32_t m_midi_event_type_urid = 0;
    LV2_Evbuf_Iterator m_iter;
    
public:
    // Note that the port direction for internal ports are defined w.r.t. ShoopDaLoop.
    // That is to say, the inputs of a plugin effect are regarded as output ports to
    // ShoopDaLoop.
    InternalLV2MidiOutputPort(
        std::string name,
        shoop_port_direction_t direction = Input,
        uint32_t capacity = 0,
        uint32_t atom_chunk_urid = 0,
        uint32_t atom_sequence_urid = 0,
        uint32_t midi_event_type_urid = 0
    );

    MidiReadableBufferInterface *PROC_get_read_output_data_buffer  (uint32_t n_frames) override;
    MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer (uint32_t n_frames) override;

    const char* name() const override;
    void close() override;
    PortDataType type() const override;
    void *maybe_driver_handle () const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;

    void PROC_write_event_value(uint32_t size,
                         uint32_t time,
                         const uint8_t* data) override;

    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;

    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    bool has_internal_read_access() const override { return false; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return false; }
    bool has_implicit_output_sink() const override { return true; }

    LV2_Evbuf *internal_evbuf() const;

    void PROC_prepare(uint32_t n_frames) override;

    ~InternalLV2MidiOutputPort();
};