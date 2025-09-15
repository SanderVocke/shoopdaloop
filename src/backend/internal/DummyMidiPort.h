#pragma once
#include "AudioMidiDriver.h"
#include "MidiPort.h"
#include "DummyPort.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "types.h"
#include <memory>
#include <memory>
#include <set>
#include <thread>
#include <vector>
#include <memory>
#include <stdint.h>
#include <mutex>

class DummyMidiPort : public virtual MidiPort,
                      public DummyPort,
                      public MidiReadableBufferInterface,
                      public MidiWriteableBufferInterface,
                      public WithCommandQueue,
                      private ModuleLoggingEnabled<"Backend.DummyMidiPort"> {
public:
    using StoredMessage = MidiMessage<uint32_t, uint16_t>;

private:

    // Queued messages as external input to the port
    std::vector<StoredMessage> m_queued_msgs;

    std::vector<StoredMessage> m_buffer_data;
    std::vector<StoredMessage> m_written_requested_msgs;

    std::atomic<uint32_t> current_buf_frames = 0;

    // Amount of frames requested for reading externally out of the port
    std::atomic<uint32_t> n_requested_frames = 0;

    std::atomic<uint32_t> n_processed_last_round = 0;
    std::atomic<uint32_t> n_original_requested_frames = 0;

public:
    uint32_t PROC_get_n_events() const override;
    virtual MidiSortableMessageInterface &PROC_get_event_reference(uint32_t idx) override;
    virtual void PROC_get_event_value(uint32_t idx,
                              uint32_t &size_out,
                              uint32_t &time_out,
                              const uint8_t* &data_out) override;
    void PROC_write_event_value(uint32_t size,
                                uint32_t time,
                                const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;
    bool read_by_reference_supported() const override;

    MidiWriteableBufferInterface *PROC_get_write_data_into_port_buffer(uint32_t n_frames) override;
    MidiReadableBufferInterface *PROC_get_read_output_data_buffer(uint32_t n_frames) override;

    DummyMidiPort(
        std::string name,
        shoop_port_direction_t direction,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>()
    );

    void queue_msg(uint32_t size, uint32_t time, const uint8_t* data);
    bool get_queue_empty();

    void clear_queues();

    // Request a certain number of frames to be stored.
    // Not allowed if previous request was not yet completed.
    void request_data(uint32_t n_frames);
    // Dequeue messages written during the requested period.
    // Timestamps are relative to when the request was made.
    std::vector<StoredMessage> get_written_requested_msgs();

    ~DummyMidiPort() override;

    bool has_internal_read_access() const override { return true; }
    bool has_internal_write_access() const override { return true; }
    bool has_implicit_input_source() const override { return true; }
    bool has_implicit_output_sink() const override { return true; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
};