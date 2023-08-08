#pragma once
#include <memory>
#include <vector>
#include <atomic>
#include "MidiPortInterface.h"
#include "LoggingEnabled.h"
#include "shoop_globals.h"

class Backend;
class PortInterface;
class MidiReadableBufferInterface;
class MidiWriteableBufferInterface;
class MidiMergingBuffer;
class MidiStateTracker;

struct ConnectedPort : public std::enable_shared_from_this<ConnectedPort>,
                       private ModuleLoggingEnabled {
    std::string log_module_name() const override;
    
    const std::shared_ptr<PortInterface> port;
    std::weak_ptr<Backend> backend;

    std::vector<std::weak_ptr<ConnectedPort>> mp_passthrough_to;

    // Audio only
    shoop_types::audio_sample_t* maybe_audio_buffer;
    std::atomic<float> volume;
    std::atomic<float> peak;

    // Midi only
    MidiReadableBufferInterface *maybe_midi_input_buffer;
    MidiWriteableBufferInterface *maybe_midi_output_buffer;
    std::shared_ptr<MidiMergingBuffer> maybe_midi_output_merging_buffer;
    std::atomic<size_t> n_events_processed;
    std::shared_ptr<MidiStateTracker> maybe_midi_state;

    // Both
    std::atomic<bool> muted;
    std::atomic<bool> passthrough_muted;
    shoop_types::ProcessWhen ma_process_when;

    ConnectedPort (std::shared_ptr<PortInterface> const& port, std::shared_ptr<Backend> const& backend);

    void PROC_reset_buffers();
    void PROC_ensure_buffer(size_t n_frames, bool do_zero=false);
    void PROC_passthrough(size_t n_frames);
    void PROC_passthrough_audio(size_t n_frames, ConnectedPort &to);
    void PROC_passthrough_midi(size_t n_frames, ConnectedPort &to);
    void PROC_check_buffer();
    void PROC_finalize_process(size_t n_frames);

    void connect_passthrough(std::shared_ptr<ConnectedPort> const& other);

    shoop_types::AudioPort *maybe_audio();
    shoop_types::MidiPort *maybe_midi();
    Backend &get_backend();
};