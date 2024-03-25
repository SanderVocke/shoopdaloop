#pragma once
#include <jack/types.h>
#include "JackApi.h"
#include "JackTestApi.h"
#include "JackPort.h"
#include "AudioPort.h"
#include "types.h"
#include <atomic>

template<typename API>
class GenericJackAudioPort : public virtual AudioPort<jack_default_audio_sample_t>, public virtual GenericJackPort<API> {
    using GenericJackPort<API>::m_port;
    using GenericJackPort<API>::m_buffer;
    using GenericJackPort<API>::m_direction;

    std::vector<jack_default_audio_sample_t> m_fallback_buffer;
public:
    GenericJackAudioPort(
        std::string name,
        shoop_port_direction_t direction,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker,
        std::shared_ptr<typename AudioPort<jack_default_audio_sample_t>::BufferPool> maybe_ringbuffer_buffer_pool
    );
    
    // Prepare step is used to get the buffer from JACK
    void PROC_prepare(uint32_t) override;
    void PROC_process(uint32_t) override;

    // Access the cached buffer.
    float *PROC_get_buffer(uint32_t n_frames) override;

    unsigned input_connectability() const override;
    unsigned output_connectability() const override;
};

using JackAudioPort = GenericJackAudioPort<JackApi>;
using JackTestAudioPort = GenericJackAudioPort<JackTestApi>;

extern template class GenericJackAudioPort<JackApi>;
extern template class GenericJackAudioPort<JackTestApi>;