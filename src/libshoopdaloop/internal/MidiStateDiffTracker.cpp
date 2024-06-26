#include "MidiStateDiffTracker.h"
#include "types.h"
#include <algorithm>
#include <array>
#include <boost/container/flat_set.hpp>
#include <functional>
#include <iostream>
#include <memory>
#include <fmt/format.h>

void MidiStateDiffTracker::note_changed(MidiStateTracker *tracker,
                                        uint8_t channel, uint8_t note,
                                        std::optional<uint8_t> maybe_velocity) {
    if (tracker != m_a.get() && tracker != m_b.get()) {
        log<log_level_debug_trace>("ignore note change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;

    if (other && other->tracking_notes() &&
        other->maybe_current_note_velocity(channel, note) != maybe_velocity) {
        m_diffs.insert({(uint8_t)(0x90 | channel), note});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_current_note_velocity(channel, note);
            int f = _f.has_value() ? _f.value() : -1;
            int t = maybe_velocity.has_value() ? maybe_velocity.value() : -1;
            log<log_level_debug_trace>("New note diff: tracker {} updated vs. {}, chan {}, note {}, vel {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, note, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0x90 | channel), note});
        log<log_level_debug_trace>("Forget note diff: tracker {} updated vs. {}, chan {}, note {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, note);
    }
}

void MidiStateDiffTracker::cc_changed(MidiStateTracker *tracker,
                                      uint8_t channel, uint8_t cc,
                                      std::optional<uint8_t> value) {
    if (tracker != m_a.get() && tracker != m_b.get()) {
        log<log_level_debug_trace>("ignore cc change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;

    if (other && other->tracking_controls() &&
        other->maybe_cc_value(channel, cc) != value &&
        value.has_value()) {
        m_diffs.insert({(uint8_t)(0xB0 | channel), cc});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_cc_value(channel, cc);
            int f = _f.has_value() ? _f.value() : -1;
            int t = value.has_value() ? value.value() : -1;
            log<log_level_debug_trace>("New CC diff: tracker {} updated vs. {}, chan {}, CC {}, val {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, cc, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0xB0 | channel), cc});
        log<log_level_debug_trace>("Forget CC diff: tracker {} updated vs. {}, chan {}, CC {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, cc);
    }
}

void MidiStateDiffTracker::program_changed(MidiStateTracker *tracker,
                                           uint8_t channel, std::optional<uint8_t> program) {
    if (tracker != m_a.get() && tracker != m_b.get()) {
        log<log_level_debug_trace>("ignore program change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;

    if (other && other->tracking_programs() &&
        other->maybe_program_value(channel) != program &&
        program.has_value()) {
        m_diffs.insert({(uint8_t)(0xC0 | channel), 0});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_program_value(channel);
            int f = _f.has_value() ? _f.value() : -1;
            int t = program.has_value() ? program.value() : -1;
            log<log_level_debug_trace>("New program diff: tracker {} updated vs. {}, chan {}, program {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0xC0 | channel), 0});
        log<log_level_debug_trace>("Forget program diff: tracker {} updated vs. {}, chan {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel);
    }
}

void MidiStateDiffTracker::pitch_wheel_changed(MidiStateTracker *tracker,
                                               uint8_t channel,
                                               std::optional<uint16_t> pitch) {
    if (tracker != m_a.get() && tracker != m_b.get()) {
        log<log_level_debug_trace>("ignore pitch wheel change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;

    if (other && other->tracking_controls() &&
        other->maybe_pitch_wheel_value(channel) != pitch &&
        pitch.has_value()) {
        m_diffs.insert({(uint8_t)(0xE0 | channel), 0});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_program_value(channel);
            int f = _f.has_value() ? _f.value() : -1;
            int t = pitch.has_value() ? pitch.value() : -1;
            log<log_level_debug_trace>("New pitch wheel diff: tracker {} updated vs. {}, chan {}, wheel {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0xE0 | channel), 0});
        log<log_level_debug_trace>("Forget pitch wheel diff: tracker {} updated vs. {}, chan {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel);
    }
}

void MidiStateDiffTracker::channel_pressure_changed(MidiStateTracker *tracker,
                                                    uint8_t channel,
                                                    std::optional<uint8_t> pressure) {
    if (tracker != m_a.get() && tracker != m_b.get()) {
        log<log_level_debug_trace>("ignore channel pressure change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;

    if (other && other->tracking_controls() &&
        other->maybe_channel_pressure_value(channel) != pressure &&
        pressure.has_value()) {
        m_diffs.insert({(uint8_t)(0xE0 | channel), 0});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_program_value(channel);
            int f = _f.has_value() ? _f.value() : -1;
            int t = pressure.has_value() ? pressure.value() : -1;
            log<log_level_debug_trace>("New channel pressure diff: tracker {} updated vs. {}, chan {}, pressure {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0xE0 | channel), 0});
        log<log_level_debug_trace>("Forget channel pressure diff: tracker {} updated vs. {}, chan {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel);
    }
}

MidiStateDiffTracker::MidiStateDiffTracker() {
    m_diffs.reserve(default_diffs_size);
}

MidiStateDiffTracker::MidiStateDiffTracker(SharedTracker a, SharedTracker b,
                                           StateDiffTrackerAction action) {
    m_diffs.reserve(default_diffs_size);
    reset(a, b, action);
}

void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b,
                                 StateDiffTrackerAction action) {
    if (m_a) {
        m_a->unsubscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    if (m_b) {
        m_b->unsubscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    m_a = a;
    m_b = b;
    if (m_a) {
        m_a->subscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    if (m_b) {
        m_b->subscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
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

void MidiStateDiffTracker::add_diff(DifferingState a) { m_diffs.insert(a); }

void MidiStateDiffTracker::delete_diff(DifferingState a) { m_diffs.erase(a); }

void MidiStateDiffTracker::check_note(uint8_t channel, uint8_t note) {
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

void MidiStateDiffTracker::check_cc(uint8_t channel, uint8_t controller) {
    if (m_a->tracking_controls() && m_b->tracking_controls()) {
        auto a = m_a->maybe_cc_value(channel, controller);
        auto b = m_b->maybe_cc_value(channel, controller);
        if (a == b) {
            m_diffs.erase({(uint8_t)(0xB0 | channel), controller});
        } else {
            m_diffs.insert({(uint8_t)(0xB0 | channel), controller});
        }
    }
}

void MidiStateDiffTracker::check_program(uint8_t channel) {
    if (m_a->tracking_programs() && m_b->tracking_programs()) {
        auto a = m_a->maybe_program_value(channel);
        auto b = m_b->maybe_program_value(channel);
        if (a == b) {
            m_diffs.erase({(uint8_t)(0xC0 | channel), 0});
        } else {
            m_diffs.insert({(uint8_t)(0xC0 | channel), 0});
        }
    }
}

void MidiStateDiffTracker::check_channel_pressure(uint8_t channel) {
    if (m_a->tracking_controls() && m_b->tracking_controls()) {
        auto a = m_a->maybe_channel_pressure_value(channel);
        auto b = m_b->maybe_channel_pressure_value(channel);
        if (a == b) {
            m_diffs.erase({(uint8_t)(0xD0 | channel), 0});
        } else {
            m_diffs.insert({(uint8_t)(0xD0 | channel), 0});
        }
    }
}

void MidiStateDiffTracker::check_pitch_wheel(uint8_t channel) {
    if (m_a->tracking_controls() && m_b->tracking_controls()) {
        auto a = m_a->maybe_pitch_wheel_value(channel);
        auto b = m_b->maybe_pitch_wheel_value(channel);
        if (a == b) {
            m_diffs.erase({(uint8_t)(0xE0 | channel), 0});
        } else {
            m_diffs.insert({(uint8_t)(0xE0 | channel), 0});
        }
    }
}

void MidiStateDiffTracker::rescan_diff() {
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

void MidiStateDiffTracker::clear_diff() { m_diffs.clear(); }

void MidiStateDiffTracker::resolve_to(
    MidiStateTracker *to,
    std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes,
    bool controls, bool programs) {
    if (to != m_a.get() && to != m_b.get()) {
        return;
    }
    if (!m_a || !m_b) {
        return;
    }
    auto &from = to == m_a.get() ? *m_b : *m_a;
    log<log_level_debug_trace>("Resolve from {} to {}", fmt::ptr(&from), fmt::ptr(to));
    uint8_t data[3];
    for (auto &d : m_diffs) {
        uint8_t kind_part = d[0] & 0xF0;
        uint8_t channel_part = d[0] & 0x0F;
        std::optional<uint8_t> v;
        switch (d[0] & 0xF0) {
        case 0x80:
        case 0x90:
            if (notes) {
                v = from.maybe_current_note_velocity(d[0], d[1]);
                if (should_log<log_level_debug_trace>()) {
                    auto o = to->maybe_current_note_velocity(d[0], d[1]);
                    int _v = v.has_value() ? (int) v.value() : -1;
                    int _o = o.has_value() ? (int) o.value() : -1;
                    log<log_level_debug_trace>("  - note {}.{} : {} -> {}", d[0], d[1], _o, _v);
                }
                data[0] =
                    v.has_value() ? 0x90 | channel_part : 0x80 | channel_part;
                data[1] = d[1];
                data[2] = v.value_or(64);
                put_message_cb(3, data);
            }
            break;
        case 0xB0:
            if (controls) {
                v = from.maybe_cc_value(channel_part, d[1]);
                if (v.has_value()) {
                    if (should_log<log_level_debug_trace>()) {
                        auto o = to->maybe_cc_value(d[0], d[1]);
                        int _v = v.has_value() ? (int) v.value() : -1;
                        int _o = o.has_value() ? (int) o.value() : -1;
                        log<log_level_debug_trace>("  - control {}.{} : {} -> {}", d[0], d[1], _o, _v);
                    }
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xC0:
            if (programs) {
                v = from.maybe_program_value(channel_part);
                if (v.has_value()) {
                    if (should_log<log_level_debug_trace>()) {
                        auto o = to->maybe_program_value(channel_part);
                        int _v = v.has_value() ? (int) v.value() : -1;
                        int _o = o.has_value() ? (int) o.value() : -1;
                        log<log_level_debug_trace>("  - program {}: {} -> {}", channel_part, _o, _v);
                    }
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xD0:
            if (controls) {
                v = from.maybe_channel_pressure_value(channel_part);
                if (v.has_value()) {
                    if (should_log<log_level_debug_trace>()) {
                        auto o = to->maybe_channel_pressure_value(channel_part);
                        int _v = v.has_value() ? (int) v.value() : -1;
                        int _o = o.has_value() ? (int) o.value() : -1;
                        log<log_level_debug_trace>("  - channel pressure {}: {} -> {}", channel_part, _o, _v);
                    }
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xE0:
            if (controls) {
                auto v = from.maybe_pitch_wheel_value(channel_part);
                if (v.has_value()) {
                    if (should_log<log_level_debug_trace>()) {
                        auto o = to->maybe_pitch_wheel_value(channel_part);
                        int _v = v.has_value() ? (int) v.value() : -1;
                        int _o = v.has_value() ? (int) o.value() : -1;
                        log<log_level_debug_trace>("  - pitch wheel {}: {} -> {}", channel_part, _o, _v);
                    }
                    data[0] = d[0];
                    data[1] = v.value() & 0b1111111;
                    data[2] = (v.value() >> 7) & 0b1111111;
                    put_message_cb(3, data);
                }
            }
            break;
        }
    }
}

void MidiStateDiffTracker::resolve_to_a(
    std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes,
    bool controls, bool programs) {
    return resolve_to(m_a.get(), put_message_cb, notes, controls, programs);
}
void MidiStateDiffTracker::resolve_to_b(
    std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes,
    bool controls, bool programs) {
    return resolve_to(m_b.get(), put_message_cb, notes, controls, programs);
}

void MidiStateDiffTracker::set_diff(DiffSet const &diff) { m_diffs = diff; }

MidiStateDiffTracker::DiffSet const &MidiStateDiffTracker::get_diff() const {
    return m_diffs;
}
MidiStateDiffTracker::SharedTracker MidiStateDiffTracker::a() const {
    return m_a;
}
MidiStateDiffTracker::SharedTracker MidiStateDiffTracker::b() const {
    return m_b;
}