# Dead code: the second engine stack

`shoop_engine` contained two parallel implementations of roughly the same thing. Only one of
them ran. This file records what is left, why it matters, and the plan for converging them.

Written 2026-07-30, on branch `rust_backend_nick`. Updated as each step lands.

## The two stacks

**Stack A — live.** `app_backend.rs` (~3200 LOC), reached from
`src/rust/frontend/src/cxx_qt_shoop/rust/qobj_backend_wrapper.rs`. It owns a
`Mutex<engine::Session>` directly, plus its own driver threads (`JackBackend`,
`CpalBackend`, `process_dummy_driver_iteration`) and its own handle types (`BackendSession`,
`Loop`, `AudioChannel`, `AudioPort`, `MidiPort`, `FXChain`).

**Stack B — dead.** `engine.rs` → `control.rs`, reachable only from
`shoop_engine/tests/*.rs`. Mutations go through a command queue applied at cycle
boundaries; reads come from a snapshot the audio thread publishes.

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

## Decision

**Migrate `app_backend.rs` onto Stack B's boundary, in five shippable steps, deleting Stack
B's façade as it is absorbed.** The audio-thread mutex (see "Related known issue") is
tolerated for now, so each step has to earn its place on convergence and test coverage
alone, not on urgency.

Three findings corrected the original "two ways out" framing, which had presented deletion
and migration as comparable-cost alternatives:

**1. The migration surface is small, and `control.rs` is the template, not the liability.**
`app_backend.rs` has only ~30 `shared.lock()` sites in production code, all of two shapes:
a `with_mut`-style mutation and a `get_state`-style read. `control.rs`'s `mutate` / `query`
helpers are a method-for-method reference implementation of that conversion. So `control.rs`
is deleted **last**, by moving its bodies into `app_backend`'s handles until it is empty —
not first, as the original framing implied.

**2. `engine.rs` as written would move the graph rebuild onto the audio thread.**
`control.rs` calls `apply_graph_changes()` inside ~10 commands, and `Engine::apply_commands`
wraps execution in `realtime_allow_alloc_once!`. That rebuild is exactly what
`graph_scheduler.rs` exists to get *off* both threads. So Stack B does **not** dominate the
shim: it wins atomicity (mutation and reschedule land together at a cycle boundary), the
shim wins cost (off-thread, coalesced, bounded staleness). The synthesis, and the real "best
of both", is to build the schedule off-thread and install it through the command queue.
`graph_scheduler.rs` is kept, not superseded.

**3. The read path is the cost, not the LOC.** `UpdateThread` fans out one `update()` at
~40 Hz to every `LoopBackend`, `PortBackend`, `LoopChannelBackend` and `FXChainBackend`,
each calling `get_state()` synchronously in-line. Under the command queue each would become
a `send_and_wait` — one audio cycle round trip, serialised: N × 5.3 ms at 48 kHz/256 frames.
Non-viable. `StateSnapshot` therefore has to grow from loops-only to the whole polled set
*before* the boundary is swapped.

### Two traps to design around before the swap

- **`send_and_wait` assumes the engine is being driven.** The QML suite runs the dummy
  driver in *controlled* mode, where no cycles run unless frames are requested, so every
  blocking query would hit the 1000 ms `DEFAULT_WAIT_TIMEOUT`. The dummy thread does spin
  continuously, so the fix is to apply commands every iteration and only `process()` when
  `n > 0`. Separately, during session construction no driver is attached at all — today
  handled by `ControlGuard`'s `None`-scheduler inline branch — so a "pump the parked engine
  on the calling thread" primitive is needed too.
- **Read-after-write ordering.** Today `lock()` means a setter followed by a read returns
  the new value. With a queue, a fire-and-forget setter followed by a snapshot read returns
  the old value for up to a cycle. `LoopBackend::update` diffs against `prev_state` to
  synthesise cycle numbers and emit signals, so this can produce spurious signals or GUI
  flicker across the QML suite.

## Already resolved

Deleted, after porting their coverage onto the live path
(`shoop_engine/tests/jack_app_backend.rs`):

| file | LOC | note |
|---|---|---|
| `src/rust/shoop_engine/src/jack_driver.rs` | 727 | 4 real-JACK tests, ported |
| `src/rust/shoop_engine/src/cpal_driver.rs` | 636 | dead duplicate of `CpalBackend` |
| `src/rust/shoop_engine/tests/cpal_driver.rs` | 166 | tested the above |
| `src/rust/shoop_engine/tests/mock_host/mod.rs` | 472 | orphan; never compiled, since `tests/cpal_driver.rs` shadowed it with an inline `mod`. A stale near-copy of `cpal_mock.rs` |

The CI hole is closed too: `.github/actions/start_jack` starts a dummy-backend `jackd`
before the Rust tests, and `jack_app_backend.rs` fails rather than skipping when no server
is present (`SHOOP_ALLOW_MISSING_BACKENDS=1` downgrades that to a skip on a machine that
genuinely has no JACK).

### Step 1 — `DummyDriver` folded into the live dummy driver ✅

`DriverInner` duplicated `DummyDriver`'s chunk arithmetic inline and duplicated its mock
external connections through a parallel `Arc<Mutex<DummyExternalConnections>>`. It now holds
a `DummyDriver` and takes its settings, active flag, chunking and mock ports from it.
`DummyDriver::external` became an `Arc<Mutex<..>>` so the one map is shared with the port
handles and the CPAL backend rather than existing twice.

Effect: `dummy_driver.rs`'s 14 inline + 6 integration tests went from testing a dead
duplicate to covering the live path, without rewriting one of them.

### Step 2 — `driver.rs`'s dead trait layer deleted ✅

`Driver`, `DummyEngineDriver`, `DriverState` and `driver_state()` — 231 lines. The trait was
extracted once three drivers existed; two of those three were the duplicates deleted above,
and a trait with one implementation was not worth keeping. `driver.rs` is now types and
host enumeration only (`AudioDriverType`, `AudioDriverState`, `BackendSessionState`,
`driver_type_supported`, the `cpal_*` / `midir_*` helpers), all of which are live.

Its 5 inline tests were replaced by `a_controlled_request_advances_the_session_by_exactly_
that_many_frames` in `app_backend.rs`, which asserts the same chunking against the driver
the application actually runs — 160 frames at a buffer of 64, so the final short cycle is
exercised rather than assumed.

### Step 3 — the graph rebuild moved off both threads ✅

`Session::apply_graph_changes` was one locked operation covering describe, build and
install. It is now three:

- `Session::describe_topology() -> Topology` — needs `&self`, cheap.
- `build_schedule(Topology) -> Result<PreparedSchedule, _>` — free function, touches no
  `Session`, does the lowering, the topological sort and every allocation a cycle will need.
- `Session::install_schedule(PreparedSchedule) -> PreparedSchedule` — moves only, and hands
  back the schedule it displaced so the caller chooses which thread frees it.

`apply_graph_changes()` remains as the composition of the three, so existing callers and
tests are untouched. `GraphScheduler`'s apply closure now runs the three separately, holding
the session lock for the describe and the install but **not** across the build.

Two things this rests on, both now asserted:

- **`graph_applied_id` comes from what the build saw, not from the session's current
  request id.** Otherwise a change landing mid-build is absent from the schedule while the
  session reports itself current, so nothing arms another rebuild and the change never
  routes — silence with no stale-cycle count to point at.
  `a_change_arriving_during_a_build_leaves_the_graph_stale` is the only test that catches
  this; it was mutation-checked.
- **Removals are tombstones.** Installing a schedule built from an older topology is safe
  only because `remove_port`, `remove_loop` and `remove_channel` disconnect and disable but
  never shrink an arena, so every index an older schedule holds still names the same object.
  Guarded by the pre-existing `removal_never_shrinks_the_arenas` plus the new
  `a_schedule_built_before_a_removal_still_installs_and_runs`.

### Step 4 — the published snapshot grown, and the pump added ✅

`state.rs` is new. The four state structs moved there out of `control.rs`, because two layers
need them and the publishing side should not depend on the handle layer — which is the
direction `control.rs` had it. The `lib.rs` re-export is unchanged, so the frontend's imports
did not move.

Port names are the one thing that could not simply be published: a name is a `String`, and
the audio thread can neither clone one nor own one. So the shapes are split —
`AudioPortSnapshot` / `MidiPortSnapshot` carry only the per-cycle numbers, and
`.named(..)` completes them into `AudioPortState` / `MidiPortState` on the control side,
using the name whoever holds the port handle already supplied when creating it. The split is
enforced, not merely documented: `publishing_state_does_not_allocate` now covers the port
path, and adding a `name.to_string()` back into `publish_state` aborts the test binary on a
3-byte allocation.

`StateSnapshot` now carries loops, audio channels, MIDI channels, audio ports and MIDI ports.
Both channel vectors and both port vectors are indexed by the session's single arena, with
the slot of the other kind left at its default or `None` — one extra slot per entity, in
exchange for an index a handle can use with no second map. The pre-sized-box +
`truncated()` + grow-on-`poll()` pattern was generalised to all five vectors.

That set is exactly right, which is worth recording because it was checked rather than
assumed: of the nine `get_state` methods in `app_backend.rs`, precisely five read the session
— `Loop`, `AudioChannel`, `MidiChannel`, `AudioPort`, `MidiPort` — and those five are what a
snapshot now carries. `BackendSession::get_state`, `AudioDriver::get_state` and both
`FXChain` state calls read no session at all. `get_data`, `get_all_midi_data` and
`dummy_dequeue_data` stay blocking queries; `get_connections_state` is driver-side.

`Engine::pump()` applies queued control work without running a cycle, for the two states
where cycles are not arriving: a driver spinning without processing, and an engine no driver
has taken yet. `ParkedEngine` was deliberately *not* added — there is no caller for it until
step 5, and guessing its shape first is the mistake this whole exercise is about.

## What remains

| module | dead part | live part | tests |
|---|---|---|---|
| `engine.rs` (1072 LOC) | `split`, `Engine`, `EngineHandle`, `Stats`, `StateSnapshot` | `LoopState` (re-exported at `lib.rs`) | 17 inline; 5 in `tests/no_alloc.rs`; 1 in `tests/external_ports.rs` |
| `control.rs` (769 LOC) | the handle API in full: `Backend`, `Loop`, `AudioChannel`, `MidiChannel`, `Port`, `ControlError` | — (its state structs have moved to `state.rs`) | 15 in `tests/control.rs` |

`control.rs` is now *entirely* the dead handle API, and still should not be deleted before
step 5: it is the conversion template. `engine.rs` is the target boundary.

### Step 5 — swap the boundary (not started)

The driver callbacks take ownership of `Engine`, so the four `shared.lock_rt()` sites become
`engine.session_mut()` and the audio thread stops locking. The 26 `shared.lock()` sites
become `mutate` / `query` / snapshot reads, taking `control.rs`'s bodies as they go.
`tests/control.rs` is re-pointed at `app_backend::BackendSession` with a Dummy
`AudioDriver`, keeping its 15 assertions on the live path. `ControlGuard` retires, because a
command that mutates topology arms the scheduler as part of the same queued unit.

**A coupling found while finishing step 4, which changes this step's shape.** Once the
`Engine` owns the session, the scheduler thread can no longer call `describe_topology()` — it
has no `&Session` to call it on. So the three phases from step 3 have to be redistributed
across the queue:

- **describe** becomes a blocking query (`send_and_wait`), answered on the audio thread;
- **build** stays on the scheduler thread, unchanged;
- **install** becomes a queued command whose closure *owns* the `PreparedSchedule` and keeps
  the one it displaces, so the command box carries the old schedule back to the control side
  and frees it there. This is precisely what `engine.rs`'s return queue was built for, so no
  new mechanism is needed.

The consequence is that **`Engine::pump()` becomes load-bearing rather than a convenience**:
with describe as a blocking query, a session whose engine is not being cycled cannot rebuild
its graph at all. That covers session construction (no driver yet) and the entire QML suite
(dummy driver in controlled mode, which hands out no frames unless a test asks). Whatever
drives the pump in those states has to exist before the swap lands, or the graph silently
stops converging — the same failure mode, from a new direction, as the bug that started all
this.

An incremental alternative worth weighing before committing to the full swap: keep
`Mutex<Session>` and have the driver callbacks publish a `StateSnapshot` at the end of each
cycle, then point the frontend's 40 Hz polls at the snapshot instead of the lock. That
removes the large majority of GUI/audio lock acquisitions (N objects × 40 Hz) with no
ownership change and no scheduler rework, and it exercises step 4's snapshot against the real
frontend before the boundary moves. It does not remove the audio thread's own lock, so it is
a reduction rather than a fix.

## Related known issue, deliberately not fixed

Both live callbacks (`app_backend.rs`, `JackProcess::process` and the cpal output callback)
take a plain `std::sync::Mutex` on the audio thread, contending with a GUI thread that holds
the same lock for every state poll and every control operation — some of which allocate. On
JACK this risks the watchdog zombifying the client; on cpal it degrades to glitching. The
`lock()` / `lock_rt()` split in `app_backend.rs` marks exactly which callsites are on the
audio thread, so the scope of a fix is visible.

Step 3 shortened the worst offender: the topological sort no longer happens inside the
locked region. Step 5 removes the contention outright. Judged tolerable until then, on the
grounds that it has not been observed biting in practice.
