#include "shoop_globals.h"
#include "CustomProcessingChain.h"
#include "DummyAudioMidiDriver.h"
#include "shoop_shared_ptr.h"

template<typename TimeType, typename SizeType>
CustomProcessingChain<TimeType, SizeType>::CustomProcessingChain(
    uint32_t n_audio_inputs,
    uint32_t n_audio_outputs,
    uint32_t n_midi_inputs,
    ProcessFunctor process_callback,
    shoop_shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::UsedBufferPool> maybe_buffer_pool) :
    m_active(true),
    m_freewheeling(false),
    m_process_callback(process_callback)
{
    for(uint32_t i=0; i<n_audio_inputs; i++) {
        m_input_audio_ports.push_back(shoop_make_shared<InternalAudioPort<shoop_types::audio_sample_t>>(
            "fx_audio_in_" + std::to_string(i+1), 4096,
            ShoopPortConnectability_Internal,
            0, nullptr));
    }
    for(uint32_t i=0; i<n_audio_outputs; i++) {
        // Output ports get a ringbuffer, because those may go into further channels to record
        m_output_audio_ports.push_back(shoop_make_shared<InternalAudioPort<shoop_types::audio_sample_t>>(
            "fx_audio_out_" + std::to_string(i+1), 4096,
            0,
            ShoopPortConnectability_Internal, maybe_buffer_pool));
    }
    for(uint32_t i=0; i<n_midi_inputs; i++) {
        m_input_midi_ports.push_back(
            shoop_static_pointer_cast<MidiPort>(
                shoop_make_shared<DummyMidiPort>("fx_midi_in_" + std::to_string(i+1), ShoopPortDirection_Output)
            ));
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
    log<log_level_debug_trace>("Processing {} frames", frames);
    for(auto &p : m_output_audio_ports) {
        p->PROC_get_buffer(frames); // zero outputs
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
void CustomProcessingChain<TimeType, SizeType>::stop() {}

template class CustomProcessingChain<uint32_t, uint16_t>;
template class CustomProcessingChain<uint32_t, uint32_t>;
template class CustomProcessingChain<uint16_t, uint16_t>;
template class CustomProcessingChain<uint16_t, uint32_t>;