#pragma once
#include "AudioMidiDriver.h"
#include "AudioPort.h"
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
    std::map<std::string, shoop_shared_ptr<PortInterface>> m_ports;
    shoop_shared_ptr<GenericJackAllPorts<API>> m_all_ports_tracker = nullptr;
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

    void maybe_update_sample_rate() override;
    void maybe_update_buffer_size() override;
    void maybe_update_dsp_load() override;

    void wait_process() override;

public:
    GenericJackAudioMidiDriver();
    ~GenericJackAudioMidiDriver() override;
    
    void start(AudioMidiDriverSettingsInterface &settings) override;

    shoop_shared_ptr<AudioPort<float>> open_audio_port(
        std::string name,
        shoop_port_direction_t direction,
        shoop_shared_ptr<typename AudioPort<jack_default_audio_sample_t>::BufferPool>
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
};

using JackAudioMidiDriver = GenericJackAudioMidiDriver<JackApi>;
using JackTestAudioMidiDriver = GenericJackAudioMidiDriver<JackTestApi>;

extern template class GenericJackAudioMidiDriver<JackApi>;
extern template class GenericJackAudioMidiDriver<JackTestApi>;