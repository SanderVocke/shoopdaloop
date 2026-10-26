# Change instructions — chapter 17: graph metadata capacity and work

Destination: the matching chapter under `docs/`. Baseline: empty; archive sources
`performance.md`, `planning.md`, `graph-activation.md`. Earlier changed resource,
MIDI-index and observation formulas still need full Stage 9 backfill.

Add the new graph-bank storage contract:

- `PreparedPlan::storage_bytes()` counts its descriptor object and every vector's
  capacity, not only populated rows. Allocator overhead is excluded. A short plan
  with excessive spare capacity can be refused; row count alone is not sufficient.
- Stage preparation calculates a conservative per-owner allowance from configured
  maximum nodes/edges and the sizes of NodeStep, Connection, Membership and
  DecisionBinding rows, plus PreparedPlan itself. Checked aggregate arithmetic
  covers `count + 1` compatible plan owners and `count` inline slot descriptors,
  refusing totals above i32::MAX. This deliberately overcounts some inline metadata;
  do not present it as a measured exact heap footprint or a portable numeric constant.
- Candidate admission checks the actual retained capacities. Activation also checks
  the displaced active owner, because a legacy path may have supplied oversized
  metadata. Refusal preserves ownership and current output. Staging does not grow
  descriptor capacity; returned owners keep their original capacities.
- Command payloads, compiler temporaries, validator scratch, buffers, resources and
  the rest of runtime preparation have separate costs. Do not imply that this
  graph-bank formula is an entire-application memory budget.

Separate work classes: incremental validation shares the callback-wide allowance;
internal intervals do not refresh it. Activation preflight may scan prepared
node/input-port state for pending MIDI compatibility. The final owner swap is
constant work and does not free the displaced plan. This is finite configured
work, not a universal deadline guarantee.

Recovery preemption adds one fixed inline held-normal slot, not another admission
credit or dynamically allocated owner. Existing operation/publication budgets still
apply; cancelled recovery consumes work and return allowances. Validator progress
must survive unrelated completions. Normal work can still be starved if higher-
priority recovery consumes every configured allowance indefinitely; do not promise
unconditional normal-lane fairness.

Evidence/source: `PreparedPlan::storage_bytes`, `GraphStages::prepare/accepts`,
`GraphRuntime::prepare_graph_staging`, graph capacity/exhaustion tests, and the
new guarded lifecycle preemption test. Use platform-native sizes in worked examples,
check arithmetic against current source, and keep observed benchmark timings and
Miri interpreter durations in evidence rather than turning them into deadlines.
