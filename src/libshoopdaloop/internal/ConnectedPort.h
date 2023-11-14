#pragma once
#include <memory>
#include <vector>
#include <atomic>
#include "MidiPortInterface.h"
#include "LoggingEnabled.h"
#include "shoop_globals.h"
#include "process_when.h"

class ConnectedPort : public std::enable_shared_from_this<ConnectedPort>,
                       private ModuleLoggingEnabled<"Backend.ConnectedPort"> {
public:
    
    const std::shared_ptr<PortInterface> port;
    std::weak_ptr<Backend> backend;

    std::vector<std::weak_ptr<ConnectedPort>> mp_passthrough_to;

    // Audio only
    shoop_types::audio_sample_t* maybe_audio_buffer;
    std::atomic<float> volume;
    std::atomic<float> peak;

    // Dummy audio only (testing purposes)
    std::vector<float> stored_dummy_data;

    // Midi only
    MidiReadableBufferInterface *maybe_midi_input_buffer;
    MidiWriteableBufferInterface *maybe_midi_output_buffer;
    std::shared_ptr<MidiMergingBuffer> maybe_midi_output_merging_buffer;
    std::atomic<uint32_t> n_events_processed;
    std::shared_ptr<MidiStateTracker> maybe_midi_state;

    // Both
    std::atomic<bool> muted;
    std::atomic<bool> passthrough_muted;
    shoop_types::ProcessWhen ma_process_when;

    ConnectedPort (std::shared_ptr<PortInterface> const& port,
                   std::shared_ptr<Backend> const& backend,
                   shoop_types::ProcessWhen process_when);

    void PROC_reset_buffers();
    void PROC_ensure_buffer(uint32_t n_frames, bool do_zero=false);
    void PROC_passthrough(uint32_t n_frames);
    void PROC_passthrough_audio(uint32_t n_frames, ConnectedPort &to);
    void PROC_passthrough_midi(uint32_t n_frames, ConnectedPort &to);
    bool PROC_check_buffer(bool raise_if_absent=true);
    void PROC_finalize_process(uint32_t n_frames);

    void connect_passthrough(std::shared_ptr<ConnectedPort> const& other);

    std::shared_ptr<shoop_types::AudioPort> maybe_audio();
    std::shared_ptr<shoop_types::MidiPort> maybe_midi();
    Backend &get_backend();
};