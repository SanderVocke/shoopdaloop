#include "GraphAudioPort.h"
#include "AudioPort.h"
#include "PortInterface.h"
#include "types.h"
#include <memory>

GraphAudioPort::GraphAudioPort (shoop_shared_ptr<shoop_types::_AudioPort> const& port,
                    shoop_shared_ptr<BackendSession> const& backend) :
    GraphPort(backend),
    port(port),
    m_plot_frames_processed("GraphAudioPort/" + std::string(port->name()) + "/frames_processed"),
    m_plot_input_peak("GraphAudioPort/" + std::string(port->name()) + "/input_peak"),
    m_plot_output_peak("GraphAudioPort/" + std::string(port->name()) + "/output_peak"),
    m_plot_internal_connections("GraphAudioPort/" + std::string(port->name()) + "/internal_connections"),
    m_plot_input_checksum("GraphAudioPort/" + std::string(port->name()) + "/input_checksum"),
    m_plot_output_checksum("GraphAudioPort/" + std::string(port->name()) + "/output_checksum") {}

PortInterface &GraphAudioPort::get_port() const {
    return static_cast<PortInterface &>(*port);
}

shoop_types::_AudioPort *GraphAudioPort::maybe_audio_port() const {
    return port.get();
}

shoop_shared_ptr<shoop_types::_AudioPort> GraphAudioPort::maybe_shared_audio_port() const {
    return port;
}

shoop_shared_ptr<PortInterface> GraphAudioPort::maybe_shared_port() const {
    return shoop_static_pointer_cast<PortInterface>(port);
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
    auto buf = port->PROC_get_buffer(n_frames);

    // Compute input checksum before processing
    double input_checksum = checksum::compute_audio_checksum(buf, n_frames);

    port->PROC_process(n_frames);
    PROC_internal_connections(n_frames);

    // Compute output checksum after processing
    double output_checksum = checksum::compute_audio_checksum(buf, n_frames);

    // Plot metrics
    m_plot_frames_processed.plot(static_cast<double>(n_frames));
    m_plot_input_peak.plot(static_cast<double>(port->get_input_peak()));
    m_plot_output_peak.plot(static_cast<double>(port->get_output_peak()));
    m_plot_internal_connections.plot(static_cast<double>(mp_internal_port_connections.size()));
    m_plot_input_checksum.plot(input_checksum);
    m_plot_output_checksum.plot(output_checksum);
}