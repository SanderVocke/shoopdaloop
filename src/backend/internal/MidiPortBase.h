#pragma once
#include "IMidiStateTracking.h"
#include "IMidiRingbuffer.h"
#include "shoop_shared_ptr.h"
#include "LoggingEnabled.h"
#include "MidiStateTracker.h"
#include "MidiRingbuffer.h"
#include <atomic>
#include <cstdint>
#include <optional>

// Forward declaration
class IMidiStorage;

/**
 * MidiPortBase - Core implementation of MIDI port logic.
 * 
 * This class contains the core state tracking, ringbuffer management,
 * and event counting logic that can be composed into different MIDI port
 * implementations (MidiPort, DummyMidiPort, etc.).
 * 
 * Does NOT inherit from PortInterface - maintains separate identity to
 * support composition without diamond inheritance issues.
 * 
 * This class is thread-safe for atomic members (event counts, muted state).
 */
class MidiPortBase : public IMidiStateTracking,
                     public IMidiRingbuffer,
                     private ModuleLoggingEnabled<"Backend.MidiPortBase"> {
public:
    /**
     * Configuration for what to track in MIDI state.
     */
    struct TrackingConfig {
        bool track_notes;
        bool track_controls;
        bool track_programs;
        
        TrackingConfig(bool notes = true, bool controls = true, bool programs = true)
            : track_notes(notes), track_controls(controls), track_programs(programs) {}
    };

private:
    // Tracking configuration
    TrackingConfig m_tracking_config;

    // The current MIDI state on the port.
    shoop_shared_ptr<MidiStateTracker> m_maybe_midi_state;

    // The MIDI state at the tail of the ringbuffer. This is basically a time-delayed
    // version of the current state.
    shoop_shared_ptr<MidiStateTracker> m_ringbuffer_tail_state;

    // The MIDI ringbuffer for retroactive recording
    shoop_shared_ptr<MidiRingbuffer> m_midi_ringbuffer;

    // Mute state
    std::atomic<bool> m_muted = false;

    // Event counters
    std::atomic<uint32_t> n_input_events = 0;
    std::atomic<uint32_t> n_output_events = 0;

    // Friend classes for test helper access
    friend class MidiPort;
    friend class MidiPortTestHelper;

public:
    // IMidiStateTracking interface implementation
    uint32_t n_notes_active() const override;
    uint32_t get_input_event_count() const override;
    uint32_t get_output_event_count() const override;

    // IMidiRingbuffer interface implementation
    void set_n_samples(uint32_t n) override;
    uint32_t get_n_samples() const override;
    void snapshot(IMidiStorage& target, std::optional<uint32_t> start_offset_from_end = std::nullopt) const override;
    uint32_t get_current_n_samples() const override;

    // State tracker accessors
    shoop_shared_ptr<MidiStateTracker>& maybe_midi_state_tracker();
    const shoop_shared_ptr<MidiStateTracker>& maybe_midi_state_tracker() const;
    shoop_shared_ptr<MidiStateTracker>& maybe_ringbuffer_tail_state_tracker();
    const shoop_shared_ptr<MidiStateTracker>& maybe_ringbuffer_tail_state_tracker() const;

    // Ringbuffer accessors
    shoop_shared_ptr<MidiRingbuffer>& maybe_midi_ringbuffer();
    const shoop_shared_ptr<MidiRingbuffer>& maybe_midi_ringbuffer() const;

    // Mute state accessors
    void set_muted(bool muted);
    bool get_muted() const;

    // Event counter management
    void reset_n_input_events();
    uint32_t get_n_input_events() const;
    void increment_input_events(uint32_t count);

    void reset_n_output_events();
    uint32_t get_n_output_events() const;
    void increment_output_events(uint32_t count);

    // State processing helpers
    void process_msg_to_state(const uint8_t* data);
    void snapshot_ringbuffer_into(IMidiStorage& s) const;

    // Constructor with tracking configuration
    MidiPortBase(const TrackingConfig& config);
    
    // Constructor with individual tracking flags (backward compatible)
    MidiPortBase(bool track_notes, bool track_controls, bool track_programs);
    
    virtual ~MidiPortBase();
};