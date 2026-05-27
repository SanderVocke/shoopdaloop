# Plan: Refactor AudioMidiDriver for Rust Portability (no behavior/perf regression)

## Goal
Prepare `src/backend/internal/AudioMidiDriver` and related driver implementations for full Rust migration by removing C++ patterns that are incompatible with `cxx` bridges:
1. class inheritance as the primary extensibility mechanism,
2. storing callable processing logic as C++ function objects,
3. storing smart pointers to interface classes in cross-boundary state.

The refactor must preserve current behavior, processing order, and real-time safety/performance characteristics.

## Key code locations
- Core C++ driver abstraction:
  - `src/backend/internal/AudioMidiDriver.h`
  - `src/backend/internal/AudioMidiDriver.cpp`
- Driver factory and type dispatch:
  - `src/backend/internal/AudioMidiDrivers.h`
  - `src/backend/internal/AudioMidiDrivers.cpp`
  - `src/backend/libshoopdaloop_backend.cpp` (type checks, API wiring)
- Concrete drivers:
  - `src/backend/internal/DummyAudioMidiDriver.h/.cpp`
  - `src/backend/internal/jack/JackAudioMidiDriver.h/.cpp`
- Existing Rust-side core and CXX bridge:
  - `src/rust/backend_rust/src/audio_midi_driver.rs`
  - `src/rust/backend_rust/src/audio_midi_driver_cxx.rs`
  - `src/rust/backend_rust/src/backend_api_cxx.rs`
- Tests touching this area:
  - `src/backend/test/unit/test_DummyAudioMidiDriver.cpp`
  - `src/backend/test/unit/test_JackPorts.cpp`
  - `src/backend/test/integration/test_*`

## Target architecture

### 1) Replace inheritance-based behavior with composition + ops interface
- Introduce a non-virtual driver façade (or evolve existing `AudioMidiDriver`) that owns:
  - common driver state (already partly in Rust core),
  - command queue,
  - processor/decoupled-port registries,
  - a backend ops table/adapter for backend-specific actions (`start`, `open_audio_port`, `open_midi_port`, `close`, `find_external_ports`, telemetry refresh hooks, wait behavior).
- Concrete backends (Dummy/Jack) become ops providers/adapters rather than subclasses depended on for polymorphism.
- Keep public C API unchanged during migration by preserving existing external handles and behavior.

### 2) Replace process callback member with FFI-safe callback representation
- Replace stored C++ callable patterns with explicit C-ABI-safe callback tuple:
  - function pointer + opaque user pointer.
- Keep invocation point and order identical inside process cycle:
  1. optional process hook,
  2. command queue handling,
  3. decoupled MIDI processing,
  4. processor callbacks,
  5. set last processed.
- Ensure callback registration/lifetime rules are explicit and deterministic.

### 3) Replace smart-pointer interface registries with stable handles
- Replace cross-boundary registries of interface smart pointers with handle-based registration (`usize`/`u64` IDs).
- Keep ownership local to the owning language side; cross-boundary only passes handles.
- Maintain current copy-on-write/snapshot semantics for process-thread iteration to avoid lock contention/regressions.
- Provide explicit register/unregister APIs for processors and decoupled ports; add stale-handle handling.

## Phased implementation

### Phase A: Introduce compatibility layer without behavior change
- Add new ops abstraction and callback tuple APIs while keeping old virtual/subclass pathway functional.
- Bridge old implementations (Dummy/Jack) through adapters so runtime behavior remains identical.
- Add tracing/assertions for process order and callback invocation parity.

### Phase B: Migrate processor/port registries to handle-based APIs
- Add dual-path support (old smart-pointer API + new handle API).
- Internally source process iteration from handle-based snapshot.
- Gradually switch call sites in backend and API layer to handle registration/unregistration.
- Keep tests green throughout migration.

### Phase C: Retire inheritance and pointer-interface coupling
- Remove/deprecate direct reliance on virtual overrides in core driver path.
- Convert factory (`AudioMidiDrivers`) to construct façade + backend ops.
- Replace `dynamic_cast`-based type checks in API dispatch with explicit backend type metadata (enum/tag) carried by driver instance.

### Phase D: Rust-port enablement follow-through
- Align CXX bridge surface to new abstractions (ops-neutral core, handle registries, callback tuple).
- Ensure Rust core becomes canonical owner for shared driver state and registration metadata.
- Keep any backend-specific platform integration (e.g., JACK callbacks) in narrow adapters.

## Behavior/performance constraints
- No changes in externally observable sequencing or timing semantics.
- No additional locks on RT/audio thread; preserve snapshot iteration strategy.
- Callback indirection should remain one predictable function-pointer hop.
- Preserve current command queue semantics and `wait_process` behavior.

## Build and test instructions
From repository root:
1. `cargo build`
2. `cargo test`
3. Ensure backend C++ tests are built and run via produced `test_runner` executable (generated as part of backend crate build); run it from its target path.
4. Run app self-tests: `./target/debug/shoopdaloop_dev.sh --self-test`
5. Final verification before completion:
   - `cargo fmt --all`
   - `RUSTFLAGS="-D warnings" cargo build`
   - rerun relevant tests (`cargo test`, backend `test_runner`, self-test) to confirm no regressions.

## Risks and mitigations
- Risk: Hidden behavior in subclass overrides.
  - Mitigation: adapter phase + parity tests + process-order assertions.
- Risk: Handle lifetime bugs.
  - Mitigation: explicit ownership contracts, unregister-on-drop hooks where applicable, stale-handle-safe lookup.
- Risk: API-level type dispatch breakage from removing `dynamic_cast` checks.
  - Mitigation: explicit driver type tag and migration shim maintaining old behavior.

## Deliverable definition
- Driver core no longer depends on inheritance for backend polymorphism.
- No stored `std::function`/interface smart-pointer state crossing FFI boundaries.
- Handle-based registration active for processors/decoupled ports.
- Existing public C API behavior preserved.
- Full project builds/tests pass; formatting and warnings requirements satisfied.