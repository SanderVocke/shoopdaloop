#include "TrackedRelativeMidiState.h"
#include <iostream>

TrackedRelativeMidiState::TrackedRelativeMidiState(bool notes,
                                                            bool controls,
                                                            bool programs)
    : m_valid(false),
      state(shoop_make_shared<MidiStateTracker>(notes, controls, programs)),
      diff(shoop_make_shared<MidiStateDiffTracker>()) {
    std::cerr << "[CPP] TrackedRelativeMidiState::TrackedRelativeMidiState()" << std::endl;
}

TrackedRelativeMidiState &
TrackedRelativeMidiState::operator=(
    TrackedRelativeMidiState const &other) {
    std::cerr << "[CPP] TrackedRelativeMidiState::operator=()" << std::endl;
    std::cerr << "[CPP]   other.m_valid=" << other.m_valid << " other.diff=" << other.diff.get() << " other.diff->a()=" << other.diff->a().get() << " other.state=" << other.state.get() << std::endl;
    
    // We want to track the same source state, yet copy
    // the cached state contents from the other tracker
    // so that the diffs may diverge.
    m_valid = other.m_valid;
    std::cerr << "[CPP]   copying state" << std::endl;
    state->copy_relevant_state(*other.state);
    
    std::cerr << "[CPP]   getting diff from other" << std::endl;
    auto other_diff = other.diff->get_diff();
    std::cerr << "[CPP]   other diff size: " << other_diff.size() << std::endl;
    
    std::cerr << "[CPP]   calling diff->set_diff()" << std::endl;
    diff->set_diff(other_diff);
    
    std::cerr << "[CPP]   calling diff->reset() with a=" << other.diff->a().get() << " state=" << state.get() << std::endl;
    diff->reset(other.diff->a(), state, StateDiffTrackerAction::None);
    
    std::cerr << "[CPP]   operator= done, diffs after reset:" << std::endl;
    auto my_diffs = diff->get_diff();
    for (auto const& d : my_diffs) {
        std::cerr << "[CPP]     diff [" << std::hex << (int)d[0] << ", " << (int)d[1] << std::dec << "]" << std::endl;
    }
    
    return *this;
}

void
TrackedRelativeMidiState::start_tracking_from(shoop_shared_ptr<MidiStateTracker> &t) {
    std::cerr << "[CPP] TrackedRelativeMidiState::start_tracking_from(t=" << t.get() << ")" << std::endl;
    state->copy_relevant_state(*t);
    std::cerr << "[CPP]   calling diff->reset(t, state)" << std::endl;
    diff->reset(t, state, StateDiffTrackerAction::ClearDiff);
    m_valid = true;
    std::cerr << "[CPP]   m_valid set to true" << std::endl;
}

void
TrackedRelativeMidiState::start_tracking_from_with_state(shoop_shared_ptr<MidiStateTracker> &to_track,
                                            shoop_shared_ptr<MidiStateTracker> const& starting_state)
{
    std::cerr << "[CPP] TrackedRelativeMidiState::start_tracking_from_with_state(to_track=" << to_track.get() << " starting_state=" << starting_state.get() << ")" << std::endl;
    
    auto tmp_diff = shoop_make_shared<MidiStateDiffTracker>();
    std::cerr << "[CPP]   tmp_diff created" << std::endl;
    tmp_diff->reset(to_track, starting_state, StateDiffTrackerAction::ScanDiff);

    start_tracking_from(to_track);
    std::cerr << "[CPP]   calling diff->reset(to_track, state, ClearDiff)" << std::endl;
    diff->reset(to_track, state, StateDiffTrackerAction::ClearDiff);
    
    std::cerr << "[CPP]   calling diff->set_diff(tmp_diff->get_diff())" << std::endl;
    auto tmp_diffs = tmp_diff->get_diff();
    std::cerr << "[CPP]   tmp_diff has " << tmp_diffs.size() << " diffs" << std::endl;
    diff->set_diff(tmp_diffs);
    
    state->copy_relevant_state(*starting_state);

    m_valid = true;
    std::cerr << "[CPP]   m_valid set to true" << std::endl;
}

void
TrackedRelativeMidiState::reset() {
    std::cerr << "[CPP] TrackedRelativeMidiState::reset()" << std::endl;
    if (state) {
        state->clear();
    }
    if (diff) {
        diff->reset();
    }
    m_valid = false;
}

bool
TrackedRelativeMidiState::valid() const { 
    std::cerr << "[CPP] TrackedRelativeMidiState::valid() returning " << m_valid << std::endl;
    return m_valid; 
}

void
TrackedRelativeMidiState::set_valid(bool v) { 
    std::cerr << "[CPP] TrackedRelativeMidiState::set_valid(" << v << ")" << std::endl;
    m_valid = v; 
}

void
TrackedRelativeMidiState::resolve_to_output(
    std::function<void(uint32_t size, uint8_t *data)> send_cb, bool ccs, bool notes, bool programs) {
    std::cerr << "[CPP] TrackedRelativeMidiState::resolve_to_output(ccs=" << ccs << " notes=" << notes << " programs=" << programs << ") m_valid=" << m_valid << std::endl;
    if (m_valid) {
        std::cerr << "[CPP]   calling diff->resolve_to_a()" << std::endl;
        diff->resolve_to_a(send_cb, notes, ccs, programs);
    }
}