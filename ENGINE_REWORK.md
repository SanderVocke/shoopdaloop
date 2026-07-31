# Engine rework: status

Converging `shoop_engine`'s two control/audio-thread boundaries onto one. Background, the
original plan and the reasoning behind it are in `DEAD_STACK_B.md`; this file records where
the work stands.

Last updated 2026-07-31.

## Where things are

| Step | State |
|---|---|
| 1 — fold `DummyDriver` into the live dummy driver | ✅ pushed |
| 2 — delete `driver.rs`'s dead trait layer | ✅ pushed |
| 3 — move the graph rebuild off both threads | ✅ pushed |
| 4 — grow the published snapshot, add `Engine::pump` | ✅ pushed |
| 5 — swap the boundary | ⚠️ working, 6 test failures left, **not pushed** |

Steps 1–4 are on `rust_backend_nick` as commit `d14d699`, merged and pushed (`bbcc066`).

Step 5 is on branch **`rust_backend_nick_stack_b_swap`**, three commits on top of `bbcc066`:

- `baaa252` — the swap itself, committed as a debugging baseline while it still segfaulted.
- `650f6d2` — four defects (lock convoy, scheduler spin, wait latency, unpublished snapshots).
- `b14dcc1` — the segfault, read-after-write staleness, and `wait_process` not draining.

## Verification

| Gate | Before step 5 | Now |
|---|---|---|
| `cargo test -p shoop_engine --features app_backend` | 675 / 0 | **674 / 0** |
| real-JACK tests, `jackd` running, no skip flag | 4 / 4 | **4 / 4** |
| QML `--self-test` | 186 pass / 1 skip | **180 pass / 6 fail / 1 skip** |
| QML wall time | ~13 min | **~3.5 min** |

The one skip is `CpalPorts` — no audio device in this sandbox. Three `midir_driver` tests
fail locally without `SHOOP_ALLOW_MISSING_BACKENDS=1` because there is no ALSA sequencer;
pre-existing and unrelated.

The test count drops by one because `tests/control.rs` went from 15 tests to 14 when it was
re-pointed at `app_backend`: `asking_for_a_loop_that_is_not_there_fails` tested `control.rs`'s
`loop_at`, which has no counterpart in the live API. The behaviour underneath it is covered by
`session.rs`'s own tests.

## What step 5 actually changed

`SharedSession` no longer holds a `Mutex<Session>`. It holds an `EngineHandle` behind a mutex
that no audio thread ever waits on, plus the `Engine` itself while no driver has claimed it.
There are **zero** session-lock sites left in `app_backend.rs`; the JACK callback, the CPAL
output callback and the dummy driver thread each own the engine outright while running.

The 26 former lock sites became three primitives on `SharedSession`:

- `send` — queue a mutation, arm the scheduler.
- `query` — queue and wait; ordered behind everything already queued, so a read always sees
  the writes before it.
- `poll` — read the newest published snapshot, for the 40 Hz update path.

`control.rs` is deleted. Its method bodies were moved into `app_backend`'s handles, which is
why the plan kept it until last: it was the conversion template, not dead weight to clear
first. `tests/control.rs` now drives `app_backend::BackendSession` through a dummy
`AudioDriver`, so its 14 assertions hold on the path the application takes.

## Seven defects found while getting it working

None of these presented as a failure at the point of the mistake, which is the theme.

1. **Lock convoy.** `send_and_wait` was called while holding the mutex guarding the handle, so
   every control operation queued behind a full round trip to the audio thread. With the
   scheduler describing the topology on its own thread this starved the GUI thread
   continuously. Split into `EngineHandle::send_for_result` (needs the handle) and the free
   function `wait_for_result` (cannot take it), so the mutex is released before waiting.
   `control.rs` had the identical flaw, never exercised under contention.

2. **Scheduler round-trip spin.** Every armed window asked the audio thread whether the graph
   was stale — ~90 blocking round trips a second, almost always answered "nothing to do",
   keeping the command queue permanently busy. The engine now publishes `Stats::graph_stale`.
   A `true` reading is trusted at once; a `false` only when the command queue is also empty,
   which closes the race where a queued-but-unapplied mutation would be missed with nothing
   left to arm another window.

3. **A 1 ms sleep in the result wait.** Harmless while nothing outside the tests used
   `send_and_wait`; a 1 ms floor per read once every control read became a round trip. Now
   yields for a short budget before falling back to sleeping.

4. **Snapshots were never published.** All three drivers called `session_mut().process(n)`
   rather than `Engine::run_cycle(n)`, so the counters were never updated and no state snapshot
   was ever published. Every `poll_state` therefore returned `None` and every reader silently
   fell back to a blocking round trip — step 4's snapshot work was dead on arrival, with no
   symptom other than the application being inexplicably slow. `Engine::run_cycle` now exists
   so a driver can stage buffers between applying control work and running the cycle without
   reaching past the engine, and its doc comment names the trap.

5. **The segfault.** With a driver owning the engine, the engine owns the session — so when the
   dummy driver's thread ended it destroyed the session *on that thread*. A session holding
   Carla LV2 hosts does not survive its plugin instances being torn down off the thread that
   created them, which is why this surfaced as a crash at a test-file boundary rather than
   anywhere near the cause. Ownership was transferable but not returnable; the thread now hands
   the engine back through `SharedSession::return_engine` before exiting. That also stops the
   session becoming unreachable to anything outliving the driver.

6. **Read-after-write against a snapshot.** A queued setter followed by a snapshot read returned
   the value from before the set. `SharedSession` now records the cycle at which the most recent
   mutation was queued, and `poll` only trusts a snapshot whose `cycle` is past it — otherwise
   it reports nothing and the caller asks the engine directly. This one accounted for 84 QML
   failures, including `verify_loop_cleared` seeing a loop it had just cleared still playing at
   its old length.

7. **"Settled" did not include control work.** `AudioDriver::wait_process` flushed the graph but
   not the command queue, so a caller that configured something and then advanced the driver by
   an exact number of frames ran those frames against the old configuration. It now drains the
   queue first.

Also fixed along the way: `FXChain::make_audio_port` / `make_midi_port` used `.unwrap_or(0)` as
the failure fallback for a port index, silently aliasing port 0 and handing back a handle to
someone else's port. They return `Option` now — a missing port is visible, a wrong one is not.

## What is left

Six failures, all in one group: `test_grab_ringbuffer_*` (two in `CompositeLoop_running`, four
in `ThreeLoops`).

These are wrong values, not crashes or hangs. The adoption runs but computes the wrong window:
length 1000 where 200 is expected, position 0 where 50 is expected.

Ruled out: write-visibility on `set_ringbuffer_n_samples` and `dummy_queue_data` — making both
blocking changed nothing.

Current read of it: the fault is in the grab's *inputs* rather than its writes. `on_grab_clicked`
derives its cycle count from the GUI-side sync-loop length and position, and a wrong cycle count
would produce exactly this shape. That is the same read-after-write family as defect 6 but on
the QML side, so tracing it means going into the grab logic in `qobj_loop_gui` and the QML rather
than the engine boundary.

Two caveats worth carrying forward:

- `test_grab_ringbuffer_no_play` passed when run in isolation but failed in the full suite, so
  there is residual order sensitivity in this group beyond the wrong-window problem.
- The real-JACK gate has not been re-run since `650f6d2` and `b14dcc1`. The full Rust suite has,
  and it includes those four tests and passed — but they have not been run on their own against
  a live `jackd` since those commits.

## Notes for picking this up

Local test setup for this sandbox — writable `CARGO_HOME`, the in-repo Qt, and `jackd` — is
recorded in the project memory under "Running tests in the bwrap sandbox". Two things learned
while debugging step 5 that are not there:

- `ptrace` is blocked, so `gdb` cannot attach. Thread states are still readable from
  `/proc/<pid>/task/*/{comm,wchan,stat}`, and the wrapper `shoopdaloop_dev.sh` is a shell script
  — find the real process with `pgrep -x shoopdaloop`, not by matching the script name.
- A single QML test file is selected with `--self-test -f "$PWD/src/qml/test/tst_Foo.qml"`. The
  argument is a full glob path, not a bare filename; a bare name silently matches nothing and
  reports a pass over zero testcases. `--filter` narrows by testcase name and is what makes the
  feedback loop seconds instead of minutes.
