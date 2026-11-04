# Plan: P12 loop timing parity

Status: implementation in progress on `feat/p12-loop-timing`. Written under
[write_plan.md](.agents/prompts/write_plan.md).

- Base and PR target: **`feat/fixed-quantum-runtime`**, inspected at
  `df4683527dbde0ec7483c3e522fc3873e39a81c8`.
- Implementation branch: `feat/p12-loop-timing`, created from the inspected
  target; remote head was unchanged when implementation began.
- Reference: `master` at `92079fb6f00308be0f6b407b7e8c02229662e420`, including
  range editing from #895 and its preserved `shoop_app_reference`/legacy fixtures.
- Deliver through a PR into `feat/fixed-quantum-runtime`, with green checks and
  reviewed evidence on the final head. Do not target `master` or
  `feat/backend-replacement`, or merge the PR without instructions.

## Goals and scope

Close **P12 in full** in the [feature parity ledger](docs/native_session_recovery/feature-parity.md):
loop length, start offset, pre-play and timeline edits through the production
client/service/RT stack, including persistence and UI recovery. A length/offset-only
implementation is an intermediate milestone, not completion.

Cover every currently supported primitive audio, MIDI, mixed-media and timing-only
loop, including Sync, stopped/playing loops, active recording and scheduled
transitions. Preserve the newer range editor and per-channel tools; keep track
alignment/latency controls under their existing gates. Test Lua observations/events
against the same confirmed state; a new Lua timing-setter API is not required where
master has none.

Retain the integration branch's accepted quantum and capacity contracts in
[ARCHITECTURE.md](ARCHITECTURE.md) and
[PLAN_FIXED_QUANTUM_CLEANUP.md](PLAN_FIXED_QUANTUM_CLEANUP.md): immutable
Q in `1, 16, 32, 64, 128`, default 32, one whole-quantum graph traversal and
quantum-aligned logical loop periods. Q=1 provides exact-frame master comparisons;
at larger Q, nonaligned length requests refuse explicitly and the editor previews
the valid effective length. Raw offsets, media and MIDI timestamps retain frame
precision. Document this existing precision qualification in completion evidence;
do not claim arbitrary-frame loop periods at Q>1 or silently round API requests.

Other parity rows retain their scope: do not enable composites, replacing/dry-wet
modes, latency/take-alignment controls, grab or wider track topologies as a side
effect. Preserve their fields and integration points. P12 must supply the MIDI
window/seek/pickup-state behavior it needs rather than defer pre-play to P11.
Unrelated P11 import/initial-state limitations remain explicit. No host-crash disk
durability, Lua-VM recovery, shared-memory reattachment or multi-client expansion.

## Immutable acceptance criteria

Goals and these criteria require explicit user approval to change.

1. **Editor and intent parity:** both `SetLoopTimeline` and
   `SetLoopChannelTimings` execute through production. Single edits affect only
   the addressed channel; all-channel timeline start edits preserve relative
   offsets; loop end/length remains shared. Scalar timing edits retain their
   reference semantics. Grid/snap, preview and zoom/fit remain usable. Invalid end
   clicks do not collapse a loop.
2. **State-dependent admission:** length changes work while stopped/playing and
   refuse if recording is active or any recording transition is armed. Offset and
   pre-play edits retain reference admission while recording/armed; they do not
   move the capture clock, rewrite capture alignment, stop recording or cancel
   pending transitions. A request combining a forbidden length change with other
   fields refuses entirely. Validate again at the actual execution cut.
3. **Playback and sync:** a manual length edit retains absolute play position,
   applying modulo when it reaches or exceeds a nonzero new length. It never
   proportionally scales position or media. Offset/pre-play edits preserve period
   and transport.
   Sync-source edits update future pulse timing without stretching followers or
   inventing pulses/cycles; pending cycle counts and unrelated playback survive.
   Zero length and empty media have defined, tested outcomes under the existing
   empty-loop contract; no infinite wrap or unintended recording results.
4. **Real pre-play:** an eligible stopped loop with imminent scheduled playback
   emits retained material before the start, then continues into normal playback.
   Match master eligibility and absence of a new lead-in on ordinary wraps/seeks/
   retriggers. Cancellation, supersession, Stop, source changes and prediction
   invalidation terminate/recompute lead-in coherently. Audio is continuous;
   MIDI restores needed state and carries pickup notes across the start without
   duplicate attacks or stuck notes. Test different pre-play values per channel.
5. **Atomic ownership and truthful results:** prevalidate the entire single/all
   channel transaction, including identity, ranges, alignment, capacity and
   revisions. Publish one complete timing/media state at a settled boundary, or
   leave every field, owner and pending operation unchanged. Queue acceptance is
   not execution confirmation. UI loss after admission retains service-owned work;
   reconnection reconciles its result without replay. Stale generations cannot
   modify a new loop/session reusing a slot.
6. **Retained media and persistence:** timing edits preserve raw PCM/MIDI and
   permit moving the window again. Displayed coordinates consistently account for
   capture alignment, while alignment itself stays unchanged. Out-of-media audio
   is silent; valid aligned-take constraints still apply. Save/load/save preserves
   per-channel timing, shared length and exact raw media at unchanged sample rate.
   Export honors existing exact/raw versus logical-window format semantics.
   Nonzero pre-play loads and plays. Failed loads preserve the running session;
   playing saves use coherent cuts and recording saves retain their busy refusal.
7. **Fixed-quantum and RT safety:** satisfy the precision contract above at every
   supported Q and arbitrary host callback partitioning. No adaptive graph
   intervals, second musical clock, callback allocation/free/locks/waits/I/O,
   unbounded payloads or media-sized timing-edit transfers. Preserve finite
   recording/resource budgets and off-render retirement, including failure paths.
8. **Deployment:** identical admitted semantics through typed local delivery,
   encoded native socket and supported shared-memory delivery, and browser
   main-thread service/AudioWorklet. Verify socket reattachment, native host
   output, and actual hosted/self-contained browser UI and audio/MIDI output.
9. **Evidence and closure:** preserved reference fixtures and new production
   tests pass. Link acceptance evidence for every criterion before marking P12
   COMPLETE; enable `LoopTimingEdits` only when all production paths are ready.
   Final-head CI runs and passes for the requested PR base.

## Baseline findings and design rules

- Existing UI is retained in `shoop_egui/src/{details_pane,waveform_widget,
  midi_sequence_widget}.rs`; only mutation is capability-gated. Contracts already
  exist in `shoop_app_api`. `shoop_app_client/src/runtime.rs` refuses both intents,
  `shoop_app_protocol::Command` has neither, the service does not project the
  capability, and `shoop_rt_backend` refuses timing setters. Add the complete route.
- Use `shoop_app_reference::set_loop_timeline`/`set_loop_channel_timings`, legacy
  `channel_timing_contract`, native armed-recording refusal tests, and engine
  `audio_preplay`/`midi_preplay` as behavioral anchors. The older
  [loop assessment](LOOP_FEATURES_ASSESSMENT.md) is proposal/history, not current
  authority: it predates per-channel editing and overgeneralizes recording refusal.
- The service owns validation, session/channel identity and admitted work. The
  adapter maps to engine identities; the resolver/planner owns timing and the
  actual cut. Use bounded asynchronous completion, existing operation IDs,
  generation/revision fences and retained results. Do not wait synchronously for
  render progress on the service/browser thread.
- Distinguish raw start offset, capture alignment and displayed start. Per-channel
  requests subtract that channel's alignment. Preserve scalar reference-alignment
  behavior separately from the range editor's explicit per-channel delta batch.
  Resolve channel IDs against live service-owned topology, never trust a client
  index or synthesize authority from cached media.
- Add versioned per-channel playback-window metadata over immutable raw snapshots.
  Raw storage length and logical loop length are different. Reuse leases/indexes;
  metadata edits share media instead of copying/re-uploading it. Retain old versions
  for in-flight reads/exports and retire off render. Validate aligned-take windows
  without requiring every ordinary window to lie inside raw storage.
- Current `shoop_rt_backend/src/prepared.rs` renders offsets into a normalized
  loop-length upload; newly recorded reads report zero timing. Extend preparation,
  capture/read, resync and replacement to carry authoritative raw media plus
  windows. Do not make the adapter's canonical import cache the sole timing owner.
- `ContentRetime`/content-family activation currently scales positions and refuses
  recording. Provide a distinct manual timing-edit policy/path; preserve existing
  import/sample-rate family-retiming semantics and tests. Do not replace the whole
  session, rebuild physical nodes or stop playback for an ordinary timing edit.
- Derive lead-in from resolver-owned scheduled-start predictions. Publish explicit
  per-channel render spans/masks, including a pre-play onset inside a quantum,
  while processing the graph once per tick. Preserve sample/event positions and
  avoid a second clock or speculative renderer-owned transport. Pre-play may be
  audible before a subsequently cancelled start, as on master; clean it up at the
  cancellation's effective boundary. Never replay an already emitted pickup.
- Timing metadata remains available during recording even though the existing
  media-read path declines active captures. Separate metadata projection from
  optional waveform/MIDI reads; finalize captures with the latest accepted timing
  without publishing mutable raw storage or freezing capture to service a view.
- Prepare finite channel-edit/window/receipt capacity and validate checked signed
  arithmetic and legacy field bounds before promising success. Version changed
  app/RT layouts, host/worklet handshakes and fixture codecs as needed. Preserve
  `no_std`, safe Rust, the import-free worklet and existing dependency boundaries.
- Preserve UI/Lua ownership, current recording limits and other parity gates.
  Reuse `shoop_session` document fields; introduce a file-format migration only if
  demonstrably needed. P12 does not require undo or new transport modes.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Staged implementation

Execute in order. Stages 1–4 establish contracts and loop mechanisms; Stage 5
connects production execution/recovery; Stage 6 completes persistence; Stage 7
enables the editor/capability. Stages 8–9 validate and deliver the complete feature.
Each stage's verification must pass before its dependent stage is closed.

### Stage 0 — branch, reference matrix and baseline

- [x] Fetch the latest `feat/fixed-quantum-runtime`, preserve local work, create
  `feat/p12-loop-timing`, and record the actual base SHA and reference master SHA.
  Reconcile intervening changes before carrying this plan forward.
- [ ] Add this plan to the work branch. Record a behavior matrix for scalar versus
  per-channel edits, recording/armed refusals, zero length, signed offsets,
  alignment, sync-source edits and pre-play cancellation/continuity. Add focused
  reference assertions where behavior is not yet pinned; do not weaken fixtures.
- [x] Check PR workflow filters: `build_and_test.yml`, `docs.yml` and `codeql.yml`
  currently omit `feat/fixed-quantum-runtime`. Add the target or arrange equivalent
  final-head runs so a PR to the correct base receives actual checks.
- [ ] Verify: run the relevant existing reference/backend/editor fixtures and
  parity/dependency checks; record baseline failures independently of P12 changes.

### Stage 1 — timing transaction and projection contracts

- [ ] Extend application/backend/RT contracts for scalar and bounded channel-batch
  edits, explicit optional fields, expected identity/revisions, reliable outcomes,
  and authoritative per-channel timing projection. Define metadata visibility for
  empty and recording loops and its relationship to raw-media revisions.
- [ ] Specify one atomic publication cut, conflicts with capture/clear/import/load/
  removal, no-op/empty-batch outcomes and queue ordering. Reserve staging/result
  credits before admission; revalidate active and all armed recording at execution.
- [ ] Define quantum length validation and effective-frame reporting. Carry Q to
  editor validation; preserve frame-precision offsets/pre-play and MIDI timestamps.
- [ ] Verify: typed/wire codec round-trips, version refusal, oversized/duplicate/
  foreign/stale channels, integer overflow, nonaligned lengths and protocol bounds.

### Stage 2 — retained raw media and channel windows

- [ ] Extend the loop media ledger, planner views, audio/MIDI renderers and export
  descriptors to distinguish raw data duration from logical window duration,
  with independent audio-channel offsets and MIDI start/pre-play metadata.
- [ ] Implement prepared immutable window versions sharing existing media leases,
  transactional activation and off-render retirement. Preserve raw data beyond a
  shortened logical loop and in-flight exports of previous versions.
- [ ] Apply signed raw-position mapping with silence outside audio bounds and
  correct MIDI indexed seek/restore. Keep timing-only/empty-MIDI duration valid.
- [ ] Verify: Q=1 and larger-Q render oracles for negative/positive/different-channel
  offsets, shorten/re-extend, empty media, aligned-take rejection, raw export
  retention, saturation/stale sources and heap-guarded publication/cleanup.

### Stage 3 — manual length and live timing execution

- [ ] Add the manual timing transaction to resolver/planner execution: preserve
  position or modulo it, update period without scaling media, retain pending
  transitions, and recalculate source pulse prediction without retiming followers.
- [ ] Apply whole-loop and channel-batch fields atomically. Permit reference-valid
  offset/pre-play changes during capture/armed recording while keeping capture
  progress/alignment fixed; refuse length-containing transactions in those states.
- [ ] Integrate capture start/restart/finalization and clear with timing generations;
  prevent a completing take or stale queued edit from restoring old metadata.
- [ ] Verify: stopped/playing/recording/armed matrix; position inside/outside new
  length; zero length; multiple pending transitions; source/follower sync; coincident
  Stop/Record/clear; unrelated-loop continuity; refusal with no partial changes.
  Retain existing proportional import/family-retiming regression results.

### Stage 4 — complete audio/MIDI pre-play

- [ ] Implement resolver-owned imminent-start prediction and per-channel lead-in
  views, preserving logical Stopped state and emitting no early pulse/cycle. Render
  available raw lead-in, silence outside retained audio and start exactly at the
  published effective boundary; do not fabricate missing capture history.
- [ ] Handle edits during lead-in, different channel pre-play values, source
  stop/length/sync changes, replaced starts, cancellation, mute and explicit Stop.
  Preserve continuity into playback and cleanup when prediction becomes invalid.
- [ ] Extend bounded MIDI window/seek state handling as needed for note, sustain,
  controller/program/pitch state and ordered packet playback. Preserve pickup
  notes across start and release audible state on cancellation/removal/disconnect.
- [ ] Verify: port `audio_preplay`, `midi_preplay` and pickup-note corner cases to
  independent production-loop oracles. Test onset within a whole quantum,
  callback partitions/zero callbacks, long lead-in, no new lead-in on wraps/seeks/
  retriggers, cancellation after output, supersession and exhausted resource/MIDI
  capacity without partial audio/MIDI publication or callback heap activity.

### Stage 5 — production commands, confirmation and recovery

- [ ] Implement RT request handling, reliable publication/results and full state/
  resync metadata; implement backend timing setters with bounded asynchronous
  completion and generation-safe cache invalidation rather than media re-upload.
- [ ] Add service admission/handlers and client intent routing for both intents.
  Validate alignment-adjusted channel coordinates against authoritative topology,
  including newly recorded loops and requests without selected media loaded.
- [ ] Reconcile pending results and snapshots by confirmed revisions. Keep all
  admitted work owned by the service after UI loss; abandon unadmitted preparation
  safely. Stop/clear/removal/session replacement fence stale work and raw reads.
- [ ] Verify: typed and encoded production client/service/adapter/engine tests for
  success, RT refusal, backpressure, lost acknowledgment, duplicate operation IDs,
  stale generations, slot reuse and disconnect before/after admission. Socket
  reattach must recover state/results without double edits or replayed pickup notes;
  shared memory exercises supported single attachment and normal cleanup.

### Stage 6 — persistence, import/export and coherent capture

- [ ] Carry timing and raw lengths through RT capture/read descriptors, service
  persistence images/mappings, adapter preparation/replacement and portable codecs.
  Remove only P12 pre-play refusals in document, image and prepared validation;
  retain unrelated channel-mode/latency/unsupported-MIDI diagnostics.
- [ ] Save/load edited media and timing as one immutable cut; fence pending edits
  against capture/import/load, retain fresh live IDs and stopped loops on load.
  Preserve existing sample-rate consent, resampling, exact/raw exports and logical
  WAV/SMF window rules, including lead-in metadata in formats that represent it.
- [ ] Verify: save/load/save with independent channel timing, signed/aligned offsets,
  nonzero pre-play and shortened/re-expanded raw takes; rendered output after load;
  audio/MIDI import preserving opposite media/timing; sample-rate conversion;
  malformed/out-of-range data, capacity/cancellation and stale commit refusal.
  Exercise coherent playing-save and recording/armed-save refusal without stopping
  capture, plus UI-loss reconciliation during accepted persistence work.

### Stage 7 — editor integration and capability enablement

- [ ] Supply confirmed channel timing independently of optional media reads, with
  consistent audio/MIDI alignment coordinates and stable generation-safe IDs.
  Preserve lanes or metadata controls during active recording; loading a waveform
  is not a prerequisite for a valid timing command.
- [ ] Connect existing start/pre-play/end tools and metadata controls. Preserve
  single/all scope, relative starts, shared length, grid/snap anchoring and preview.
  Preview valid quantum-aligned periods and show actual refusals/pending outcomes;
  never present an unconfirmed edit as authoritative or silently quantize an API call.
- [ ] Enable `LoopTimingEdits` consistently in engine/backend, service projection
  and client/UI only after Stages 1–6 pass. Keep unrelated capabilities disabled.
- [ ] Verify: real production UI events reach service commands, change output and
  refresh confirmed markers/lengths. Cover mixed audio/MIDI lanes, active recording,
  armed-length refusal, stale data/refusal restoration and unavailable backends.
  Verify existing Lua length getters/change events and scheduling use confirmed
  edited state; no direct script-to-backend bypass or automatic script restart.

### Stage 8 — final end-to-end validation and evidence

- [ ] Run formatting, warning-denying workspace build and the full reference plus
  production suite per `.agents/info/build.md` and `.agents/info/test.md`:

  ```sh
  cargo fmt --all -- --check
  RUSTFLAGS='-D warnings' cargo build --locked --workspace
  SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
  python3 scripts/check_shoop_test_usage.py
  python3 scripts/check_tracing_coverage.py --require-closed
  python3 scripts/check_feature_parity.py
  python3 scripts/check_rt_runtime_boundary.py
  python3 scripts/check_loops_boundary.py
  python3 scripts/check_rt_engine_boundary.py
  ```

- [ ] Run affected native debug/release loop-host direct/wire/two-thread fixtures,
  heap guards and selected Miri cases covering shared raw leases, window activation,
  active-capture edits, MIDI pickup cancellation and refusal/retirement. Update
  inventories and keep hashed reports, source SHAs and seeds.
- [ ] Run Wasm/Node suites, no-default-feature/bare-metal checks and built worklet/
  codec artifact contracts using existing runners. Exercise all Q values and
  awkward host partitions, especially Q=1 exact reference cases and default Q=32.
- [ ] Exercise packaged native client/host paths on sockets/shared memory and a
  native output fixture (JACK/dummy loopback as appropriate), including recording,
  live edits, scheduled pre-play, persistence and socket reattachment. Validate
  samples/MIDI and transport state, not just rendered labels or acknowledgments.
- [ ] Extend/run actual hosted and isolated self-contained browser acceptance
  through the production AudioWorklet: edit audio/MIDI, play pickups, cancel/rearm,
  save/load/save and resume. Measure output/packet continuity and check errors,
  xruns and unchanged core/bus/preview workflows. Use existing browser runners and
  packaging contracts; compilation/Node execution cannot replace browser evidence.
- [ ] Write `docs/native_session_recovery/evidence/loop-timing.md` mapping all nine
  criteria to tests, native/browser artifacts, Q qualification and final CI runs.
  Update architecture/usage docs and the P12 row/capability disposition, removing
  stale P12 refusals and updating OPEN counts. Mark COMPLETE only after full
  pre-play, persistence, recovery and production UI acceptance; preserve other rows.

### Stage 9 — PR, CI and automated review

- [ ] Commit each completed milestone, push `feat/p12-loop-timing`, and create a
  PR with base **`feat/fixed-quantum-runtime`**. Describe final behavior, quantum
  precision, validation and evidence links for a reviewer without this discussion.
- [ ] Verify the requested base actually triggers build/platform, browser, docs,
  coverage and CodeQL checks. Inspect failures using `.agents/info/ci-debug.md`,
  fix causes, and repeat affected validation until final-head checks are green.
- [ ] Check automated review comments/threads as they arrive; address applicable
  feedback, explain evidence-backed disagreements, and repeat review/fixes until
  the reviewer approves. Record explicitly if no automated review is provided.
- [ ] Keep this plan and acceptance evidence current, verify clean intended scope
  and passing checks on the final head, and report the PR ready to merge into
  `feat/fixed-quantum-runtime`. Leave the merge to a separate instruction.
