//! One-for-one translation of `src/backend/test/unit/test_BasicLoop.cpp`.
//!
//! The C++ cases keep a real sync source loop alive only so the loop under test
//! does not transition immediately; nothing is ever read from it. Here a default
//! `SyncSourceState` snapshot does the same job, since sync is read from a snapshot
//! rather than by following a pointer.
//!
//! `PROC_update_poi` is `update_poi`, and `PROC_process(n)` is `process(n)`.
//! `PROC_trigger()` defaults `propagate` to true, so the bare C++ call is
//! `trigger(true)`: it is what makes the loop report itself as triggering, which
//! two of these cases assert.
//!
//! The repeated `PROC_process(1)` calls in the C++ are load-bearing rather than
//! incidental: a loop refuses to trigger twice in one cycle, so advancing is what
//! makes the next trigger take effect.

use assert2::check;
use shoop_engine::basic_loop::{BasicLoop, SyncSourceState};
use shoop_engine::loop_mode::LoopMode;

/// A loop that will not transition until told to, as the C++ achieves by attaching
/// an otherwise unused sync source.
fn synced_recording_loop() -> BasicLoop {
    let mut l = BasicLoop::default();
    l.set_sync_source(Some(SyncSourceState::default()));
    l.set_mode(LoopMode::Recording);
    l.set_length(10);
    l.update_poi();
    l
}

#[test]
fn basic_loop_stop() {
    let mut l = BasicLoop::default();

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    l.process(1000);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);
}

#[test]
fn basic_loop_record() {
    let mut l = BasicLoop::default();
    l.set_mode(LoopMode::Recording);
    l.update_poi();

    check!(l.mode() == LoopMode::Recording);
    // Recording has no end to reach, so nothing bounds it.
    check!(l.next_poi() == None);
    check!(l.length() == 0);
    check!(l.position() == 0);

    l.process(20);

    check!(l.mode() == LoopMode::Recording);
    check!(l.next_poi() == None);
    check!(l.length() == 20);
    check!(l.position() == 0);
}

#[test]
fn basic_loop_planned_transition() {
    let mut l = synced_recording_loop();

    l.plan_transition(LoopMode::Playing, Some(0), None);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.trigger(true);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(10)); // end of loop
}

#[test]
fn basic_loop_planned_transition_delayed() {
    let mut l = synced_recording_loop();

    l.plan_transition(LoopMode::Playing, Some(1), None);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.trigger(true);
    l.process(1); // cannot trigger twice in one cycle

    // One cycle of delay, so the first trigger only counted down.
    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.trigger(true);

    check!(l.mode() == LoopMode::Playing);
    // Recording for one frame lengthened the loop, so the end is one later.
    check!(l.next_poi() == Some(11));
}

#[test]
fn basic_loop_planned_transitions_delayed() {
    let mut l = synced_recording_loop();

    l.plan_transition(LoopMode::Playing, Some(1), None);
    l.plan_transition(LoopMode::Recording, Some(3), None);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.trigger(true);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.process(1);
    l.trigger(true);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(11)); // end of loop

    l.process(1);
    l.trigger(true);

    check!(l.mode() == LoopMode::Playing);
    check!(l.next_poi() == Some(10)); // end of loop

    l.process(1);
    l.trigger(true);

    // The second planned transition comes due.
    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);
}

#[test]
fn basic_loop_planned_transitions_cancellation() {
    let mut l = synced_recording_loop();

    // The nearer transition wins and cancels the one behind it.
    l.plan_transition(LoopMode::Playing, Some(3), None);
    l.plan_transition(LoopMode::Stopped, Some(2), None);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.trigger(true);
    l.process(1);
    l.trigger(true);

    check!(l.next_poi() == None);
    check!(l.mode() == LoopMode::Recording);

    l.process(1);
    l.trigger(true);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);

    // The cancelled transition to Playing never arrives.
    l.process(1);
    l.trigger(true);
    l.process(1);
    l.trigger(true);

    check!(l.mode() == LoopMode::Stopped);
    check!(l.next_poi() == None);
}

#[test]
fn basic_loop_generate_trigger() {
    let mut l = BasicLoop::default();
    l.set_mode(LoopMode::Stopped);
    l.set_length(10);
    l.set_position(0);

    check!(!l.is_triggering_now());
    l.trigger(true);
    check!(l.is_triggering_now());
}

#[test]
fn basic_loop_generate_trigger_on_restart() {
    let mut l = BasicLoop::default();
    check!(!l.is_triggering_now());

    l.set_length(10);
    l.set_mode(LoopMode::Playing);
    l.process(1);

    check!(!l.is_triggering_now());

    l.update_poi();
    l.process(8);

    check!(!l.is_triggering_now());

    // Reaching the end of the loop is itself a trigger.
    l.process(1);
    check!(l.is_triggering_now());

    l.handle_poi();

    check!(l.position() == 0);

    l.process(5);

    check!(!l.is_triggering_now());
}

#[test]
fn basic_loop_playback_zero_length() {
    let mut l = BasicLoop::default();
    l.set_mode(LoopMode::Playing);
    l.set_length(0);
    l.set_position(0);

    l.update_poi();
    l.process(10);

    // Nothing to play, so it stops rather than spinning.
    check!(l.mode() == LoopMode::Stopped);
}
