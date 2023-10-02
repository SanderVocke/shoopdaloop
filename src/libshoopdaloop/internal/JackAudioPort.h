#pragma once
#include <jack/types.h>
#include "JackPort.h"
#include "AudioPortInterface.h"

class JackAudioPort : public virtual AudioPortInterface<jack_default_audio_sample_t>, public JackPort {
public:
    JackAudioPort(
        std::string name,
        PortDirection direction,
        jack_client_t *client,
        std::shared_ptr<JackAllPorts> all_ports_tracker
    );
    
    float *PROC_get_buffer(size_t n_frames, bool do_zero=false) override;
};