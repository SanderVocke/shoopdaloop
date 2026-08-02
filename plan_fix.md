# Fix plan: integrate backend composite loops with master’s object-control architecture

## Completion status

This plan has been implemented. Composite loops now use per-object lock-free state mirrors and sequenced controls; schedule installation has an explicit asynchronous acknowledgement; the old global composite state snapshot has been replaced by a trace-only diagnostic publication; anticipated primitive transitions are mirrored; prepared ringbuffer adoption publishes channel data off the realtime thread; and backend objects recover correctly when a session wrapper is replaced.

Final verification completed successfully:

- `cargo fmt --all -- --check`
- `RUSTFLAGS="-D warnings" cargo build`
- All focused composite, timing, control, state-machine, and no-allocation tests
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1`
- Full QML self-tests: 192 total, 191 passed, 0 failed, 1 supported CPAL hardware skip

The remainder of this document records the original problem statement, design requirements, and implementation checklist used to reach that result.

## Historical starting repository state

- Current branch: `rt_composites`
- Local merge commit: `61734d13 Merge remote-tracking branch 'origin/master' into rt_composites`
- Merge parents:
  - Branch before merge: `725c8ef4`
  - Merged `origin/master`: `106e2b9b`
- The branch has **not been pushed**.
- The merge commit is 22 commits ahead of `origin/rt_composites`.
- The merge originally conflicted in:
  - `src/rust/shoop_engine/src/app_backend.rs`
  - `src/rust/shoop_engine/src/engine.rs`
  - `src/rust/shoop_engine/src/lib.rs`

There is currently uncommitted diagnostic/fix work in:

- `src/rust/frontend/src/cxx_qt_shoop/rust/qobj_composite_loop_backend.rs`
- `src/rust/shoop_engine/src/app_backend.rs`
- `src/rust/shoop_engine/src/audio_midi_loop.rs`
- `src/rust/shoop_engine/src/basic_loop.rs`
- `src/rust/shoop_engine/src/session.rs`

Do not assume this uncommitted work is the right final design. Review it against the architecture described below and keep only pieces that fit that architecture.

## What changed on master

Master replaced the old shared engine snapshot/query model for ordinary frontend/backend objects with a per-object control model.

The important design elements are:

1. **Pending object handles**
   - Frontend-facing objects own an `ObjectControl`.
   - `ObjectControl` contains a session ID, lifecycle atomics, engine index, creation sequence, error state, and an `Arc` to the object’s state mirror.
   - Creation returns a pending handle immediately.
   - The queued engine command creates the engine object and marks the control ready.
   - Dropping a pending handle can cancel creation because the command holds only a `Weak` reference.

2. **Sequenced asynchronous commands**
   - Commands receive a `CommandSequence`.
   - The engine publishes `last_applied_command`.
   - Setters normally queue commands and return without waiting.
   - Explicit command fences are used only where the caller genuinely requires ordering or completion.
   - The callback drains only the commands accepted at callback start, preserving a fixed boundary cutoff.

3. **Per-object state mirrors**
   - Loops, channels, and ports publish state into dedicated mirrors.
   - Frontend reads do not issue engine queries and do not use a global session snapshot.
   - Scalar fields are atomic.
   - Complex channel data has dedicated mirror handling and acknowledgement sequences.

4. **Frontend desired-state write-through**
   - Where appropriate, a successful setter updates the frontend-visible desired mirror immediately while the command is pending.
   - Authoritative runtime values are subsequently published by the engine.

5. **No ordinary global polling snapshot**
   - Master removed the global state snapshot path for basic loops, channels, and ports.
   - Frontend update loops read their object mirrors directly.

## What the merge resolution got wrong

The initial merge resolution understood the master changes only partially and did not apply the same architecture to composite loops.

It retained the branch’s old composite state publication mechanism:

- `Engine` still owns a small global `StateSnapshot` pool for composite state.
- `EngineHandle::poll()` still returns composite snapshots.
- `CompositeLoop::poll_state()` reads the global snapshot.
- Composite transitions and configuration still use blocking query/result paths in several places.
- `CompositeLoop` does not have a master-style `ObjectControl` and per-object state mirror.

This is architecturally inconsistent with master. The later `queued_at_cycle` freshness workaround and explicit publishing of anticipated primitive transitions were attempts to compensate for stale state caused by this mismatch. They are not a substitute for a composite state mirror.

## Current uncommitted workaround changes

The uncommitted changes currently include the following ideas:

1. `app_backend.rs`
   - Restores a `queued_at_cycle` freshness check for the retained composite snapshot.
   - Tracks primitive loop controls with weak references.
   - Adds a persistent primitive sync-source cache so dropped frontend handles do not erase topology needed to validate prepared composite timelines.

2. `basic_loop.rs`, `audio_midi_loop.rs`, and `session.rs`
   - Add a way to publish a basic loop state with a composite-anticipated transition.
   - Publish anticipated primitive transitions when composite controls are accepted and after processing.
   - This concept may still be valid in the final design because basic loop mirrors must expose transitions scheduled by the composite runtime, but it should be reviewed and tested as part of the mirror design rather than retained blindly.

3. `qobj_composite_loop_backend.rs`
   - Attempts to install a dirty composite schedule immediately after ringbuffer adoption.
   - This did not resolve the remaining update timeout and should not be treated as a proven fix.

An attempted change made prepared ringbuffer adoption copy channel contents into `AudioChannelStateMirror` from the engine command. It made waveform/marker data visible, but the frontend update timeout remained and the change was reverted during diagnosis. The latest diagnostic test was interrupted before producing a result.

## Verification results so far

### Passed

Before the QML investigation:

- `RUSTFLAGS="-D warnings" cargo build`
- Focused composite tests
- Composite control tests
- Realtime no-allocation tests
- Full Rust workspace tests with unavailable hardware backends allowed to skip:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1
```

The full Rust workspace run passed.

### QML failures before workarounds

The full QML self-test command was:

```bash
QT_QPA_PLATFORM=offscreen \
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  target/debug/shoopdaloop_dev.sh --self-test
```

Initial result:

- 192 testcases
- 166 passed
- 25 failed
- 1 skipped

Failures were concentrated in:

- Composite runtime state/transition observation
- Composite ringbuffer grabs
- Basic `ThreeLoops` ringbuffer grabs

### Improvement from anticipated-transition publication

After publishing composite-anticipated transitions into basic loop mirrors, `tst_CompositeLoop_running.qml` improved to:

- 20 passed
- 6 failed

The six remaining failures were all ringbuffer-grab/update cases:

- `CompositeLoop_running::test_grab_ringbuffer_nested_composite_transaction`
- `CompositeLoop_running::test_grab_ringbuffer_synced_fixed_length`
- `CompositeLoop_running::test_grab_ringbuffer_synced_then_play`
- `CompositeLoop_running::test_grab_ringbuffer_synced_then_stop`
- `CompositeLoop_running::test_grab_ringbuffer_unsynced_then_play`
- `CompositeLoop_running::test_grab_ringbuffer_unsynced_then_stop`

Their immediate failure is generally:

```text
Backend not updated in time
```

After the attempted channel-data mirror publication, marker/data assertions stopped failing, but the backend update timeout remained. This suggests at least two separate concerns:

1. Adopted channel contents must reach the per-channel data mirror.
2. Some blocking or stalled update path remains around adoption and composite schedule refresh.

The `ThreeLoops` ringbuffer-grab tests must also be rerun after the final solution; they have not been proven fixed.

## Required target architecture

Composite loops should follow the same principles as the master-side basic objects.

### 1. Add a per-object composite state mirror

Introduce a `CompositeLoopStateMirror` containing the frontend-visible composite runtime state, including at least:

- Stable `LoopIdentity`
- Lifecycle/readiness as appropriate
- Active plan version
- Pending plan version
- Current mode
- Next mode
- Next transition delay
- Iteration
- Cycle count
- Length
- Position
- `play_after_record`
- Runtime counters
- Runtime fault
- Active children

Scalar values should use atomics where possible. Multi-field or bounded-array values need a coherent lock-free publication strategy, such as a generation/seqlock-style protocol or an existing project pattern. Do not put a mutex in the realtime publication path.

Active children are bounded by the existing composite limits, so fixed-capacity storage should be possible.

### 2. Give composite handles master-style object control

`CompositeLoop` should have a control structure analogous to other backend objects, rather than relying on a session-global snapshot.

Decide explicitly how composite identity and lifecycle work:

- The current branch allocates stable composite `LoopIdentity` slots on the control side.
- That may remain useful, but readiness/failure/closure and mirror ownership should still be represented consistently.
- Timeline installation/replacement must preserve or reconnect the same mirror for a retained composite identity.

Pending creation/configuration must not accidentally be kept alive by strong references in session-side bookkeeping. Preserve master’s cancellation behavior where applicable.

### 3. Use sequenced asynchronous commands

Composite operations should normally queue commands and return `CommandSequence`, including:

- Transition requests
- Immediate transitions
- `play_after_record`
- Configuration/timeline installation where practical
- Removal/closure

Do not use a blocking engine query merely to update ordinary frontend state.

Transactional operations that can be rejected—prepared timeline installation, topology/version validation, or plan replacement—need an explicit completion/error mechanism. That mechanism should be separate from ordinary state polling and should not make every frontend refresh block on the audio thread.

### 4. Publish runtime state directly to the composite mirror

The session/composite timeline should publish each composite’s state:

- After accepted controls when frontend-visible pending state changes immediately
- At the end of processing/callback boundaries
- After plan activation or replacement
- After stop/removal/fault-reset operations

Once this is working, remove the retained composite `StateSnapshot` machinery:

- Composite vectors in `engine::StateSnapshot`
- Snapshot producer/consumer pools if no other consumer remains
- `EngineHandle::poll()` for composite state
- `snapshots_dropped`
- `SharedSession::queued_at_cycle` snapshot freshness workaround

Transition trace/history is diagnostic session-level information, not ordinary per-object state. If it still needs frontend access, give it a separate bounded diagnostic publication mechanism instead of retaining the whole old state snapshot architecture.

### 5. Integrate anticipated primitive transitions with basic loop mirrors

Composite control can schedule a future transition for a primitive loop without adding a transition to the primitive loop’s own queue. The basic loop frontend still needs to see that next mode/delay.

Preserve this behavior through the basic `LoopStateMirror`, but implement it deliberately:

- Publish the effective next transition as primitive planned transition first, otherwise composite-anticipated transition.
- Update it when composite controls are accepted.
- Update it as countdowns advance or plans change.
- Clear it when transitions are cancelled, plans are removed, or identities become stale.
- Add focused Rust tests proving the mirror behavior.

The current uncommitted helper methods are a possible starting point, not a completed solution.

### 6. Fix ringbuffer adoption without violating realtime constraints

The branch intentionally prepares ringbuffer destination storage off the realtime thread and commits it transactionally. Preserve these properties:

- Validation is all-or-nothing.
- Storage allocation/preparation happens off the realtime thread.
- Commit does not allocate or free in the realtime section.
- Replaced buffers are returned and dropped off the realtime thread.

At the same time, master expects frontend channel data to be available through each channel’s data mirror.

Do not solve this by locking or allocating in the realtime callback. Instead design an off-realtime publication path. Possible direction:

1. Prepare/copy adopted data into control-owned prepared storage.
2. Commit prepared buffers on the engine thread.
3. Return an acknowledgement/result carrying enough ownership or metadata to update the corresponding channel mirrors on a control thread.
4. Update each `AudioChannelStateMirror` data vector off the realtime thread.
5. Advance/acknowledge the channel data sequence consistently.

This likely requires reliable mapping from adoption targets to their `AudioChannel` object controls or mirrors. Avoid strong-reference registries that break pending-object cancellation.

Also remove blocking work from frontend refresh callbacks. `CompositeLoopBackend::update()` should read mirrors and queue work, not wait for engine round trips. Ringbuffer adoption and any required schedule recompile/install should have explicit asynchronous completion/fencing.

### 7. Preserve composite realtime guarantees

Do not regress the branch’s existing guarantees:

- Fixed command cutoff at callback start
- No allocation/free in realtime processing
- Prepared timeline validation
- Primitive topology recheck at activation
- Version ordering and stale-version rejection
- Runtime-preserving replacement where valid
- Deterministic dependency ordering
- Bounded event/control/trace capacities
- Plan reclamation and destruction off the realtime thread
- Transactional multi-loop ringbuffer adoption

## Suggested implementation sequence

1. **Review or discard the current uncommitted workaround diff.**
   - Preserve only changes that fit the target architecture.
   - Do not commit the snapshot freshness workaround as the final design.

2. **Implement and unit-test `CompositeLoopStateMirror`.**
   - Start with mirror read/write tests, including coherent active-child publication.

3. **Attach mirrors to composite timeline nodes/identities.**
   - Ensure replacements preserve mirror association by stable identity.
   - Ensure removed/stale identities stop publishing.

4. **Convert `CompositeLoop` frontend handles to mirror reads.**
   - Remove `poll_state()` dependence on `EngineHandle::poll()`.
   - Make `get_state()` a mirror read with lifecycle-aware errors, as for other objects.

5. **Convert composite control methods to sequenced commands.**
   - Add explicit acknowledgement only for operations whose rejection must be observed.
   - Ensure QML exact waits can fence the relevant sequence through the existing backend wait mechanisms.

6. **Publish effective primitive next transitions into `LoopStateMirror`.**
   - Add tests for acceptance, countdown, cancellation, replacement, and immediate transition.

7. **Remove the global composite snapshot path.**
   - Update existing composite tests to read mirrors or explicit diagnostics.

8. **Redesign prepared ringbuffer adoption mirror publication.**
   - Keep preparation and data-mirror replacement off realtime.
   - Add tests for data visibility, data-dirty acknowledgement, transactional failure, and no allocation.

9. **Update QML composite backend code.**
   - Make refresh/update mirror-only and nonblocking.
   - Queue schedule installs and adoption operations.
   - Fence only in explicit test/session-settle paths.

10. **Run focused tests after each layer, then all gates.**

## Required verification before push

Run formatting and warning-free build:

```bash
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build
```

Run focused Rust tests while iterating:

```bash
cargo test -p shoop_engine --features app_backend --test composite_app_backend
cargo test -p shoop_engine --features app_backend --test composite_control
cargo test -p shoop_engine --features app_backend --test composite_state_machine
cargo test -p shoop_engine --features app_backend --test composite_timeline
cargo test -p shoop_engine --features app_backend --test composite_timing
cargo test -p shoop_engine --features app_backend --test no_alloc
```

Run the complete Rust workspace gate:

```bash
SHOOP_ALLOW_MISSING_BACKENDS=1 \
  cargo test --workspace --features shoop_engine/app_backend -- --test-threads=1
```

Run affected QML files first:

```bash
QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 \
  target/debug/shoopdaloop_dev.sh --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_CompositeLoop_running.qml"

QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 \
  target/debug/shoopdaloop_dev.sh --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_ThreeLoops.qml"

QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 \
  target/debug/shoopdaloop_dev.sh --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_Session_save_load.qml"
```

Then run the full QML gate:

```bash
QT_QPA_PLATFORM=offscreen SHOOP_ALLOW_MISSING_BACKENDS=1 \
  target/debug/shoopdaloop_dev.sh --self-test
```

Do not push while any of the known composite or ringbuffer cases fail.

## Completion criteria

The integration is complete only when:

- Composite frontend state is read from a per-object mirror, not a global snapshot.
- Ordinary composite controls are sequenced and asynchronous.
- Rejected transactional operations have an explicit, bounded acknowledgement path.
- Basic loop mirrors expose composite-anticipated transitions correctly.
- Ringbuffer adoption updates frontend channel data without realtime allocation, freeing, or locking.
- Pending object cancellation and cross-session validation still work.
- The global composite snapshot workaround is removed.
- Focused realtime/composite tests pass.
- Full Rust workspace tests pass.
- Full QML self-tests pass apart from explicitly supported hardware-backend skips.
- Only then is `rt_composites` pushed to `origin`.
