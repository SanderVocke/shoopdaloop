> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/STAGE6_READINESS.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Stage 6 implementation coverage and verified completion

This is the historical operational acceptance audit for PLAN_LOOP_LAYER.md,
not a new full callback audit, authored guide or reduction of that plan.
**Stage 6 was verified complete at the recorded implementation snapshot.** Consolidated source-matched
Miri was collected successfully:81 commands/228 executions,128 runtime+100 loop,
from immutable `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea`. At that implementation checkpoint the user required a pause before Stage7;
this is not an instruction to pause/commit the current documentation goal.
Stages7–10 and LL-A final implementation acceptance were not closed by this audit. Broader randomized/performance/concurrency/failure-canary work required
by Stage8 is not waived by this audit.

Current APIs are taught in the [runtime book](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/rt_runtime/docs/README.md)
and [loop guides](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/README.md). Source-match/freshness
statements below are those recorded at collection, not a recheck of today's dirty
comments/examples. No independent loop host/benchmark or new audit/performance/
platform campaign is supplied by the documentation pass. This preserves the
implementation plan's obligations without making them documentation dependencies.

## Checklist-to-evidence mapping

| Stage 6 requirement | Implemented path and observed evidence |
| --- | --- |
| Reliable typed sync/countdown/trigger/composite control, cancellation and terminal outcomes separate from automatic progression | `runtime::{Operation,Reply}`, planner pending receipts and resolver requests; runtime, runtime_topology, interloop and typed_execution tests compare admission/delivery/terminal frames, request-family outcomes, cancellation, direct priority and independent recovery. Full normal slots/unread terminal records do not stop automatic wraps/cascades. Native/Wasm evidence below and all selected affected Miri cases pass. |
| Prepared activation/replacement, graph/loop generations, consistent bindings/resources/program cut, unchanged rejected candidates and owning retirement | `runtime/{topology,configuration,roles,planner}.rs`, generic graph activation permit and media owner permit. `runtime_topology` tests cover fixed-role graph/timing cuts, new logical generations over prepared physical templates, removal/reuse, retained old exports, input-state policy and idle capture claims. The four `program_graph_scenarios` cases add a running-program/graph cut with real capture-family preflight and separate retirement. `configuration-stage6` and `program-graph-stage6` evidence cover exact epochs, rejected/Ready/Taken owners, stale scopes/generations, cancellation and faults. |
| Primitive/composite stop, seek, nested activation/replacement; active/pending/stopped states and same-frame precedence | Standalone resolver/program tests cover sparse nested regular/explicit programs, empty/ended/changed programs, zero/stopped cuts, removed children, direct overrides and speculative cancellation. Facade interloop coverage now also seeks an outer program into/back out of inner capture/play segments, combines child Stop with inherited Record, cancels a countdown with a winning Stop, preserves direct child playback under stopped parents, restarts and checks a second capture revision. Full samples/MIDI/public state/journals/exports are independently predicted, not compared only against another partition of the implementation. |
| Coherent state/capture metadata tagged by frame/configuration; bounded queries with no music dependence on consumer credits | `Cut` and `Export` identify frame, graph/topology epochs, logical owner/revision and actual source/capture metadata. Facade tests inspect post-settlement/pre-render cuts, strictly pre-F MIDI state, original F packets, old snapshot immutability, optional export refusal and capacity exhaustion. Every new nested boundary asserts all six states are present. Full domain slots, unread results and retained exports/graph/topology owners do not prevent admitted progression. |
| Full A→B→C + nested composite + FX/capture outcome scenario | Original `interloop::cascades_nested_clocked_program_and_capture_outcomes_match_independent_media_and_journals` remains intact, including the137-page fixture and independent137/138/139-page admission/continuation/refusal calculations. It passes whole/single/irregular partitions, reversed declarations and journal capacities0/1/1024 with full pool recovery. The additional nested-control fixture uses256 pages specifically for sufficient-capacity equivalence, not to change the original exhaustion scenario. |
| Mixed saturation/stalls/stale identities/cancellation/replacement/disconnect/recovery/stop/restart/disposal | The28 topology-facade cases include full normal-domain slots, delayed/stalled results, unread retired graphs/programs, export pressure, incremental source/timing validation, graph/catalog epochs, logical generation reuse, pending selection pins, sequence/ticket/recovery cancellation, source transfer/cancel/reuse, generation exhaustion, disconnect before/after publication and faults before interval finish. The paired path is exercised through128 clock cycles with16 domain slots and unread outcomes; stopped cleanup twice recovers all owners. The nested case supplies integrated seek/stop/restart and immutable old/new captures. Earlier standalone ownership models and transport/concurrency evidence remain separately identified, not relabelled as this integrated scenario. |

The facade does not construct arbitrary processors on the callback. Physical
consumer templates and signatures are prepared and fixed; logical generations can
change while those slots persist. This follows the design's explicit persistent-
physical-slot case, with physical/logical identity checks and input policy tested.
The runtime remains generic. Independent loop hosts and their bounded cross-instance
materialization protocol are Stage7 work, not presumed complete by native staging.

## Latest executable verification

`nested-lifecycle-stage6{,-provenance}.json` records153 loop tests in each native
debug/release and Node/Chromium run,2141 workspace passes/four existing skips,19
scoped gates,8 executed positive borrowing prototypes,23 expected compiler failures,
and30 exact-filter native mirrors. Source hashes distinguish the selected123 Rust
inputs from concurrent documentation examples. The planner's sole production change
in this slice skips a redundant pending-table lookup when no program is selected;
there is no asserted measured speedup or changed publication rule.

Generic runtime source remains matched to the graph-activation prerequisite evidence,
including its188 native/184 Wasm cases and actual generic hosts/canaries. Workspace
regressions are fresh. That generic host is not a substitute for the pending loop host.
Initial compile/fixture mistakes and superseded first-broad reports remain archived
under the milestone target directories. No failed intermediate source-tree reproduction
is claimed unless explicitly archived.

## Consolidated Miri gate

The readiness snapshot was committed as `faec78df` and job
`20261027T025325-e065d48941` ran from02:53:25Z to04:55:23Z on2026-10-27.
Terminal collection passed with exit0 and228 executions. See
`evidence/miri-stage6.json`, `miri-stage6-collected.json`, `miri-stage6-provenance.json`
and the reproducible `miri-stage6-logs.tar.gz` raw-log bundle. Verification checked
the671-file snapshot (hashes/executable bits/symlinks), reproduced git archive hash,
copied supervisor, exact ordered filters/counts/seeds, pinned toolchain/version and
all82 report/log artifact hashes. Current123 scoped Rust inputs,10 configuration/
runner inputs and supervisor/helper hashes match the interpreted snapshot. Later
commits only changed documentation/evidence; concurrent dirty authored drafts and
untracked examples are not included in a whole-tree freshness claim.


The committed selection is **228 executions:128 runtime+100 loop**. The28 topology
and two interloop cases are individually exact-filtered with seed0; all30 names match
native discovery and every filter executes exactly one test. No scenario, guard,
iteration count or the900-second command watchdog was reduced/extended. Historical
interloop wall time468.43s and grouped suite growth motivate per-case supervision,
not a claim of an observed new timeout. This is a new explicit inventory; grouped
historical interpreter runs cannot establish its completion.

One consolidated supervised invocation completed; no second routine run was needed.
The dense128-boundary case took877.7475190730183s under the unchanged900s watchdog;
the nested lifecycle case took590.2424795530096s. Total supervisor time was
7318.292420581012s. These interpreter timings do not establish native callback
deadlines, CI margin or speedup. All workloads/guards remain intact. The interrupted
171/178 run and earlier partial checkpoints remain historical partial evidence.
Helpers, snapshot smoke and native mirrors are still not interpreter passes.

The independent provenance-check driver initially compared a digest to a manifest
entry object rather than its sha256 field; its failed attempt is retained separately.
The corrected hash/executable/symlink verification passed. This was not an interpreter
failure or changed snapshot, and no test was rerun or weakened to correct it.

Every Stage6 row above now has implementation/native/Wasm evidence and the required
collected interpreter gate. The checkpoint required recording completion/commit and pausing before Stage7.
That historical instruction is not the current documentation delivery workflow.
This audit does not waive later implementation gates or authorize merging a PR.
