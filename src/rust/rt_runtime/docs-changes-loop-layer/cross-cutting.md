# Cross-cutting and other-document follow-ups

These are instructions for the later documentation pass. Do not edit the named
sources/destinations as part of preparing this handoff. The initial generic
graph/recovery notes are retained below with later integration/backfill updates;
remaining earlier-stage review and publication are tracked in the index.

## README, navigation, reference and rustdoc

- `src/rust/rt_runtime/README.md`: distinguish callback-boundary legacy commits from
  planner-coordinated activation; list validated owner staging and explicit owning
  release. Its current transitional links to `docs-previous/` are intentional while
  the book is incomplete. Once ready, point to chapters 5/6/13–19 and the book index;
  do not present empty chapter files as finished guides.
- `src/rust/rt_runtime/docs/README.md`: preserve the user's ordered nineteen-chapter
  layout. Add no competing flat `graph-activation.md` or `planning.md`. Cross-links
  between chapter 15 and ownership, MIDI, planning, capacity and shutdown should
  avoid repeating full contracts in each chapter.
- The documentation worker's planned `docs/reference/` should include StageId scope/
  generation semantics, Ready versus Taken, staging/activation/refusal conditions,
  actual-capacity accounting, and the distinction between normal transport FIFO and
  recovery priority. State identity counters are checked, not saturating diagnostics.
- Public rustdoc: review `GraphRuntime::prepare_graph_staging`, Command staging/
  release, `PreparedPlan::storage_bytes`, GraphAccess/GraphActivation, planner context
  fields and `RenderEndpoint::begin_cycle/next_operation`. Explain off-render
  preparation, the no-fallible-domain-work-after-publication rule, borrowed authority,
  normal-lane owner release, and fixed-slot parking without re-observing cancellation.
  Existing source comments are not a substitute for a complete public walkthrough.
  A concrete current rustdoc correction is needed in `src/rust/rt_runtime/src/lib.rs`:
  the crate introduction still says scheduling uses “split” credits and describes
  all resource work as quota-based. Remove the obsolete split-credit claim
  (`Budgets::subranges` is ignored), and distinguish required RenderIo accounting
  from optional/background allowances. The operation review read the current
  source and passed rustdoc, but a clean build does not make that prose accurate.
- Recompute links from nested chapter directories. Keep snippets traceable to real
  executable code, distinguish example driving helpers from APIs, and run the
  documentation worker's actual example/check inventory rather than assuming
  `cargo test --examples` executes every example's `main`.

## Repository runtime contracts and architecture

Destinations: `docs/rt_runtime/{CONTRACTS,GUIDE,RESOURCES,AUDIT,HOST_FIXTURE,PERFORMANCE,VALIDATION}.md`
and `DESIGN_RT_ENGINE.md` / `PROPOSAL_RT_ENGINE.md`.

- Replace blanket graph-activation-only-at-callback statements in current contracts
  with the legacy/coordinated distinction; do not rewrite old measurements as if
  they tested this extension. Graph permits publish metadata, not arbitrary nodes,
  resource transactions or application state rollback.
- Add the explicit preflight rationale: validate current/candidate epochs and reserve
  retirement before domain publication; dropping an unused permit changes nothing;
  final ownership swap is infallible and lifetime-protected.
- Record pending MIDI compatibility: already-delivered input prevents planning-time
  removal; operation-time removal instead causes a later injection to fail. This
  addresses stranded/replayed input, not MIDI note cleanup for retired musical roles.
- Record held-normal preemption with fixed owner parking, FIFO resumption, original
  cancellation observation, callback-budget limits and retained validator progress.
  The failure was reproduced before correction; do not describe an unchanged test
  rerun as the causal fix.
- Capacity references should count actual vector capacities and conservative bank
  aggregates, plus explain separate command/temporary/validator costs. Benchmark
  observations are not deadlines. Normal-lane fairness is not guaranteed when
  recovery consumes every allowed operation/publication indefinitely.
- Host evidence remains generic native/actual-worklet regression evidence. It does
  not prove the future independent loop host or general loop role/media transaction.
  Preserve old golden journals and explicitly label verification scope/freshness.

## Loop-specific documentation

Destinations include `PROPOSAL_LOOP_LAYER.md`,
`docs/loop_layer/{RUNTIME_INTEGRATION,FACADE_DESIGN,TOPOLOGY_ACTIVATION_DESIGN,SEMANTICS}.md`,
`src/rust/shoop_loop_engine/README.md` and its runtime/topology/program guides.

- Explain that generic graph banks and permits now provide a prerequisite for
  coordinated loop transactions. They do not themselves implement logical/physical
  role-generation switching, retained old-generation media exports, capture claims,
  or old-role MIDI cleanup. The later configuration/program-graph notes now cover
  tested loop integration; do not preserve the initial blanket pending claim.
- Keep the restricted loop facade's implemented surface distinct from generic
  GraphRuntime capabilities. The later [live-graph slice](loop-facade-live-graphs.md)
  adds explicit control compilation, staging/release and checked graph or paired
  graph/timing activation for fixed roles. This supersedes the initial fixed-graph
  limitation. The subsequent [configuration slice](loop-configuration-activation.md)
  integrates logical generation/media/capture-claim changes over prepared physical
  templates; shapes/node IDs remain fixed. Old-generation exports remain separate.
- The loop topology receiver and generic graph bank have different StageId types,
  storage lifecycles and validation meanings. LTP1 readiness proves timing IR, not
  graph/media/role compatibility. Do not claim that generic graph staging is a
  wire materializer or that native pointers can cross Wasm instances.
- Preserve fixed-graph immediate topology activation versus owning running-program
  activation distinctions, actual capture preflight, and permanent quiescence on
  consumed-engine cleanup. Configuration publication and the later
  [paired running-program/graph path](loop-program-graph-activation.md) now hold
  prepared permits through actual domain/capture preflight; arbitrary callback
  processor construction is not added. Neither a permit nor quiescence supplies
  retroactive rollback. Use current verification ledgers for the still-open full
  Stage6 Miri/independent-host/final-delivery status, not historical slice counts.

The full set of Stage 0–5 loop-guide changes still needs separate source-backed
notes. Existing loop guides and implementation evidence are review inputs, not
permission to silently reuse old claims as freshly verified results.

## Operational evidence and handoff boundary

Implementation-owned plan/evidence/verification ledgers may continue recording real
commands, failures, counts and debt. The chapter worker should link authoritative
reports rather than copy transient milestone totals into teaching prose. No new
Miri interpreter invocation was made for this prerequisite slice; accumulated
cases await the consolidated numbered-stage run. Handoff completeness, publication
by the separate worker and final documentation acceptance are distinct milestones.
