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
- [x] **5. Production integration and master parity.** Integrate loop/engine,
  adapters, host ingress/egress and codec/file paths. Retire superseded production
  machinery. Verify UI/Lua, duplication, save/load/store/export, accepted-operation
  recovery and both typed/encoded paths, preserving unrelated audio behavior.
- [x] **6. End-to-end acceptance and delivery.** Run full required native gates,
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

## Delivery and acceptance

Completed 2026-11-14. [PR #996](https://github.com/SanderVocke/shoopdaloop/pull/996)
targets `feat/backend-replacement`; no merge is authorized. P11 is complete under
the user-approved basic-MIDI scope. The implementation, deliberate exclusions,
resource limits, commands and platform results are recorded in the
[acceptance evidence](docs/native_session_recovery/evidence/midi-rtl-runtime.md).

The verified implementation is `24319e7273bce270fd302c1d6057d3aafe9d9c0c`.
The closure commit changes only Markdown records. Delivery includes:

- Production fixed events/pages, sixteen-level immutable directories, finite
  retirement and explicit resource credits; the old packet slab/semantic tree is
  removed from the runtime.
- Stepped state interpretation, pinned position jobs, prepared entry banks,
  advancing writer frontiers and atomic complete spans/cuts, integrated through
  histories, Grab, recording, Replace, playback, edits and files.
- Consistent native/browser/firmware/control-port classification, whole-batch
  egress admission, acknowledged reset recovery, explicit input loss, and reported
  legacy-prefix migration. Unsupported documents refuse before activation.
- Bank indices and small descriptors rather than nested full-state stack copies;
  firmware stack budgets and native small-stack/JACK acceptance are verified.
- Baseline 3,241 native tests; refreshed full suite **3,244 passed, 11 skipped**;
  final **91** native scripting/adapter tests; standalone loop host **60** passed;
  both replay harnesses; all **17** runtime examples and both MIDI examples.
- All **11** strict real-JACK tests, **50** shutdown repetitions and **20** mixed
  routing/retirement/shutdown repetitions; both application transports and a
  one-MiB-stack socket workflow; **20** release media measurement cases.
- Actual rebuilt Chromium/Web MIDI and worklet/codec contracts passed with zero
  xruns, including exact input/output, files, loss, recovery and hosted processors.
- Warning-denying workspace build, formatting/test/doc audits, native feature
  matrix, four-target dependency audit, ARM USB build and portable USB sanitizers.
- All implementation-commit desktop/browser, coverage, JACK, bare-metal,
  firmware/USB/network/board-profile, hosted processor/Miri, CodeQL and docs CI
  passed. Automated review has no unresolved requests. Existing nightly and
  web-debug execution conditions remain documented rather than broadened here.

There is no remaining basic-MIDI parity implementation work. P07, P09 and P10
retain their separate scopes. Future RTL, a concrete RAM-port arbiter and FPGA
timing qualification are not part of this Rust delivery.
