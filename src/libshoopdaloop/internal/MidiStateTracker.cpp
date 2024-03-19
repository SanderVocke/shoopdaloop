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
#include <fmt/format.h>

using namespace logging;

uint32_t MidiStateTracker::cc_index(uint8_t channel, uint8_t cc) {
    return (uint32_t)(channel & 0x0F) * 128 + (uint32_t)cc;
}

uint32_t MidiStateTracker::note_index(uint8_t channel, uint8_t note) {
    return (uint32_t)(channel & 0x0F) * 128 + (uint32_t)note;
}

void MidiStateTracker::process_noteOn(uint8_t channel, uint8_t note,
                                      uint8_t velocity) {
    if (m_notes_active_velocities.size() == 0) {
        log<log_level_debug_trace>("Ignore note on (not tracking)");
        return;
    }
    
    log<log_level_debug_trace>("Process note on: {}, {}, {}", channel, note, velocity);

    if (m_notes_active_velocities.at(note_index(channel, note)) == NoteInactive) {
        m_n_notes_active++;
    }
    if (m_notes_active_velocities.at(note_index(channel, note)) != velocity) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->note_changed(this, channel, note, velocity);
            }
        }
    }
    m_notes_active_velocities.at(note_index(channel, note)) = velocity;
}

void MidiStateTracker::process_allNotesOff(uint8_t channel) {
    if (m_notes_active_velocities.size() == 0) {
        log<log_level_debug_trace>("Ignore all notes off (not tracking)");
        return;
    }

    log<log_level_debug_trace>("Process all notes off: {}", channel);

    for (size_t note=0; note < 128; note++) {
        if (m_notes_active_velocities.at(note_index(channel, note)) != NoteInactive) {
            m_n_notes_active = std::max(0, m_n_notes_active - 1);
            for (auto const &s : m_subscribers) {
                if (auto ss = s.lock()) {
                    ss->note_changed(this, channel, note, std::nullopt);
                }
            }
        }
        m_notes_active_velocities.at(note_index(channel, note)) = NoteInactive;
    }
}

void MidiStateTracker::process_noteOff(uint8_t channel, uint8_t note) {
    if (m_notes_active_velocities.size() == 0) {
        log<log_level_debug_trace>("Ignore note off (not tracking)");
        return;
    }

    log<log_level_debug_trace>("Process note off: {}, {}", channel, note);

    if (m_notes_active_velocities.at(note_index(channel, note)) != NoteInactive) {
        m_n_notes_active = std::max(0, m_n_notes_active - 1);
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->note_changed(this, channel, note, std::nullopt);
            }
        }
    }
    m_notes_active_velocities.at(note_index(channel, note)) = NoteInactive;
}

void MidiStateTracker::process_cc(uint8_t channel, uint8_t controller,
                                  uint8_t value) {
    if (m_controls.size() == 0) {
        log<log_level_debug_trace>("Ignore CC (not tracking)");
        return;
    }

    auto idx = cc_index(channel & 0x0F, controller);
    auto current = m_controls.at(idx);

    log<log_level_debug_trace>("Process cc: {}, {}, {} -> {}", channel, controller, current, value);

    if (current != value) {
        for (auto const &s : m_subscribers) {
            if (auto ss = s.lock()) {
                ss->cc_changed(this, channel, controller, value);
            }
        }
    }
    m_controls[idx] = (unsigned char)value;
}

void MidiStateTracker::process_program(uint8_t channel, uint8_t value) {
    if (m_programs.size() == 0) {
        log<log_level_debug_trace>("Ignore PC (not tracking)");
        return;
    }
    log<log_level_debug_trace>("Process program: {}, {}", channel, value);

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
    if (m_pitch_wheel.size() == 0) {
        log<log_level_debug_trace>("Ignore pitch wheel (not tracking)");
        return;
    }
    log<log_level_debug_trace>("Process pitch wheel: {}, {}", channel, value);

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
    if (m_channel_pressure.size() == 0) {
        log<log_level_debug_trace>("Ignore channel pressure (not tracking)");
        return;
    }
    log<log_level_debug_trace>("Process channel pressure: {}, {}", channel, value);
    
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
      m_controls(track_controls ? 16 * 128 : 0),
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
    log<log_level_debug>("Copy state from {}", fmt::ptr(&other));

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

std::vector<std::vector<uint8_t>> MidiStateTracker::state_as_messages() {
    std::vector<std::vector<uint8_t>> rval;

    if (tracking_programs()) {
        for (uint8_t channel; channel < (uint8_t) m_programs.size(); channel++) {
            auto v = m_programs[channel];
            if (v != ProgramUnknown) {
                rval.push_back(programChange(channel, v));
            }
        }
    }
    if (tracking_controls()) {
        for (uint8_t channel; channel < (uint8_t) m_programs.size(); channel++) {
            auto pw = m_pitch_wheel[channel];
            auto cp = m_channel_pressure[channel];
            if (pw != PitchWheelUnknown && pw != PitchWheelDefault) { rval.push_back(pitchWheelChange(channel, pw)); }
            if (cp != ChannelPressureUnknown) { rval.push_back(channelPressure(channel, cp)); }

            for (uint8_t controller; controller < 128; controller++) {
                auto v = m_controls[channel * 128 + controller];
                if (v != default_cc(channel, controller)) { rval.push_back(cc(channel, controller, v)); }
            }
        }
    }
    if (tracking_notes()) {
        for (uint8_t chan=0; chan < (uint8_t) m_notes_active_velocities.size(); chan++) {
            for (uint8_t note=0; note < 128; note++) {
                auto v = m_notes_active_velocities[note_index(chan, note)];
                if (v != NoteInactive) {
                    rval.push_back(noteOn(chan, note, v));
                }
            }
        }
    }

    return rval;    
}

uint8_t MidiStateTracker::default_cc (uint8_t channel, uint8_t controller) {
    if (controller == 64 || controller == 69) {
        // Special case: hold pedal assumed off
        return 0;
    } else {
        return CCValueUnknown;
    }
}

void MidiStateTracker::clear() {
    log<log_level_debug>("Clear");

    for (size_t n=0; n < m_notes_active_velocities.size(); n++) {
        m_notes_active_velocities[n] = NoteInactive;
    }
    m_n_notes_active = 0;
    for (auto &p : m_pitch_wheel) {
        p = PitchWheelDefault;
    }
    for (auto &p : m_programs) {
        p = ProgramUnknown;
    }
    for (auto &p : m_channel_pressure) {
        p = ChannelPressureUnknown;
    }
    for (size_t cc=0; cc < m_controls.size(); cc++) {
        m_controls[cc] = default_cc(cc / 128, cc % 128);
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
        process_allNotesOff(*chan);
    } else if (auto chan = is_all_sound_off_for_channel(data)) {
        process_allNotesOff(*chan);
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
    if (m_notes_active_velocities.size() < (note_index(channel, note))) { return std::nullopt; }
    auto v =  m_notes_active_velocities[note_index(channel, note)];
    if (v == NoteInactive) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_controls() const {
    return m_controls.size() > 0;
}

std::optional<uint8_t> MidiStateTracker::maybe_cc_value(uint8_t channel, uint8_t controller) {
    auto v =  m_controls.at(cc_index(channel & 0x0F, controller));
    if (v == CCValueUnknown) { return std::nullopt; }
    return v;
}

std::optional<uint16_t> MidiStateTracker::maybe_pitch_wheel_value(uint8_t channel) {
    auto v = m_pitch_wheel.at(channel & 0x0F);
    if (v == PitchWheelUnknown) { return std::nullopt; }
    return v;
}

std::optional<uint8_t> MidiStateTracker::maybe_channel_pressure_value(uint8_t channel) {
    auto v = m_channel_pressure.at(channel & 0x0F);
    if (v == ChannelPressureUnknown) { return std::nullopt; }
    return v;
}

bool MidiStateTracker::tracking_programs() const {
    return m_programs.size() > 0;
}

std::optional<uint8_t> MidiStateTracker::maybe_program_value(uint8_t channel) {
    auto v = m_programs.at(channel & 0x0F);
    if (v == ProgramUnknown) { return std::nullopt; }
    return v;
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