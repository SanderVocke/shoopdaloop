#include "AudioMidiDriver.h"
#include <cstdint>

void AudioMidiDriver::add_processor(HasAudioProcessingFunction &p) {
    auto old = m_processors;
    auto _new = std::make_shared<std::set<HasAudioProcessingFunction*>>(*old);
    _new->insert(&p);
    m_processors = _new;
}

void AudioMidiDriver::remove_processor(HasAudioProcessingFunction &p) {
    auto old = m_processors;
    auto _new = std::make_shared<std::set<HasAudioProcessingFunction*>>(*old);
    _new->erase(&p);
    m_processors = _new;
}

std::set<HasAudioProcessingFunction*> AudioMidiDriver::processors() const {
    return *m_processors;
}

void AudioMidiDriver::PROC_process(uint32_t nframes) {
    PROC_handle_command_queue();
    auto lock = m_processors;
    for(auto &p : *lock) {
        p->PROC_process(nframes);
    }
}

uint32_t AudioMidiDriver::get_xruns() const {
    return m_xruns;
}

float AudioMidiDriver::get_dsp_load() {
    maybe_update_dsp_load();
    return m_dsp_load;
}

void AudioMidiDriver::register_decoupled_midi_port(std::shared_ptr<MidiPortInterface> port) {
    m_decoupled_midi_ports.insert(port);
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