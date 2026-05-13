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
    std::cerr << "[CPP] MidiStateDiffTracker::pitch_wheel_changed() tracker=" << tracker << " ch=" << (int)channel << " pitch=" << (pitch.has_value() ? pitch.value() : -1) << " (0x" << std::hex << (pitch.has_value() ? pitch.value() : 0) << std::dec << ")" << std::endl;
    if (tracker != m_a.get() && tracker != m_b.get()) {
        std::cerr << "[CPP]   -> ignore: unknown tracker" << std::endl;
        log<log_level_debug_trace>("ignore pitch wheel change: unknown tracker");
        return;
    }
    auto &other = tracker == m_a.get() ? m_b : m_a;
    std::cerr << "[CPP]   -> other tracker=" << other.get() << " tracking_controls=" << (other && other->tracking_controls() ? "true" : "false") << std::endl;

    if (other && other->tracking_controls()) {
        auto other_pitch = other->maybe_pitch_wheel_value(channel);
        std::cerr << "[CPP]   -> other pitch=" << (other_pitch.has_value() ? other_pitch.value() : -1) << " -> diff? " << (other_pitch != pitch ? "YES" : "NO") << std::endl;
    }

    if (other && other->tracking_controls() &&
        other->maybe_pitch_wheel_value(channel) != pitch &&
        pitch.has_value()) {
        std::cerr << "[CPP]   -> INSERTING pitch wheel diff [0xE0 | " << (int)channel << ", 0]" << std::endl;
        m_diffs.insert({(uint8_t)(0xE0 | channel), 0});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_program_value(channel);
            int f = _f.has_value() ? _f.value() : -1;
            int t = pitch.has_value() ? pitch.value() : -1;
            log<log_level_debug_trace>("New pitch wheel diff: tracker {} updated vs. {}, chan {}, wheel {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, f, t);
        }
    } else {
        std::cerr << "[CPP]   -> ERASING pitch wheel diff [0xE0 | " << (int)channel << ", 0]" << std::endl;
        m_diffs.erase({(uint8_t)(0xE0 | channel), 0});
        log<log_level_debug_trace>("Forget pitch wheel diff: tracker {} updated vs. {}, chan {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel);
    }
    std::cerr << "[CPP]   -> diffs now: " << m_diffs.size() << std::endl;
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
        m_diffs.insert({(uint8_t)(0xD0 | channel), 0});
        if (should_log<log_level_debug_trace>()) {
            auto _f = other->maybe_program_value(channel);
            int f = _f.has_value() ? _f.value() : -1;
            int t = pressure.has_value() ? pressure.value() : -1;
            log<log_level_debug_trace>("New channel pressure diff: tracker {} updated vs. {}, chan {}, pressure {} -> {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel, f, t);
        }
    } else {
        m_diffs.erase({(uint8_t)(0xD0 | channel), 0});
        log<log_level_debug_trace>("Forget channel pressure diff: tracker {} updated vs. {}, chan {}", fmt::ptr(tracker), fmt::ptr(other.get()), channel);
    }
}

MidiStateDiffTracker::MidiStateDiffTracker() {
    std::cerr << "[CPP] MidiStateDiffTracker::MidiStateDiffTracker()" << std::endl;
    m_diffs.reserve(default_diffs_size);
}

MidiStateDiffTracker::MidiStateDiffTracker(SharedTracker a, SharedTracker b,
                                           StateDiffTrackerAction action) {
    m_diffs.reserve(default_diffs_size);
    reset(a, b, action);
}

void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b,
                                 StateDiffTrackerAction action) {
    std::cerr << "[CPP] MidiStateDiffTracker::reset() a=" << a.get() << " b=" << b.get() << " action=" << (int)action << std::endl;
    if (m_a) {
        std::cerr << "[CPP]   unsubscribing from old m_a=" << m_a.get() << std::endl;
        m_a->unsubscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    if (m_b) {
        std::cerr << "[CPP]   unsubscribing from old m_b=" << m_b.get() << std::endl;
        m_b->unsubscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    m_a = a;
    m_b = b;
    if (m_a) {
        std::cerr << "[CPP]   subscribing to new m_a=" << m_a.get() << std::endl;
        m_a->subscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    if (m_b) {
        std::cerr << "[CPP]   subscribing to new m_b=" << m_b.get() << std::endl;
        m_b->subscribe(shoop_static_pointer_cast<MidiStateTracker::Subscriber>(shared_from_this()));
    }
    std::cerr << "[CPP]   subscribers now: m_a->m_subscribers.size=?? m_b->m_subscribers.size=?? (pointer-based)" << std::endl;
    std::cerr << "[CPP]   diffs before action: " << m_diffs.size() << std::endl;
    switch (action) {
    case StateDiffTrackerAction::ScanDiff:
        std::cerr << "[CPP]   calling rescan_diff()" << std::endl;
        rescan_diff();
        break;
    case StateDiffTrackerAction::ClearDiff:
        std::cerr << "[CPP]   calling clear_diff()" << std::endl;
        clear_diff();
        break;
    case StateDiffTrackerAction::None:
    default:
        break;
    }
    std::cerr << "[CPP]   diffs after action: " << m_diffs.size() << std::endl;
    for (auto& d : m_diffs) {
        std::cerr << "[CPP]     diff: [0x" << std::hex << (int)d[0] << ", " << std::dec << (int)d[1] << "]" << std::endl;
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
        std::cerr << "[CPP]   check_cc ch=" << (int)channel << " cc=" << (int)controller << ": a=" << (a.has_value() ? std::to_string(a.value()) : "none") << " b=" << (b.has_value() ? std::to_string(b.value()) : "none") << std::endl;
        if (a == b) {
            m_diffs.erase({(uint8_t)(0xB0 | channel), controller});
        } else {
            std::cerr << "[CPP]     -> ADDING CC diff [0x" << std::hex << (int)(0xB0 | channel) << ", " << std::dec << (int)controller << "]" << std::endl;
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
    std::cerr << "[CPP] MidiStateDiffTracker::rescan_diff()" << std::endl;
    clear_diff();
    if (m_a && m_b && m_a->tracking_notes() && m_b->tracking_notes()) {
        std::cerr << "[CPP]   scanning notes..." << std::endl;
        for (uint8_t channel = 0; channel < 16; channel++) {
            for (uint8_t note = 0; note < 128; note++) {
                check_note(channel, note);
            }
        }
    }
    if (m_a && m_b && m_a->tracking_controls() && m_b->tracking_controls()) {
        std::cerr << "[CPP]   scanning CCs and pitch..." << std::endl;
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
    std::cerr << "[CPP]   rescan_diff complete: " << m_diffs.size() << " diffs" << std::endl;
}

void MidiStateDiffTracker::clear_diff() { 
    std::cerr << "[CPP] MidiStateDiffTracker::clear_diff() diffs before: " << m_diffs.size() << std::endl;
    m_diffs.clear(); 
}

void MidiStateDiffTracker::resolve_to(
    MidiStateTracker *to,
    std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes,
    bool controls, bool programs) {
    std::cerr << "[CPP] MidiStateDiffTracker::resolve_to() to=" << to << " notes=" << notes << " controls=" << controls << " programs=" << programs << std::endl;
    std::cerr << "[CPP]   m_a=" << m_a.get() << " m_b=" << m_b.get() << std::endl;
    if (m_a) {
        auto pw_a = m_a->maybe_pitch_wheel_value(0);
        std::cerr << "[CPP]   m_a pitch_wheel[0]=" << (pw_a.has_value() ? pw_a.value() : -1) << std::endl;
    }
    if (m_b) {
        auto pw_b = m_b->maybe_pitch_wheel_value(0);
        std::cerr << "[CPP]   m_b pitch_wheel[0]=" << (pw_b.has_value() ? pw_b.value() : -1) << std::endl;
    }
    if (to != m_a.get() && to != m_b.get()) {
        std::cerr << "[CPP]   -> to is not m_a or m_b, returning" << std::endl;
        return;
    }
    if (!m_a || !m_b) {
        std::cerr << "[CPP]   -> m_a or m_b is null, returning" << std::endl;
        return;
    }
    auto &from = to == m_a.get() ? *m_b : *m_a;
    auto pw_from = from.maybe_pitch_wheel_value(0);
    std::cerr << "[CPP]   -> from=" << &from << " (to is " << (to == m_a.get() ? "m_a" : "m_b") << ")" << std::endl;
    std::cerr << "[CPP]   -> from.maybe_pitch_wheel_value(0)=" << (pw_from.has_value() ? pw_from.value() : -1) << std::endl;
    std::cerr << "[CPP]   -> m_diffs.size() = " << m_diffs.size() << std::endl;
    for (auto &d : m_diffs) {
        std::cerr << "[CPP]     diff: [0x" << std::hex << (int)d[0] << ", " << std::dec << (int)d[1] << "]" << std::endl;
    }
    log<log_level_debug_trace>("Resolve from {} to {}", fmt::ptr(&from), fmt::ptr(to));
    uint8_t data[3];
    for (auto &d : m_diffs) {
        uint8_t kind_part = d[0] & 0xF0;
        uint8_t channel_part = d[0] & 0x0F;
        std::cerr << "[CPP]   processing diff [0x" << std::hex << (int)d[0] << ", " << std::dec << (int)d[1] << "]" << std::endl;
        std::optional<uint8_t> v;
        switch (d[0] & 0xF0) {
        case 0x80:
        case 0x90:
            if (notes) {
                v = from.maybe_current_note_velocity(d[0], d[1]);
                std::cerr << "[CPP]     -> note from=" << (v.has_value() ? std::to_string(v.value()) : "none") << " (" << (v.has_value() ? "active" : "inactive") << ")" << std::endl;
                if (v.has_value() || true) {  // C++ outputs even inactive notes
                    data[0] =
                        v.has_value() ? 0x90 | channel_part : 0x80 | channel_part;
                    data[1] = d[1];
                    data[2] = v.value_or(64);
                    std::cerr << "[CPP]     -> outputting note [0x" << std::hex << (int)data[0] << ", " << (int)data[1] << ", " << (int)data[2] << "]" << std::dec << std::endl;
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xB0:
            if (controls) {
                v = from.maybe_cc_value(channel_part, d[1]);
                std::cerr << "[CPP]     -> CC from=" << (v.has_value() ? std::to_string(v.value()) : "none") << " (" << (v.has_value() ? "known" : "unknown") << ")" << std::endl;
                if (v.has_value()) {
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    std::cerr << "[CPP]     -> outputting CC [0x" << std::hex << (int)data[0] << ", " << (int)data[1] << ", " << (int)data[2] << "]" << std::dec << std::endl;
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xC0:
            if (programs) {
                v = from.maybe_program_value(channel_part);
                std::cerr << "[CPP]     -> program from=" << (v.has_value() ? std::to_string(v.value()) : "none") << " (" << (v.has_value() ? "known" : "unknown") << ")" << std::endl;
                if (v.has_value()) {
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    std::cerr << "[CPP]     -> outputting program [0x" << std::hex << (int)data[0] << ", " << (int)data[1] << ", " << (int)data[2] << "]" << std::dec << std::endl;
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xD0:
            if (controls) {
                v = from.maybe_channel_pressure_value(channel_part);
                std::cerr << "[CPP]     -> channel_pressure from=" << (v.has_value() ? std::to_string(v.value()) : "none") << " (" << (v.has_value() ? "known" : "unknown") << ")" << std::endl;
                if (v.has_value()) {
                    data[0] = d[0];
                    data[1] = d[1];
                    data[2] = v.value();
                    std::cerr << "[CPP]     -> outputting channel_pressure [0x" << std::hex << (int)data[0] << ", " << (int)data[1] << ", " << (int)data[2] << "]" << std::dec << std::endl;
                    put_message_cb(3, data);
                }
            }
            break;
        case 0xE0:
            if (controls) {
                auto v = from.maybe_pitch_wheel_value(channel_part);
                std::cerr << "[CPP]     -> pitch wheel from=" << (v.has_value() ? std::to_string(v.value()) : "none") << std::endl;
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
                    std::cerr << "[CPP]     -> put_message_cb(3, [0x" << std::hex << (int)data[0] << ", " << (int)data[1] << ", " << (int)data[2] << "]" << std::dec << std::endl;
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