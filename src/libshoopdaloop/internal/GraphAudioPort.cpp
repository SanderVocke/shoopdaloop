#include "GraphAudioPort.h"
#include "AudioPort.h"
#include "types.h"
#include <memory>

GraphAudioPort::GraphAudioPort (std::shared_ptr<shoop_types::_AudioPort> const& port,
                    std::shared_ptr<BackendSession> const& backend) :
    GraphPort(backend), port(port) {}

PortInterface &GraphAudioPort::get_port() const {
    return static_cast<PortInterface &>(*port);
}

shoop_types::_AudioPort *GraphAudioPort::maybe_audio_port() const {
    return port.get();
}

void GraphAudioPort::PROC_passthrough(uint32_t n_frames) {
    
    auto get_buf = [&](auto &maybe_to) -> audio_sample_t* {
        if(auto _to = maybe_to.lock()) {
            if(auto port = _to->maybe_audio_port()) {
                return port->PROC_get_buffer(n_frames);
            }
        }
        return nullptr;
    };
    
    auto from_buf = port->PROC_get_buffer(n_frames);
    if (m_passthrough_enabled) {
        for (auto &p : mp_passthrough_to) {
            if (n_frames > 0) {
                log<log_level_trace>("process audio passthrough ({} samples)", n_frames);
            }
            if(auto buf = get_buf(p)) {
                for (size_t i=0; i<n_frames; i++) {
                    buf[i] += from_buf[i];
                }
            }
        }
    }
}

void GraphAudioPort::PROC_prepare(uint32_t n_frames) {
    port->PROC_prepare(n_frames);
}

void GraphAudioPort::PROC_process(uint32_t n_frames) {
    port->PROC_process(n_frames);
    PROC_passthrough(n_frames);
}