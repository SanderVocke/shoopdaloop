//! CXX bridge for BasicLoopCore to expose to C++.
//!
//! Exposes the BasicLoopCore type and its methods for use from C++.
//! Note: LoopMode is passed as u32 (matching shoop_loop_mode_t enum values).
//! Note: CommandQueue handling is done in C++ directly (thread_safe parameter).

#![allow(dead_code)]

use crate::basic_loop::BasicLoopCore;

#[cxx::bridge(namespace = "backend_rust")]
mod ffi {
    extern "Rust" {
        type BasicLoopCore;

        // Constructor
        fn new_basic_loop_core() -> Box<BasicLoopCore>;

        // Basic getters (thread-safe via atomic load)
        fn basic_loop_get_position(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_length(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_mode(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_sync_source(core: &BasicLoopCore) -> usize;

        // Basic setters (process-thread only - C++ handles thread_safe via CommandQueue)
        fn basic_loop_set_position(core: &mut BasicLoopCore, position: u32);
        fn basic_loop_set_length(core: &mut BasicLoopCore, len: u32);
        fn basic_loop_set_mode(core: &mut BasicLoopCore, mode: u32);
        fn basic_loop_set_sync_source(core: &mut BasicLoopCore, ptr: usize);

        // POI getters
        fn basic_loop_proc_get_next_poi(core: &BasicLoopCore) -> u32; // Returns u32::MAX if None
        fn basic_loop_proc_predicted_next_trigger_eta(core: &BasicLoopCore) -> u32; // Returns u32::MAX if None

        // POI updates
        fn basic_loop_proc_update_poi(core: &mut BasicLoopCore);
        fn basic_loop_proc_update_trigger_eta(core: &mut BasicLoopCore);

        // Trigger handling
        fn basic_loop_proc_trigger(core: &mut BasicLoopCore, propagate: bool);
        fn basic_loop_proc_handle_transition(core: &mut BasicLoopCore, new_mode: u32);
        fn basic_loop_proc_handle_poi(core: &mut BasicLoopCore);
        fn basic_loop_proc_is_triggering_now(core: &mut BasicLoopCore) -> bool;

        // Process method
        fn basic_loop_proc_process(core: &mut BasicLoopCore, n_samples: u32);

        // Planned transitions
        fn basic_loop_plan_transition(
            core: &mut BasicLoopCore,
            mode: u32,
            maybe_n_cycles_delay: u32, // u32::MAX means None
            maybe_to_sync_cycle: u32,  // u32::MAX means None
        );
        fn basic_loop_clear_planned_transitions(core: &mut BasicLoopCore);
        fn basic_loop_get_n_planned_transitions(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_planned_transition_delay(core: &BasicLoopCore, idx: u32) -> u32;
        fn basic_loop_get_planned_transition_state(core: &BasicLoopCore, idx: u32) -> u32;
        fn basic_loop_get_first_planned_transition_mode(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_first_planned_transition_delay(core: &BasicLoopCore) -> u32;

        // Triggering state (atomic access)
        fn basic_loop_is_triggering_now_atomic(core: &BasicLoopCore) -> bool;
        fn basic_loop_set_triggering_now(core: &mut BasicLoopCore, val: bool);

        // State accessors (for AudioMidiLoop to use)
        fn basic_loop_get_next_poi_when(core: &BasicLoopCore) -> u32; // Returns u32::MAX if None
        fn basic_loop_get_next_poi_type_flags(core: &BasicLoopCore) -> u32; // Returns 0 if None
        fn basic_loop_set_next_poi(core: &mut BasicLoopCore, when: u32, type_flags: u32); // when=u32::MAX clears
        fn basic_loop_get_next_trigger(core: &BasicLoopCore) -> u32; // Returns u32::MAX if None
        fn basic_loop_set_next_trigger(core: &mut BasicLoopCore, val: u32); // val=u32::MAX clears
        fn basic_loop_get_maybe_next_planned_mode(core: &BasicLoopCore) -> u32;
        fn basic_loop_get_maybe_next_planned_delay(core: &BasicLoopCore) -> i32;
        fn basic_loop_get_next_trigger_eta(core: &BasicLoopCore) -> u32; // Returns u32::MAX if None
    }
}

use crate::basic_loop::LoopMode;
use std::sync::atomic::Ordering;

const U32_MAX: u32 = u32::MAX;

// Constructor
fn new_basic_loop_core() -> Box<BasicLoopCore> {
    crate::basic_loop::new_basic_loop_core()
}

// Basic getters
fn basic_loop_get_position(core: &BasicLoopCore) -> u32 {
    core.get_position()
}

fn basic_loop_get_length(core: &BasicLoopCore) -> u32 {
    core.get_length()
}

fn basic_loop_get_mode(core: &BasicLoopCore) -> u32 {
    core.get_mode().to_u32()
}

fn basic_loop_get_sync_source(core: &BasicLoopCore) -> usize {
    core.get_sync_source()
}

// Basic setters
fn basic_loop_set_position(core: &mut BasicLoopCore, position: u32) {
    core.set_position(position);
}

fn basic_loop_set_length(core: &mut BasicLoopCore, len: u32) {
    core.set_length(len);
}

fn basic_loop_set_mode(core: &mut BasicLoopCore, mode: u32) {
    core.set_mode(LoopMode::from_u32(mode));
}

fn basic_loop_set_sync_source(core: &mut BasicLoopCore, ptr: usize) {
    core.set_sync_source(ptr);
}

// POI getters - return u32::MAX if None
fn basic_loop_proc_get_next_poi(core: &BasicLoopCore) -> u32 {
    core.proc_get_next_poi().unwrap_or(U32_MAX)
}

fn basic_loop_proc_predicted_next_trigger_eta(core: &BasicLoopCore) -> u32 {
    core.proc_predicted_next_trigger_eta().unwrap_or(U32_MAX)
}

// POI updates
fn basic_loop_proc_update_poi(core: &mut BasicLoopCore) {
    core.proc_update_poi();
}

fn basic_loop_proc_update_trigger_eta(core: &mut BasicLoopCore) {
    core.proc_update_trigger_eta();
}

// Trigger handling
fn basic_loop_proc_trigger(core: &mut BasicLoopCore, propagate: bool) {
    core.proc_trigger(propagate);
}

fn basic_loop_proc_handle_transition(core: &mut BasicLoopCore, new_mode: u32) {
    core.proc_handle_transition(LoopMode::from_u32(new_mode));
}

fn basic_loop_proc_handle_poi(core: &mut BasicLoopCore) {
    core.proc_handle_poi();
}

fn basic_loop_proc_is_triggering_now(core: &mut BasicLoopCore) -> bool {
    core.proc_is_triggering_now()
}

// Process method
fn basic_loop_proc_process(core: &mut BasicLoopCore, n_samples: u32) {
    core.proc_process(n_samples);
}

// Planned transitions
fn basic_loop_plan_transition(
    core: &mut BasicLoopCore,
    mode: u32,
    maybe_n_cycles_delay: u32,
    maybe_to_sync_cycle: u32,
) {
    let delay = if maybe_n_cycles_delay == U32_MAX {
        None
    } else {
        Some(maybe_n_cycles_delay)
    };
    let sync_cycle = if maybe_to_sync_cycle == U32_MAX {
        None
    } else {
        Some(maybe_to_sync_cycle)
    };
    core.plan_transition(LoopMode::from_u32(mode), delay, sync_cycle);
}

fn basic_loop_clear_planned_transitions(core: &mut BasicLoopCore) {
    core.clear_planned_transitions();
}

fn basic_loop_get_n_planned_transitions(core: &BasicLoopCore) -> u32 {
    core.get_n_planned_transitions()
}

fn basic_loop_get_planned_transition_delay(core: &BasicLoopCore, idx: u32) -> u32 {
    core.get_planned_transition_delay(idx)
}

fn basic_loop_get_planned_transition_state(core: &BasicLoopCore, idx: u32) -> u32 {
    core.get_planned_transition_state(idx).to_u32()
}

fn basic_loop_get_first_planned_transition_mode(core: &BasicLoopCore) -> u32 {
    core.get_first_planned_transition().0.to_u32()
}

fn basic_loop_get_first_planned_transition_delay(core: &BasicLoopCore) -> u32 {
    core.get_first_planned_transition().1
}

// Triggering state (atomic access)
fn basic_loop_is_triggering_now_atomic(core: &BasicLoopCore) -> bool {
    core.ma_triggering_now.load(Ordering::Relaxed)
}

fn basic_loop_set_triggering_now(core: &mut BasicLoopCore, val: bool) {
    core.ma_triggering_now.store(val, Ordering::Relaxed);
}

// State accessors (for AudioMidiLoop to use)
fn basic_loop_get_next_poi_when(core: &BasicLoopCore) -> u32 {
    core.get_next_poi()
        .as_ref()
        .map(|poi| poi.when)
        .unwrap_or(U32_MAX)
}

fn basic_loop_get_next_poi_type_flags(core: &BasicLoopCore) -> u32 {
    core.get_next_poi()
        .as_ref()
        .map(|poi| poi.type_flags)
        .unwrap_or(0)
}

fn basic_loop_set_next_poi(core: &mut BasicLoopCore, when: u32, type_flags: u32) {
    if when == U32_MAX {
        core.set_next_poi(None);
    } else {
        core.set_next_poi(Some(crate::basic_loop::PointOfInterest {
            when,
            type_flags,
        }));
    }
}

fn basic_loop_get_next_trigger(core: &BasicLoopCore) -> u32 {
    core.get_next_trigger().unwrap_or(U32_MAX)
}

fn basic_loop_set_next_trigger(core: &mut BasicLoopCore, val: u32) {
    if val == U32_MAX {
        core.set_next_trigger(None);
    } else {
        core.set_next_trigger(Some(val));
    }
}

fn basic_loop_get_maybe_next_planned_mode(core: &BasicLoopCore) -> u32 {
    core.get_maybe_next_planned_mode().to_u32()
}

fn basic_loop_get_maybe_next_planned_delay(core: &BasicLoopCore) -> i32 {
    core.get_maybe_next_planned_delay()
}

fn basic_loop_get_next_trigger_eta(core: &BasicLoopCore) -> u32 {
    core.get_next_trigger_eta().unwrap_or(U32_MAX)
}
