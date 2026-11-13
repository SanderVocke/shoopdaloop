# Long-term ARM–FPGA boundary plan

Status: milestone 1 is implemented in [PR #990](https://github.com/SanderVocke/shoopdaloop/pull/990).
Milestones 2–3 are implemented in the stacked [PR #993](https://github.com/SanderVocke/shoopdaloop/pull/993),
with broad local verification and targeted corrected reruns complete. CI is unreviewed by instruction.
The [completion plan](PLAN_FPGA_BOUNDARY_COMPLETION.md) adds a measured TP area
pass before milestones 4–6, then covers autonomy, application integration and
physical qualification. Those implementation stages have not started.
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

- [x] Audit execution fields and move original command echoes/public reconstruction
  to the shared Rust owner. Preserve full u64 ordering and authoritative hardware
  frame rounding, countdowns and terminal decisions.
- [x] Store bounded compact action/results in the same reserved cell: 397 bits
  plus temporary pointers at this checkpoint (417 bits at R=272), versus 660.
- [x] Separate the held host reply from fabric PROCESS/FINISH start/done/error.
  Already-runnable execution and loop reads continue with unread replies.

The independently measured M2 checkpoint is `47f1fbcb`: routed 11,356 LUTs,
7,898 FF and 25.5 BRAM equivalents at 64/272/400, meeting standalone 100 MHz.
Against the fresh common-harness M1 baseline it saves 3.5 BRAM and 250 FF, with
36 more LUTs. PROCESS cycles remain unchanged at this checkpoint.

### 3. ARM-managed pending membership

- [x] Compare linked membership and flat rosters on the actual resolver. Select
  generation-qualified linked nodes for faster dense/stale traversal and one
  fewer BRAM; retain only that representation in production.
- [x] Publish complete immutable banks with BEGIN/NODE/ROOT/COMMIT at legal
  execution boundaries. Final INSTALL admits and rounds against the current
  FPGA frame. Partial publication leaves the old view executable.
- [x] Remove FPGA insertion, unlinking and neighbour repair. Hardware retains
  eligibility, source ticks, arbitration, supersession and stop cancellation.
  Stale nodes have capacity-derived visit bounds and require no ARM cleanup.
- [x] Preserve shared capacity, terminal credits, duplicate retention, checked
  generation retirement and coordinated reset. Static streams reserve real
  credits and compile one boot membership without predicting outcomes.

The combined implementation is `708d34a6` plus producer/regression fixes.
Dense R=272 PROCESS falls from 5,465 to 4,380 cycles (19.9%). Complete dense
admission grows to 276 wire exchanges / 1,654 elapsed fabric clocks in the
one-loop workload. These are command-service costs; milestone 5 still owns the
application service/callback separation, and no ARM deadline is qualified here.
The integrated 1,024-quantum comparisons include held replies and partially
staged membership while exact audio and resumed results/media match production.

Matched final routing meets standalone 100 MHz (+0.105/+0.007 ns setup/hold),
but costs 13,312 LUTs / 8,813 FF / 28.5 BRAM equivalents. Relative to the common
M1 baseline this is **17.6% more LUTs**, 8.2% more FF and only 0.5 fewer BRAM.
Publication banks and guards offset much of the TP storage saving. The matched
complete-core geometry separately saves 592 LUTs (2.0%), but setup worsens from
-1.353 to -1.845 ns; both miss 100 MHz. Milestone 6 owns that timing closure.
M2–3 do not establish TP LUT reduction. The completion plan makes a measured
reduction against the current TP a deliverable before autonomy/integration,
with final verification after timing and board work.

Detailed [execution plan](PLAN_FPGA_COMPACT_ACTIONS_AND_MEMBERSHIP.md),
[wire/ownership contract](fpga/transition_processor/COMPACT.md) and
[measurements](fpga/transition_processor/measurements/COMPACT_ACTIONS_AND_MEMBERSHIP.md)
record the checkpoint separation, tradeoffs and remaining qualification.

### 3a. Attribute and reduce TP area

- [ ] Attribute the M3 LUT increase with controlled, matched experiments; separate
  publication/mask/mux costs from resolver datapaths and synthesis mapping.
- [ ] Implement the smallest sufficient publication contract for one trusted ARM
  producer, preserving device safety, atomicity, generation lifetimes and service
  independence. Publish a routed area checkpoint before M4.
- [ ] Demonstrate final TP LUT reduction against the 13,312-LUT M2–3 result at
  unchanged capacities and 100 MHz; target recovery toward the 11,320-LUT M1
  baseline. Account for final observation, timing and board integration costs.

Detailed stages, acceptance gates and resource comparisons are in the
[completion plan](PLAN_FPGA_BOUNDARY_COMPLETION.md).

### 4. Autonomous audio progression

Execution is covered by the [completion plan](PLAN_FPGA_BOUNDARY_COMPLETION.md).

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

The completion plan makes device-side observation/media continuity a prerequisite
for moving application command and response work off the deadline path.

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

The completion plan includes the real ARM control/audio integration currently
absent from the halted-CPU benchmark and dummy-audio firmware. Physical board
evidence remains required for completion; simulation is an independent gate.

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
  separate. Preserve the early milestone measurements and verify the explicit
  final area target in the completion plan without claiming an unmeasured saving.
- Follow the milestone plan's commit, validation, PR and review process. Update
  this roadmap as milestones finish; revise implementation choices when evidence
  warrants it without silently relaxing the goal or acceptance criteria.
