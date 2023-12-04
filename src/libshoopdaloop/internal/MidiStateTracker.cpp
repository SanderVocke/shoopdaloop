#include "MidiStateTracker.h"
#include "midi_helpers.h"
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <functional>
#include <iostream>
#include <memory>
#include <optional>
#include <vector>

using namespace logging;

uint32_t MidiStateTracker::note_index(uint8_t channel, uint8_t note) {
    return (uint32_t)channel * 128 + (uint32_t)note;
}
uint32_t MidiStateTracker::cc_index(uint8_t channel, uint8_t cc) {
    return (uint32_t)channel * 128 + (uint32_t)cc;
}

void MidiStateTracker::process_noteOn(uint8_t channel, uint8_t note,
                                      uint8_t velocity) {
    log<log_trace>("Process note on: {}, {}, {}", channel, note,
                         velocity);

    if (m_notes_active_velocities.size() == 0) {
        return;
    }

    auto idx = note_index(channel & 0x0F, note);
    if (!m_notes_active_velocities.at(idx).has_value()) {
        m_n_notes_active++;
    }
    if (m_notes_active_velocities.at(idx).value_or(255) != velocity) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->note_changed(this, channel, note, velocity);
            }
        }
    }
    m_notes_active_velocities.at(idx) = velocity;
}

void MidiStateTracker::process_noteOff(uint8_t channel, uint8_t note) {
    log<log_trace>("Process note off: {}, {}", channel, note);

    if (m_notes_active_velocities.size() == 0) {
        return;
    }

    auto idx = note_index(channel & 0x0F, note);
    if (m_notes_active_velocities.at(idx).has_value()) {
        m_n_notes_active = std::max(0, m_n_notes_active - 1);
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->note_changed(this, channel, note, std::nullopt);
            }
        }
    }
    m_notes_active_velocities.at(idx).reset();
}

void MidiStateTracker::process_cc(uint8_t channel, uint8_t controller,
                                  uint8_t value) {
    log<log_trace>("Process cc: {}, {}, {}", channel, controller, value);

    if (m_controls.size() == 0) {
        return;
    }

    auto idx = cc_index(channel & 0x0F, controller);
    if (m_controls.at(idx) != value) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->cc_changed(this, channel, controller, value);
            }
        }
    }
    m_controls[idx] = (unsigned char)value;
}

void MidiStateTracker::process_program(uint8_t channel, uint8_t value) {
    log<log_trace>("Process program: {}, {}", channel, value);

    if (m_programs.size() == 0) {
        return;
    }
    if (m_programs.at(channel & 0x0F) != value) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->program_changed(this, channel, value);
            }
        }
    }
    m_programs[channel & 0x0F] = value;
}

void MidiStateTracker::process_pitch_wheel(uint8_t channel, uint16_t value) {
    log<log_trace>("Process pitch wheel: {}, {}", channel, value);

    if (m_pitch_wheel.size() == 0) {
        return;
    }
    if (m_pitch_wheel.at(channel & 0x0F) != value) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->pitch_wheel_changed(this, channel, value);
            }
        }
    }
    m_pitch_wheel[channel & 0x0F] = value;
}

void MidiStateTracker::process_channel_pressure(uint8_t channel,
                                                uint8_t value) {
    log<log_trace>("Process channel pressure: {}, {}", channel, value);

    if (m_channel_pressure.size() == 0) {
        return;
    }
    if (m_channel_pressure.at(channel & 0x0F) != value) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->channel_pressure_changed(this, channel, value);
            }
        }
    }
    m_channel_pressure[channel & 0x0F] = value;
}

MidiStateTracker::MidiStateTracker(bool track_notes, bool track_controls,
                                   bool track_programs)
    : m_n_notes_active(0),
      m_notes_active_velocities(track_notes ? 16 * 128 : 0),
      m_controls(track_controls ? 128 * 128 : 0),
      m_programs(track_programs ? 16 : 0),
      m_pitch_wheel(track_controls ? 16 : 0),
      m_channel_pressure(track_controls ? 16 : 0) {
    clear();
}

MidiStateTracker &MidiStateTracker::operator=(MidiStateTracker const &other) {
    m_n_notes_active = other.m_n_notes_active.load();
    m_notes_active_velocities = other.m_notes_active_velocities;
    m_controls = other.m_controls;
    m_programs = other.m_programs;
    m_pitch_wheel = other.m_pitch_wheel;
    m_channel_pressure = other.m_channel_pressure;

    // Leave subscribers unchanged: subscribers are not supposed to get
    // additional signals from copies of the origin object.
    return *this;
}

void MidiStateTracker::copy_relevant_state(MidiStateTracker const &other) {
    log<log_debug>("Copy state from other");

    if (tracking_notes()) {
        m_n_notes_active = other.m_n_notes_active.load();
        m_notes_active_velocities = other.m_notes_active_velocities;
    }
    if (tracking_controls()) {
        m_controls = other.m_controls;
        m_pitch_wheel = other.m_pitch_wheel;
        m_channel_pressure = other.m_channel_pressure;
    }
    if (tracking_programs()) {
        m_programs = other.m_programs;
    }
}

void MidiStateTracker::clear() {
    log<log_debug>("Clear");

    for (uint32_t i = 0; i < m_notes_active_velocities.size(); i++) {
        m_notes_active_velocities[i].reset();
    }
    m_n_notes_active = 0;
    for (auto &p : m_pitch_wheel) {
        p = 0x2000;
    }
    for (auto &p : m_programs) {
        p = 0;
    }
    for (auto &p : m_channel_pressure) {
        p = 0;
    }
    for (auto &p : m_controls) {
        p = 0;
    }
}

void MidiStateTracker::process_msg(const uint8_t *data) {
    if (is_noteOn(data)) {
        auto _channel = channel(data);
        auto _note = note(data);
        auto _velocity = velocity(data);
        process_noteOn(_channel, _note, _velocity);
    } else if (is_noteOff(data)) {
        auto _channel = channel(data);
        auto _note = note(data);
        process_noteOff(_channel, _note);
    } else if (auto chan = is_all_notes_off_for_channel(data)) {
        for (uint32_t i = 0; i < 128; i++) {
            process_noteOff(*chan, i);
        }
    } else if (auto chan = is_all_sound_off_for_channel(data)) {
        for (uint32_t i = 0; i < 128; i++) {
            process_noteOff(*chan, i);
        }
    } else if (is_pitch_wheel(data)) {
        uint16_t value = (uint16_t)data[1] | ((uint16_t)data[2] << 7);
        process_pitch_wheel(channel(data), value);
    } else if (is_channel_pressure(data)) {
        process_channel_pressure(channel(data), data[1]);
    } else if (is_program(data)) {
        process_program(channel(data), data[1]);
    } else if (is_cc(data)) {
        process_cc(channel(data), data[1], data[2]);
    }
}

bool MidiStateTracker::tracking_notes() const {
    return m_notes_active_velocities.size() > 0;
}

uint32_t MidiStateTracker::n_notes_active() const { return m_n_notes_active; }

std::optional<uint8_t>
MidiStateTracker::maybe_current_note_velocity(uint8_t channel,
                                              uint8_t note) const {
    return m_notes_active_velocities.at(note_index(channel & 0x0F, note));
}

bool MidiStateTracker::tracking_controls() const {
    return m_controls.size() > 0;
}

uint8_t MidiStateTracker::cc_value(uint8_t channel, uint8_t controller) {
    return m_controls.at(cc_index(channel & 0x0F, controller));
}

uint16_t MidiStateTracker::pitch_wheel_value(uint8_t channel) {
    return m_pitch_wheel.at(channel & 0x0F);
}

uint8_t MidiStateTracker::channel_pressure_value(uint8_t channel) {
    return m_channel_pressure.at(channel & 0x0F);
}

bool MidiStateTracker::tracking_programs() const {
    return m_programs.size() > 0;
}

uint8_t MidiStateTracker::program_value(uint8_t channel) {
    return m_programs.at(channel & 0x0F);
}

bool MidiStateTracker::tracking_anything() const {
    return tracking_programs() || tracking_controls() || tracking_notes();
}

void MidiStateTracker::subscribe(std::shared_ptr<Subscriber> s) {
    m_subscribers.insert(s);
}
void MidiStateTracker::unsubscribe(std::shared_ptr<Subscriber> s) {
    m_subscribers.erase(s);
}