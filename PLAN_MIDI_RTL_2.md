# MIDI reference backend: shared storage, state ownership, and incremental differences

## Goal and scope

Evolve the MIDI proof of concept in `rt_fpga_reference` into a bounded, multi-loop
Rust reference with shared event storage, clearly owned musical state, and
incremental state-difference tracking. Keep a later FPGA implementation in mind:
use finite pools, explicit handles, sequential event access, and measurable work.
This remains a functional Rust backend, not a cycle-accurate hardware design.

Investigated baseline: `feat/fpga-reference-midi` at
`17c06dcd0e7c3f21879638a7822ad0cae278fa70`. Execution started on `feat/fpga-reference-midi-storage`.
The completed first milestone is documented in [midi_RTL_new.md](midi_RTL_new.md).

This milestone includes recording, playback from the beginning, stop/repeat,
history capture and grabs, retained content export/import, duplication/replacement,
and programmatic bindings for multiple loop MIDI data channels. Each data channel
has one sequence view; a loop initially has one MIDI data channel. Transfers are
component-level event/state operations, not MIDI file conversion or UI work.

Keep the current managed-message subset: ordinary MIDI 1.0 channel messages and
System Reset; explicitly refuse parameter-editing CCs 6, 38, 96–101, SysEx, and
other system messages. Preserve refined note, pedal, controller, pressure,
bank/program, and reset behavior. Broader message support, arbitrary playback
seeks, overdub, clip arrangements, general routing/voice merging, frontend/control
wire changes, shared graph-compiler MIDI support, and SystemVerilog changes are
outside this milestone. The new architecture must leave room for those extensions.

## Immutable acceptance criteria

1. A prepared reference configuration supports 128 MIDI-enabled loops, each with
   capacity for at least 8,192 short events. Demonstrate 30 seconds of 100 CC
   messages/second per loop, and at least one continuing history source with that
   retention/density. Provision history, staging, retained old content, snapshots,
   comparisons, and cleanup headroom separately from the 128 live recordings.
2. Event bytes, sample timestamps, equal-time ordering, and silent duration survive
   capture, view creation, duplication, export/import, and playback. Existing
   supported state semantics and known/unknown distinctions are preserved.
3. Published content and saved state are immutable and independently retained.
   Source append, overwrite-by-new-take, history rollover, replacement, or deletion
   cannot change an existing view/export. Stale handles are rejected before use.
4. State differences are maintained as relevant state changes occur. Normal
   start/repeat of a ready binding does not compare all note/controller slots or
   reset all controllers before restoring them. Identical supported states need
   no synthetic restoration messages; ordinary differing controls emit only their
   needed changes. Intentional stop/retrigger behavior and original recorded
   commands remain distinct from restoration and retain their defined semantics.
5. State transitions preserve the refined ordering needed for pedals, notes,
   selected versus pending banks, and programs. Destination state and difference
   progress reflect accepted output only. Refusal, partial transitions, resets,
   and recovery never expose an unfinished transition as successfully restored.
6. One history grab can publish a selected retained interval into any selection
   of 1–128 loops within one quantum, including its correct initial state and
   ready comparisons. The selected interval may start inside a storage block and
   contain leading/trailing silence. Define valid ranges using the existing loop
   timing constraints; do not silently reduce this to a single preselected range.
   Group publication is atomic: all destinations change or none do. Sharing the
   same source/range must not copy events or full state once per target loop.
7. Import/export/duplicate/replace can progress across quanta while playback and
   history capture continue. Export retains one coherent content version; import
   publishes only fully validated, prepared content. Background work has a finite
   service budget and does not hold an engine lock while doing an entire transfer.
8. After preparation, processing, grabs, publication, and cleanup perform no heap
   allocation or unbounded destruction. Releasing content detaches ownership in
   bounded foreground work; a budgeted collector reclaims it incrementally.
   Exhaustion is explicit, preserves live content, and has no synchronous sweep
   fallback. Incomplete recordings remain visibly incomplete.
9. Publish memory accounting and work bounds for the capacity profile, including
   state copies, comparison fanout, expensive message effects, grab preparation,
   event lookup, transfers, and reclamation. Demonstrate the required workloads
   in a combined audio/MIDI pipeline and report measured CPU timing and its limits.
   A metadata-only grab measurement is insufficient evidence for criterion 6.
10. Existing loop timing remains authoritative. Audio-only behavior, combined
    audio output, sync waits, callback partitioning, and fatal-fault handling stay
    correct. App-facing compiler and hardware targets continue to refuse MIDI.

## Design and ownership

### Stored content and running state

| Owner | Data and responsibility |
| --- | --- |
| MIDI content store | Event blocks, sequence-view descriptors, immutable state snapshots/checkpoints, references, and deferred retirement records. |
| Input/history service | One mutable interpreter per configured input, retained history, boundary/checkpoint preparation, and recording-start state. Input tracking is not repeated for every recording loop. |
| Output service | One current-state interpreter per controlled destination, after routing/channel mapping; update it for all accepted playback, monitored-input, transition, and recovery output. |
| Playback context | Per loop MIDI data channel: binding, event cursor, note/pedal activity, and resumable transition progress. It never mutates a saved starting snapshot. |
| Difference service | Engine-owned comparisons keyed by target snapshot, destination, and mapping identity/revision; holds references and fixed difference maps. |
| Existing loop controller | Mode, length, position, synchronization, and loop identity. MIDI bindings refer to it; musical state does not become a second scheduler. |

```text
Loop identity / timing
        |
   MIDI binding ----> playback cursor and transition progress
        |
        +-----------> sequence view ----> event blocks
        |                   |
        |                   +----------> immutable starting snapshot
        |
        +-----------> comparison handle
                            | target: starting snapshot
                            | current: destination state
```

Names are provisional. Use prepared arrays/pools with fixed-width handles and
generations, not raw pointers or callback subscription lifetimes. Physical RAM
placement is a later hardware decision; separate ownership does not require
separate memory devices or threads. Retain `no_std` + `alloc` and safe Rust.

### Event storage and lifetime

- Use one allocation/ownership level: fixed-capacity blocks of timestamped events.
  Fill by event count, not elapsed time or quantum. Do not copy audio's two-level
  chunk/block geometry. Smaller future memory transfers are a separate concern.
- A view describes one chronological event range, its time origin, duration,
  initial-state handle, and revision. It may reference several physical blocks;
  it is not a playlist of other views. Nested slices retain independent ownership.
- Append only into unpublished tail space. Retained prefixes stay stable while
  the writer continues. Never overwrite pinned history blocks; allocate/reuse
  another block or report the configured capacity failure.
- Duplicate a view by retaining its handle. A retake/import creates new content
  and replaces the binding. Export pins the old version until explicitly released.
- Transfer old ownership to a bounded retirement queue at clear/replace/delete.
  Count release of event blocks, snapshots, comparisons, and descriptor metadata;
  free a block only after its last reference and outstanding use have ended.
- Reuse audio's ownership and deferred-release principles. Do not call its current
  allocating `slice` or whole-list `clear` in a MIDI hot path, or generalize the
  entire audio store merely to share code. Its payload is audio-specific, while
  the RTL media reclaimer supplies a useful budgeted-lifetime reference.

### State updates, differences, and transitions

- Saved starting states/checkpoints live in an immutable snapshot pool. Mutable
  input, destination, and playback activity state lives in engine-owned pools.
  Views and contexts hold handles; no full state is embedded in every event block.
- Extend the stepped interpreter to report the fields and semantic groups changed
  by each step. A controller normally affects one field; pedals, modes, programs,
  and resets can affect several. Difference maintenance participates in the same
  progress/publication contract; partial state and stale difference maps are not
  presented as ready. No mutator may bypass this path.
- Use fixed difference bitmaps with summaries for nonempty channels/groups.
  Compare the affected field against each interested target and set **or clear**
  its bit. A value returning to its target removes the difference. Count the
  number of interested comparisons: work is not constant when fanout grows.
- Share a comparison when target, destination, and mapping match. Playback cursors
  and transition cursors remain separate. A shared difference map describes the
  latest destination state; it is not a frozen packet list for another loop.
- Prepare new comparison pairs before binding publication. Whole-state scans or
  copies during cold preparation must be staged/accounted, not moved invisibly
  to the next playback callback. Generation/version checks invalidate prepared
  work if the destination, routing, target, or recovery epoch changes.
- Replace unconditional cleanup/full restore with an ordered transition over
  differences and dependent groups. Define note continuation versus deliberate
  release/retrigger, bank/program dependencies, and pedal reconstruction. Do not
  suppress original messages, replay an old reset merely because a marker is set,
  or invent an unknown controller value. Broad reset remains an explicit command
  or recovery action, not the normal way to establish a matching target.
- Serialize transitions at a destination and revalidate on intervening output.
  Bound simultaneous contexts and their dependency bookkeeping. Initially require
  explicit, nonconflicting channel ownership for independently controlled parts;
  reject unsupported simultaneous overlaps before publication. Several loops may
  still retain the same content/comparison. General same-key voice merging is not
  implied. Recovery is coordinated for every context affected at that destination.

### History grabs and bounded preparation

- History owns rolling state and retained checkpoints independently of loops.
  Retain the state immediately before the selected interval, including when its
  first event occurs much later. A state snapshot remains alive after history
  advances beyond it if a view retains it.
- Establish an explicit bound on finding the event range and obtaining its state
  and comparisons. Candidate: retained checkpoints with live comparison maps,
  plus a bounded replay from the preceding checkpoint. Derive a new comparison
  from that checkpoint's difference map and the replay's changed fields, avoiding
  a fresh full-state comparison per destination. Include fixed-size copy costs.
- Choose checkpoint cadence by worst-case state work as well as event count;
  repeated reset/pedal commands cannot hide unbounded replay inside a small event
  prefix. Account for checkpoint creation, retention, comparison fanout, and pool
  exhaustion. Do not store an unbounded snapshot per quantum or per event.
- Prepare supported destination/mapping relationships as part of binding/history
  setup. New routes can require staged setup; ordinary grabs on configured routes
  must meet the one-quantum contract. Maintain readiness while capture/output move.
- For one source/range, prepare one retained view and starting snapshot, sharing
  comparison handles where applicable. Reserve every target binding and required
  retirement record before committing the group at the existing quantum boundary.
- Logical grab completion includes coherent content, state, and ready bindings.
  Actual restoration messages use separately reserved output bandwidth and the
  defined play-start behavior; a grab must not claim that unlimited messages can
  physically reach an instrument in one quantum. Measure both costs explicitly.

### Capacities and control work

The initial event profile is 128 loops × 8,192 records × the current 16-byte
record = 16 MiB before history, old versions, and metadata. A full current state
is roughly 9 KiB; controller difference bits alone are 256 bytes per comparison.
These are starting estimates, not a complete memory budget. Stage 1 must choose
block size, history/source/destination counts, checkpoint policy, and finite
snapshot/comparison/view/retirement/transfer capacities and headroom.

Use prepared request/reply or bounded transfer steps for control operations.
Execution owns mutable pools; a control worker must not mutate the engine's live
arrays. Import validation and payload delivery progress off the critical path;
publication switches handles at a quantum boundary. Export provides duration,
starting state, and original event order, not just a list of messages.

## Staged implementation

### 1. Establish contracts, capacity profile, and baseline

- [x] Create `feat/fpga-reference-midi-storage` from the recorded baseline,
  preserving unrelated work. If milestone 1 is unmerged, use a stacked PR against
  `feat/fpga-reference-midi`; record any later base change without merging it.
- [x] Read repository style/build/test guidance. Record focused reference/compiler
  baseline results and the first milestone's documented native-suite failures.
- [x] Specify handle lifetimes, legal history ranges, publication order, channel
  ownership, recording failure policy, and stop/repeat/transition semantics.
- [x] Calculate a complete capacity/work budget for the required profile. Bound
  checkpoint lookup/replay and comparison setup for all supported grab ranges,
  including multi-target replacement. Select geometry and checkpoint strategy.
- [x] Add a small cost/model fixture to test the proposed grab/preparation bound
  before committing to the layout. If it fails, revise the design; do not narrow
  the goal, defer unfinished grab work, or mark unprepared bindings as ready.

Verification: baseline tests and a written, executable bound covering event,
state, comparison, metadata, and reclamation costs. Commit the agreed contracts.

### 2. Implement shared event blocks, views, snapshots, and retirement

- [x] Replace `Take`'s private flat-buffer ownership with prepared block/view pools,
  recording writers, immutable snapshots, and generation-checked retained handles.
- [x] Implement exact range views, independent duration, append into stable tails,
  shared duplication, pinned cursors/exports, and incomplete-recording status.
- [x] Add fixed-cost detachment and resumable reference/metadata reclamation with
  separate foreground reservations and non-accumulating maintenance budgets.
- [x] Preserve a small compatibility adapter for the existing one-loop fixtures
  while migrating callers; remove duplicate authoritative storage at integration.

Verification: compare byte/time/order results with a plain event-list model;
exercise nested ranges, silent content, retakes with retained prefixes, stale
handles, every pool's exhaustion, reference overflow, and complete resource return.
Use heap guards and foreground work counts independent of old content length.

### 3. Establish live-state owners and incremental difference tracking

- [x] Extract prepared input, destination, and playback-context registries from
  `Engine`; make snapshot ownership and binding references explicit.
- [x] Add changed-field/dependency reporting to every interpreter mutation path,
  including reset, bulk copy, controller modes, pedals, and bank/program changes.
- [x] Implement fixed difference maps, sparse summaries, shared comparison keys,
  bounded interested-target dispatch, staged initialization, and epoch invalidation.
- [x] Update destination state only for accepted output, including monitored input
  and generated messages. Keep incomplete state/difference work private and bound
  its completion/resumption within the processing/output-order contract.

Verification: use full comparisons only as a test oracle after randomized updates;
verify drift-and-return clears bits, unchanged values create no work, mappings and
channels remain isolated, duplicate comparisons share work, and fanout is counted.
Check reset/pedal progress, failed initialization rollback, and allocation guards.

### 4. Integrate ordered transitions and multi-loop reference execution

- [x] Replace `EmissionKind::Restore`'s unconditional full traversal with a
  destination-aware transition planner/cursor over differences and dependencies.
- [x] Preserve refined pedal/note and bank/program behavior, with explicit ordinary
  stop/repeat policy. Keep acknowledged progress, reserved recovery, and accepted
  destination-state updates consistent under output refusal and interleaving.
- [x] Replace the one-engine/one-input/one-output integration ceiling with prepared
  bindings and bounded context iteration in `Pipeline::process_midi_block` or its
  successor. Observe each input once and consume existing settled loop records.
- [x] Stage content/binding replacements atomically; retain old contexts until
  their accepted output and required cleanup complete. Coordinate destination-wide
  recovery, sync waits, and fatal audio faults without changing the SP clock.
- [x] Migrate existing fixtures and remove superseded single-take state owners.

Verification: independently specified exact traces for equal/differing controls,
bank dependencies, pedal-held notes, continuation/retrigger policy, repeat, stops,
direct play-to-record, shared comparisons, interleaved monitored input, refusing
sinks, and recovery. Preserve Q=1/16/32/64/128 and callback-partition equivalence.

### 5. Implement continuing history and atomic multi-loop grabs

- [ ] Add rolling append-only history, bounded checkpoint preparation, boundary
  states, and comparison readiness using the Stage 1 strategy.
- [ ] Implement range lookup/state derivation without whole-history replay; keep
  checkpoint/difference work coherent with moving input and destination state.
- [ ] Reserve and publish one-range-to-many-loop grabs atomically, sharing event
  storage, starting snapshots, and matching comparisons. Include loop metadata
  updates at the same existing settlement boundary.
- [ ] Preserve grabbed contents during subsequent rollover and pinned-history
  capacity pressure. Reject a failed reservation before changing any target.

Verification: 1- and 128-target grabs, already populated targets, starts inside
blocks/checkpoint intervals, history wrap, held/pedal-latched start state, silent
margins, different configured destinations, and every preparation failure point.
Assert a single-quantum complete commit and count all state/comparison work; test
the least-prepared legal range, not only an already cached favorable boundary.

### 6. Add bounded content transfers and replacement operations

- [ ] Add coherent retained exports and incremental reads of snapshot, duration,
  and events. Replacing/deleting/recording the source must not alter an export.
- [ ] Add staged imports with incremental validation, reservations, comparison
  preparation, cancellation, and atomic publication. Reuse existing message policy.
- [ ] Expose handle-based duplication and loop-to-loop replacement. Specify the
  preflight/cleanup sequence for replacing a playing target without dangling uses.
- [ ] Bound control-transfer and collector service per quantum, reserve recording
  headroom, and expose progress/capacity status. No background operation may
  silently consume all resources promised to admitted capture or grab operations.

Verification: export/import round trips including initial state and silence;
duplicate divergence after retake; cancellation and malformed input; slow transfer
consumers; saturated queues; concurrent history/capture/playback/grabs with exact
audio and MIDI output comparisons against a run without background transfers.

### 7. End-to-end validation, documentation, and delivery

- [ ] Run a 128-loop session fixture with the required 30-second/100-CC-per-second
  recordings, continuing history, duplication, export/import, multi-loop grabs,
  playback/repeat/stop, replacement, and eventual full reclamation. Also fill
  sequences to 8,192 events and exercise dense admitted boundaries.
- [ ] Stress distinct targets, shared comparisons, maximum configured comparison
  fanout, cold legal grab ranges, expensive state changes, backpressure, and
  resource exhaustion. Verify no leaks, stale reads, late replay, hidden heap work,
  or disturbance of audio/capture from background transfers.
- [ ] Extend the profiling example to report complete engine memory and maximum
  per-quantum work/timing. Separate steady processing, preparation, logical grab,
  restoration output, transfers, and retirement. Record hardware/toolchain/profile
  and timing distribution; software work units are not FPGA cycle counts.
- [ ] Update component/API docs and this plan with capacities, actual bounds,
  ownership diagrams, message/route limits, commands, results, and remaining
  hardware/frontend integration boundaries.
- [ ] Run formatting, test-policy checks, all-feature reference/compiler tests,
  warning-denying workspace build, and the documented complete native suite once
  the integrated implementation is stable. Investigate relevant local regressions
  and report reproduced baseline failures explicitly.
- [ ] Commit completed milestones, push the implementation branch, and create or
  update its PR with local validation evidence. Follow the user's override:
  disregard CI results and review feedback unless explicitly asked. Do not poll,
  wait for, rerun, or address remote CI/review as a completion gate. Do not merge.

Verification: all acceptance tests pass, resources return to their expected
baseline after retirement drains, and the complete work/memory profile is
documented. Report unrelated native baseline failures separately.

Verification commands, in the environment selected by repository build guidance:

```sh
cargo test --locked -p rt_fpga_reference --all-features -p rt_fpga_compiler
cargo fmt --all -- --check
python3 scripts/check_shoop_test_usage.py
RUSTFLAGS="-D warnings" cargo build --workspace
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

Add targeted test commands as the new components land. Use small-capacity,
independent behavioral tests and heap/work assertions during development; run the
large session/profile and full native gates at the final integration milestone.
Repeat broad testing only when new changes or unresolved failures warrant it.
No RTL simulation, synthesis, physical-device, or hardware deadline result is
claimed by these Rust-only checks.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Disregard CI results and review feedback unless the user explicitly asks. Local
  checks and transparent evidence determine completion; remote approval does not.
- The user authorized full execution of this plan with `/goal`.

## Execution evidence

Stage 1: branch created at the recorded baseline; all 99 reference/compiler tests
passed (`midi2-baseline.log`). Contracts and the initial capacity/work model are
recorded in `docs/fpga/midi-reference-storage.md`. The profile uses 256-event
blocks, at most 33 block references for an 8,192-event range, and independent
snapshot/comparison/retirement budgets. A 128-target same-range publication needs
one state derivation, not 128. Checkpoints follow expensive state operations;
the ordinary replay prefix is limited to 31 events.

Stage 2: shared content pools, immutable snapshots/ranges, generation checks,
retained prefixes, and bounded retirement are implemented. The original `Take`
API now delegates to the same store using one allocation block; it no longer
owns a second flat event representation. All 103 reference/compiler tests pass,
including four allocation-guarded ownership/capacity tests. Warning-denying
reference build and Rust test-policy check pass.

Stage 3 progress: stepped updates now notify changed cells; fixed difference maps
track drift and return, support staged initialization, and derive new target maps
from an existing checkpoint. Three new tests pass, including 700 mixed updates
checked against fresh full comparisons, resets/pedals/modes, and heap guards.
The comparison registry uses generation-validated weak snapshot references; live
bindings/history own the strong references. This avoids a reference cycle while
rejecting stale comparisons after snapshot reuse. Live-owner extraction follows
with multi-loop integration.

Stages 3–4: `MidiSession` now owns input, output, playback, and shared comparison
registries. Destination transitions use sparse differences and ordered dependency
groups. All 13 original pipeline regression scenarios were migrated to the new
runtime, including fault/recovery, pedal ownership, sync waits, and incomplete
recordings; the superseded `Engine` was removed. All 116 reference/compiler tests
pass. State transitions, history, control operations, and event ownership are
separate modules. Transfers have shared per-quantum event/setup/metadata grants;
complete cold preparation before the first processing call remains an offline
operation. History/transfer APIs are implemented; their large-session validation
is the next milestone. Reference warning-denying build and test policy pass.
