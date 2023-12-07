#include "shoop_globals.h"
#include "CustomProcessingChain.h"
#include "DummyAudioMidiDriver.h"

template<typename TimeType, typename SizeType>
CustomProcessingChain<TimeType, SizeType>::CustomProcessingChain(
    uint32_t n_audio_inputs,
    uint32_t n_audio_outputs,
    uint32_t n_midi_inputs,
    ProcessFunctor process_callback) :
    m_active(true),
    m_freewheeling(false),
    m_process_callback(process_callback)
{
    for(uint32_t i=0; i<n_audio_inputs; i++) {
        m_input_audio_ports.push_back(std::make_shared<InternalAudioPort<shoop_types::audio_sample_t>>("fx_audio_in_" + std::to_string(i+1), PortDirection::Output, 4096));
    }
    for(uint32_t i=0; i<n_audio_outputs; i++) {
        m_output_audio_ports.push_back(std::make_shared<InternalAudioPort<shoop_types::audio_sample_t>>("fx_audio_out_" + std::to_string(i+1), PortDirection::Input, 4096));
    }
    for(uint32_t i=0; i<n_midi_inputs; i++) {
        m_input_midi_ports.push_back(std::make_shared<DummyMidiPort>("fx_midi_in_" + std::to_string(i+1), PortDirection::Output));
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
void CustomProcessingChain<TimeType, SizeType>::process(uint32_t frames) {
    for(auto &p : m_output_audio_ports) {
        p->PROC_get_buffer(frames, true); // zero outputs
    }
    if (m_process_callback && m_active.load()) {
        m_process_callback(frames, m_input_audio_ports, m_output_audio_ports, m_input_midi_ports);
    }
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
void CustomProcessingChain<TimeType, SizeType>::ensure_buffers(uint32_t size) {
    for (auto &port : m_input_audio_ports) {
        port->reallocate_buffer(size);
    }
    for (auto &port : m_output_audio_ports) {
        port->reallocate_buffer(size);
        port->PROC_get_buffer(size, true);
    }
}

template <typename TimeType, typename SizeType>
uint32_t CustomProcessingChain<TimeType, SizeType>::buffers_size() const {
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

template class CustomProcessingChain<uint32_t, uint16_t>;
template class CustomProcessingChain<uint32_t, uint32_t>;
template class CustomProcessingChain<uint16_t, uint16_t>;
template class CustomProcessingChain<uint16_t, uint32_t>;