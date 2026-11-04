# Session and media conversion to the engine format

Written using [write_plan.md](.agents/prompts/write_plan.md), continuing
[the fixed-quantum redesign](PLAN_FIXED_QUANTUM.md). Planning baseline:
`3ed8ff118f5a06d650be05c66c810729be8097a6` on
`feat/fixed-quantum-runtime`. Reuse that branch and draft
[PR #930](https://github.com/SanderVocke/shoopdaloop/pull/930), targeting
`feat/backend-replacement`.

## Goals and scope

Allow supported sessions and media to enter an engine with a different sample
rate or processing quantum through one explicit, offline conversion. Cover session
load, WAV/loop-audio import, exact-MIDI/SMF import, retained channel content and
sync dependents, including sessions recorded at Q=16 loaded into Q=32 or Q=64.
Reuse the existing persistence, codec, worker and publication paths.

Corrections use one algorithm regardless of their size. No detailed preview,
pitch-preserving stretch mode or automatic engine-format switching is required.
Unsupported document features remain unsupported; this work does not enable FX,
composites or new kinds of sync topology. Performance experiments and broader
backend parity remain tracked in the parent plan.

## Immutable acceptance criteria

1. Every supported, otherwise valid input can be converted within existing
   resource limits. Unaligned logical lengths alone are not a refusal reason.
   Zero-length loops remain valid; nonempty loops become positive multiples of Q.
2. Related loop lengths follow one dependency-aware plan. Exact source-cycle
   multiples survive; fractional relationships receive deterministic nearest-grid
   approximations. Sample-rate and quantum conversion are planned together.
3. Audio is resampled once from original input, with coherent channel phase and
   compensated filter delay. MIDI bytes, initial state and stable event ordering
   survive; timestamps retain frame precision inside a quantum. Identity mappings
   preserve audio samples and exact MIDI content bit for bit.
4. Signed offsets, capture alignment, preplay, stored history and retained channels
   remain coherent with playback. No events or media are silently discarded just
   to satisfy the grid. Existing deliberate import crop/pad semantics still apply.
5. One explicit conversion warning covers sample-rate changes and/or retiming;
   cancellation leaves the running session untouched. There is no per-loop preview
   or algorithm selector. Native, browser and scripted entry points follow the
   same lossiness policy.
6. A live import publishes all affected loops atomically at a tick boundary.
   Unrelated playback and controls survive. Failure, stale preparation or rejected
   active capture leaves the previous live content intact. No conversion, codec
   work, allocation, destruction of owners, blocking or I/O enters the RT thread.
7. Source files are untouched. Normal saves persist converted media and timing;
   reloading in the same engine format is an identity operation. Older sessions
   without quantum metadata continue to load.
8. All production transports carry the authoritative target format and required
   fences. Capacity, cancellation, connection-loss and ownership guarantees are
   preserved, with automated native/browser coverage.
9. Implementation and local automated validation finish before pushing or working
   on CI. CI, automated PR review and human acceptance are the final stage.

## Design rules

### One exact timing plan

Use `ConversionTarget { sample_rate, quantum_frames }`, read from the prepared
engine, and an immutable `ConversionPlan` containing checked rational timing maps,
affected loops/media, dependency order and publication fences. Plan before
resampling. Do not compose the existing independently rounded resampling helpers.

First construct the intended post-import candidate using existing channel mapping
and keep-length/update-length semantics. For a session load, that candidate is the
decoded session. Express each candidate duration I in target-rate frames without
rounding: I = source_frames * target_rate / source_rate.

Resolve the supported production sync forest using the existing topology rules;
self-sync is a root. Visit sources before dependents, independent of document order:

```text
align(0) = 0
align(x > 0) = max(Q, nearest multiple of Q to x; ties upward)
root or empty sync source: N = align(I)
dependent of nonempty source s: N = align(I * N_s / I_s)
```

Use original candidate ideals throughout; never compound already-rounded media
conversions. Only sync dependencies transmit the duration correction, not routing
or trigger edges. An empty source has no defined period, so its children align
independently. Retain dimensionless cycle/repetition counts; recompile derived
timing from the final periods. Do not broaden document/topology acceptance.

For example, at 48 kHz, Q=16 to Q=32: 48,016 -> 48,032 frames;
its four-cycle child 192,064 -> 192,128; that child's three-cycle child
576,192 -> 576,384. The last correction is 192 frames (4 ms), but every loop
receives the same approximately 0.033% duration change. Larger absolute downstream
corrections use the same conversion path.

### One media transformation

Extend the existing windowed-sinc converter to accept an exact output length and
playback origin. Combine sample-rate scaling with the loop's automatic correction
N/I in one pass from original samples. A deliberate import length edit first
defines I; it does not implicitly tempo-fit every retained channel or dependent
to the newly imported file. Keep-length imports retain the existing aligned window
and normal crop/pad behavior; unaligned raw file length alone requires no retiming.

For each channel, let a be its total original-to-output frame scale and let
O = start_offset + capture_alignment. Use the anchored map:

```text
O' = ceil(a * O)                       // signed exact ceil
audio source position at output j = O + (j - O') / a
MIDI event at original frame e -> O' + floor(a * (e - O))
```

Convert the combined origin first, then derive consistent component offsets.
Preserve prefix/suffix history and half-open playback windows, including negative
pre-roll; equal mapped MIDI times retain original order. MIDI timestamps are not
rounded to Q. Raw channel buffers need not be Q-aligned; logical loop lengths do.
For empty loops or media without a loop period, apply sample-rate scaling alone.
Identical maps may share converted media; different maps/origins require distinct
versions even if their original media ID was shared.

Use identical filter phase for related audio channels and compensate filter delay.
Use periodic filter context only for genuinely whole-loop media; otherwise use
stored context and existing missing-sample zero semantics. Do not bake in new
crossfades. Musical frame metadata follows the timing map; hardware calibration
scales only with sample rate. Wall-clock dates, IDs, gains and routes are unchanged.
Use checked rational arithmetic and bounded, cancellable off-RT work. Validate
ratios, lengths and allocation limits explicitly; do not silently clamp the ratio
or truncate a converted tail to hide an error.

### Publication and consent

Session load converts the whole incoming candidate, then uses existing atomic
replacement; loaded loops start stopped. A live length-changing import stages a
bounded affected-family content transaction, including changed retained media and
required derived topology. Extend the current single-loop `ContentApply` path;
whole-session replacement would stop unrelated playback and is unsuitable here.

Snapshot and check target rate/Q, session epoch, loop identities/generations,
dependency links, lengths and content revisions before publication and again at
RT commit. Publish once after complete preflight; return all rejected/retired
owners through the existing control-side reclamation path. Preserve current
unrelated controls rather than replaying an old full-session snapshot. Map playing
positions through the candidate timing map, then floor to an in-range Q boundary;
retain cycle/delay counts and absolute host-clock scheduling semantics. Reject
affected growing captures according to replacement policy without stopping
unrelated recording. Preserve exactly-once accepted commits across UI loss/reconnect.

Generalize the existing sample-rate confirmation to one conversion confirmation:

> This import requires resampling or retiming to the session's sample rate and
> processing quantum. This will slightly affect audio, MIDI timing and synchronized
> loop lengths.

Offer Convert/Cancel before transformation/publication. Prompt for a sample-rate
change or an actual nonidentity timing plan; differing recorded Q alone is not a
reason if all timings already fit. Keep existing independent format-loss warnings.
APIs/scripts must explicitly authorize the same conversion. Exercise this user
flow automatically during development; engineering human review stays last.

## Implementation stages

Dependencies: 1 -> 2 -> 3; stage 4 needs stage 2 and can precede completion of 3.
Stage 5 joins 3 and 4; then 6 -> 7 -> 8. Commit milestones locally throughout;
do not push them until stage 8, since the existing PR would start CI.

### Progress and field contract

The initial milestone adds the shared rational planner and anchored, incremental
48-tap windowed-sinc audio/MIDI converter. Application status now reports the
prepared engine quantum separately from host callback size; application wire
version 14 fences the changed snapshot schema. Production whole-session load now
uses this conversion through native codecs and the browser worker, with one generic
Convert/Cancel warning. Atomic live-family imports are now integrated through the
backend, service, native codecs and browser worker. Conversion handles
physical versus musical metadata, context-sensitive shared-media splitting,
MIDI collision ordering, cancellation and converted save/reload idempotence.

Initial milestone validation: 2,613 native tests pass, with four host-facility
skips; the warning-denying workspace build, formatting, test-attribute and closed
tracing inventory checks pass. Local socket tests require network permission in
the execution sandbox. Logs: `work/retiming-core-native.log`,
`work/retiming-stage1-build.log` and `work/retiming-core-session-tests.log`.

Workflow milestone validation: 2,615 native tests pass, with four host-facility
skips; 14 conversion tests and 12 codec/worker tests pass in Node/Wasm. The browser
application checks successfully for Wasm and the warning-denying workspace build
passes. End-to-end tests cover same-rate Q=16-aligned media loaded at Q=32,
combined rate/quantum conversion, preserved sync multiples, cancellation and
conversion-free reload. Logs: `work/retiming-workflow-native.log`,
`work/retiming-session-workflow.log`, `work/retiming-core-node.log`,
`work/retiming-codec-node.log` and `work/retiming-io-wasm-check.log`.

The workflow test also exposed a callback-start restriction on complete session
replacement. A configured fixed planner now permits replacement at any fixed tick,
including when host callbacks are smaller than Q. The component test checks
mid-callback tick publication, retained old exports and MIDI cleanup together.

Atomic runtime milestone: the bounded family transaction now stages media without
changing playback, then publishes periods, audio, MIDI and mapped playheads at one
fixed tick. RT protocol version 6 carries family staging and authoritative content
revisions, including changes that keep the same length. Core and engine tests
cover Q=16/32/64, callbacks smaller than Q, stale revisions, unrelated recording
and gain changes, cancellation, resource reclamation and disconnects before and
after admitted commits. The subsequent integration milestone connects this cut
through the backend, service and import coordinator.

Atomic runtime validation: 2,621 native tests pass, with four host-facility skips;
all three engine family tests also pass in Node/Wasm. The warning-denying workspace
build, formatting, test-attribute and tracing inventory checks pass. Logs:
`work/retiming-family-native.log`, `work/retiming-family-node.log` and
`work/retiming-family-workspace-build.log`.

Live integration milestone: the frontend plans from authoritative resolved sync
links, captures only changed loops, retains original file input through consent
and context capture, then converts retained and imported media once. A bounded
family manifest and backend operation carry format, identity, dependency, length
and revision fences to one RT cut. Unrelated recording and current controls
survive; session loads still use full replacement and start stopped. Native and
the real browser codec Worker use the same planner and transformation. Cooperative
capture release now precedes subsequent capture/staging, including synchronous
Wasm completion and cancellation under queued delivery.

Completed local validation: 2,630 native tests pass (four host-facility skips);
16 file-workflow tests pass in Node and 17 in Chromium, including the real Worker
and large media. Four engine and three adapter family tests pass natively, in Node
and Chromium; two family codec/worker tests pass natively, in Node and Chromium.
They check Q=16/32/64, retained audio/MIDI/history, combined SRC, three-level sync
propagation, exact event frames, current gains/playback, unrelated recording,
stale format/content/dependencies, affected capture, cancellation, bounded ownership
and controller loss. Production application/codec/worklet Wasm builds and artifact
contracts pass, as do the warning-denying workspace build, formatting,
test-attribute, tracing, four-target boundary and runtime documentation checks.
The import-free worklet is built independently of the application to avoid Cargo
feature unification pulling browser bindings into the worklet artifact.

Evidence: `work/retiming-complete-native.log`,
`work/retiming-live-family-node.log`, `work/retiming-live-family-chromium.log`,
`work/retiming-family-stack-node.log`,
`work/retiming-family-components-chromium.log`,
`work/retiming-family-engine-chromium-final.log`,
`work/retiming-live-codec-node.log`, `work/retiming-live-production-wasm.log`,
`work/retiming-live-artifact-contract.log` and
`work/retiming-live-family-workspace-build.log`. Stages 1–7 are complete;
push, CI, incoming automated review and human acceptance remain stage 8.

Timing field classification: loop lengths, channel offsets, combined playback
origin, capture alignment, preplay, raw audio/MIDI extents and event frames follow
the musical map. Track/processor manual latency and port history capacities follow
sample-rate scaling only. Cycle/delay/repetition counts remain dimensionless;
absolute engine-clock deadlines retain their existing scheduling meaning. Names,
identities, controls, routes and recording dates are unchanged.

The sinc kernel evaluates explicit source positions rather than using the old
rate-only wrapper's truncated/padded output. This supplies fractional playback
origins and exact output bounds while retaining its 48-tap, squared Blackman-Harris
window and 256-phase linear interpolation; it is one path for every correction.

### 1. Contracts, authoritative format and fixtures

- [x] Inspect the existing conversion/publication paths and reuse the current
  implementation branch and draft PR.
- [x] Specify conversion types and enumerate timing fields by musical, physical
  or dimensionless meaning; expose authoritative engine Q alongside sample rate
  through service/app status and native/browser worker inputs.
- [x] Snapshot target format and relevant revisions; define coordinated protocol
  version changes and explicit conversion authorization for API/Lua callers.
- [x] Verify: format/status round-trips, default/old-session handling, malformed
  target refusal and fixtures for identity, Q=16 -> 32/64 and combined SRC.

### 2. Dependency-aware rational planner

- [x] Build the post-import candidate and resolve its supported sync forest;
  implement checked alignment, source-first propagation and per-channel maps.
- [x] Identify the full affected family, retained media, required topology and
  identity/deduplication cases without allocating output audio yet.
- [x] Verify with independent arithmetic expectations: chains, multiple roots,
  shuffled declarations, self-sync, empty/silent/MIDI-only loops, exact multiples,
  fractional ratios, ties, minimum lengths, overflow and all supported Q values
  (1/16/32/64/128). Include the 48,016-frame example and 44.1 -> 48 kHz cases.

### 3. Phase-coherent audio, MIDI and metadata conversion

- [x] Implement exact-length, origin-aware sinc resampling and the shared anchored
  MIDI/metadata map; retain original inputs until conversion succeeds or cancels.
- [x] Handle signed origins, history, periodic versus stored context, shared assets
  with different maps and bounded cooperative conversion work.
- [x] Verify identity bit equality; impulse/transient position, constant/sine
  behavior, stereo phase, filter delay and loop seams against independent oracles.
  Check MIDI start state, boundary events, timestamp collisions, negative pre-roll,
  prefix/suffix preservation and cancellation/resource limits.

### 4. Atomic affected-family publication

- [x] Extend bounded backend/protocol/engine content staging to multiple affected
  loops with one preflight/commit, including derived timing and playhead mapping.
- [x] Carry fences and ownership through in-process, socket/shared-memory and
  worklet routes; keep aligned-input validation as the final RT boundary.
- [x] Verify no partial family is observable, exact tick publication, active
  playback continuity, unrelated control edits/recording, capture refusal, stale
  format/topology/content, saturation and complete reclamation with RT heap guards.
  Cover cancellation and connection loss before and after commit.

### 5. Wire session and standalone import flows

- [x] Replace rate-only persistence orchestration with plan -> authorize ->
  convert -> prepare -> fenced publish, sharing the Rust planner across native
  codecs and the browser worker. Resolve channel mapping/length choice before
  planning; coordinate worker/protocol schema changes.
- [x] Route session loads to whole-session replacement and live imports to the
  affected-family transaction. Cover retained audio/MIDI, sync-source imports,
  empty loops, structure-only loads and generated-media insertion paths.
- [x] Save converted actual media and lengths without changing source files;
  accept older sessions without quantum provenance. Never infer correctness from
  recorded Q alone or change the prepared engine's format to fit a file.
- [x] Verify WAV, loop-audio, exact MIDI and SMF workflows for both length choices,
  mixed retained/imported content, combined rate/Q conversion, save/reload
  idempotence, stale tasks and worker transfer/cancellation bounds.

### 6. Unified conversion warning and API behavior

- [x] Generalize rate-only task states, warning data and confirmation intents;
  show one Convert/Cancel warning for rate and/or timing changes with no preview.
- [x] Apply the same explicit authorization to script/API imports. Preserve
  independent format-loss warnings and existing task/error reporting.
- [x] Verify automatically in native and browser app tests: accept/cancel,
  identity/no prompt, aligned old-Q/no prompt, combined conversion/one prompt,
  required authorization, stale confirmation and reconnect reconciliation.

### 7. Complete local end-to-end validation

- [x] Run production session/import/save/reload scenarios at Q=16/32/64, including
  sync chains and an affected playing family beside unrelated playback/recording.
  Check audio/MIDI/timing and atomicity against independent expected results.
- [x] Run targeted suites throughout, then the full native workspace suite,
  Node/Chromium tests, production application/worklet Wasm builds and artifact
  contracts using [.agents/info/test.md](.agents/info/test.md). Run formatting,
  warning-denying builds, test-attribute, tracing and boundary/document checks.
  Keep workspace builds and native integration execution sequential to avoid
  replacing a test-enabled host executable during tests.
- [x] Record local evidence and remaining limitations, update the parent plan and
  commit the completed milestones. No CI investigation or human acceptance yet.

### 8. Final delivery, CI and human acceptance — last

- [ ] Only after stages 1–7 pass: push local commits to the existing branch and
  update draft PR #930 with the final scope, evidence and conversion policy.
- [ ] Now investigate both existing and new CI failures; reproduce and fix their
  causes without weakening assertions or refreshing behavioral fixtures without
  semantic validation. Repeat required native/browser CI until green.
- [ ] Check incoming automated PR review and address feedback through approval;
  rerun affected local/CI checks after changes.
- [ ] Finally request human review and acceptance on representative sessions and
  devices, including the warning and synchronized-loop result. Record outcomes
  and remaining parent-plan device/performance gates. Do not merge without an
  instruction to merge.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
