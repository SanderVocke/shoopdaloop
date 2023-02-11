#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "types.h"

// A sub loop is a class which can run as a dependent on an actual loop. It does
// not manage its own state transitions, position or length, but instead is tightly
// controlled by its parent loop.
class SubloopInterface {
public:
    // Get the next point of interest in ticks.
    // A point of interest is the first point until when the loop can be processed
    // without updating its state.
    // Returning a nullopt_t indicates that the loop may be processed indefinitely.
    virtual std::optional<size_t> get_next_poi(loop_state_t state,
                                               size_t length,
                                               size_t position) const = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void handle_poi(loop_state_t state,
                            size_t length,
                            size_t position) = 0;

    // Process. The subloop may alter the after state.
    virtual void process(
        loop_state_t state_before,
        loop_state_t state_after,
        size_t n_samples,
        size_t pos_before,
        size_t pos_after,
        size_t length_before,
        size_t ength_after
    ) = 0;

    SubloopInterface() = default;
    virtual ~SubloopInterface() {}
};