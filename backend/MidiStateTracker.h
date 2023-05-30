#pragma once
#include <vector>
#include <cstddef>
#include <cstdint>
#include <optional>
#include <atomic>
#include <functional>
#include <set>
#include <memory>

#include "midi_helpers.h"

// If you feed MidiStateTracker a sequence of MIDI messages, it will keep track of current state
// on the MIDI stream, optionally including:
// - Notes on or off (including if terminated by All Sound Off / All Notes Off, but not including hold pedals)
// - CC values
// - Progam changes
// - Pitch wheel and channel pressure
class MidiStateTracker {
public:
    class Subscriber {
    public:
        virtual void note_changed(MidiStateTracker *from, uint8_t channel, uint8_t note, std::optional<uint8_t> maybe_velocity) = 0;
        virtual void cc_changed(MidiStateTracker *from, uint8_t channel, uint8_t cc, uint8_t value) = 0;
        virtual void program_changed(MidiStateTracker *from, uint8_t channel, uint8_t program) = 0;
        virtual void pitch_wheel_changed(MidiStateTracker *from, uint8_t channel, uint8_t pitch) = 0;
        virtual void channel_pressure_changed(MidiStateTracker *from, uint8_t channel, uint8_t pressure) = 0;
    };

private:
    std::atomic<int> m_n_notes_active;
    std::vector<std::optional<uint8_t>> m_notes_active_velocities;        // Track Note On / Note Off (16*128)
    std::vector<uint8_t> m_controls;                       // Track CC values          (128*128)
    std::vector<uint8_t> m_programs;                       // Track Program values     (16)
    std::vector<uint8_t> m_pitch_wheel;                    // 16 channels
    std::vector<uint8_t> m_channel_pressure;               // 16 channels
    std::set<std::weak_ptr<Subscriber>> m_subscribers;


    static size_t note_index (uint8_t channel, uint8_t note) { return (size_t)channel * 128 + (size_t)note; }
    static size_t cc_index (uint8_t channel, uint8_t cc) { return (size_t)channel * 128 + (size_t)cc; }

    void process_noteOn(uint8_t channel, uint8_t note, uint8_t velocity) {
        if (m_notes_active_velocities.size() == 0) { return; }

        auto idx = note_index(channel & 0x0F, note);
        if (!m_notes_active_velocities.at(idx).has_value()) {
            m_n_notes_active++;
        }
        if (m_notes_active_velocities.at(idx).value_or(255) != velocity) {
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->note_changed(this, channel, note, velocity); } }
        }
        m_notes_active_velocities.at(idx) = velocity;
    }

    void process_noteOff(uint8_t channel, uint8_t note) {
        if (m_notes_active_velocities.size() == 0) { return; }

        auto idx = note_index(channel & 0x0F, note);
        if (m_notes_active_velocities.at(idx).has_value()) {
            m_n_notes_active = std::max(0, m_n_notes_active-1);
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->note_changed(this, channel, note, std::nullopt); } }
        }
        m_notes_active_velocities.at(idx).reset();
    }

    void process_cc(uint8_t channel, uint8_t controller, uint8_t value) {
        if (m_controls.size() == 0) { return; }

        auto idx = cc_index(channel & 0x0F, controller);
        if (m_controls.at(idx) != value) {
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->note_changed(this, channel, controller, value); } }
        }
        m_controls[idx] = (unsigned char) value;
    }

    void process_program(uint8_t channel, uint8_t value) {
        if (m_programs.size() == 0) { return; }
        if (m_programs.at(channel & 0x0F) != value) {
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->program_changed(this, channel, value); } }
        }
        m_programs[channel & 0x0F] = value;
    }
    
    void process_pitch_wheel(uint8_t channel, uint8_t value) {
        if (m_pitch_wheel.size() == 0) { return; }
        if (m_pitch_wheel.at(channel & 0x0F) != value) {
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->pitch_wheel_changed(this, channel, value); } }
        }
        m_pitch_wheel[channel & 0x0F] = value;
    }

    void process_channel_pressure(uint8_t channel, uint8_t value) {
        if (m_channel_pressure.size() == 0) { return; }
        if (m_channel_pressure.at(channel & 0x0F) != value) {
            for (auto const& s : m_subscribers) { if(auto ss = s.lock()) { ss->channel_pressure_changed(this, channel, value); } }
        }
        m_channel_pressure[channel & 0x0F] = value;
    } 

public:
    MidiStateTracker(bool track_notes=false, bool track_controls=false, bool track_programs=false) :
        m_n_notes_active(0),
        m_notes_active_velocities(track_notes ? 16*128 : 0),
        m_controls(track_controls ? 128*128 : 0),
        m_programs(track_programs ? 16 : 0),
        m_pitch_wheel(track_controls ? 16 : 0),
        m_channel_pressure(track_controls ? 16 : 0)
        {
            for (size_t i=0; i<m_notes_active_velocities.size(); i++) {
                m_notes_active_velocities[i].reset();
            }
        }
    
    MidiStateTracker& operator= (MidiStateTracker const& other) {
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
    
    void clear() {
        for(size_t i=0; i<m_notes_active_velocities.size(); i++) {
            m_notes_active_velocities[i].reset();
        }
        m_n_notes_active = 0;
    }

    void process_msg(const uint8_t *data) {
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
            for(size_t i=0; i<128; i++) {
                process_noteOff(*chan, i);
            }
        } else if (auto chan = is_all_sound_off_for_channel(data)) {
            for(size_t i=0; i<128; i++) {
                process_noteOff(*chan, i);
            }
        }
    }

    bool tracking_notes() const { return m_notes_active_velocities.size() > 0; }
    size_t n_notes_active() const { return m_n_notes_active; }
    std::optional<uint8_t> maybe_current_note_velocity(uint8_t channel, uint8_t note) const {
        return m_notes_active_velocities.at(note_index(channel & 0x0F, note));
    }

    bool tracking_controls() const { return m_controls.size() > 0; }
    uint8_t cc_value (uint8_t channel, uint8_t controller) { return m_controls.at(cc_index(channel & 0x0F, controller)); }
    uint8_t pitch_wheel_value (uint8_t channel) { return m_pitch_wheel.at(channel & 0x0F); }
    uint8_t channel_pressure_value (uint8_t channel) { return m_channel_pressure.at(channel & 0x0F); }

    bool tracking_programs() const { return m_programs.size() > 0; }
    uint8_t program_value (uint8_t channel) { return m_programs.at(channel & 0x0F); }

    bool tracking_anything() const { return tracking_programs() || tracking_controls() || tracking_notes(); }

    void subscribe(std::shared_ptr<Subscriber> s) { m_subscribers.insert(s); }
    void unsubscribe(std::shared_ptr<Subscriber> s) { m_subscribers.erase(s); }
};