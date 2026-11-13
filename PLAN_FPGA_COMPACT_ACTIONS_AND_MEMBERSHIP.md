# FPGA boundary milestones 2–3: compact actions and ARM-managed membership

Status: implementation in progress on the existing branch and PR #993.
Execution base: `32d835faadcbb74eeec8d02fb5212a37939e1321`; parent #990 remains
`45d535166d931f798fd4cb61e7433c89842fc1ed`.
Parent roadmap: [PLAN_LONGTERM_FPGA_BOUNDARY.md](PLAN_LONGTERM_FPGA_BOUNDARY.md).
Prerequisite: [request-slot milestone](PLAN_FPGA_ARM_REQUEST_SLOTS.md),
[PR #990](https://github.com/SanderVocke/shoopdaloop/pull/990).
Planning branch: `plan/fpga-compact-actions-membership`.
Stack base: `feat/fpga-arm-request-slots` at
`45d535166d931f798fd4cb61e7433c89842fc1ed`.
Eventual integration target: `proto/fpga-rust-reference`.

The user has now authorized full implementation, including the local verification
in this plan. The instruction not to inspect CI after pushing remains in force.
No CI success or review approval is inferred from publishing changes.

## Goal and scope

Execute roadmap milestones 2 and 3 in order, with an independently measurable,
working checkpoint between them:

1. Store compact executable actions and bounded terminal results in FPGA;
   reconstruct public request/completion data in the existing Rust owner.
   Separate host acknowledgements/readback from real-time execution.
2. Have ARM prepare pending membership and publish it at explicit boundaries.
   Remove FPGA list insertion and unlinking while retaining all musical
   eligibility, countdown, arbitration and terminal decisions in hardware.

An admitted action must remain executable without further ARM service, including
actions triggered by wraps, self-sync, propagated pulses and repeat waits. ARM
must never compute future pulse times or provide a continuation at execution.

Preserve public mono/application queues, supported-operation whitelists, shared
request capacity, generation retirement, receipt ownership and existing musical
behavior. Cover the current primitive TP operations, positioned play/seek,
retrigger and the application's empty-content clear extension at all supported
quanta. Physical wire records and internal RAM layouts may change coherently.

Removing mandatory per-quantum fences, removing application settlement pauses,
moving required media work to ARM, adding composites/new musical modes, defining
a physical ARM register map and qualifying board deadlines are outside this plan.
Only the coordinator/transport changes needed to isolate replies and publish
membership safely belong here. Fence-free progression remains milestone 4;
application callback integration remains milestone 5.

## Starting evidence and code

At the stack base the Rust `TransitionService` retains original 20-word commands,
selects the lowest reusable slot, checks retained sequences and permits one
outstanding wire exchange. FPGA INSTALL still uses a 640-bit command interface
and 832-bit response interface. At 64 loops / 272 requests / 400 events, request
RAM rows are 660 bits, loop rows 832 bits and events 256 bits.

The [milestone 1 report](fpga/transition_processor/measurements/ARM_REQUEST_SLOTS.md)
records 11,310 routed TP LUTs, 8,299 flip-flops, 29 BRAM36 equivalents and
+0.103/+0.006 ns setup/hold slack at 100 MHz. Empty-list admission is 16 cycles;
the last insertion into a dense 272-action list is 1,103 cycles. Dense PROCESS
is 5,465 cycles. Those are reference cases, not complete-system deadline bounds.
The final expanded component regression was interrupted at the user's request;
do not relabel it as a completed baseline run. Use its actual recorded evidence
and establish missing focused baseline coverage when implementation is authorized.

Relevant implementation:

- `fpga/transition_processor/rtl/transition_processor.sv`: `R_*` fields,
  `HEAD_AT`/`HEAD_SYNC`, `absolute`, `S_INSERT_*`, `S_UNLINK_*`, terminal writes,
  source-first resolution and `reply`/ready gating.
- `fpga/loop_pipeline/rtl/loop_pipeline.sv`: `run_ready`, host/coordinator command
  routing, shared TP responses and `HOST_SETTLE_WAIT`. A host response must not
  be mistaken for an internal PROCESS/FINISH completion.
- `fpga/audio_io/rtl/audio_control.sv`: command/fence dispatch and response waits;
  prepared audio and ZedBoard paths must obey the same ownership rules.
- `src/rust/rt_fpga_adapter/src/{transition_service,protocol,looping,engine}.rs`:
  retained originals, transfer lifecycle, public completion decoding and adapters.
- `src/rust/rt_fpga_testbench/src/{drivers/transition,drivers/loops,prepared}.rs`,
  `suites/{transition,production,production_faults,continuous,demo,zedboard}.rs`:
  native wire driver, preparation and production comparisons.
- `src/rust/rt_fpga_sim/src/simulator.rs`, `rtl/marlin_pipeline.sv` and
  `tests/transition_boundary.rs`: independent audio/service progression.
- `src/rust/shoop_rt_host_fpga_sim/tests/`: application/host ownership, audio,
  arithmetic, capture and media regressions.
- `src/rust/rt_backend_testbench/src/transition_service.rs`,
  `scripts/rt_backend_testbench/__init__.py`, `fpga/abi/transition.py`,
  `fpga/audio_io/prepare_demo.py`, `fpga/zedboard/prepare.py`: remaining packed
  consumers and Python input preparation using the same Rust owner.
- `fpga/transition_processor/{measure.py,measure_commands.py,check.sh}`:
  reproducible physical and command-cost measurements.

## Immutable acceptance criteria

1. At identical logical delivery boundaries, production Rust and service-plus-RTL
   agree on loop state, ordered events, pending actions, completion identity,
   requested/effective/terminal frames, outcomes and reasons. Q8.24 audio,
   captured/exported samples and media ownership agree exactly; retain the
   independent float-accuracy gate and existing supported profiles.
2. Preserve public admission/error precedence, shared pool limits, lowest-slot
   reuse, duplicate retention through terminal collection, cancellation without
   release, pending collection, checked generation retirement and coordinated
   reset. Uncertain exchanges fault the owner rather than permitting unsafe reuse.
3. Compact records retain every execution-dependent field. Original command
   echoes and public response construction move to Rust. Hardware retains actual
   frame rounding, countdowns, ordering comparisons, dynamic validation and all
   real-time musical decisions. Numeric ranges and overflow behavior do not shrink.
4. A stalled host acknowledgement/readback, full terminal storage or an unfinished
   membership update cannot stall already-admitted executable work. Control may
   be backpressured or refused without overwriting replies/results. This applies
   when the current fabric fence/run prerequisites are satisfied; do not claim
   progression in the absence of fences still required by milestone 4.
5. Membership publication is atomic relative to execution. An interrupted update
   leaves the last committed membership usable. A publication cannot resurrect a
   completed/cancelled action, reset a countdown, duplicate a source tick, expose
   a staged action early or make a stale slot reference execute a reused action.
6. FPGA no longer performs pending-list insertion, predecessor/successor repair
   or unlinking on install, cancellation or terminal resolution. It may mark an
   action inactive and traverse published membership for real-time decisions.
   Traversal and storage have finite capacity-derived bounds even if ARM never
   compacts terminal entries. No per-loop partition reduces the shared capacity.
7. After admission, stop all request/membership service for at least 1,024 quanta.
   Future absolute actions, source waits, self-sync, repeat waits, source chains,
   supersession, failed higher-priority actions and stop cancellation continue
   correctly. Retained terminal records consume their normal credits; resume and
   reconstruct exact results without service calls hidden in the audio harness.
8. All producers use one reusable Rust service and one final wire contract.
   Prepared streams may not depend on expected resolver outcomes or collect
   credits they never actually collect. Regenerate affected artifacts and reject
   incompatible prepared inputs explicitly. No synthesized legacy mode remains.
9. Publish matched M1, M2 and combined M2–3 resource/timing and command/processing
   measurements at unchanged capacities and constraints. Preserve the standalone
   TP 100 MHz target. Report full-core timing separately and expose regressions;
   no percentage of area saving or board deadline is assumed in advance.

## Design rules and decisions to settle

### Compact records and response ownership

- Write a field-use table before choosing widths: field, readers/writers,
  execution lifetime, public reconstruction source, numeric range and storage
  location. Distinguish immutable action data, mutable execution state, terminal
  data and membership metadata. Count the total physical storage, including any
  separate terminal/status banks and duplicated publication banks.
- Keep slot/generation identity. Preserve full sequence ordering until an
  alternative is proved for arbitrary valid u64 sequences, including out-of-order
  arrival. Sequence affects offer/protected arbitration and cancellation of older
  actions; it is not merely reply metadata. Keep position/revision interpretation
  correct for seek versus the existing clear extension.
- Use retained originals to reconstruct public identity/requested fields. FPGA
  supplies authoritative effective/terminal frames and outcome/reason. Pending
  readback must still obtain mutable countdown/status from hardware. Extend the
  raw inspection contract where reconstruction is impossible without the owner;
  do not silently invent fields in testbench decoders.
- Reserve terminal capacity for every admitted action. Do not replace it with an
  event FIFO whose overflow or absent consumer becomes an execution dependency.
  Collection must capture a stable response before releasing device storage;
  Rust releases ownership only after a validated successful reply. Lost replies
  retain the existing uncertain-transfer fault policy.
- Keep the existing synchronous public exchange. An operation may use a bounded
  sequence of wire transfers, with at most one outstanding transfer and explicit
  reserve/stage/commit/refuse/uncertain transitions. No unbounded poll-until-audio
  work or command management is added to the audio callback.
- Separate host reply storage/routing from coordinator start/done/error traffic.
  Preserve ready/valid payload stability and internal fault propagation. Audit
  `busy`, `command_ready`, `run_ready` and host-settle phase transitions together;
  changing only TP response registers is insufficient. Runnable audio has bounded
  priority over command floods. Bound any control work admitted between quanta.

### Membership ownership, publication and slot reuse

- ARM prepares membership only; FPGA still determines eligibility, consumes
  source ticks, resolves winners, cancels older actions on stop and handles
  wraps/pulses. Terminal resolution marks authoritative state; it does not need
  ARM to repair membership before the next quantum.
- Compare two concrete representations in stage 5: ARM-written ordered linked
  membership with inactive nodes retained, and shared ordered slot rosters with
  per-loop/timing-class descriptors. Evaluate atomic update cost and stale-entry
  handling for each. Select one from measurements and document the decision;
  do not carry both implementations into the final design.
- Preserve canonical traversal and tie behavior, including the current slot
  ordering where observable. Membership storage must have an explicit bound in
  terms of shared request capacity, loop descriptors and a fixed bank count.
  Avoid silently multiplying request capacity by loop count or imposing per-loop
  quotas. State maximum visits per phase, pulse, loop and quantum, including stop
  cancellation and stale entries; finite storage alone does not bound all work.
- Stage writes into storage the resolver cannot observe, then publish a complete
  descriptor/version at a legal boundary. Specify bank/version ownership and
  retirement acknowledgement. A delayed writer cannot edit the active view or
  overwrite a bank still referenced by execution. Do not copy mutable countdowns
  or terminal flags from an ARM snapshot back into hardware during publication.
- Define the public admission point at successful action-and-membership commit.
  Preserve error precedence and frame rounding against the actual FPGA frame at
  that cut. If audio advances during staging, do not reuse an earlier rounded
  frame or expose an accepted action that is absent from committed membership.
  Failed/uncertain multitransfer operations need explicit rollback/quarantine.
- Every membership reference needs a proved lifetime rule: generation-qualified
  identity or enforced removal/retirement before reuse. Skipping terminal/stale
  entries must use authoritative FPGA state. Test collection/reuse while old
  entries and held replies still exist, and prevent duplicate live membership
  across publication versions. Host compaction is optional for continued audio.
- Hardware guards bounds, identities, versions, bank ownership and finite walks.
  ARM may validate global construction properties as the trusted producer, but
  malformed low-level traffic must not permit out-of-bounds access or an infinite
  walk. Do not recreate the removed allocator/list manager as a complex hardware
  publication validator.
- Treat active execution, pending publication, terminal collection and reset as
  one documented ownership state machine. Reset invalidates all old transfers,
  bank references and cached service metadata together. New publication/version
  counters need checked exhaustion behavior; wrapping must not alias an old
  reference. Do not add host-only reset.

## Stages and verification

Execute stages in order. The M2 checkpoint gates M3 cutover; only narrow layout
experiments needed for the stage 5 decision may precede it. Keep each wire
cutover coherent across consumers and commit each working milestone.

### 0. Establish the stack and execution baseline

- [x] Create the planning branch from the fetched #990 head recorded above.
- [x] When implementation is requested, refresh #990 and preserve the stacked
  relationship; record the exact execution base and any upstream changes. Reuse
  this branch/PR or create an explicit implementation child without modifying #990.
  Do not include unrelated upstream history merely to obtain a newer tool setup.
- [x] Freeze M1 RTL/service/driver sources and configuration manifests under
  `work/`. Verify which existing M1 reports match those hashes; rerun only missing
  or invalidated baseline cases, including relevant interrupted-regression coverage.
- [x] Save focused baseline observations for dense lists, retained terminals,
  arbitrary sequence order, held host responses and fabric-run prerequisites.
  Separate known coupling that this plan changes from musical semantics it preserves.

Gate: reproducible base, producer inventory and explicit delivery cuts; no
unverified full-baseline or final-CI pass claim.

### 1. Specify records, ownership and boundary contracts

- [x] Complete the field-use/storage audit and exact compact wire/RAM layouts.
  Specify pending/terminal/empty/retired states, inspection data, collection
  snapshot/release timing and public response reconstruction.
- [x] Specify host reply versus fabric execution interfaces, queue depths,
  arbitration, fault routing and behavior of held/full replies at every phase.
- [ ] Specify the publication/reuse state machine sufficiently to avoid designing
  M2 storage that cannot support M3. Leave the final membership representation to
  stage 5. Document acceptance/rounding cuts for staging that spans quanta.
- [x] Add the ABI/version/capability and prepared-input invalidation strategy to
  the installation contract; enumerate every codec, adapter and generated input.

Gate: each omitted field has a justified reconstruction source or is obsolete;
each mutation has an owner and linearization point. No bit-width or identity
truncation is accepted on intuition alone.

### 2. Extend the shared Rust owner and codecs

- [x] Add compact action/result codecs, public completion/readback reconstruction
  and bounded multitransfer lifecycle support where needed. Keep queue interfaces
  and public supported profiles intact; avoid new simulator/oracle dependencies.
- [x] Extend owner metadata for staged/published/terminal/reusable identities.
  Keep duplicate retention and lowest-slot/generation policies unchanged.
- [x] Use small transport doubles for error precedence, pending/terminal reads,
  generation retirement, refusal rollback, malformed replies, uncertain commits,
  reset and collection snapshots held while execution proceeds. Doubles do not
  perform musical resolution.

Gate: focused service/codec tests prove lifecycle and exact reconstruction before
the hardware wire cutover. Production Rust remains an independent resolver.

### 3. Compact FPGA actions/results and migrate producers

- [x] Introduce compact action/execution/terminal storage and eliminate obsolete
  wide copies and writeback mux paths. Retain existing pending lists for this
  checkpoint so storage gains are measured independently of membership changes.
- [x] Integrate codecs/readback with both Marlin paths, mono/engine adapters,
  regular host, bridge and Rust/Python preparation. Regenerate demo/board assets
  and update manifest/version guards in the same coherent cutover.
- [x] Pass raw identity/bounds/phase/reset guards, transport stability, service
  lifecycle tests and a small production/RTL record-play-stop/capture comparison.
  Then run all five TP capacities with the existing directed cases and 20 seeds
  each, including requested/effective/terminal frame comparisons.

Gate: a working compact-record implementation with unchanged public behavior;
no synthesized full-record compatibility path or second software owner.

### 4. Isolate replies and publish the M2 checkpoint

- [x] Route PROCESS/FINISH and host replies independently through TP, coordinator,
  Marlin and fabric wrappers. Holding a host reply must not consume an internal
  completion, gate an already-runnable quantum or defer required fault handling.
- [x] Exercise held install/cancel/collect/readback replies, full credits, delayed
  collection, reset in flight and continuous command attempts. Test TP and
  integrated audio separately. Identify exactly which fence/run inputs are still
  supplied; do not use hidden ARM service to sustain the tests.
- [x] Repeat the independent 1,024-quantum service pause and bounded application
  replay. Compare exact audio, ordered events, state, results and captured media.
- [x] Run matched M1/M2 command/processing measurements, lint/generic synthesis
  across capacities and deployment-size Vivado synthesis/route. Publish widths,
  resource/timing deltas, response storage cost and remaining dependencies.

Gate: milestone 2 acceptance is demonstrated and measured before removing lists.
If separate result/reply storage offsets savings, report it and simplify before
proceeding; do not hide the cost inside the M3 comparison.

### 5. Select bounded ARM-prepared membership

- [ ] Prototype the two stage-1-compatible membership representations on the real
  resolver path using small directed workloads. Compare host preparation cost,
  publication bytes/cycles, stale-entry visits, total storage and mapped logic.
  Neither prototype may implement another musical resolver.
- [ ] Evaluate empty/dense membership, all actions on one loop, 64-loop chains,
  mixed timing classes, maximum uncollected terminals and reuse before compaction.
  Derive visit bounds and state the no-ARM-compaction worst case.
- [ ] Choose and document one representation, publication protocol and numeric
  capacity/visit limits. Show how it preserves the full shared pool and fits the
  processing budget under the unchanged target clock. Retain only that candidate.

Gate: a concrete decision backed by measurements and ownership/race analysis.
If worst-case stale traversal is too costly, redesign indexing/filtering; do not
require timely ARM cleanup, reduce public capacity or change musical semantics.

### 6. Implement atomic publication and safe reuse

- [ ] Add ARM membership preparation/compaction to the shared owner, with bounded
  staging storage and explicit version/bank ownership. Preserve one outstanding
  wire transfer and reserve public acceptance until the whole operation commits.
- [ ] Implement device staging/commit/retirement guards at legal boundaries.
  Incomplete or refused publication keeps the old committed view executable.
  Perform authoritative rounding at the defined admission cut.
- [ ] Verify partial writes, delayed/held acknowledgements, publication versus
  terminal/cancel/collect races, bank exhaustion, reused generations, stale
  versions and reset after every transfer phase. Check old replies cannot be
  interpreted as acknowledgements of a new owner or publication.

Gate: publication state-machine and raw protocol tests pass, including a service
pause halfway through an update while existing admitted work continues.

### 7. Replace hardware list maintenance and complete M3 cutover

- [ ] Traverse committed membership using authoritative active/terminal state;
  preserve slot/sequence arbitration, source-first settlement, self-sync,
  once-per-pulse countdown, repeat waits, pulse masking and stop cancellation.
- [ ] Make terminal resolution and cancellation mark state without repairing
  membership. Remove `S_INSERT_*`, `S_UNLINK_*`, request next/previous writeback
  and obsolete head/update logic. Remove or repurpose metadata only after all
  readers are accounted for. Keep real-time cancellation traversal in hardware.
- [ ] Migrate adapters, raw guards, prepared streams and all observations to the
  final publication contract. Static streams reserve actual credits and compile
  membership from inputs without predicting when actions finish.
- [ ] Re-run directed TP and pipeline comparisons, then all five TP capacities
  with 20 seeds each, delayed admission/cancel cuts and bounded application replay.

Gate: one final implementation contains no hardware insertion/unlink machinery,
all public behavior remains equivalent, and every producer uses the shared owner.

### 8. Prove service independence and measure combined gains

- [ ] Extend the 1,024-quantum comparison to cover no membership compaction, all
  terminal credits retained, stale entries across reuse, partially staged updates,
  held replies and a stalled collector. Keep the audio/fence driver independent
  of request management; log any intentionally supplied fabric prerequisites.
- [ ] Cover multiple eligible actions, out-of-order sequences and u64 edges,
  failing high-priority operations, stop cancelling older future actions, self-sync,
  source chains, repeat waits and exactly one countdown decrement per delivered
  source tick. Compare resumed results and media with production Rust.
- [ ] Record M1/M2/M2–3 deltas for empty/dense install, cancel, collect, publication,
  compaction, PROCESS and observation. Include growing stale membership and the
  8x8/64-loop-chain workloads. Account for all host preparation and transfer work;
  a fast INSTALL alone is not the admission cost if publication sends a roster.
- [ ] Repeat matched deployment-size routing and supported-capacity lint/mapping.
  Record total action/result/membership/reply storage, LUT/FF/BRAM, setup/hold,
  source/tool/harness hashes and full-core timing impact. Preserve the same part,
  100 MHz constraints, capacities and Vivado version as the M1 comparison.

Gate: measured structural changes and bounded stale-membership cost, with no
unsupported callback, full-core timing or board-deadline claims.

### 9. Final end-to-end validation, documentation and handoff

- [ ] Once implementation is authorized and stable, run the full native suite
  once, the complete relevant native Marlin matrix, simulator suite and regular
  FPGA-host/application suites. Include production faults, stalls, instant/fixed/
  DRAMsim3 memory, long sessions, 64-loop workloads and independent float bounds.
- [ ] Check generated wrappers, prepared ROM parity, continuous fabric audio,
  demo recovery and deployed ZedBoard controls. Keep the remaining fence-driven
  flow working. Investigate failures against the frozen checkpoint; never relax
  comparisons or silently promote an interrupted run to a pass.
- [ ] Update INSTALL/TP/adapter/testbench documentation, publish compact/public
  reconstruction and membership contracts, and link measurements from the parent
  roadmap. Check every acceptance criterion against specific evidence.
- [ ] Commit coherent milestones and push only this stack's work. Keep the PR
  based on #990's branch while it is open; after #990 lands, rebase/retarget onto
  its integration result without duplicating parent changes. Do not merge #990
  or this PR autonomously. Rewrite the PR description if it later gains code.
- [ ] Let configured CI run normally. The user's instruction overrides the
  planning template's default CI-to-green inspection loop: do not inspect, poll
  or rerun CI after pushing unless the user later changes that instruction.
  State unreviewed CI honestly. Inspect available automated review feedback and
  address actionable comments with affected local verification when authorized;
  do not claim approval when no review has been submitted.

Gate: complete local evidence and a reviewable stack, with any user-stopped
verification or external blocker explicitly recorded. CI observation and merging
remain governed by the user's instructions, not inferred approval.

## Verification cost and principal commands

For this authorized implementation, begin with owner/codec tests and one small TP or
pipeline case. Broaden at each wire or ownership cutover; use all capacities
after stabilization, then expensive full regressions/routing at the M2 and final
checkpoints as specified above. Reuse valid frozen M1 evidence rather than
rerunning it indiscriminately. Run the full native suite only at the final
behavioral milestone, repeating broader tests only for new changes or failures.

Use the selected environment per `.agents/info/build.md`. Keep native nextest
and builds that replace host executable aliases serialized. Leave waveforms off
except for a focused failure, and retain exact commands/seeds/failure traces.

Representative focused and final gates, adapted to the final wire contracts:

```sh
cargo test --locked -p rt_fpga_adapter -p rt_backend_testbench
cargo run --locked -p rt_fpga_testbench --features sim -- transition --config small --random-cases 2
cargo run --locked -p rt_fpga_testbench --features sim -- transition --config all --random-cases 20
cargo test --locked -p rt_fpga_sim --features sim --test transition_boundary
cargo test --locked -p shoop_rt_host_fpga_sim --features sim --test app_rtl common_app_replay_matches_native_audio_and_loop_observations -- --exact --test-threads=1
cargo run --locked -p rt_fpga_testbench --features sim -- production --directed --random-cases 30 --stalls
cargo run --locked -p rt_fpga_testbench --features sim -- production-faults
cargo test --locked -p rt_fpga_testbench --features sim
cargo test --locked -p rt_fpga_sim --features sim -- --test-threads=1
cargo test --locked -p shoop_rt_host_fpga_sim --features sim --tests -- --test-threads=1
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

Also use the existing generated-input/coverage/float checks and measurement
frontends documented in `src/rust/rt_fpga_testbench/README.md` and the M1 report.
Extend command measurements to retain complete multitransfer publication costs;
do not compare an old full exchange with only the new indexed-write substep.
Before committing Rust changes run `cargo fmt --all` and
`RUSTFLAGS="-D warnings" cargo build --workspace`; changed Rust tests also require
`python3 scripts/check_shoop_test_usage.py`. These are implementation gates.

## Expected gains and limits

M2 directly targets wide record RAMs, registers, muxes and read/modify/write
control; separate reply storage removes a host backpressure dependency. M3 targets
insertion/unlink states, pointer repair and terminal write traffic. Together they
should simplify TP beyond M1's area-neutral allocation-scan removal.

Separate terminal cells, publication banks, guards and stale-entry traversal
have real costs. BRAM changes in physical mapping steps, and an ARM-prepared
roster can increase transfer bytes or service work. PROCESS may improve from
removing unlinking yet worsen when many inactive entries remain. Report measured
net changes, including worst observed cases; promise neither a saving percentage
nor uniformly lower latency. Milestones 4–6 still own fence-free progression,
application callback integration and physical deadline qualification.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.


## Execution evidence

- M1 sources are frozen under `work/fpga-compact/baseline` with a SHA-256 manifest.
  The parent remains `45d53516`; both open PR heads match the recorded stack.
- Focused M1 replay passes all five capacities, seven groups and two seeds per
  capacity. The fresh R=272 command workload reproduces the published 16-cycle
  empty install, 1,103-cycle last dense install and 5,465-cycle dense PROCESS.
- The field-use/ABI audit is in [COMPACT.md](fpga/transition_processor/COMPACT.md).
  M2 uses 429 significant INSTALL bits and 397-bit compact cells plus temporary
  list pointers (417 bits at R=272). Public wire bus widths stay 640/832 bits.
- The compact owner/codec unit suite passes, including exact original/requested
  reconstruction, authoritative effective/terminal frames and large countdowns.
  A small RTL replay passes all seven groups and two seeds; the broader
  five-capacity/20-seed compact-record gate passes before reply isolation.
  ABI refusal/non-mutation and Rust/Python prepared-ROM parity also pass.
  The warning-denying workspace build and Rust test-attribute policy pass.
- The isolated-execution checkpoint is `47f1fbcb`. Eight TP groups and 20 seeds
  pass at all five capacities. A held INSTALL reply survives all 1,024 exact-audio
  quanta; held settlement, cancel, collect and inspection cases also pass.
  The 45-case stalled production corpus, production faults and bounded app replay
  pass. [Matched M1/M2 measurements](fpga/transition_processor/measurements/COMPACT_ACTIONS_AND_MEMBERSHIP.md)
  show 3.5 fewer BRAM equivalents, 250 fewer routed flip-flops, 36 more LUTs and
  positive setup/hold at the unchanged 100 MHz target. All five capacity
  lint/generic checks and default ECP5 mapping pass,
  resuming the interrupted check against a frozen M2 source snapshot.

- The instant-memory prepared-demo loader/recovery suite passes at the M2
  checkpoint. Full M2 RTL and tooling snapshots are retained under
  `work/fpga-compact/checkpoints/m2` for independent later comparisons.
