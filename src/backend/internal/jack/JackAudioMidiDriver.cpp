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
#include "run_in_thread_with_timeout.h"
#include "JackAudioPort.h"
#include "JackMidiPort.h"
#include "types.h"

int JackAudioMidiDriver::PROC_process_cb_inst(jack_nframes_t nframes) {
    process(nframes);
    return 0;
}

int JackAudioMidiDriver::PROC_xrun_cb_inst() {
    report_xrun();
    return 0;
}

void JackAudioMidiDriver::PROC_update_ports_cb_inst() {
    m_all_ports_tracker->update(reinterpret_cast<uintptr_t>(get_maybe_client_handle()));
}

JackAudioMidiDriver::JackAudioMidiDriver(
    void (*maybe_process_callback)(),
    rust::Box<backend_rust::JackApiBridgeStrong> api
):
    AudioMidiDriver(maybe_process_callback),
    m_api(std::move(api)),
    m_started(false),
    m_all_ports_tracker(std::make_shared<JackAllPorts>(m_api->clone_strong())) {}

JackTestAudioMidiDriver::JackTestAudioMidiDriver(void (*maybe_process_callback)())
    : JackAudioMidiDriver(maybe_process_callback, backend_rust::new_jack_test_api()) {}

rust::Box<backend_rust::JackApiBridgeStrong> JackAudioMidiDriver::clone_jack_api() const {
    return m_api->clone_strong();
}

void JackAudioMidiDriver::start(
    AudioMidiDriverSettingsInterface &settings)
{
    auto p_settings = dynamic_cast<JackAudioMidiDriverSettings*>(&settings);
    if (!p_settings) { throw std::runtime_error("Wrong settings type passed to JACK driver"); }
    auto &_settings = *p_settings;

    backend_rust::jack_api_init(*m_api);

    Log::log<log_level_info>("Opening JACK client with name {}.", _settings.client_name_hint);
    auto client = backend_rust::jack_api_client_open(*m_api, _settings.client_name_hint.c_str());

    if (client == 0) {
        Log::throw_error<std::runtime_error>("Unable to open JACK client.");
    }
    set_maybe_client_handle(reinterpret_cast<void*>(client));
    auto initial_client_name = backend_rust::jack_api_get_client_name(*m_api, client);
    set_client_name(initial_client_name.c_str());

    backend_rust::jack_api_set_process_callback(*m_api, client, reinterpret_cast<uintptr_t>(this));
    backend_rust::jack_api_set_xrun_callback(*m_api, client, reinterpret_cast<uintptr_t>(this));
    backend_rust::jack_api_set_port_connect_callback(*m_api, client, reinterpret_cast<uintptr_t>(this));
    backend_rust::jack_api_set_port_registration_callback(*m_api, client, reinterpret_cast<uintptr_t>(this));
    backend_rust::jack_api_set_port_rename_callback(*m_api, client, reinterpret_cast<uintptr_t>(this));
    backend_rust::jack_api_set_error_info_logging(*m_api);

    m_all_ports_tracker->update(client);

    rust_command_queue::exec_all(this->get_command_queue());

    if (backend_rust::jack_api_activate(*m_api, client)) {
        Log::throw_error<std::runtime_error>("Could not activate JACK client.");
    }

    set_maybe_client_handle(reinterpret_cast<void*>(client));
    auto client_name = backend_rust::jack_api_get_client_name(*m_api, client);
    set_client_name(client_name.c_str());
    set_dsp_load(0.0f);
    set_sample_rate(backend_rust::jack_api_get_sample_rate(*m_api, client));
    set_buffer_size(backend_rust::jack_api_get_buffer_size(*m_api, client));
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
            std::make_shared<JackAudioPort>(
                name,
                direction,
                reinterpret_cast<uintptr_t>(get_maybe_client_handle()),
                m_all_ports_tracker,
                m_api->clone_strong(),
                buffer_pool)
        );
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<RustAudioPortF32>(port);
}

std::shared_ptr<MidiPort> JackAudioMidiDriver::open_midi_port(std::string name, shoop_port_direction_t direction) {
    std::shared_ptr<PortInterface> port;

    if (direction == ShoopPortDirection_Input) {
        port = std::static_pointer_cast<PortInterface>(
            std::make_shared<JackMidiInputPort>(
                name,
                reinterpret_cast<uintptr_t>(get_maybe_client_handle()),
                m_all_ports_tracker,
                m_api->clone_strong())
        );
    } else {
        port = std::static_pointer_cast<PortInterface>(
            std::make_shared<JackMidiOutputPort>(
                name,
                reinterpret_cast<uintptr_t>(get_maybe_client_handle()),
                m_all_ports_tracker,
                m_api->clone_strong())
        );
    }
    m_ports[port->name()] = port;
    return std::dynamic_pointer_cast<MidiPort>(port);
}

void JackAudioMidiDriver::close() {
    if (get_maybe_client_handle()) {
        Log::log<log_level_debug>("Closing JACK client.");
        auto client = reinterpret_cast<uintptr_t>(get_maybe_client_handle());
        try {
            run_in_thread_with_timeout_unsafe([this, client]() { backend_rust::jack_api_deactivate(*m_api, client); }, 5000);
        } catch (std::exception &e) {
            Log::log<log_level_warning>("Attempt to deactivate JACK client failed: {}. Continuing with close.", e.what());
        }
        try {
            run_in_thread_with_timeout_unsafe([this, client]() { backend_rust::jack_api_client_close(*m_api, client); }, 5000);
        } catch (std::exception &e) {
            Log::log<log_level_warning>("Attempt to close JACK client failed: {}. Abandoning.", e.what());
        }
        set_maybe_client_handle(nullptr);
    }
}

void JackAudioMidiDriver::maybe_update_sample_rate() {
    set_sample_rate(backend_rust::jack_api_get_sample_rate(*m_api, reinterpret_cast<uintptr_t>(get_maybe_client_handle())));
}

void JackAudioMidiDriver::maybe_update_buffer_size() {
    set_buffer_size(backend_rust::jack_api_get_buffer_size(*m_api, reinterpret_cast<uintptr_t>(get_maybe_client_handle())));
}

void JackAudioMidiDriver::maybe_update_dsp_load() {
    set_dsp_load(backend_rust::jack_api_cpu_load(*m_api, reinterpret_cast<uintptr_t>(get_maybe_client_handle())));
};

std::vector<ExternalPortDescriptor> JackAudioMidiDriver::find_external_ports(
        const char* maybe_name_regex,
        shoop_port_direction_t maybe_direction_filter,
        shoop_port_data_type_t maybe_data_type_filter
    )
{
    auto client = reinterpret_cast<uintptr_t>(get_maybe_client_handle());
    auto names = backend_rust::jack_api_get_filtered_ports(
        *m_api,
        client,
        maybe_name_regex ? maybe_name_regex : "",
        maybe_name_regex != nullptr,
        static_cast<uint32_t>(maybe_direction_filter),
        maybe_direction_filter != ShoopPortDirection_Any,
        static_cast<uint32_t>(maybe_data_type_filter),
        maybe_data_type_filter != ShoopPortDataType_Any);

    std::vector<ExternalPortDescriptor> rval;
    for (auto const &n : names) {
        auto name = std::string(n);
        auto p = backend_rust::jack_api_port_by_name(*m_api, client, name);
        if (p && !backend_rust::jack_api_port_is_mine(*m_api, client, p)) {
            ExternalPortDescriptor desc;
            desc.name = name;
            desc.direction = backend_rust::jack_api_port_is_input(*m_api, p)
                ? ShoopPortDirection_Input
                : ShoopPortDirection_Output;
            if (backend_rust::jack_api_port_is_audio(*m_api, p)) { desc.data_type = ShoopPortDataType_Audio; }
            else if (backend_rust::jack_api_port_is_midi(*m_api, p)) { desc.data_type = ShoopPortDataType_Midi; }
            else { continue; }
            rval.push_back(desc);
        }
    }

    return rval;
}

void JackAudioMidiDriver::wait_process() {
    if (backend_rust::jack_api_supports_processing(*m_api)) {
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
