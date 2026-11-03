# Configurable fixed-quantum redesign

## Goals and acceptance criteria

Replace adaptive graph intervals and the loop planner/observation lifecycle with
one configurable fixed tick. Preserve supported behavior except for the explicit
alignment policy. Keep preparation, ownership and reclamation safe for real-time
execution. Complete production native/browser integration and retire the old
execution path only after parity and end-to-end validation.

- Every core graph call processes exactly the prepared quantum (16/32/64/128,
  default 32), independently of host callback size.
- Loop lengths, seeks and transitions align to that grid. Requested/effective
  times and same-tick ordering are deterministic. MIDI retains intra-tick offsets.
- Arbitrary host partitions preserve the global clock and report buffering latency.
- Supported recording, MIDI, sync/program, graph/media editing, persistence and
  recovery behavior remains covered; existing unsupported features stay tracked.
- RT execution does not allocate, free owners, lock, wait, or perform I/O.
- Validation reports full-callback microseconds, budget use, tails, maxima and all
  misses for steady and transition-heavy work. VM timings are not device acceptance.

## Design rules

Quantum is immutable after preparation. Changing it requires stop/reprepare.
Host capacity, core quantum and plugin block size are separate settings. Alignment
rounds future requests upward, never earlier; overdue requests use the next eligible
tick. Unaligned stored durations are refused until an explicit conversion is supplied;
original media is retained. Tempo conversion must derive related lengths from a
shared tick clock and must not silently drift through independent rounding.

The outer callback owns ingress and maintenance budgets. Each tick settles changes,
executes the compiled graph, then advances capture/state. No partial graph tick is
permitted. General host adaptation has constant declared latency; direct divisible
hosts may use a separately prepared zero-buffering mode.

Cache prepared routing and state bindings. Keep atomic preflight for resource/owner
changes; removing interval planning does not remove required failure/ownership rules.
Do not port the benchmark's opt-in flags or bypasses into the production lifecycle.

## Implementation

- [x] Create `feat/fixed-quantum-runtime` from `feat/backend-replacement`; retain
  independent benchmark worktree/evidence.
- [ ] Specify timing types, alignment and callback contracts; verify with scalar
  reference tests, overflow/refusal cases and all supported quantum sizes.
- [ ] Implement compiled fixed audio/MIDI graph execution with prebound routing,
  Q-sized buffers and a direct shared state lifecycle. Verify DAG output, exact
  tick counts, MIDI ordering, errors, heap guards and callback-scoped maintenance.
- [ ] Implement direct and arbitrary-partition host adapters. Verify constant
  latency, clocks, audio/MIDI correspondence, zero blocks and partition invariance.
- [ ] Implement direct tick-based loop playback, transitions, sync/programs,
  recording and media ownership. Verify against independent reference models,
  aligned legacy cases, capacity failures and resource reclamation.
- [ ] Integrate engine, protocol/host configuration, settings, persistence and
  recovery. Verify native and browser production routes and capability parity.
- [ ] Re-run whole-callback performance experiments for regular, two-transition,
  simultaneous-burst, recording, MIDI and graph/media workloads; include plugin
  block/readiness adapter coverage and target-device evidence.
- [ ] Run final native/browser end-to-end and parity gates, remove superseded
  execution code, push, open a PR, obtain green CI and address automated review
  feedback through approval.

Each stage depends on its used interfaces, but independent work may move earlier.
Partial milestones remain explicitly partial; do not switch production to a path
that lacks currently supported functionality.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Progress and evidence

Initial branch: `6de0b7a18f04caf43f8cb76f81b1b2c040e089e2`.
The earlier experiments are in `/workspace/work/fixed-quantum`; no experimental
source edits were copied into this branch.
