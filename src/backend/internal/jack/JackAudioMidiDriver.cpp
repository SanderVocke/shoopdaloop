#include "JackAudioMidiDriver.h"
#include "AudioMidiDriver.h"
#include "LoggingBackend.h"
#include "MidiPort.h"
#include "PortInterface.h"
#include <chrono>
#include <cstring>
#include <stdexcept>
#include <memory>
#include <atomic>
#include <thread>
#include "jack/types.h"
#include "run_in_thread_with_timeout.h"
#include "JackAudioPort.h"
#include "JackMidiPort.h"
#include "types.h"

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
GenericJackAudioMidiDriver<API>::GenericJackAudioMidiDriver(void (*maybe_process_callback)()):
    AudioMidiDriver(maybe_process_callback),
    m_started(false),
    m_all_ports_tracker(shoop_make_shared<GenericJackAllPorts<API>>()) {}

template<typename API>
void GenericJackAudioMidiDriver<API>::start(
    AudioMidiDriverSettingsInterface &settings)
{
    auto p_settings = dynamic_cast<JackAudioMidiDriverSettings*>(&settings);
    if (!p_settings) { throw std::runtime_error("Wrong settings type passed to JACK driver"); }
    auto &_settings = *p_settings;

    API::init();

    jack_status_t status;

    Log::log<log_level_info>("Opening JACK client with name {}.", _settings.client_name_hint);
    auto client = API::client_open(_settings.client_name_hint.c_str(), JackNullOption, &status);

    if (client == nullptr) {
        Log::throw_error<std::runtime_error>("Unable to open JACK client.");
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

    // Processing the command queue once will ensure that it knows processing is active.
    // That way commands added from now on will be executed on the process thread.
    ma_queue.PROC_exec_all();

    if (API::activate(client)) {
        Log::throw_error<std::runtime_error>("Could not activate JACK client.");
    }

    set_maybe_client_handle((void*)client);
    set_client_name(API::get_client_name(client));
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
shoop_shared_ptr<AudioPort<float>>GenericJackAudioMidiDriver<API>::open_audio_port(std::string name, shoop_port_direction_t direction,
    shoop_shared_ptr<typename AudioPort<jack_default_audio_sample_t>::UsedBufferPool> buffer_pool) {
    shoop_shared_ptr<PortInterface> port =
        shoop_static_pointer_cast<PortInterface>(
            shoop_make_shared<GenericJackAudioPort<API>>(name, direction, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker, buffer_pool)
        );
    m_ports[port->name()] = port;
    return shoop_dynamic_pointer_cast<AudioPort<float>>(port);
}

template<typename API>
shoop_shared_ptr<MidiPort> GenericJackAudioMidiDriver<API>::open_midi_port(std::string name, shoop_port_direction_t direction) {
    shoop_shared_ptr<PortInterface> port;
    
    if (direction == ShoopPortDirection_Input) {
        port = shoop_static_pointer_cast<PortInterface>(
            shoop_make_shared<GenericJackMidiInputPort<API>>(name, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker)
        );
    } else {
        port = shoop_static_pointer_cast<PortInterface>(
            shoop_make_shared<GenericJackMidiOutputPort<API>>(name, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker)
        );
    }
    m_ports[port->name()] = port;
    return shoop_dynamic_pointer_cast<MidiPort>(port);
}

template<typename API>
void GenericJackAudioMidiDriver<API>::close() {
    if (get_maybe_client_handle()) {
        Log::log<log_level_debug>("Closing JACK client.");
        try {
            run_in_thread_with_timeout_unsafe([this]() { API::client_close((jack_client_t*)get_maybe_client_handle()); }, 10000);
        } catch (std::exception &e) {
            Log::log<log_level_warning>("Attempt to close JACK client failed: {}. Abandoning.", e.what());
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

template<typename API>
std::vector<ExternalPortDescriptor> GenericJackAudioMidiDriver<API>::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    const char* maybe_type_regex =
        (maybe_data_type_filter == ShoopPortDataType_Audio) ? JACK_DEFAULT_AUDIO_TYPE :
        (maybe_data_type_filter == ShoopPortDataType_Midi)  ? JACK_DEFAULT_MIDI_TYPE  :
        nullptr;
    
    unsigned flags =
        (maybe_direction_filter == ShoopPortDirection_Input)  ? JackPortIsInput  :
        (maybe_direction_filter == ShoopPortDirection_Output) ? JackPortIsOutput :
        0;

    auto client = (jack_client_t*) get_maybe_client_handle();
    const char** result = API::get_ports(client, maybe_name_regex, maybe_type_regex, flags);

    // First gather up the names
    std::vector<std::string> names;
    for(auto it = result; it != nullptr && *it != nullptr; it++) {
        ExternalPortDescriptor desc;
        names.push_back(std::string(*it));
    }
    if (result) { API::free((void*) result); }

    // Now fill in further data
    std::vector<ExternalPortDescriptor> rval;
    for (auto &n : names) {
        jack_port_t *p = API::port_by_name(client, n.c_str());
        if (p && !API::port_is_mine(client, p)) {
            ExternalPortDescriptor desc;
            desc.name = n;
            auto flags = API::port_flags(p);
            desc.direction = (flags & JackPortIsInput) ? ShoopPortDirection_Input : ShoopPortDirection_Output;
            auto type = API::port_type(p);
            if (!strcmp(type, JACK_DEFAULT_AUDIO_TYPE)) { desc.data_type = ShoopPortDataType_Audio; }
            else if (!strcmp(type, JACK_DEFAULT_MIDI_TYPE)) { desc.data_type = ShoopPortDataType_Midi; }
            else { continue; /* Ignore other types */ }
            rval.push_back(desc);
        }
    }

    return rval;
}

template<typename API>
void GenericJackAudioMidiDriver<API>::wait_process() {
    if (API::supports_processing) {
        AudioMidiDriver::wait_process();
    } else {
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
}

template class GenericJackAudioMidiDriver<JackApi>;
template class GenericJackAudioMidiDriver<JackTestApi>;