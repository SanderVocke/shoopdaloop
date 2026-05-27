# Plan: Complete AudioMidiDriver migration to Rust (no C++ wrapper in final state)

## Objective
Replace the C++ `AudioMidiDriver` class hierarchy with a Rust-owned driver implementation exposed via `cxx`, while keeping the public C API behavior stable.

## Scope boundaries
- Keep `libshoopdaloop_backend.h` API signatures stable.
- Preserve process-order semantics and real-time behavior.
- Final production path must not instantiate/use C++ `AudioMidiDriver` class.

## Current status snapshot
- Rust core already holds atomic state + handle registries.
- C++ API dispatch moved partly from RTTI to explicit driver type tags.
- RT processor iteration now uses Rust handle snapshot.
- C++ ownership model (`shared_ptr/weak_ptr<AudioMidiDriver>`) still drives runtime.

## Execution roadmap

### Phase 0 — Baseline and migration guardrails
1. Keep an up-to-date dependency inventory (`docs/audio_midi_driver_rust_migration_inventory.md`).
2. Define parity checklist for behavior:
   - process order,
   - dummy controlled/automatic mode behavior,
   - wait semantics,
   - driver telemetry fields,
   - C API return/error behavior.
3. Add temporary debug assertions/logging hooks to compare old/new paths where dual-path exists.

### Phase 1 — Finalize Rust driver API contract
1. Extend Rust driver API (`backend_rust`) to include complete driver lifecycle and backend metadata:
   - create/destroy,
   - backend kind,
   - start/close/wait,
   - open audio/midi/decoupled ports,
   - external-port discovery,
   - dummy control methods.
2. Expose all through `audio_midi_driver_cxx.rs` (or split bridge modules if clearer).
3. Add Rust-side unit tests for all new state/lifecycle methods.
4. Add bridge-oriented tests (C++/Rust integration) for type/kind metadata, handles, and lifecycle edge cases.

### Phase 2 — Introduce Rust-owned driver handle runtime (parallel path)
1. Add Rust-owned driver registry and opaque handle type for backend runtime.
2. Introduce C++ interop helpers that translate `shoop_audio_driver_t*` to Rust handles (without using `AudioMidiDriver` pointers).
3. Keep existing C++ path temporarily behind compatibility shims.
4. Switch `create_audio_driver` and `destroy_audio_driver` to the Rust-owned handle path first.

### Phase 3 — Port process orchestration into Rust
1. Move full process-cycle orchestration into Rust (same exact order):
   - process hook,
   - command handling,
   - decoupled MIDI,
   - processors,
   - last_processed update.
2. Keep command queue semantics identical; if command queue remains C++ for now, use explicit bridge callbacks.
3. Verify RT constraints (no new locks on audio hot path).
4. Validate parity with focused tests and existing Dummy integration tests.

### Phase 4 — Dummy backend full production migration
1. Make Rust Dummy backend canonical for production API path.
2. Move/bridge all Dummy-only controls (`dummy_audio_*`, `dummy_driver_*`) to Rust driver handle implementation.
3. Remove production dependence on C++ `DummyAudioMidiDriver` class.
4. Update tests to avoid direct use of C++ dummy internals.

### Phase 5 — JACK migration strategy and implementation
1. Choose one final JACK strategy:
   - pure Rust JACK bindings, or
   - minimal C++ callback trampoline with Rust-owned state/logic.
2. Implement chosen strategy while preserving:
   - xrun updates,
   - sample-rate/buffer-size refresh,
   - DSP load updates,
   - external port notifications.
3. Remove inheritance dependency on `GenericJackAudioMidiDriver` in production path.
4. Validate JACK unit/integration behavior and C API parity.

### Phase 6 — C API rewiring end-to-end to Rust handles
1. Migrate all `libshoopdaloop_backend.cpp` driver call sites from `internal_audio_driver()` shared_ptr objects to Rust-handle dispatch.
2. Remove `g_active_drivers` C++ set of driver objects.
3. Ensure API errors/return semantics unchanged.
4. Update/fix backend session wiring (`set_audio_driver`, port open flows, decoupled port flows).

### Phase 7 — Remove C++ class hierarchy and shims
1. Delete production `AudioMidiDriver` class and `.cpp`.
2. Delete production `DummyAudioMidiDriver` / `JackAudioMidiDriver` class-based paths no longer needed.
3. Remove compatibility shims and obsolete includes.
4. Keep only minimal platform interop code that does not model driver ownership/behavior.

### Phase 8 — Test migration and stabilization
1. Refactor tests still depending on C++ class internals to use stable API/bridge behavior checks.
2. Ensure integration coverage for dummy controlled mode and JACK behavior remains equivalent.
3. Add regression tests for lifecycle/handle invalidation and destroy-order safety.

### Phase 9 — Final quality gates and cleanup
1. `cargo fmt --all`
2. `RUSTFLAGS="-D warnings" cargo build`
3. `cargo test`
4. backend `test_runner`
5. `./target/debug/shoopdaloop_dev.sh --self-test`
6. Remove temporary migration logs/assertions.
7. Update docs to describe Rust-owned backend architecture.

## Build and test commands
- Build: `cargo build`
- Rust tests: `cargo test`
- Backend C++ tests: `target/.../test/test_runner`
- App self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`
- Final strict build: `RUSTFLAGS="-D warnings" cargo build`

## Definition of done
- No C++ `AudioMidiDriver` wrapper/class in production runtime path.
- Driver lifecycle/state/process orchestration owned by Rust.
- C API behavior preserved.
- All tests passing and formatting/warning gates clean.