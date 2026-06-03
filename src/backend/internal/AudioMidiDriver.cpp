#include "AudioMidiDriver.h"

#include <chrono>
#include <cstdint>
#include <thread>

#include "AudioMidiDriverCxxTrampolines.h"
#include "BridgeObject.h"
#include "DecoupledMidiPort.h"
#include "IProcessor.h"
#include "RustCommandQueue.h"

namespace backend_rust {
void audiomididriver_invoke_maybe_process_callback(uintptr_t maybe_fn_ptr) {
    if (!maybe_fn_ptr) { return; }
    auto fn = reinterpret_cast<void (*)()>(maybe_fn_ptr);
    fn();
}
}

AudioMidiDriver::AudioMidiDriver(void (*maybe_process_callback)())
    : m_rust_core(backend_rust::new_audio_midi_driver_core()),
      m_maybe_process_callback(maybe_process_callback),
      m_client_name_cache("unknown") {
    m_rust_core->set_client_name("unknown");
}

void AudioMidiDriver::add_processor(std::shared_ptr<IProcessor> p) {
    auto strong = std::make_unique<ProcessorBridgeStrong>(p);
    auto weak = strong->downgrade();
    m_rust_core->add_processor(reinterpret_cast<uintptr_t>(p.get()), std::move(weak), std::move(strong));
}

void AudioMidiDriver::remove_processor(std::shared_ptr<IProcessor> p) {
    m_rust_core->remove_processor_by_cpp_identity(reinterpret_cast<uintptr_t>(p.get()));
}

std::vector<std::unique_ptr<ProcessorBridgeWeak>> AudioMidiDriver::processors() const {
    std::vector<std::unique_ptr<ProcessorBridgeWeak>> result;
    auto handles = m_rust_core->get_processor_handles();
    result.reserve(handles.size());
    for (auto const &handle : handles) {
        auto weak = m_rust_core->get_processor_bridge_weak_handle(handle);
        if (weak) {
            result.push_back(std::move(weak));
        }
    }
    return result;
}

void AudioMidiDriver::process(uint32_t nframes) {
    log<log_level_debug_trace>("AudioMidiDriver::process {}", nframes);
    m_rust_core->process_cycle(reinterpret_cast<uintptr_t>(m_maybe_process_callback), nframes);
}

void AudioMidiDriver::exec_all_commands_for_process_thread() {
    m_rust_core->exec_all_commands_for_process_thread();
}

void AudioMidiDriver::unregister_decoupled_midi_port(std::shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
    rust_command_queue::queue_and_wait(*queue, [this, port]() {
        auto handle = port->registry_handle();
        if (handle != 0) {
            m_rust_core->close_decoupled_port(handle);
            m_rust_core->unregister_decoupled_port(handle);
        }
    });
}

std::shared_ptr<shoop_types::_DecoupledMidiPort> AudioMidiDriver::make_decoupled_midi_port(
    std::shared_ptr<MidiPort> port,
    std::weak_ptr<AudioMidiDriver> driver,
    shoop_port_direction_t direction
) {
    constexpr uint32_t decoupled_midi_port_queue_size = 256;
    auto decoupled = std::make_shared<shoop_types::_DecoupledMidiPort>(port, driver, decoupled_midi_port_queue_size, direction);
    auto strong = std::make_shared<std::unique_ptr<DecoupledMidiPortBridgeStrong>>(
        std::make_unique<DecoupledMidiPortBridgeStrong>(decoupled));
    auto weak = std::make_shared<std::unique_ptr<DecoupledMidiPortBridgeWeak>>(
        (*strong)->downgrade());
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
    rust_command_queue::queue(*queue, [this, decoupled, strong, weak]() {
        auto handle = m_rust_core->register_decoupled_port(std::move(*weak), std::move(*strong));
        decoupled->set_registry_handle(handle);
    });
    return decoupled;
}

uint32_t AudioMidiDriver::get_xruns() const { return m_rust_core->get_xruns(); }
void AudioMidiDriver::reset_xruns() { m_rust_core->reset_xruns(); }
void AudioMidiDriver::report_xrun() { m_rust_core->report_xrun(); }
float AudioMidiDriver::get_dsp_load() { return m_rust_core->get_dsp_load(); }
void AudioMidiDriver::set_dsp_load(float load) { m_rust_core->set_dsp_load(load); }
uint32_t AudioMidiDriver::get_sample_rate() { return m_rust_core->get_sample_rate(); }
void AudioMidiDriver::set_sample_rate(uint32_t sample_rate) { m_rust_core->set_sample_rate(sample_rate); }
uint32_t AudioMidiDriver::get_buffer_size() { return m_rust_core->get_buffer_size(); }
void AudioMidiDriver::set_buffer_size(uint32_t buffer_size) { m_rust_core->set_buffer_size(buffer_size); }
const char* AudioMidiDriver::get_client_name() const { m_client_name_cache = std::string(m_rust_core->get_client_name()); return m_client_name_cache.c_str(); }
void AudioMidiDriver::set_client_name(const char* name) { m_rust_core->set_client_name(name ? name : "unknown"); }
void* AudioMidiDriver::get_maybe_client_handle() const { return reinterpret_cast<void*>(m_rust_core->get_client_handle()); }
void AudioMidiDriver::set_maybe_client_handle(void* handle) { m_rust_core->set_client_handle(reinterpret_cast<uintptr_t>(handle)); }
bool AudioMidiDriver::get_active() const { return m_rust_core->get_active(); }
void AudioMidiDriver::set_active(bool active) { m_rust_core->set_active(active); }
uint32_t AudioMidiDriver::get_last_processed() const { return m_rust_core->get_last_processed(); }
void AudioMidiDriver::set_last_processed(uint32_t nframes) { m_rust_core->set_last_processed(nframes); }

void AudioMidiDriver::wait_process() {
    log<log_level_debug_trace>("AudioMidiDriver::wait_process");
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
    rust_command_queue::queue_and_wait(*queue, []() {});
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    rust_command_queue::queue_and_wait(*queue, []() {});
    log<log_level_debug_trace>("AudioMidiDriver::wait_process done");
}

void AudioMidiDriver::queue_process_thread_command(std::function<void()> fn) {
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
    rust_command_queue::queue(*queue, std::move(fn));
}

void AudioMidiDriver::exec_process_thread_command(std::function<void()> fn) {
    auto *queue = reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
    rust_command_queue::queue_and_wait(*queue, std::move(fn));
}

backend_rust::CommandQueue &AudioMidiDriver::get_command_queue() {
    return *reinterpret_cast<backend_rust::CommandQueue *>(m_rust_core->command_queue_ptr());
}
