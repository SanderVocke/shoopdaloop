#include "TrackedRelativeMidiState.h"

TrackedRelativeMidiState::TrackedRelativeMidiState(bool notes,
                                                            bool controls,
                                                            bool programs)
    : m_valid(false),
      state(shoop_make_shared<MidiStateTracker>(notes, controls, programs)),
      diff(shoop_make_shared<MidiStateDiffTracker>()) {}

TrackedRelativeMidiState &
TrackedRelativeMidiState::operator=(
    TrackedRelativeMidiState const &other) {
    // We want to track the same source state, yet copy
    // the cached state contents from the other tracker
    // so that the diffs may diverge.
    m_valid = other.m_valid;
    state->copy_relevant_state(*other.state);
    diff->set_diff(other.diff->get_diff());
    diff->reset(other.diff->a(), state, StateDiffTrackerAction::None);
    return *this;
}

void
TrackedRelativeMidiState::start_tracking_from(shoop_shared_ptr<MidiStateTracker> &t) {
    state->copy_relevant_state(*t);
    diff->reset(t, state, StateDiffTrackerAction::ClearDiff);
    m_valid = true;
}

void
TrackedRelativeMidiState::start_tracking_from_with_state(shoop_shared_ptr<MidiStateTracker> &to_track,
                                            shoop_shared_ptr<MidiStateTracker> const& starting_state)
{
    auto tmp_diff = shoop_make_shared<MidiStateDiffTracker>();
    tmp_diff->reset(to_track, starting_state, StateDiffTrackerAction::ScanDiff);

    start_tracking_from(to_track);
    diff->reset(to_track, state, StateDiffTrackerAction::ClearDiff);
    diff->set_diff(tmp_diff->get_diff());
    state->copy_relevant_state(*starting_state);

    m_valid = true;
}

void
TrackedRelativeMidiState::reset() {
    if (state) {
        state->clear();
    }
    if (diff) {
        diff->reset();
    }
    m_valid = false;
}

bool
TrackedRelativeMidiState::valid() const { return m_valid; }

void
TrackedRelativeMidiState::set_valid(bool v) { m_valid = v; }

void
TrackedRelativeMidiState::resolve_to_output(
    std::function<void(uint32_t size, uint8_t *data)> send_cb, bool ccs, bool notes, bool programs) {
    if (m_valid) {
        diff->resolve_to_a(send_cb, notes, ccs, programs);
    }
}