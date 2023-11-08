#pragma once
#include "AudioSystemInterface.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
#include "PortInterface.h"
#include "LoggingEnabled.h"
#include "WithCommandQueue.h"
#include "MidiMessage.h"
#include "types.h"
#include <chrono>
#include <cstddef>
#include <ratio>
#include <stdexcept>
#include <memory>
#include <iostream>
#include <memory>
#include <set>
#include <thread>
#include <cstring>
#include <vector>
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <stdint.h>

class DummyPort : public virtual PortInterface {
protected:
    std::string m_name;
    PortDirection m_direction;
    PortType m_type;

public:
    DummyPort(
        std::string name,
        PortDirection direction,
        PortType type
    );

    const char* name() const override;
    PortDirection direction() const override;
    PortType type() const override;
    void close() override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
};

class DummyAudioPort : public virtual AudioPortInterface<audio_sample_t>,
                       public DummyPort,
                       private ModuleLoggingEnabled {
    std::string log_module_name() const override;

    std::string m_name;
    PortDirection m_direction;
    boost::lockfree::spsc_queue<std::vector<audio_sample_t>> m_queued_data;
    std::atomic<size_t> m_n_requested_samples;
    std::vector<audio_sample_t> m_retained_samples;
    std::vector<audio_sample_t> m_buffer_data;

public:
    DummyAudioPort(
        std::string name,
        PortDirection direction);
    
    audio_sample_t *PROC_get_buffer(size_t n_frames, bool do_zero=false) override;
    ~DummyAudioPort() override;

    // For input ports, queue up data to be read from the port.
    void queue_data(size_t n_frames, audio_sample_t const* data);
    bool get_queue_empty(); 

    // For output ports, ensure the postprocess function is called
    // and samples can be requested/dequeued.
    void PROC_post_process(audio_sample_t* buf, size_t n_frames);
    void request_data(size_t n_frames);
    std::vector<audio_sample_t> dequeue_data(size_t n);
};

class DummyMidiPort : public virtual MidiPortInterface,
                      public DummyPort,
                      public MidiReadableBufferInterface,
                      public MidiWriteableBufferInterface,
                      private ModuleLoggingEnabled {
public:
    using StoredMessage = MidiMessage<uint32_t, uint32_t>;
    
private:
    std::string log_module_name() const override;

    // Queued messages as external input to the port
    std::vector<StoredMessage> m_queued_msgs;
    std::atomic<size_t> current_buf_frames;
    std::vector<StoredMessage> m_buffer_data;

    // Amount of frames requested for reading externally out of the port
    std::atomic<size_t> n_requested_frames;
    std::atomic<size_t> n_original_requested_frames;
    std::atomic<size_t> m_update_queue_by_frames_pending = 0;
    std::vector<StoredMessage> m_written_requested_msgs;

public:
    size_t PROC_get_n_events() const override;
    virtual MidiSortableMessageInterface &PROC_get_event_reference(size_t idx) override;
    void PROC_write_event_value(uint32_t size,
                                uint32_t time,
                                const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    DummyMidiPort(
        std::string name,
        PortDirection direction
    );

    MidiReadableBufferInterface &PROC_get_read_buffer (size_t n_frames) override;
    MidiWriteableBufferInterface &PROC_get_write_buffer (size_t n_frames) override;

    void queue_msg(uint32_t size, uint32_t time, const uint8_t* data);
    bool get_queue_empty();
    
    void clear_queues();

    void PROC_post_process(size_t n_frames);
    // Request a certain number of frames to be stored.
    // Not allowed if previous request was not yet completed.
    void request_data(size_t n_frames);
    // Dequeue messages written during the requested period.
    // Timestamps are relative to when the request was made.
    std::vector<StoredMessage> get_written_requested_msgs();

    ~DummyMidiPort() override;
};

enum class DummyAudioSystemMode {
    Controlled,
    Automatic
};

template<typename Time, typename Size>
class DummyAudioSystem : public AudioSystemInterface<Time, Size>,
                         private ModuleLoggingEnabled,
                         public WithCommandQueue<20, 1000, 1000> {
    std::string log_module_name() const override;

    std::function<void(size_t)> m_process_cb;
    const size_t mc_sample_rate;
    const size_t mc_buffer_size;
    std::atomic<bool> m_finish;
    std::atomic<DummyAudioSystemMode> m_mode;
    std::atomic<size_t> m_controlled_mode_samples_to_process;
    std::atomic<bool> m_paused;
    std::thread m_proc_thread;
    std::set<std::shared_ptr<DummyAudioPort>> m_audio_ports;
    std::set<std::shared_ptr<DummyMidiPort>> m_midi_ports;
    std::string m_client_name;

    std::function<void(std::string, PortDirection)> m_audio_port_opened_cb, m_midi_port_opened_cb;
    std::function<void(std::string)> m_audio_port_closed_cb, m_midi_port_closed_cb;

public:

    DummyAudioSystem(
        std::string client_name,
        std::function<void(size_t)> process_cb,
        DummyAudioSystemMode mode = DummyAudioSystemMode::Automatic,
        size_t sample_rate = 48000,
        size_t buffer_size = 256
    );

    void start() override;

    ~DummyAudioSystem() override;

    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    size_t get_sample_rate() const override;
    size_t get_buffer_size() const override;
    void* maybe_client_handle() const override;
    const char* client_name() const override;
    void close() override;

    void pause();
    void resume();

    size_t get_xruns() const override;
    void reset_xruns() override;

    // Usually the dummy audio system will automatically process samples
    // continuously. When in controlled mode, instead the process callback
    // will always process 0 samples unless a specific amount of samples
    // is requested.
    // Process callback do still keep happening so that process thread
    // commands are still processed.
    void enter_mode(DummyAudioSystemMode mode);
    DummyAudioSystemMode get_mode() const;

    // In controlled mode, this amount of samples will be requested for
    // processing. If the amount is larger than the default buffer size,
    // it is processed in multiple iterations. If it is smaller, the process
    // thread will process exactly this amount.
    void controlled_mode_request_samples(size_t samples);
    size_t get_controlled_mode_samples_to_process() const;

    // Run until the requested amount of samples has been completed.
    void controlled_mode_run_request(size_t timeout_ms = 100);

    // Post process handler gets passed the amount of samples to process
    // and the amount of samples that were requested in controlled mode, if any
    void install_post_process_handler(std::function<void(size_t, size_t)> cb);

    void wait_process();
};

extern template class DummyAudioSystem<uint32_t, uint16_t>;
extern template class DummyAudioSystem<uint32_t, uint32_t>;
extern template class DummyAudioSystem<uint16_t, uint16_t>;
extern template class DummyAudioSystem<uint16_t, uint32_t>;
extern template class DummyAudioSystem<uint32_t, uint64_t>;
extern template class DummyAudioSystem<uint64_t, uint64_t>;