#pragma once
#include <RtAudio.h>
#include "AudioSystemInterface.h"
#include "LoggingEnabled.h"

template<typename ApiObject>
class GenericRtAudioSystem :
    public AudioSystemInterface,
    private ModuleLoggingEnabled<"Backend.RtAudioSystem"> {
    
    ApiObject m_api;

public:
    GenericRtAudioSystem(
        std::string device_name,
        size_t input_channels,
        size_t output_channels,
        size_t sample_rate,
        size_t buffer_size,
        size_t n_buffers,
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

using RtAudioSystem = GenericRtAudioSystem<RtAudio>;
extern template class GenericRtAudioSystem<RtAudio>;