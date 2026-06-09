#include "JackAudioMidiDriver.h"
#include "RustCommandQueue.h"
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

int JackAudioMidiDriver::PROC_process_cb_static(jack_nframes_t nframes, void *arg) {
    auto &inst = *((JackAudioMidiDriver *)arg);
    return inst.PROC_process_cb_inst(nframes);
}

int JackAudioMidiDriver::PROC_xrun_cb_static(void *arg) {
    auto &inst = *((JackAudioMidiDriver *)arg);
    return inst.PROC_xrun_cb_inst();
}

void JackAudioMidiDriver::PROC_port_connect_cb_static(jack_port_id_t a, jack_port_id_t b, int connect, void *arg) {
    (void)a; (void)b; (void)connect;
    auto &inst = *((JackAudioMidiDriver *)arg);
    inst.PROC_update_ports_cb_inst();
}

void JackAudioMidiDriver::PROC_port_registration_cb_static(jack_port_id_t port, int, void *arg) {
    (void)port;
    auto &inst = *((JackAudioMidiDriver *)arg);
    inst.PROC_update_ports_cb_inst();
}

void JackAudioMidiDriver::PROC_port_rename_cb_static(jack_port_id_t port, const char *old_name, const char *new_name, void *arg) {
    (void)port; (void)old_name; (void)new_name;
    auto &inst = *((JackAudioMidiDriver *)arg);
    inst.PROC_update_ports_cb_inst();
}

void JackAudioMidiDriver::error_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::log<"Backend.JackAudioMidiDriver", log_level_error>(std::nullopt, std::nullopt, _msg);
}

void JackAudioMidiDriver::info_cb_static(const char *msg) {
    std::string _msg = "JACK error: " + std::string(msg);
    logging::log<"Backend.JackAudioMidiDriver", log_level_info>(std::nullopt, std::nullopt, _msg);
}

int JackAudioMidiDriver::PROC_process_cb_inst(jack_nframes_t nframes) {
    process(nframes);
    return 0;
}

int JackAudioMidiDriver::PROC_xrun_cb_inst() {
    report_xrun();
    return 0;
}

void JackAudioMidiDriver::PROC_update_ports_cb_inst() {
    m_all_ports_tracker->update((jack_client_t*) get_maybe_client_handle());
}

JackAudioMidiDriver::JackAudioMidiDriver(void (*maybe_process_callback)(), std::shared_ptr<IJackApi> api):
    AudioMidiDriver(maybe_process_callback),
    m_api(std::move(api)),
    m_started(false),
    m_all_ports_tracker(std::make_shared<JackAllPorts>(m_api)) {}

JackTestAudioMidiDriver::JackTestAudioMidiDriver(void (*maybe_process_callback)())
    : JackAudioMidiDriver(maybe_process_callback, std::make_shared<JackTestApiInterface>()) {}

void JackAudioMidiDriver::start(
    AudioMidiDriverSettingsInterface &settings)
{
    auto p_settings = dynamic_cast<JackAudioMidiDriverSettings*>(&settings);
    if (!p_settings) { throw std::runtime_error("Wrong settings type passed to JACK driver"); }
    auto &_settings = *p_settings;

    m_api->init();

    jack_status_t status;

    Log::log<log_level_info>("Opening JACK client with name {}.", _settings.client_name_hint);
    auto client = m_api->client_open(_settings.client_name_hint.c_str(), JackNullOption, &status);

    if (client == nullptr) {
        Log::throw_error<std::runtime_error>("Unable to open JACK client.");
    }
    set_maybe_client_handle((void*) client);
    set_client_name(m_api->get_client_name(client));

    m_api->set_process_callback(client, JackAudioMidiDriver::PROC_process_cb_static,
                              (void *)this);
    m_api->set_xrun_callback(client, JackAudioMidiDriver::PROC_xrun_cb_static,
                           (void *)this);
    m_api->set_port_connect_callback(client,
                                   JackAudioMidiDriver::PROC_port_connect_cb_static,
                                   (void *)this);
    m_api->set_port_registration_callback(client,
                                   JackAudioMidiDriver::PROC_port_registration_cb_static,
                                   (void *)this);
    m_api->set_port_rename_callback(client,
                                   JackAudioMidiDriver::PROC_port_rename_cb_static,
                                   (void *)this);
    m_api->set_error_function(JackAudioMidiDriver::error_cb_static);
    m_api->set_info_function(JackAudioMidiDriver::info_cb_static);

    m_all_ports_tracker->update(client);

    // Processing the command queue once will ensure that it knows processing is active.
    // That way commands added from now on will be executed on the process thread.
    rust_command_queue::exec_all(this->get_command_queue());

    if (m_api->activate(client)) {
        Log::throw_error<std::runtime_error>("Could not activate JACK client.");
    }

    set_maybe_client_handle((void*)client);
    set_client_name(m_api->get_client_name(client));
    set_dsp_load(0.0f);
    set_sample_rate(m_api->get_sample_rate(client));
    set_buffer_size(m_api->get_buffer_size(client));
    set_active(true);
}

JackAudioMidiDriver::~JackAudioMidiDriver() {
    close();
    set_maybe_client_handle(nullptr);
}

std::shared_ptr<RustAudioPortF32>JackAudioMidiDriver::open_audio_port(std::string name, shoop_port_direction_t direction,
    std::shared_ptr<RustAudioPortF32::UsedBufferPool> buffer_pool) {
    std::shared_ptr<PortInterface> port =
        std::static_pointer_cast<PortInterface>(
            std::make_shared<JackAudioPort>(name, direction, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker, m_api, buffer_pool)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<RustAudioPortF32>(port);
}

std::shared_ptr<MidiPort> JackAudioMidiDriver::open_midi_port(std::string name, shoop_port_direction_t direction) {
    std::shared_ptr<PortInterface> port;

    if (direction == ShoopPortDirection_Input) {
        port = std::static_pointer_cast<PortInterface>(
            std::make_shared<JackMidiInputPort>(name, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker, m_api)
        );
    } else {
        port = std::static_pointer_cast<PortInterface>(
            std::make_shared<JackMidiOutputPort>(name, (jack_client_t*)get_maybe_client_handle(), m_all_ports_tracker, m_api)
        );
    }
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<MidiPort>(port);
}

void JackAudioMidiDriver::close() {
    if (get_maybe_client_handle()) {
        Log::log<log_level_debug>("Closing JACK client.");
        try {
            // First deactivate the client to stop the process callback
            run_in_thread_with_timeout_unsafe([this]() { m_api->deactivate((jack_client_t*)get_maybe_client_handle()); }, 5000);
        } catch (std::exception &e) {
            Log::log<log_level_warning>("Attempt to deactivate JACK client failed: {}. Continuing with close.", e.what());
        }
        try {
            // Then close the client
            run_in_thread_with_timeout_unsafe([this]() { m_api->client_close((jack_client_t*)get_maybe_client_handle()); }, 5000);
        } catch (std::exception &e) {
            Log::log<log_level_warning>("Attempt to close JACK client failed: {}. Abandoning.", e.what());
        }
        set_maybe_client_handle(nullptr);
    }
}

void JackAudioMidiDriver::maybe_update_sample_rate() {
    set_sample_rate(m_api->get_sample_rate((jack_client_t*) get_maybe_client_handle()));
}

void JackAudioMidiDriver::maybe_update_buffer_size() {
    set_buffer_size(m_api->get_buffer_size((jack_client_t*) get_maybe_client_handle()));
}

void JackAudioMidiDriver::maybe_update_dsp_load() {
    set_dsp_load(m_api->cpu_load((jack_client_t*) get_maybe_client_handle()));
};

std::vector<ExternalPortDescriptor> JackAudioMidiDriver::find_external_ports(
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
    const char** result = m_api->get_ports(client, maybe_name_regex, maybe_type_regex, flags);

    // First gather up the names
    std::vector<std::string> names;
    for(auto it = result; it != nullptr && *it != nullptr; it++) {
        ExternalPortDescriptor desc;
        names.push_back(std::string(*it));
    }
    if (result) { m_api->free((void*) result); }

    // Now fill in further data
    std::vector<ExternalPortDescriptor> rval;
    for (auto &n : names) {
        jack_port_t *p = m_api->port_by_name(client, n.c_str());
        if (p && !m_api->port_is_mine(client, p)) {
            ExternalPortDescriptor desc;
            desc.name = n;
            auto flags = m_api->port_flags(p);
            desc.direction = (flags & JackPortIsInput) ? ShoopPortDirection_Input : ShoopPortDirection_Output;
            auto type = m_api->port_type(p);
            if (!strcmp(type, JACK_DEFAULT_AUDIO_TYPE)) { desc.data_type = ShoopPortDataType_Audio; }
            else if (!strcmp(type, JACK_DEFAULT_MIDI_TYPE)) { desc.data_type = ShoopPortDataType_Midi; }
            else { continue; /* Ignore other types */ }
            rval.push_back(desc);
        }
    }

    return rval;
}

void JackAudioMidiDriver::wait_process() {
    if (m_api->supports_processing()) {
        AudioMidiDriver::wait_process();
    } else {
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
}

rust::Box<backend_rust::DecoupledMidiPortBridgeStrong> JackAudioMidiDriver::open_decoupled_midi_port(std::string name, shoop_port_direction_t direction) {
    auto port = open_midi_port(name, direction);
    return make_decoupled_midi_port(port, this->weak_driver_from_this(), direction);
}

void JackAudioMidiDriver::add_processor(std::shared_ptr<IProcessor> p) { AudioMidiDriver::add_processor(p); }

void JackAudioMidiDriver::remove_processor(std::shared_ptr<IProcessor> p) { AudioMidiDriver::remove_processor(p); }

std::vector<std::unique_ptr<ProcessorBridgeWeak>> JackAudioMidiDriver::processors() const { return AudioMidiDriver::processors(); }

uint32_t JackAudioMidiDriver::get_xruns() const { return AudioMidiDriver::get_xruns(); }

float JackAudioMidiDriver::get_dsp_load() { maybe_update_dsp_load(); return AudioMidiDriver::get_dsp_load(); }

uint32_t JackAudioMidiDriver::get_sample_rate() { maybe_update_sample_rate(); return AudioMidiDriver::get_sample_rate(); }

uint32_t JackAudioMidiDriver::get_buffer_size() { maybe_update_buffer_size(); return AudioMidiDriver::get_buffer_size(); }

void JackAudioMidiDriver::reset_xruns() { AudioMidiDriver::reset_xruns(); }

const char* JackAudioMidiDriver::get_client_name() const { return AudioMidiDriver::get_client_name(); }

void* JackAudioMidiDriver::get_maybe_client_handle() const { return AudioMidiDriver::get_maybe_client_handle(); }

bool JackAudioMidiDriver::get_active() const { return AudioMidiDriver::get_active(); }

uint32_t JackAudioMidiDriver::get_last_processed() const { return AudioMidiDriver::get_last_processed(); }

void JackAudioMidiDriver::queue_process_thread_command(std::function<void()> fn) { AudioMidiDriver::queue_process_thread_command(std::move(fn)); }

void JackAudioMidiDriver::exec_process_thread_command(std::function<void()> fn) { AudioMidiDriver::exec_process_thread_command(std::move(fn)); }

backend_rust::CommandQueue &JackAudioMidiDriver::get_command_queue() { return AudioMidiDriver::get_command_queue(); }
