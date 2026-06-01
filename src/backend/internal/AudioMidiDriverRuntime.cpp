#include "AudioMidiDriverRuntime.h"

#include <algorithm>
#include <chrono>
#include <thread>

#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include "AudioMidiDriverCxxTrampolines.h"

namespace {
[[maybe_unused]] auto *force_link_trampoline_1 = &backend_rust::audiomididriver_invoke_maybe_process_callback;
[[maybe_unused]] auto *force_link_trampoline_2 = &backend_rust::audiomididriver_exec_command_queue;
[[maybe_unused]] auto *force_link_trampoline_3 = &backend_rust::audiomididriver_process_processor;
[[maybe_unused]] auto *force_link_trampoline_4 = &backend_rust::audiomididriver_process_decoupled_port;
[[maybe_unused]] auto *force_link_trampoline_5 = &backend_rust::audiomididriver_close_decoupled_port;
}

AudioMidiDriverRuntime::AudioMidiDriverRuntime(void (*maybe_process_callback)())
    : m_rust_core(backend_rust::new_audio_midi_driver_core()),
      m_command_queue(rust_command_queue::make(shoop_constants::command_queue_size, 1000, 1000)),
      m_maybe_process_callback(maybe_process_callback),
      m_client_name_cache("unknown") {
    m_rust_core->set_client_name("unknown");
}

void AudioMidiDriverRuntime::add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    m_processors.push_back(p);
    auto strong = bridge_object::register_processor(p);
    auto weak = bridge_object::downgrade(strong);
    auto handle = m_rust_core->add_processor(weak.id, weak.type_id);
    m_processor_handles[p.get()] = handle;
    m_processor_bridge_strongs[p.get()] = strong;
}

void AudioMidiDriverRuntime::remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
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
    auto sit = m_processor_bridge_strongs.find(p.get());
    if (sit != m_processor_bridge_strongs.end()) {
        bridge_object::release_strong(sit->second);
        m_processor_bridge_strongs.erase(sit);
    }
}

std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> AudioMidiDriverRuntime::processors() const {
    return m_processors;
}

void AudioMidiDriverRuntime::process(uint32_t nframes) {
    log<log_level_debug_trace>("AudioMidiDriver::process {}", nframes);
    m_rust_core->process_cycle(
        reinterpret_cast<uintptr_t>(m_maybe_process_callback),
        reinterpret_cast<uintptr_t>(&(*m_command_queue)),
        nframes);
}

void AudioMidiDriverRuntime::exec_all_commands_for_process_thread() {
    rust_command_queue::exec_all(*m_command_queue);
}

void AudioMidiDriverRuntime::unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    rust_command_queue::queue_and_wait(*m_command_queue, [this, port]() {
        auto handle = port->registry_handle();
        if (handle != 0) {
            m_rust_core->close_decoupled_port(handle);
            m_rust_core->unregister_decoupled_port(handle);
            m_decoupled_midi_ports.erase(handle);
        }
    });
}

shoop_shared_ptr<shoop_types::_DecoupledMidiPort> AudioMidiDriverRuntime::make_decoupled_midi_port(
    shoop_shared_ptr<MidiPort> port,
    shoop_weak_ptr<AudioMidiDriver> driver,
    shoop_port_direction_t direction
) {
    constexpr uint32_t decoupled_midi_port_queue_size = 256;
    auto decoupled = shoop_make_shared<shoop_types::_DecoupledMidiPort>(
        port,
        driver,
        decoupled_midi_port_queue_size,
        direction);
    rust_command_queue::queue(*m_command_queue, [this, decoupled]() {
        auto handle = m_rust_core->register_decoupled_port(reinterpret_cast<uintptr_t>(decoupled.get()));
        decoupled->set_registry_handle(handle);
        m_decoupled_midi_ports[handle] = decoupled;
    });
    return decoupled;
}

uint32_t AudioMidiDriverRuntime::get_xruns() const {
    return m_rust_core->get_xruns();
}

void AudioMidiDriverRuntime::reset_xruns() {
    m_rust_core->reset_xruns();
}

void AudioMidiDriverRuntime::report_xrun() {
    m_rust_core->report_xrun();
}

float AudioMidiDriverRuntime::get_dsp_load() const {
    return m_rust_core->get_dsp_load();
}

void AudioMidiDriverRuntime::set_dsp_load(float load) {
    m_rust_core->set_dsp_load(load);
}

uint32_t AudioMidiDriverRuntime::get_sample_rate() const {
    return m_rust_core->get_sample_rate();
}

void AudioMidiDriverRuntime::set_sample_rate(uint32_t sample_rate) {
    m_rust_core->set_sample_rate(sample_rate);
}

uint32_t AudioMidiDriverRuntime::get_buffer_size() const {
    return m_rust_core->get_buffer_size();
}

void AudioMidiDriverRuntime::set_buffer_size(uint32_t buffer_size) {
    m_rust_core->set_buffer_size(buffer_size);
}

const char* AudioMidiDriverRuntime::get_client_name() const {
    m_client_name_cache = std::string(m_rust_core->get_client_name());
    return m_client_name_cache.c_str();
}

void AudioMidiDriverRuntime::set_client_name(const char* name) {
    m_rust_core->set_client_name(name ? name : "unknown");
}

void* AudioMidiDriverRuntime::get_maybe_client_handle() const {
    return reinterpret_cast<void*>(m_rust_core->get_client_handle());
}

void AudioMidiDriverRuntime::set_maybe_client_handle(void* handle) {
    m_rust_core->set_client_handle(reinterpret_cast<uintptr_t>(handle));
}

bool AudioMidiDriverRuntime::get_active() const {
    return m_rust_core->get_active();
}

void AudioMidiDriverRuntime::set_active(bool active) {
    m_rust_core->set_active(active);
}

uint32_t AudioMidiDriverRuntime::get_last_processed() const {
    return m_rust_core->get_last_processed();
}

void AudioMidiDriverRuntime::set_last_processed(uint32_t nframes) {
    m_rust_core->set_last_processed(nframes);
}

void AudioMidiDriverRuntime::wait_process() {
    log<log_level_debug_trace>("AudioMidiDriver::wait_process");
    rust_command_queue::queue_and_wait(*m_command_queue, []() {});
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    rust_command_queue::queue_and_wait(*m_command_queue, []() {});
    log<log_level_debug_trace>("AudioMidiDriver::wait_process done");
}

void AudioMidiDriverRuntime::queue_process_thread_command(std::function<void()> fn) {
    rust_command_queue::queue(*m_command_queue, std::move(fn));
}

void AudioMidiDriverRuntime::exec_process_thread_command(std::function<void()> fn) {
    rust_command_queue::queue_and_wait(*m_command_queue, std::move(fn));
}

backend_rust::CommandQueue &AudioMidiDriverRuntime::get_command_queue() {
    return *m_command_queue;
}
