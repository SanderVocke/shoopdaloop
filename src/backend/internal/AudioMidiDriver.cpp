#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include <cstdint>
#include "shoop_globals.h"
#include <thread>

AudioMidiDriver::AudioMidiDriver(void (*maybe_process_callback)()) :
    AudioMidiDriver(
        maybe_process_callback ? +[](void* user) {
            auto fn = reinterpret_cast<void (*)()>(user);
            fn();
        } : nullptr,
        reinterpret_cast<void*>(maybe_process_callback)
    ) {}

AudioMidiDriver::AudioMidiDriver(ProcessHook maybe_process_hook, void* maybe_process_hook_user) :
  m_rust_core(backend_rust::new_audio_midi_driver_core()),
  m_command_queue(shoop_constants::command_queue_size, 1000, 1000),
  m_processors(shoop_make_shared<std::vector<shoop_weak_ptr<HasAudioProcessingFunction>>>()),
  m_maybe_process_hook(maybe_process_hook),
  m_maybe_process_hook_user(maybe_process_hook_user)
{
    // Set default client name in Rust core
    m_rust_core->set_client_name("unknown");
}

void AudioMidiDriver::add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    auto old = m_processors;
    auto _new = shoop_make_shared<std::vector<shoop_weak_ptr<HasAudioProcessingFunction>>>();
    *_new = *old;
    _new->push_back(p);
    m_processors = _new;
    register_processor_handle(reinterpret_cast<uintptr_t>(p.get()));
}

void AudioMidiDriver::remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    auto old = m_processors;
    auto _new = shoop_make_shared<std::vector<shoop_weak_ptr<HasAudioProcessingFunction>>>();
    for (auto _p : *old) {
        if (auto __p = _p.lock()) {
            if (__p != p) { _new->push_back(__p); }
        }
    }
    m_processors = _new;
    unregister_processor_handle(reinterpret_cast<uintptr_t>(p.get()));
}

std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> AudioMidiDriver::processors() const {
    return *m_processors;
}

void AudioMidiDriver::register_processor_handle(uintptr_t handle) {
    m_rust_core->add_processor(handle);
}

void AudioMidiDriver::unregister_processor_handle(uintptr_t handle) {
    m_rust_core->remove_processor(handle);
}

std::vector<uintptr_t> AudioMidiDriver::processor_handles() const {
    auto vals = m_rust_core->get_processors();
    std::vector<uintptr_t> out;
    out.reserve(vals.size());
    for (size_t i = 0; i < vals.size(); i++) { out.push_back(vals[i]); }
    return out;
}

void AudioMidiDriver::PROC_process(uint32_t nframes) {
    log<log_level_debug_trace>("AudioMidiDriver::process {}", nframes);
    if (m_maybe_process_hook) {
        m_maybe_process_hook(m_maybe_process_hook_user);
    }
    m_command_queue.PROC_handle_command_queue();
    PROC_process_decoupled_midi_ports(nframes);
    // Migration path: process via handle snapshot from Rust core to avoid
    // interface smart-pointer coupling in the RT loop.
    for (auto handle : processor_handles()) {
        if (!handle) { continue; }
        auto* p = reinterpret_cast<HasAudioProcessingFunction*>(handle);
        p->PROC_process(nframes);
    }
    set_last_processed(nframes);
}

uint32_t AudioMidiDriver::get_xruns() const {
    return m_rust_core->get_xruns();
}

float AudioMidiDriver::get_dsp_load() {
    maybe_update_dsp_load();
    return m_rust_core->get_dsp_load();
}

void AudioMidiDriver::unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    unregister_decoupled_midi_port_handle(reinterpret_cast<uintptr_t>(port.get()));
    m_command_queue.exec_process_thread_command([this, port]() {
        m_decoupled_midi_ports.erase(port);
    });
}

void AudioMidiDriver::PROC_process_decoupled_midi_ports(uint32_t nframes) {
    auto lock = m_decoupled_midi_ports;
    for(auto &p : lock) {
        p->PROC_process(nframes);
    }
}

uint32_t AudioMidiDriver::get_sample_rate() {
    maybe_update_sample_rate();
    return m_rust_core->get_sample_rate();
}

uint32_t AudioMidiDriver::get_buffer_size() {
    maybe_update_buffer_size();
    return m_rust_core->get_buffer_size();
}

void AudioMidiDriver::reset_xruns() {
    m_rust_core->reset_xruns();
}

void AudioMidiDriver::report_xrun() {
    m_rust_core->report_xrun();
}

void AudioMidiDriver::set_dsp_load(float load) {
    m_rust_core->set_dsp_load(load);
}

void AudioMidiDriver::set_sample_rate(uint32_t sample_rate) {
    m_rust_core->set_sample_rate(sample_rate);
}

void AudioMidiDriver::set_buffer_size(uint32_t buffer_size) {
    m_rust_core->set_buffer_size(buffer_size);
}

void AudioMidiDriver::set_client_name(const char* name) {
    m_rust_core->set_client_name(name);
}

void AudioMidiDriver::set_active(bool active) {
    m_rust_core->set_active(active);
}

void AudioMidiDriver::set_last_processed(uint32_t nframes) {
    m_rust_core->set_last_processed(nframes);
}

const char* AudioMidiDriver::get_client_name() const {
    static std::string cached_name;
    cached_name = std::string(m_rust_core->get_client_name());
    return cached_name.c_str();
}

void AudioMidiDriver::set_maybe_client_handle(void* handle) {
    m_rust_core->set_client_handle(reinterpret_cast<uintptr_t>(handle));
}

void* AudioMidiDriver::get_maybe_client_handle() const {
    return reinterpret_cast<void*>(m_rust_core->get_client_handle());
}

bool AudioMidiDriver::get_active() const {
    return m_rust_core->get_active();
}

uint32_t AudioMidiDriver::get_last_processed() const {
    return m_rust_core->get_last_processed();
}

void AudioMidiDriver::wait_process() {
    log<log_level_debug_trace>("AudioMidiDriver::wait_process");
    m_command_queue.exec_process_thread_command([]() { ; });
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    m_command_queue.exec_process_thread_command([]() { ; });
    log<log_level_debug_trace>("AudioMidiDriver::wait_process done");
}

void AudioMidiDriver::register_decoupled_midi_port_handle(uintptr_t handle) {
    m_rust_core->register_decoupled_port(handle);
}

void AudioMidiDriver::unregister_decoupled_midi_port_handle(uintptr_t handle) {
    m_rust_core->unregister_decoupled_port(handle);
}

std::vector<uintptr_t> AudioMidiDriver::decoupled_midi_port_handles() const {
    auto vals = m_rust_core->get_decoupled_ports();
    std::vector<uintptr_t> out;
    out.reserve(vals.size());
    for (size_t i = 0; i < vals.size(); i++) { out.push_back(vals[i]); }
    return out;
}

shoop_shared_ptr<shoop_types::_DecoupledMidiPort> AudioMidiDriver::open_decoupled_midi_port(std::string name, shoop_port_direction_t direction) {
    constexpr uint32_t decoupled_midi_port_queue_size = 256;
    auto port = open_midi_port(name, direction);
    auto decoupled = shoop_make_shared<shoop_types::_DecoupledMidiPort>(
        port,
        weak_from_this(),
        decoupled_midi_port_queue_size,
        direction);
    register_decoupled_midi_port_handle(reinterpret_cast<uintptr_t>(decoupled.get()));
    m_command_queue.queue_process_thread_command([this, decoupled](){ m_decoupled_midi_ports.insert(decoupled); });
    return decoupled;
}
