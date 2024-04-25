#pragma once
#include "AudioMidiDriver.h"
#include "MidiPort.h"
#include "AudioPort.h"
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
#include "shoop_shared_ptr.h"


class DummyPort;
struct DummyExternalConnections : private ModuleLoggingEnabled<"Backend.DummyAudioMidiDriver"> {
    std::vector<std::pair<DummyPort*, std::string>> m_external_connections;
    std::vector<ExternalPortDescriptor> m_external_mock_ports;

    void add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type);
    void remove_external_mock_port(std::string name);
    void remove_all_external_mock_ports();

    void connect(DummyPort* port, std::string external_port_name);
    void disconnect(DummyPort* port, std::string external_port_name);

    ExternalPortDescriptor &get_port(std::string name);

    std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    );

    PortExternalConnectionStatus connection_status_of(const DummyPort* p);
};

class DummyPort : public virtual PortInterface {
protected:
    std::string m_name = "";
    shoop_port_direction_t m_direction = ShoopPortDirection_Input;
    shoop_weak_ptr<DummyExternalConnections> m_external_connections;

public:
    DummyPort(
        std::string name,
        shoop_port_direction_t direction,
        PortDataType type,
        shoop_weak_ptr<DummyExternalConnections> external_connections = shoop_weak_ptr<DummyExternalConnections>()
    );

    const char* name() const override;
    void close() override;
    void *maybe_driver_handle() const override;

    PortExternalConnectionStatus get_external_connection_status() const override;
    void connect_external(std::string name) override;
    void disconnect_external(std::string name) override;
};

class DummyAudioPort : public virtual AudioPort<audio_sample_t>,
                       public DummyPort,
                       private ModuleLoggingEnabled<"Backend.DummyAudioPort"> {

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

class DummyMidiPort : public virtual MidiPort,
                      public DummyPort,
                      public MidiReadableBufferInterface,
                      public MidiWriteableBufferInterface,
                      private ModuleLoggingEnabled<"Backend.DummyMidiPort"> {
public:
    using StoredMessage = MidiMessage<uint32_t, uint32_t>;
    
private:

    // Queued messages as external input to the port
    std::vector<StoredMessage> m_queued_msgs;
    std::atomic<uint32_t> current_buf_frames = 0;
    std::vector<StoredMessage> m_buffer_data;

    // Amount of frames requested for reading externally out of the port
    std::atomic<uint32_t> n_requested_frames = 0;

    std::atomic<uint32_t> n_processed_last_round = 0;
    std::atomic<uint32_t> n_original_requested_frames = 0;
    std::vector<StoredMessage> m_written_requested_msgs;

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
    using Log = ModuleLoggingEnabled<"Backend.DummyAudioMidiDriver">;

    std::atomic<bool> m_finish = false;
    std::atomic<DummyAudioMidiDriverMode> m_mode = DummyAudioMidiDriverMode::Automatic;
    std::atomic<uint32_t> m_controlled_mode_samples_to_process = 0;
    std::atomic<bool> m_paused = false;
    std::thread m_proc_thread;
    std::set<shoop_shared_ptr<DummyAudioPort>> m_audio_ports;
    std::set<shoop_shared_ptr<DummyMidiPort>> m_midi_ports;
    std::string m_client_name_str = "";

    std::function<void(std::string, shoop_port_direction_t)> m_audio_port_opened_cb = nullptr;
    std::function<void(std::string, shoop_port_direction_t)> m_midi_port_opened_cb = nullptr;
    std::function<void(std::string)> m_audio_port_closed_cb = nullptr;
    std::function<void(std::string)> m_midi_port_closed_cb = nullptr;

public:

    shoop_shared_ptr<DummyExternalConnections> m_external_connections;

    DummyAudioMidiDriver();
    virtual ~DummyAudioMidiDriver();

    void start(AudioMidiDriverSettingsInterface &settings) override;

    shoop_shared_ptr<AudioPort<audio_sample_t>> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        shoop_shared_ptr<typename AudioPort<audio_sample_t>::BufferPool> buffer_pool
    ) override;

    shoop_shared_ptr<MidiPort> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) override;

    void close() override;

    std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    ) override;

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

    void add_external_mock_port(std::string name, shoop_port_direction_t direction, shoop_port_data_type_t data_type);
    void remove_external_mock_port(std::string name);
    void remove_all_external_mock_ports();
};

extern template class DummyAudioMidiDriver<uint32_t, uint16_t>;
extern template class DummyAudioMidiDriver<uint32_t, uint32_t>;
extern template class DummyAudioMidiDriver<uint16_t, uint16_t>;
extern template class DummyAudioMidiDriver<uint16_t, uint32_t>;
extern template class DummyAudioMidiDriver<uint32_t, uint64_t>;
extern template class DummyAudioMidiDriver<uint64_t, uint64_t>;