#include "MidiStateTracker.h"
#include "MidiStateDiffTracker.h"

MidiStateTracker::MidiStateTracker(bool track_notes, bool track_controls, bool track_programs)
    : m_rust(backend_rust::new_midi_state_tracker(track_notes, track_controls, track_programs)) {}

MidiStateTracker& MidiStateTracker::operator=(const MidiStateTracker& other) {
    m_rust->copy_relevant_state(*other.m_rust);
    return *this;
}

void MidiStateTracker::copy_relevant_state(MidiStateTracker const &other) {
    m_rust->copy_relevant_state(*other.m_rust);
}

void MidiStateTracker::clear() {
    m_rust->clear();
}

void MidiStateTracker::process_msg(const uint8_t *data) {
    m_rust->process_msg_raw(data);
}

bool MidiStateTracker::tracking_notes() const {
    return m_rust->tracking_notes();
}

uint32_t MidiStateTracker::n_notes_active() const {
    return m_rust->n_notes_active();
}

std::optional<uint8_t> MidiStateTracker::maybe_current_note_velocity(uint8_t channel, uint8_t note) const {
    auto v = m_rust->maybe_current_note_velocity(channel, note);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_controls() const {
    return m_rust->tracking_controls();
}

std::optional<uint8_t> MidiStateTracker::maybe_cc_value(uint8_t channel, uint8_t controller) {
    auto v = m_rust->maybe_cc_value(channel, controller);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

std::optional<uint16_t> MidiStateTracker::maybe_pitch_wheel_value(uint8_t channel) {
    auto v = m_rust->maybe_pitch_wheel_value(channel);
    if (v == 0x8000) { return std::nullopt; }
    return v;
}

std::optional<uint8_t> MidiStateTracker::maybe_channel_pressure_value(uint8_t channel) {
    auto v = m_rust->maybe_channel_pressure_value(channel);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_programs() const {
    return m_rust->tracking_programs();
}

std::optional<uint8_t> MidiStateTracker::maybe_program_value(uint8_t channel) {
    auto v = m_rust->maybe_program_value(channel);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_anything() const {
    return m_rust->tracking_anything();
}

void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    (void)s;
    // No-op: subscriptions are managed entirely inside Rust via
    // synchronous trait callbacks. See midi_state_tracker.rs.
}

void MidiStateTracker::unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    (void)s;
    // No-op: see subscribe() above.
}

std::vector<std::vector<uint8_t>> MidiStateTracker::state_as_messages() {
    auto flat = m_rust->state_as_messages_flat();
    std::vector<std::vector<uint8_t>> result;
    size_t i = 0;
    while (i < flat.size()) {
        uint8_t len = flat[i];
        ++i;
        result.emplace_back(flat.begin() + i, flat.begin() + i + len);
        i += len;
    }
    return result;
}
