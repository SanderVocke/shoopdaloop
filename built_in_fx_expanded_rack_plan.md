# Expanded Built-in FX rack implementation plan

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Before every commit containing Rust changes, run `cargo fmt --all`, build with `RUSTFLAGS="-D warnings"`, and run the targeted tests for that stage. When Rust tests change, also run `python3 scripts/check_shoop_test_usage.py`.
- Before pushing behavior changes, run the complete local validation gates in Stage 9.
- Run Cargo, Python, Trunk, documentation, and test commands in the environment selected by `.agents/info/build.md`; on Nix/NixOS, run them inside `nix develop`.

## Goals

- Expand **Built-in FX** from one fixed stereo reverb into a comprehensive but bounded FunDSP-backed fixed rack: compressor, drive, three-band EQ, chorus, modulation, and reverb.
- Support matching mono, stereo, and general N-channel audio topologies, using dedicated stereo behavior only when the processor has exactly two audio channels.
- Give every stage a cheap bypass so disabled stages perform no effect DSP, while preserving exact passthrough when the whole rack is disabled.
- Mirror Built-in Synth MIDI connectivity and MIDI Learn for continuous Built-in FX parameters, including local track MIDI and the existing global FX-control MIDI fan-out.
- Preserve native, in-process, browser Worker, AudioWorklet, session, application, and embedded-editor support with strict compatibility and realtime guarantees.

## Scope

### In scope

- Stable processor identity `builtin_fx` and display label **Built-in FX**.
- Matching one-to-N dry audio inputs and wet audio outputs plus one required dry MIDI input and no MIDI output.
- Fixed rack order:
  1. Compressor
  2. Drive
  3. Three-band EQ
  4. Chorus
  5. Modulation
  6. Reverb
- Effect controls:
  - Compressor: enabled, Threshold, Ratio, Attack, Release, Makeup.
  - Drive: enabled, type (`Saturation`, `Overdrive`, `Distortion`, `Fuzz`), Drive, Tone, Mix, Output.
  - EQ: enabled, Low, Mid, High; fixed low-shelf, broad-mid-bell, and high-shelf frequencies/Q.
  - Chorus: enabled, Rate, Depth, Mix, Stereo Width.
  - Modulation: enabled, type (`Tremolo`, `Flanger`, `Phaser`), Rate, Depth, Mix, Feedback, Stereo Spread.
  - Reverb: enabled, type (`Room`, `Hall`, `Plate`), Amount, Tone.
- Absolute MIDI CC learning for all and only the continuous controls listed above, with no default assignments and a flat parameter list matching the Built-in Synth workflow.
- Versioned state/session migration, typed protocol and backend transport, editor controls, documentation, native/Wasm/browser tests, PR creation, green CI, and automated-review closure.

### Out of scope

- Effect reordering, arbitrary rack composition, multiple instances of one effect, presets, parameter automation lanes, relative encoders, default CC mappings, or MIDI mapping of toggles and selectors.
- MIDI note synthesis, MIDI output, forwarding ignored MIDI messages, or changing Built-in Synth MIDI behavior.
- Delay, gate, convolution, tempo synchronization, multiband dynamics, a gain-reduction meter, or serialization of DSP delay/tail buffers.
- Inferring surround layouts or stereo pairs for N-channel tracks.
- New third-party DSP dependencies unless a separately documented need is approved.

## Immutable acceptance criteria

1. Native and browser catalogs advertise `builtin_fx` as an available stateful processor with an embedded editor, matching 1..N dry/wet audio channels, one required MIDI input, and no MIDI output. Creation rejects zero, mismatched, or missing-MIDI shapes.
2. The processor implements the fixed stage order and exact stage/type/control set listed in Scope. New tracks default to the current Room reverb sound enabled at additive Amount `0.2` with neutral Tone; all other stages are disabled and MIDI assignments are empty.
3. Mono processing preserves one channel; exactly two channels use true-stereo reverb, stereo-linked compression, decorrelated chorus, and stereo modulation; N greater than two preserves channel count and isolation with independent mono processing, without guessed pairing or cross-channel leakage.
4. Every disabled stage skips its effect DSP and does not advance hidden state. When all stages are disabled, output is an exact channel-for-channel copy and no FunDSP/effect processing call occurs. Generic processor inactivity also runs no audio DSP.
5. Enabled effects are stable and observably respond to every control: compression reduces qualifying peaks with attack/release behavior; Drive types produce distinct bounded nonlinear responses; EQ bands boost/cut their fixed regions; Chorus and Modulation produce their intended time-varying effects; Reverb types produce distinct tails and Amount/Tone behavior.
6. Disabling or changing a tail/stateful effect discards the displaced state so it cannot reappear later. Only the selected Drive, Modulation, and Reverb type is processed.
7. Continuous control changes are validated, finite, bounded, reflected in snapshots, smoothed where discontinuities would click or zipper, and applied without allocation, locking, or graph construction in steady-state processing. Processor construction, type replacement, sample-rate setup, and buffer allocation occur on preparation/control paths.
8. Built-in FX mirrors Built-in Synth MIDI behavior: MIDI Learn inspects the latest local track-input message and permits assignment when it is a valid CC; learned local CC updates mapped continuous controls; existing global FX-control MIDI fan-out updates the same mappings; notes and all unsupported/non-CC traffic are ignored. Inactive audio DSP is not awakened by MIDI, and the existing bounded deferred-global-control behavior is retained.
9. MIDI Learn has no default assignments, uses absolute CC values with parameter-appropriate mappings, lists all continuous parameters in one flat selector, excludes toggles/types, enforces one source per parameter and one parameter per source as Built-in Synth does, and supports assign/remove/remove-all with persisted assignments.
10. Built-in FX state uses a strict canonical current format. Existing `shoop-builtin-fx:1:0|1` state and all currently accepted session document versions migrate transactionally: old stereo tracks retain their audio behavior, gain an unconnected MIDI input, receive new defaults, and preserve the old reverb-enabled value. Malformed state, topology, or assignments are rejected before publishing a replacement session.
11. The same target-independent processor and control model serves native dummy/offline, JACK, CPAL, browser Worker, and AudioWorklet paths. Backend and browser adapters translate topology/state/control only and do not duplicate DSP algorithms.
12. The embedded editor exposes all stages and controls, flat MIDI Learn, current learned assignments, and the existing clickable FunDSP attribution. UI state follows backend snapshots; editor visibility remains transient.
13. Relevant engine, backend, protocol, session, application, UI, native, Node Wasm, and browser tests cover 1/2/3/6-channel routing, DSP behavior, bypass/no-CPU behavior, MIDI, migration, save/reload, and realtime constraints. Full local gates and all required PR checks are green on the final pushed SHA, with no unresolved actionable automated-review findings.

## Design rules and constraints

- Keep the rack as independently bypassable stages rather than one monolithic typed FunDSP graph. Use two preallocated dynamic-channel ping-pong buffers and invoke only enabled stages.
- Keep all DSP target-independent under `shoop_engine`. Use FunDSP nodes where they map directly; use small fixed-storage stage implementations around FunDSP primitives when runtime-controlled stock constructors would otherwise require callback-time graph rebuilding.
- Use one mono processor bank for one or more non-stereo channels and a dedicated stereo implementation only when channel count is exactly two. For N > 2, use deterministic per-channel seeds where needed but never mix channels.
- Implement stereo-linked feed-forward peak compression with a fixed soft knee. Apply low/high shelves and a fixed broad mid bell independently per channel.
- Build Drive from bounded waveshapers, tone filtering, output compensation, and DC blocking. Evaluate FunDSP 2x oversampling in Stage 0 and retain it if native/Wasm cost and latency are bounded; oversampling is active only while Drive is enabled.
- Map Room/Hall/Plate to tuned FunDSP reverb families without claiming exact hardware emulation. Preserve dry gain at unity and treat Amount as additive wet gain so the version-1 default maps to `0.2`.
- Treat Feedback as retained but inactive in Tremolo mode. Stereo Width/Spread affect only exactly-two-channel processing; their stored values remain valid in other channel modes.
- Smooth continuous gain, filter, dynamics, and modulation parameters with fixed preallocated state. Type selectors and toggles are block-boundary controls and are not MIDI targets.
- Parse MIDI without allocation. Use only well-formed channel CC messages for learning/control. Ignore notes and unsupported traffic rather than forwarding it to another processor.
- Mirror Built-in Synth assignment semantics and inactive/global-control behavior. Reuse a narrowly scoped assignment helper or MIDI Learn UI component only where it reduces duplicated behavior without making public API, persistence, or wire types generic and harder to version.
- Define one typed Built-in FX continuous-parameter enum shared conceptually across engine/API/backend/wire/session translations. CC endpoint and midpoint tests must lock each linear, logarithmic, dB, or skewed mapping.
- Persist processor controls in a canonical ordered `shoop-builtin-fx:2` envelope using exact stable enum tags and finite numeric encoding. Persist Built-in FX CC assignments as concrete typed assignment documents beside processor state, analogous to Built-in Synth; do not serialize runtime filters, LFO phase, delay lines, or tails.
- Change Built-in FX topology documents and wire topology to carry audio channel count and the required MIDI shape. Bump both session document and browser protocol versions; explicitly migrate the prior fixed-stereo/no-MIDI session form.
- Keep snapshots, optimistic UI state, command supersession, replacement, and rollback typed for each individual control. A stale command for one parameter must not supersede a different parameter or assignment mutation.
- Preserve callback chunking for blocks larger than prepared processor storage and FunDSP's maximum block size. All enabled and disabled steady-state paths must remain allocation-free.
- Keep detailed realtime tracing around actual enabled stage processing only and update tracing inventories for new spans.
- Preserve the pinned minimal-feature FunDSP dependency and import-free AudioWorklet contract.

## Stage 0 — Baseline, branch, and DSP/control contract

- [ ] Ensure the completed Built-in FX MVP is present on the intended base branch. Prefer updated `master` after PR #844 merges; do not create a stacked implementation PR without explicit approval.
- [ ] Record base SHA, clean worktree, existing Built-in FX/OxiSynth focused test results, complete native/Node baselines, and current session/protocol versions in this plan.
- [ ] Create and publish a dedicated branch such as `builtin-fx-expanded-rack` from that base.
- [ ] Inventory the current Built-in FX control/state path through `shoop_engine`, `shoop_app_api`, `shoop_backend`, `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, `shoop_app`, `shoop_session`, and `shoop_egui`; record every fixed-stereo/no-MIDI assumption that must change.
- [ ] Prototype the proposed compressor, four Drive shapes, shelves/bell, chorus, modulation modes, and three reverb families for native and `wasm32-unknown-unknown`; verify channel I/O, sample-rate setup, reset, allocation, and mutable-control strategy without integrating behavior.
- [ ] Measure/inspect 2x Drive oversampling latency and bounded CPU on native and Wasm, then record the supported choice and any latency-reporting requirement.
- [ ] Record exact validated ranges, defaults, CC curves, smoothing constants, fixed EQ frequencies/Q, stereo modulation behavior, and FunDSP reverb mappings in this plan before production implementation.
- [ ] Decide the narrow reusable MIDI assignment/UI seams after comparing Built-in Synth engine and editor code; document why each extraction is beneficial or why concrete duplication is safer.
- [ ] Verify baseline formatting, warning-denying build, focused native/Node tests, locked Wasm checks, and worklet dependency/import contracts.
- [ ] Commit the baseline/prototype plan milestone without committing generated prototype artifacts.

## Stage 1 — Versioned state, parameters, and variable-channel rack foundation

Depends on Stage 0.

- [ ] Expand `shoop_engine/src/builtin_fx.rs` and focused submodules with typed stage states, effect type enums, the continuous-parameter enum, strict validation, defaults, and canonical version-2 encode/decode plus version-1 migration.
- [ ] Add Built-in FX CC assignment storage with Built-in Synth-equivalent uniqueness, validation, iteration, matching, assign/remove/clear behavior, and no default assignments.
- [ ] Add callback-visible runtime parameter publication so UI/backend and MIDI updates converge on one current state without locks; ensure captured/editor state observes MIDI-driven values.
- [ ] Replace fixed `[Vec<f32>; 2]` storage with prepared N-channel input/output and ping-pong storage, preserving legal FunDSP chunking and exact all-disabled passthrough.
- [ ] Add a fixed stage interface with explicit enable/reset/process behavior and test-visible processing counters; a disabled stage must never be called even when later stages are enabled.
- [ ] Implement block-boundary state transitions, parameter smoothers, sample-rate replacement, and reset semantics without callback allocation.
- [ ] Test strict/canonical state, version-1 migration, invalid/non-finite/out-of-range controls, assignment conflicts, CC curve endpoints/midpoints, 1/2/3/6-channel buffer access, chunking, exact bypass, and no-allocation control synchronization.
- [ ] Verify focused `shoop_engine` native/Node Wasm tests, formatting, warning-denying workspace build, tracing/test policies, and locked Wasm compilation.
- [ ] Commit the rack foundation milestone.

## Stage 2 — Dynamics, Drive, and EQ DSP

Depends on Stage 1.

- [ ] Implement the compressor with peak detection, fixed soft knee, attack/release smoothing, ratio/threshold gain computer, makeup gain, mono/N independent detectors, and exactly-stereo linked detection.
- [ ] Implement Saturation, Overdrive, Distortion, and Fuzz with shared Drive/Tone/Mix/Output controls, bounded finite output, DC blocking, and the Stage 0 oversampling decision.
- [ ] Implement the fixed-frequency three-band EQ with low shelf, broad mid bell, and high shelf for every channel.
- [ ] Ensure each stage and unselected Drive type performs zero DSP work while bypassed and resets bounded internal state on disable/type replacement where required.
- [ ] Add deterministic signal tests for compressor threshold/ratio/attack/release and stereo linking; Drive type distinction, harmonic/nonlinear response, DC/finite bounds and mix/output; EQ neutral response and per-band boost/cut selectivity.
- [ ] Cover mono, stereo, and 3/6-channel isolation at 44.1/48/96 kHz and callback sizes including 1, 128, 257, and larger-than-prepared blocks.
- [ ] Verify focused native/Node tests, allocation tests, tracing inventories, and per-commit gates.
- [ ] Commit the dynamics/Drive/EQ milestone.

## Stage 3 — Chorus, Modulation, and Reverb DSP

Depends on Stage 1; may proceed in parallel with Stage 2 only if edits remain non-overlapping.

- [ ] Implement Chorus Rate/Depth/Mix/Width with preallocated delay/modulation state, mono behavior, exactly-stereo decorrelation, and deterministic independent N-channel seeds.
- [ ] Implement Tremolo, Flanger, and Phaser behind one Modulation stage with shared Rate/Depth/Mix/Feedback/Spread controls; keep Feedback inactive in Tremolo and process only the selected mode.
- [ ] Implement Room, Hall, and Plate using the Stage 0 FunDSP mappings, additive Amount, wet Tone filtering, true-stereo processing for two channels, and mono wrappers for mono/N-channel banks.
- [ ] Reset chorus/modulation/reverb tails and displaced type state on disable or type change; prove re-enable cannot revive stale output.
- [ ] Add deterministic tests for modulation frequency/depth/mix, stereo width/spread, mode distinction, feedback bounds, reverb type distinction/tails/Amount/Tone, channel isolation, sample rates, callback sizes, and finite output under extreme valid controls.
- [ ] Prove bypassed stages and unselected types receive zero calls and all steady-state paths allocate nothing.
- [ ] Verify focused native/Node tests, tracing inventories, warning-denying builds, and per-commit gates.
- [ ] Commit the modulation/reverb milestone.

## Stage 4 — Engine routing and MIDI behavior

Depends on Stages 1–3.

- [ ] Extend `Session` Built-in FX routing from fixed stereo to matching N-channel ports and pass the combined track/global MIDI stream to the processor while preserving oversized-callback chunking and event order.
- [ ] Give Built-in FX a `process_midi_controls_only` path equivalent to Built-in Synth: local MIDI may update controls while processor audio is inactive; global controls retain the existing bounded deferred behavior and never wake inactive DSP.
- [ ] Apply learned absolute CC mappings to continuous parameters at the event/block boundary with smoothing; ignore notes, selectors/toggles, malformed CC, program changes, pressure, pitch bend, and other unsupported messages.
- [ ] Preserve local MIDI plus global fan-out ordering, saturation bounds, pending-control replacement, diagnostics, and no-recording behavior already established for built-in processors.
- [ ] Test local learning/control, global mapped control, local/global additive order, note/unsupported-message rejection, inactive behavior, bounded deferred global restoration, assignment changes, no default mapping, and no audio-stage calls caused only by MIDI.
- [ ] Test routed 1/2/3/6-channel input/output, exact bypass, stereo-specific behavior, generic inactivity, callback growth, and channel-count/port-shape rejection.
- [ ] Verify focused `shoop_engine` native/Node tests and per-commit gates.
- [ ] Commit the engine routing/MIDI milestone.

## Stage 5 — Application API and native/in-process backends

Depends on Stage 4.

- [ ] Expand `shoop_app_api` with concrete Built-in FX state/type/parameter/assignment types and typed controls for each toggle, selector, continuous value, assignment mutation, and editor snapshot.
- [ ] Change the descriptor to matching minimum-one audio input/output, no fixed maximum unless Stage 0 finds an existing repository safety limit that must be applied consistently, and required MIDI.
- [ ] Extend command intent/supersession keys so each continuous/toggle/selector control supersedes only itself; assignment mutations remain ordered and non-supersedable as with Built-in Synth.
- [ ] Extend in-process and native backend track creation, dry/wet/MIDI port mapping, controls, optimistic state, snapshots, state capture, staged replacement, sample-rate/backend recreation, and processor activity.
- [ ] Add concrete Built-in FX assignment fields and conversion/validation paths to backend session capture/replace without weakening OxiSynth assignment validation.
- [ ] Reject controls sent to the wrong processor, invalid parameter/type values, malformed assignments, mismatched audio counts, zero channels, and absent/extra MIDI before mutation.
- [ ] Test both backends for catalog constraints, 1/2/3/6-channel creation/rendering, all controls/types, local/global MIDI mappings, snapshots, capture/restore, rollback, driver/sample-rate replacement, and inactive no-DSP behavior.
- [ ] Verify focused `shoop_app_api`, `shoop_engine`, and `shoop_backend` native/Node tests plus per-commit gates.
- [ ] Commit the API/backend milestone.

## Stage 6 — Browser protocol, Worker, and AudioWorklet

Depends on Stage 5.

- [ ] Bump `shoop_audio_protocol::PROTOCOL_VERSION` and change Built-in FX wire topology to carry matching audio channel count and required MIDI.
- [ ] Add concrete wire enums/structures for all Built-in FX controls, types, continuous parameters, assignments, and editor state; update stable-envelope and raw-host contract expectations.
- [ ] Give each wire control a correct supersession identity and retain ordered assignment mutations; add serialization and version-mismatch tests.
- [ ] Translate topology, controls, state, and assignments in `shoop_audio_worklet` and `shoop_worklet_client`; reserve/register N dry inputs, N wet outputs, and one MIDI input deterministically.
- [ ] Ensure browser tracks with N internal channels remain valid even though the current physical Web Audio device boundary is stereo; reject only actual storage/protocol-limit violations.
- [ ] Exercise production worklet audio and MIDI for mono/stereo/N routing, each stage, bypass, learned local/global CC, ignored notes, snapshots, processor replacement, and save/restore transport.
- [ ] Verify focused protocol/worklet/client native and Node Wasm tests, locked Wasm builds, dependency isolation, generated zero-import worklet, raw-host contract, and applicable browser tests.
- [ ] Commit the browser transport milestone.

## Stage 7 — Session and application persistence/migration

Depends on Stages 5 and 6.

- [ ] Bump `SESSION_DOCUMENT_VERSION` and change `TrackTopologyDocument::BuiltInFx` to store matching audio channel count with one required dry MIDI channel/port.
- [ ] Add concrete Built-in FX CC assignment document types/fields while preserving the existing OxiSynth document representation and all previously accepted versions.
- [ ] Implement deterministic migration of version-9 fixed-stereo/no-MIDI Built-in FX tracks: convert topology to two channels, add the required unconnected MIDI port and empty MIDI loop channels using collision-free IDs, migrate state version 1 to version 2, and leave mappings empty.
- [ ] Validate canonical current state, finite/ranged controls, exact topology/port/channel shape, chain identity, assignment parameter/source uniqueness, and processor-specific assignment ownership for live and recorded processor states before publication.
- [ ] Map expanded Built-in FX topology, state, and assignments through `shoop_app` capture, archive save/load, backend replacement, native/browser transfer, recorded FX-state handling, and sample-rate recreation.
- [ ] Preserve transactional staging and rollback for malformed state, unsupported versions/types, invalid N-channel shape, assignment conflicts, missing capability, and processor construction failure.
- [ ] Test deterministic current-format round trips; migration from every accepted document version; old reverb enabled/disabled preservation; 1/2/3/6-channel sessions; assignments and MIDI port connections; malformed current/live/recorded state; native/browser transfer; and tails/editor visibility remaining transient.
- [ ] Update `docs/session_format_v1.md` with the new document version, topology, state grammar, assignment representation, migration, and transient/runtime-only state.
- [ ] Verify focused `shoop_session` and `shoop_app` native/Node tests plus per-commit gates.
- [ ] Commit the persistence/migration milestone.

## Stage 8 — Embedded editor, MIDI Learn, and documentation

Depends on Stages 5 and 7.

- [ ] Expand `shoop_egui/src/builtin_fx_editor.rs` with visibly ordered stage sections, enable controls, selectors, continuous controls with units/ranges, mode-dependent disabling, and state-driven rendering.
- [ ] Add a Built-in FX MIDI Learn window matching Built-in Synth: inspect the latest local input message, enable Assign only when it is a valid CC, show one flat continuous-parameter list plus assignment rows, and support Remove/Remove all; do not list toggles or selectors.
- [ ] If Stage 0 justified reuse, extract and test a narrow generic MIDI Learn UI/assignment helper and migrate OxiSynth without changing its behavior; otherwise keep concrete editors and share only trivial helpers.
- [ ] Preserve **Powered by FunDSP** and its working project link, editor close/reopen behavior, simultaneous Built-in Synth/FX editors, and transient visibility.
- [ ] Update Add Track UI and browser capability self-tests for variable matching audio count and required MIDI; ensure mono, stereo, and higher channel requests are representable and invalid shapes cannot be submitted.
- [ ] Add UI tests for every emitted control type, snapshot reflection, stage/mode enablement, flat learn list membership/exclusion, assignment lifecycle, latest-CC display, attribution, and coexistence with Built-in Synth.
- [ ] Update `src/rust/shoopdaloop/README.md`, `docs/source/concept.rst`, and `docs/source/usage.trackcontrols.rst` with rack order, controls/types, mono/stereo/N semantics, MIDI/global fan-out, bypass behavior, and FunDSP attribution.
- [ ] Build Sphinx with warnings denied and verify focused `shoop_egui`/`shoop_app` native and Node tests plus per-commit gates.
- [ ] Commit the editor/documentation milestone.

## Stage 9 — End-to-end and final local validation

Depends on all implementation stages. Run all payloads in the environment selected by `.agents/info/build.md`; on Nix/NixOS, use `nix develop`.

- [ ] Extend native application smoke coverage to create mono, stereo, and N-channel Built-in FX tracks with MIDI; exercise representative audio for every stage/type, continuous controls, exact bypass, stale-tail reset, local/global learned CC, ignored notes, save/reload, and migrated version-9 state.
- [ ] Extend browser self-test and production Worker/AudioWorklet smoke coverage with catalog/topology, representative DSP, local/global MIDI control, snapshots, save/reload, and N-channel internal routing evidence.
- [ ] Add objective offline render checks for each effect and stereo specialization; inspect generated metrics/output for intended response rather than relying only on state snapshots or call counters.
- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `cargo check --locked --no-default-features -p shoopdaloop --target wasm32-unknown-unknown` and `cargo build --locked -p shoop_audio_worklet --target wasm32-unknown-unknown`.
- [ ] Run `python3 scripts/check_worklet_client_dependencies.py --target wasm32-unknown-unknown`, inspect relevant `cargo tree` output, and verify FunDSP/new rack code does not introduce disallowed worklet dependencies.
- [ ] Run `python3 scripts/run_wasm_tests.py --runtime node --profile dev`.
- [ ] Run `python3 scripts/run_wasm_tests.py --runtime chrome --profile dev` when Chrome is available; otherwise record the local limitation and require the corresponding PR matrix job.
- [ ] Run `python3 -m unittest scripts.tests.test_wasm_test_report` and `python3 scripts/check_wasm_smoke_budget.py`.
- [ ] Run `trunk build` from `src/rust/shoopdaloop`, verify the generated worklet remains import-free through the existing contract, and run applicable browser smoke commands from its README.
- [ ] Run `sphinx-build -W --keep-going docs/source _build`.
- [ ] Build a prompt-to-artifact completion checklist mapping every goal, scope item, immutable criterion, named file, command, test, migration, target, PR gate, and review item to inspected evidence. Treat uncertainty as incomplete and fix or verify every gap.
- [ ] Inspect `git diff --check`, `git status`, changed-file scope, dependency trees, and generated-artifact exclusions; commit final corrections and rerun affected/full gates until the worktree is clean.
- [ ] Commit the final validation milestone.

## Stage 10 — Pull request, CI, and automated-review closure

Depends on a clean Stage 9.

- [ ] Review the commit series for meaningful stage boundaries and ensure no generated `dist`, worklet, `_build`, trace, credential, or unrelated files are committed.
- [ ] Push the implementation branch and open a PR against `master` with the immutable behavior contract, rack/channel/MIDI design, migration/protocol notes, FunDSP attribution, and exact local validation evidence. Keep it draft until local gates are complete if opened earlier for CI feedback.
- [ ] Record the PR URL and pushed head SHA in this plan.
- [ ] Monitor required checks with `gh pr checks`/`gh run watch`; require Build and Test matrices, Rust coverage, Docs, CodeQL, Codecov, Node/Chromium Wasm, Chrome AudioWorklet/Perfetto, and Firefox Web Audio jobs applicable to the changed paths.
- [ ] For failures, inspect the exact attempt, matrix job, logs, and artifacts with `gh run view`, `gh run download`, or `gh api`; compare peers before classifying a failure and read the Perfetto skill before analyzing `.pftrace` files.
- [ ] Reproduce deterministic failures locally where possible, add or improve regression coverage, fix the root cause, rerun affected and mandatory gates, commit, push, and wait for replacement checks. Do not use blind reruns instead of diagnosis.
- [ ] Query PR reviews, issue comments, and inline review threads after every push/review cycle. Classify every automated finding; fix valid issues with tests/evidence, or answer invalid/already-covered findings with concrete evidence, and resolve threads where permitted.
- [ ] Repeat CI and automated-review handling until the latest pushed SHA is green and review-clean; then mark the PR ready and confirm it is mergeable.
- [ ] Perform a final completion audit against the actual PR head and base, recheck every immutable acceptance criterion and Stage 9 artifact, verify the worktree/remote SHA, and record final check/review disposition in this plan.
- [ ] Finish only when no requirement is missing or uncertain, all required checks are green, no actionable review finding remains unresolved, the PR is mergeable, and the worktree is clean.
