#include <optional>
#include <stdio.h>
#include "types.h"

class LoopInterface {
public:
    // Get the next point of interest in ticks.
    // A point of interest is the first point until when the loop can be processed
    // without updating its state.
    // Returning a nullopt_t indicates that the loop may be processed indefinitely.
    virtual std::optional<size_t> get_next_poi() const = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void handle_poi() = 0;

    // Process the loop for N ticks.
    // A loop may never be processed beyond its next POI. If that is attempted,
    // an exception is thrown.
    virtual void process(size_t n_samples) = 0;

    // Up to two next states may be planned for a loop.
    virtual void set_next_state(loop_state_t state) = 0;
    virtual void set_next_next_state(loop_state_t state) = 0;
    virtual loop_state_t get_state() const = 0;
    virtual loop_state_t get_next_state() const = 0;
    virtual loop_state_t get_next_next_state() const = 0;

    // State transitions of a loop can be planned in samples.
    // Only the most recently set transition is remembered.
    virtual void plan_transition(size_t when=0) = 0;

    // Transition right away.
    virtual void transition_now() = 0;

    // Getters.
    virtual size_t get_position() const = 0;
    virtual size_t get_length() const = 0;

    LoopInterface() = default;
    virtual ~LoopInterface() {}
};