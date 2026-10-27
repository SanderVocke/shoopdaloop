# Stage 6 implementation coverage and consolidated-verification readiness

This is an operational acceptance audit, not an authored guide or a reduction of
`PLAN_LOOP_LAYER.md`. **Stage 6 is not yet verified complete.** Its native/Wasm
implementation coverage is present; the consolidated source-matched Miri run remains
the outstanding stage-level verification gate. Stages7–10 and LL-A final acceptance
remain open. Broader randomized/performance/concurrency/failure-canary work required
by Stage8 is not waived by this audit.

## Checklist-to-evidence mapping

| Stage 6 requirement | Implemented path and observed evidence |
| --- | --- |
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

The committed selection is **228 executions:128 runtime+100 loop**. The28 topology
and two interloop cases are individually exact-filtered with seed0; all30 names match
native discovery and every filter executes exactly one test. No scenario, guard,
iteration count or the900-second command watchdog was reduced/extended. Historical
interloop wall time468.43s and grouped suite growth motivate per-case supervision,
not a claim of an observed new timeout. This is a new explicit inventory; grouped
historical interpreter runs cannot establish its completion.

Launch one durable background job from the immutable committed readiness snapshot,
using `scripts/run_rt_runtime_miri_jobs.py start --stage stage-6 --commit <commit>`.
Continue documentation review while it runs. Check at natural work checkpoints,
verify terminal collection plus source/configuration/toolchain/inventory hashes, and
retain any failed/partial reports. The interrupted171/178 run remains partial.
Helpers, snapshot smoke, native mirrors and registered selections are not interpreter
passes. If a case fails or times out, record the concrete failure and use only the
additional focused interpreter work justified by that failure/freshness concern.

Do not complete the Stage6 goal task or check off final stage verification until this
gate passes. Afterward record the collected report/commit, reconcile every checklist
row and advance to the independent-host stage. This audit does not authorize skipping
any later plan gate or merging a PR.
