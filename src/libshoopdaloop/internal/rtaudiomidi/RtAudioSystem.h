#pragma once
#include "RtAudioApi.h"
#include "AudioSystemInterface.h"
#include "LoggingEnabled.h"

template<typename Api>
class GenericRtAudioSystem :
    public AudioSystemInterface,
    private ModuleLoggingEnabled<"Backend.RtAudioSystem"> {

public:
    GenericRtAudioSystem(
        std::string client_name,
        std::function<void(uint32_t)> process_cb
    );

    void start() override;

    ~GenericRtAudioSystem() override;

    std::shared_ptr<AudioPortInterface<float>> open_audio_port(
        std::string name,
        PortDirection direction
    ) override;

    std::shared_ptr<MidiPortInterface> open_midi_port(
        std::string name,
        PortDirection direction
    ) override;

    uint32_t get_sample_rate() const override;
    uint32_t get_buffer_size() const override;
    void* maybe_client_handle() const override;
    const char* client_name() const override;
    void close() override;
    uint32_t get_xruns() const override;
    void reset_xruns() override;
    
};

using RtAudioSystem = GenericRtAudioSystem<RtAudioApi>;
extern template class GenericRtAudioSystem<RtAudioApi>;