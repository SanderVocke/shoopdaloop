#include "JackAudioMidiDriver.h"
#include "AudioMidiDriver.h"
#include "LoggingBackend.h"
#include "MidiPortInterface.h"
#include "PortInterface.h"
#include <stdexcept>
#include <memory>
#include <atomic>
#include "run_in_thread_with_timeout.h"
#include "JackAudioPort.h"
#include "JackMidiPort.h"

template<typename API>
int GenericJackAudioMidiDriver<API>::PROC_process_cb_static(jack_nframes_t nframes, void *arg) {
    auto &inst = *((GenericJackAudioMidiDriver<API> *)arg);
    return inst.PROC_process_cb_inst(nframes);
}

template<typename API>
int GenericJackAudioMidiDriver<API>::PROC_xrun_cb_static(void *arg) {
    auto &inst = *((GenericJackAudioMidiDriver<API> *)arg);
    return inst.PROC_xrun_cb_inst();
}

template<typename API>
void GenericJackAudioMidiDriver<API>::PROC_port_connect_cb_static(jack_port_id_t a, jack_port_id_t b, int connect, void *arg) {
    auto &inst = *((GenericJackAudioMidiDriver<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioMidiDriver<API>::PROC_port_registration_cb_static(jack_port_id_t port, int, void *arg) {
    auto &inst = *((GenericJackAudioMidiDriver<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioMidiDriver<API>::PROC_port_rename_cb_static(jack_port_id_t port, const char *old_name, const char *new_name, void *arg) {
    auto &inst = *((GenericJackAudioMidiDriver<API> *)arg);
    inst.PROC_update_ports_cb_inst();
}

template<typename API>
void GenericJackAudioMidiDriver<API>::error_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::log<"Backend.JackAudioMidiDriver", log_level_error>(std::nullopt, std::nullopt, _msg);
}

template<typename API>
void GenericJackAudioMidiDriver<API>::info_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::log<"Backend.JackAudioMidiDriver", log_level_info>(std::nullopt, std::nullopt, _msg);
}

template<typename API>
int GenericJackAudioMidiDriver<API>::PROC_process_cb_inst(jack_nframes_t nframes) {
    AudioMidiDriver::PROC_process(nframes);
    return 0;
}

template<typename API>
int GenericJackAudioMidiDriver<API>::PROC_xrun_cb_inst() {
    AudioMidiDriver::report_xrun();
    return 0;
}

template<typename API>
void GenericJackAudioMidiDriver<API>::PROC_update_ports_cb_inst() {
    m_all_ports_tracker->update((jack_client_t*) get_maybe_client_handle());
}

template<typename API>
GenericJackAudioMidiDriver<API>::GenericJackAudioMidiDriver():
    m_started(false),
    m_all_ports_tracker(std::make_shared<GenericJackAllPorts<API>>()) {}

template<typename API>
void GenericJackAudioMidiDriver<API>::start(
    AudioMidiDriverSettingsInterface &settings)
{
    auto p_settings = dynamic_cast<JackAudioMidiDriverSettings*>(&settings);
    if (!p_settings) { throw std::runtime_error("Wrong settings type passed to JACK driver"); }
    auto &_settings = *p_settings;

    API::init();

    jack_status_t status;

    log<log_level_info>("Opening JACK client with name {}.", _settings.client_name_hint);
    auto client = API::client_open(_settings.client_name_hint.c_str(), JackNullOption, &status);

    if (client == nullptr) {
        throw_error<std::runtime_error>("Unable to open JACK client.");
    }
    AudioMidiDriver::set_maybe_client_handle((void*) client);
    AudioMidiDriver::set_client_name(API::get_client_name(client));
    
    API::set_process_callback(client, GenericJackAudioMidiDriver<API>::PROC_process_cb_static,
                              (void *)this);
    API::set_xrun_callback(client, GenericJackAudioMidiDriver<API>::PROC_xrun_cb_static,
                           (void *)this);
    API::set_port_connect_callback(client,
                                   GenericJackAudioMidiDriver<API>::PROC_port_connect_cb_static,
                                   (void *)this);
    API::set_port_registration_callback(client,
                                   GenericJackAudioMidiDriver<API>::PROC_port_registration_cb_static,
                                   (void *)this);
    API::set_port_rename_callback(client,
                                   GenericJackAudioMidiDriver<API>::PROC_port_rename_cb_static,
                                   (void *)this);
    API::set_error_function(GenericJackAudioMidiDriver<API>::error_cb_static);
    API::set_info_function(GenericJackAudioMidiDriver<API>::info_cb_static);
    
    m_all_ports_tracker->update(client);


    if (API::activate(client)) {
        throw_error<std::runtime_error>("Could not activate JACK client.");
    }

    set_maybe_client_handle((void*)client);
    set_client_name(jack_get_client_name(client));
    set_dsp_load(0.0f);
    set_sample_rate(API::get_sample_rate(client));
    set_buffer_size(API::get_buffer_size(client));
    set_active(true);
}

template<typename API>
GenericJackAudioMidiDriver<API>::~GenericJackAudioMidiDriver() {
    close();
    AudioMidiDriver::set_maybe_client_handle(nullptr);
}

template<typename API>
std::shared_ptr<AudioPortInterface<float>>GenericJackAudioMidiDriver<API>::open_audio_port(std::string name, shoop_port_direction_t direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<GenericJackAudioPort<API>>(name, direction, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<AudioPortInterface<float>>(port);
}

template<typename API>
std::shared_ptr<MidiPortInterface> GenericJackAudioMidiDriver<API>::open_midi_port(std::string name, shoop_port_direction_t direction) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<GenericJackMidiPort<API>>(name, direction, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<MidiPortInterface>(port);
}

template<typename API>
void GenericJackAudioMidiDriver<API>::close() {
    if (get_maybe_client_handle()) {
        log<log_level_debug>("Closing JACK client.");
        try {
            run_in_thread_with_timeout_unsafe([this]() { API::client_close((jack_client_t*)get_maybe_client_handle()); }, 10000);
        } catch (std::exception &e) {
            log<log_level_warning>("Attempt to close JACK client failed: {}. Abandoning.", e.what());
        }
        set_maybe_client_handle(nullptr);
    }
}

template<typename API>
void GenericJackAudioMidiDriver<API>::maybe_update_sample_rate() {
    AudioMidiDriver::set_sample_rate(API::get_sample_rate((jack_client_t*) get_maybe_client_handle()));
}

template<typename API>
void GenericJackAudioMidiDriver<API>::maybe_update_buffer_size() {
    AudioMidiDriver::set_buffer_size(API::get_buffer_size((jack_client_t*) get_maybe_client_handle()));
}

template<typename API>
void GenericJackAudioMidiDriver<API>::maybe_update_dsp_load() {
    AudioMidiDriver::set_dsp_load(API::cpu_load((jack_client_t*) get_maybe_client_handle()));
};

template class GenericJackAudioMidiDriver<JackApi>;
template class GenericJackAudioMidiDriver<JackTestApi>;