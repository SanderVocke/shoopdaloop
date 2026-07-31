# Composite RT automated verification record

## Gate status

This is a live Stage 6 record. It is intentionally **not** a final green gate: the complete RT callback audit, real-backend stability, and manual validation remain open. The existing composite QML matrix and focused nested save/load coverage are green; the only last full-suite failure is explicitly environment-qualified below.

| Gate | Current status |
|---|---|
| Targeted composite engine tests | Passing |
| Allocator-enforced engine tests | 21 passing, including dense/failure, lifecycle, topology restart, and transactional grab paths |
| Warning-denied core engine build | Passing without application/backend features |
| Complete engine suite with `app_backend` | Project compiles; full run blocked/unreliable on unavailable JACK/ALSA services |
| Application composite adapter tests | 3 passing with dummy driver |
| Frontend Rust build/unit tests | Warning-denied check passes; 32 library unit tests pass |
| QML self-tests | Focused composite 26/26 and nested save/load 6/6; last full run 188/189 with sole CPAL device/port environment failure |
| Full workspace suite | Core/no-default engine suite passes; application-feature workspace gate pending |
| Callback benchmark/cost gate | Composite-only measurement recorded; whole callback gate pending |

## Environment

Recorded 2026-07-31T17:20:52+02:00:

- Host: Linux x86_64, NixOS kernel `7.0.3`, `PREEMPT_RT`.
- Rust: `rustc 1.97.0-nightly (ad3a598ca 2026-05-03)`.
- Cargo: `cargo 1.97.0-nightly (4f9b52075 2026-05-01)`.
- Agent model: `gpt-5.6-sol`.
- ALSA, JACK, LV2, Lua, Qt, libclang, OpenGL, and libsndfile build dependencies were located in the Nix store and exposed explicitly for application/frontend checks.
- No reliable live JACK/ALSA service is available for the full real-backend test gate.

## Passing targeted runs

### Complete core engine suite without application/backend features

```sh
cargo test -p shoop_engine --no-default-features
```

Result: all unit, integration, allocator, and doc-test binaries passed. The last complete run before the deferred-replacement additions reported 535 library-unit passes and zero failures across all integration binaries. A repeat complete core run is required before final status; current targeted coverage contains 63 composite-focused tests and 21 allocator tests.

```sh
RUSTFLAGS="-D warnings" cargo check -p shoop_engine --no-default-features --all-targets
```

Result: passed, including tests and the callback benchmark target.

This is broader regression evidence but is not a substitute for the required `app_backend` gate.

### Composite state, timing, control, and transport

```sh
cargo test -p shoop_engine \
  --test composite_state_machine \
  --test composite_timeline \
  --test composite_timing \
  --test composite_control
```

Latest equivalent targeted run after the control/timeline changes:

- `composite_state_machine`: 25 passed, 0 failed.
- `composite_timeline`: 12 passed, 0 failed.
- `composite_timing`: 10 passed, 0 failed.
- `composite_control`: 16 passed, 0 failed.

Covered surfaces include pure semantics, exact samples, callback partition independence, nested propagation, fixed callback command cutoff, timestamp rejection, monotonic plan versions, stale topology, stopped/pending/running activation, same-topology candidate supersession, changed-topology callback-boundary restart, non-RT rejected/displaced/retired ownership, immediate seek validation, countdown transitions, play-after-record acceptance, fault reset, bounded snapshots, and trace observation after polling stalls.

### Allocation guard

```sh
cargo test -p shoop_engine --test no_alloc -- --test-threads=1
```

Latest result: 21 passed, 0 failed. The composite integration test queues plan installation, duplicate-version rejection, control acceptance, repeated processing, rolling trace, and state publication under `assert_no_alloc`. The grab test copies one rolling capture into two child loops and commits both modes in one transaction, then exercises duplicate-target rejection without allocation, deallocation, or partial mutation. Dense 64-target resolution and fail-closed primitive-event overflow are guarded separately, and a structural test rejects lock primitives in composite callback state sources.

This does not prove unexercised driver/FX branches safe. Open callback mechanisms are listed in [RT_SAFETY.md](RT_SAFETY.md#active-callback-path-audit-open-findings).

### Composite callback benchmark

```sh
cargo run --release -p shoop_engine --example composite_callback_bench
```

Result on the environment above:

```text
ordinary: 1 composites x 4 targets, 20000 callbacks: 1.516 us/callback (30.327 ms total)
maximum: 64 composites x 64 targets, 500 callbacks: 832.104 us/callback (416.052 ms total)
```

The maximum is a resolver/capacity stress case without representative driver, audio-channel, MIDI, or hosted-FX load. Interpretation and callback-budget risk are recorded in [RT_SAFETY.md](RT_SAFETY.md#composite-callback-cost-measurement).

### Formatting and warning-denied core build

```sh
cargo fmt --all --check
git diff --check
RUSTFLAGS="-D warnings" cargo build -p shoop_engine --no-default-features
```

All passed after the current changes.

## Application and frontend adapter evidence

With Nix development paths exported:

```sh
cargo test -p shoop_engine --features app_backend --test composite_app_backend -- --test-threads=1
RUSTFLAGS="-D warnings" cargo check -p frontend
cargo test -p frontend --lib
```

Latest focused application run: 3 tests passed, covering creation/configuration/control/state/removal, transactional transitive-cycle rejection, transitive dependent removal, idempotent removal, and primitive self-sync normalization. Warning-denied frontend compilation passed; 32 frontend Rust unit tests passed. The frontend test link required explicit Nix `LIBRARY_PATH` entries for OpenGL and libsndfile.

The application and complete QML suite were then built and run offscreen:

```sh
cargo build
target/debug/shoopdaloop_dev.sh --self-test
```

Result: **188 passed, 1 failed, 0 skipped** across 189 test cases. All 24 `CompositeLoop_running` cases passed, covering sequential/parallel/script/nested execution, countdowns, GUI/file-I/O stalls, immediate seeks, recording/play-after-record, circular references, conversion, and all current grab variants. The sole failure was `CpalPorts::test_virtual_playback_ports_are_app_connectable`: this host exposes no CPAL playback ports. The focused command for `tst_CompositeLoop_running.qml` independently passed 24/24.

After adding nested regular/script session-replacement coverage, the focused persistence file passed:

```sh
QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_Session_save_load.qml"
```

Result: **6 passed, 0 failed**, including saving, loading, reference restoration, engine reconfiguration, nested script execution, and teardown. The lifecycle extension to `composite_timeline_processing_does_not_allocate_or_free` also passed independently, covering stop cleanup, empty-timeline installation, and displaced plan ownership return without RT allocation or deallocation.

After deleting the fallback frontend state machine and dependency-cycle slots, both focused files were repeated against a warning-denied application build:

```sh
RUSTFLAGS="-D warnings" cargo build
QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_CompositeLoop_running.qml"
QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_Session_save_load.qml"
```

Latest results: composite behavior **26/26 passed** and session save/load **6/6 passed**. The composite run includes all synchronized/unsynchronized, fixed/default-length, stop/play grab outcomes plus nested-composite flattening into one bounded engine adoption transaction. The retained frontend schedule traversal is limited to off-RT grab preparation; no compatibility slot advances composite runtime state.

The full `app_backend` suite was also attempted after dependency discovery. It is not recorded as green: tests requiring real JACK/ALSA services failed or teardown became unreliable on this host. That is a runtime-service/environment blocker, not evidence for completion.

## Pending required commands

```sh
cargo test -p shoop_engine --features app_backend
cargo test --workspace --features shoop_engine/app_backend
```

Before final status, repeat timing-sensitive composite tests, run dense/max-capacity and all required allocation failure paths, execute callback benchmarks, and record exact final totals here.
