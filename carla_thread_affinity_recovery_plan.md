# Carla thread-affinity and recovery plan

## Goals

- Honor Carla Native UI thread-affinity requirements in both in-process and subprocess hosting.
- Keep Carla UI lifecycle/event work independent from DSP block processing so a slow UI call cannot stall the DSP worker by host design.
- Recover automatically from processing deadline misses and stale completions without falsely reporting a plugin crash or requiring an FX-button click.
- Present distinct, actionable UI states for healthy, temporarily degraded/recovering, restarting, crashed, stopped, and unavailable processors.

## Scope

- Carla Native host ownership and callback dispatch in `shoop_engine`.
- In-process and subprocess Carla execution paths and their shared-memory transports.
- Native backend propagation of processor lifecycle and health.
- Public FX state and the egui FX button/tooltip behavior.
- Deterministic fake-host tests, real-Carla UI probes, tracing, and user-facing documentation needed to verify the changes.

## Out of scope

- Passing dry audio through when a Carla deadline is missed; existing bounded silent fallback may remain.
- Perfetto buffer sizing or streaming changes.
- Changes to built-in FX, OxiSynth DSP, session document semantics, or third-party plugin behavior beyond what is needed to preserve existing compatibility.

## Immutable acceptance criteria

1. For Carla v2.5.10 descriptors carrying `NATIVE_PLUGIN_NEEDS_UI_MAIN_THREAD`, `ui_show`, `ui_idle`, and UI teardown execute on the main/UI thread that owns the native application or Carla worker event loop, never on the audio callback or DSP worker.
2. Carla `process` executes on a dedicated DSP path and is not serialized behind `ui_show` or `ui_idle` by a Shoop-owned blocking mutex. A deliberately slow fake UI can open, idle, close, and reopen while DSP blocks continue to complete.
3. UI idling is scheduled at a bounded UI cadence rather than once per audio block.
4. `DeadlineMiss` and `StaleCompletion` are recoverable transport outcomes in both hosting modes: obsolete work is discarded, transport capacity is reclaimed or resynchronized, subsequent blocks can succeed, and no user click is required.
5. Recoverable deadline/stale events never set the processor lifecycle to `Crashed`. Unrecoverable protocol errors, worker exits, and genuine plugin failures still do.
6. The application exposes a distinct degraded/recovering FX state with useful deadline/stale health information. The FX control renders it amber/yellow with an automatic-recovery explanation; actual crashes remain red with their crash summary; running-active remains green and running-inactive/stopped remains gray.
7. Visibility toggling and crash recovery have explicit semantics: ordinary healthy/degraded interaction does not masquerade as crash recovery, while a genuinely crashed recoverable processor still offers a recovery action.
8. Existing Carla state save/restore, activation, MIDI/audio processing, generation logs, supervised subprocess restart behavior, and session loading remain functional.
9. Deadline fallback does not add dry-signal bypass behavior.

## Design rules and constraints

- Treat the pinned Carla v2.5.10 descriptor flags as an ABI contract. Define and validate `NATIVE_PLUGIN_NEEDS_UI_MAIN_THREAD`; do not silently ignore it.
- Model native-instance ownership explicitly: UI callbacks, DSP buffers/callbacks, between-block control/state operations, and final cleanup must each have a documented thread and lifetime.
- Stop and join/quiesce DSP before hiding/destroying the native instance; cleanup must run exactly once on an allowed thread.
- Use bounded queues, atomics, immutable snapshots, or another realtime-safe handoff between UI/control and DSP. Do not add allocation, blocking locks, file I/O, or unbounded waits to the realtime path.
- Keep application/worker main-thread pumping non-blocking from the audio path. Synchronous UI calls may delay the GUI action that requested them, but must not own a lock required by DSP.
- Serialize activation and state mutation at safe block boundaries where Carla requires exclusion; do not assume every non-UI callback may race with `process`.
- Use one shared lifecycle/health state machine for in-process and subprocess hosting where practical. Hosting-mode differences must not produce different meanings for deadline, degraded, or crashed.
- A single successful post-failure block returns `Degraded` to `Running`; persistent fallback remains `Degraded` until success or an unrecoverable failure. Counters remain cumulative.
- Keep trace events/counters bounded and realtime-safe. Add enough UI and recovery instrumentation to correlate show/idle, misses, resynchronization, and lifecycle transitions without requiring detailed tracing.
- Avoid unrelated refactors or formatting churn.

## Stage 0 — Establish the work branch and executable baseline

- [x] Confirm the current `carlabugs` branch is the dedicated implementation branch; create/switch to a dedicated branch first if it is not suitable.
- [x] Record the current in-process and subprocess ownership/callback paths, descriptor hints, transport slot state machine, and lifecycle transitions in implementation notes or focused test names.
- [x] Add or preserve a minimal reproduction procedure covering continuous audio/MIDI processing while opening, closing, and reopening Carla UI.
- [x] Run the existing focused Carla bridge, subprocess, backend, and worker-entry tests to establish the baseline.
- [x] Commit the baseline tests/notes if they introduce a meaningful standalone milestone.

Baseline evidence: the trace investigation and this plan record the two current ownership paths and the continuous-processing UI reproduction. On `carlabugs`, 11 focused `shoop_engine` Carla/shared-memory tests and the fake `shoopdaloop::carla_worker_entry` test passed before behavior changes.

Verification:

- Focused Carla tests pass before behavior changes.
- The reproduction distinguishes UI delay, recoverable deadline/stale completion, unrecoverable protocol failure, and worker exit.

## Stage 1 — Encode the Carla threading contract

- [x] Add the missing native UI-main-thread hint and validate the pinned Rack, Patchbay, and Patchbay16 descriptors against their required UI contract.
- [x] Refactor the native host into explicit lifetime-safe facets: a main-thread UI endpoint, a DSP endpoint with audio/MIDI storage, and a control/state path serialized at block boundaries.
- [x] Add a main-thread Carla UI service/dispatcher owned and pumped by the native application runtime; integrate an equivalent owner into the Carla subprocess main loop.
- [x] Route `ui_show`, bounded-cadence `ui_idle`, UI-close observation, and UI teardown exclusively through those main-thread owners.
- [x] Route `process` exclusively through DSP endpoints and remove the Shoop-owned UI/DSP mutex serialization in the subprocess worker.
- [x] Define shutdown ordering that quiesces DSP, unregisters UI pumping, hides the UI if needed, and cleans up the descriptor exactly once.

Verification:

- Fake descriptors record thread identities and prove all UI callbacks run on the registered main/UI thread while `process` runs elsewhere.
- A fake `ui_show`/`ui_idle` delay does not stop successful DSP completions.
- Rapid show/hide and shutdown tests detect no use-after-free, duplicate cleanup, deadlock, or leaked worker.
- Run `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace` in the repository development shell before committing the stage.

Stage evidence: the main-thread UI service tests cover show/idle/cleanup affinity and cadence; the slow external-UI bridge test proves DSP progresses independently. All 29 Carla/shared-memory engine tests and the worker-entry test passed, test-attribute policy passed, formatting was applied, and the warning-denying workspace build passed.

## Stage 2 — Make deadline and stale completion recovery robust

- [x] Classify `DeadlineMiss` and `StaleCompletion` as recoverable in the in-process bridge, matching the subprocess policy.
- [x] Make cancellation/resynchronization explicit so late workers cannot leave occupied slots that force all future submissions to fail.
- [x] Continue accepting the next sequence after recoverable failure; prevent stale output from being published for a newer block.
- [x] Preserve bounded silent output for failed blocks without adding dry bypass.
- [x] Keep unrecoverable channel-layout, protocol, panic, and process-exit failures on the crash/unavailable path.
- [x] Add counters and coarse trace events for deadline miss, stale completion, resynchronization, recovery success, and unrecoverable failure.

Verification:

- Deterministic tests force wait timeout, late completion, full-slot pressure, and a later successful block in both hosting modes.
- Tests prove no manual `ToggleOrRecover` is needed, slot occupancy returns to a usable state, sequence ordering is preserved, and lifecycle does not become `Crashed`.
- Separate tests prove genuine protocol faults still become `Crashed` and supervised worker exits still follow restart policy.
- Realtime allocation/lock guards and existing no-allocation/no-standard-mutex checks remain green.
- Run `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace` in the repository development shell before committing the stage.

Stage evidence: deterministic in-process and subprocess delay-once tests observe silent fallback, `Degraded`, later successful output, return to `Running`, cumulative misses, and no recovery click. Existing shared-slot timeout/out-of-order tests verify abandoned-slot reclamation and stale-output rejection; panic and supervisor tests retain crash/restart behavior. Coarse deadline, stale, and recovery counters are registered in native tracing.

## Stage 3 — Introduce accurate lifecycle and health states

- [x] Add a `Degraded` lifecycle/state through engine, backend, application API, snapshots, fake backend, and tests.
- [x] Publish cumulative deadline-miss/stale-completion health and a non-crash status summary without overloading `crash_summary`.
- [x] Implement deterministic transitions: `Running` to `Degraded` on recoverable fallback, `Degraded` to `Running` on the next successful block, restart states only for actual restart activity, and `Crashed` only for unrecoverable failure.
- [x] Split visibility toggle from explicit crash recovery in backend/UI actions so button behavior follows lifecycle rather than one ambiguous command.
- [x] Render degraded/recovering amber or yellow with an automatic-retry tooltip and health counters; retain green, gray, yellow restart/startup, and red crash/unavailable meanings.
- [x] Update accessibility/hover text and focused widget tests for every lifecycle, active state, and available action.

Verification:

- State-machine tests cover every allowed transition and reject false crash/restart transitions.
- Backend snapshot tests preserve the new health fields and summaries.
- egui tests verify colors, tooltip content, enabled actions, visibility toggling, and explicit recovery behavior.
- Existing non-Carla processor rendering and control tests remain unchanged in behavior.
- Run `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace` in the repository development shell before committing the stage.

Stage evidence: engine and public lifecycle enums carry `Degraded`; native snapshots include cumulative deadline/stale counts and a separate status summary. Focused backend and egui tests verify mapping, yellow rendering, automatic-retry text, visibility action for degraded state, and explicit recovery for crashes.

## Stage 4 — Strengthen real-runtime probes and documentation

- [x] Extend the Carla UI probe to process and validate continuous audio/MIDI while opening, idling, hiding, reopening, and closing each bundled Carla UI.
- [x] Assert callback thread identities where observable, no permanent deadline-fault state, successful post-UI blocks, and clean shutdown.
- [x] Run the probe for both in-process and subprocess modes where the real runtime and display are available.
- [x] Document the supported Carla callback threading model, degraded/recovery semantics, hosting modes, and diagnostic counters for maintainers and users.

Verification:

- The fake probe is deterministic in automated tests.
- The real-runtime probe passes repeatedly on the development system without FX turning red or requiring manual recovery.
- A coarse Perfetto capture contains usable UI/recovery events and no unexplained callback discontinuity during the probe; tracing overhead is called out separately from correctness.
- Run `cargo fmt --all -- --check` and `RUSTFLAGS="-D warnings" cargo build --workspace` in the repository development shell before committing the stage.

Stage evidence: the continuous-DSP real Carla UI probe passed for all three descriptors in-process, and the opted-in real subprocess UI/process test passed. A 7.947-second coarse trace (`traces/0001-application.pftrace`) had no data-loss/error stats and showed 12 UI show/hide slices plus 120 UI idle slices on `shoopdaloop`, while 7,240 processor slices ran on `carla-ui-smoke-`. The README documents affinity, degradation, recovery, counters, and probe behavior.

## Stage 5 — Final end-to-end validation

- [x] Run `python3 scripts/check_shoop_test_usage.py` because Rust tests are changed.
- [x] Run focused Carla native, bridge, shared-memory, subprocess supervisor, backend, app, egui, worker-entry, realtime-allocation, and lock-safety tests.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Run the complete Rust suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [x] With real Carla/JACK/display facilities, repeat the original gain-effect and MIDI-piano scenarios in both hosting modes, including repeated UI open/close cycles, and verify uninterrupted processing, automatic recovery, and truthful UI states.
- [x] Capture a short coarse Perfetto trace for the final reproduction and verify no data loss, UI callbacks on their intended track, continued DSP completions, and expected lifecycle transitions.
- [x] Confirm no deadline-fallback dry bypass was introduced.

Validation evidence: test attribute policy, formatting, warning-denying workspace build, and tracing coverage all passed. The final complete CI-profile Rust run passed all 1,713 executed tests (four skipped). Real in-process and subprocess Carla UI/DSP probes passed on this system; the internal Audio Gain fixture covers the effect path, and opted-in in-process plus subprocess regressions loaded the system MDA ePiano, opened/closed Carla UI, injected MIDI, and verified audible-level generated samples without a crash or manual recovery. The final coarse trace had no loss/error stats and separated all UI callbacks from 7,240 DSP slices. Inspection confirms both deadline branches still zero wet/MIDI output and do not copy dry input.

## Stage 6 — Delivery

- [ ] Review the complete diff for scope, unsafe-code justification, shutdown ordering, realtime guarantees, and platform-specific main-thread behavior.
- [ ] Ensure every completed stage or meaningful milestone has a focused commit.
- [ ] Push the branch only after the behavior-affecting test suites are green.
- [ ] Open a PR summarizing the Carla v2.5.10 contract, trace evidence, architecture change, recovery state machine, UI behavior, and verification evidence.
- [ ] Monitor CI and fix failures until all required jobs are green.
- [ ] Check repeatedly for automated review feedback, address defensible findings with tests, push updates, and continue until the automated reviewer approves or no actionable feedback remains.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
