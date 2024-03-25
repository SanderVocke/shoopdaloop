#pragma once
#include "ProcessingChainInterface.h"
#include <functional>
#include <atomic>
#include "LoggingEnabled.h"
#include "shoop_globals.h"

template<typename SampleT> class AudioPort;

template<typename TimeType, typename SizeType>
class CustomProcessingChain : public ProcessingChainInterface<TimeType, SizeType>,
                              private ModuleLoggingEnabled<"Backend.CustomProcessingChain"> {
    
public:
    using SharedInternalAudioPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedInternalAudioPort;
    using SharedMidiPort = typename ProcessingChainInterface<TimeType, SizeType>::SharedMidiPort;
    using ProcessFunctor = std::function<void(uint32_t,
        std::vector<SharedInternalAudioPort> &,
        std::vector<SharedInternalAudioPort> &,
        std::vector<SharedMidiPort> &)>;

private:
    std::atomic<bool> m_active = false;
    std::atomic<bool> m_freewheeling = false;

    std::vector<SharedInternalAudioPort> m_input_audio_ports;
    std::vector<SharedInternalAudioPort> m_output_audio_ports;
    std::vector<SharedMidiPort> m_input_midi_ports;

    const ProcessFunctor m_process_callback;

public:

    CustomProcessingChain(uint32_t n_audio_inputs,
                          uint32_t n_audio_outputs,
                          uint32_t n_midi_inputs,
                          ProcessFunctor process_callback,
                          std::shared_ptr<typename AudioPort<shoop_types::audio_sample_t>::BufferPool> maybe_buffer_pool);
    
    std::vector<SharedInternalAudioPort> const& input_audio_ports() const override;
    std::vector<SharedInternalAudioPort> const& output_audio_ports() const override;
    std::vector<SharedMidiPort> const& input_midi_ports() const override;

    bool is_freewheeling() const override;
    void set_freewheeling(bool enabled) override;
    void process(uint32_t frames) override;
    bool is_ready() const override;
    bool is_active() const override;
    void set_active(bool active) override;

    void stop() override;
};

extern template class CustomProcessingChain<uint32_t, uint16_t>;
extern template class CustomProcessingChain<uint32_t, uint32_t>;
extern template class CustomProcessingChain<uint16_t, uint16_t>;
extern template class CustomProcessingChain<uint16_t, uint32_t>;