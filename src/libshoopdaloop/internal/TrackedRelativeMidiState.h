#pragma once
#include "shoop_shared_ptr.h"
#include "MidiStateTracker.h"
#include "MidiStateDiffTracker.h"
#include <functional>

// Tracks or remembers a MIDI state (controls, pedals, active notes, etc).
// Also keeps track of the diff between that state and another tracked state
// so that the difference can be resolved by sending the corresponding MIDI
// messages.
struct TrackedRelativeMidiState {
    bool m_valid = false;
    shoop_shared_ptr<MidiStateTracker> state = nullptr;
    shoop_shared_ptr<MidiStateDiffTracker> diff = nullptr;

    TrackedRelativeMidiState(bool notes=false, bool controls=false, bool programs=false);
    TrackedRelativeMidiState& operator= (TrackedRelativeMidiState const& other);

    // Take the given state tracker's current state as a starting point.
    // Store a reference to it in "state" and start tracking differences to
    // the current state.
    void start_tracking_from(shoop_shared_ptr<MidiStateTracker> &t);

    // Start tracking from the given state, but don't take the current state
    // as a starting point. Rather use another state as the point to later
    // resolve back to.
    // Note that this requires scanning the states for differences.
    void start_tracking_from_with_state(shoop_shared_ptr<MidiStateTracker> &to_track,
                                        shoop_shared_ptr<MidiStateTracker> const&starting_state);

    void reset();
    bool valid() const;
    void set_valid(bool v);

    // "Resolve" the diff. That means that for any difference that exists between
    // the saved state and the current state, message(s) will be sent to get back to the saved
    // state (e.g. CC value messages, note on, note off etc.)
    void resolve_to_output(std::function<void(uint32_t size, uint8_t *data)> send_cb, bool ccs=true, bool notes=true, bool programs=true);
};