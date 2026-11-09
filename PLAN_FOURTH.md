# Plan: compact audio engine and a true audio-only comparison

Status: executing stage 0 on `proto/audio-compact`. Upstream is still
`888bd8a6`; it is already an ancestor of this branch. Local placement is
controller CPU 0, render CPU 1, optional load CPU 2; allowed CPUs 0–4,
four-CPU quota, ordinary scheduling, unchanged 950/1,000 ms RT limits.
Inspected base: `proto/audio-chunks` at `61ae8539`. Follow this branch's
[write_plan.md](.agents/prompts/write_plan.md). Preserve the completed
[three-variant prototype](PLAN_CHUNK_PROTO.md) and its archived results.

## Goals and scope

Build a fourth opt-in native variant, `compact`, integrated with the normal app,
and remove internal MIDI work from **all four** benchmark variants. Obtain a quick
first comparison in this execution environment; Pi builds, SSH, packaging and Pi
measurements are outside this plan.

| Startup mode | Audio implementation under the common audio-only profile |
| --- | --- |
| `current` | Existing sample-buffer renderer. |
| `chunks` | Existing shared chunks and reference routing. |
| `chunks-descriptors` | Existing chunks with endpoint-composed, approximate ramps. |
| `compact` | Flat audio storage and cursors, compact routing operations, event-driven loop state, and deferred/fused sample processing with current ramp semantics. |

The hypothesis is that removing repeated planning, dependent storage lookups and
generic node plumbing will shrink the active working set and improve callback
cost. Existing routing is already compiled on topology changes; merely caching
that graph again is insufficient. The [Pi profiles](docs/design/chunks/PI_PROFILING.md)
motivate this work but do not establish a predicted speedup.

### Immutable acceptance criteria

These goals and criteria require explicit user approval to change.

1. One native build exposes all four modes through `SHOOP_AUDIO_RENDER`.
   `SHOOP_AUDIO_BENCH_AUDIO_ONLY=1` selects the same true audio-only policy for
   every comparison. Ordinary startup without either override retains current
   behavior, including MIDI. Preserve selection through native driver preparation
   and supported restarts; reject unavailable combinations without fallback.
2. In that profile, omit MIDI histories, indexes, capture/playback/restoration,
   owner maintenance and meaningful per-loop/source MIDI state storage or copying.
   Disabling device ports alone is insufficient. Protocol compatibility values
   may remain empty; they must not reintroduce MIDI processing or reservations.
3. `compact` supports the same basic workflow as the other variants: default sync
   loop; track/loop creation, removal and recreation; audio routing and monitoring;
   recording and simultaneous playback/capture; stop/start, repeated wraps,
   clearing/replacing takes; gain, balance, mute and ramps; basic sync/delayed
   transitions; recent-audio grabs, overlapping retained grabs and grab-and-continue.
4. Use the existing host, app service, RT protocol and dummy/JACK/CPAL boundaries.
   A normal local or remote frontend receives correct state, meters, completions
   and waveform/audio readback. A standalone mixer benchmark is insufficient.
5. `compact` matches current audio-only frame counts, capture cuts, transition
   timing and supported sample output, initially at `atol=1e-6, rtol=1e-6`.
   Preserve quantum buffering latency and callback partition independence.
   Existing descriptor-mode interior-ramp approximation remains confined to that
   mode; retain its endpoint oracle and explain any numerical differences.
6. Published audio and retained grabs remain valid through continued capture,
   history wrap, deletion/replacement and bulk-read leases. More sharing never
   causes a sweep copying old audio. Grabs freeze the requested capture window;
   asynchronous preparation must not silently shift it.
7. All new callback work has prepared capacities and explicit bounds. No heap
   allocation/deallocation, blocking locks, I/O, whole-take conversion, or
   recording-length scan in one callback. Admission failures are atomic; pending
   deletion stays charged until reclamation. Preserve `no_std + alloc` and the
   existing unsafe-code prohibition in lower crates.
8. On the supported compact steady path, playback/capture bypass the generic
   node router, full resolver preview and persistent binary audio index. A fourth
   mode label around the third renderer does not meet this criterion.
9. Run and archive a quick local audio-only baseline and four-way comparison, then
   exercise every existing benchmark scenario end to end. Export actual mode and
   MIDI policy, raw callbacks, accepted setup/final state, readback validation,
   memory reservations and timing summaries. No speedup threshold or WCET claim
   is an acceptance condition.

### Scope boundaries and comparison fairness

- Keep the three existing audio algorithms unchanged except for common MIDI
  exclusion, integration fixes and equivalent diagnostics. Compact's planner,
  storage and execution changes are intentional comparison variables.
- No separate per-source **payload arenas**. Compact may use per-source history
  handle rings in one common persistent pool. This explicitly supersedes the
  earlier plan's history-index-specialization restriction for `compact` only.
- FX/synth hosting, composites, advanced dry/wet editing, persistence polish and
  web/embedded feature parity are not required for compact. Explicitly refuse
  unsupported operations/configurations. Do not weaken supported audio behavior
  or silently route it through a different renderer.
- Private APIs, engine-specific operations and duplicated internal code are
  acceptable. Avoid a new generic framework, driver or transport-schema redesign.

## Design rules

### Common audio-only preparation

Thread one prepared policy through host startup, engine preparation and storage.
Set audio loop bindings to `midi: false`; prepare audio histories independently
of MIDI; omit MIDI index configuration, writers, trackers, recovery and related
staging reservations. Audit unconditional MIDI state construction/copies and
maintenance loops as well as optional stores. Report actual MIDI reservations
and work; test that supported audio operations never instantiate them later.
Reject MIDI requests consistently in this profile while retaining normal MIDI
tests outside it. Keep original archives immutable and identify these as fresh
baselines, rather than relabeling old device-only-MIDI-disabled measurements.

### Compact integration, timing and execution

- Keep existing app-visible IDs, request/reply vocabulary, resource readback
  protocol and `Cells` publication. Add private backend dispatch at engine/control
  and render preparation as needed: concrete `LoopPlanner`/`GraphRuntime` aliases
  currently couple those owners. Adapt these seams rather than fork the frontend.
- Prepare compact routing operations and active-slot arrays outside rendering.
  Activate prepared changes atomically at established boundaries. Store generation
  checked identities; retire old plans/owners with bounded work.
- RT loop state is authoritative. Use frame anchors, lengths, cursors and a
  prepared pending-event queue to derive positions/cycles. Apply commands,
  synchronization dependencies, recording completion and due transitions at the
  existing quantum boundary. A wrap with no due action advances a cursor without
  rebuilding a full-world preview. Reuse existing timing rules/helpers where
  practical; compare behavior against the existing resolver as the oracle.
- Preserve command ordering, cancellation/refusal and completion semantics,
  telemetry and disconnect/fault handling. Do not maintain a competing control-side
  simulation that can drift from recording cuts or the render frame.
- Fuse gains, mixing and meter calculations where observation points allow.
  Retain required intermediate capture/meter taps and smoothing. Multiplying
  overlapping ramps must preserve their actual per-sample product; compact does
  not replace that product with the descriptor mode's endpoint approximation.
  No growing lazy expression tree or new sample-processing approximation.

### Flat storage and ownership

- Reuse terminology (`AudioChunk`, `ChunkRef`, `AudioSequence`, `RenderArena`)
  where accurate. Prepare a typed sample slab, indexed slots/free lists, flat
  recording/playback handle tables and source history rings. Start with the
  existing quantum-sized channel-group payload layout. Avoid a persistent tree
  or searching store catalogs on each audio read; cache the current block/cursor.
- Publish fresh payload immutable. An owner retains its audio; graph edges and
  sample loops borrow it. Share known identical producer data by identity rather
  than comparing sample buffers to discover equality. Materialize once at the
  correct capture tap when the signal actually differs.
- Reserve recording table capacity and publication/export owners before starting
  a take. Append handles while recording; stopping publishes the existing table
  and valid length without a full-table walk. Capacity refusal leaves the prior
  take valid. Use bounded partial-quantum staging and preserve capture tap semantics.
- A history grab freezes a logical frame range and prepares a retained flat view
  in budgeted steps. Copy/pin metadata oldest first, **before** history can
  overwrite each unprocessed entry; keep the source generation alive while this
  job is pending. Admission must reserve table space and enough per-quantum
  progress for all admitted jobs to stay ahead of overwrite, or refuse atomically.
  Publish only a complete owner; respect the existing activation timing contract.
  A timing deadline that cannot be met is refused, not silently delayed.
- Record and verify those overwrite/progress bounds before accepting this
  mechanism. Pinning the entire history synchronously, postponing progress until
  after wrap, or retaining payload without retaining its lookup information are
  invalid implementations. Shared requests for an identical cut may reuse a view.
- Clearing, cancellation and deletion detach owners quickly and enqueue bounded
  retirement. Reserve continued-capture headroom while retained views, readers
  and retirement jobs exist. Bulk readback streams through existing bounded APIs;
  it cannot require conversion of a whole take in the callback. Preserve the
  current restriction on reading an actively recording take.

## Code map

| Area | Work |
| --- | --- |
| `shoop_rt_host_native/src/lib.rs`, `rt_runtime/src/audio_mode.rs` | Fourth startup choice, typed common policy, actual-mode reporting. |
| `shoop_rt_engine/src/{description,lib,history,midi_layout}.rs` | True audio-only preparation, reservations and validation. |
| `shoop_rt_engine/src/{render,endpoint,graph,node,bulk,cells}.rs` and private compact modules | Backend dispatch, prepared operations, state/control integration, meter/readback adapters. |
| `rt_runtime/src/resource/`, `audio_refs.rs` | Shared sample primitives, compact owner/table storage and bounded readback/retirement bridges; avoid requiring generic index ownership. |
| `shoop_loops/src/runtime/`, resolver and media helpers | Common MIDI exclusion, timing oracle/helpers and compact transition/capture integration. |
| Existing engine/native/host tests and `shoop_host/examples/chunk_benchmark.rs` | Four-mode audio workflows, real app connection, positive readback and ownership checks. |
| `scripts/{benchmark,analyze,collect,profile}_audio_chunk*.py` as applicable | Mode subsets, four-way order rotation, actual MIDI-policy checks, compatible result handling. |

Placement may change for a documented reason. Keep changes local to the shortest
sound path; do not spread an experimental backend through public schemas.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Follow repository build/test guidance. Before Rust commits run `cargo fmt --all`
  and a warning-denying build; when Rust tests change also run
  `python3 scripts/check_shoop_test_usage.py`. Use focused checks during iteration,
  broaden before behavior-changing pushes, and complete final repository gates.
- Publish early measurements without claiming partial functionality is complete.
  Do not merge a PR autonomously.

## Implementation stages

### Stage 0 — Branch and freeze the local comparison

Depends on: nothing.

- [x] Create `proto/audio-compact` from the current prototype branch, preserving
  user changes and design/results. Fetch upstream `feat/backend-replacement`;
  incorporate relevant new base changes consistently across all modes and record
  the resulting baseline SHA. Do not overwrite the earlier evidence.
- [x] Check toolchain/build setup and available CPUs, topology and cgroup quota.
  Select separate allowed controller/render CPUs and, if feasible, a third load
  CPU. Record actual scheduling and RT runtime limits; use ordinary scheduling
  here, with no FIFO request or kernel-setting changes.
- [x] Fix the first measurement configuration: dummy driver, stereo 48 kHz,
  Q=32, buffer 256, four voices, four track slots, sixteen loop slots and existing
  history/loop budgets. Preserve meter/telemetry/recorder policy across variants.
- [ ] Verify an unchanged app-connected host record/play/readback smoke. Before
  MIDI removal, capture the same short three-mode quiet idle/play/ramps matrix
  planned for stage 1, using the existing device-only audio profile. Retain this
  build/ref, binary hashes and settings so MIDI-only gains have a fresh comparison
  on this machine. Push the branch and open a draft PR targeting
  `proto/audio-chunks`, linking this plan and prior PR #959.

### Stage 1 — True audio-only and the first three-way local baseline

Depends on: stage 0.

- [ ] Implement common MIDI exclusion throughout preparation and runtime, with
  zero MIDI payload/index/history reservations and no MIDI state/maintenance path.
  Preserve normal non-benchmark MIDI and supported app creation workflows.
- [ ] Add actual audio-only identity/reservation reporting and benchmark validation.
  Add `--modes` subsets; derive rotation from selected mode count, removing `% 3`
  assumptions. Keep old three-mode result archives/analyzers usable.
- [ ] Verify record/play, grabs, continued capture, readback and default sync startup
  in the three modes under audio-only heap guards. Test rejected MIDI operations
  and an ordinary MIDI-enabled regression case; check no MIDI work is recreated.
- [ ] Build the release host/launcher and run the quick command below without
  `compact`: quiet idle/play/ramps, two repetitions, 0.5-second warmup and 384
  selected callbacks. Save raw results as the first audio-only milestone in
  `docs/design/chunks/FOURTH_RESULTS.md`; compare with the fresh stage-0 baseline.
  Historical results remain contextual, with protocol differences made explicit.

### Stage 2 — Compact storage, ownership and preparation

Depends on: stage 1.

- [ ] Add `compact` preparation and its real backend identity. Decouple the legacy
  `chunked()` predicate from selection of the new storage/executor; incomplete
  compact configurations fail explicitly rather than using another path.
- [ ] Implement prepared sample slots, handle tables, cursors, transient space,
  append/publication, readback leases and bounded retirement. Integrate existing
  resource/media identities without duplicating PCM into legacy storage.
- [ ] Implement history rings and the frozen-range preparation mechanism; specify
  admission/progress/overwrite bounds and continued-capture headroom numerically
  from configuration. Complete lifecycle tests before live grab activation.
- [ ] Verify partial spans, retained ownership, ring wrap, cancelled jobs, source
  removal, stale generations, exhausted table/payload/retirement budgets and
  atomic refusal. Exercise callback heap guards and bounded work at full capacity.

### Stage 3 — Integrated compact recording, playback and controls

Depends on: stage 2. This delivers the first useful fourth-mode vertical slice.

- [ ] Add compact prepared routing and render execution behind the existing engine
  endpoint. Wire normal track/loop creation, host I/O, monitoring and bus output.
- [ ] Implement recording/publication, playback cursors, stop/start/clear/replace,
  sync and scheduled transitions, gain/balance/mute/ramps and the required smoothing.
  Publish real state, meters and streamed media through the existing app service.
- [ ] Verify deterministic samples/capture cuts against audio-only current at
  Q=1/32 and callback partitions 1/13/128/256, including simultaneous capture and
  playback, pending controls and multiple loop wraps. Extend the existing oracle;
  keep the descriptor mode's endpoint-only exception separate.
- [ ] Run actual-host idle/record/play/ramps smoke checks with positive media
  readback. Confirm compact executes its new path and avoids steady legacy
  planner previews, node routing and persistent audio-index traversal.

### Stage 4 — Quick first four-way benchmark in this environment

Depends on: stage 3; do not wait for all grab/churn integration or broad CI.

- [ ] Run the quick four-way command below on one release build. Keep all raw
  callbacks, check actual mode/audio-only policy, zero drops/faults, exact selected
  frame continuity and positive scenario state/readback. Two repetitions are
  exploratory; do not describe their ordering as a fully balanced comparison.
- [ ] If a separate allowed CPU and adequate quota exist, add a short play-only
  memory-interference run using the existing worker. Record placement/quota and
  interference conditions; retain quiet results separately.
- [ ] Update `docs/design/chunks/FOURTH_RESULTS.md`; retain replayable raw artifacts
  and a hash/build manifest under `results/fourth-local-<date>/` beside it, using
  the existing result collector. Report p50/p95/p99, maxima, overruns, lateness,
  RSS and prepared/live audio metadata. Give initial compact/current and compact/descriptor
  ratios; label partial functionality, short warmup and small sample count.

### Stage 5 — Complete grabbing and dynamic app workflows

Depends on: stages 2–4.

- [ ] Wire normal grab and adopt-ringbuffer requests to compact frozen views,
  scheduled publication and writer continuation. Share retained audio without
  depending on the source ring's current contents.
- [ ] Complete overlapping grabs, multi-loop retention, growing recordings,
  sync-aligned cuts and activation, bulk readers surviving replacement, removal
  and recreation of loops/tracks, and cancellation/disconnect/fault cleanup.
- [ ] Verify repeated grabs through several history wraps, simultaneous recording
  and playback, deletion while reading, and pool/owner saturation. Check bounded
  per-callback preparation/reclamation and unchanged old takes after refusal.
- [ ] Pass every existing host scenario in compact, including `grab`, `shared`,
  `continue`, `record-play` and `churn`. Validate accepted controls, active state,
  finite/nonempty signal readback, and final published media; silence stays separate.

### Stage 6 — Final end-to-end validation and local results

Depends on: stage 5. This is the functional and measurement completion gate.

- [ ] Exercise all four variants through the normal app/service/RT stack, including
  a normal remote frontend connection and meters/waveforms, while preserving
  true audio-only policy through supported preparation/restarts. Smoke existing
  JACK/CPAL devices where available and report actual coverage/unavailability.
- [ ] Run all benchmark scenarios locally at least once per mode. For the final
  quiet idle/play/ramps comparison use four repetitions to balance four-way order;
  add buffer 32 with equal selected audio duration. Include a default 30-second
  history-fill/wrap shared run per mode; use accelerated/small-history deterministic
  tests for broader wrap/capacity coverage rather than a long timing matrix.
- [ ] Retain all selected outliers and distinguish wall callback time, delivery
  lateness and scheduler/quota effects. Original warmup/verification rows remain
  archived but excluded from selected-window statistics. Call copy counts logical
  work, not DDR traffic; no Pi conclusions or WCET claims from this local run.
- [ ] Run formatting, warning-denying workspace build, Rust test-attribute check,
  affected suites and the complete native gate in `.agents/info/test.md`. Run
  existing `no_std` compile gates for changed lower crates; use CI for other
  platforms, without requiring new compact web/embedded functionality.
- [ ] Update launch instructions, limitations, benchmark commands, result manifest
  and this plan. Interpret MIDI-only gains separately from compact's additional
  gains/regressions using the new audio-only baselines. If performance disappoints,
  report it and sample CPU separately if available; do not substitute profiling
  measurements for unprofiled timing or expand into an unrequested redesign.

### Stage 7 — Publication, green CI and review

Depends on: stage 6; fixes may return to the relevant earlier stage.

- [ ] Push completed milestones, rewrite the PR description around the delivered
  four-mode comparison, and mark the full prototype ready for review.
- [ ] Monitor required CI until green; investigate failures, fix relevant causes,
  rerun affected checks and record unrelated infrastructure limitations honestly.
- [ ] Check automated reviews repeatedly, address valid findings and request/recheck
  review until approval when a reviewer is available. Record absent reviewers
  explicitly; do not claim approval that did not occur.
- [ ] Deliver branch/PR, commands, raw evidence and the concise four-way result.
  Leave merging to the user; no autonomous PR merge.

## Quick benchmark and verification strategy

After implementing `--modes`, build in the environment selected by
`.agents/info/build.md` and choose `FOURTH_HOST_CPU`/`FOURTH_RENDER_CPU` from the
actual allowed CPU set. The stage-1 command omits `compact`; stage 4 uses:

```sh
RUSTFLAGS="-D warnings" cargo build --locked --release -p shoop_host \
  --features testing --bin shoop_host --example chunk_benchmark
python3 scripts/benchmark_audio_chunks.py \
  --host target/release/shoop_host \
  --launcher target/release/examples/chunk_benchmark \
  --modes current chunks chunks-descriptors compact \
  --output work/fourth/quick-four --scenarios idle:silence play ramps \
  --quantum 32 --buffer 256 --voices 4 --tracks 4 --loops 16 \
  --warmup 0.5 --callbacks 384 --repeats 2 \
  --host-cpu "$FOURTH_HOST_CPU" --render-cpu "$FOURTH_RENDER_CPU"
```

This selects about two seconds of audio per run and is intended to take minutes,
not a multi-hour matrix, excluding build time. It deliberately does not fill
30-second histories; final wrapped-history checks are separate. No SSH or Pi run.

Use targeted runtime/storage tests for stage 2, existing engine workflow/sample
oracles for stages 1/3/5, and actual-host smoke after each working slice. Avoid
tests that only mirror new structs. Broaden to all scenarios, default startup/MIDI
regressions and full native/compile gates at stage 6. This protects ownership,
timing and integration while getting the first empirical result before polishing
unsupported features or running broad performance matrices.
