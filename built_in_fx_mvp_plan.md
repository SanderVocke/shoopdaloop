# Built-in FX MVP implementation plan

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised only for a documented, well-supported reason.
- Goals and acceptance criteria must not change without explicit user approval.
- Before every commit containing Rust changes, run `cargo fmt --all`, build with `RUSTFLAGS="-D warnings"`, and run the targeted tests for that stage. When Rust tests change, also run `python3 scripts/check_shoop_test_usage.py`.
- Before pushing behavior changes, run the complete local validation gates in Stage 7.

## Goals

- Add a first-class **Built-in FX** audio processor backed by the `fundsp` crate.
- Deliver an extensible fixed-rack foundation whose MVP contains one on/off stereo reverb.
- Support the same native, in-process, browser Worker, and AudioWorklet paths as other built-in processors.
- Persist and transactionally restore Built-in FX state in sessions.
- Provide an embedded editor with visible, clickable FunDSP attribution.

## Scope

### In scope

- A stable `builtin_fx` processor identity and **Built-in FX** display label.
- Fixed two-channel audio input and two-channel audio output topology with no MIDI.
- A fixed reverb configuration and one persisted `reverb_enabled` control.
- Realtime-safe native and Wasm processing, backend catalogs, protocol transport, UI, session persistence, documentation, and tests.
- Branch push, pull request creation, green CI, and resolution of actionable automated review feedback.

### Out of scope

- Additional effects, effect reordering, variable rack topology, presets, MIDI Learn, automation, or reverb parameter controls.
- Serialization of effect tails or FunDSP implementation internals.
- Refactoring or replacing the existing port-insert `fx_chain::FxChain` prototype unless a narrowly necessary conflict is discovered and documented.
- External plugin hosting changes.

## Immutable acceptance criteria

1. Native and browser processor catalogs advertise an available processor with stable ID `builtin_fx`, label **Built-in FX**, exactly two dry audio inputs, exactly two wet audio outputs, no MIDI, persistent state, and an embedded editor.
2. With reverb enabled, non-silent stereo input produces deterministic stereo post-effect output with an observable reverb tail at supported sample rates and callback sizes.
3. With reverb disabled, output is a transparent channel-for-channel copy of input; steady-state processing does not invoke FunDSP reverb DSP, allocate, lock, rebuild a graph, or advance a hidden reverb tail.
4. Disabling reverb discards its existing tail so re-enabling cannot resume stale audio. The one-time state transition may perform bounded reset work; subsequent disabled blocks remain on the cheap passthrough path.
5. FunDSP graph construction, sample-rate setup, reset, and allocation occur outside steady-state audio processing. Enabled and disabled steady-state processing are allocation-free, and buffers larger than FunDSP's supported block size are handled safely.
6. The processor works through shared engine routing for native dummy/offline, JACK, CPAL, browser Worker, and AudioWorklet operation without backend-specific DSP implementations.
7. The reverb enabled state survives session save/reload, native/browser transfer, and sample-rate/backend replacement. Malformed or unsupported Built-in FX state is rejected before publishing a replacement session. Reverb tails and editor visibility remain transient.
8. The embedded editor exposes the reverb toggle and clearly attributes FunDSP with a working link to its project page.
9. Existing session versions accepted before this feature continue to load with their existing semantics; sessions containing Built-in FX use a documented, strictly validated current format.
10. Relevant unit, backend, protocol, application, UI, native, and Wasm tests pass; the full local gates and all required GitHub PR checks are green on the final pushed commit, with no unresolved actionable automated review findings.

## Design rules and constraints

- Add `fundsp` as a pinned workspace dependency with `default-features = false` and only the minimal `std` feature; do not pull file decoding or FFT convolution into the worklet.
- Implement a dedicated callback-owned `BuiltInFxProcessor` and control/state codec in `shoop_engine`, following the prepared-processor ownership pattern used by other built-ins. Do not put the new rack on individual audio ports.
- Keep fixed rack constants and dry/wet blend explicit and deterministic. The rack's post-effect output must retain the source signal while enabled; the exact fixed blend is an implementation constant, not a user control in this MVP.
- Make the disabled branch explicit around the FunDSP call. Use instrumentation or a test-visible counter/seam so tests prove that disabled blocks do not enter reverb processing rather than relying on timing assertions.
- Reset/discard the reverb tail once when transitioning from enabled to disabled. Do not process tails while disabled.
- Preallocate processor output/adaptation storage for the backend's maximum callback size. Split processing into legal FunDSP block sizes without constructing buffers or graphs in the callback.
- Use strict, canonical, versioned Shoop-owned state, for example `shoop-builtin-fx:1:<reverb-enabled>`. Persist only rack controls, never an opaque FunDSP object or tail buffers.
- Add a dedicated session topology/chain identity instead of representing Built-in FX as Carla or OxiSynth. Bump the session document version and preserve migrations from all currently accepted versions.
- Keep controls and snapshots typed across `shoop_app_api`, backend interfaces, and the worklet protocol. The reverb toggle should supersede older queued values for the same parameter.
- Reuse the generic dry/wet routing and active lifecycle. Generic processor inactivity must also avoid FunDSP processing.
- Keep the DSP implementation target-independent. Native adapters and browser adapters may translate topology/control/state, but must not duplicate audio algorithms.
- Add a detailed realtime tracing span only around actual enabled FunDSP processing and update tracing inventory metadata for any new instrumented source.
- Follow existing transactional session staging: decode and prepare the processor at the destination sample rate and buffer size before replacing live state.
- Use FunDSP's MIT OR Apache-2.0 dependency consistently with repository licensing, update `Cargo.lock`, and provide the requested in-product attribution without bundling unnecessary assets.

## Stage 0 — Baseline and dependency contract

Baseline evidence (2026-09-01): branch `shoopdaloop-fundsp` at `f8196bb17`; the only initial worktree item was this untracked plan. Existing OxiSynth coverage passed 18 native library tests and the same 18 Node Wasm tests. The pre-change worklet compiled for `wasm32-unknown-unknown`. The first broad nextest attempt overbuilt unrelated integration targets and hit concurrent `ld.lld` bus errors; bounded two-job focused commands passed and are the local execution policy for subsequent iteration.

Chosen MVP contract: FunDSP `reverb_stereo(10.0, 2.5, 0.5)`, enabled by default, with the dry stereo signal plus reverb scaled to `0.2`; disabling is exact passthrough and performs one reset to discard the tail. Canonical state is `shoop-builtin-fx:1:0` or `shoop-builtin-fx:1:1`.

- [x] Record the current branch, clean worktree, baseline targeted test results, and relevant processor/session/browser behavior in this plan.
- [x] Add the pinned minimal-feature `fundsp` workspace dependency and update `Cargo.lock`.
- [x] Compile a minimal stereo reverb graph for native and `wasm32-unknown-unknown`; confirm the concrete graph/unit is suitable for callback ownership and can be initialized with the runtime sample rate.
- [x] Inspect `cargo tree` for `shoop_audio_worklet` and confirm FunDSP file/FFT-convolution dependencies are absent and the worklet dependency policy still passes.
- [x] Document the chosen fixed reverb constants, enabled default, fixed blend, reset behavior, and canonical state string in this plan before implementing behavior.
- [x] Verify with native and Wasm `cargo check`, `python3 scripts/check_worklet_client_dependencies.py --target wasm32-unknown-unknown`, and focused dependency assertions.
- [x] Commit the dependency/baseline milestone.

## Stage 1 — Realtime engine processor

Depends on Stage 0.

- [x] Add a dedicated Built-in FX module containing validated control state, strict versioned encode/decode, prepared FunDSP stereo reverb, preallocated buffers, and enable/disable transition handling.
- [x] Add the processor backend variant and fixed audio routing to `Session`; copy stereo input to output while disabled and invoke chunked FunDSP processing only while enabled.
- [x] Add creation, replacement, activation, state capture/restore, and editor-state access through the engine application backend and FX-chain facade.
- [x] Ensure sample-rate or maximum-buffer changes create and allocate a replacement on the control/staging path before callback publication.
- [x] Add realtime instrumentation around enabled processing only and update tracing coverage metadata.
- [x] Test strict state validation, canonical round-trip, stereo processing/tail behavior, exact disabled passthrough, stale-tail removal, block chunking, sample-rate handling, generic inactivity, and no allocation in both steady-state branches.
- [x] Verify with focused `shoop_engine` native tests, focused `shoop_engine` Node Wasm tests, tracing coverage, formatting, warning-denying workspace build, and the Rust-test policy check.
- [x] Commit the engine milestone.

## Stage 2 — Application API and both backend implementations

Depends on Stage 1.

- [x] Add `builtin_fx` processor constants, descriptor/editor state, typed reverb control, action intent kind, and snapshot representation to `shoop_app_api`.
- [x] Advertise the fixed stereo/no-MIDI descriptor from both the in-process and native processor catalogs.
- [x] Extend in-process `EngineBackend` track creation, controls, optimistic state, snapshot capture, processor-state capture, staged session replacement, and sample-rate recreation.
- [x] Extend `NativeBackend` and its engine/app-backend facade with the same shape validation, controls, state capture, transactional restoration, and snapshots.
- [x] Reject Built-in FX controls on other processor types and reject invalid channel shapes before mutation.
- [x] Test both backend implementations for fixed ports, input-to-output rendering, enabled tail, disabled passthrough/no DSP, state capture/restore, failed replacement rollback, and sample-rate/backend switching.
- [x] Verify with focused `shoop_app_api`, `shoop_engine`, and `shoop_backend` native and Node Wasm tests plus the per-commit gates.
- [x] Commit the backend/API milestone.

## Stage 3 — Browser protocol, Worker, and AudioWorklet

Depends on Stage 2.

- [x] Add Built-in FX topology, reverb control, and editor/snapshot state to `shoop_audio_protocol`, including serialization and command-journal supersession tests.
- [x] Translate the new topology, state, and controls in both `shoop_audio_worklet` and `shoop_worklet_client`.
- [x] Advertise Built-in FX in the browser catalog and reserve/register its fixed stereo ports in deterministic order.
- [x] Exercise real input audio through the import-free worklet engine and verify enabled tail, disabled passthrough, state snapshots, and command application.
- [x] Cover Worker/client round trips and confirm unavailable native-only processors retain their current browser behavior.
- [x] Verify focused protocol/worklet/client native tests, Node Wasm tests, worklet import/dependency checks, and the per-commit gates. Targeted local Chromium execution was attempted but `chromedriver` is absent from the development shell; the policy-triggered PR Chromium job remains mandatory in Stage 9.
- [x] Commit the browser transport milestone.

## Stage 4 — Session and application persistence

Depends on Stages 2 and 3.

- [x] Add dedicated Built-in FX track topology and chain-type document variants with strict fixed-shape and chain/state consistency validation.
- [x] Bump the session document version; retain decoding and migration behavior for every version currently accepted before the bump.
- [x] Map Built-in FX among application topology, backend requests, captured session tracks, processor states, and recorded-take state where the generic stateful-processor contract applies.
- [x] Decode and prepare Built-in FX state during staged load before backend mutation; preserve the current session on malformed state, missing capability, or construction failure.
- [x] Test new-session save/reload, old-version compatibility, canonical state, malformed state, wrong chain/topology combinations, cross native/browser transfer, and sample-rate recreation.
- [x] Update `docs/session_format_v1.md` with the identity, fixed topology, state grammar, transient state, compatibility, and current document version.
- [x] Verify focused `shoop_session` and `shoop_app` native/Node Wasm tests plus the per-commit gates.
- [x] Commit the persistence milestone.

## Stage 5 — Embedded editor and attribution

Depends on Stage 2; land after Stage 4 so UI tests can exercise persistence.

- [x] Add a Built-in FX egui editor with a clearly labeled reverb toggle and state-driven rendering.
- [x] Add visible **Powered by FunDSP** attribution and a working link to `https://github.com/SamiPerttu/fundsp`, following the Built-in Synth information/editor interaction pattern.
- [x] Route the editor through each track widget and ensure closing/reopening affects only transient visibility, not the reverb setting.
- [x] Ensure the generic Add Track flow presents Built-in FX from native and browser catalogs with the fixed channel/MIDI constraints.
- [x] Add UI tests for opening/closing, toggle action emission and state reflection, attribution visibility/link action, and coexistence with Built-in Synth editors.
- [x] Update user documentation and processor descriptions in `src/rust/shoopdaloop/README.md`, `docs/source/concept.rst`, and `docs/source/usage.trackcontrols.rst`; build Sphinx with warnings denied.
- [x] Verify focused `shoop_egui` and `shoop_app` native/Node Wasm tests plus the per-commit gates.
- [x] Commit the UI/documentation milestone.

## Stage 6 — End-to-end feature coverage

Depends on Stages 1–5.

- [ ] Add or extend native application smoke coverage to create Built-in FX, feed stereo audio, observe reverb, disable it, verify transparent passthrough/no stale tail, save, reload, and confirm state.
- [ ] Extend browser smoke/runtime coverage with the same catalog, topology, control, processing, snapshot, and save/reload evidence through the production Worker/AudioWorklet protocol.
- [ ] Verify processor behavior with non-default sample rates and callback sizes, including sizes larger than one FunDSP processing block.
- [ ] Confirm generic inactive routing and element-disabled routing both skip the enabled FunDSP tracing span; use a targeted Perfetto capture only if ordinary deterministic evidence is insufficient.
- [ ] Review all new `match` arms and identity mappings across engine, native backend, in-process backend, application, session, protocol, worklet, client, and UI for accidental OxiSynth-only assumptions.
- [ ] Run focused native, Node, and Chromium end-to-end tests and commit the integration-test milestone.

## Stage 7 — Final local validation

Depends on all implementation stages. Run in the environment selected by `.agents/info/build.md`; on Nix/NixOS, run all payloads inside `nix develop`.

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `cargo check --locked --no-default-features -p shoopdaloop --target wasm32-unknown-unknown` and `cargo build --locked -p shoop_audio_worklet --target wasm32-unknown-unknown` (the UI's default native-driver features are intentionally disabled for the browser target).
- [ ] Run `python3 scripts/check_worklet_client_dependencies.py --target wasm32-unknown-unknown`, inspect the relevant `cargo tree`, and verify the generated worklet remains import-free through the existing contract check.
- [ ] Run `python3 scripts/run_wasm_tests.py --runtime node --profile dev`.
- [ ] Run `python3 scripts/run_wasm_tests.py --runtime chrome --profile dev` when Chrome is available; otherwise record the local limitation and require the corresponding PR matrix job to pass.
- [ ] Run `python3 -m unittest scripts.tests.test_wasm_test_report` and `python3 scripts/check_wasm_smoke_budget.py`.
- [ ] Run `trunk build` from `src/rust/shoopdaloop` and execute the applicable browser smoke commands documented there when a browser is available.
- [ ] Run `sphinx-build -W --keep-going docs/source _build`.
- [ ] Recheck every immutable acceptance criterion against test output and inspect `git diff --check` and `git status`.
- [ ] Commit any final validation-only corrections, rerun affected gates, and leave the implementation worktree clean.

## Stage 8 — Push and open the pull request

Depends on a clean Stage 7.

- [ ] Review the commit series for meaningful stage boundaries and ensure no generated `dist`, worklet, `_build`, trace, credential, or unrelated files are committed.
- [ ] Push the branch with `git push -u origin shoopdaloop-fundsp`.
- [ ] Open a PR against `master` using `gh pr create`, with a concise summary, immutable behavior contract, session-format note, FunDSP attribution/dependency details, and exact local validation evidence.
- [ ] Record the PR URL and final pushed head SHA in this plan.

## Stage 9 — CI and automated review closure

Depends on the open PR. Repeat until the latest pushed SHA is green and review-clean.

- [ ] Monitor required checks with `gh pr checks`/`gh run watch`; confirm Build and Test matrix, Rust coverage, Docs, and CodeQL complete for the latest head SHA.
- [ ] For failures, inspect the exact attempt, matrix job, logs, and artifacts with `gh run view`, `gh run download`, or `gh api`. Compare matrix peers before deciding whether a failure is deterministic or flaky; read the Perfetto skill before analyzing any `.pftrace` artifact.
- [ ] Reproduce deterministic failures locally where possible, add or improve a regression test, fix the root cause, rerun all affected local gates, commit the correction, and push it. Do not use blind reruns as a substitute for diagnosis.
- [ ] Query PR reviews, issue comments, and inline review comments with `gh pr view` and `gh api` after each review cycle so automated findings are not missed.
- [ ] Classify every automated finding. For a valid finding, add/adjust coverage, implement the fix, run affected and mandatory gates, commit, push, and reply with the fix SHA/evidence. For an invalid or already-covered finding, reply with concrete code/test evidence and resolve it where permissions allow.
- [ ] After each push, wait for replacement checks and automated reviewers to finish on the new SHA, then repeat the CI/review query.
- [ ] Finish only when all required checks are green on the latest SHA, there are no unresolved actionable automated findings, the PR is mergeable, and the worktree is clean. Record the final PR URL, head SHA, check summary, and review disposition in this plan.
