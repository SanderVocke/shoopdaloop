#pragma once
#include <jack/types.h>
#include "JackApi.h"
#include "JackTestApi.h"
#include "JackPort.h"
#include "AudioPortInterface.h"

template<typename API>
class GenericJackAudioPort : public virtual AudioPortInterface<jack_default_audio_sample_t>, public GenericJackPort<API> {
    using GenericJackPort<API>::m_port;
public:
    GenericJackAudioPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client,
        std::shared_ptr<GenericJackAllPorts<API>> all_ports_tracker
    );
    
    float *PROC_get_buffer(size_t n_frames, bool do_zero=false) override;
};

using JackAudioPort = GenericJackAudioPort<JackApi>;
using JackTestAudioPort = GenericJackAudioPort<JackTestApi>;