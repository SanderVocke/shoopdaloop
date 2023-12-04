#include "AudioMidiDriver.h"

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
    auto lock = m_processors;
    for(auto &p : *lock) {
        p->PROC_process(nframes);
    }
}