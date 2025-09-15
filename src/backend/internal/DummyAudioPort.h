#pragma once
#include "AudioMidiDriver.h"
#include "AudioPort.h"
#include "DummyPort.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "WithCommandQueue.h"
#include "types.h"
#include <memory>
#include <memory>
#include <set>
#include <thread>
#include <vector>
#include <memory>
#include <stdint.h>
#include "shoop_shared_ptr.h"

class DummyAudioPort : public virtual AudioPort<audio_sample_t>,
                       public DummyPort,
                       private ModuleLoggingEnabled<"Backend.DummyAudioPort">,
                       private WithCommandQueue {

    std::string m_name = "";
    shoop_port_direction_t m_direction = ShoopPortDirection_Input;
    boost::lockfree::spsc_queue<std::vector<audio_sample_t>> m_queued_data;
    std::atomic<uint32_t> m_n_requested_samples = 0;
    std::vector<audio_sample_t> m_retained_samples;
    std::vector<audio_sample_t> m_buffer_data;

public:
    DummyAudioPort(
        std::string name,
        shoop_port_direction_t direction,
        shoop_shared_ptr<AudioPort<audio_sample_t>::BufferPool> maybe_ringbuffer_buffer_pool,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>());

    audio_sample_t *PROC_get_buffer(uint32_t n_frames) override;
    ~DummyAudioPort() override;

    // For input ports, queue up data to be read from the port.
    void queue_data(uint32_t n_frames, audio_sample_t const* data);
    bool get_queue_empty();

    // For output ports, ensure the postprocess function is called
    // and samples can be requested/dequeued.
    void request_data(uint32_t n_frames);
    std::vector<audio_sample_t> dequeue_data(uint32_t n);

    bool has_internal_read_access() const override { return m_direction == ShoopPortDirection_Input; }
    bool has_internal_write_access() const override { return m_direction == ShoopPortDirection_Output; }
    bool has_implicit_input_source() const override { return m_direction == ShoopPortDirection_Input; }
    bool has_implicit_output_sink() const override { return m_direction == ShoopPortDirection_Output; }

    void PROC_prepare(uint32_t nframes) override;
    void PROC_process(uint32_t nframes) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
};