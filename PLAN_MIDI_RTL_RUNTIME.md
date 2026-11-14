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
- [ ] **2. Short-message policy and state.** Introduce the fixed record and common
  policy/validation, canonical supported-state behavior and reserved recovery.
  Verify ordinary MIDI oracles and intentional rejection behavior; audit initial
  prefix migration and live routing separately from file loading.
- [ ] **3. Block storage and ownership.** Implement fixed-depth directories,
  immutable versions, private tile writes, finite credits and incremental
  retirement. Verify old/new versions, clipping, density, slot reuse and allocator
  guards against independent packet-list oracles.
- [ ] **4. State preparation and continuous writers.** Replace semantic index
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
  Production integration is pending; this is not yet a change in live policy.
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
- Foundation milestone is intentionally not wired into production yet. Next:
  normalize clipped views before editing, adapt the production writer/resource
  APIs to the new block store and integrate ready positions at loop activation.
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

### Integration notes

Production currently enters `resource/indexed_midi.rs`, whose private `Core` is
`resource/midi_writer.rs`; input history is `midi_writer/history.rs`. It is not
enough to replace the older `shared_midi` implementation. The current Core API
owns source histories, imported resource bases, writer admissions, windows,
snapshots, serialized reads, state queries and retirement credits.

The new `midi_runtime::Store` already handles immutable roots, fixed-depth ordered
lookup, wire-byte prefix sums, append, replacement, clipping and normalization.
`PositionJobs` performs explicit finite preparation; `BasicUpdate` decomposes
controller/key/reset work into bounded state-cell steps. Current legacy paths
are unchanged until their adapter is replaced.

Do not implement `Core::state` by driving a whole-prefix replay to completion
inside `RenderIo`. State at zero and a writer's advancing frontier are ready fast
paths; cold positions require preparation. Reusing a valid active playback
reader's strict-before-next-frame state can seed an immediately activated writer.
This is relevant to `shoop_loops/runtime/shared/writer.rs`: existing Windowed
writers are admitted before their actual raw start position is supplied in a
`WriteSpan`. Preserve exact public transitions with an explicit ready-state
handoff/preparation, rather than hiding a prefix scan or shifting an accepted cut.

Clipped views currently return Busy for edits until `Store::normalize` produces a
root containing only visible originals. Root ownership uses separate version
leases for retain, so releasing the original token cannot release a job's pin.
Include directory/page retirement debt, sparse-tile fragmentation and state-bank
copies in profile sizing. Existing default source history advertises 65,536 total
record slots and 4,096 MIDI arena pages; preserve its supported density with an
honest new memory profile, not an accidental effective reduction from page packing.
