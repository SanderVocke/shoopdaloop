#pragma once
#include <optional>
#include <memory>
#include <stdio.h>
#include "types.h"

class LoopInterface {
public:
    // Get the next point of interest in ticks.
    // A point of interest is the first point until when the loop can be processed
    // without updating its state.
    // Returning a nullopt_t indicates that the loop may be processed indefinitely.
    virtual std::optional<size_t> get_next_poi() const = 0;

    // Triggers are used for triggering other loops to transition their state.
    // This function returns whether the the loop is triggering exactly on the
    // current cycle.
    virtual bool is_triggering_now() = 0;
    // The sync source determines from which other loop this loop receives
    virtual std::shared_ptr<LoopInterface> const& get_sync_source() const = 0;
    virtual void set_sync_source(std::shared_ptr<LoopInterface> const& src) = 0;

    // Trigger from outside. Handled immediately.
    virtual void trigger() = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void handle_poi() = 0;

    // Handle any pending triggers from sync source.
    virtual void handle_sync() = 0;

    // Process the loop for N ticks.
    // A loop may never be processed beyond its next POI. If that is attempted,
    // an exception is thrown.
    virtual void process(size_t n_samples) = 0;

    // State transitions may be planned for a loop.
    // The delay is measured in amount of triggers received before transitioning.
    // Note that the delays are stacked (each delay starts counting)
    // down from the transition before it).
    virtual size_t          get_n_planned_transitions() const = 0;
    virtual size_t          get_planned_transition_delay(size_t idx) const = 0;
    virtual loop_state_t    get_planned_transition_state(size_t idx) const = 0;
    virtual void            clear_planned_transitions() = 0;
    virtual void            plan_transition(loop_state_t state, size_t n_cycles_delay = 0) = 0;

    // Getters and setters.
    virtual size_t get_position() const = 0;
    virtual size_t get_length() const = 0;
    virtual void set_position(size_t pos) = 0;
    virtual loop_state_t get_state() const = 0;

    LoopInterface() = default;
    virtual ~LoopInterface() {}
};