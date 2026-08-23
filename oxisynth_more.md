# Built-in Synth controls and Tiny Synth removal plan

## Status and execution contract

This document is an implementation plan. No implementation stages are complete yet.

During implementation:

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

Stages are ordered by dependency. Where a shared enum or schema change makes two adjacent stages inseparable, combine them into one buildable milestone and record that adjustment here.

## Goals

- Present OxiSynth as the user-facing **Built-in Synth** on native and browser runtimes while retaining its stable internal `oxisynth` processor identity.
- Expose normalized reverb-send and chorus-send knobs that add to the selected SoundFont preset's authored sends through OxiSynth's direct Rust generator API.
- Give both send knobs MIDI Learn with the same source uniqueness, realtime update, snapshot convergence, and current-chain persistence guarantees previously used by Tiny Synth/FX.
- Restrict Control Change messages forwarded into OxiSynth to CC 1 modulation, CC 11 expression, and CC 64 sustain while retaining pitch bend and currently supported non-CC note/pressure messages.
- Keep OxiSynth's own reverb and chorus units enabled and preserve their tails across Panic and preset changes.
- Remove Tiny Synth/FX and `tinyviolin` completely from production code, catalogs, protocols, persistence, UI, active documentation, tests, and dependencies.
- Add subtle clickable OxiSynth attribution to the Built-in Synth editor.
- Persist the selected preset, both send controls, and their MIDI assignments transactionally without preserving compatibility with pre-change session files.

## Scope

Included:

- OxiSynth engine control/runtime state, strict state codec, MIDI filtering, direct send-generator control, MIDI Learn processing, panic, and preset-switch behavior.
- Native FX-chain and in-process engine backends, snapshots, session capture/replacement, global FX MIDI, inactive control handling, and driver switching.
- Application/backend domain types, optimistic controls, worklet protocol, AudioWorklet host, Worker/dummy runtime, and remote client conversions.
- Session schema and validation for Built-in Synth state and assignments, with a clean unsupported-version failure for old files.
- Built-in Synth editor knobs, MIDI Learn window, preset selector, Panic, branding, embedded logo, and external link.
- Removal of Tiny Synth/FX implementation and all current test/smoke/documentation assumptions that it is available.

Excluded:

- The future **Built-in FX** processor.
- Applying OxiSynth reverb or chorus to injected audio.
- Direct use of `oxisynth-reverb` or `oxisynth-chorus` by ShoopDaLoop.
- A custom 64-frame effects cache, new processor latency, or latency compensation.
- User controls for reverb room size/damping/width or chorus voice count/speed/depth/mode.
- Effect enable checkboxes; zero knob value means no additional Shoop-controlled send, while preset-authored sends remain.
- User-supplied SoundFonts, multiple presets per track, arbitrary OxiSynth channels, or automatic recorded-take OxiSynth state snapshots.
- Compatibility or migration for existing session documents, Tiny Synth/FX tracks, or version-1 OxiSynth processor-state strings.

## Target architecture

The fixed Built-in Synth topology remains two ignored dry audio inputs, two wet audio outputs, and one dry MIDI input. The inputs stay available as inert dry/wet-track ports but their samples never enter OxiSynth or affect its output. OxiSynth remains a logically single-channel adapter over its required internal channel set: accepted events from every source channel are remapped to channel 0 and dedicated-drum behavior stays disabled.

OxiSynth's internal reverb and chorus remain active. SoundFont preset/instrument zones continue to provide their own per-voice base sends. ShoopDaLoop owns two normalized additive controls:

```text
reverb_send, chorus_send: finite f32 in 0.0..=1.0
generator contribution: normalized value * 200.0
```

The `0..=200` generator-unit range matches the contribution of OxiSynth's standard CC 91/93 SoundFont modulators. The processor applies it with `Synth::set_gen(0, GeneratorType::ReverbSend | GeneratorType::ChorusSend, value)` rather than constructing MIDI messages. A default value of zero preserves only the SoundFont-authored send. The direct generator value is reasserted after any operation that can replace or reset channel state.

The engine owns typed send parameters, assignments, control state, editor state, and a bounded runtime publication bridge. MIDI-learned CC changes happen on the render side at the event boundary, call the direct generator API, and publish normalized values back to control-side snapshots and persistence without locking or allocating. UI/backend changes use the existing scheduled control path and converge through the same authoritative state.

MIDI Learn observes original source bytes before OxiSynth filtering. A learned event changes its assigned send whether or not that CC is forwarded. Forwarding is independent and permits only CC 1, CC 11, and CC 64; all other Control Change messages, including CC 0/32, 7, 10, 91, 93, and channel-mode CCs, are withheld from OxiSynth. Pitch bend and currently supported non-CC note, note-off, poly-pressure, and channel-pressure messages remain accepted and remapped to channel 0. Original event bytes remain unchanged in recorded/session MIDI media.

Panic and preset selection stop voices with OxiSynth's direct all-sounds-off behavior rather than System Reset, then select/reassert the desired preset and send controls as needed. They do not reset the reverb or chorus units, so existing tails decay naturally. Full processor construction/replacement may start with empty transient effect state.

Processor state uses a new strict canonical version containing logical SoundFont identity, preset identity, and both normalized send values. MIDI assignments remain typed current-chain metadata rather than effect-tail or recorded-take state. One parameter and one `(source channel, CC)` may each appear at most once. New tracks start with no assignments.

## Design rules and constraints

- Keep raw `oxisynth::Synth`, SoundFont handles, generators, and internal channels private to `shoop_engine::oxisynth`.
- Explicitly enable OxiSynth reverb and chorus in `SynthDescriptor`; do not rely on dependency defaults.
- Use `Synth::set_gen` for send changes. Do not synthesize CC 91/93, NRPN, or other MIDI messages for backend/UI controls.
- Keep normalized application values in `0.0..=1.0`; reject NaN, infinity, noncanonical encodings, and out-of-range values before mutation.
- Treat send controls as additive modulation over preset-authored sends, not absolute wet/dry controls.
- Keep all SoundFont parsing, state decoding, replacement construction, image decoding, and unbounded work outside realtime processing.
- Keep realtime MIDI mapping, direct generator changes, rendering, and publication bounded and allocation-free.
- Preserve event order and sample offsets at the Shoop processor boundary; OxiSynth's existing internal 64-frame rendering granularity remains an implementation detail.
- Separate mapping from forwarding: any valid learned CC can control a send, but only CC 1, 11, and 64 may also reach OxiSynth.
- Preserve source channel in MIDI Learn assignments even though accepted synthesis events are remapped to channel 0.
- Preserve track/global control semantics: ordinary track CC can update mapped controls while the processor is inactive; global FX controls remain deferred until normal processing resumes.
- Stop voices without clearing effect state on Panic, preset selection, and deactivation paths where a processor is retained.
- Retain fixed stereo dry/wet topology and prove dry input is ignored rather than silently incorporating it later.
- Keep the stable internal processor ID and chain identity `oxisynth`; use **Built-in Synth** only as the user-facing label/title.
- Embed `third_party/oxisynth/logo.png` in native and Wasm UI artifacts, preserve its aspect ratio, decode/upload it outside realtime work, and link only the attribution to `https://github.com/PolyMeilex/oxisynth`.
- Introduce a new session document version and OxiSynth processor-state version, accept only the new session version, and reject old documents transactionally before backend mutation.
- Keep current-chain Built-in Synth state separate from recorded-take state; effect tails, voices, live controller state, Panic history, and editor visibility remain transient.
- Remove Tiny Synth/FX rather than retaining hidden compatibility aliases, fallback processors, wire variants, or migration-only runtime code.
- Historical planning records may retain clearly historical references, but active source, user documentation, smoke fixtures, catalogs, schemas, and dependencies must not advertise or instantiate Tiny Synth/FX.

## Immutable acceptance criteria

- The processor selector and editor identify OxiSynth tracks as **Built-in Synth** on native, AudioWorklet, and Worker/dummy runtimes; no current catalog exposes Tiny Synth/FX.
- The Built-in Synth editor retains preset selection and Panic, adds reverb-send and chorus-send knobs, and shows a subtle clickable `Powered by` OxiSynth logo linking to the canonical GitHub repository.
- A new Built-in Synth track starts with the default preset, both additional sends at zero, no MIDI assignments, and OxiSynth's reverb and chorus units active.
- Each send knob is finite and normalized, maps deterministically to an additive `0..=200` SoundFont generator contribution, changes current and future voices through `Synth::set_gen`, and never injects CC 91/93 or NRPN into OxiSynth.
- Preset-authored reverb/chorus sends remain effective when Shoop's corresponding knob is zero.
- Reverb and chorus controls can each learn one exact source channel/CC assignment; duplicate targets/sources and invalid channels/controllers are rejected, and assign/remove/remove-all behavior is consistent across native and remote runtimes.
- Learned CC values update audio, authoritative editor snapshots, save state, and optimistic UI convergence. Mapping does not consume an event for forwarding purposes.
- Only CC 1, CC 11, and CC 64 reach OxiSynth channel 0. Pitch bend and currently supported non-CC note/pressure events continue to reach channel 0; every other CC, Program Change, and bank selection is blocked.
- CC 64 demonstrably sustains and releases notes; CC 1 and CC 11 demonstrably retain modulation/expression behavior. CC 7, 10, 91, 93, 120, and 123 demonstrably do not alter OxiSynth directly.
- Live, looped, start-state, on-screen, and global FX MIDI all obey the same final OxiSynth filter, while recorded/session MIDI retains original channels and bytes.
- Panic and preset changes stop old voices without clearing existing reverb/chorus tails. A new preset renders with the persisted additive sends still applied.
- Samples on either Built-in Synth dry audio input have no effect on wet output; only OxiSynth-generated audio appears there.
- Selected preset, both send values, and MIDI assignments round-trip exactly through native/in-process capture, save/load, remote replacement, browser restart/replay, and driver/sample-rate switching.
- Malformed state, invalid sends/assignments, unsupported session versions, and failed staged replacement leave the prior running session and processing progress intact.
- New sessions use only the new schema/version. Pre-change session files are rejected as unsupported; no Tiny Synth or old OxiSynth migration is attempted.
- No automatic OxiSynth recorded-take `fx_state` is written; dry MIDI continues to render through the track's current Built-in Synth state.
- `tinyviolin`, Tiny Synth/FX engine/UI/domain/wire/session variants, active fixtures, and current documentation are absent after dependency regeneration and repository review.
- OxiSynth steady rendering, MIDI filtering/mapping, send changes, Panic, and preset switching remain bounded and allocation-free on the realtime path.
- Carla, External, direct-track, session transactions, global controls, native drivers, worklet transport, and unrelated UI behavior remain passing.
- Formatting, test-usage policy, warning-denying native/Wasm builds, tracing coverage, the complete Rust suite, Node/Wasm suites, and available packaged browser smoke validation pass.

## Implementation stages

### Stage 0 — Baseline and inventory

- [ ] Record current OxiSynth state/control flow and all Tiny Synth/FX references across engine, app backend, backend facade/native adapter, app API, application, protocol, worklet, remote client, session, egui, smoke fixtures, docs, tracing inventory, workspace dependencies, and lockfile.
- [ ] Add or strengthen characterization tests for current preset-authored reverb/chorus output, direct CC 91/93 behavior, sustain, modulation, expression, dry-input isolation, preset switching, Panic, and realtime allocations.
- [ ] Confirm `Synth::set_gen` behavior for `ReverbSend`/`ChorusSend`: additive units, existing/future voices, channel reset, preset selection, and exact audio effect under the bundled SoundFont.
- [ ] Confirm an all-sounds-off preset/Panic path stops dry voices without resetting OxiSynth effect state.
- [ ] Record focused native and Node/Wasm commands and representative audio fixtures for later comparisons.

Verification:

- [ ] Existing focused OxiSynth, Tiny Synth/FX, backend, session, protocol, worklet, client, application, and egui tests pass before production changes.
- [ ] The inventory accounts for both native FX-chain and in-process engine backends, global/inactive MIDI paths, state capture/replacement, driver switching, protocol replay/coalescing, browser smokes, and every Tiny removal surface.
- [ ] Audio evidence distinguishes preset-authored sends, standard CC modulation, direct generator modulation, dry voice output, and effect tails.

### Stage 1 — Implement OxiSynth send state and MIDI policy

Depends on Stage 0.

- [ ] Add typed `ReverbSend`/`ChorusSend` parameter identities, validated normalized values, MIDI assignments, editor state, and bounded runtime publication to `shoop_engine::oxisynth`.
- [ ] Extend control state and the strict processor-state codec with preset plus both send values; choose and document one canonical version-2 encoding using exact finite float representation.
- [ ] Configure both OxiSynth effect units explicitly active and apply normalized sends through `Synth::set_gen` on channel 0 during processor preparation and realtime control.
- [ ] Process learned CCs before synthesis filtering, apply their normalized values at the event boundary, and publish MIDI-driven changes back to control-side snapshots without locks or allocations.
- [ ] Replace generic CC forwarding with an explicit CC 1/11/64 allowlist while retaining pitch bend and accepted non-CC messages; continue dropping Program Change and bank select.
- [ ] Replace System Reset in Panic/preset-switch paths with all-sounds-off plus direct program/send reassertion so effect tails survive and old dry voices do not.
- [ ] Add a control-only MIDI path for inactive Built-in Synth tracks and preserve deferred global-control behavior.
- [ ] Keep processor output independent of both routed dry audio inputs.

Verification:

- [ ] Codec tests cover canonical round trip, zero/max sends, noncanonical floats, NaN/infinity, ranges, malformed envelopes, unknown versions/fonts, and unavailable presets.
- [ ] Direct API tests prove zero preserves preset-authored sends, normalized one maps to 200 generator units, UI/MIDI changes affect existing and future voices, and no synthetic effect MIDI is sent.
- [ ] MIDI tests cover every source channel, learned and unlearned CCs, CC 1/11/64 forwarding, blocked representative CCs including 0/7/10/32/91/93/120/123, pressure, pitch bend, malformed messages, ordering, and offsets.
- [ ] Audio tests prove sustain/modulation/expression, send audibility, ignored dry inputs, surviving effect tails after Panic/preset change, and absence of old-preset dry voices.
- [ ] Allocation guards cover steady rendering, mapped/filtered MIDI, direct send changes, Panic, preset selection, and control-only inactive processing.
- [ ] Run focused native and Node/Wasm `shoop_engine` tests, formatting, test-usage policy, and a warning-denying engine build.

### Stage 2 — Extend typed backend and application-domain controls

Depends on Stage 1.

- [ ] Add OxiSynth send parameters, assignments, editor fields, and set/assign/remove/clear controls to `shoop_app_api` and `shoop_backend` domain types.
- [ ] Give preset and each send its own optimistic/supersedable control key; keep assignment mutations durable and Panic ephemeral.
- [ ] Extend native `FXChainBackendKind::OxiSynth` control mirrors and scheduled render mutations for UI send changes, MIDI assignment changes, state capture/restore, snapshots, and Panic.
- [ ] Extend in-process `EngineOxiFx` with the same behavior, including MIDI-driven runtime-to-control synchronization before snapshots and capture.
- [ ] Carry OxiSynth assignments through backend session capture/replacement data with strict source/target uniqueness validation.
- [ ] Ensure restore prepares a complete synth, send state, and assignments before publication, and that rejected controls cannot partially update mirrors or processors.
- [ ] Preserve fixed 2-dry/2-wet/1-MIDI topology and update descriptor label to **Built-in Synth** without changing internal identity.

Verification:

- [ ] Native and in-process backend tests cover defaults, direct controls, MIDI-driven controls, assignment lifecycle, active/inactive behavior, global controls, snapshot synchronization, state capture/restore, and malformed rollback.
- [ ] Backend rendering tests prove identical generated output/send behavior and dry-input isolation in both backend models.
- [ ] Rapid send changes are last-write-wins and authoritative snapshots converge after accepted/rejected mutations.
- [ ] Driver/sample-rate replacement preserves exact preset, sends, and assignments while transient tails may restart.
- [ ] Run focused `shoop_backend`, `shoop_engine` app-backend, and `shoop_app_api` tests plus formatting and warning-denying builds.

### Stage 3 — Update protocol, AudioWorklet, and remote client

Depends on Stage 2.

- [ ] Add wire send-parameter, assignment, editor-state, set/assign/remove/clear control representations for OxiSynth.
- [ ] Add unique coalescing keys for both continuous sends; keep assignment operations journaled/durable and Panic non-journaled.
- [ ] Implement complete conversions and validation in `shoop_audio_worklet` and `shoop_worklet_client`.
- [ ] Publish MIDI-driven send changes through worklet snapshots and preserve desired-state overlays through stale acknowledgements, rejection, replay, restart, and generation changes.
- [ ] Carry processor state and assignments through chunked remote session capture/replacement without a stateless special case.
- [ ] Update Worker/dummy and AudioWorklet render fixtures for the CC allowlist, MIDI Learn, direct send behavior, and ignored audio input.

Verification:

- [ ] Protocol JSON round-trip and coalescing tests cover every new OxiSynth variant and reject invalid values/assignments.
- [ ] Native and Node/Wasm worklet/client tests cover preset, both sends, assignment lifecycle, track/global learned CC, CC 64 sustain, blocked CC 91/93 forwarding, snapshots, replay/restart, stale responses, and rollback.
- [ ] Worklet audio tests prove mapped CC 91/93 changes the external control through the direct API while an unmapped CC 91/93 has no direct OxiSynth effect.
- [ ] Run focused `shoop_audio_protocol`, `shoop_audio_worklet`, and `shoop_worklet_client` native/Wasm tests and warning-denying Wasm builds.

### Stage 4 — Replace session/application persistence with the clean schema

Depends on Stages 2 and 3.

- [ ] Introduce one new `SESSION_DOCUMENT_VERSION`, require it exactly, and remove migrations/compatibility paths for older session documents.
- [ ] Replace Tiny-specific assignment document/backend fields with typed OxiSynth reverb/chorus send assignments.
- [ ] Require Built-in Synth tracks to have matching OxiSynth chain identity, canonical version-2 processor state, valid unique assignments, fixed topology, and no automatic take-state records.
- [ ] Update application save/load conversion, optimistic state, action dispatch, capture, staged replacement, and driver-switch logic for preset, sends, and assignments.
- [ ] Remove branches that accept empty/stateless OxiSynth state or migrate version-4/version-5 OxiSynth documents.
- [ ] Keep semantic OxiSynth decoding in staged backend preparation so malformed state cannot replace the running session.
- [ ] Update session fixtures and `docs/session_format_v1.md` to describe only the new supported document/state contract and explicit rejection of older files.

Verification:

- [ ] Archive tests cover exact-version acceptance, unsupported old/future versions, structural OxiSynth requirements, invalid assignments, malformed state, wrong chain identity, and transaction rollback.
- [ ] Application tests cover native/in-process/remote save-load, nondefault sends and assignments, MIDI-driven values before save, browser replacement, and driver/sample-rate switching.
- [ ] Saved manifests contain canonical nonempty OxiSynth state and current-chain assignments but no OxiSynth automatic recorded-take state.
- [ ] Failed decode/preparation leaves the prior application/backend session and processing progress intact.
- [ ] Run focused `shoop_session`, `shoop_app`, backend, protocol, and client tests plus the test-usage policy check.

### Stage 5 — Build the Built-in Synth editor and attribution

Depends on Stages 2 through 4.

- [ ] Rename the processor selector label and editor title to **Built-in Synth** while retaining stable internal IDs and typed OxiSynth variants.
- [ ] Add authoritative/optimistic reverb-send and chorus-send knobs over `0.0..=1.0` without enable checkboxes.
- [ ] Adapt the Tiny MIDI Learn interaction pattern for only the two OxiSynth send parameters: latest CC display, parameter selection, assign, per-assignment removal, and remove all.
- [ ] Keep the existing filterable preset selector, Panic, visibility behavior, and stable per-track window identity.
- [ ] Embed and cache `third_party/oxisynth/logo.png`, then render a small muted `Powered by` attribution whose logo opens the canonical OxiSynth GitHub URL on click.
- [ ] Ensure native and Wasm image loading/link behavior uses the shared egui path and does not require runtime filesystem access.

Verification:

- [ ] Shared native/Wasm UI tests cover both knobs, rapid changes, MIDI Learn assignment/removal, preset selection, Panic, visibility, and authoritative convergence.
- [ ] Descriptor/add-track tests show **Built-in Synth** and no user-facing OxiSynth processor label except the attribution/logo.
- [ ] Attribution tests verify logo dimensions/aspect, clickable response, exact URL, and graceful rendering if texture creation fails.
- [ ] Existing unrelated track header, connection, lifecycle, log, and recovery tests remain passing.
- [ ] Run focused `shoop_egui` and `shoop_app` native/Node-Wasm tests, formatting, warning-denying builds, and the test-usage policy check.

### Stage 6 — Remove Tiny Synth/FX and clean active surfaces

Depends on Stages 1 through 5. This is a cross-workspace removal milestone and may be committed with adjacent schema/UI work if required to keep exhaustive enums buildable.

- [ ] Delete `shoop_engine::tiny_synth_fx`, its session processor backend/routes/accessors, control queues, allocation tests, and app-backend chain kind.
- [ ] Remove `tinyviolin` from workspace/engine manifests and regenerate `Cargo.lock` so the package is absent.
- [ ] Remove Tiny processor IDs, topologies, controls, state, descriptors, backend fields, native chain mappings, application actions/keys, protocol variants, conversions, and remote/worklet support.
- [ ] Delete the Tiny editor/module and remove it from track-widget composition.
- [ ] Remove Tiny session topology/chain/parameter/assignment variants, validators, fixtures, and obsolete migration paths.
- [ ] Rewrite native/browser smoke fixtures, Wasm runtime tests, browser capability probes, and scripted labels to exercise Built-in Synth instead.
- [ ] Update active usage/concept documentation, browser README text, tracing inventory, and test descriptions; remove stale Tiny-specific claims from current documentation.
- [ ] Review historical records separately and mark/retain them only when their historical status is unambiguous.

Verification:

- [ ] Repository searches find no `tinyviolin` dependency and no Tiny Synth/FX production symbol, catalog entry, session/wire variant, active smoke fixture, or current user documentation.
- [ ] Workspace metadata and lockfile contain no `tinyviolin` package.
- [ ] Processor catalogs on native and browser contain Built-in Synth exactly once and preserve feature-dependent External/Carla entries.
- [ ] Session and wire decoding reject old Tiny-bearing artifacts rather than flattening or partially loading them.
- [ ] Run all packages affected by removal on native and Node/Wasm, plus formatting, test-usage policy, warning-denying workspace builds, and tracing coverage.

### Stage 7 — Final end-to-end validation

Depends on all prior stages.

- [ ] Create Built-in Synth tracks in native dummy/offline, an available native physical driver, browser Worker/dummy, and browser AudioWorklet runtimes; confirm identical defaults, labels, editor state, and catalog.
- [ ] Render representative dry and effect-heavy presets; move each send knob through zero/intermediate/one and confirm additive preset-preserving behavior.
- [ ] Learn CCs including 91/93 on chosen source channels, drive both sends from track and global inputs, and confirm audio plus authoritative UI/persistence updates without direct forwarding.
- [ ] Exercise CC 1 modulation, CC 11 expression, CC 64 sustain/release, pitch bend, pressure, notes, and note-offs from multiple source channels; confirm all other representative CCs and bank/program changes are blocked at OxiSynth while source MIDI media remains exact.
- [ ] Feed silence and nonzero audio into both dry inputs and prove identical synth output.
- [ ] Trigger Panic and preset changes during wet notes; confirm old dry voices stop, tails decay, the new preset uses retained sends, and the realtime allocation guard remains clean.
- [ ] Save/reload nondefault preset/sends/assignments on native and browser paths, restart remote worklets, and switch driver/sample rate; confirm exact durable state.
- [ ] Attempt old, Tiny-bearing, malformed, wrong-version, invalid-assignment, and invalid-OxiSynth sessions; confirm explicit transactional rejection.
- [ ] Confirm current-state dry-MIDI rerender behavior and absence of automatic OxiSynth take-state records.
- [ ] Run `cargo fmt --all`, then `cargo fmt --all -- --check`.
- [ ] Run `python3 scripts/check_shoop_test_usage.py`.
- [ ] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [ ] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [ ] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [ ] Run `python3 scripts/run_wasm_tests.py --runtime node --profile ci` and focused browser-runtime tests when Chrome is available.
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown` and run the packaged browser smoke commands documented in `src/rust/shoopdaloop/README.md` when browser tooling is available.
- [ ] Audit active docs, generated artifacts, workspace metadata, and repository searches for stale Tiny Synth support, stale OxiSynth labels, old session compatibility claims, or accidental Built-in FX scope.

Final verification is complete when the repository is clean, each immutable acceptance criterion maps to direct test/manual evidence recorded in this plan, and the final milestone commit contains no unrelated changes.
