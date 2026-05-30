#pragma once
#include "GraphPort.h"
#include "shoop_shared_ptr.h"
#include "TracyPlotter.h"
#include "Checksum.h"

class GraphAudioPort : public GraphPort {
    const shoop_shared_ptr<shoop_types::_AudioPort> port = nullptr;

    // Tracy plotters for graph audio port debugging
    TracyPlotter m_plot_frames_processed;
    TracyPlotter m_plot_input_peak;
    TracyPlotter m_plot_output_peak;
    TracyPlotter m_plot_internal_connections;

    // Checksum tracking for data consistency verification
    TracyPlotter m_plot_input_checksum;
    TracyPlotter m_plot_output_checksum;
public:
    GraphAudioPort (shoop_shared_ptr<shoop_types::_AudioPort> const& port,
                        shoop_shared_ptr<BackendSession> const& backend);

    PortInterface &get_port() const override;
    shoop_types::_AudioPort *maybe_audio_port() const override;
    shoop_shared_ptr<PortInterface> maybe_shared_port() const override;
    shoop_shared_ptr<shoop_types::_AudioPort> maybe_shared_audio_port() const override;
    void PROC_internal_connections(uint32_t n_frames) override;
    void PROC_prepare(uint32_t n_frames) override;
    void PROC_process(uint32_t n_frames) override;
};