#pragma once
#include <cstdint>
#include <optional>
#include <vector>
#include "shoop_shared_ptr.h"
#include "backend_rust/src/cxx.rs.h"

class MidiStateDiffTracker;

class MidiStateTracker {
    rust::Box<backend_rust::MidiStateTracker> m_rust;

public:
    MidiStateTracker(bool track_notes, bool track_controls, bool track_programs);

    MidiStateTracker(const MidiStateTracker&) = delete;
    MidiStateTracker& operator=(const MidiStateTracker&) = delete;

    void copy_relevant_state(MidiStateTracker const &other);
    void clear();
    void process_msg(const uint8_t *data);
    bool tracking_notes() const;
    uint32_t n_notes_active() const;
    std::optional<uint8_t> maybe_current_note_velocity(uint8_t channel, uint8_t note) const;
    bool tracking_controls() const;
    std::optional<uint8_t> maybe_cc_value(uint8_t channel, uint8_t controller);
    std::optional<uint16_t> maybe_pitch_wheel_value(uint8_t channel);
    std::optional<uint8_t> maybe_channel_pressure_value(uint8_t channel);
    bool tracking_programs() const;
    std::optional<uint8_t> maybe_program_value(uint8_t channel);
    bool tracking_anything() const;
    void subscribe(shoop_shared_ptr<MidiStateDiffTracker> s);
    void unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s);
    std::vector<std::vector<uint8_t>> state_as_messages();

    backend_rust::MidiStateTracker* raw_ptr() { return m_rust.get(); }
    backend_rust::MidiStateTracker const* raw_ptr() const { return m_rust.get(); }
};
