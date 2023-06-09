#pragma once

#include <functional>
#include <memory>
#include <boost/container/flat_set.hpp>
#include <algorithm>
#include <array>

#include "MidiStateTracker.h"

enum class StateDiffTrackerAction {
    ScanDiff,
    ClearDiff,
    None
};

// MidiStateDiffTracker combines two MIDI state trackers and keeps track of the differences
// in state between them. It provides quick access to a shortlist of any notes/controls/programs
// that are not the same in both state trackers, so that it is not necessary to fully iterate
// over all state in both trackers to get this information.
class MidiStateDiffTracker : public MidiStateTracker::Subscriber, public std::enable_shared_from_this<MidiStateDiffTracker> {
    using SharedTracker = std::shared_ptr<MidiStateTracker>;

    // Two bytes identify the state, but don't hold its value. (example: two bytes of CC identify the channel and controller in question).
    // Special for notes is that they may be indicated by NoteOn or NoteOff but this distinction does not mean anything here
    // (example: 0x90, 127 signifies there is a diff on note 127 of channel 0, regardless of what the exact note states are)
    using DifferingState = std::array<uint8_t, 2>;
    
    using DiffSet = boost::container::flat_set<DifferingState>;

    static const size_t default_diffs_size = 256;

    SharedTracker m_a, m_b;
    DiffSet m_diffs;

    void note_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t note, std::optional<uint8_t> maybe_velocity) override {
        if (tracker != m_a.get() && tracker != m_b.get()) { return; }
        auto &other =
            tracker == m_a.get() ? m_b :
            m_a;

        if (other && other->tracking_notes() && other->maybe_current_note_velocity(channel, note) != maybe_velocity) {
            m_diffs.insert({ (uint8_t)(0x90 | channel), note });
        } else {
            m_diffs.erase({ (uint8_t)(0x90 | channel), note });
        }
    }
  
    void cc_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t cc, uint8_t value) override {
        if (tracker != m_a.get() && tracker != m_b.get()) { return; }
        auto &other =
            tracker == m_a.get() ? m_b :
            m_a;
        
        if (other && other->tracking_controls() && other->cc_value(channel, cc) != value) {
            m_diffs.insert({ (uint8_t)(0xB0 | channel), cc });
        } else {
            m_diffs.erase({ (uint8_t)(0xB0 | channel), cc });
        }
    }

    void program_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t program) override {
        if (tracker != m_a.get() && tracker != m_b.get()) { return; }
        auto &other =
            tracker == m_a.get() ? m_b :
            m_a;

        if (other && other->tracking_programs() && other->program_value(channel) != program) {
            m_diffs.insert({ (uint8_t)(0xC0 | channel), 0 });
        } else {
            m_diffs.erase({ (uint8_t)(0xC0 | channel), 0 });
        }
    }
  
    void pitch_wheel_changed(MidiStateTracker *tracker, uint8_t channel, uint16_t pitch) override {
        if (tracker != m_a.get() && tracker != m_b.get()) { return; }
        auto &other =
            tracker == m_a.get() ? m_b :
            m_a;

        if (other && other->tracking_controls() && other->pitch_wheel_value(channel) != pitch) {
            m_diffs.insert({ (uint8_t)(0xE0 | channel), 0 });
        } else {
            m_diffs.erase({ (uint8_t)(0xE0 | channel), 0 });
        }
    }
   
    void channel_pressure_changed(MidiStateTracker *tracker, uint8_t channel, uint8_t pressure) override {
        if (tracker != m_a.get() && tracker != m_b.get()) { return; }
        auto &other =
            tracker == m_a.get() ? m_b :
            m_a;

        if (other && other->tracking_controls() && other->channel_pressure_value(channel) != pressure) {
            m_diffs.insert({ (uint8_t)(0xE0 | channel), 0 });
        } else {
            m_diffs.erase({ (uint8_t)(0xE0 | channel), 0 });
        }
    }
   
public:
    MidiStateDiffTracker() {
        m_diffs.reserve(default_diffs_size);
    }

    MidiStateDiffTracker(SharedTracker a, SharedTracker b, StateDiffTrackerAction action=StateDiffTrackerAction::ScanDiff) {
        m_diffs.reserve(default_diffs_size);
        reset(a, b, action);
    }

    // (Re-)set the diffs to compare.
    void reset(SharedTracker a=nullptr, SharedTracker b=nullptr, StateDiffTrackerAction action=StateDiffTrackerAction::ScanDiff) {
        if (m_a) {
            m_a->unsubscribe(shared_from_this());
        }
        if (m_b) {
            m_b->unsubscribe(shared_from_this());
        }
        m_a = a;
        m_b = b;
        if (m_a) {
            m_a->subscribe(shared_from_this());
        }
        if (m_b) {
            m_b->subscribe(shared_from_this());
        }
        switch (action) {
        case StateDiffTrackerAction::ScanDiff:
            rescan_diff();
            break;
        case StateDiffTrackerAction::ClearDiff:
            clear_diff();
            break;
        case StateDiffTrackerAction::None:
        default:
            break;
        }
    }

    void add_diff(DifferingState a) {
        m_diffs.insert(a);
    }

    void delete_diff(DifferingState a) {
        m_diffs.erase(a);
    }

    void check_note(uint8_t channel, uint8_t note) {
        if (m_a->tracking_notes() && m_b->tracking_notes()) {
            auto a = m_a->maybe_current_note_velocity(channel, note);
            auto b = m_b->maybe_current_note_velocity(channel, note);
            if (a == b) {
                m_diffs.erase({(uint8_t)(0x90 | channel), note});
            } else {
                m_diffs.insert({(uint8_t)(0x90 | channel), note});
            }
        }
    }

    void check_cc(uint8_t channel, uint8_t controller) {
        if (m_a->tracking_controls() && m_b->tracking_controls()) {
            auto a = m_a->cc_value(channel, controller);
            auto b = m_b->cc_value(channel, controller);
            if (a == b) {
                m_diffs.erase({(uint8_t)(0xB0 | channel), controller});
            } else {
                m_diffs.insert({(uint8_t)(0xB0 | channel), controller});
            }
        }
    }

    void check_program(uint8_t channel) {
        if (m_a->tracking_programs() && m_b->tracking_programs()) {
            auto a = m_a->program_value(channel);
            auto b = m_b->program_value(channel);
            if (a == b) {
                m_diffs.erase({(uint8_t)(0xC0 | channel), 0});
            } else {
                m_diffs.insert({(uint8_t)(0xC0 | channel), 0});
            }
        }
    }

    void check_channel_pressure(uint8_t channel) {
        if (m_a->tracking_controls() && m_b->tracking_controls()) {
            auto a = m_a->channel_pressure_value(channel);
            auto b = m_b->channel_pressure_value(channel);
            if (a == b) {
                m_diffs.erase({(uint8_t)(0xD0 | channel), 0});
            } else {
                m_diffs.insert({(uint8_t)(0xD0 | channel), 0});
            }
        }
    }

    void check_pitch_wheel(uint8_t channel) {
        if (m_a->tracking_controls() && m_b->tracking_controls()) {
            auto a = m_a->pitch_wheel_value(channel);
            auto b = m_b->pitch_wheel_value(channel);
            if (a == b) {
                m_diffs.erase({(uint8_t)(0xE0 | channel), 0});
            } else {
                m_diffs.insert({(uint8_t)(0xE0 | channel), 0});
            }
        }
    }

    void rescan_diff() {
        clear_diff();
        if (m_a && m_b && m_a->tracking_notes() && m_b->tracking_notes()) {
            for (uint8_t channel = 0; channel < 16; channel++) {
                for (uint8_t note = 0; note < 128; note++) {
                    check_note(channel, note);
                }
            }
        }
        if (m_a && m_b && m_a->tracking_controls() && m_b->tracking_controls()) {
            for (uint8_t channel = 0; channel < 16; channel++) {
                for (uint8_t controller = 0; controller < 128; controller++) {
                    check_cc(channel, controller);
                }
                check_pitch_wheel(channel);
                check_channel_pressure(channel);
            }
        }
        if (m_a && m_b && m_a->tracking_programs() && m_b->tracking_programs()) {
            for (uint8_t channel = 0; channel < 16; channel++) {
                check_program(channel);
            }
        }
    }

    void clear_diff() {
        m_diffs.clear();
    }

    // "Resolve" the difference.
    // Will call the supplied MIDI message callback with the MIDI messages which would exactly negate the state
    // diffrerences between both states.
    void resolve_to(MidiStateTracker *to, std::function<void(size_t size, uint8_t *data)> put_message_cb,
        bool notes=true, bool controls=true, bool programs=true) {
        if (to != m_a.get() && to != m_b.get()) { return; }
        if (!m_a || !m_b) { return; }
        auto &other =
            to == m_a.get() ? m_b :
            m_a;
        uint8_t data[3];
        for (auto &d : m_diffs) {
            uint8_t kind_part = d[0] & 0xF0;
            uint8_t channel_part = d[0] & 0x0F;
            std::optional<uint8_t> v;
            switch(d[0] & 0xF0) {
                case 0x80:
                case 0x90:
                    if (notes) {
                        v = other->maybe_current_note_velocity(d[0], d[1]);
                        data[0] = v.has_value() ? 0x90 | channel_part : 0x80 | channel_part;
                        data[1] = d[1];
                        data[2] = v.value_or(64);
                        put_message_cb(3, data);
                    }
                    break;
                case 0xB0:
                    if (controls) {
                        data[0] = d[0];
                        data[1] = d[1];
                        data[2] = other->cc_value(channel_part, d[1]);
                        put_message_cb(3, data);
                    }
                    break;
                case 0xC0:
                    if (programs) {
                        data[0] = d[0];
                        data[1] = d[1];
                        data[2] = other->program_value(channel_part);
                        put_message_cb(3, data);
                    }
                    break;
                case 0xD0:
                    if (controls) {
                        data[0] = d[0];
                        data[1] = d[1];
                        data[2] = other->channel_pressure_value(channel_part);
                        put_message_cb(3, data);
                    }
                    break;
                case 0xE0:
                    if (controls) {
                        auto v = other->pitch_wheel_value(channel_part);
                        data[0] = d[0];
                        data[1] = v & 0b1111111;
                        data[2] = (v >> 7) & 0b1111111;
                        put_message_cb(3, data);
                    }
                    break;
            }
        }
    }

    void resolve_to_a(std::function<void(size_t size, uint8_t *data)> put_message_cb, bool notes=true, bool controls=true, bool programs=true) {
        return resolve_to(m_a.get(), put_message_cb, notes, controls, programs);
    }
    void resolve_to_b(std::function<void(size_t size, uint8_t *data)> put_message_cb, bool notes=true, bool controls=true, bool programs=true) {
        return resolve_to(m_b.get(), put_message_cb, notes, controls, programs);
    }

    void set_diff(DiffSet const& diff) {
        m_diffs = diff;
    }

    DiffSet const& get_diff() const { return m_diffs; }

    MidiStateDiffTracker& operator= (MidiStateDiffTracker const& other) {
        m_a = other.m_a;
        m_b = other.m_b;
        set_diff(other.get_diff());
        return *this;
    }
};