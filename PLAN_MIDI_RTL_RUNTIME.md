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
