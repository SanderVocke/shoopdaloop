# Long-term ARM–FPGA boundary plan

Status: milestone 1 implemented in [PR #990](https://github.com/SanderVocke/shoopdaloop/pull/990);
the final expanded local regression was stopped at the user's request, and CI
is unreviewed by instruction. Milestone 2 is the recommended next implementation;
milestones 2–6 have not started.
Target integration branch: `proto/fpga-rust-reference`.
Investigated baseline: `5b7dd0a5b242884f86862991dc7b456a5b091df7`.

## Goal

Make ARM own command admission, request bookkeeping and application responses.
Keep the FPGA responsible for musical time, resident scheduled actions, natural
wraps, repeat waits, synchronization, transition arbitration and audio execution.
After admission, an action must not require ARM to return at its execution time.
Continuing ordinary engine operation must not require ARM command service.

Use the Marlin boundary as the experimental ARM–FPGA boundary. Upstream moved
the original cocotb coverage into the native testbench during milestone 1. Keep
the production Rust resolver/runtime as the independent behavioral oracle. Each
milestone must leave a working end-to-end path with reproducible measurements.

## Milestones, in order

### 1. ARM-owned request slots

- [x] Implement a reusable Rust transition service and an indexed action-install
  interface. Move free-slot selection, duplicate-sequence checking and request
  ownership to ARM. Separate service calls from simulator audio progression.
- [x] Preserve current shared capacities, packed records, pending-list traversal,
  terminal retention and musical resolution. Migrate all submission producers,
  including prepared audio and ZedBoard control streams.
- [x] Compare the service plus RTL with production Rust, including a pause in ARM
  service after admission. Measure command cycles, processing cycles and mapped
  resources against a frozen baseline.

Detailed implementation plan: [PLAN_FPGA_ARM_REQUEST_SLOTS.md](PLAN_FPGA_ARM_REQUEST_SLOTS.md).
Results: [matched measurements](fpga/transition_processor/measurements/ARM_REQUEST_SLOTS.md).
At 272 request slots empty-list admission falls from 1,103 to 16 FPGA cycles;
routed area changes by +13 LUTs, −103 flip-flops and zero BRAM.
PROCESS cycle counts are unchanged. The service-pause comparison covers 1,024
quanta after installation acknowledgements have been consumed.

The remaining hardware still stores 660-bit request rows, 832-bit loop rows and
256-bit events at the measured deployment size. Removing the sequential scan
left these RAMs and most wide record-update logic intact, explaining the nearly
unchanged area. Pending-list insertion is also unchanged: the last admission
into a dense 272-action list still takes 1,103 FPGA cycles.

This milestone establishes the boundary; it does not establish full continuous
audio autonomy or a target percentage of area reduction.

### 2. Compact actions and results

Combined implementation plan for milestones 2–3:
[PLAN_FPGA_COMPACT_ACTIONS_AND_MEMBERSHIP.md](PLAN_FPGA_COMPACT_ACTIONS_AND_MEMBERSHIP.md).
It preserves a measured milestone 2 checkpoint before the membership cutover.

Recommended next step: use the existing Rust owner to stop carrying the full
public request/result representation through the FPGA execution machinery.
Deliver this in two reviewable parts, without changing pending-list ownership
or musical resolution.

- [ ] Audit each request/result field against its RTL readers. Specify a compact
  execution record and bounded terminal record, and identify which fields Rust
  can reconstruct from its retained original command. Start with the request
  path; shrinking loop/event records is not a prerequisite. Preserve all fields
  needed for frame rounding, source waits, operation validation and arbitration.
- [ ] Implement compact action storage and terminal readback. Rust reconstructs
  public completions using the retained request plus authoritative FPGA outcome,
  reason and effective/terminal frames. Keep slot/generation guards and terminal
  retention until successful collection. Keep an ordering key in hardware
  wherever supersession or stop cancellation depends on command order; moving
  public sequence bookkeeping to ARM does not remove that requirement.
- [ ] Separate command acknowledgements and terminal readback from the execution
  FSM using bounded storage and explicit admission/publication boundaries.
  Held replies must not stall already-admitted real-time work. Full terminal
  storage may exhaust admission credits; it must retain results without stopping
  audio. Keep the synchronous ARM exchange and coordinated-reset contract.
- [ ] Compare the new path end to end with production Rust through both Marlin
  transports and prepared streams. Cover exact audio/state/events/completions,
  retained results, stale identities, reset, full credits and extended service
  pauses. Exercise held replies while the fabric has executable work under its
  existing fence protocol. Keep expected musical decisions in the Rust oracle.
- [ ] Measure each part against milestone 1 at the same capacities, tools and
  constraints. Report routed LUT/FF/BRAM and setup/hold, command/PROCESS cycles,
  transfer sizes and native service cost separately. Publish the resulting wire
  contract and measurements before proceeding to pending-membership changes.

Expected gain: fewer request RAM bits and narrower read/modify/write paths should
reduce mux, register and control logic. This is a more direct opportunity for
LUT savings than the allocation-scan removal. BRAM savings depend on physical
width/depth mapping and the cost of separate terminal storage; they are not
guaranteed. No percentage or final record width is promised before the field
audit and matched implementation measurements.

Separating replies should also remove a command-service backpressure dependency
from execution. PROCESS may still take the same number of cycles: the resolver
and pending-list traversal remain. Dense-list admission is a milestone 3 target;
per-quantum fences and application settlement pauses remain for milestones 4
and 5. This step alone does not qualify ARM audio-callback performance or board
deadlines.

### 3. ARM-managed pending membership

Execution is covered by the same
[combined plan](PLAN_FPGA_COMPACT_ACTIONS_AND_MEMBERSHIP.md), including atomic
publication, stale membership, slot reuse and service-pause verification.

- [ ] Move list insertion, unlinking and later compaction to ARM-prepared
  membership published at defined boundaries. Compare bounded slot rosters with
  the current traversal before selecting the representation.
- [ ] Keep eligibility, source-tick consumption, same-boundary arbitration,
  supersession and stop cancellation in hardware. Inactive roster entries must
  have a bounded cost even while ARM is absent.
- [ ] Preserve the shared request pool unless a documented capacity-policy change
  is approved. Recheck dense pending sets, sync chains and cancellation, and
  measure worst observed work with inactive entries left uncollected.

### 4. Autonomous audio progression

- [ ] Remove the need for ARM-produced per-quantum fences. With no published
  update, process normally using installed state and actions. Publish complete
  control batches atomically at defined boundaries within a bounded budget.
- [ ] Define admission deadlines and late-update behavior. An incomplete update,
  command flood, held acknowledgement or full result storage must not become an
  audio scheduling dependency.
- [ ] Use continuous fabric I/O to verify extended ARM pauses, automatic wraps,
  repeat waits, sync propagation and resident future actions. Preserve existing
  audio publication, memory-fault and coordinated-reset contracts.

### 5. Application integration

- [ ] Put command management and response service outside the ARM audio callback.
  Replace the application adapter's per-quantum settlement pause with autonomous,
  frame-stamped observations that ARM can consume later.
- [ ] Audit monitoring, capture starts, retakes and media retention at transition
  cuts. Behavior needed at that cut stays device-side; ARM must not supply a
  continuation for required audio or media work.
- [ ] Compare application commands, queries, audio, exports and ownership with the
  production runtime. Distinguish delayed observation delivery from the frame at
  which the observation or action actually occurred.

### 6. Qualification

- [ ] Run broad production/RTL and application regressions, long sessions,
  adversarial command traffic, service stalls, exhaustion and reset checks.
- [ ] Measure ARM service load and memory/bus interference separately from the
  audio deadline path. Bound control traffic and assess the full processing
  budget, rather than extrapolating from an isolated TP benchmark.
- [ ] Close routed timing for the integrated design and qualify the selected
  board's clocks, DDR/DMA behavior and physical audio path. Simulation establishes
  behavior and measured cases; physical deadline claims require board evidence.

## Rules across milestones

- ARM compiles requests into actions; it does not predict pulse times or execute
  a second loop resolver. All accepted future and pulse-relative actions remain
  resident until terminal or explicitly cancelled.
- Keep dynamic operation validation in FPGA where it depends on execution-time
  state. Protect every interface with bounds, identity and reset-epoch rules.
- Keep real-time capture allocation, reference safety and required media work in
  the device. Preserve the existing sticky-fault/silence/reset contract.
- Preserve public musical behavior, canonical ordering and completion ownership.
  Any proposed semantic or capacity change must be explicit.
- Use one reusable Rust ARM service. Marlin tests the combined service and RTL;
  the native Marlin testbench supplies parameterized protocol/kernel coverage
  without a second ARM implementation in Python. Expected decisions come from
  production Rust.
- Record exact source revisions, capacities, tool versions and constraints for
  comparisons. Keep command, processing, observation and host-service costs
  separate. Do not impose an unmeasured area-saving claim on early milestones.
- Follow the milestone plan's commit, validation, PR and review process. Update
  this roadmap as milestones finish; revise implementation choices when evidence
  warrants it without silently relaxing the goal or acceptance criteria.
