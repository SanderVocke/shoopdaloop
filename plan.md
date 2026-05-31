# Plan: complete migration to Rust-backed `AudioMidiDriver` with ultra-thin C++ wrappers

## Goal
Finish the migration so `AudioMidiDriver` and `DummyAudioMidiDriver` are effectively Rust-owned implementations with only minimal C++ forwarding/wrapper code for ABI/API compatibility.

Current state already achieved:
- `AudioMidiDriver` process-cycle orchestration moved to Rust runtime (`process_cycle`) with C++ trampolines.
- `DummyAudioMidiDriver` process thread/timing loop moved to Rust, C++ methods delegate.
- Decoupled MIDI registration bookkeeping moved to Rust, with C++ keepalive reintroduced to fix a lifetime race.
- Incremental Phase B handle-lifecycle scaffolding added: Rust per-handle decoupled dispatch APIs for process/close and a C++ close trampoline; unregister path now closes via Rust handle dispatch before unregister.
- Build/test gates currently pass including QML self-test after the keepalive fix.

## Scope and compatibility constraints
- Keep external C API and C++ call signatures stable during migration.
- Minimize changes in unrelated C++ modules.
- Use callback trampolines/opaque handles where required.
- Prioritize correctness and lifetime safety over immediate architectural purity.
- Perform migration in small verifiable steps with tests at each milestone.

## Key remaining blockers to a truly thin C++ layer
1. Processor dispatch still relies on C++ polymorphic objects and raw pointer handles.
2. Decoupled MIDI object lifetime/logic still crosses C++/Rust ownership in fragile ways.
3. Port creation/ownership still rooted in C++ classes (`DummyAudioPort`, `DummyMidiPort`) and C++ return types.
4. `AudioMidiDriver` remains a C++ abstract base with stateful responsibilities.
5. Settings boundary still uses C++ interface + unchecked cast pattern.
6. Dummy template shell (`DummyAudioMidiDriver<Time, Size>`) remains part of wrapper surface.

## Suggested execution phases

### Phase A: harden lifetime/race behavior first
Add targeted tests to lock in decoupled MIDI safety and avoid regressions before further ownership migration.

Work:
- Add backend tests that exercise:
  - close/unregister while processing is active
  - repeated open/close cycles with queued MIDI messages
  - stale handle access behavior from API side
- Ensure no panic/abort behavior in Rust queue paths.

Acceptance:
- `cargo test` and backend `test_runner` include explicit race/lifetime coverage.
- QML self-test remains green.

### Phase B: move decoupled MIDI object ownership to Rust
Keep existing API shape but migrate actual decoupled port lifetime/queue ownership from C++-anchored object lifetime to Rust-owned object/state.

Work:
- Introduce Rust-side decoupled-port registry with stable IDs/handles.
- Keep C++ wrapper object minimal and non-owning (or ownership only of handle token).
- Route push/pop/process/close through Rust-owned state.
- Remove C++ keepalive set once ownership is fully Rust and lifecycle safe.

Acceptance:
- No raw C++ decoupled object pointers required by Rust process-cycle for decoupled logic.
- Existing API functions (`open_decoupled_midi_port`, `maybe_next_message`, `close_decoupled_midi_port`, etc.) preserve behavior.

### Phase C: formalize processor handle lifecycle and callback bridge
Keep C++ processors for now but make Rust-side registration robust and explicit.

Work:
- Replace ad-hoc raw pointer assumptions with stable registration tokens (or validated handle table).
- Ensure unregister removes any possibility of callback after destruction.
- Keep trampoline invocation but with stronger lifetime contracts.

Acceptance:
- Processor add/remove cycles safe under concurrent process-thread command activity.
- No use-after-free hazards via stale callback handles.

### Phase D: collapse `AudioMidiDriver` C++ class into forwarding-only wrapper
Reduce C++ class responsibilities to argument conversion + delegation.

Work:
- Move remaining mutable state/storage responsibilities to Rust runtime object(s).
- Keep C++ virtual methods/signature compatibility but forward directly.
- Remove any C++ state containers not strictly required for ABI wrappers.

Acceptance:
- `AudioMidiDriver.cpp` contains mostly delegation/trampolines.
- Process ordering, command queue semantics, and wait behavior unchanged.

### Phase E: simplify dummy wrapper/template/settings boundary
Preserve external compatibility while removing internal C++ complexity.

Work:
- Keep template instantiations for ABI compatibility but route all to one shared Rust backend path.
- Replace unchecked settings cast boundary with typed bridge/config conversion.
- Ensure dummy wrapper has minimal non-forwarding code.

Acceptance:
- No behavior changes for existing users.
- Wrapper becomes declarative, not stateful.

### Phase F: final cleanup and documentation
Work:
- Remove dead code paths and obsolete comments.
- Update docs/comments for Rust-vs-C++ responsibility split.
- Confirm all final gates pass.

Acceptance:
- Clean codebase with explicit ownership model and lifecycle guarantees.

## Where relevant code is located
- C++ wrappers/drivers:
  - `src/backend/internal/AudioMidiDriver.h/.cpp`
  - `src/backend/internal/DummyAudioMidiDriver.h/.cpp`
  - `src/backend/internal/DecoupledMidiPort.h/.cpp`
  - trampoline headers in `src/backend/internal/*CxxTrampolines.h`
- C++ API layer:
  - `src/backend/libshoopdaloop_backend.cpp`
- Rust backend core/bridges:
  - `src/rust/backend_rust/src/audio_midi_driver*.rs`
  - `src/rust/backend_rust/src/dummy_audio_midi_driver*.rs`
  - `src/rust/backend_rust/src/decoupled_midi_port*.rs`

## Build/test instructions (project-specific)
From repository root:
1. Iterative build: `cargo build`
2. Rust tests: `cargo test`
3. Backend C++ tests: run backend Catch2 `test_runner` built under `target/.../test/test_runner`
4. App self-test: `./target/debug/shoopdaloop_dev.sh --self-test`

Final gate at completion:
1. `cargo fmt --all`
2. `RUSTFLAGS="-D warnings" cargo build`
3. `cargo test`
4. backend `test_runner`
5. `./target/debug/shoopdaloop_dev.sh --self-test`

If external environment/dependency issues occur outside project code, report as environment blocker with logs and stop.

## Deliverable criteria
- `AudioMidiDriver` and `DummyAudioMidiDriver` operationally Rust-owned.
- C++ wrapper logic reduced to thin forwarding/trampoline layer.
- No known decoupled MIDI lifetime race/panic paths.
- Full test suite and self-test pass under final gate commands.
