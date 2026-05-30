#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include <cstdint>
#include "shoop_globals.h"
#include <thread>
#include <algorithm>
#include "AudioMidiDriverCxxTrampolines.h"

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr) {
    if (!maybe_fn_ptr) { return; }
    auto fn = reinterpret_cast<void (*)()>(maybe_fn_ptr);
    fn();
}

void audiomididriver_exec_command_queue(uintptr_t command_queue_ptr) {
    if (!command_queue_ptr) { return; }
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(command_queue_ptr);
    rust_command_queue::exec_all(*queue);
}

void audiomididriver_process_processor(uintptr_t processor_ptr, uint32_t nframes) {
    if (!processor_ptr) { return; }
    auto *processor = reinterpret_cast<HasAudioProcessingFunction *>(processor_ptr);
    processor->PROC_process(nframes);
}

void audiomididriver_process_decoupled_port(uintptr_t decoupled_port_ptr, uint32_t nframes) {
    if (!decoupled_port_ptr) { return; }
    auto *port = reinterpret_cast<::DecoupledMidiPort *>(decoupled_port_ptr);
    port->PROC_process(nframes);
}

void audiomididriver_close_decoupled_port(uintptr_t decoupled_port_ptr) {
    if (!decoupled_port_ptr) { return; }
    auto *port = reinterpret_cast<::DecoupledMidiPort *>(decoupled_port_ptr);
    port->close();
}
}

AudioMidiDriver::AudioMidiDriver(void (*maybe_process_callback)()) :
  m_rust_core(backend_rust::new_audio_midi_driver_core()),
  m_command_queue(rust_command_queue::make(shoop_constants::command_queue_size, 1000, 1000)),
  m_processors(),
  m_maybe_process_callback(maybe_process_callback)
{
    // Set default client name in Rust core
    m_rust_core->set_client_name("unknown");
}

void AudioMidiDriver::add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    m_processors.push_back(p);
    auto handle = m_rust_core->add_processor(reinterpret_cast<uintptr_t>(p.get()));
    m_processor_handles[p.get()] = handle;
}

void AudioMidiDriver::remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    m_processors.erase(
        std::remove_if(m_processors.begin(), m_processors.end(), [&](auto const &wp) {
            auto sp = wp.lock();
            return !sp || sp == p;
        }),
        m_processors.end());
    auto it = m_processor_handles.find(p.get());
    if (it != m_processor_handles.end()) {
        m_rust_core->remove_processor(it->second);
        m_processor_handles.erase(it);
    }
}

std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> AudioMidiDriver::processors() const {
    return m_processors;
}

void AudioMidiDriver::PROC_process(uint32_t nframes) {
    log<log_level_debug_trace>("AudioMidiDriver::process {}", nframes);
    m_rust_core->process_cycle(
        reinterpret_cast<uintptr_t>(m_maybe_process_callback),
        reinterpret_cast<uintptr_t>(&(*m_command_queue)),
        nframes);
}

uint32_t AudioMidiDriver::get_xruns() const {
    return m_rust_core->get_xruns();
}

float AudioMidiDriver::get_dsp_load() {
    maybe_update_dsp_load();
    return m_rust_core->get_dsp_load();
}

void AudioMidiDriver::unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    rust_command_queue::queue_and_wait(*m_command_queue, [this, port]() {
        auto handle = port->registry_handle();
        if (handle != 0) {
            m_rust_core->close_decoupled_port(handle);
            m_rust_core->unregister_decoupled_port(handle);
            m_decoupled_midi_ports.erase(handle);
        }
    });
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
    // Cache the name in a static string to return const char*
    // Note: Rust String needs conversion to std::string
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
    // To ensure a complete process cycle was done, execute two commands with
    // a small delay in-between. Each command will end up in a separate process
    // iteration.
    log<log_level_debug_trace>("AudioMidiDriver::wait_process");
    rust_command_queue::queue_and_wait(*m_command_queue, []() { ; });
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    rust_command_queue::queue_and_wait(*m_command_queue, []() { ; });
    log<log_level_debug_trace>("AudioMidiDriver::wait_process done");
}

shoop_shared_ptr<shoop_types::_DecoupledMidiPort> AudioMidiDriver::open_decoupled_midi_port(std::string name, shoop_port_direction_t direction) {
    constexpr uint32_t decoupled_midi_port_queue_size = 256;
    auto port = open_midi_port(name, direction);
    auto decoupled = shoop_make_shared<shoop_types::_DecoupledMidiPort>(
        port,
        weak_from_this(),
        decoupled_midi_port_queue_size,
        direction);
    rust_command_queue::queue(*m_command_queue, [this, decoupled](){
        auto handle = m_rust_core->register_decoupled_port(reinterpret_cast<uintptr_t>(decoupled.get()));
        decoupled->set_registry_handle(handle);
        m_decoupled_midi_ports[handle] = decoupled;
    });
    return decoupled;
}