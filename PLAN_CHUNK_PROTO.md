# Plan: three benchmarkable audio-render variants

Status: all three render paths, the app workflow, native measurements and local
validation are complete. Final CI/review evidence is tracked by the live checks
and review feed in [PR #959](https://github.com/SanderVocke/shoopdaloop/pull/959).
See [the measured report](docs/design/chunks/RESULTS.md) and
[raw-data manifest](docs/design/chunks/results/manifest.json).

Base: `feat/backend-replacement`, inspected at
`7a77c8d5048d6ec33c305385710dd40044d039b7`. The checkout was refreshed before
implementation from the original planning revision `b874bbbb`; the current base
includes upstream Pi idle/control optimizations, bus FX restoration, and native
audio-driver switching. Before final measurements it was refreshed again to
`888bd8a66143a6a6a675883823578a7c1d7e4bee`, including composite integration
merged upstream while this prototype was in progress. This plan follows that branch's
`.agents/prompts/write_plan.md` and the subsequent user decisions. It takes
precedence over earlier exploratory implementation sequencing in
`IDEA_CHUNKS.md` and `docs/design/chunks/DESIGN.md` where they differ.

## Goal and scope

Deliver an opt-in native implementation that can be driven by the normal app,
including a remote app connection, and benchmarked in three modes from one build:

| Mode | Implementation |
| --- | --- |
| `current` | The existing replacement backend's audio path. |
| `chunks` | Immutable persistent audio chunks, a per-quantum render arena, and reference-based routing; existing gain/ramp semantics. |
| `chunks-descriptors` | The same chunk implementation with immediate composition of gain/ramp descriptors and the accepted endpoint-preserving ramp approximation. |

Reach an actual host benchmark as early as possible. Experimental private APIs,
special cases for engine-owned nodes, and duplicated implementation code are
acceptable. Do not make a generic framework, complete feature parity, protocol
redesign, or additional analytic modeling a prerequisite for the first result.

### Immutable acceptance criteria

These criteria may not be changed without explicit user approval.

1. A normal app can connect to `shoop_host` and use each mode. Mode selection is
   opt-in at host startup; leaving it unset preserves the normal current mode.
2. All three modes support the normal basic audio workflow: the default sync
   loop; adding tracks and loops within configured capacities; input/output
   routing and monitoring; recording; playback and repeated wraps; stop/start;
   clearing or replacing recordings; gain, balance, mute and the existing basic
   transitions; grabbing recent audio; and grabbing with continued recording.
   Ordinary removal and recreation of tracks/loops must also work.
3. The app receives the state, meters, and audio/waveform readback needed for that
   workflow. All controls above use existing app and RT requests. A bare engine
   harness alone is not completion.
4. The existing dummy, JACK, and CPAL host integrations remain the device
   boundaries. Do not create a new audio driver or device adapter. Dummy is the
   mandatory deterministic benchmark target; existing real drivers must continue
   to build and receive native smoke coverage where devices are available.
5. `chunks` follows the current backend's audio and timing behavior. Supported
   deterministic cases must match frame counts, capture cuts and transitions
   exactly; sample comparison initially uses `atol=1e-6, rtol=1e-6` for ordinary
   f32 rounding, without shifted or missing samples.
6. `chunks-descriptors` keeps those timing and ownership guarantees, while
   deliberately permitting the agreed interior-ramp approximation. The composed
   gain at the first/last sample of each quantum matches the ideal stack under
   the same f32 tolerance. Q=1 uses its intended single gain. Other changes to
   the audio result must be explained by that approximation or fixed.
7. Grabs and retained snapshots survive continued capture and history wrap.
   Additional sharing never requires copying old payloads to keep capture
   running. Multiple recordings of the same capture point share the produced
   persistent payload where applicable.
8. New render-path code uses prepared capacities and bounded work. It performs
   no general heap allocation/deallocation, blocking locks, or I/O in the audio
   callback. Lower crates remain `no_std + alloc` and retain their unsafe-code
   prohibition. Capacity failures cannot expose partial edits or corrupt owners.
9. Reproducible host-level benchmarks exercise all three modes and export raw
   callback measurements, configuration/mode identity, summary statistics and
   relevant memory-work counters. Produce native results and Pi 4 reproduction
   instructions; run Pi measurements when hardware is available and explicitly
   mark them pending otherwise. No speedup threshold is assumed in advance.
10. Neither separate grab-history arenas nor custom per-source history-index
    specialization is implemented. Histories use the common persistent chunk
    storage and index with circular logical addressing.

### Allowed shortcuts and exclusions

- MIDI functionality is not an acceptance target. Disable or stub it in the
  experimental audio-only profile, retaining empty compatibility objects when
  cheaper than removing them. Use the same MIDI policy in all benchmark modes.
  Do not spend a milestone redesigning MIDI or its protocols.
- Composite arrangements, synth/plugin hosting, advanced dry/wet editing,
  persistence polish, and web/embedded integration are not required to complete
  this prototype. Existing processors may use materialized-buffer adapters if
  inexpensive. Do not silently accept unsupported operations that alter the
  supported workflow or silently fall back to another render mode.
- Keep state/request-array work and unrelated optimizations consistent across
  modes. Solving those independently is outside this comparison.
- Internal audio-backing assumptions and native-only startup overrides need not
  appear in transport schemas, version negotiation, or UI settings. Unsupported
  combinations must still produce a clear startup or operation error.

## Design rules

### Mode selection and integration

- Use `SHOOP_AUDIO_RENDER=current|chunks|chunks-descriptors` initially. Read it in
  native startup code, validate it once, and pass a typed preparation choice into
  the engine/runtime. No environment reads or mode switching inside callbacks.
- Provide a common explicit audio-only benchmark profile, for example
  `SHOOP_AUDIO_BENCH_AUDIO_ONLY=1`. The launcher sets it for all comparison runs;
  experimental modes require this profile initially. Normal unset-mode startup
  must retain its existing functionality.
- Preserve mode/profile selection through native driver preparation, retries,
  and any subprocess setup used by a supported launch path. A private local
  preparation field or child-local environment read is sufficient; do not add
  wire fields just for the experiment.
- Log requested and actual mode, driver, build SHA, quantum, callback size,
  capacities and experimental limitations. Benchmark startup must reject a
  driver fallback that would invalidate the requested comparison.
- Reuse the current app service, `RtBackend`, RT endpoint, loop resolver and
  topology commands. Keep gains/state/timing authoritative in the same places.

### Chunk storage and reference routing

- Use `AudioChunk`, `ChunkRef`, `ChunkSpan`, `AudioSequence`,
  `PersistentAudioPool`, and `RenderArena` as the shared terminology.
- Start with exactly one quantum per payload chunk and a prepared channel-group
  layout. Use typed sample slabs, indexed slots and free lists. No individual
  heap object per chunk and no arbitrary variable-duration allocator.
- Produce fresh persistent chunks and publish them immutable. Use the proposed
  small fixed-fanout index; four-way nodes are a starting implementation choice,
  not a reason to delay the first benchmark for another comparison study.
- Budget index staging and streaming path retirement separately from bulk
  deletion. Deleted owners remain charged until physical reclamation completes.
  Reserve continued-capture headroom even with retained grabs and queued deletion.
- Connections borrow flat, bounded sequences. Optimize silence and single-span
  cases. Owners, not individual graph edges, retain persistent audio.
- Reset the arena once per graph quantum, after consumers and host-output
  recovery. Existing buffering adapters must own any samples kept across quanta.
- Use the scoped read-view/writer pattern demonstrated by the borrowing probe.
  Multi-output processors can use a contiguous transient batch. A retained output
  may pay one explicit capture copy if direct persistent production is awkward.
- Existing effects, smoothing operations and other sample consumers can request
  materialization. Bridge only where needed; basic playback/routing must not
  secretly round-trip through the old page-backed audio storage.
- Bulk readback/export can stream chunks through existing byte-oriented APIs.
  Full-length conversion must not become an unbounded callback operation.

### Descriptor variant

- One logical output carries an `AudioSequence` and one gain-envelope pair per
  independently controlled channel. There is no growing stack of arithmetic
  operations passed through the graph.
- A gain/ramp node immediately composes incoming and local endpoint gains:
  `out.start = in.start * local.start`, and likewise for `end`. Identity uses
  `(1, 1)`; constant gain uses equal endpoints. This work touches no samples.
- Define the resulting envelope as linear over the fixed graph quantum. Derive
  actual first/last sample gains from the existing ramp convention. This is the
  explicitly accepted approximation to products of overlapping ramps.
- Calculate the sample step once at materialization, increment gain in the
  sample loop, and honor the final endpoint. Partial spans evaluate the same
  quantum envelope; callback or chunk fragmentation must not resegment it.
- A general mixer consumes its input descriptors, evaluates gains while mixing
  into a fresh transient chunk, and forwards that chunk with a unity envelope.
  Mixing independent signals is not collapsed into one original chunk reference.
  Contributions from the exact same span may be coalesced, but this is optional.
- Required effects/capture/meter consumers observe the correct point in the
  graph. Meter taps may evaluate a descriptor without allocating an audio
  buffer; optimizing away their scans is not required. Keep equivalent meter
  policy in every benchmark mode.
- Boundary smoothing or other operations that are not simple gain envelopes
  may remain materialization boundaries. No general lazy expressions, polynomial
  envelopes, descriptor growth through mixing, or endpoint/device-side deferred
  composition is required.
- Descriptors remain opt-in and independently benchmarkable. `chunks` must not
  inherit their approximation or depend on their implementation.

## Code map

| Area | Intended work |
| --- | --- |
| `shoop_host/src/main.rs`, `engine.rs`, `serve.rs` | Native startup selection, common benchmark profile, existing driver launch, normal app-service connection. |
| `shoop_rt_host_native/src/lib.rs`, `cycle.rs`, `dummy.rs` | Carry the preparation choice, retain current I/O/timing boundaries, expose bounded benchmark collection and deterministic input. |
| `shoop_rt_engine/src/{description,lib,history,render,graph,node}.rs` | Mode preparation, storage wiring, reference-aware engine nodes, gain/mixer behavior, normal state publication. |
| `rt_runtime/src/{buffer,processor,executor}.rs` and `resource/` | Experimental chunk backing/index, borrowed sequence/writer access, arena, reference routing, bounded retirement. |
| `shoop_loops/src/audio.rs`, `runtime/{processor,shared}.rs`, `media/` | Chunk-backed playback/capture/grabs and retained-owner/readback adapters while reusing loop timing. |
| Existing engine/backend/native/host test harnesses | Parameterized supported audio workflows and actual app-protocol host coverage. |
| `scripts/` and benchmark examples | Reproducible variant launcher, actual host-callback collection, machine-readable results and Pi instructions. |

Placement can be simplified or consolidated as implementation reveals the
shortest sound route. Do not fork the app or replace driver integrations to avoid
an internal interface change.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Follow branch build/test instructions and Rust test-attribute rules. Before
  committing Rust changes, run `cargo fmt --all` and a warning-denying build.
  Run `python3 scripts/check_shoop_test_usage.py` when Rust tests change.
- Publish milestone results promptly. Do not wait for framework cleanup, all
  platforms, or the descriptor variant before exposing the first useful chunks
  measurements. A partial milestone is not completion of this plan.

## Implementation stages

### Stage 0 — Branch, baseline and benchmark contract

Depends on: nothing. No production behavior changes yet.

- [x] Create an implementation branch such as `proto/audio-chunks` from the
  inspected replacement branch, preserving the existing design files and user
  changes. Record the exact baseline SHA and retain a clean baseline build/ref.
- [x] Review existing preparation and app workflow seams in the code map; choose
  the minimal private mode/backing interfaces and document unsupported features.
- [x] Include the refreshed upstream optimizations equally in all variants.
  Audit the restored driver-switch and bus-FX paths for mode preservation or
  explicit experimental limitations. Capture fresh baselines from this base,
  not the earlier planning revision.
- [x] Freeze a small benchmark configuration: stereo, Q=32, 48 kHz, 256-frame
  callbacks; include both the default empty sync session and active audio cases.
- [x] Specify raw-result fields, warmup, fixed callback/sample counts, common
  telemetry/profiling policy and reference sample expectations. Add Q=1 as a
  correctness case and larger callback partitions later.
- [x] Record one current-mode host run using the existing dummy driver so there
  is an empirical baseline before implementation changes. Do not use an analytic
  byte model or the offline example as the sole performance baseline.
- [x] Verify: normal host startup, app attach and one record/play/grab workflow;
  save configuration, build identity and raw baseline results. Commit milestone.

### Stage 1 — Opt-in modes and an actual-host comparison harness

Depends on: stage 0. This stage establishes the common path used by later modes.

- [x] Add startup parsing and the typed local preparation choice. Validate bad
  mode names and reject unavailable experimental paths instead of falling back.
- [x] Add the common audio-only benchmark profile. Neutralize MIDI at the least
  expensive layer; keep public request schemas and basic audio creation intact.
- [x] Reuse `DummyConfig::input` for an optional deterministic non-silent signal
  varying with frame and channel. Retain silence as a separate idle case. The
  current testing signal hook exists; extend/reuse it without a new audio driver.
- [x] Build a launcher/controller around the normal host/app connection, following
  the attach/command pattern in `shoop_host/tests/recovery.rs`. Script ordinary
  app commands and confirmations; use the same path the remote app uses.
- [x] Collect actual callback elapsed times at the existing host boundary. Use
  prepared bounded storage; drain/serialize outside rendering. Count dropped
  observations. Separate deadline overruns from late callback delivery.
- [x] Verify: current default remains normal, current audio-only works, actual
  mode/driver identity is recorded, input is nonzero/channel-distinguishable,
  and benchmark collection has no callback heap/I/O. Experimental modes may
  remain explicitly unavailable until their implementation stages. Commit.

### Stage 2 — Chunk-backed capture, history, grabbing and playback

Depends on: stage 1. Keep materialized graph buffers initially where faster.

- [x] Implement prepared persistent sample/index slabs and validated handles.
  Add ownership, fixed-path updates, trimmed range snapshots, logical silence,
  guard capacity, and bounded streaming/bulk reclamation.
- [x] Add an experimental internal audio backing selected at preparation. Route
  required history and loop-media operations through it without new wire types.
- [x] Connect normal recording, stop/play, wrapping, clear/replace, grabs and
  grab-and-continue to chunks. Keep the existing resolver and sync behavior.
- [x] Adapt readback and snapshot ownership used by the app. Return real audio
  through the existing bulk APIs, with bounded work per callback.
- [x] Add the mode-specific allocation/copy/index/retirement counters required
  to distinguish new payload writes from metadata work and compatibility copies.
- [x] Verify focused storage invariants: Q=1 and Q=32, wrap with several retained
  grabs, overlapping ranges, source isolation, slot reuse, snapshot isolation,
  admission refusal, deletion during capture, and continued recording after grab.
- [x] Verify one normal app-protocol track/loop creation and record/play/grab flow
  with nonzero readback. Parameterize relevant existing engine workflow tests.
- [x] Run and publish first current/chunks host measurements. Label this milestone
  as storage-only if the graph still copies buffers. It does not complete the
  `chunks` variant. Commit milestone.

### Stage 3 — Reference graph and complete chunks-only app workflow

Depends on: stage 2. This is the first fully implemented experimental variant.

- [x] Add the arena, silence/single-span/flat-sequence representations, scoped
  readers/writers and contiguous transient output batches to the actual runtime.
- [x] Change chunk-mode graph connections to forward references. Remove obsolete
  audio clears/copies on reference-only paths; leave current mode's path intact.
- [x] Make loop playback publish spans directly. Update engine input/output
  stages, loop faders and buses to materialize their ordinary processing as
  needed. Keep current gain/ramp semantics in `chunks`.
- [x] Integrate normal dynamic track/loop creation/removal and graph activation.
  Refresh descriptor/arena capacity admission when prepared topology changes.
- [x] Recover samples through existing host output paths and buffering adapters
  before arena reset. Keep adapters for supported sample-consuming processors.
- [x] Verify the complete accepted audio workflow through `shoop_host` and the
  normal app connection. Compare current/chunks output and state at the recorded
  tolerance; exercise routing fan-out, mixing, monitoring, balance, mute, ramps,
  wrap smoothing, retained grabs, and deletion/recreation.
- [x] Check callback partitioning, stale transient references and sample/metadata
  capacity failures; use existing allocation guards around processing.
- [x] Publish repeatable current/chunks results including reference-forwarded
  edges, materializations and compatibility copies. Confirm supported flows no
  longer use the old page-backed capture path. Open/update a draft PR with this
  first complete variant and its measurements. Commit milestone.

### Stage 4 — Independently selectable arithmetic descriptors

Depends on: stage 3. Reuse all chunk storage, timing and app-control integration.

- [x] Add fixed-size per-channel endpoint envelopes to logical audio outputs in
  `chunks-descriptors` only. Make each eligible gain/ramp node compose the pair
  immediately, passing one descriptor onward rather than accumulating operations.
- [x] Use the existing gain/balance/control-ramp values at each quantum's actual
  first and last samples. Split longer ramps at quantum boundaries; handle Q=1
  and spans with offsets using the same domain.
- [x] Implement final incremental gain evaluation. Mix input descriptors directly
  into the mixer's new chunk and emit a unity descriptor. Do not defer a mix tree
  or postpone all composition to the device output.
- [x] Preserve capture and meter observation points; materialize non-envelope
  smoothing/effect operations where needed. Keep these policies explicit and
  bounded. Stored audio contains the evaluated capture result.
- [x] Verify independent-source fan-in, chains of gains, constant/ramp mixtures,
  overlapping ramps, stop/start fades, balance/mute, intermediate capture and
  metering, and multi-quantum ramps. Compare gain endpoints against an independent
  product oracle; intentionally do not require equal interior curves.
- [x] Verify unchanged timing and grab ownership across all three modes, and
  identical results for unity/constant-gain cases within tolerance. Ensure chunk
  span or host callback partitioning does not alter the approximation segment.
- [x] Run all three through the app workflow and benchmark launcher. Confirm
  sample-work counters distinguish eliminated gain buffers from remaining mixes,
  meters, effects and captures. Commit milestone.

### Stage 5 — End-to-end benchmark matrix and user reproduction

Depends on: stage 4. This is the final functional/measurement acceptance gate.

- [x] Exercise all three modes through the real host/service/RT stack and an
  actual app client. Include a manual remote UI check of track/loop creation,
  input monitoring, recording, play/stop, gains, meters, waveforms and grabbing.
- [x] Add reproducible scenarios: empty default sync session; silent and non-silent
  capture; multiple playing loops; recording while playing; heavily shared grabs;
  grab-and-continue; repeated deletion/recreation; near-capacity retention; and
  simultaneous active control ramps. Confirm each setup was accepted and reached
  the intended active graph before measuring it.
- [x] Compare fixed configurations across modes: same build flags, driver, audio
  profile, input, capacities, sample rate, Q, callback size, script, telemetry,
  collection settings and run duration. Record actual memory reservations.
- [x] Warm up explicitly, repeat runs and rotate their order. Capture p50/p95/p99,
  observed maximum, budget-overrun count, callback-delivery lateness, collection
  drops and resource high-water marks. Do not drop slow callbacks from reports.
- [x] Include idle, periodic other-core activity and a reproducible other-core
  memory workload. Keep host/core/IRQ settings external and recorded; do not
  change PREEMPT_RT or isolation settings silently.
- [x] Report payload and index traffic separately, plus arena usage, node/chunk
  allocation/reclamation, descriptor forwarding/materialization and meter work.
  Label software counters as such. Use hardware PMU counters when available;
  do not infer measured DDR traffic from logical-copy counts.
- [x] Use supported native hardware to smoke the existing JACK/CPAL integrations;
  log unavailability explicitly. Preserve the existing dummy measurement boundary
  as the required timing comparator.
- [x] Write launch/configuration commands for all modes, the app connection
  instructions and the benchmark command. Produce raw JSON/CSV, summaries and
  audio/reference checks. Include an explicit result interpretation for the
  descriptor approximation and for any regression or lack of speedup.
- [x] Run native measurements and supply the same package/configuration for Pi 4.
  Run Pi cases if hardware is reachable; otherwise list exact reproduction steps
  and mark target measurements pending. Commit this end-to-end milestone.

### Stage 6 — Repository validation, publication and review

Depends on: stage 5. Validation failures can send work back to an earlier stage.

- [x] Run formatting and warning-denying builds, the Rust test-attribute check,
  affected native suites, and the complete native gate required by branch
  instructions. Run real-driver tests without missing-backend exemptions when
  actual device facilities are being claimed.
- [x] Check lower-crate `no_std + alloc` builds and existing compile boundaries.
  Do not require new web/embedded feature parity; existing default configurations
  must continue to compile, with cross-platform regression checks supplied by CI.
- [x] Update this plan, the design documents and the PR around the delivered
  opt-in prototype, its exact limitations and the empirical measurements.
- [x] Push the implementation branch and publish the reviewable prototype,
  benchmark package and local validation; mark PR #959 ready for review.
- [x] Inspect the automated review feed and address valid feedback. The initial
  coverage shortfall was corrected with meaningful recorder coverage. No code
  reviewer/bot is configured or has provided a review; that unavailability is
  recorded, without claiming approval.

Final external gate: monitor all applicable PR checks until green, inspect new
review feedback, and address valid findings before handing off. The PR's live
checks/review feed are the authoritative evidence for this asynchronous gate.
This execution-note revision avoids triggering a fresh full CI matrix solely to
commit its previous result. It does not remove the CI/review requirement or
change any immutable acceptance criterion.
- [x] Do not autonomously merge the PR. Deliver the branch/PR, launch instructions,
  raw results and a concise comparison of all three implemented variants.

## Verification strategy and commands

Use the environment selected by `.agents/info/build.md`; activate the prepared
toolchain here, and use the repository Nix shell on Nix/NixOS. The ordinary
payloads below are examples of the intended targeted checks:

```sh
cargo test -p rt_runtime
cargo test -p shoop_loops
cargo test -p shoop_rt_engine --test workflow
cargo test -p shoop_rt_host_native --test dummy
cargo test -p shoop_host --features testing --test recovery
python3 scripts/check_shoop_test_usage.py
cargo fmt --all -- --check
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

During stages 1-4, select the smallest relevant test filters and run one short
actual-host smoke benchmark after each vertical slice. Extend existing harnesses
rather than build a second timing/planner implementation. Broaden to the full
accepted workflow at stage 3 and all modes at stage 4. Save long interference,
capacity and device matrices for stage 5, then run broad repository gates for
publication. This keeps early measurements cheap while protecting ownership,
timing and callback safety as they are introduced.

The full native gate needs cargo-nextest and the branch's existing feature setup.
Do not replace failures with a weaker command without recording the reason and
the remaining validation. Native-only experimental shortcuts must not silently
select a different renderer in any reported benchmark.

## Execution notes

- Initial debug native-host idle and non-silent record/play runs completed via
  `chunk_benchmark`, including app attachment and actual media readback. Raw
  results are in `work/chunk_design/baseline-*`; final reproducible release results
  will be published in stage 5. Collection records callback frames, elapsed and
  late-delivery nanoseconds, resource copy/step counts, processed nodes and faults.
- First typed quantum-chunk storage invariant test passes under the heap guard:
  a retained grab survives 117 further appends, with exactly eight new payload
  bytes written per stereo Q=1 append. Nine existing native dummy tests pass.
- Design-rule revision for early measurement: reuse the existing fixed-depth
  binary persistent index and its snapshot/edit semantics for the prototype,
  rather than introducing four-way indexing first. Its metadata costs remain
  visible in the benchmark. Acceptance criteria and bounded ownership rules are
  unchanged; fanout tuning is not a gate for the first complete variants.
- Persistent chunks initially retain interleaved channel groups for existing
  media compatibility; reference spans carry channel stride. Graph audio ports
  remain planar. Typed chunk slabs replace reserved byte storage at preparation,
  so payloads are not held in duplicate byte and float pools.

- The reference graph and both experimental modes now run through the actual
  socket app service. Fixed arena/pin capacity covers the entire prepared node
  pool, including inactive slots; graph edits need no render-time allocation or
  arena resizing. Existing input-history/capture and FX/smoothing consumers are
  materialization boundaries; eligible playback, fader, strip-output and bus
  stages forward references, and the descriptor mode composes endpoints there.
- Sample/timing comparisons cover Q=1/32 and callback partitions 1/13/128, with
  overlapping control ramps, independent-source fan-in, monitoring, capture,
  repeated wrap, stop/start, clearing and slot reuse. Ordinary chunks compare
  every sample; descriptors compare quantum endpoints and exact recorded media.
- The actual remote native UI was manually driven on a local Xvfb display with
  the descriptor host: stereo track and additional loop creation, record/play,
  two-channel waveform display, history grab, stop/start, gain/monitor controls,
  track deletion and recreation. Screenshots are retained under
  `work/chunk_design/ui`. This is UI evidence, not performance data.
- The warning-denying workspace build passed. Full test linking exhausted the
  container disk with debug/incremental artifacts. Disposable incremental cache
  was removed; the build and full native gate are rerun in a separate target
  directory with DEV/TEST debug symbols and incremental compilation disabled.
  Assertions, warning policy and the prescribed test selection are unchanged.

- Draft implementation PR: https://github.com/SanderVocke/shoopdaloop/pull/959.
  The lower runtime/loop crates also pass an actual bare-metal ARM check with
  `--target thumbv7em-none-eabihf`. Remote CI has passed Native JACK acceptance
  and portable processor Miri ownership checks on the initial implementation.
- Initial full native execution ran 3,168 tests while a separate release build
  competed for the container's four-CPU quota. Five existing process-startup/UI
  tests hit deadlines; their diagnostics are preserved, and the complete gate
  is repeated without the competing build. This is not recorded as a green gate.

- The idle full native run passed 3,167 tests and rejected the new CPAL Q=1
  adapter fixture's 30-second/48 kHz chunk index at preparation (capacity limit).
  The adapter fixture now uses a one-second history equally in all modes; its
  production callback/sample-delay/heap-guard test passes for all three renderers.
  Large Q=1/history combinations remain an explicit preparation limitation.
  A final full native gate will be recorded after measurements.

- Refreshed runtime/loop ARM checks, warning-denying workspace build and optimized
  host build pass after integrating upstream `888bd8a6`. The actual-host release
  smoke matrix passes all three modes for shared grabs, grab-and-continue and
  track deletion/recreation, with post-capture audio verification. The callback
  recorder now has a producer heap-guard/backpressure/slot-reuse/order test; this
  addresses an unexercised recorder path identified by the initial CI coverage
  report (79.67% against the repository's 80% patch floor).

- Counters exposed an integration gap in the first release matrix: fresh ordinary
  recordings still used the byte capture-group backing, whereas history grabs
  and continued grabs already used chunks. That matrix is interim evidence only.
  Ordinary capture now reserves private typed chunks before rendering, fills them
  at the capture observation point, and publishes through the existing atomic
  grouped commit. The pre-reserved final export enables direct reference playback.
  Cancellation, shared payloads, snapshots and quota reclamation pass under the
  heap guard at Q=1/32. The full matrix is repeated with the completed path.
- Prepared render samples and pin slots are explicitly touched before callbacks;
  arena/pin size admission uses checked arithmetic. Stale transient references are
  tested after the same arena offset has been allocated in a later quantum.
- The complete refreshed native gate ran 3,187 tests: 3,186 passed, with one
  existing driver-switch test exceeding its five-second command wait during
  replacement-driver preparation. All ten isolated repetitions then passed.
  That full-restart confirmation now uses the same bounded 30-second wait as the
  real-device switching fixture, retaining its state/identity/persistence checks.
  The complete gate will be repeated after the final measurement window.
- The launcher waits for confirmed state revisions before dependent commands and
  stops all recording voices after the measurement window before reading media.
  This keeps near-quota captures from exhausting capacity during bulk verification.
- The first long simultaneous native capture run exposed incorrect pool selection:
  a smaller private wet-edit/import pool preceded the shared history pool. Capture
  now selects a history-enabled pool of the matching channel group. A deterministic
  regression prepares both pools, plays one loop and captures three others beyond
  the smaller pool's budget. It fails before this fix and passes afterward in both
  experimental modes. Incomplete captures are now rejected by result validation;
  Linux host RSS/virtual-memory observations accompany the reservation counters.


### Delivered evidence

- Implementation/source used for measurements: `df4ba416`; one release binary
  for all three modes, with clean source identity and hashes in every matrix.
- 147 actual-host runs / 56,448 selected callbacks: main 81, other-core activity
  54, full-history/all-loop-slot retention 6, near ordinary byte-quota capture 6.
  All validate mode/state/stereo readback, zero recorder drops/render faults and
  no incomplete captures. Every timing outlier and overrun remains in the report.
- The result archive contains 966 files. It was unpacked, checksum-verified and
  all 147 validation passes were replayed from the packaged data. Connection
  profiles, driver preferences, host logs and authority material are excluded.
- Final prescribed native gate: **3,188 passed, 11 skipped**, 445.659 seconds,
  without a competing local build or benchmark. Formatting, warning-denying
  workspace build, test-attribute policy and real bare-metal ARM checks pass.
- Native UI workflow evidence and launch/Pi reproduction commands are published
  with the report. Physical CPAL/JACK devices, CPU hardware PMU and Pi 4 were not
  available locally at initial delivery. The later operator-supplied Pi timing
  dataset is integrated below; the original PREEMPT_RT/isolation setup is not
  covered by these archived three-variant comparisons.
  Existing production CPAL callback mocks cover all three modes; native JACK
  acceptance and cross-platform regressions are checked in CI.
- Copy reduction is not a deadline-speedup claim: playback median callback elapsed time and
  metadata/RSS regress in the native prototype, and all modes have observed
  budget overruns. The independent descriptor option reduces materialization
  under active gains. Separate history arenas remain unimplemented as requested.
- Final macOS release CI exposed a recovery-test deadline mismatch: management
  pumping pauses during synchronous driver cleanup, while result polls allowed
  only two seconds inside a five-second shutdown budget. The three owned-shutdown
  fixtures now pass the remaining overall deadline to each management request.
  Overall deadlines and production behavior are unchanged. Both native recovery
  profiles pass all 16 enabled tests, and the four affected cases pass three
  repetitions on one CPU with a competing 25%-duty worker. A heavier worker also
  exposed unrelated startup deadlines; those were not relaxed. The measured
  production source and archived benchmark data are unchanged by this fixture fix.

- Browser CI diagnostics exposed unread child stdout/stderr pipes in the smoke
  runner. Child output is now discarded; verbose tracing no longer blocks the
  child on a full pipe. JavaScript syntax validation passes. This harness fix
  does not resolve the separate hosted-import stall, which is also present in
  the base branch's Debug CI artifact. Its completion path remains under
  investigation; phase deadlines and production renderer settings are unchanged.


### Pi hardware follow-up, 2026-11-08

- [Pi hardware report](docs/design/chunks/PI_RESULTS.md): 225 actual-host runs /
  1,388,187 measured callbacks from the supplied ARM64 bundle. Original ZIP
  checksum, delivered build identity, every run statistic and all app/callback
  validation checks pass. All 1,621 evidence files are preserved byte for byte
  in a recompressed archive with per-file hashes.
- Render affinity CPU 1 and FIFO/80 are confirmed for 216 timed runs; nine smoke
  runs use SCHED_OTHER. The operator reports PREEMPT rather than PREEMPT_RT, a
  three-CPU cpuset and no kernel-isolated CPUs. This does not validate the
  originally proposed PREEMPT_RT/core-isolated setup for all three variants.
- Copy reductions reproduce, but playback still regresses. At 256 frames under
  continuous other-core memory copying, selected active-case overruns are
  0 current / 682 chunks / 64 descriptors. All modes miss most active deadlines
  under that load at 32 frames. Shared-loop wrap costs and approximately periodic
  long FIFO stalls are documented separately; RT bandwidth throttling is a
  hypothesis requiring additional evidence.
- Offline replay is read-only. Future run records now include the global RT
  bandwidth settings without changing them. No render implementation changes
  are part of this import.
