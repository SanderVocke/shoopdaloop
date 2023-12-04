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
    jack_client_t * m_client;
    std::string m_client_name;
    uint32_t m_sample_rate;
    uint32_t m_buffer_size;
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::function<void(uint32_t)> m_process_cb;
    std::atomic<unsigned> m_xruns = 0;
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

public:
    GenericJackAudioMidiDriver();
    ~GenericJackAudioMidiDriver() override;

    bool started() const override;
    void start(AudioMidiDriverSettingsInterface &settings,
               std::function<void(uint32_t)> process_cb) override;

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    shoop_audio_driver_state_t get_state() const override;
    
    void close() override;

    void reset_xruns() override;
};

using JackAudioMidiDriver = GenericJackAudioMidiDriver<JackApi>;
using JackTestAudioMidiDriver = GenericJackAudioMidiDriver<JackTestApi>;

extern template class GenericJackAudioMidiDriver<JackApi>;
extern template class GenericJackAudioMidiDriver<JackTestApi>;