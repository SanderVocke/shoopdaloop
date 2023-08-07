#pragma once
#include "ProcessingChainInterface.h"
#include <functional>
#include <atomic>

template<typename TimeType, typename SizeType>
class CustomProcessingChain : public ProcessingChainInterface<TimeType, SizeType> {
public:
    using SharedInternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedInternalAudioPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;
    using ProcessFunctor = std::function<void(size_t,
        std::vector<SharedInternalAudioPort> &,
        std::vector<SharedInternalAudioPort> &,
        std::vector<SharedMidiPort> &)>;

private:
    std::atomic<bool> m_active;
    std::atomic<bool> m_freewheeling;

    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort> m_input_midi_ports;

    const ProcessFunctor m_process_callback;

public:

    CustomProcessingChain(size_t n_audio_inputs,
                          size_t n_audio_outputs,
                          size_t n_midi_inputs,
                          ProcessFunctor process_callback,
                          size_t audio_buffer_size);
    
    std::vector<SharedInternalAudioPort> const& input_audio_ports() const override;
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const override;
    std::vector<SharedMidiPort> const& input_midi_ports() const override;

    bool is_freewheeling() const override;
    void set_freewheeling(bool enabled) override;
    void process(size_t frames) override;
    bool is_ready() const override;
    bool is_active() const override;
    void set_active(bool active) override;
};

#ifndef IMPLEMENT_CustomProcessingChain_H
extern template class CustomProcessingChain<uint32_t, uint16_t>;
extern template class CustomProcessingChain<uint32_t, uint32_t>;
extern template class CustomProcessingChain<uint16_t, uint16_t>;
extern template class CustomProcessingChain<uint16_t, uint32_t>;
#endif