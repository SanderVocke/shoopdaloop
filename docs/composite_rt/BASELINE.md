# Pre-change automated baseline

## Baseline point

- Source commit: `06266290769186be70d4bad9def74723ff9fd315`
- Run date: `2026-07-31` (UTC timestamp captured before runs: `2026-07-31T11:36:32Z`)
- Worktree before baseline: detached `HEAD`; only user-provided `PLAN.md` was untracked.
- Composite implementation at this point: frontend/update-thread only; no `shoop_engine` composite type or composite engine tests.
- Raw logs were retained locally under ignored `target/stage0_*.log`; this document is the canonical committed record.

## Environment

| Item | Value |
|---|---|
| Host | Windows 11 build `10.0.26200.8893` |
| Shell reported by `uname -a` | `MINGW64_NT-10.0-26200 ALSI-5CD5248D5K 3.6.5-22c95533.x86_64` |
| Rust | `rustc 1.96.0-nightly (f5eca4fcf 2026-04-09)` |
| Cargo | `cargo 1.96.0-nightly (eb94155a9 2026-04-09)` |
| Target | `x86_64-pc-windows-msvc` (from generated development launcher/toolchain paths) |
| Audio test backend | Dummy/controlled for composite QML tests |
| JACK | Unavailable: `libjack64.dll` could not be loaded (`LoadLibraryExW failed`) |
| CPAL | Application QML test selected CPAL virtual ports; `CpalTest` settings were absent, but `tst_Cpal_ports.qml` passed with the available CPAL path |

No behavior had been replaced before these runs. The later Stage 0 contract-helper tests are recorded separately below and are not presented as part of the pre-change baseline.

## Commands and results

### 1. Required engine command, unchanged environment

```sh
cargo test -p shoop_engine --features app_backend
```

**Result: FAIL (exit 101), environment-only JACK failures.** Compilation completed in `3m 16s`; Cargo had initially waited for a package-cache lock. Before Cargo stopped at the failing integration binary, 631 tests passed. `tests/jack_app_backend.rs` then ran four tests and all four failed before exercising assertions because JACK could not load:

- `session_output_reaches_a_jack_consumer`
- `registered_ports_are_visible_to_jack_with_direction_flags`
- `audio_keeps_flowing_across_a_mid_stream_topology_change`
- `jack_audio_input_reaches_a_recording_channel`

The common diagnostic was:

```text
JACK is required by this test but unavailable: ... LoadLibraryExW failed.
Start the backend (CI runs `jackd -d dummy`), or set
SHOOP_ALLOW_MISSING_BACKENDS=1 to skip backend-dependent tests.
```

Because Cargo stopped at this binary, later integration binaries in the package were not run by this invocation. This is a known environment limitation, not a composite test failure.

### 2. Engine baseline with documented unavailable-backend opt-out

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_engine --features app_backend
```

**Result: PASS (exit 0).** Cargo build setup completed in `2.35s`; 672 tests passed across unit/integration binaries, 0 failed, and doc tests passed (0 tests). The four JACK tests report `ok` because the test helper permits unavailable backends under this environment variable. Notable coverage already present:

- 536 library unit tests;
- 16 `tests/no_alloc.rs` tests;
- primitive-loop POI, transition, wrap, command-queue, state-publication, and graph tests;
- no composite-loop engine tests, because composites do not yet exist in `shoop_engine`.

This opt-out run verifies the remainder of the package but does not claim live JACK coverage.

### 3. Required application build before QML tests

```sh
cargo build
```

**Result: PASS (exit 0).** Clean/incremental dependency build completed in `7m 47s` and produced `target/debug/shoopdaloop.exe` plus the Windows development launcher.

### 4. Plan's Unix QML launcher spelling on this Windows target

```sh
target/debug/shoopdaloop_dev.sh --self-test
```

**Result: NOT RUN (exit 127).** The generated launcher is platform-specific; `target/debug/shoopdaloop_dev.sh` does not exist on this Windows build. The equivalent generated file is `target/debug/shoopdaloop_dev.bat`. This is a command-path environment difference, not an application/test failure.

### 5. Platform-equivalent frontend/QML self-test

```sh
./target/debug/shoopdaloop_dev.bat --self-test
```

**Result: PASS (exit 0).** Overall totals:

```text
Testcases: 187
Passed:    187
Failed:    0
Skipped:   0
```

All 26 QML test files reported PASS. Composite-relevant surfaces included:

- `tst_CompositeLoop_running.qml`: 24/24 tests passed;
- `tst_Session_save_load.qml`: 5/5 passed, including composition descriptors/reference restoration;
- `tst_TwoLoops.qml`: 6/6 passed, including basic/composite conversion restrictions;
- `tst_LuaEngine_SessionControlHandler.qml`: 43/43 passed, including `loop_compose_add_to_end` and generic loop controls.

The 24 focused composite cases were sequential scheduling, record then play, parallel scheduling, scripts, countdown, GUI stall, blocking file-I/O stall, regular-to-script nesting, script-to-regular nesting, direct cycle rejection, converting a scheduled empty loop, self-cycle rejection, six immediate-sync variants, empty-sync grab, three synchronized grab variants, and two unsynchronized grab variants.

Expected warnings/errors observed while passing:

- self/direct circular schedules were rejected and logged;
- empty-sync composite grab was ignored and logged;
- real JACK was unavailable, while mock/virtual JACK-facing QML tests still passed.

## Relevant timing observations and baseline limitations

1. Focused tests use a 100-sample sync loop for most deterministic scenarios and assert states at 50-sample mid-cycle positions. They test user-visible sequencing but do not assert that child transitions happened on an exact engine sample.
2. `tst_CompositeLoop_running.qml::process` intentionally splits frame requests. Its comment states cycle detection fails when processing wraps to exactly the same position; splitting is a workaround. This is direct evidence that current cycle authority depends on frontend polling/partitioning.
3. `test_ui_frozen` and `test_fileio_frozen` use 12,000-sample cycles and process 42,000 frames asynchronously while the GUI thread blocks. They prove independence from the GUI thread, but not from the separate engine update thread where the composite `QObject` runs.
4. Primitive wrapper cycle detection compares the newly polled position with the previous position and increments by at most one. Multiple wraps between updates are not represented.
5. Existing composite tests exercise sync-boundary states, nesting, seek, and grab marker alignment. They do not cover engine callback partition independence, same-sample target conflicts, transitive cycles longer than two, command acceptance races, plan replacement states, stale generations, bounded overflow, or RT allocation/locking for composites.
6. The 16 existing no-allocation tests cover current engine paths only. Since the current composite state machine is not in the engine callback, they provide no evidence for composite RT safety.

## Known pre-existing failures

| Failure | Classification | Reproduction | Impact on baseline |
|---|---|---|---|
| Four `jack_app_backend` tests fail because JACK DLL/backend is unavailable | Environment limitation | Required engine command without `SHOOP_ALLOW_MISSING_BACKENDS=1` | Required command is red on this host; remainder passes with the documented opt-out. No composite test is implicated. |
| Unix `.sh` development launcher absent | Platform command-path difference | Plan command exactly as written | Use generated `.bat` launcher on Windows; equivalent QML suite passes. |

No other failing, ignored, or skipped test was observed.

## Stage 0 contract-test addition

After recording the pre-change baseline, Stage 0 added executable semantic decisions without adding a composite runtime:

```sh
cargo test -p shoop_engine composite_semantics --features app_backend
```

**Result: PASS.** 19 semantic contract tests passed, 0 failed. They cover half-open boundaries, coincident ordering/conflicts, stop suppression, nesting, deterministic DAG/cycle behavior, duration/mode/record/pass/seek rules, plan activation, stale identities, capacity dispositions, and command/timestamp acceptance. See [SEMANTICS.md](SEMANTICS.md) for the requirement-to-test map.

Stage 0 completion was also checked with:

```sh
cargo fmt --all
RUSTFLAGS="-D warnings" cargo build
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo test -p shoop_engine --features app_backend
```

Formatting completed successfully, the warnings-denied application build passed in `8m 44s`, and the final engine run passed 691 tests (the 672-test baseline plus 19 semantic contract tests), 0 failed. The unavailable-JACK qualification above remains applicable.
