#pragma once
#include "AudioSystemInterface.h"
#include "JackAllPorts.h"
#include "JackApi.h"
#include "JackTestApi.h"
#include "LoggingEnabled.h"
#include <jack/types.h>
#include <map>
#include <atomic>
#include <optional>

template<typename API>
class GenericJackAudioSystem :
    public AudioSystemInterface,
    private ModuleLoggingEnabled
{
private:
    std::string log_module_name() const override;

    jack_client_t * m_client;
    std::string m_client_name;
    uint32_t m_sample_rate;
    std::map<std::string, std::shared_ptr<PortInterface>> m_ports;
    std::function<void(uint32_t)> m_process_cb;
    std::atomic<unsigned> m_xruns = 0;
    std::shared_ptr<GenericJackAllPorts<API>> m_all_ports_tracker;

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
    GenericJackAudioSystem(
        std::string client_name,
        std::function<void(uint32_t)> process_cb
    );

    void start() override;

    ~GenericJackAudioSystem() override;

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    uint32_t get_sample_rate() const override;
    uint32_t get_buffer_size() const override;
    void* maybe_client_handle() const override;
    const char* client_name() const override;
    void close() override;
    uint32_t get_xruns() const override;
    void reset_xruns() override;
};

using JackAudioSystem = GenericJackAudioSystem<JackApi>;
using JackTestAudioSystem = GenericJackAudioSystem<JackTestApi>;

extern template class GenericJackAudioSystem<JackApi>;
extern template class GenericJackAudioSystem<JackTestApi>;