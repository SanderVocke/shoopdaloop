# Make realtime mutex acquisition explicit

Status: complete

Branch: `content-snapshots-code`

Base: `origin/master` at `996de36a`

## Goals

- Detect unapproved project-owned mutex acquisition from `shoop_engine` realtime sections, including uncontended acquisitions that never enter a futex syscall.
- Make every currently known realtime mutex site explicit and locally justified in source.
- Prevent new `std::sync::Mutex` use in `shoop_engine` from bypassing the detector.
- Extend realtime-section coverage from the engine cycle alone to the complete project-owned JACK, CPAL, and dummy callback plumbing.
- Preserve behavior when the developer guard is disabled.

## Scope

This includes a checked mutex abstraction for project-owned `shoop_engine` mutexes, realtime-section tracking, explicit temporary permissions for known callback lock debt, static enforcement, a developer CLI switch, and focused tests. It covers project-owned Rust callsites currently using `std::sync::Mutex`, including command-reachable sites.

This does not remove the known locks, redesign driver/capture/plugin data flow, interpose native synchronization APIs, or claim coverage of locks inside JACK, CPAL, midir, lilv, LV2 plugins, libc, or operating-system audio code. Optional futex/pthread tracing is follow-up diagnostic work, not part of this implementation.

## Acceptance criteria (immutable)

- [x] Every project-owned mutex declaration in production `shoop_engine` code uses the checked mutex abstraction; a structural check fails if production code imports, names, or directly acquires `std::sync::Mutex` outside the abstraction module.
- [x] Checked `lock()` and `try_lock()` attempts made in a marked realtime section are detected before acquisition, including uncontended attempts.
- [x] Realtime marking covers `Engine::process`, `Engine::run_cycle`, and `Engine::pump`, plus the complete project-owned JACK process callback, CPAL input/output callbacks, and dummy process iteration. Nested marking is correct and thread-local.
- [x] An unapproved realtime lock attempt is a deterministic guard violation in tests and terminates immediately in developer strict mode rather than blocking the callback.
- [x] Every known realtime lock site that remains is wrapped in a narrowly scoped, reason-labelled permission at the individual acquisition expression; there is no blanket permission around the command executor, whole engine cycle, whole driver callback, or arbitrary closure body.
- [x] The detector and permission path do not allocate, log, format, take another lock, or perform blocking I/O in realtime context.
- [x] Mutex poisoning behavior, guard lifetimes, `Condvar` interoperability, `Send`/`Sync` properties, and existing public engine behavior remain compatible.
- [x] The guard is disabled by default, can be enabled through a documented developer CLI option, and adds no lock-policy behavior change while disabled.
- [x] Focused tests cover disabled and enabled behavior, non-realtime acquisition, `lock`, `try_lock`, nested scopes, permission scope containment, thread locality, and representative engine/driver/capture/plugin lock sites.
- [x] Formatting, warning-free build, Rust tests, realtime no-allocation tests, and the QML self-test suite pass; a guard-enabled supported-backend test run reports no unapproved project-owned lock attempt.

## Design rules

- Detect at the Rust mutex API boundary, not at the futex boundary. Futex and pthread observation misses uncontended fast paths and is platform-dependent.
- Use a project-owned wrapper around `std::sync::Mutex` that delegates to the standard mutex and returns standard guards/results where possible. Do not introduce a different locking algorithm.
- Check a cheap global enable flag before consulting constant-initialized thread-local realtime and permission depths. Disabled control-thread mutex use must remain cheap.
- Realtime and permission scopes must be panic-safe RAII scopes and support nesting without leaking state into later work.
- The strict violation path must be realtime-safe. Human-readable diagnostics may use static site metadata and postmortem/control-thread reporting, but must not format or print from the callback.
- Permission labels are literals describing why the lock remains. Permission wraps only the acquisition; code executed while holding the guard is not placed inside a broad detector bypass.
- Migrate all project-owned mutexes, including control-only mutexes, so a future call-graph change cannot silently move an uninstrumented mutex into realtime context.
- Explicitly inventory command-reachable lock paths. The allocation permission around command execution must not imply lock permission.
- Treat third-party and native locking as an explicit coverage boundary in code and documentation; do not imply allocator-guard-equivalent dependency coverage.
- Preserve existing source formatting and avoid unrelated lock removal or architectural changes in this work.

## Explicit permission baseline

The structural test fixes the initial production baseline at 34 individually labelled acquisitions:

- object creation failure publication: 1;
- JACK registered-port and decoupled MIDI queues: 3;
- CPAL capture rings, connection/endpoint registries, and decoupled MIDI queues: 9;
- dummy driver engine claim and iteration state: 5;
- deferred external audio/MIDI connection operations: 4;
- external audio capture: 4;
- external MIDI capture: 3;
- Carla host processing: 1;
- LV2 URID map/unmap operations: 4.

Changing this count requires an intentional test and inventory update. It is a debt baseline, not evidence that these locks are realtime-safe.

## Implementation plan

### Stage 1 — Establish the lock inventory and guard contract

- [x] Record the production `shoop_engine` mutex declarations and classify each acquisition as control-only, driver-callback, engine/session processing, capture, plugin/URID, or command-reachable.
- [x] Record the initial explicit realtime permission inventory, including JACK port/decoupled MIDI locks, CPAL rings/registries/endpoints/queues, dummy driver state, external capture, Carla/URID locks, object failure publication, and queued external-connection operations.
- [x] Add focused contract tests for realtime scope nesting, thread locality, unapproved acquisition, exact permission scope, and disabled behavior before migrating production callsites.
- [x] Verify the focused tests expose an unapproved synthetic mutex acquisition and do not classify ordinary control-thread acquisition as realtime; commit the stage.

### Stage 2 — Add the checked mutex and realtime lock guard

- [x] Implement the checked mutex abstraction with compatible construction, `lock`, `try_lock`, mutable access, ownership extraction, poisoning, and debug behavior required by current users.
- [x] Implement allocation-free global enablement and thread-local realtime/permission RAII scopes.
- [x] Implement a strict unapproved-lock violation path and a test-observable policy that does not weaken production strict behavior.
- [x] Add a reason-labelled permission macro or method whose scope covers only one acquisition expression.
- [x] Test `lock`, `try_lock`, poisoning, nested scopes, permission non-leakage, cross-thread isolation, and `Condvar` use; build warning-free and commit the stage.

### Stage 3 — Migrate and statically enforce project-owned mutex use

- [x] Replace every production `shoop_engine` `std::sync::Mutex` declaration/import with the checked abstraction, including control-only and feature-gated LV2 code.
- [x] Preserve public signatures or migrate all workspace callers where a mutex-bearing public engine type crosses a module or crate boundary.
- [x] Add a crate-local structural test or lint that permits direct `std::sync::Mutex` use only inside the abstraction implementation and catches both declarations and direct `lock`/`try_lock` bypasses.
- [x] Run package tests with default features and `app_backend`, plus a warning-free build, before committing the mechanical migration.

### Stage 4 — Mark complete realtime boundaries and annotate existing debt

- [x] Mark engine `process`, `run_cycle`, and `pump` sections while preserving correct nested behavior under driver-level scopes.
- [x] Mark the full JACK process callback and annotate each retained JACK registered-port and decoupled-MIDI acquisition individually.
- [x] Mark CPAL input and output callbacks and annotate each retained connection registry, capture ring, endpoint registry, and decoupled queue acquisition individually.
- [x] Mark the dummy process iteration and annotate each retained driver-state acquisition individually.
- [x] Annotate retained processing locks in external audio/MIDI capture, Carla host processing, and URID mapping individually.
- [x] Audit queued commands for lock acquisition and annotate only the individual retained sites, including failure publication and deferred external connection operations; do not permit the command executor as a whole.
- [x] Add representative tests proving each callback family accepts its explicit baseline while an added unlabelled acquisition fails. Use mocks/structural coverage where hardware callbacks are unavailable.
- [x] Verify guard code itself remains allocation-free under the existing allocator test harness; commit the stage.

### Stage 5 — Expose and document developer operation

- [x] Add a disabled-by-default `--rt-lock-guard` developer option and enable the engine guard during application startup alongside the allocation guard.
- [x] Document that the guard covers project-owned checked mutexes and that dependency/native locks require separate OS-level diagnostics.
- [x] Ensure strict violations retain static callsite/reason evidence suitable for crash diagnosis without realtime logging.
- [x] Add CLI parsing/startup tests and a supported mock/dummy run with the guard enabled; commit the stage.

### Stage 6 — End-to-end validation

- [x] Run `cargo fmt --all -- --check` and `git diff --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build`.
- [x] Run focused checked-mutex, callback-boundary, structural enforcement, and no-allocation tests.
- [x] Run `cargo test --workspace --features shoop_engine/app_backend`, using serialized execution and the documented missing-backend allowance if required by the environment.
- [x] Build and run `SHOOP_ALLOW_MISSING_BACKENDS=1 target/debug/shoopdaloop_dev.sh --self-test`.
- [x] Run the supported self-test or focused driver suite with `--rt-lock-guard` enabled and verify that only the committed explicit permission baseline is exercised and no unapproved lock terminates the run.
- [x] Re-audit every acceptance criterion against code and test evidence, document any unavailable hardware-only verification, commit final evidence, and mark the plan complete.

## Validation evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- `RUSTFLAGS="-D warnings" cargo build` passed.
- Focused checked-mutex unit tests passed, including detection, nesting, permission containment, thread locality, poisoning, mutable/owned access, and `Condvar` interoperability.
- Focused structural, no-allocation, dummy callback, and CLI tests passed. The structural baseline is 34 production permission expressions; the no-allocation suite passed all 22 tests.
- The serialized workspace test run with `shoop_engine/app_backend` passed.
- The normal and guard-enabled QML self-test runs passed with 197 testcases: 196 passed and one CPAL testcase skipped because this environment has no CPAL settings. The guard-enabled run completed without an unapproved project-owned lock attempt.
- Five additional guard-enabled focused CPAL self-test launches passed and consistently reported the same environment-driven skip.
- Hardware CPAL callbacks and a physical JACK server were unavailable. CPAL/JACK callback-boundary and permission placement are therefore covered structurally; the repository's `JackTest` QML test and the supported dummy backend passed, but no claim is made about dependency/native locks.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
