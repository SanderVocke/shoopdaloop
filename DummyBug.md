# CompositeLoop / TwoLoops QML Test Failures in CI — Assessment

## Context

Inspection of CI run **30406371413** (run from PR #647 on branch `rust_backend_nick`,
`Build and test` workflow). The run completed but its `Test` composite step failed
across all four Linux test jobs:

- `test_linux (release_debian_appimage)`
- `test_linux (release_debian_stable)`
- `test_linux (debug_debian_stable)`
- `test_linux (coverage_debian_bookworm)`

The run summary from the QML runner's stdout (release_debian_appimage):

```
Totals:
- Testcases: 187
- Passed: 182
- Failed: 5
- Skipped: 0
Failed cases:
- CompositeLoop_running::test_script_triggers_composite
- CompositeLoop_running::test_sequential
- CompositeLoop_running::test_transition_with_instant_sync_middle_cycle
- CpalPorts::test_virtual_playback_ports_are_app_connectable
- TwoLoops::test_two_loops_countdown
```

Five QML testcases failed. Separately, three Rust integration tests in
`shoop_engine::cpal_driver` also failed (`a_playing_loop_reaches_the_device_ports`,
`the_device_callback_drives_the_engine`, `duplex_bridges_the_two_streams`) — that
is the cpal-OS-audio issue that the previous commit fixed by adding a
software mock host.

This document is about the four QML failures that are *not* the cpal one
(`CompositeLoop_running::test_*` × 3 and `TwoLoops::test_two_loops_countdown`).

---

## What the failures look like

All four failures follow the same shape. From `CompositeLoop_running::test_sequential`,
the first `verify_states(...)` after `process(50)` and `c().on_play_clicked()` reports:

```
test_sequential: verify_eq failed (a = 1, b = 2) - loop 0 mode         @ tst_CompositeLoop_running.qml:259
test_sequential: verify_eq failed (a = 1, b = 2) - composite loop mode @ tst_CompositeLoop_running.qml:259
test_sequential: verify_eq failed (a = 0, b = 50) - sync loop pos      @ tst_CompositeLoop_running.qml:259
test_sequential: verify_eq failed (a = 0, b = 50) - loop 0 pos         @ tst_CompositeLoop_running.qml:259
test_sequential: verify_eq failed (a = 0, b = 50) - composite loop pos @ tst_CompositeLoop_running.qml:259
```

The same five failures appear again a few lines later at `:268` after
`process(100)`, but the numbers shift (expected `150`, got `0`). A third wave
appears at `:280`-ish with `expected 250, got 0`, etc. — every position stays
at `0` even though `process(N)` was called.

The failures can be decoded against `LoopMode` discriminants:

```
LoopMode::Unknown          = 0
LoopMode::Stopped          = 1   ← actual mode for failed checks
LoopMode::Playing          = 2   ← expected mode
LoopMode::Recording        = 3
LoopMode::Replacing        = 4
LoopMode::PlayingDryThroughWet    = 5
LoopMode::RecordingDryIntoWet     = 6
```

So the loops the test expects to be `Playing` are still `Stopped`. The composite
loop is `Stopped` even after `c().on_play_clicked()`. All positions are `0`.

`TwoLoops::test_two_loops_countdown` shows the same pattern in miniature:

```
test_two_loops_countdown: verify_eq failed (a = 1, b = 2) @ tst_TwoLoops.qml:112
```

That is `verify_eq(other_loop().mode, ShoopRustConstants.LoopMode.Playing)` —
`other_loop()` never transitioned out of `Stopped`.

---

## What the test is actually doing

All four failing tests use the **Dummy** backend — `lib_impl.rs:81-89` selects
`AudioDriverType::Dummy` when `--self-test` is passed:

```rust
let backend_type = match &cli_args.backend {
    Some(backend) => get_audio_driver_from_name(backend.as_str()),
    None => match cli_args.self_test_options.self_test {
        true => AudioDriverType::Dummy,
        false => AudioDriverType::Jack,
    },
};
```

The Dummy backend spins up a dedicated thread that calls
`process_dummy_driver_iteration` on a fixed cadence
(`app_backend.rs:1299-1319`). That iteration is the only thing that drives
the engine: it locks the session, calls `s.process(n)`, and that `process`
is what advances loop positions and applies pending mode transitions.

In controlled mode (`dummy_enter_controlled_mode`), each iteration processes
`min(requested, buffer_size)` frames; otherwise it processes one buffer's
worth automatically. `dummy_request_controlled_frames(n)` just increments
`requested`; it does **not** actively process.

The tests call `dummy_request_controlled_frames(n)` from a QML helper, then
`testcase.wait_updated(backend)` — which is implemented as three calls of
`wait_condition(..., timeout=500ms)` each polling on the
`updated_on_gui_thread` signal from the BackendWrapper
(`ShoopTestCase.qml:259-271`).

---

## Hypotheses

The fact that the sync loop's **mode** correctly transitions to `Playing` but
its **position** stays at `0` is the central clue. Mode is set by
`s().transition(Playing)` going through the command queue; position only
advances when `Session::process()` actually runs against the queue.

That means:

1. The command queue is being drained (the transition landed — `mode == Playing`).
2. The audio thread is *not* running cycles — `process(n)` is either not being
   called, or being called with `n == 0`.

Two candidates, in order of likelihood:

### Hypothesis A — the dummy driver thread is starved under offscreen Qt + JIT AppImage

`process_dummy_driver_iteration` increments `process_generation` per
iteration. The QML side waits for `process_generation += 2` via
`wait_for_dummy_generation`, which polls every 1 ms for up to **100 ms**
(`app_backend.rs:1134-1146`). If the dummy thread does not get two iterations
inside 100 ms, `wait_process` returns silently and `wait_updated` returns
without having observed any actual frame processing.

The thread's own tick budget is `buffer_size / sample_rate` microseconds
between iterations. With the Dummy defaults (buffer_size=64, sample_rate=48000
≈ 1.33 ms/iter), two iterations should fit in 100 ms with massive slack — *if*
the thread is actually getting CPU. But under CI load (AppImage extracted,
`QT_QPA_PLATFORM=offscreen` causing the Qt render thread to spin, the entire
self-test binary running alongside `cargo nextest`'s Rust test binaries on the
same host, and the QML test runner's `wait(5)` busy-loops), the dummy thread
can easily be descheduled past its budget.

The simple repro would be: take a sync loop to Playing, then `process(50)`,
then read `last_processed`. In CI it should still be `0`. Locally (where the
test passes for the developer) the same flow returns `25`.

This is also consistent with all four failing tests passing on developer
machines and only failing in CI — the developer machine has fewer things
competing for the same OS thread.

### Hypothesis B — a value-of-zero makes `wait_for_dummy_generation` exit before any frame is processed

The QML test helper that increments requested is:

```qml
function process(amount, steps=2) {
    for (var i = 0; i < steps; i++) {
        session.backend.dummy_request_controlled_frames(Math.round(amount / steps))
        testcase.wait_updated(session.backend)
    }
}
```

If a caller ever invokes this with `amount=0` (directly or via `Math.round`
of a sub-unit value), the loop runs, but `requested += 0` is a no-op. Combined
with `wait_updated`'s 500 ms timeout per attempt, the test would silently
spend its budget waiting for an update that will never come (no requested
frames, no iteration that bumps `process_generation`), then continue.

Whether any of the failing tests hit this path depends on whether any
`Math.round` ever resolves to `0`. For the four known failures the calls
are `process(50)`, `process(100)`, `process(200, 4)`, `process(42000)`, all
of which produce non-zero `amount/steps`, so hypothesis B is unlikely for the
*observed* failures but is worth ruling out for future ones.

### Hypothesis C — `wait_process` returning early because `process_generation` did not advance

`wait_process` (`app_backend.rs:1452-1463`) computes
`target = i.process_generation.saturating_add(2)` *under the lock* and then
polls up to 100 ms. If between releasing the lock and the first poll the
dummy thread runs two iterations, the wait returns immediately. Conversely,
if the dummy thread does not run two iterations in 100 ms, the wait returns
*also* immediately — silently, with no error.

This is the same observation as Hypothesis A from a different angle: the
wait timeout is silent, so a starved dummy thread produces the same
observable behaviour as "no dummy thread at all". The fix in either case
would be either (a) make the wait timeout reported, so the QML side can
fail loudly; or (b) make the dummy driver more robust under contention
(e.g. signal a condvar when `requested > 0` instead of polling at fixed
intervals).

### Hypothesis D — a clear()/init interaction that resets state mid-test

The test's `clear()` does:

```qml
function clear() {
    s().clear(); l0().clear(); l1().clear(); l2().clear(); c().clear();
    AppRegistries.state_registry.set_sync_active(true);
    AppRegistries.state_registry.set_apply_n_cycles(0);
    testcase.wait_updated(session.backend);
    ...
}
```

followed by `testcase_init_fn`:

```qml
testcase_init_fn: () =>  {
    session.backend.dummy_enter_controlled_mode()
    testcase.wait_controlled_mode(session.backend)
},
```

`dummy_enter_controlled_mode` zeroes `requested` and `last_processed`
(`app_backend.rs:1491-1495`). If for any reason `clear()` runs after
`dummy_enter_controlled_mode` has set up some pending frame requests —
e.g. the QML test runner triggers `clear()` from a deferred action that
fires after `init_fn` — those queued frames get wiped. With requested=0,
no iteration will process anything, and the loop will appear frozen.

This is less likely than A, but the order of "init_fn vs clear()" is
implicit and worth confirming.

---

## What I did not yet verify

These would tighten or rule out each hypothesis:

1. Read `last_processed` from `BackendWrapper` immediately after each
   `process(N)` call in CI. If it is `0`, the dummy thread did not run;
   Hypothesis A is confirmed and B is ruled out. If it is `> 0`, the
   thread *did* run but the position update somehow did not propagate to
   the QML side, which would point to the GUI/backend thread update path
   (`update_on_gui_thread`) being broken.
2. Add a `wait_process` timeout counter that increments when the 100 ms
   wait elapses without progress. If the counter is non-zero after a
   failed test, the dummy thread is starved; if zero, something else is
   wrong.
3. Trace whether the order of `clear()` and `init_fn` is deterministic.
   If a `clear()` can fire after `dummy_enter_controlled_mode` has
   already set `controlled=true` and accumulated `requested`, the state
   would be reset mid-test (Hypothesis D).
4. Run the failing tests under `strace -f -e trace=sched` on CI to see
   whether the dummy driver thread is being preempted for tens of
   milliseconds at a time — that would distinguish starvation (A) from
   an ordering bug (D) cleanly.

The simplest test of (1) is to add `eprintln!("last_processed = {}",
backend.last_processed)` in the QML test helper and re-run on CI. If
`last_processed` is non-zero, the dummy thread is running and the
problem is in the update-publish path. If it is zero, the thread is
either not running or being preempted.