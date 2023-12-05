#pragma once
#include "AudioMidiDriver.h"
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

    std::string client_name_hint;
    std::optional<std::string> maybe_server_name_hint;
};

template<typename API>
class GenericJackAudioMidiDriver :
    public AudioMidiDriver,
    private ModuleLoggingEnabled<"Backend.JackAudioMidiDriver">
{
private:
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::shared_ptr<GenericJackAllPorts<API>> m_all_ports_tracker;
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

public:
    GenericJackAudioMidiDriver();
    ~GenericJackAudioMidiDriver() override;

    bool started() const override;
    void start(AudioMidiDriverSettingsInterface &settings) override;

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    void close() override;
};

using JackAudioMidiDriver = GenericJackAudioMidiDriver<JackApi>;
using JackTestAudioMidiDriver = GenericJackAudioMidiDriver<JackTestApi>;

extern template class GenericJackAudioMidiDriver<JackApi>;
extern template class GenericJackAudioMidiDriver<JackTestApi>;