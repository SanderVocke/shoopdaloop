#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include <cstdint>
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

AudioMidiDriver::AudioMidiDriver(void (*maybe_process_callback)())
    : m_runtime(maybe_process_callback) {}

void AudioMidiDriver::add_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    m_runtime.add_processor(p);
}

void AudioMidiDriver::remove_processor(shoop_shared_ptr<HasAudioProcessingFunction> p) {
    m_runtime.remove_processor(p);
}

std::vector<shoop_weak_ptr<HasAudioProcessingFunction>> AudioMidiDriver::processors() const {
    return m_runtime.processors();
}

void AudioMidiDriver::PROC_process(uint32_t nframes) {
    m_runtime.process(nframes);
}

uint32_t AudioMidiDriver::get_xruns() const {
    return m_runtime.get_xruns();
}

float AudioMidiDriver::get_dsp_load() {
    maybe_update_dsp_load();
    return m_runtime.get_dsp_load();
}

void AudioMidiDriver::unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    m_runtime.unregister_decoupled_midi_port(port);
}

uint32_t AudioMidiDriver::get_sample_rate() {
    maybe_update_sample_rate();
    return m_runtime.get_sample_rate();
}

uint32_t AudioMidiDriver::get_buffer_size() {
    maybe_update_buffer_size();
    return m_runtime.get_buffer_size();
}

void AudioMidiDriver::reset_xruns() {
    m_runtime.reset_xruns();
}

void AudioMidiDriver::report_xrun() {
    m_runtime.report_xrun();
}

void AudioMidiDriver::set_dsp_load(float load) {
    m_runtime.set_dsp_load(load);
}

void AudioMidiDriver::set_sample_rate(uint32_t sample_rate) {
    m_runtime.set_sample_rate(sample_rate);
}

void AudioMidiDriver::set_buffer_size(uint32_t buffer_size) {
    m_runtime.set_buffer_size(buffer_size);
}

void AudioMidiDriver::set_client_name(const char* name) {
    m_runtime.set_client_name(name);
}

void AudioMidiDriver::set_active(bool active) {
    m_runtime.set_active(active);
}

void AudioMidiDriver::set_last_processed(uint32_t nframes) {
    m_runtime.set_last_processed(nframes);
}

const char* AudioMidiDriver::get_client_name() const {
    return m_runtime.get_client_name();
}

void AudioMidiDriver::set_maybe_client_handle(void* handle) {
    m_runtime.set_maybe_client_handle(handle);
}

void* AudioMidiDriver::get_maybe_client_handle() const {
    return m_runtime.get_maybe_client_handle();
}

bool AudioMidiDriver::get_active() const {
    return m_runtime.get_active();
}

uint32_t AudioMidiDriver::get_last_processed() const {
    return m_runtime.get_last_processed();
}

void AudioMidiDriver::wait_process() {
    m_runtime.wait_process();
}

shoop_shared_ptr<shoop_types::_DecoupledMidiPort> AudioMidiDriver::open_decoupled_midi_port(std::string name, shoop_port_direction_t direction) {
    auto port = open_midi_port(name, direction);
    return m_runtime.make_decoupled_midi_port(port, weak_from_this(), direction);
}
