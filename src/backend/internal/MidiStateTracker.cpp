#include "MidiStateTracker.h"
#include "MidiStateDiffTracker.h"

MidiStateTracker::MidiStateTracker(bool track_notes, bool track_controls, bool track_programs)
    : m_rust(backend_rust::new_midi_state_tracker(track_notes, track_controls, track_programs)) {}

void MidiStateTracker::copy_relevant_state(MidiStateTracker const &other) {
    backend_rust::copy_relevant_state(*m_rust, *other.m_rust);
}

void MidiStateTracker::clear() {
    backend_rust::clear(*m_rust);
}

void MidiStateTracker::process_msg(const uint8_t *data) {
    backend_rust::process_msg_raw(*m_rust, data);
}

bool MidiStateTracker::tracking_notes() const {
    return backend_rust::tracking_notes(*m_rust);
}

uint32_t MidiStateTracker::n_notes_active() const {
    return backend_rust::n_notes_active(*m_rust);
}

std::optional<uint8_t> MidiStateTracker::maybe_current_note_velocity(uint8_t channel, uint8_t note) const {
    auto v = backend_rust::maybe_current_note_velocity(*m_rust, channel, note);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_controls() const {
    return backend_rust::tracking_controls(*m_rust);
}

std::optional<uint8_t> MidiStateTracker::maybe_cc_value(uint8_t channel, uint8_t controller) {
    auto v = backend_rust::maybe_cc_value(*m_rust, channel, controller);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

std::optional<uint16_t> MidiStateTracker::maybe_pitch_wheel_value(uint8_t channel) {
    auto v = backend_rust::maybe_pitch_wheel_value(*m_rust, channel);
    if (v == 0x8000) { return std::nullopt; }
    return v;
}

std::optional<uint8_t> MidiStateTracker::maybe_channel_pressure_value(uint8_t channel) {
    auto v = backend_rust::maybe_channel_pressure_value(*m_rust, channel);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_programs() const {
    return backend_rust::tracking_programs(*m_rust);
}

std::optional<uint8_t> MidiStateTracker::maybe_program_value(uint8_t channel) {
    auto v = backend_rust::maybe_program_value(*m_rust, channel);
    if (v == 0x80) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_anything() const {
    return backend_rust::tracking_anything(*m_rust);
}

void MidiStateTracker::subscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    if (s) backend_rust::subscribe(*m_rust, s->raw_ptr());
}

void MidiStateTracker::unsubscribe(shoop_shared_ptr<MidiStateDiffTracker> s) {
    if (s) backend_rust::unsubscribe(*m_rust, s->raw_ptr());
}

std::vector<std::vector<uint8_t>> MidiStateTracker::state_as_messages() {
    auto flat = backend_rust::state_as_messages_flat(*m_rust);
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
