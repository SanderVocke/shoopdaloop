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
    virtual std::optional<uint32_t> PROC_get_next_poi() const = 0;

    // Get the next upcoming trigger ETA prediction in ticks.
    virtual std::optional<uint32_t> PROC_predicted_next_trigger_eta() const = 0;

    // Handle the current point of interest, leading to any internal state change
    // necessary. If the loop is not currently exactly at a point of interest,
    // nothing happens.
    virtual void PROC_handle_poi() = 0;

    // Triggers are used for triggering other loops to transition their state.
    // This function returns whether the the loop is triggering exactly on the
    // current cycle.
    virtual bool PROC_is_triggering_now() = 0;

    // The sync source determines from which other loop this loop receives triggers.
    virtual std::shared_ptr<LoopInterface> get_sync_source(bool thread_safe = true) = 0;
    virtual void set_sync_source(std::shared_ptr<LoopInterface> const& src, bool thread_safe = true) = 0;

    // Trigger from outside. Handled immediately. If propagate is set to true,
    // other loops synced to this one will also receive the trigger.
    virtual void PROC_trigger(bool propagate=true) = 0;

    // Handle sync
    virtual void PROC_handle_sync() = 0;

    // Process the loop for N ticks.
    // A loop may never be processed beyond its next POI. If that is attempted,
    // an exception is thrown.
    virtual void PROC_process(uint32_t n_samples) = 0;

    // mode transitions may be planned for a loop.
    // The delay is measured in amount of triggers received before transitioning.
    // The delays are absolute, as opposed to relative to the transition before it.
    virtual uint32_t          get_n_planned_transitions(bool thread_safe = true) = 0;
    virtual uint32_t          get_planned_transition_delay(uint32_t idx, bool thread_safe = true) = 0;
    virtual shoop_loop_mode_t     get_planned_transition_state(uint32_t idx, bool thread_safe = true) = 0;
    virtual void            clear_planned_transitions(bool thread_safe = true) = 0;
    virtual void            plan_transition(
        shoop_loop_mode_t mode,
        std::optional<uint32_t> maybe_n_cycles_delay = 0,
        std::optional<uint32_t> maybe_to_sync_cycle = std::nullopt,
        bool thread_safe=true) = 0;

    // Getters and setters.
    virtual uint32_t       get_position() const = 0;
    virtual uint32_t       get_length() const = 0;
    virtual void         set_position(uint32_t position, bool thread_safe=true) = 0;
    virtual void         set_length(uint32_t length, bool thread_safe=true) = 0;
    virtual shoop_loop_mode_t  get_mode() const = 0;
    virtual void         set_mode(shoop_loop_mode_t mode, bool thread_safe=true) = 0;
    virtual void get_first_planned_transition(shoop_loop_mode_t &maybe_mode_out, uint32_t &delay_out) = 0;

    LoopInterface() = default;
    virtual ~LoopInterface() {}
};