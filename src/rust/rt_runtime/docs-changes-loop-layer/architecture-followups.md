# Architecture/proposal reconciliation through Stage6 implementation

Reviewed DESIGN_RT_ENGINE.md, PROPOSAL_RT_ENGINE.md, PROPOSAL_LOOP_LAYER.md and the
complete governing INTER_LOOP_PROCESSING.md. Baselines are in repository-baseline.json.
Do not rewrite these destinations during implementation handoff. Preserve concurrent
DESIGN_RT_ENGINE edits, historical proposal rationale and DONE_INITIAL_CRATE.md.
These notes update implemented status and API attribution, not scope or acceptance.

## Governing decision: justified no-semantic-change finding

INTER_LOOP_PROCESSING.md remains the governing decision: one render-owned resolver,
separate signal/timing relations, exact finite progression with structural termination,
no interaction quantum/per-hop delay/subdivision quota, and prepared storage without
hard deadline guarantees. The implementation matches that direction; no change to
its decision is needed or authorized here. Its original open-API/semantic questions
and initial-runtime implications are historical context. Other documents should
link the now-resolved APIs/SEMANTICS rather than treating those questions as current
ambiguity or editing the decision to weaken difficult verification obligations.

## DESIGN_RT_ENGINE.md

The current dirty draft already adds initial graph preparation, required I/O and
Stage6 graph permits. Preserve those additions; do not duplicate them. Its earlier
initial-design paragraphs and later evolution section still mix proposed and current
claims. Add an explicit current implementation map while keeping the initial delivery
record dated and intact:

| Original design phrase | Current fact / teaching link |
| --- | --- |
| Generic planner API remains open | Planner/NoPlanner, PlannerSpec/ConsumerAccess, DecisionBinding, PlanContext/FinishContext and GraphRuntime<P,L> are implemented. Use chapters3/13, not repeated process calls. |
| Event/subrange reservation and min(...,subranges-1) | Historical initial contract only. Current normal event/result capacity is min(scheduled_events,budgets.events); internal intervals ignore subranges. |
| Diagnostic worst-case bounds/API design work | Memory representability is checked; work counters saturate with overflow. No aggregate musical-work admission cutoff. |
| Sequential resources/positioning still needed | Append-maintained balanced page indexes and fixed MIDI state checkpoints provide scoped positioning with bounded local replay. |
| Group activation/freeze/loop adaptation pending | Generic claims/prepared-first-interval/final leases and coherent facade record→play/FX/export are implemented, with separate attribution. |
| Fixed program receiver remains future | LTP1/Stager has fixed reusable metadata, transactional chunks, incremental validation and explicit owning recycle. It is loop timing IR, not the generic host's RTR1 extension. |
| All graph commits at callback boundaries | Legacy Commit remains ingress-boundary; staged graph permits publish at operation/planning boundaries after all fallible domain preflight. |
| General loop role/media transactions incomplete | Supported stopped-owner configuration over prepared templates and paired selected running-program/graph activation exist. Arbitrary physical processor/shape construction is not added. |

The actual primary type is GraphRuntime; conceptual names such as GraphBuilder,
ExecutionPlan or LoopTimelineResolver in architecture diagrams are not necessarily
exported APIs. The loop implementation uses Resolver and LoopPlanner. Label sketches
or use public symbols instead of leaving users to search for nonexistent constructors.

Explain the five distinct boundaries together: off-render preparation; callback-wide
admission/background work; exact operation/planning publication; interval execution/
finish; stopped owner reclamation. Legacy processor headroom does not throttle required
RenderIo. A render fault after publication is not rollback, and loop quiescence is not
a resumable generic fault policy. Held normal validation now yields eligible recovery
at callback snapshots while preserving ownership/FIFO/progress; it does not wait for
all validation to finish first or permit endless producer retries.

Keep all native/browser historical host and CI claims tied to the generic fixture's
source. Its codec has no loop operations/planner bindings. The independent actual
loop AudioWorklet, Stage8 audit/benchmark/concurrency evidence and final PR/CI remain
outstanding. The current graph-permit section's Miri status must point to the operational
ledger at publication time, not infer success from registration or partial progress.

## PROPOSAL_RT_ENGINE.md

Preserve the original extraction rationale, broad optional migration roadmap and
unselected alternatives as proposal history. Update the opening next-layer status:
generic planning/dependencies/typed retained results, separate required work, indexed
resources, capture groups and scoped graph activation are implemented. This was a
fresh independent implementation, not a wholesale shoop_engine move.

Make these original aspirations explicitly non-shipped rather than silently adopting
them as current guarantees:

- The native closure queue is not the new portable runtime command API or an accepted
  callback allocation exception. Core operations use typed prepared data.
- A native publisher thread sharing Arc chunks was an alternative; current native and
  non-shared-memory hosts use the same single-owner store and bounded-copy semantics.
- JACK/CPAL, a shared ShoopEngineService, Session/backend/protocol migration and production
  worklet/client integration remain excluded/later work. The initial completion criteria
  about that service do not define completion of the present loop-layer plan.
- Bulk wire framing and browser glue belong to independent host fixtures/adapters, not
  production dependencies of the generic runtime. A test-only host is not a general
  supported device/plugin package.
- Overload policies such as drop-oldest/coalesce apply only where explicitly allowed
  (e.g. optional telemetry), never to accepted reliable outcomes, owning returns or
  required musical transitions. Lossless unbounded MIDI/recording is not promised.

Constant-time graph owner publication does not make resource preparation, scoped
preflight or whole callbacks constant-time. No-stop sample continuity is not automatic
crossfading or an unconditional hard-real-time browser guarantee.

## PROPOSAL_LOOP_LAYER.md

Preserve its recommendation, authority split, exclusions and implementation order.
Replace opening/build-next/planning-API-open/only-sequential-resource claims with a
clearly scoped implemented-status section linked to current APIs and evidence. Do not
rewrite the proposal as if the future independent host/performance work already passed.

The original same-frame questions are now resolved in SEMANTICS and the resolver:
clock identity is loop+absolute frame; one edge is coalesced per loop/frame; direct
sequence then explicit/inherited/natural precedence; pre-clock Stop suppresses versus
reactive Stop preserving an existing edge; only pure-sync SCC cycles are permitted,
while action feedback/recursive programs are rejected. Arbitrary callbacks cannot
manufacture unbounded same-frame occurrences. No new scheduling quantum or musical
quota is introduced to make this explanation shorter.

Map implemented facilities: standalone resolver/compiler/sparse composite views;
generic planner/bindings/receipts; page/MIDI indexes and capture groups; audio and MIDI
consumers/ledger/facade; fixed topology, single-program and prepared configuration/
paired graph activation. State the exact constrained scopes, checked generations,
real capture preflight, coherent settled cuts and explicit old-owner retention.
The current facade does not publish an arbitrary whole-domain state mirror or provide
all old backend modes. Take revision, generic resource revision and graph/topology
epochs are different identities.

Keep interval execution incremental. Longer span batches/narrower traversal remain
optional unimplemented optimizations unless separately proven: no source-reviewed
no-selection lookup shortcut establishes a measured speedup or permits assumptions
about future recording outcomes. Parameter-history MIDI restoration limits, finite
recording/export ownership and valid-work xruns remain explicit.

## Publication/check obligations

- Reconcile root proposal/design links with the nineteen-chapter runtime book and
  loop user guides only after their actual publication. Do not recreate renamed
  PLAN/PROPOSAL/DESIGN files or edit DONE_INITIAL_CRATE's completed evidence.
- Run the full intended doc/example inventory on the published final tree. Drafting
  --allow-pending is not full acceptance; historical CI or rustdoc compilation alone
  cannot establish accurate prose.
- Keep current stages/LL-A mapping in operational ledgers. These source-backed notes
  close the Stage0–6 architecture review slice, not Stage9 as a whole: Stage7–8 changes,
  complete current callback audit/performance, final source-matched gates, publication
  and final-head PR/CI/review remain required.
