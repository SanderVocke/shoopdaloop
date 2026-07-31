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

---

## Follow-up investigation (vs. C++ baseline)

### Old C++ `DummyAudioMidiDriver` (working) vs new Rust dummy driver

Reviewed `../shoopdaloop/src/backend/internal/DummyAudioMidiDriver.cpp` (the
previous fully working implementation).

**Key behavioural differences:**

1. **`controlled_mode_request_samples` queued onto the process thread.**
   The C++ version did not directly increment the requested counter from
   the caller's thread; it used `exec_process_thread_command([=]() {
   m_controlled_mode_samples_to_process += samples; })`. This enqueues
   the increment onto the *process thread's command queue*, which the
   dummy process thread drained on every iteration via
   `PROC_handle_command_queue()` immediately before reading
   `m_controlled_mode_samples_to_process`. The end result: the next
   iteration of the process loop always saw the increment.

   The Rust `dummy_request_controlled_frames` directly does
   `self.inner.lock().unwrap().requested += n;` from the *caller's
   thread* (typically the GUI thread). This is still mutex-serialised
   against the dummy thread's read/decrement, but the *ordering* with
   respect to the dummy thread's iteration is not guaranteed by anything
   other than the mutex. If the GUI thread is currently holding the
   mutex when the dummy thread wakes up, the dummy thread will see the
   previous value of `requested` (zero) and process nothing.

2. **`PROC_process` ran the command queue at the top of every iteration.**
   The C++ process loop called `PROC_handle_command_queue()` on every
   tick before reading state and processing. This meant *every* command
   that touched state (including ones queued by other places in the
   driver) was guaranteed to be visible at the next process call.

   The Rust `process_dummy_driver_iteration` does not have a command
   queue step; it reads `requested` directly under the mutex. Commands
   queued via other paths are processed by `send_and_wait` on the GUI
   thread synchronously, not by the dummy thread.

3. **No `apply_graph_changes` was needed in C++.** The old C++ backend
   had a separate `m_recalculate_graph_thread` that was *notified*
   from the process loop (`graph_id != graph_request_id` → notify recalc
   thread) and the process loop *continued processing with the
   previous schedule* while the recalc was happening. So a stale graph
   did not block progress.

   The Rust `session.process()` is a strict no-op when the graph is out
   of date:

   ```rust
   pub fn process(&mut self, n_frames: usize) -> Result<(), SessionError> {
       if !self.graph_up_to_date() {
           return Err(SessionError::GraphOutOfDate);
       }
       ...
   }
   ```

   The dummy driver iterates `s.process(n)` and *ignores the error*
   (`let _ = s.process(n as usize)`), but the work is silently dropped.

### The most damning single observation

In `process_dummy_driver_iteration` (`app_backend.rs:1091-1132`), the
dummy thread sets `i.last_processed = n` **before** calling
`s.process(n)`:

```rust
let n = if i.controlled {
    i.requested.min(i.settings.buffer_size)
} else {
    i.settings.buffer_size
};
if i.controlled {
    i.requested -= n;
}
i.last_processed = n;          // <-- set unconditionally
i.process_generation = i.process_generation.wrapping_add(1);
...
if n == 0 { return; }
if let Some(shared) = session {
    let mut s = shared.lock();
    ...
    let _ = s.process(n as usize);   // <-- may return Err and do nothing
}
```

So `last_processed` reflects the **requested** amount, not the amount
**actually processed**. `wait_updated` waits on the `updated_on_gui_thread`
signal which fires after the update thread ticks with whatever
`last_processed` was at snapshot time. The QML test sees
`backend.last_processed == 25` and concludes "frames processed", but the
session.process() call may have done no work — and the loop position
stays at 0.

This matches the CI failure exactly:
- sync loop **mode** transitions to Playing (the transition command goes
  through `send_and_wait` and lands in the session queue)
- sync loop **position** stays at 0 (because the dummy thread's
  `process(n)` is a no-op due to either `graph_up_to_date` returning
  false, or some other early-exit path I have not yet pinned down)

### Updated hypothesis ranking

Hypothesis A (dummy thread starved) is still plausible on slow CI, but
the new observation above opens a more likely root cause:

**Hypothesis A' — `last_processed` is a lie.** The dummy thread bumps
`last_processed` to the requested amount **before** invoking
`session.process(n)`. If `process(n)` returns `Err(GraphOutOfDate)` or
otherwise does no work, the GUI sees a non-zero `last_processed` and
considers the cycle done. The QML tests read `s().position` /
`l0().mode` etc. directly from loop state on the GUI thread, and those
properties are only updated via the `LoopBackend.update()` →
`get_state()` round-trip on the *update thread*. If the update thread
takes its snapshot at a moment when `session.process()` did no work
(say, before the graph was applied), the snapshot reflects the
pre-process state and the GUI never sees the new mode/position.

This is consistent with the failure on slow CI and with the same tests
passing locally. Locally, the ordering of events between
`dummy_request_controlled_frames`, `dummy_thread` iteration, and
`update_thread` tick is such that the update thread's snapshot happens
*after* the dummy thread's process call has landed. On slow CI, those
events can race and the snapshot happens first.

**Hypothesis D (clear/init ordering) becomes more plausible too.** If
`clear()` and `testcase_init_fn` race such that `clear()` runs after
`dummy_enter_controlled_mode`, then `clear()`'s loop mutations can
leave the graph out of date *for longer* than expected, and the dummy
thread will silently drop the first several iterations until something
calls `apply_graph_changes()`. The QML tests do `testcase.wait_controlled_mode`
which calls `wait_process` (waits for `process_generation += 2`),
but `wait_process` returns success even when iterations are no-ops.

### Concrete next steps to confirm/deny

1. **Print `last_processed` *and* `session.process()` result from the
   dummy thread.** Add a `if s.process(n as usize).is_err() { ... }`
   branch and bump a counter when process returns Err. If the counter
   is non-zero after a failing test, we know the dummy thread is
   dropping cycles silently.

2. **Move `apply_graph_changes()` into the dummy iteration, ahead of
   `s.process(n)`.** If this fixes the failing tests, the root cause
   is "graph out of date silently aborts processing". The old C++
   backend's recalc thread handled this asynchronously; the new Rust
   session does not.

3. **Print `graph_up_to_date()` from the dummy thread.** Should be
   one debug log line per iteration, sufficient to see whether the
   graph is ever out of date during a failing test run.

4. **Move `last_processed = n` to *after* `s.process(n)` returns Ok.**
   This at least makes `last_processed` honest, so `wait_updated` will
   keep waiting until real work has happened.

---

## Local reproduction (2026-07-29)

Reproduced the failure deterministically on a fast NixOS workstation
(16 cores, 61 GB RAM, no resource limits). This rules out the
"CI-only race condition" angle.

### How

```
QT_QPA_PLATFORM=offscreen \
  target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_TwoLoops.qml" \
  --junit-xml /tmp/qml_test_results/r1.xml
```

(`shoopdaloop_dev.sh` is generated by `shoopdaloop/build.rs` and sets
`SHOOP_CONFIG` so the dev config is loaded; without it the QML engine
fails to load any test file and reports 0 testcases. The dev binary
directly does not work because `SHOOP_CONFIG` env var is unset and the
embedded QML paths collapse to empty.)

### Result: 3/3 runs fail identically

```
Totals:
- Testcases: 6
- Passed: 5
- Failed: 1
- Skipped: 0

Failed cases:
- TwoLoops::test_two_loops_countdown
```

Same for `tst_CompositeLoop_running.qml`: all three known CI failures
reproduce locally:

```
Totals:
- Testcases: 24
- Passed: 21
- Failed: 3

Failed cases:
- CompositeLoop_running::test_script_triggers_composite
- CompositeLoop_running::test_sequential
- CompositeLoop_running::test_transition_with_instant_sync_middle_cycle
```

The failure mode is exactly what CI shows: loops never advance (positions
stay at 0), modes never reach `Playing`. No variation between runs.

### Implications

- **Hypothesis A (CI-only race) is ruled out.** The bug is deterministic.
- **Hypothesis A' (`last_processed` is a lie) is still the strongest
  candidate**, but no longer because of timing — it's because
  `process(n)` *consistently* does no work.
- The bug must be in the code path that decides *whether* `process(n)`
  should run, not in the timing of when it runs.

The next step is to instrument `process_dummy_driver_iteration` to log
whether `s.process(n)` returned Ok, Err, or was called at all, and to
log `graph_up_to_date()` at the point of each call. Then the failure
becomes a hard error to investigate rather than a timing race.