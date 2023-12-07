#pragma once
#include "AudioMidiDriver.h"
#include "MidiPortInterface.h"
#include "AudioPortInterface.h"
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
#include <boost/lockfree/spsc_queue.hpp>
#include <memory>
#include <stdint.h>

class DummyPort : public virtual PortInterface {
protected:
    std::string m_name;
    shoop_port_direction_t m_direction;
    PortType m_type;

public:
    DummyPort(
        std::string name,
        shoop_port_direction_t direction,
        PortType type
    );

    const char* name() const override;
    shoop_port_direction_t direction() const override;
    PortType type() const override;
    void close() override;
    void *maybe_driver_handle() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
};

class DummyAudioPort : public virtual AudioPortInterface<audio_sample_t>,
                       public DummyPort,
                       private ModuleLoggingEnabled<"Backend.DummyAudioPort"> {

    std::string m_name;
    shoop_port_direction_t m_direction;
    boost::lockfree::spsc_queue<std::vector<audio_sample_t>> m_queued_data;
    std::atomic<uint32_t> m_n_requested_samples;
    std::vector<audio_sample_t> m_retained_samples;
    std::vector<audio_sample_t> m_buffer_data;

public:
    DummyAudioPort(
        std::string name,
        shoop_port_direction_t direction);
    
    audio_sample_t *PROC_get_buffer(uint32_t n_frames, bool do_zero=false) override;
    ~DummyAudioPort() override;

    // For input ports, queue up data to be read from the port.
    void queue_data(uint32_t n_frames, audio_sample_t const* data);
    bool get_queue_empty(); 

    // For output ports, ensure the postprocess function is called
    // and samples can be requested/dequeued.
    void PROC_post_process(audio_sample_t* buf, uint32_t n_frames);
    void request_data(uint32_t n_frames);
    std::vector<audio_sample_t> dequeue_data(uint32_t n);
};

class DummyMidiPort : public virtual MidiPortInterface,
                      public DummyPort,
                      public MidiReadableBufferInterface,
                      public MidiWriteableBufferInterface,
                      private ModuleLoggingEnabled<"Backend.DummyMidiPort"> {
public:
    using StoredMessage = MidiMessage<uint32_t, uint32_t>;
    
private:

    // Queued messages as external input to the port
    std::vector<StoredMessage> m_queued_msgs;
    std::atomic<uint32_t> current_buf_frames;
    std::vector<StoredMessage> m_buffer_data;

    // Amount of frames requested for reading externally out of the port
    std::atomic<uint32_t> n_requested_frames;
    std::atomic<uint32_t> n_original_requested_frames;
    std::atomic<uint32_t> m_update_queue_by_frames_pending = 0;
    std::vector<StoredMessage> m_written_requested_msgs;

public:
    uint32_t PROC_get_n_events() const override;
    virtual MidiSortableMessageInterface &PROC_get_event_reference(uint32_t idx) override;
    void PROC_write_event_value(uint32_t size,
                                uint32_t time,
                                const uint8_t* data) override;
    void PROC_write_event_reference(MidiSortableMessageInterface const& m) override;
    bool write_by_reference_supported() const override;
    bool write_by_value_supported() const override;

    DummyMidiPort(
        std::string name,
        shoop_port_direction_t direction
    );

    MidiReadableBufferInterface &PROC_get_read_buffer (uint32_t n_frames) override;
    MidiWriteableBufferInterface &PROC_get_write_buffer (uint32_t n_frames) override;

    void queue_msg(uint32_t size, uint32_t time, const uint8_t* data);
    bool get_queue_empty();
    
    void clear_queues();

    void PROC_post_process(uint32_t n_frames);
    // Request a certain number of frames to be stored.
    // Not allowed if previous request was not yet completed.
    void request_data(uint32_t n_frames);
    // Dequeue messages written during the requested period.
    // Timestamps are relative to when the request was made.
    std::vector<StoredMessage> get_written_requested_msgs();

    ~DummyMidiPort() override;
};

struct DummyAudioMidiDriverSettings : public AudioMidiDriverSettingsInterface {
    DummyAudioMidiDriverSettings() {}

    uint32_t sample_rate = 48000;
    uint32_t buffer_size = 256;
    std::string client_name = "dummy";
};

enum class DummyAudioMidiDriverMode {
    Controlled,
    Automatic
};

template<typename Time, typename Size>
class DummyAudioMidiDriver : public AudioMidiDriver,
                             private ModuleLoggingEnabled<"Backend.DummyAudioMidiDriver"> {
    std::atomic<bool> m_finish;
    std::atomic<DummyAudioMidiDriverMode> m_mode;
    std::atomic<uint32_t> m_controlled_mode_samples_to_process;
    std::atomic<bool> m_paused;
    std::thread m_proc_thread;
    std::set<std::shared_ptr<DummyAudioPort>> m_audio_ports;
    std::set<std::shared_ptr<DummyMidiPort>> m_midi_ports;
    std::string m_client_name_str;

    std::function<void(std::string, shoop_port_direction_t)> m_audio_port_opened_cb, m_midi_port_opened_cb;
    std::function<void(std::string)> m_audio_port_closed_cb, m_midi_port_closed_cb;

public:

    DummyAudioMidiDriver();
    virtual ~DummyAudioMidiDriver();

    void start(AudioMidiDriverSettingsInterface &settings) override;

    std::shared_ptr<AudioPortInterface<audio_sample_t>> open_audio_port(
        std::string name,
        shoop_port_direction_t direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) override;

    void close() override;

    void pause();
    void resume();

    // Usually the dummy audio system will automatically process samples
    // continuously. When in controlled mode, instead the process callback
    // will always process 0 samples unless a specific amount of samples
    // is requested.
    // Process callback do still keep happening so that process thread
    // commands are still processed.
    void enter_mode(DummyAudioMidiDriverMode mode);
    DummyAudioMidiDriverMode get_mode() const;

    // In controlled mode, this amount of samples will be requested for
    // processing. If the amount is larger than the default buffer size,
    // it is processed in multiple iterations. If it is smaller, the process
    // thread will process exactly this amount.
    void controlled_mode_request_samples(uint32_t samples);
    uint32_t get_controlled_mode_samples_to_process() const;

    // Run until the requested amount of samples has been completed.
    void controlled_mode_run_request(uint32_t timeout_ms = 100);
};

extern template class DummyAudioMidiDriver<uint32_t, uint16_t>;
extern template class DummyAudioMidiDriver<uint32_t, uint32_t>;
extern template class DummyAudioMidiDriver<uint16_t, uint16_t>;
extern template class DummyAudioMidiDriver<uint16_t, uint32_t>;
extern template class DummyAudioMidiDriver<uint32_t, uint64_t>;
extern template class DummyAudioMidiDriver<uint64_t, uint64_t>;