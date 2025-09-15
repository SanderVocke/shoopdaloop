#pragma once
#include "AudioMidiDriver.h"
#include "MidiPort.h"
#include "AudioPort.h"
#include "DummyAudioPort.h"
#include "DummyMidiPort.h"
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

    DummyAudioMidiDriver(void (*maybe_process_callback)() = nullptr);
    virtual ~DummyAudioMidiDriver();

    void start(AudioMidiDriverSettingsInterface &settings) override;

    shoop_shared_ptr<AudioPort<audio_sample_t>> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        shoop_shared_ptr<typename AudioPort<audio_sample_t>::UsedBufferPool> buffer_pool
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