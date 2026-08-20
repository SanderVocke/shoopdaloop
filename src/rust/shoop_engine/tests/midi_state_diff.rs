//! Translation of `unit test test_MidiStateDiffTracker.cpp`.
//!
//! notifies a `MidiStateDiffTracker`, which keeps a set of `(status, data1)` keys
//! where the two disagree. Its three cases inspect that set directly, all of them
//! guarding one bug class -- channel pressure keyed under the pitch wheel's status
//! byte, `0xE0` instead of `0xD0`.
//!
//! There is no diff set here: differences are computed by comparing two trackers
//! when a restore is needed, which removes the subscriber wiring entirely. So these
//! assert the same property through the messages a restore emits, which is what the
//! key was ever used for. The bug class survives the redesign, because channel
//! pressure and pitch wheel are neighbouring fields of one struct.

use assert2::check;
use shoop_engine::midi;
use shoop_engine::midi_state::{MidiStateTracker, TrackWhat, PITCH_WHEEL_CENTRE};

fn tracker() -> MidiStateTracker {
    MidiStateTracker::new(TrackWhat {
        notes: false,
        controls: true,
        programs: false,
    })
}

#[shoop_wasm_test_support::shoop_test]
fn channel_pressure_diff_uses_the_correct_status_byte() {
    let mut a = tracker();
    let mut b = tracker();

    a.process(&midi::channel_pressure(0, 64));
    b.process(&midi::channel_pressure(0, 64));

    // Identical, so nothing to restore in either direction.
    check!(a.diff_to(&b).is_empty());
    check!(b.diff_to(&a).is_empty());

    a.process(&midi::channel_pressure(0, 100));

    let diff = a.diff_to(&b);
    check!(diff.len() == 1);
    // Channel pressure, not pitch wheel.
    check!(diff[0] == midi::channel_pressure(0, 64).to_vec());
    check!(diff[0][0] == 0xD0);
}

#[shoop_wasm_test_support::shoop_test]
fn channel_pressure_is_independent_from_the_pitch_wheel() {
    let mut a = tracker();
    let mut b = tracker();

    for t in [&mut a, &mut b] {
        t.process(&midi::channel_pressure(0, 50));
        t.process(&midi::pitch_wheel(0, PITCH_WHEEL_CENTRE));
    }
    check!(a.diff_to(&b).is_empty());

    // Moving only the channel pressure must not drag the pitch wheel along.
    a.process(&midi::channel_pressure(0, 75));

    let diff = a.diff_to(&b);
    check!(diff.len() == 1);
    check!(diff[0][0] == 0xD0);
    check!(!diff.iter().any(|m| m[0] == 0xE0));
}

#[shoop_wasm_test_support::shoop_test]
fn channel_pressure_carries_its_own_channel() {
    let mut a = tracker();
    let mut b = tracker();

    a.process(&midi::channel_pressure(5, 42));
    b.process(&midi::channel_pressure(5, 0));

    let diff = a.diff_to(&b);
    check!(diff.len() == 1);
    // 0xD0 | 5, not 0xE0 | 5.
    check!(diff[0][0] == 0xD5);
    check!(diff[0] == midi::channel_pressure(5, 0).to_vec());
}

/// The converse of the cases above: moving the pitch wheel must not emit a channel
#[shoop_wasm_test_support::shoop_test]
fn the_pitch_wheel_is_independent_from_channel_pressure() {
    let mut a = tracker();
    let mut b = tracker();

    for t in [&mut a, &mut b] {
        t.process(&midi::channel_pressure(0, 50));
        t.process(&midi::pitch_wheel(0, PITCH_WHEEL_CENTRE));
    }

    a.process(&midi::pitch_wheel(0, 1000));

    let diff = a.diff_to(&b);
    check!(diff.len() == 1);
    check!(diff[0][0] == 0xE0);
    check!(diff[0] == midi::pitch_wheel(0, PITCH_WHEEL_CENTRE).to_vec());
}
#[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);
