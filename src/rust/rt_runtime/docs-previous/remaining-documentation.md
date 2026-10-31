# Documentation completion reference

[Historical execution and completion record](../../../../docs/history/pre-consolidation-fffc1faa/DOCS_REWRITE_PLAN.md) · [Archived implementation review](implementation-review/README.md)

`DOCS_REWRITE_PLAN.md` records completion of the book, reference and related documentation. This page preserves the substantive requirements that were unfinished when the former implementation handoff was consolidated, now archived under `implementation-review/`. Its requirement wording is supporting history, not an active task ledger or a request for more handoffs.

The user has closed incoming implementation/handoff work for this documentation pass. Write against the code and evidence already in the checkout. Do not wait for new notes, invent APIs or turn a missing host/benchmark into a code task. Existing historical reports retain their original inputs, dates and scope.

## Runtime book and reference

Chapters 1–4 are approved; chapter 7 is the approved style reference. The correctness merge changed only chapters 1, 3 and 4. At consolidation, fourteen chapters needed authoring: 5–6 and 8–19. The execution plan records their completed authoring, individual teaching requirements, focused runnable examples and six reference pages.

Important distinctions preserved in the completed documentation:

- Startup graph installation, ordinary callback-ingress Commit and staged Ready/permit activation are different paths. Ready is not active; pending domain references do not lock candidates. Retired graph owners need explicit release.
- Recovery can park held normal work at a callback snapshot. Resumption preserves FIFO, validation progress and the original cancellation observation; operation/publication allowances and possible normal starvation remain explicit.
- Owning command returns, scheduled execution results and domain outcomes have separate lifetimes. Requested/effective/delivered/terminal frames are not interchangeable. Recovery must terminate at its delivery frame before positive interval progress.
- Required media/index/group work is independent of optional/background allowances. Storage, callback work allowances and diagnostics are different limits. Actual retained vector capacity matters; diagnostic counters saturate while semantic identities cannot wrap.
- Resource revisions, musical take revisions and graph/topology epochs are separate identities. Raw resource/snapshot IDs require same-store use; scoped views additionally validate lifetime. Copying a handle acquires no lease.
- Byte positioning is not MIDI event/state positioning. State is strictly before the target; equal-time original packets remain for playback. Checkpoint capacity is `C × K − 1`; state lookup is not restoration or an external synth snapshot.
- Group preparation/first-interval reservation precedes dependent rendering. Members have separate promises, common-prefix publication and reserved final leases. Consumers stage data; the planner owns lifecycle authority.
- A fault after publication is not rollback. Disconnect is not Stop. Stopped extraction must preserve any cleanup ledger; never fake `finish`, replay samples or promise device cleanup after the device stops.

Keep examples short. The larger integration models, saturation cases and partition matrices in the archived review are regression references, not tutorial infrastructure to reproduce. Preserve source excerpt markers and compare snippets manually. Execute new/changed examples in debug/release; prose-only changes do not require rerunning unchanged programs.

## Loop-library guides and examples

Destination root: `src/rust/shoop_loop_engine/`. These are now part of the documentation plan, not deferred to a future implementation worker.

- `README.md`: describe the implemented facade, fixed-role routes, stopped-owner configuration and paired running-program/graph publication. Keep prepared physical-template restrictions and missing independent-host evidence explicit. Link the actual task guides.
- `docs/audio.md`: retain DSP/range/smoothing/refusal contracts; replace stale adapter/FX/export/example-pending claims with the implemented facade. Low-level rendering still has no timing/lease authority.
- `docs/midi-restore.md` and `docs/midi.md`: preserve cleanup → strict-pre-target restoration → original-event order, projection/parameter-history limits and first-sink-failure behavior. Attribute indexed playback/capture, input tracking and runtime recovery correctly. Preserve the historical stack-overflow/fixed-box investigation without inventing an exact stack frame or new host proof.
- `docs/media.md`: teach resident/playback/latest/pin/export ownership and `OwnerChange`/`OwnerActivation` whole-batch preflight. An unused permit changes nothing; commit transfers prepared inputs without ordinary failure. Old-generation exports remain immutable; conservative metadata requirements use currently free slots, not anticipated retirement credits. The ledger does not certify a settled musical cut.
- `docs/topology.md` and `docs/topology-staging.md`: preserve sparse flat representation, borrowed views, canonical LTP1 encoding, bounded ingestion/validation and explicit owner recycling. Timing Ready is not signal-graph/media compatibility or a cross-instance worklet protocol.
- `docs/program-boundaries.md` and `docs/program-changes.md`: distinguish informational unpublished probes from owning activation. Explain real zero entry versus an idle cursor at zero, family refusal, independent candidate ownership and permanent quiescence. Keep the standalone resolver's single-program/fixed-identity scope separate from facade graph/configuration operations.
- `docs/topology-activation.md`: teach each supported path separately: fixed-graph timing activation; fixed-graph running-program activation; fixed-role `GraphActivate`; immediate paired `TopologyGraphActivate`; stopped-owner `ConfigurationActivate`; pending `ProgramGraphActivate`. Explain their different stopped/zero conditions and independent graph/topology/role retirements. A same-frame queued Stop is not already-stopped state for an immediate delivery-time check.
- `docs/runtime.md`: complete preparation, operation/results, nested controls, coherent queries/export and teardown. Explain `prepare_with_configuration_staging` plus separate graph staging, prepared spare templates, source snapshot/header validation, role banks, explicit input preserve/reset and real capture preflight. Controller submission uses prepared flags, unlike allocating generic Control::submit; payload preparation/destruction remains separate.

The runtime guide must also cover:

- Exact recovery-safe set: Cancel, TopologyCancel, RoleCancel, RoleSourceRelease and Timing::At Stop; payload-free, nonfuture, finite admission/result storage, terminal at delivery frame. Activation/ordinary source validation and normal-lane ReleaseGraph are not emergency bypasses.
- `Timing::At` with a separately supplied frame; source tick/countdown behavior; Play/Seek/retrigger, Record/monitor_previous, Stop and composite PlayIteration/SeekIteration. An idle valid source does not time out or invent pulses. Facade selection derives duration/revision from pinned media.
- A short nested delay/play/record/seek/stop/restart walkthrough with direct-command precedence, truthful revisions and old exports. Use independently expected outputs, not resolver output as its own oracle. Preserve both existing sufficient-capacity and exhaustion scenarios as separately scoped evidence.
- State/Export at one settled pre-render cut. Actual Cut fields are not a public AfterInterval enum; standalone after-finish observations are different. Recording exports either acquire every prefix or return no partial export. Read buffers/cursors return through owning commands; retain ExportId until reads finish. Obtain all MIDI channels and reset marker from that same immutable export.
- Scoped staging/media IDs versus caller-routed LoopId/standalone RequestId; runtime Sequence is another identity. No Rust pointer/Vec/trait-object transport between independent Wasm instances.
- Graceful musical/device cleanup while callbacks run versus stopped abandonment/fault cleanup. `Engine::into_stopped`/`Stopped::cleanup` permanently quiesce and recover actual prefixes/owners; imported resources remain caller-owned. No rollback or implied resumability.

`examples/coordinated_loop.rs` is a real offline lifecycle example with assertions, but no allocator guard of its own and no nested/configuration walkthrough. Use it as the starting example, then add only small runnable demonstrations needed by the updated task guides. Do not relabel integration-harness helpers as public APIs. Run any new or changed example in native debug/release; leave unchanged examples alone for prose-only edits.

Detailed source material: [foundations](implementation-review/loop-foundations-backfill.md), [facade/state/export](implementation-review/loop-runtime-backfill.md), [live routes](implementation-review/loop-facade-live-graphs.md), [media-owner permits](implementation-review/loop-media-owner-activation.md), [configuration](implementation-review/loop-configuration-activation.md), [paired program/graph](implementation-review/loop-program-graph-activation.md) and [nested lifecycle](implementation-review/loop-nested-lifecycle.md). Their old future-stage, hash/checker and publication-worker instructions are retired.

## Crate entry points and public API prose

Update both crate READMEs and book/reference navigation as content is completed. Replace stale blanket “loop integration/general graph transactions pending” claims with the implemented constrained scope; do not point to empty chapters as complete guides.

In `rt_runtime` public rustdoc, remove the obsolete split-credit/all-resource-quota crate summary. Review planner/processor contexts and borrowed authority, graph staging/activation/storage bytes, command owning returns, prepared flags, resource/index/capture lifetimes, terminal receipt timestamps and transport begin_cycle/next_operation/complete semantics. The loop crate's API prose needs consistent preparation, IDs/ownership, query/export cuts and cleanup descriptions. These are documentation/comment edits, not permission to change behavior or signatures.

## Repository runtime references

Destinations: `docs/rt_runtime/`.

- `CONTRACTS.md`: clearly separate the initial split-credit/quota contract from current planner, recovery, required-resource and staged-publication behavior. Preserve historical A1–A8 acceptance and attribute loop LL-A requirements separately.
- `RESOURCES.md`: current scoped access, checkpoint costs, PreparedReserved/first-interval authority, final leases, optional prefixes, three-copy group path and planner-ledger extraction. Preserve historical ownership proof scope.
- `GUIDE.md`: link the completed book/current examples and explain startup/live/staged paths, legacy headroom versus required reads and actual loop integration. Distinguish historical support matrices from current platform evidence.
- `AUDIT.md`: preserve the initial audit as history. Correct current-facing bounds/claims for positive intervals, planner views, parked normal owners, graph permits, required media/index access and groups; link separate loop resolver/consumer reasoning. Any new source-review summary must say what was actually inspected, not claim a fresh measurement/model/guard campaign.
- `HOST_FIXTURE.md` and `tests/rt_runtime_host/README.md`: describe the existing generic NoPlanner RTR1/RTJ1 fixture and actual browser/native commands. Correct the `preserved.s` typo; keep historical memory measurements/journals/canaries scoped. It is not the absent independent loop host.
- `PERFORMANCE.md`: preserve original measurements, raw provenance, outliers, overhead and limitations. Label later smoke runs as smokes and legacy quota-based capture as that path. Report only existing loop measurements; no synthetic replacement benchmark report or inferred speedup/deadline.
- `VALIDATION.md`: retain chronological commands, failures, interpreter selections, historical CI/PR results and evidence bytes. Add current status/navigation without rewriting old totals into new results or generating replacement evidence.

Details: [repository review](implementation-review/repository-runtime-followups.md). Its former requirements for new implementation gates, remote delivery or future host work do not block documentation completion.

## Loop contracts and root architecture

Under `docs/loop_layer/`, update `SEMANTICS.md`, `RUNTIME_INTEGRATION.md`, `FACADE_DESIGN.md` and `TOPOLOGY_ACTIVATION_DESIGN.md` to distinguish implemented prepared-template configuration/program changes from old design-only status. Correct recovery ordering/termination, conceptual versus public cut fields, command identity layers, explicit off-render import padding and post-publication cleanup. Review `MIDI_INDEX_DESIGN.md` and `CAPTURE_TRANSACTIONS_DESIGN.md` for the same current-versus-historical distinction. Keep governing musical scope/precedence and exclusions unchanged.

Review current-facing headings/pointers in `VALIDATION.md`, `STAGE6_READINESS.md`, `MIRI_JOBS.md`, `MIRI_BACKGROUND.md` and `TEST_EXECUTION_TIMES.md`. Preserve actual reports, job history, runner behavior and timings; do not start jobs, invent a new audit/performance campaign or turn old checkpoints into fresh verification.

For `DESIGN_RT_ENGINE.md`, `PROPOSAL_RT_ENGINE.md` and `PROPOSAL_LOOP_LAYER.md`, add a clear implemented-versus-proposed map while retaining the original rationale and dated delivery history. Replace obsolete open planner/index/group/staging questions with actual API links. Identify conceptual diagram names versus exported types. Keep device adapters, production backend/UI migration, unrestricted live construction and hard deadline claims outside shipped scope. Preserve `INTER_LOOP_PROCESSING.md`'s governing decision and `DONE_INITIAL_CRATE.md`'s historical evidence.

Details: [architecture review](implementation-review/architecture-followups.md). No implementation-plan stage, acceptance threshold or unrelated goal is changed by these documentation updates.

## Closed implementation boundary

At consolidation time, `tests/` contains the generic `rt_runtime_host` and Wasm suites, not `tests/loop_engine_host`; the loop examples contain `coordinated_loop`, not the proposed loop benchmark. Document these as not provided in this snapshot. Existing Node/Chromium package tests, offline examples and generic actual-worklet regressions are not evidence of an independent loop AudioWorklet.

Do not create a fictional host README/codec/journal recipe, promise an incoming implementation, or leave this plan blocked on a future handoff. Document existing host constraints and evidence honestly. Finish source-based documentation of the shipped implementation; unavailable host/performance/final-platform evidence remains a stated limitation, not an unowned documentation task or an instruction to generate it.
