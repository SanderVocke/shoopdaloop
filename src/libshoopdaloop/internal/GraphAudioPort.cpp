#include "GraphAudioPort.h"
#include "AudioPort.h"
#include "PortInterface.h"
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

std::shared_ptr<shoop_types::_AudioPort> GraphAudioPort::maybe_shared_audio_port() const {
    return port;
}

std::shared_ptr<PortInterface> GraphAudioPort::maybe_shared_port() const {
    return port;
}

void GraphAudioPort::PROC_internal_connections(uint32_t n_frames) {

    if (n_frames > 0) {
        log<log_level_debug_trace>("internal connections: {} targets, enabled {}", mp_internal_port_connections.size(), m_passthrough_enabled.load());
    }

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
        for (auto &p : mp_internal_port_connections) {
            if (n_frames > 0) {
                log<log_level_debug_trace>("process audio internal connection ({} samples, 1st = {})", n_frames, from_buf[0]);
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
    PROC_internal_connections(n_frames);
}