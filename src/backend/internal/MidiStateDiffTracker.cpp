#include "MidiStateDiffTracker.h"
#include "MidiStateTracker.h"
#include <vector>

MidiStateDiffTracker::MidiStateDiffTracker()
    : m_rust(backend_rust::new_midi_state_diff_tracker()) {}

MidiStateDiffTracker::MidiStateDiffTracker(SharedTracker a, SharedTracker b, StateDiffTrackerAction action)
    : MidiStateDiffTracker() {
    reset(a, b, action);
}

void MidiStateDiffTracker::reset(SharedTracker a, SharedTracker b, StateDiffTrackerAction action) {
    if (m_a) m_a->raw_ptr()->unsubscribe(m_rust.operator->());
    if (m_b) m_b->raw_ptr()->unsubscribe(m_rust.operator->());
    m_a = a;
    m_b = b;
    if (m_a) m_a->raw_ptr()->subscribe(m_rust.operator->());
    if (m_b) m_b->raw_ptr()->subscribe(m_rust.operator->());
    m_rust->reset(m_a ? m_a->raw_ptr() : nullptr, m_b ? m_b->raw_ptr() : nullptr);
    switch (action) {
    case StateDiffTrackerAction::ScanDiff:
        m_rust->rescan_diff();
        break;
    case StateDiffTrackerAction::ClearDiff:
        m_rust->clear_diff();
        break;
    default:
        break;
    }
}

void MidiStateDiffTracker::add_diff(DifferingState a) {
    m_rust->add_diff(a[0], a[1]);
}

void MidiStateDiffTracker::delete_diff(DifferingState a) {
    m_rust->delete_diff(a[0], a[1]);
}

void MidiStateDiffTracker::check_note(uint8_t channel, uint8_t note) {
    m_rust->check_note(channel, note);
}

void MidiStateDiffTracker::check_cc(uint8_t channel, uint8_t controller) {
    m_rust->check_cc(channel, controller);
}

void MidiStateDiffTracker::check_program(uint8_t channel) {
    m_rust->check_program(channel);
}

void MidiStateDiffTracker::check_channel_pressure(uint8_t channel) {
    m_rust->check_channel_pressure(channel);
}

void MidiStateDiffTracker::check_pitch_wheel(uint8_t channel) {
    m_rust->check_pitch_wheel(channel);
}

void MidiStateDiffTracker::rescan_diff() {
    m_rust->rescan_diff();
}

void MidiStateDiffTracker::clear_diff() {
    m_rust->clear_diff();
}

void MidiStateDiffTracker::resolve_to(MidiStateTracker *to, std::function<void(uint32_t size, uint8_t *data)> put_message_cb,
    bool notes, bool controls, bool programs) {
    auto data = m_rust->resolve_to(to ? to->raw_ptr() : nullptr, notes, controls, programs);
    for (size_t i = 0; i + 2 < data.size(); i += 3) {
        put_message_cb(3, const_cast<uint8_t*>(&data[i]));
    }
}

void MidiStateDiffTracker::resolve_to_a(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes, bool controls, bool programs) {
    return resolve_to(m_a.get(), put_message_cb, notes, controls, programs);
}

void MidiStateDiffTracker::resolve_to_b(std::function<void(uint32_t size, uint8_t *data)> put_message_cb, bool notes, bool controls, bool programs) {
    return resolve_to(m_b.get(), put_message_cb, notes, controls, programs);
}

void MidiStateDiffTracker::set_diff(DiffSet const &diff) {
    std::vector<uint8_t> data;
    data.reserve(diff.size() * 2);
    for (auto const &d : diff) {
        data.push_back(d[0]);
        data.push_back(d[1]);
    }
    m_rust->set_diff_flat(rust::Slice<const uint8_t>(data.data(), data.size()));
}

MidiStateDiffTracker::DiffSet MidiStateDiffTracker::get_diff() const {
    auto data = m_rust->get_diff_flat();
    DiffSet ds;
    ds.reserve(data.size() / 2);
    for (size_t i = 0; i + 1 < data.size(); i += 2) {
        ds.insert({data[i], data[i + 1]});
    }
    return ds;
}

MidiStateDiffTracker::SharedTracker MidiStateDiffTracker::a() const {
    return m_a;
}

MidiStateDiffTracker::SharedTracker MidiStateDiffTracker::b() const {
    return m_b;
}
