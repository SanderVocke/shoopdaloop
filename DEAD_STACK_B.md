# Dead code: the second engine stack

`shoop_engine` contains two parallel implementations of roughly the same thing. Only one of
them runs. This file records what is left of the other after the JACK-silence work, why it
matters, and the two ways out.

Written 2026-07-30, on branch `rust_backend_nick`.

## The two stacks

**Stack A — live.** `app_backend.rs` (~3000 LOC), reached from
`src/rust/frontend/src/cxx_qt_shoop/rust/qobj_backend_wrapper.rs`. It owns a
`Mutex<engine::Session>` directly, plus its own driver threads (`JackBackend`,
`CpalBackend`, `process_dummy_driver_iteration`) and its own handle types (`BackendSession`,
`Loop`, `AudioChannel`, `AudioPort`, `MidiPort`, `FXChain`). It contains no reference to
`engine::split`, `EngineHandle`, `control::`, `driver::Driver` or `DummyDriver`.

**Stack B — dead.** `engine.rs` → `control.rs` → `driver.rs`, reachable only from
`shoop_engine/tests/*.rs`. It is the better-factored design: mutations go through a command
queue applied at cycle boundaries, and its control layer reschedules the graph inside the
same locked mutation that dirtied it.

## Why this is not just tidiness

The two stacks drift, and the tests follow the dead one. That is exactly how the JACK
silence bug survived:

- `jack_driver.rs` had four real-JACK integration tests, including
  `process_callback_writes_session_output_to_jack_audio_port` — a direct test of the output
  path that was broken in production. It tested a callback the app never invoked.
- `app_backend.rs`'s `JackProcess::process`, the callback the app *does* invoke, had no
  tests at all.
- CI compounded it by never starting a `jackd`, so those four tests bailed out early and
  reported passes.

Same shape for cpal: `cpal_driver.rs` was dead while `CpalBackend` in `app_backend.rs` was
live.

## Already resolved

Deleted, after porting their coverage onto the live path
(`shoop_engine/tests/jack_app_backend.rs`):

| file | LOC | note |
|---|---|---|
| `src/rust/shoop_engine/src/jack_driver.rs` | 727 | 4 real-JACK tests, ported |
| `src/rust/shoop_engine/src/cpal_driver.rs` | 636 | dead duplicate of `CpalBackend` |
| `src/rust/shoop_engine/tests/cpal_driver.rs` | 166 | tested the above |
| `src/rust/shoop_engine/tests/mock_host/mod.rs` | 472 | orphan; never compiled, since `tests/cpal_driver.rs` shadowed it with an inline `mod`. A stale near-copy of `cpal_mock.rs` |

## What remains

| module | dead part | live part | tests at risk |
|---|---|---|---|
| `engine.rs` (702 LOC) | all of it: `split`, `Engine`, `EngineHandle`, `Stats` | `LoopState` (re-exported at `lib.rs`) | 13 inline; 5 in `tests/no_alloc.rs`; 4 in `tests/external_ports.rs` |
| `control.rs` (833 LOC) | the handle API: `Backend`, `Loop`, `AudioChannel`, `MidiChannel`, `Port`, `ControlError` (~700 LOC) | 4 state structs — `AudioChannelState`, `MidiChannelState`, `AudioPortState`, `MidiPortState` — re-exported at `lib.rs` and consumed by `frontend/src/any_backend_{channel,port}.rs` | 15 in `tests/control.rs` |
| `driver.rs` (404 LOC) | `Driver` trait, `DummyEngineDriver`, `DriverState`, `driver_state()` (~220 LOC) | `AudioDriverType`, `AudioDriverState`, `BackendSessionState`, `driver_type_supported`, the `cpal_*`/`midir_*` enumeration helpers | 5 inline |
| `dummy_driver.rs` (402 LOC) | `DummyDriver`, `DriverMode` | `DriverSettings` | 14 inline; 6 in `tests/dummy_driver.rs` |

None of these can be deleted wholesale — each has a live subset that `app_backend.rs` or the
frontend imports.

The tests are the real cost. They are worth something despite testing a dead façade, because
much of what they exercise is `session.rs` and the loop/channel core underneath, which *is*
shared. `tests/no_alloc.rs` in particular uses `engine::split` to assert the realtime
allocation guard — coverage with no equivalent on the live path.

## Two ways out

**Delete Stack B.** Removes ~1600 more LOC of dead code and the ambiguity about which
implementation is real. Costs ~37 tests; the ones covering shared core behaviour would need
re-pointing at `app_backend`, and `no_alloc.rs` would need a live-path equivalent.

**Migrate `app_backend.rs` onto Stack B.** Keeps the better design and its tests. `engine.rs`'s
command queue already solves, properly, the problem `graph_scheduler.rs` now solves for the
shim — mutations and their reschedule land together at a cycle boundary, so the audio thread
never sees a half-applied change and never needs the session mutex the way it does today.
That would also address the remaining known issue below. But `app_backend.rs` is ~3000 LOC
and Stack B has no equivalent for Carla FX chains, decoupled MIDI ports, or the dummy-driver
test API the QML suite depends on.

## Related known issue, deliberately not fixed

Both live callbacks (`app_backend.rs`, `JackProcess::process` and the cpal output callback)
take a plain `std::sync::Mutex` on the audio thread, contending with a GUI thread that holds
the same lock for every state poll and every control operation — some of which allocate. On
JACK this risks the watchdog zombifying the client; on cpal it degrades to glitching. The
`lock()` / `lock_rt()` split in `app_backend.rs` marks exactly which callsites are on the
audio thread, so the scope of a fix is visible. Migrating to Stack B's command queue would
remove the contention outright.
