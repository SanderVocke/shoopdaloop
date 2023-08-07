#include "shoop_globals.h"
#define IMPLEMENT_CustomProcessingChain_H
#include "CustomProcessingChain.h"
#include "DummyAudioSystem.h"
template class CustomProcessingChain<uint32_t, uint16_t>;
template class CustomProcessingChain<uint32_t, uint32_t>;
template class CustomProcessingChain<uint16_t, uint16_t>;
template class CustomProcessingChain<uint16_t, uint32_t>;

template<typename TimeType, typename SizeType>
std::string CustomProcessingChain<TimeType, SizeType>::log_module_name() const {
    return "Backend.CustomProcessingChain";
}

template<typename TimeType, typename SizeType>
CustomProcessingChain<TimeType, SizeType>::CustomProcessingChain(
    size_t n_audio_inputs,
    size_t n_audio_outputs,
    size_t n_midi_inputs,
    ProcessFunctor process_callback) :
    m_active(true),
    m_freewheeling(false),
    m_process_callback(process_callback)
{
    log_init();
    for(size_t i=0; i<n_audio_inputs; i++) {
        m_input_audio_ports.push_back(std::make_shared<InternalAudioPort<shoop_types::audio_sample_t>>("audio_in_" + std::to_string(i+1), PortDirection::Input, 4096));
    }
    for(size_t i=0; i<n_audio_outputs; i++) {
        m_output_audio_ports.push_back(std::make_shared<InternalAudioPort<shoop_types::audio_sample_t>>("audio_out_" + std::to_string(i+1), PortDirection::Output, 4096));
    }
    for(size_t i=0; i<n_midi_inputs; i++) {
        m_input_midi_ports.push_back(std::make_shared<DummyMidiPort>("midi_in_" + std::to_string(i+1), PortDirection::Input));
    }
}
    
template<typename TimeType, typename SizeType>
std::vector<typename CustomProcessingChain<TimeType, SizeType>::SharedInternalAudioPort> const& 
CustomProcessingChain<TimeType, SizeType>::input_audio_ports() const {
    return m_input_audio_ports;
}

template<typename TimeType, typename SizeType>
std::vector<typename CustomProcessingChain<TimeType, SizeType>::SharedInternalAudioPort> const& 
CustomProcessingChain<TimeType, SizeType>::output_audio_ports() const {
    return m_output_audio_ports;
}

template<typename TimeType, typename SizeType>
std::vector<typename CustomProcessingChain<TimeType, SizeType>::SharedMidiPort> const& 
CustomProcessingChain<TimeType, SizeType>::input_midi_ports() const {
    return m_input_midi_ports;
}

template<typename TimeType, typename SizeType>
bool CustomProcessingChain<TimeType, SizeType>::is_freewheeling() const { return m_freewheeling; };

template<typename TimeType, typename SizeType>
void CustomProcessingChain<TimeType, SizeType>::set_freewheeling(bool enabled) {
    m_freewheeling = enabled;
}

template<typename TimeType, typename SizeType>
void CustomProcessingChain<TimeType, SizeType>::process(size_t frames) {
    if (m_process_callback) {
        m_process_callback(frames, m_input_audio_ports, m_output_audio_ports, m_input_midi_ports);
    }
    log<logging::LogLevel::debug>("fx {} {}", m_input_audio_ports[0]->PROC_get_buffer(frames)[0],
        m_output_audio_ports[0]->PROC_get_buffer(frames)[0]);
}

template<typename TimeType, typename SizeType>
bool CustomProcessingChain<TimeType, SizeType>::is_ready() const{
    return true;
};

template<typename TimeType, typename SizeType>
bool CustomProcessingChain<TimeType, SizeType>::is_active() const { return m_active; };

template<typename TimeType, typename SizeType>
void CustomProcessingChain<TimeType, SizeType>::set_active(bool active) {
    m_active = active;
}

template <typename TimeType, typename SizeType>
void CustomProcessingChain<TimeType, SizeType>::ensure_buffers(size_t size) {
    for (auto &port : m_input_audio_ports) {
        port->reallocate_buffer(size);
    }
    for (auto &port : m_output_audio_ports) {
        port->reallocate_buffer(size);
    }
}

template <typename TimeType, typename SizeType>
size_t CustomProcessingChain<TimeType, SizeType>::buffers_size() const {
    if (!m_input_audio_ports.empty()) {
        return m_input_audio_ports[0]->buffer_size();
    } else if (!m_output_audio_ports.empty()) {
        return m_output_audio_ports[0]->buffer_size();
    } else {
        return 0;
    }    
}

template <typename TimeType, typename SizeType>
void CustomProcessingChain<TimeType, SizeType>::stop() {}

