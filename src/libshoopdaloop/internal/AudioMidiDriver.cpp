#include "AudioMidiDriver.h"
#include "DecoupledMidiPort.h"
#include <cstdint>
#include "shoop_globals.h"
#include <thread>

AudioMidiDriver::AudioMidiDriver() :
  WithCommandQueue(),
  m_processors(shoop_make_shared<std::set<HasAudioProcessingFunction*>>()),
  m_active(false),
  m_client_name("unknown")
{
}

void AudioMidiDriver::add_processor(HasAudioProcessingFunction &p) {
    auto old = m_processors;
    auto _new = shoop_make_shared<std::set<HasAudioProcessingFunction*>>();
    *_new = *old;
    _new->insert(&p);
    m_processors = _new;
}

void AudioMidiDriver::remove_processor(HasAudioProcessingFunction &p) {
    auto old = m_processors;
    auto _new = shoop_make_shared<std::set<HasAudioProcessingFunction*>>();
    *_new = *old;
    _new->erase(&p);
    m_processors = _new;
}

std::set<HasAudioProcessingFunction*> AudioMidiDriver::processors() const {
    return *m_processors;
}

void AudioMidiDriver::PROC_process(uint32_t nframes) {
    log<log_level_debug_trace>("AudioMidiDriver::process {}", nframes);
    PROC_handle_command_queue();
    PROC_process_decoupled_midi_ports(nframes);
    auto lock = m_processors;
    for(auto &p : *lock) {
        p->PROC_process(nframes);
    }
    set_last_processed(nframes);
}

uint32_t AudioMidiDriver::get_xruns() const {
    return m_xruns;
}

float AudioMidiDriver::get_dsp_load() {
    maybe_update_dsp_load();
    return m_dsp_load;
}

void AudioMidiDriver::unregister_decoupled_midi_port(shoop_shared_ptr<shoop_types::_DecoupledMidiPort> port) {
    exec_process_thread_command([this, port]() {
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
    return m_sample_rate;
}

uint32_t AudioMidiDriver::get_buffer_size() {
    maybe_update_buffer_size();
    return m_buffer_size;
}

void AudioMidiDriver::reset_xruns() {
    m_xruns = 0;
}

void AudioMidiDriver::report_xrun() {
    m_xruns++;
}

void AudioMidiDriver::set_dsp_load(float load) {
    m_dsp_load = load;
}

void AudioMidiDriver::set_sample_rate(uint32_t sample_rate) {
    m_sample_rate = sample_rate;
}

void AudioMidiDriver::set_buffer_size(uint32_t buffer_size) {
    m_buffer_size = buffer_size;
}

void AudioMidiDriver::set_client_name(const char* name) {
    m_client_name = name;
}

void AudioMidiDriver::set_active(bool active) {
    m_active = active;
}

void AudioMidiDriver::set_last_processed(uint32_t nframes) {
    m_last_processed = nframes;
}

const char* AudioMidiDriver::get_client_name() const {
    return m_client_name;
}

void AudioMidiDriver::set_maybe_client_handle(void* handle) {
    m_maybe_client_handle = handle;
}

void* AudioMidiDriver::get_maybe_client_handle() const {
    return m_maybe_client_handle;
}

bool AudioMidiDriver::get_active() const {
    return m_active;
}

uint32_t AudioMidiDriver::get_last_processed() const {
    return m_last_processed;
}

void AudioMidiDriver::wait_process() {
    // To ensure a complete process cycle was done, execute two commands with
    // a small delay in-between. Each command will end up in a separate process
    // iteration.
    log<log_level_debug_trace>("AudioMidiDriver::wait_process");
    exec_process_thread_command([]() { ; });
    std::this_thread::sleep_for(std::chrono::milliseconds(1));
    exec_process_thread_command([]() { ; });
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
    queue_process_thread_command([this, decoupled](){ m_decoupled_midi_ports.insert(decoupled); });
    return decoupled;
}