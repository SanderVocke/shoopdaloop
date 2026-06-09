#pragma once
#include "../AudioMidiDriver.h"
#include "../RustAudioPort.h"
#include "JackAllPorts.h"
#include "../LoggingEnabled.h"
#include "backend_rust/src/jack_api_cxx.rs.h"
#include <jack/types.h>
#include <map>
#include <atomic>
#include <optional>
#include <cstdint>

struct JackAudioMidiDriverSettings : public AudioMidiDriverSettingsInterface {
    JackAudioMidiDriverSettings() {}

    std::string client_name_hint = "";
    std::optional<std::string> maybe_server_name_hint = std::nullopt;
};

class JackAudioMidiDriver :
    public AudioMidiDriver,
    private ModuleLoggingEnabled<"Backend.JackAudioMidiDriver">
{
    using Log = ModuleLoggingEnabled<"Backend.JackAudioMidiDriver">;
private:
    rust::Box<backend_rust::JackApiBridgeStrong> m_api;
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::shared_ptr<JackAllPorts> m_all_ports_tracker = nullptr;
    std::atomic<bool> m_started = false;

    void maybe_update_sample_rate();
    void maybe_update_buffer_size();
    void maybe_update_dsp_load();

public:
    explicit JackAudioMidiDriver(
        void (*maybe_process_callback)()=nullptr,
        rust::Box<backend_rust::JackApiBridgeStrong> api=backend_rust::new_jack_api()
    );
    ~JackAudioMidiDriver() override;

    int PROC_process_cb_inst (uint32_t nframes);
    int PROC_xrun_cb_inst ();
    void PROC_update_ports_cb_inst();

    rust::Box<backend_rust::JackApiBridgeStrong> clone_jack_api() const;

    void start(AudioMidiDriverSettingsInterface &settings) override;

    std::shared_ptr<RustAudioPortF32> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        std::shared_ptr<RustAudioPortF32::UsedBufferPool>
    ) override;

    std::shared_ptr<MidiPort> open_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) override;

    rust::Box<backend_rust::DecoupledMidiPortBridgeStrong> open_decoupled_midi_port(
        std::string name,
        shoop_port_direction_t direction
    ) override;

    void close() override;

    void add_processor(std::shared_ptr<IProcessor> p) override;
    void remove_processor(std::shared_ptr<IProcessor> p) override;
    std::vector<std::unique_ptr<ProcessorBridgeWeak>> processors() const override;

    uint32_t get_xruns() const override;
    float get_dsp_load() override;
    uint32_t get_sample_rate() override;
    uint32_t get_buffer_size() override;
    void reset_xruns() override;
    const char* get_client_name() const override;
    void* get_maybe_client_handle() const override;
    bool get_active() const override;
    uint32_t get_last_processed() const override;

    void wait_process() override;

    void queue_process_thread_command(std::function<void()> fn) override;
    void exec_process_thread_command(std::function<void()> fn) override;
    backend_rust::CommandQueue &get_command_queue() override;

    std::vector<ExternalPortDescriptor> find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    ) override;
};

class JackTestAudioMidiDriver : public JackAudioMidiDriver {
public:
    explicit JackTestAudioMidiDriver(void (*maybe_process_callback)()=nullptr);
};
