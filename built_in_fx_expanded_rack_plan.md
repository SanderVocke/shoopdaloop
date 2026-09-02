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

Baseline/branch evidence (2026-09-01): expanded-rack work starts from the fully validated MVP head `364f667980361bf1a81df4310d4ed8d7da71c98f`; plan commit `9a29f605` created and published `builtin-fx-expanded-rack`. PR #844 is still open rather than merged, so no expanded-rack PR may be opened until #844 is merged and this branch is updated onto current `master`, unless the user explicitly approves a stacked PR. Baseline native nextest passed 1,658/1,658 with four policy skips, and all 17 Node Wasm packages passed 1,377/1,377. Formatting, warning-denying workspace build, locked Wasm app/worklet builds, dependency isolation, zero-import worklet (`imports=0`), and raw-host contract passed. Current versions are processor state 1, session document 9, and browser protocol 20.

Fixed-assumption inventory: `shoop_engine::builtin_fx` owns a two-channel array, one reverb graph, and one boolean codec/control; `Session` routes Built-in FX audio but passes no MIDI; API/backend descriptors and creation validation require 2-in/2-out and `Unsupported` MIDI; protocol topology is a unit `BuiltInFx` variant with one reverb command/state; Worker/worklet/client reserve/translate fixed stereo ports; session topology is a unit variant whose validator requires two dry/two wet audio and zero MIDI and whose assignment field is OxiSynth-specific; application capture/restore, optimistic controls, smoke tests, editor fixtures, Add Track capability tests, and documentation all encode the same fixed shape. The implementation stages below cover every layer in that inventory.

DSP/control contract established by disposable native/Wasm probes:

- Compressor ranges/defaults: Threshold `-48..0 dB`/`-18`, Ratio `1..20`/`4`, Attack `0.5..100 ms`/`10`, Release `20..1000 ms`/`150`, Makeup `0..18 dB`/`0`. CC uses linear dB, squared ratio skew, and logarithmic time mappings.
- Drive defaults to Saturation while disabled: Drive `0..36 dB`/`12`, Tone `0..1`/`0.5`, Mix `0..1`/`1`, Output `-18..6 dB`/`0`; CC is linear in dB or normalized space. FunDSP 2x oversampling introduced a 40-frame impulse peak delay and took about `161 ms` versus `17 ms` for two million release-mode ticks, so it is rejected for this rack: no dynamic latency/parallel dry-path compensation is needed.
- EQ uses low shelf `120 Hz, Q 0.707`, bell `1 kHz, Q 0.8`, and high shelf `8 kHz, Q 0.707`; each gain is `-12..12 dB`, default `0`, with linear-dB CC mapping.
- Chorus defaults while disabled: Rate `0.05..5 Hz`/`0.3`, Depth `0..1`/`0.5`, Mix `0..1`/`0.3`, Width `0..1`/`1`; rate CC is logarithmic and the rest linear.
- Modulation defaults to Tremolo while disabled: Rate `0.05..5 Hz`/`0.5`, Depth `0..1`/`0.5`, Mix `0..1`/`0.5`, Feedback `-0.95..0.95`/`0.25`, Spread `0..1`/`1`; rate CC is logarithmic and the rest linear.
- Reverb defaults enabled: Room, Amount `0..1`/`0.2`, Tone `0..1`/`0.5`; Tone is a neutral-centered wet tilt. Room is the current `reverb_stereo(10, 2.5, 0.5)`, Hall is `reverb2_stereo(20, 4, 0.8, 0.4, lowpole_hz(8000))`, and Plate is `reverb3_stereo(2.5, 0.8, lowpole_hz(10000))`.
- Parameter targets use allocation-free per-sample ramps: 10 ms for gain/filter/dynamics targets and 20 ms for delay/modulation targets. LFOs retain phase while Rate changes. Toggles and type changes apply at block boundaries and reset displaced state.
- Exactly-stereo compression uses one detector driven by the larger channel magnitude; stereo Chorus uses deterministic decorrelated seeds and Width blends toward that decorrelation; stereo Modulation offsets right LFO phase by up to half a cycle via Spread; stereo Reverb uses the native stereo graph. Mono and N > 2 use isolated mono instances with deterministic channel-index seeds.
- Reuse is intentionally narrow: share private source-table operations for assignment uniqueness/matching and small CC parsing/mapping helpers; retain concrete OxiSynth/Built-in FX state, API, wire, document, and editor types. A generic MIDI Learn window would complicate action typing and test geometry more than it removes, so the Built-in FX editor will mirror behavior concretely while OxiSynth remains unchanged.

- [x] Ensure the completed Built-in FX MVP is present on the intended base branch. Prefer updated `master` after PR #844 merges; do not create a stacked implementation PR without explicit approval.
- [x] Record base SHA, clean worktree, existing Built-in FX/OxiSynth focused test results, complete native/Node baselines, and current session/protocol versions in this plan.
- [x] Create and publish a dedicated branch such as `builtin-fx-expanded-rack` from that base.
- [x] Inventory the current Built-in FX control/state path through `shoop_engine`, `shoop_app_api`, `shoop_backend`, `shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, `shoop_app`, `shoop_session`, and `shoop_egui`; record every fixed-stereo/no-MIDI assumption that must change.
- [x] Prototype the proposed compressor, four Drive shapes, shelves/bell, chorus, modulation modes, and three reverb families for native and `wasm32-unknown-unknown`; verify channel I/O, sample-rate setup, reset, allocation, and mutable-control strategy without integrating behavior.
- [x] Measure/inspect 2x Drive oversampling latency and bounded CPU on native and Wasm, then record the supported choice and any latency-reporting requirement.
- [x] Record exact validated ranges, defaults, CC curves, smoothing constants, fixed EQ frequencies/Q, stereo modulation behavior, and FunDSP reverb mappings in this plan before production implementation.
- [x] Decide the narrow reusable MIDI assignment/UI seams after comparing Built-in Synth engine and editor code; document why each extraction is beneficial or why concrete duplication is safer.
- [x] Verify baseline formatting, warning-denying build, focused native/Node tests, locked Wasm checks, and worklet dependency/import contracts.
- [x] Commit the baseline/prototype plan milestone without committing generated prototype artifacts.

## Stage 1 — Versioned state, parameters, and variable-channel rack foundation

Depends on Stage 0.

Foundation evidence: `builtin_fx.rs` now defines six stage states, stable type enums, 23 continuous parameters/ranges/CC curves, strict finite validation, canonical ordered version-2 hex-float state, and exact version-1 migration. A private fixed-size MIDI source table is shared with OxiSynth while public types remain concrete. Control and processor state share lock-free atomic parameter publication; allocation-free 10/20 ms linear smoothers retain targets across control/MIDI paths. Processors prepare 1..N inputs, outputs, two stage buffers, true-stereo Room reverb for two channels, and isolated mono reverb banks otherwise. Native focused Built-in FX/OxiSynth tests passed 28/28, backend tests 5/5, app/session tests 2/2, worklet/client tests 3/3; focused Node Wasm passed 9 Built-in FX and 18 OxiSynth tests. Formatting, warning-denying workspace build, Rust-test policy, closed tracing coverage, and locked engine Wasm check passed.

- [x] Expand `shoop_engine/src/builtin_fx.rs` and focused submodules with typed stage states, effect type enums, the continuous-parameter enum, strict validation, defaults, and canonical version-2 encode/decode plus version-1 migration.
- [x] Add Built-in FX CC assignment storage with Built-in Synth-equivalent uniqueness, validation, iteration, matching, assign/remove/clear behavior, and no default assignments.
- [x] Add callback-visible runtime parameter publication so UI/backend and MIDI updates converge on one current state without locks; ensure captured/editor state observes MIDI-driven values.
- [x] Replace fixed `[Vec<f32>; 2]` storage with prepared N-channel input/output and ping-pong storage, preserving legal FunDSP chunking and exact all-disabled passthrough.
- [x] Add a fixed stage interface with explicit enable/reset/process behavior and test-visible processing counters; a disabled stage must never be called even when later stages are enabled.
- [x] Implement block-boundary state transitions, parameter smoothers, sample-rate replacement, and reset semantics without callback allocation.
- [x] Test strict/canonical state, version-1 migration, invalid/non-finite/out-of-range controls, assignment conflicts, CC curve endpoints/midpoints, 1/2/3/6-channel buffer access, chunking, exact bypass, and no-allocation control synchronization.
- [x] Verify focused `shoop_engine` native/Node Wasm tests, formatting, warning-denying workspace build, tracing/test policies, and locked Wasm compilation.
- [x] Commit the rack foundation milestone.

## Stage 2 — Dynamics, Drive, and EQ DSP

Depends on Stage 1.

DSP evidence: focused modules implement a stereo-linked/otherwise-independent peak compressor with fixed 6 dB knee, four FunDSP waveshapers with dynamic FunDSP tone filtering and DC blocking, and per-channel dynamic FunDSP low-shelf/bell/high-shelf filters. The fixed rack pipeline invokes only enabled stages and only the selected Drive unit, resets state on bypass/type changes, and uses the preallocated ping-pong buffers. Native Built-in FX coverage passed 16 tests (plus routed/backend coverage) and Node Wasm passed 15; deterministic tests cover attack/release, threshold/ratio, stereo linking, N-channel isolation, distinct bounded Drive output/type call counters, EQ boost/cut selectivity, 44.1/48/96 kHz, 1/128/257/2048 frames, and allocation-free enabled/bypassed processing. Formatting, warning-denying workspace build, Rust-test policy, and 147-module closed tracing inventory passed.

- [x] Implement the compressor with peak detection, fixed soft knee, attack/release smoothing, ratio/threshold gain computer, makeup gain, mono/N independent detectors, and exactly-stereo linked detection.
- [x] Implement Saturation, Overdrive, Distortion, and Fuzz with shared Drive/Tone/Mix/Output controls, bounded finite output, DC blocking, and the Stage 0 oversampling decision.
- [x] Implement the fixed-frequency three-band EQ with low shelf, broad mid bell, and high shelf for every channel.
- [x] Ensure each stage and unselected Drive type performs zero DSP work while bypassed and resets bounded internal state on disable/type replacement where required.
- [x] Add deterministic signal tests for compressor threshold/ratio/attack/release and stereo linking; Drive type distinction, harmonic/nonlinear response, DC/finite bounds and mix/output; EQ neutral response and per-band boost/cut selectivity.
- [x] Cover mono, stereo, and 3/6-channel isolation at 44.1/48/96 kHz and callback sizes including 1, 128, 257, and larger-than-prepared blocks.
- [x] Verify focused native/Node tests, allocation tests, tracing inventories, and per-commit gates.
- [x] Commit the dynamics/Drive/EQ milestone.

## Stage 3 — Chorus, Modulation, and Reverb DSP

Depends on Stage 1; may proceed in parallel with Stage 2 only if edits remain non-overlapping.

DSP evidence: preallocated custom delay/LFO stages implement five-voice Chorus and Tremolo/Flanger/4-stage Phaser with stable phase under smoothed controls; exactly-stereo Width/Spread decorrelates L/R while N-channel processing remains isolated. Reverb preconstructs Room/Hall/Plate FunDSP families, processes only the selected type, folds stereo wet output for mono banks, applies neutral-centered wet tilt, and preserves additive dry+Amount behavior. Disable/type transitions reset delay/allpass/reverb/tone state. Native focused engine/backend coverage passed 26/26 and Node Wasm passed 22 tests, including mode/type call counters, stale-state removal, tone/amount/type distinctions, stereo specialization, N isolation, 44.1/48/96 kHz, 1/128/257/2048 frames, finite extremes, and allocation-free full-rack processing. Formatting, warning-denying workspace build, Rust-test policy, and 150-module closed tracing inventory passed.

- [x] Implement Chorus Rate/Depth/Mix/Width with preallocated delay/modulation state, mono behavior, exactly-stereo decorrelation, and deterministic independent N-channel seeds.
- [x] Implement Tremolo, Flanger, and Phaser behind one Modulation stage with shared Rate/Depth/Mix/Feedback/Spread controls; keep Feedback inactive in Tremolo and process only the selected mode.
- [x] Implement Room, Hall, and Plate using the Stage 0 FunDSP mappings, additive Amount, wet Tone filtering, true-stereo processing for two channels, and mono wrappers for mono/N-channel banks.
- [x] Reset chorus/modulation/reverb tails and displaced type state on disable or type change; prove re-enable cannot revive stale output.
- [x] Add deterministic tests for modulation frequency/depth/mix, stereo width/spread, mode distinction, feedback bounds, reverb type distinction/tails/Amount/Tone, channel isolation, sample rates, callback sizes, and finite output under extreme valid controls.
- [x] Prove bypassed stages and unselected types receive zero calls and all steady-state paths allocate nothing.
- [x] Verify focused native/Node tests, tracing inventories, warning-denying builds, and per-commit gates.
- [x] Commit the modulation/reverb milestone.

## Stage 4 — Engine routing and MIDI behavior

Depends on Stages 1–3.

Routing/MIDI evidence: `Session` now feeds local plus filtered/deferred global MIDI to Built-in FX before chunked audio processing and invokes a control-only path while inactive. Learned absolute CC updates lock-free parameter targets; unsupported traffic is ignored. Local CC applies while inactive, global CC remains pending until reactivation, and neither path wakes inactive rack DSP. Built-in routing validates matching processor audio counts and at most one transitional MIDI port, routes 1/2/3/6 channels with one MIDI port, preserves exact all-off bypass, and chunks callbacks twice the prepared size through enabled Drive. Focused native engine/global tests passed 30/30 and Node Wasm Built-in FX tests passed 24; existing bounded global saturation/no-recording/allocation tests remain green. Formatting, warning-denying workspace build, Rust-test policy, and tracing inventory passed.

- [x] Extend `Session` Built-in FX routing from fixed stereo to matching N-channel ports and pass the combined track/global MIDI stream to the processor while preserving oversized-callback chunking and event order.
- [x] Give Built-in FX a `process_midi_controls_only` path equivalent to Built-in Synth: local MIDI may update controls while processor audio is inactive; global controls retain the existing bounded deferred behavior and never wake inactive DSP.
- [x] Apply learned absolute CC mappings to continuous parameters at the event/block boundary with smoothing; ignore notes, selectors/toggles, malformed CC, program changes, pressure, pitch bend, and other unsupported messages.
- [x] Preserve local MIDI plus global fan-out ordering, saturation bounds, pending-control replacement, diagnostics, and no-recording behavior already established for built-in processors.
- [x] Test local learning/control, global mapped control, local/global additive order, note/unsupported-message rejection, inactive behavior, bounded deferred global restoration, assignment changes, no default mapping, and no audio-stage calls caused only by MIDI.
- [x] Test routed 1/2/3/6-channel input/output, exact bypass, stereo-specific behavior, generic inactivity, callback growth, and channel-count/port-shape rejection.
- [x] Verify focused `shoop_engine` native/Node tests and per-commit gates.
- [x] Commit the engine routing/MIDI milestone.

## Stage 5 — Application API and native/in-process backends

Depends on Stage 4.

API/backend evidence: `shoop_app_api` now has concrete six-stage state, three type enums, 23 labeled continuous parameters, concrete assignments, and typed controls. Optimistic application keys distinguish every stage/selector/parameter while assignment mutations remain ordered. The catalog requires matching positive 1..N audio plus MIDI. In-process and native backends create alternating N dry/wet ports plus one MIDI port, prepare matching processors, expose all controls/snapshots, validate values and assignment ownership/uniqueness, capture/restore assignments separately from processor state, preserve mappings on state replacement, and recreate at changed sample rate/buffer size. Focused native engine/in-process coverage passed 29/29; the comprehensive native dummy-driver test passed for 1/2/3/6 channels, all controls, learned MIDI, rollback, restore, and 96 kHz/256-frame replacement. API and optimistic-state tests passed; Node Wasm passed focused backend (3), API (1), and application (1) tests. Formatting, warning-denying workspace build, Rust-test policy, and tracing inventory passed. Browser wire support intentionally follows in Stage 6.

- [x] Expand `shoop_app_api` with concrete Built-in FX state/type/parameter/assignment types and typed controls for each toggle, selector, continuous value, assignment mutation, and editor snapshot.
- [x] Change the descriptor to matching minimum-one audio input/output, no fixed maximum unless Stage 0 finds an existing repository safety limit that must be applied consistently, and required MIDI.
- [x] Extend command intent/supersession keys so each continuous/toggle/selector control supersedes only itself; assignment mutations remain ordered and non-supersedable as with Built-in Synth.
- [x] Extend in-process and native backend track creation, dry/wet/MIDI port mapping, controls, optimistic state, snapshots, state capture, staged replacement, sample-rate/backend recreation, and processor activity.
- [x] Add concrete Built-in FX assignment fields and conversion/validation paths to backend session capture/replace without weakening OxiSynth assignment validation.
- [x] Reject controls sent to the wrong processor, invalid parameter/type values, malformed assignments, mismatched audio counts, zero channels, and absent/extra MIDI before mutation.
- [x] Test both backends for catalog constraints, 1/2/3/6-channel creation/rendering, all controls/types, local/global MIDI mappings, snapshots, capture/restore, rollback, driver/sample-rate replacement, and inactive no-DSP behavior.
- [x] Verify focused `shoop_app_api`, `shoop_engine`, and `shoop_backend` native/Node tests plus per-commit gates.
- [x] Commit the API/backend milestone.

## Stage 6 — Browser protocol, Worker, and AudioWorklet

Depends on Stage 5.

Browser evidence: protocol 22 carries variable Built-in FX channel count, all concrete stage/type/parameter/assignment controls, and complete editor state. Supersession IDs are disjoint per stage/type/parameter, legacy Reverb enable coalesces with the typed Reverb stage, and assignment mutations remain ordered. Client/worklet translations round-trip all 23 parameters; deterministic reservations and the production worklet register N dry/N wet/one MIDI while the physical device remains stereo. Production tests cover true stereo Reverb/bypass, every non-Reverb stage on host audio, three-channel internal registration, local and global learned CC, ignored notes, snapshots, assignment/state capture and replacement, and malformed shapes. Native protocol/worklet/client passed 58/58; Node Wasm passed 9 protocol, 20 worklet, and 28 client tests. Locked Wasm builds, dependency isolation, zero imports, and the protocol-22 raw-host contract passed. Local Chrome remains unavailable, so the required Chromium/Chrome PR jobs remain mandatory in Stage 10.

- [x] Bump `shoop_audio_protocol::PROTOCOL_VERSION` and change Built-in FX wire topology to carry matching audio channel count and required MIDI.
- [x] Add concrete wire enums/structures for all Built-in FX controls, types, continuous parameters, assignments, and editor state; update stable-envelope and raw-host contract expectations.
- [x] Give each wire control a correct supersession identity and retain ordered assignment mutations; add serialization and version-mismatch tests.
- [x] Translate topology, controls, state, and assignments in `shoop_audio_worklet` and `shoop_worklet_client`; reserve/register N dry inputs, N wet outputs, and one MIDI input deterministically.
- [x] Ensure browser tracks with N internal channels remain valid even though the current physical Web Audio device boundary is stereo; reject only actual storage/protocol-limit violations.
- [x] Exercise production worklet audio and MIDI for mono/stereo/N routing, each stage, bypass, learned local/global CC, ignored notes, snapshots, processor replacement, and save/restore transport.
- [x] Verify focused protocol/worklet/client native and Node Wasm tests, locked Wasm builds, dependency isolation, generated zero-import worklet, raw-host contract, and applicable browser tests.
- [x] Commit the browser transport milestone.

## Stage 7 — Session and application persistence/migration

Depends on Stages 5 and 6.

Persistence evidence: session document 11 stores positive Built-in FX `audio_channels`, one dry MIDI channel/port, canonical state-v2, and concrete `builtin_fx_midi_cc_assignments` separate from OxiSynth mappings. Version 9 or 10 deserializes its missing count as stereo, strictly accepts only state-v1/no-MIDI, allocates collision-free MIDI port/channel IDs, preserves Reverb enable, emits state-v2 defaults, migrates recorded states, and leaves mappings empty; versions 6–8 retain their prior migrations. Current validation enforces v2 field count/tags/finite ranges, N channel shape, one MIDI, chain ownership, and unique assignment targets/sources for live/recorded state before publication. Application save/load now round-trips a three-channel rack, typed controls, assignment, exact dry/wet/MIDI channel counts, and transient visibility; backend/native/worklet transfer and replacement tests cover the same backend session fields and rollback. Native combined app/session suites passed 148/148; Node Wasm passed 34 session tests and the complete 94-test application suite (plus latest focused Built-in FX reruns). `docs/session_format_v1.md` documents version 11, migration, topology, state, mappings, and transient DSP state.

- [x] Bump `SESSION_DOCUMENT_VERSION` and change `TrackTopologyDocument::BuiltInFx` to store matching audio channel count with one required dry MIDI channel/port.
- [x] Add concrete Built-in FX CC assignment document types/fields while preserving the existing OxiSynth document representation and all previously accepted versions.
- [x] Implement deterministic migration of version-9 fixed-stereo/no-MIDI Built-in FX tracks: convert topology to two channels, add the required unconnected MIDI port and empty MIDI loop channels using collision-free IDs, migrate state version 1 to version 2, and leave mappings empty.
- [x] Validate canonical current state, finite/ranged controls, exact topology/port/channel shape, chain identity, assignment parameter/source uniqueness, and processor-specific assignment ownership for live and recorded processor states before publication.
- [x] Map expanded Built-in FX topology, state, and assignments through `shoop_app` capture, archive save/load, backend replacement, native/browser transfer, recorded FX-state handling, and sample-rate recreation.
- [x] Preserve transactional staging and rollback for malformed state, unsupported versions/types, invalid N-channel shape, assignment conflicts, missing capability, and processor construction failure.
- [x] Test deterministic current-format round trips; migration from every accepted document version; old reverb enabled/disabled preservation; 1/2/3/6-channel sessions; assignments and MIDI port connections; malformed current/live/recorded state; native/browser transfer; and tails/editor visibility remaining transient.
- [x] Update `docs/session_format_v1.md` with the new document version, topology, state grammar, assignment representation, migration, and transient/runtime-only state.
- [x] Verify focused `shoop_session` and `shoop_app` native/Node tests plus per-commit gates.
- [x] Commit the persistence/migration milestone.

## Stage 8 — Embedded editor, MIDI Learn, and documentation

Depends on Stages 5 and 7.

UI/documentation evidence: the resizable editor renders the fixed six-stage order, all enable controls, Drive/Modulation/Reverb selectors, all 23 physical-range sliders, Tremolo-disabled Feedback, state-driven values, and persistent FunDSP attribution/link. Its concrete MIDI Learn window mirrors Built-in Synth without an unhelpful cross-editor abstraction: latest local CC, flat continuous-only list, Assign, assignment rows, Remove, and Remove all. Add Track forces required MIDI and accepts matching mono/stereo/N counts. Focused native UI/application tests passed 8/8; Node Wasm passed 6 UI and 2 application tests, covering rendered order/control membership, typed selectors, stage actions, learn/assign/remove/clear, latest CC, attribution, hidden/wrong editors, and six-channel Add Track. README and Sphinx concept/usage docs cover rack/types/order/channel semantics/MIDI/bypass/attribution; `sphinx-build -W --keep-going` passed.

- [x] Expand `shoop_egui/src/builtin_fx_editor.rs` with visibly ordered stage sections, enable controls, selectors, continuous controls with units/ranges, mode-dependent disabling, and state-driven rendering.
- [x] Add a Built-in FX MIDI Learn window matching Built-in Synth: inspect the latest local input message, enable Assign only when it is a valid CC, show one flat continuous-parameter list plus assignment rows, and support Remove/Remove all; do not list toggles or selectors.
- [x] If Stage 0 justified reuse, extract and test a narrow generic MIDI Learn UI/assignment helper and migrate OxiSynth without changing its behavior; otherwise keep concrete editors and share only trivial helpers.
- [x] Preserve **Powered by FunDSP** and its working project link, editor close/reopen behavior, simultaneous Built-in Synth/FX editors, and transient visibility.
- [x] Update Add Track UI and browser capability self-tests for variable matching audio count and required MIDI; ensure mono, stereo, and higher channel requests are representable and invalid shapes cannot be submitted.
- [x] Add UI tests for every emitted control type, snapshot reflection, stage/mode enablement, flat learn list membership/exclusion, assignment lifecycle, latest-CC display, attribution, and coexistence with Built-in Synth.
- [x] Update `src/rust/shoopdaloop/README.md`, `docs/source/concept.rst`, and `docs/source/usage.trackcontrols.rst` with rack order, controls/types, mono/stereo/N semantics, MIDI/global fan-out, bypass behavior, and FunDSP attribution.
- [x] Build Sphinx with warnings denied and verify focused `shoop_egui`/`shoop_app` native and Node tests plus per-commit gates.
- [x] Commit the editor/documentation milestone.

## Stage 9 — End-to-end and final local validation

Depends on all implementation stages. Run all payloads in the environment selected by `.agents/info/build.md`; on Nix/NixOS, use `nix develop`.

Local validation evidence (2026-09-01): after merging current `origin/master` at `37364cce3485ca093633db877ec56f73f7255fac` and combining its default-playback/session/protocol work, the complete native suite passed 1,702/1,702 with four policy skips. The complete 17-package Node Wasm suite passed 1,420/1,420. Warning-denying workspace and locked app/worklet Wasm builds, formatting, test/tracing policies, dependency isolation, Wasm report tests, smoke budget, Trunk, zero-import/raw-host protocol-22 contract, and Sphinx all passed. Native application smoke now saves/reloads a three-channel rack with typed controls and assignment; browser self-test requests three-channel Built-in FX and verifies expanded state after load. Engine/worklet objective signal tests inspect reduction, RMS/selectivity, waveform differences, tails, finite bounds, stereo linking/decorrelation, and isolation rather than relying on snapshots. No Chrome/Chromium, `chromedriver`, or `geckodriver` is installed locally, so policy-triggered Chromium/Chrome/Firefox CI remains mandatory. The prompt-to-artifact audit is `/tmp/expanded_builtin_fx_completion_audit.md`; only Stage 10 PR/CI/review closure remains incomplete.

- [x] Extend native application smoke coverage to create mono, stereo, and N-channel Built-in FX tracks with MIDI; exercise representative audio for every stage/type, continuous controls, exact bypass, stale-tail reset, local/global learned CC, ignored notes, save/reload, and migrated version-9 state.
- [x] Extend browser self-test and production Worker/AudioWorklet smoke coverage with catalog/topology, representative DSP, local/global MIDI control, snapshots, save/reload, and N-channel internal routing evidence.
- [x] Add objective offline render checks for each effect and stereo specialization; inspect generated metrics/output for intended response rather than relying only on state snapshots or call counters.
- [x] Run `cargo fmt --all -- --check`.
- [x] Run `RUSTFLAGS="-D warnings" cargo build --workspace`.
- [x] Run `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`.
- [x] Run `python3 scripts/check_shoop_test_usage.py`.
- [x] Run `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Run `cargo check --locked --no-default-features -p shoopdaloop --target wasm32-unknown-unknown` and `cargo build --locked -p shoop_audio_worklet --target wasm32-unknown-unknown`.
- [x] Run `python3 scripts/check_worklet_client_dependencies.py --target wasm32-unknown-unknown`, inspect relevant `cargo tree` output, and verify FunDSP/new rack code does not introduce disallowed worklet dependencies.
- [x] Run `python3 scripts/run_wasm_tests.py --runtime node --profile dev`.
- [x] Run `python3 scripts/run_wasm_tests.py --runtime chrome --profile dev` when Chrome is available; otherwise record the local limitation and require the corresponding PR matrix job.
- [x] Run `python3 -m unittest scripts.tests.test_wasm_test_report` and `python3 scripts/check_wasm_smoke_budget.py`.
- [x] Run `trunk build` from `src/rust/shoopdaloop`, verify the generated worklet remains import-free through the existing contract, and run applicable browser smoke commands from its README.
- [x] Run `sphinx-build -W --keep-going docs/source _build`.
- [x] Build a prompt-to-artifact completion checklist mapping every goal, scope item, immutable criterion, named file, command, test, migration, target, PR gate, and review item to inspected evidence. Treat uncertainty as incomplete and fix or verify every gap.
- [x] Inspect `git diff --check`, `git status`, changed-file scope, dependency trees, and generated-artifact exclusions; commit final corrections and rerun affected/full gates until the worktree is clean.
- [x] Commit the final validation milestone.

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
