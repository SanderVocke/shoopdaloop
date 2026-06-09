#pragma once
#include "AudioMidiDriver.h"
#include "RustAudioPort.h"
#include "JackAllPorts.h"
#include "JackApi.h"
#include "JackTestApi.h"
#include "LoggingEnabled.h"
#include <jack/types.h>
#include <map>
#include <atomic>
#include <optional>

struct JackAudioMidiDriverSettings : public AudioMidiDriverSettingsInterface {
    JackAudioMidiDriverSettings() {}

    std::string client_name_hint = "";
    std::optional<std::string> maybe_server_name_hint = std::nullopt;
};

template<typename API>
class GenericJackAudioMidiDriver :
    public AudioMidiDriver,
    private ModuleLoggingEnabled<"Backend.JackAudioMidiDriver">
{
    using Log = ModuleLoggingEnabled<"Backend.JackAudioMidiDriver">;
private:
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::shared_ptr<GenericJackAllPorts<API>> m_all_ports_tracker = nullptr;
    std::atomic<bool> m_started = false;

    static int PROC_process_cb_static (uint32_t nframes,
                                  void *arg);
    static int PROC_xrun_cb_static(void *arg);
    static void PROC_port_connect_cb_static(jack_port_id_t a, jack_port_id_t b, int connect, void *arg);
    static void PROC_port_registration_cb_static(jack_port_id_t port, int, void *arg);
    static void PROC_port_rename_cb_static(jack_port_id_t port, const char *old_name, const char *new_name, void *arg);

    int PROC_process_cb_inst (uint32_t nframes);
    int PROC_xrun_cb_inst ();
    void PROC_update_ports_cb_inst();

    static void error_cb_static(const char* msg);
    static void info_cb_static(const char* msg);

    void maybe_update_sample_rate();
    void maybe_update_buffer_size();
    void maybe_update_dsp_load();

public:
    GenericJackAudioMidiDriver(void (*maybe_process_callback)()=nullptr);
    ~GenericJackAudioMidiDriver() override;
    
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

using JackAudioMidiDriver = GenericJackAudioMidiDriver<JackApi>;
using JackTestAudioMidiDriver = GenericJackAudioMidiDriver<JackTestApi>;

extern template class GenericJackAudioMidiDriver<JackApi>;
extern template class GenericJackAudioMidiDriver<JackTestApi>;