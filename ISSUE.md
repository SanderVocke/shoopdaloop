# QML Test Suite Hang: `wait_updated` async pipeline breakdown across test-file reloads

## Summary

The full QML test suite (`./target/debug/shoopdaloop_dev.sh --self-test`) hangs
after ~22 of ~26 test files have passed. Individual test files pass in isolation.
The hang occurs in `wait_updated()` which waits for the `updated_on_gui_thread`
QML signal — part of the async state-propagation pipeline that connects the
dummy audio driver thread to the QML GUI thread.

## Root cause hypothesis

Each test file creates and destroys a `BackendWrapper` which owns an
`AudioDriver` with a dedicated dummy-processing thread. The async update
pipeline is:

```
UpdateThread (singleton, 40 Hz timer)
  → update() signal
    → BackendWrapper::update_on_other_thread()  [DIRECT connection]
      → d.get_state() → s.get_state()
      → emit updated_on_backend_thread()
        → BackendWrapper::update_on_gui_thread()  [QUEUED → GUI thread]
          → emit updated_on_gui_thread()
```

`wait_updated()` connects to `updated_on_gui_thread` and polls the GUI event
loop via `wait()` → `process_events()` / `send_posted_events()`.

After many test-file reloads (each creating a new BackendWrapper / AudioDriver /
dummy thread and then destroying them), the `updated_on_gui_thread` signal
stops being delivered to the QML `wait_updated` handler. This causes
`wait_updated` to time out after 3 × 500 ms = 1.5 s per call, and tests that
rely on it (e.g. `check_backend()`, `reset()`) hang when they call
`wait_updated` in a loop.

The exact mechanism (connection leak, event-loop stall, Qt object lifetime
issue during `delete_later`, singleton UpdateThread state corruption) is
still unknown.

## What changed in this branch

### 1. `last_processed = 0` when dummy thread drains to zero
**File:** `src/rust/shoop_engine/src/app_backend.rs`

The dummy thread's `process_dummy_driver_iteration` did not reset
`last_processed` when `n == 0` (all requested frames consumed). This caused
the QML `wait_controlled_mode()` to loop forever: `while (last_processed != 0)`
never terminated because `last_processed` stayed at the last non-zero value.

**Fix:** Set `last_processed = 0` in the early-return path for `n == 0`.
This fixed the ThreeLoops test hang.

### 2. Synchronous `dummy_wait_controlled_mode()`
**Files:**
- `src/rust/shoop_engine/src/app_backend.rs` — new `dummy_wait_controlled_mode()`
- `src/rust/frontend/src/cxx_qt_shoop/rust/qobj_backend_wrapper.rs` — wrapper
- `src/rust/frontend/src/cxx_qt_shoop/rust/qobj_backend_wrapper_bridge.rs` — bridge
- `src/qml/test/ShoopTestCase.qml` — hybrid `wait_controlled_mode`

Added a synchronous Rust method that polls `DriverInner.last_processed` and
`requested` directly, bypassing the async update pipeline. The QML
`wait_controlled_mode` now calls `wait_updated` once (to ensure at least one
graph-change / transition cycle is applied) then delegates to the synchronous
wait.

### 3. Event flushing in `unload_qml`
**File:** `src/rust/frontend/src/cxx_qt_shoop/rust/qobj_application.rs`

Added `self.as_mut().wait(100)` before and after `delete_later()` to attempt
to flush pending deferred deletions and Qt connection cleanups between
test-file loads. **Did not resolve the issue.**

## Current status

| Test file | Status (full suite) | Status (isolated) |
|-----------|--------------------|--------------------|
| tst_Backend.qml | PASS | PASS |
| tst_Backend_jack.qml | PASS | PASS |
| tst_CompositeLoop_running.qml | PASS (all 24) | PASS |
| tst_Cpal_ports.qml | SKIP | SKIP |
| tst_FetchDisplayData.qml | PASS | PASS |
| tst_Jack_ports.qml | PASS | PASS |
| tst_LoopReorder.qml | PASS | PASS |
| tst_LuaEngine.qml | PASS | PASS |
| tst_LuaEngine_SessionControlHandler.qml | PASS | PASS |
| tst_LuaScriptWithEngine.qml | PASS | PASS |
| tst_Midi.qml | PASS | PASS |
| tst_MidiControlPort.qml | PASS | PASS |
| tst_MidiControl_actions.qml | PASS | PASS |
| tst_MidiControl_filters.qml | PASS | PASS |
| tst_Profiling.qml | PASS | PASS |
| tst_Resample.qml | PASS | PASS |
| tst_SessionDescriptor_default.qml | PASS | PASS |
| tst_SessionDescriptor_track_controls.qml | PASS | PASS |
| tst_Session_channels.qml | PASS | PASS |
| tst_Session_save_load.qml | PASS | PASS |
| tst_ThreeLoops.qml | PASS | PASS |
| tst_TrackControlAndLoop_direct.qml | **HANGS** (sometimes) | PASS |
| tst_TrackControlAndLoop_drywet.qml | **HANGS** | PASS |
| tst_TrackControl_direct.qml | UNREACHED | PASS |
| tst_TrackControl_drywet.qml | UNREACHED | PASS |
| tst_TwoLoops.qml | UNREACHED | PASS |

The hang point varies between runs: sometimes in TrackControlAndLoop_direct,
sometimes in TrackControlAndLoop_drywet. When run with `--filter` to skip
TrackControlAndLoop_drywet, the remaining files (TrackControl_direct,
TrackControl_drywet, TwoLoops) all pass.

## Prior investigation (resolved)

The original bug in DummyBug.md — `CompositeLoop_running` and `TwoLoops`
failures in CI — was a separate deterministic logic bug (graph mutations
bumping `graph_request_id` without calling `apply_graph_changes()`, causing
`session.process()` to silently drop cycles). That was fixed in commit
`4beb3d0e`.

## Next steps / open questions

1. **Why does the async update pipeline break after ~22 test file reloads?**
   - Possible Qt connection accumulation despite `delete_later` cleanup
   - Possible UpdateThread timer stops or `update_queued` flag gets stuck
   - Possible lock contention accumulating across driver sessions

2. **Can `wait_updated` be replaced with a synchronous alternative?**
   - Add a `wait_updated_sync` that uses `wait_process()` + polls `last_processed`
   - Replace all `wait_updated` calls in test infrastructure
   - Risk: `wait_updated` is used in ~50+ call sites across test files

3. **Can the test runner be changed to restart the process for each file?**
   - Already works (individual files pass)
   - Would be slower but reliable

4. **Instrument the UpdateThread to log connection count / timer health**
   - Add a counter that logs how many slots are connected to `update()`
   - Log when `update_queued` is true for too long