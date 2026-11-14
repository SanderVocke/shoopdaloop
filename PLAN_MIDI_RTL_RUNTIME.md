# MIDI runtime implementation

Base: `feat/backend-replacement` at `bfe73c9a`.
Branch: `feat/midi-rtl-runtime`.
Design: [MIDI runtime for Rust and RTL](docs/native_session_recovery/midi-runtime-rtl-design.md).

## Goal and acceptance

Replace the production Rust MIDI runtime with the design's fixed-message,
fixed-block, explicitly scheduled implementation. Deliver a PR into
`feat/backend-replacement` with no remaining basic-MIDI parity work relative to
master. Do not merge automatically.

- Published storage uses four-byte capped inline messages, bounded blocks and
  fixed-depth addressing rather than variable packet slabs and balanced semantic
  trees. Production must use the new implementation, not an optional prototype.
- Reject SysEx and unsupported messages. Reject parameter CCs by default; implement
  any advertised live-only mode without recording/tracking those packets.
- Preserve ordinary note/controller/pedal/program/bank/bend/pressure, frame/order,
  initial state, preplay, recording, Grab, continuous Replace, duplication and file
  behavior within the stated design. Preserve coherent audio/MIDI publication,
  immutable old exports, explicit resource refusal and recovery.
- State lookup/preparation, writer continuation, allocation and retirement have
  finite, observable work and memory. No callback allocation, locking, hidden
  suffix reconstruction or silently deferred accepted action.
- Preserve current supported desktop density and long-recording behavior unless
  a documented design change justifies an explicit, approved capacity contract.
- Existing tests pass except for individually documented outcomes intentionally
  changed by the design. Meaningful independent state/content oracles and tests
  of the new representation must pass. Native and actual browser routes must be
  verified; platform CI must be green and automated review addressed.
- Update the design, persistence contracts, capability descriptions and P11 ledger
  to reflect final implementation and evidence; do not leave deferred MIDI work
  hidden behind a parity-complete label.

## Design rules

Keep one authoritative frame clock and the existing resolver's precedence. Keep
service, host, codec and runtime responsibilities separate. Use fixed-width IDs,
explicit FSMs, prepared pools and generation fences. Initial state excludes the
target frame; originals at that frame play once, after cleanup/restoration. Retain
unsupported input as a diagnostic/refusal rather than truncating it. Use the same
logical decisions for typed and encoded carriers.

## Stages

- [x] **1. Establish delivery and contract.** Create implementation branch, retain
  the design/investigation and this plan, and open a draft PR targeting the
  replacement branch. The PR checkbox is completed when its URL is recorded below.
- [x] **2. Short-message policy and state.** Introduce the fixed record and common
  policy/validation, canonical supported-state behavior and reserved recovery.
  Verify ordinary MIDI oracles and intentional rejection behavior; audit initial
  prefix migration and live routing separately from file loading.
- [x] **3. Block storage and ownership.** Implement fixed-depth directories,
  immutable versions, private tile writes, finite credits and incremental
  retirement. Verify old/new versions, clipping, density, slot reuse and allocator
  guards against independent packet-list oracles.
- [x] **4. State preparation and continuous writers.** Replace semantic index
  dependencies with bounded replay jobs, cached entries and advancing writer
  state; integrate history, Grab, recording, Replace and coherent cuts. Verify
  early edits with long suffixes, all boundaries/mode exits and pressure.
- [ ] **5. Production integration and master parity.** Integrate loop/engine,
  adapters, host ingress/egress and codec/file paths. Retire superseded production
  machinery. Verify UI/Lua, duplication, save/load/store/export, accepted-operation
  recovery and both typed/encoded paths, preserving unrelated audio behavior.
- [ ] **6. End-to-end acceptance and delivery.** Run full required native gates,
  actual AudioWorklet workflows, trace/resource/work checks and platform CI.
  Resolve automated review; update contracts, P11 evidence and the final PR
  description; mark ready only when the full goal is satisfied.

Stages 2 and 3 establish the model for stage 4; stage 5 depends on that stable
runtime; stage 6 validates the final integrated implementation. Use targeted tests
while iterating and full native gates at major milestones and before behavior
pushes. Browser and CI checks supplement rather than replace the native oracles.

## Execution contract

Keep the plan updated and check off completed work. Commit completed stages or
meaningful milestones. Revise implementation steps when evidence warrants it and
design rules only for a documented reason. Do not change goals or acceptance
criteria without explicit user approval. Push the implementation branch, maintain
the PR against `feat/backend-replacement`, work CI to green and address automated
review until approved. No autonomous merge.

## Evidence and progress

- Investigation baseline: twelve focused existing MIDI tests passed; these confirm
  prior behavior, not this implementation. The source working tree initially
  contained only the two investigation/design Markdown files.
- PR: [#996](https://github.com/SanderVocke/shoopdaloop/pull/996), draft, targeting
  `feat/backend-replacement`.
- Added canonical 128-bit event records and exhaustive short-message/policy tests.
  Production ingress, resources, host bridges and codec paths now share this policy.
- Initial full native run: 3,241 passed, 11 skipped. New policy/block-store
  foundation: seven focused tests passed, including independent edit oracles,
  retained versions, dense pages, refusal, generation/scope checks and no-heap
  guards. The warning-denying workspace build passed; full runtime suite now
  passes 302 tests, including stepped state updates and prepared-position jobs.
- The first block store uses a fixed sixteen-level radix directory over 64-bit
  tile coordinates, pooled 32-record pages and incremental reference retirement.
  Clipped versions are read-only until explicit normalization, preventing hidden
  old records from reappearing when an edited view is extended.
- Added a fixed-cell basic-state update FSM, canonical managed-state copying and
  explicit position preparation/cancellation. Preparation pins an independent
  version lease, excludes target-frame originals and preserves them for playback.
- Production now uses normalized immutable block roots, explicit replay jobs and advancing writer frontiers. All live-source configurations select the block store, including automatically sized compatibility settings.
- Added append-only insertion and explicit clipped-view normalization. Normalization
  prunes complete radix branches and rebuilds at most two boundary tiles, using a
  fixed-depth iterative walker. Its tests prove later extension cannot resurrect
  hidden input events. Targeted block/position tests pass.
- Added a fixed-pool writer FSM with separate current/pending state banks and roots.
  Intervals publish only after every declared span is staged; abort preserves the
  last complete cut and freezes continuation. Tests cover an early replacement
  with a retained long suffix, held-pedal state, old snapshots and a refused
  second wrap span. Sparse append skips empty tile coordinates. The full runtime
  suite passes 305 tests and the warning-denying workspace build passes.

### Integrated implementation and outstanding acceptance

- Replaced the production resource Core and history with fixed inline records,
  sixteen-level directories and normalized immutable views. Removed the superseded
  packet-slab/semantic-index implementations. Imported files retain their existing
  streaming wire format and become fixed records before live editing.
- Added pinned, asynchronous Grab preparation plus an explicit complete replay
  grant before immediate input transactions enter the resolver. Playback entries,
  window/preplay changes and writer jumps use prepared state. Entry banks survive
  prefix-preserving edits; arbitrary readers never drive replay implicitly.
- Added bounded private block patches with sparse traversal, wrap handling,
  cancellation and atomic candidate publication. No suffix-state index is rebuilt.
- New `BlockLimits` names record pages, directories, versions, media banks, import
  bases and owner credits. State banks are sized by actual writer/job capacity;
  resource memory accounting includes the fixed pages and excludes the removed
  MIDI payload reservation from the common audio arena.
- Native output queues refuse complete batches and fence/recover affected ports;
  reset heads advance only on device acceptance. Generation fences discard an
  interrupted producer's old batch after recovery. Native/worklet ingress loss
  invalidates state until a fresh host baseline; reserved cleanup is announced once.
- Legacy null-selector-only initial prefixes normalize with explicit conversion
  metadata carried through codec workers and surfaced in completed import results.
  Unsupported timeline messages and meaningful parameter prefixes refuse atomically.
- Validation baseline: 3,241 native tests passed before implementation. The five
  low-level crates passed 581 tests during integration. Runtime/loop/engine checks
  subsequently reached 657/659; the remaining memory assertion and synchronized
  Grab credit-retirement issue were corrected. Final integrated/native/browser
  gates are still in progress, and P11 is not yet marked complete.

Remaining work: finish integrated failure checks and the full warning-denying
workspace build; verify native application canaries and actual browser workflows;
complete work/capacity/recovery audits and update design, persistence, capabilities
and parity evidence; push the implementation milestone and resolve CI/review.

- Platform acceptance follow-up: moved the four position-cache state banks out of
  stack-copied metadata, verified a one-MiB-stack application socket workflow, and
  fixed JACK shutdown to wait for acknowledged resets and completed graph cycles.
  All eleven strict real-JACK tests and the twenty-case shared-media measurement
  pass. Both application transports pass input, persistence and composite canaries.
  Standalone lockfiles/fixtures now use the shared policy; sixty loop-host tests
  and both native replay harnesses pass. Final browser and refreshed CI remain open.

### Acceptance progress

- Integrated runtime/loop/engine/native checks: 724/725 passed; the remaining
  fixed-depth lookup assertion identified a redundant record read, which was
  removed. The focused rerun passed.
- All fifteen isolated reruns of earlier full-suite failures passed, including
  both process transports, native UI recovery and driver preference/switch tests.
  The first broad run had overlapped Wasm compilation; the final run is isolated.
- The warning-denying workspace build, test-attribute policy and four-target
  dependency-boundary audit passed. Final work-accounting and worker-envelope
  regression changes will receive focused checks after the broad suite.
- The actual worklet/codec artifact contract passed, including bounded JavaScript
  ingress and explicit loss reporting. The unoptimized browser workflow progressed
  through all three hosted processor families but reached its five-minute total
  limit during the last load. Per-step checks did not fail; render memory growth,
  discontinuities and command overflows were zero. A local timeout override is
  available without changing the core CI deadline or per-step checks. The combined composite workflow has a separate seven-minute overall allowance; its earlier release CI run exhausted five minutes during the final resave after all composite checks passed.
- Firmware USB now reports loss epochs separately from cleanup, rejects excluded
  messages at the engine boundary and preflights complete output batches. The
  portable bridge passed address/undefined-behavior sanitizers; the ARM USB build
  check passed. The actual guest canary now expects exact basic MIDI and explicit
  exclusion of unsupported messages. The generic C transport codec remains an
  adapter with its own legacy transport vectors.
- Final isolated complete native suite: **3,242 passed, 11 skipped** (495 seconds).
  Subsequent accounting/worker-envelope changes: **757 focused tests passed,
  one skipped**. Final ARM USB check and both firmware probe example tests passed.
  The final warning-denying workspace build passed.
- Hosted Chromium/Web MIDI completed with zero xruns, including real device
  ingress, exact recording/playback, output-send/reset recovery, unsupported
  refusal, bounded ingress loss and reconnect. These active-engine checks now run
  in browser CI. Further platform acceptance moved initial/frontier states out of
  descriptors into prepared banks (128-byte descriptor ceiling), and budgets
  512-KiB firmware control/render stacks inside the existing RTOS heap. All 708
  focused tests, real-JACK input/persistence, a one-MiB-stack socket workflow and
  the warning-denying workspace build passed. Refreshed final gates remain open.
