# Plan: consolidate quantum processing

Status: implementation and local acceptance complete; final delivery in progress. Baseline: `52f24c02` on
`feat/fixed-quantum-runtime`, targeting `feat/backend-replacement` through existing
draft PR #930. Continue that branch and PR rather than opening a parallel migration.

This plan follows the removal of experimental runtimes, transitional tests,
temporary plans and benchmark tooling. It covers the four remaining substantial
simplifications identified in the cleanup review. Repository instructions:
[write_plan.md](.agents/prompts/write_plan.md). Current architecture:
[ARCHITECTURE.md](ARCHITECTURE.md).

## Goals and scope

1. Give every processor and planner one quantum-based execution interface, removing
   the old decision-slot/observation compatibility machinery throughout its callers.
2. Make the quantum mandatory in the loop resolver and remove variable-boundary
   execution and its optional-mode branches.
3. Specialize coordinated recording preparation, staging and completion for one
   whole quantum, removing arbitrary-interval bookkeeping while retaining atomicity.
4. Remove `shoop_engine_service` after moving its useful regression coverage to
   existing runtime, loop, DSP and production-engine test locations.

Scope includes production crates, generic examples, test support, independent
native/Wasm/worklet fixtures, graph-transfer fixture formats, documentation,
inventories and package references needed to complete those removals. Do not leave
an old implementation or a renamed copy of the demonstrator in a test crate.

Broader feature restoration in the [parity ledger](docs/native_session_recovery/feature-parity.md),
retiring legacy reference crates, new plugin hosting, changing the quantum policy,
and recreating benchmark tooling are outside this work.

## Immutable acceptance criteria

- Exactly one processor entry point and one planner begin/finish lifecycle remain.
  `uses_direct_domain`, the alternate processing block, generic decision-slot
  lookup and generic observation transport are absent from executable code.
- `PlanningDescriptor`, `PlanningStep`, `DecisionBinding` and decision/observation
  capacities disappear. Their removal preserves graph membership, loop ownership,
  role, generation and admission checks; invalid graph/processor changes still refuse.
- Every resolver is prepared with a validated, immutable `Quantum`. No constructor
  selects an adaptive mode, and execution never searches for a shorter graph interval.
  Preserve supported quanta `1, 16, 32, 64, 128` and production default `32`.
- Each graph iteration and coordinated capture reservation covers exactly one
  quantum. Arbitrary host callback partitioning retains current audio/MIDI output,
  buffering latency, zero-frame behavior and callback-wide work allowances.
- Preserve aligned loop transitions, sync/composite/program settlement and precedence;
  requested versus effective operation frames; cancellation and recovery; live
  graph/configuration changes; recording/export coherence; and retiming transactions.
- Capacity failures cannot publish a partial audio/MIDI capture or partial family
  change, consume unrelated owners, interrupt unrelated audio, or duplicate receipts.
  Existing stale-revision, saturation, controller-loss and stopped-cleanup behavior
  remains covered.
- The `shoop_engine_service` package and executable composition are removed. Every
  useful invariant from its tests has an identified retained or migrated test;
  duplicative and wrapper-specific assertions may be retired with a recorded reason.
- Retained RT code remains bounded, allocation/free/lock/I/O-free during callbacks,
  safe Rust, and portable under the existing dependency boundaries. Preserve generic
  independent DSP and signal-only use without a Shoop dependency in `rt_runtime`.
- Native, Wasm/Node and actual browser/worklet acceptance passes at the final stage.
  Do not claim a pass from compilation, rebaseline unexplained media differences,
  or delete failing behavioral tests to make acceptance green.

## Design rules

- Use one `ProcessBlock` carrying frame, quantum, callback origin, borrowed buffers,
  resource access and the render-owned domain. Merge `TickProcessBlock` into it;
  remove the processor mode flag and executor dispatch. `NoPlanner` remains the
  inexpensive signal-only domain. DSP processors may ignore domain access.
- Retain planner operations, receipts, graph admission and resource access. Keep
  only begin-tick, finish-tick and one operation hook; remove adapters to `plan`,
  `finish` and duplicate operation hooks. Derive tick end from frame plus quantum.
  Domain-owned loop views and semantic failure inputs remain valid; removing the
  generic observation writer does not mean removing loop failure semantics.
- Separate graph safety from decision delivery. Validate actual graph members and
  prepared loop-node owner/role identities against the candidate loop-node registry.
  Retain only bounded admission metadata needed for those checks, outside the render
  traversal. Generic processor changes must not replace protected loop consumers;
  configuration/program/node-pair publication validates candidate ownership before
  committing. Do not recreate decision tables under a different name.
- Preserve operation payload bounds when removing `ConsumerAccess`/`PlannerSpec`
  fields. Keep needed per-processor/planner payload limits in prepared signatures;
  remove obsolete planner IDs only after their ownership/authority uses are replaced.
- Prepare the resolver with its quantum, validate topology alignment immediately,
  settle the tick start, preflight, commit, render and finish exactly one tick.
  Keep transactional scratch needed for failed capture/graph preparation. Remove
  future-boundary search and duplicate state only when its rollback role is disproven.
- Bind capture preparation to the runtime/store quantum. Reserve a complete tick
  before rendering; activate a prepared first take only after admission; stage all
  members; publish lengths, indexes and revisions together after successful staging.
  Keep take generations, reservation sequence checks, immutable leases, cancellation
  and resource retirement. Media byte transfers and MIDI timestamps retain their
  existing precision; they are not forced to quantum-sized chunks.
- Preserve behavior through tests, not API compatibility aliases. Migrate useful
  examples and fixtures to the sole interface and remove old constructors/methods
  after their callers move. Use explicit `Q=1` for exact-frame reference scenarios;
  also retain substantive coverage at larger quanta, especially `16/32/64`.
- Keep the resource pools, graph compiler/publication, transport credits and
  application/engine/host boundaries. This work changes internal execution APIs,
  not user features or carrier semantics. Avoid production wire/schema changes;
  update and version independent fixture encodings when their graph representation
  changes. Historical evidence remains historical.
- Do not add benchmarks. Verify bounded work and callback budgets with deterministic
  tests/counters, including dense simultaneous transitions and failure/retry paths.
  Report timing guarantees only where actual evidence exists.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Complete implementation and local verification before human involvement. Respect
  the current pause on pushes and remote CI work; publication, CI investigation and
  review belong only to the final stage after the user resumes that work.

## Staged implementation

Order: stage 1 establishes the behavioral floor; stages 2–4 remove the compatibility
interfaces, adaptive resolver and arbitrary capture intervals in that order. Stage 5
then moves demonstrator coverage onto the resulting APIs and deletes the crate.
Stage 6 consolidates the complete local result. Stage 7 is the only delivery/CI stage.

### Stage 0 — planning checkpoint

- [x] Read repository planning/rules/test guidance, architecture and the parity ledger.
- [x] Inspect processor/planner interfaces, graph admission and fixture bindings,
  resolver settlement, capture reservations and demonstrator tests/dependencies.
- [x] Write this plan; leave implementation for a subsequent execution request.

Verification: review stage coverage, local Markdown links and `git diff --check`.
This documentation checkpoint supplies no new runtime evidence.

### Stage 1 — record callers and the regression floor

- [x] Confirm the baseline/branch and preserve unrelated work. Reuse the current
  branch and PR; keep all commits local until stage 7 is authorized.
- [x] Inventory callers of both processor/planner interfaces, graph planning metadata,
  adaptive resolver constructors/preview and variable-length capture reservations.
  Include examples, `shoop_loops_test_support`, `shoop_example_dsp`, production engine
  builders/configuration paths and `tests/{rt_runtime_host,loops_host}`.
- [x] Record a compact test-retirement/migration map in this plan. Map demonstrator
  DSP operation ordering, partition oracles, startup imports, immutable prefix reads,
  stalls/disconnects/faults, foreign/stale owners and latency probes to exact tests.
- [x] Preserve independent expected samples, MIDI, outcomes and ownership totals.
  Distinguish fixture metadata/layout changes from observable behavior changes.
  Add missing safety tests only where the planned removals expose a real gap.

Verification: establish a local targeted native and Wasm/Node baseline for affected
packages. Confirm negative graph admission and capture-failure coverage before
changing representations. Record existing failures separately; defer CI work.

### Stage 2 — one processor/planner interface and graph admission

- [x] Merge processing blocks and processor entry points; migrate all production
  nodes, independent DSP, boxed delegation, examples and test adapters. Remove mode
  flags and the executor's legacy dispatch/decision lookup.
- [x] Collapse planner lifecycle/operation hooks and remove associated decision and
  observation types, generic observation buffers/writers and obsolete storage bounds.
  Preserve operation payload capacities and callback-origin propagation.
- [x] Replace decision-binding validation with prepared loop-node membership/role
  admission. Cover initial graphs, generic graph/processor changes, configuration
  replacement, staged node pairs and program/graph activation. Then remove planning
  descriptors, compiler/catalog tables and builder scratch that only served delivery.
- [x] Migrate native/Wasm graph-transfer fixtures and planner fault canaries to the
  same API. Remove obsolete fixture fields/records and update their versions rather
  than keeping a second graph representation. Keep error/heap origin assertions.

Verification: runtime/loop/engine/DSP tests and examples pass. Exercise missing,
wrong-role, duplicate, stale/reused and unexpected loop consumers, protected processor
replacement and valid unrelated DSP nodes. Check mixed DSP/loop operation ordering,
reliable results and one begin/finish pair per tick. Run affected fixture codec/model
tests and Wasm/Node tests. Search executable sources for the retired interfaces.

### Stage 3 — mandatory resolver quantum and one settlement path

- [x] Pass `Quantum` at resolver/planner preparation; eliminate `Option<Quantum>`,
  post-construction mode selection and implicit adaptive constructors. Validate
  topology/program/media alignment before publication, including replacement paths.
- [x] Keep one whole-tick preview/commit/finish path; remove variable-boundary
  `Resolver::preview`, boundary-minimum scans and compatibility call chains.
  Replace arbitrary finish endpoints with checked completion of the prepared tick.
- [x] Migrate resolver tests, examples and test-only planner canaries. Retain semantic
  coverage of sync cascades, composite iteration/seek, same-frame precedence,
  program changes, recording failures, retimed families and completion ordering.
- [x] Consolidate scratch/state copies and retry setup where demonstrably redundant;
  retain rollback for refused capture/graph preparation and cancellation.

Verification: explicit `Q=1` oracles retain exact frame/media results. At `16/32/64`
verify aligned starts/wraps/lengths, never-early rounding, multiple changes due at
one tick, dependency cascades, clock overflow and cancellation. Failure/retry paths
publish once, terminate within prepared bounds and do not render a partial tick.

### Stage 4 — coordinated capture in whole quanta

- [x] Bind resource/capture preparation to the same immutable quantum as the runtime;
  refuse mismatches before processing and migrate standalone resource tests explicitly.
- [x] Replace frame-count reservation APIs with whole-tick reservation derived from
  that quantum. Remove arbitrary interval sizes, redundant end/count storage and
  partial-offset staging paths once all callers use complete ticks.
- [x] Unify ordinary and prepared-first-tick reservation bookkeeping where possible,
  retaining the activation distinction and stale ownership checks. Update loop views,
  capture nodes, planner preflight/cancellation and completion together.
- [x] Simplify resolver/capture retry handoff using the one-tick contract, keeping
  all-member reservation, atomic commit, MIDI index capacity checks, immutable
  export revisions and off-render retirement.

Verification: stereo/MIDI captures, playable selections and retained prefix exports
match samples/packets across whole, irregular and zero callbacks. Exercise page/index/
staging exhaustion, failed first activation, cancellation, missing member writes,
double/stale completion and reused takes. Refusals publish no partial length/revision,
recover ownership and leave unrelated playback/recording uninterrupted. Verify
callback quotas under many ticks and simultaneous capture/transition failures.

### Stage 5 — migrate demonstrator coverage and remove the crate

- [x] Move generic mixed loop/DSP scenarios into existing loop/runtime tests using
  `shoop_loops_test_support` and `shoop_example_dsp`. Keep typed independent DSP
  operations and partition-invariant media assertions without the service wrapper.
- [x] Map service-wrapper lifecycle assertions to existing production endpoint/
  transport tests; port missing invariants there. Remove wrapper-specific ticket or
  queue assertions only after identifying the retained ownership/credit guarantee.
- [x] Preserve startup import and physical capture/latency marker tests, including
  native-only legacy comparison where still useful. Keep exact reference oracles
  at `Q=1` and aligned production scenarios at larger quanta. Retain useful educational
  examples in existing crates only when they add distinct coverage or documentation.
- [x] Remove `src/rust/shoop_engine_service`, its manifest/lockfile entries, current
  documentation links, tracing entries and explicit package references. Retain
  `shoop_example_dsp` and shared test support; do not copy the old service wholesale.
  Update configured package selections now without running remote CI.

Verification: every row of the stage-1 coverage map has a retained test or a reasoned
duplicate/wrapper-only retirement. Native and Wasm/Node replacements pass. Cargo
metadata lists no service package/dependency, active source/configuration has no
stale references, and production dependency boundaries remain unchanged.

### Stage 6 — consolidate and complete local acceptance

- [x] Remove remaining adapters, duplicate helpers, stale examples and temporary
  artifacts. Update maintained runtime/loop/DSP guides, architecture, tracing and
  documentation inventories; preserve historical reports and reference behavior.
- [x] Audit the acceptance criteria against source searches and the coverage map.
  Record final deletions/additions and distinguish removed mechanisms from retained
  safety machinery. No second execution path or hidden demonstrator may remain.
- [x] Run formatting, warning-denying workspace build, test-attribute, tracing,
  documentation, parity and four-target dependency-boundary checks. Run the full
  native suite and affected Wasm/Node tests after the final code changes.
- [x] Commit the consolidated local result and record exact commands/results here.
  Stop with a reviewable diff before any push, remote CI inspection or CI fixes.

Verification: use [.agents/info/test.md](.agents/info/test.md) and the active build
environment. In particular run `RUSTFLAGS="-D warnings" cargo build --workspace`
before the full native command, not concurrently with host-process tests. Native:
`SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci`.
Report skips/failures explicitly; a local pass does not substitute for stage 7.

### Stage 7 — final end-to-end validation, CI and review

- [x] Run actual browser/AudioWorklet acceptance locally for
  the changed generic fixtures and production application: record/play, audio/MIDI
  export, graph publication, recovery and retimed session/import-family workflows.
  Compare full media/results to native replay, including dense transitions, saturated
  results, resource exhaustion and deliberate planner/processor failures.
- [ ] Complete affected packaged native/browser and transport scenarios: typed local,
  socket/shared-memory and MessagePort paths, arbitrary callback adaptation and
  admitted work surviving controller loss. Run selected existing Miri ownership/
  resource tests appropriate to the changed core paths. Require evidence at larger
  quanta as well as exact-frame fixtures; retain heap guards and recovery assertions.
- [ ] Resolve remaining validation failures, including previously deferred journal/
  recovery issues, with root-cause evidence. Update fixture journals only for
  explained representation/revision changes; preserve samples, MIDI and semantic
  expectations. Do not replace assertions with retries or weaken them to pass.
- [ ] Push the finished branch and update existing PR #930 against
  `feat/backend-replacement`. If the PR has already been superseded, use its successor
  rather than opening a duplicate. Describe the final implementation and validation.
- [ ] Run the required full CI platform matrix, investigate/fix failures and repeat
  affected gates until green. Check automated PR reviews and address feedback until
  the reviewer approves; rerun affected validation after changes.
- [ ] Present the final outcome, evidence, residual limits and architectural analysis
  for human review. Keep any merge approval as the final delivery decision; do not
  merge without authorization. Mark this plan complete only with acceptance evidence.

Verification: link final native/browser/Miri/CI evidence and PR review outcomes here.
Human checks or unavailable facilities must be stated explicitly, never checked off
on the basis of a proposed command or a test in a different execution environment.

## Execution evidence and coverage map

Stage 1 (2026-11-04): current branch/PR retained; only generated `work/` existed
outside committed sources. Native baseline command: `cargo nextest run -p rt_runtime
-p shoop_loops -p shoop_rt_engine -p shoop_example_dsp -p shoop_engine_service
--profile ci`: **459 passed**. Wasm/Node unit baseline: `cargo test --target
wasm32-unknown-unknown --lib` for those packages plus `shoop_persistence_codec`:
**34 passed** (the service has no unit tests). Logs are local under `work/cleanup-baseline-*`.
The prior consolidated full workspace baseline was 2,681 native passes, six skips.

Caller inventory: 40 Rust files reference the compatibility API, across runtime,
loop parts, engine, independent DSP, demonstrator, test support, examples and loop
host fixtures. Generic observation consumers exist only in loop compatibility
processors and dedicated planner examples/tests. Loop graph safety currently uses
binding counts as well as required membership; both protections must survive.
Capture interval callers and resolver-only scenarios also occur outside these files.

| Demonstrator coverage | Retained/migration destination and required invariant |
| --- | --- |
| `actual_loop_and_component_control_match_full_partition_oracles_without_warmup`; `public_scenario_matches_complete_oracles_across_all_partitions`; alternate query order | Mixed loop/DSP runtime fixture, retaining independent full stereo/MIDI/media oracles, typed DSP operations and callback partitions. |
| Both `startup.rs` tests | Loop import and production engine initialization coverage: indexed MIDI/initial state, multi-page PCM, bad shape/capacity and no startup owner leaks. |
| `prepared_route_cut_keeps_capture_live_and_returns_immutable_prefix_bytes`; saturated exports/route rejection | Loop topology/configuration and production content/export tests: uninterrupted capture, retained prefixes, refused changes and exact recovery. |
| `physical_capture_markers_and_missing_windows_agree_across_partitions`; native `current_engine_retains_history_postroll_and_independent_logical_windows` | Existing loop/DSP test fixture plus native reference test: preserve physical marker/window oracles and independent latency policy evidence. |
| Stop/disconnect, held results, cancellation, processor faults, repeated lifecycles and separate-control-thread tests in `service.rs` | Runtime typed execution/ownership tests and production endpoint/transport tests; retain credits, exactly-once outcomes, recovery progress and stopped cleanup. Record individual mappings before retiring tests. |
| `foreign_tickets_and_routes_refuse_without_consuming_recovery_or_owner_credits`; retained tickets; requested/actual phases | Existing transport identity/cancellation and runtime receipt tests; retire only service-specific wrapper identities/queue counts after matching underlying invariants. |
| State/meter read frequency, zero/callback-end requests and service preparation | DSP meter tests, runtime callback-envelope tests and production partition invariance at larger Q. |

Design clarification for stage 2: use `Processor::admit(node_id, domain)` to validate
actual prepared loop processor identities against the loop-node registry at initial
preparation/install. Graph admission checks both presence of assigned node pairs and
absence of unassigned loop consumers. Keep generic processor mutation protection.
This removes decision bindings without introducing another graph delivery table.
Planner ID remains only where needed to address typed operations; payload bounds
are cached in prepared runtime/node signatures.

Stage 2: one `ProcessBlock`/`Processor::process` and planner begin/finish/operation
lifecycle now serve all production, generic, example and independent fixture callers.
Decision/observation metadata and storage are removed from graph compilation,
validation, builders and runtime traversal. Payload bounds remain in prepared node
signatures and cached planner authority. Loop admission validates actual node roles
and owners, assigned membership and absence of unassigned consumers; new negative
coverage exercises crossed roles/owners and stale physical IDs. The engine's
`signal` module is removed because the common block needs no projection wrapper.

Verification: warning-denying workspace build and all-target check passed; 450
affected native tests passed, followed by all five graph-admission tests including
the new identity case. All 60 independent loop-host fixture tests passed. All 18
runtime book examples executed successfully. Node/Wasm planner execution (8),
graph activation (7), DSP (15), graph guards (5) and engine production (3) passed:
38 total. Test-attribute policy passed. Logs: `work/cleanup-unified-*`,
`work/cleanup-admission-tests.log`, `work/cleanup-loop-fixture-tests.log`.

Retired tests: `rt_runtime/tests/planning_graph.rs` (eight metadata-only tests) and
`stale_planner_candidate_keeps_previous_graph_and_bindings_active` exercise fields
that no longer exist. Canonical/incremental graph validation, stale epochs, descriptor
capacity/membership reuse and actual domain admission remain covered by `graph.rs`,
`graph_activation.rs`, `planner_admission.rs` and loop `graph_guards.rs`. Observation
quota/view-size assertions were replaced with domain-owned completion accounting;
full sample/MIDI and callback-budget assertions remain. Invalid planning-header
fixture cases now corrupt signal descriptors and verify exact returned ownership.
Fixture descriptor and graph-command wire formats are version 2; production wire
versions are unchanged. Documentation prose migration is reserved for stage 6.

Stage 3: `Resolver::prepare` and loop/runtime preparation now require an explicit
quantum. The resolver validates alignment immediately, exposes no setter or adaptive
preview, and completes only its committed tick without a caller-supplied endpoint.
`LoopPlanner` uses the resolver's quantum rather than retaining optional duplicate
mode state; runtime admission rejects mismatched quanta. Arbitrary-callback
preparation is named `prepare_for_host`; it is not an alternate execution mode.
Transactional scratch remains because resource/graph refusals require rollback;
removed boundary search/mode branches had no such role.

Verification: warning-denying workspace build passed; **453 affected native tests**,
**60 loop-host fixture tests**, and **26 Wasm/Node loop tests** passed. The latter
include resolver units, larger-Q sync/program behavior, graph admission and program
changes. Added runtime/domain-quantum mismatch coverage and an internal exhausted-
clock test for every supported Q, including final valid Q=1 completion. The old
public jump-to-u64::MAX test is replaced by that internal clock test; late-delivery
and invalid lifecycle assertions remain public. Direct resolver helpers now drive
complete ticks to their observation frames, keeping scalar/media oracles intact.
Logs: `work/cleanup-quantum-{build,core-tests,fixture-tests,wasm}.log`.

Stage 4: resource stores are prepared with an immutable quantum, checked against
both runtime and loop-domain preparation. Coordinated reservations expose no frame
count and use `CaptureTickId`; group state no longer stores interval length.
Audio staging, MIDI timestamp bounds and atomic completion derive size from the
store quantum. First unpublished and running ticks share the same reservation
implementation while preserving activation and cancellation phases. Group audio
scratch is sized to Q rather than the maximum outer callback. Raw media transfers
and per-event MIDI timestamps retain frame/byte precision.

Verification: **454 affected native tests**, **60 loop-host fixture tests** and
**36 Wasm/Node tests** passed, including the new heap-guarded 16/32/64-frame capture
matrix for wrong-grid admission, stale cancellation, incomplete all-member commit,
immutable old prefixes, exact audio/MIDI and full resource recovery. Standalone
variable-size capture fixtures now preserve their physical media through whole
Q=1 ticks; capture revisions count those actual publications. The independent
resource model likewise reserves one complete tick. Existing preflight retries
remain bounded and transactional. Formatting/test-attribute/tracing checks passed;
warning-denying workspace build passed. Updated Miri inventory removes the retired
metadata tests and includes the new tick capture case. Logs: `work/cleanup-capture-*`.

### Demonstrator retirement map (stage 5)

The six native startup/physical-latency/full-oracle tests move to
`shoop_loops_test_support/tests/{startup,latency_probe,mixed_dsp}.rs`. Their support
assembles the existing `LoopRuntime` directly; no old service control/render code,
queues, tickets or worker lifecycle is copied. Shared test support now forwards
independently typed processor operations. Original 42-frame / 487-packet / 378-byte
and physical-window oracles are unchanged. Nonempty outputs are consumed according
to the generic runtime contract; zero-frame callbacks do not replay retained buffers.
Retirement reclamation is explicit off-render in the test teardown.

Below, runtime tests are in `rt_runtime/tests`, DSP tests in
`shoop_example_dsp/tests/dsp.rs`, and loop scenarios in
`shoop_loops/tests/support/runtime_scenarios.rs` unless another location is named.

| Removed `shoop_engine_service` test | Exact retained coverage / retirement reason |
| --- | --- |
| `actual_loop_and_component_control_match_full_partition_oracles_without_warmup` | New `mixed_dsp::public_scenario_matches_complete_oracles_across_all_partitions`; DSP `complete_scalar_and_packet_oracles_match_across_callback_partitions`; runtime `typed_execution::typed_processor_and_planner_actions_share_exact_frame_ordering_with_parameters`. |
| `disconnected_stop_keeps_recovery_available_when_ui_holds_all_prior_results` | Runtime `typed_execution::recovery_stop_cancels_pending_work_with_every_normal_receipt_reserved` and `unread_normal_results_do_not_block_recovery_or_automatic_intervals`; engine `control::dropping_the_endpoint_silences_output_and_stopped_cleanup_recovers_pools`. The removed wrapper's disconnect-to-Stop policy is not a production UI-loss policy. |
| `processor_fault_keeps_actual_component_and_loop_receipts_without_replay` | DSP `faulted_callbacks_preserve_actual_preset_publication_without_replay`; engine `control::a_render_fault_silences_output_and_ends_the_connection`; loop `stopped_extraction_recovers_active_and_partially_staged_faulted_capture_owners`. |
| `zero_and_callback_end_requests_wait_for_a_real_callback` | Runtime `planner_execution::callback_end_and_zero_length_do_not_settle_or_reexecute_boundaries`; DSP `frame_zero_and_callback_end_activation_have_exact_nonempty_semantics`; migrated mixed scenarios. |
| `prepared_route_cut_keeps_capture_live_and_returns_immutable_prefix_bytes` | Migrated `mixed_dsp` scenario retains the real graph cut, simultaneous recording, old/current exports and complete byte/media comparisons. |
| `refusals_cancellation_and_late_delivery_preserve_requested_and_actual_phases` | DSP `refusal_details_preserve_requested_frames_and_do_not_change_state`; runtime `typed_execution::payload_limits_malformed_operations_and_domain_refusal_return_distinct_outcomes` and `transport_cancellation_precedes_delivery_and_stale_planner_ids_are_rejected`. |
| `retained_terminal_tickets_do_not_silently_recycle_cancellation_flags` | Runtime `lifecycle::cancellation_flag_model_has_a_single_observation_cut`, `full_return_queue_retains_credits_and_recovery_has_reserved_admission` and `typed_schedule::pending_receipts_keep_credits_and_distinguish_delivery_from_completion`. The extra service cancellation-flag pool/API is removed, not reproduced. |
| `state_queries_and_meter_read_frequency_leave_complete_streams_unchanged` | Migrated `mixed_dsp::alternate_valid_query_declaration_order_preserves_the_public_scenario`; DSP `meter_read_frequency_changes_neither_samples_nor_packets`. |
| `foreign_tickets_and_routes_refuse_without_consuming_recovery_or_owner_credits` | Runtime `typed_schedule::foreign_tokens_payload_overflow_and_future_recovery_are_explicit`; `graph_activation::refused_permits_stale_catalog_epochs_and_stage_generations_preserve_both_owners`. Service-specific identities/queue counts are obsolete. |
| `all_normal_credits_and_both_return_streams_can_stall_while_wraps_and_stop_progress` | Runtime `typed_execution::unread_normal_results_do_not_block_recovery_or_automatic_intervals`; loop `recovery_and_cancellation_do_not_require_normal_domain_or_runtime_result_credits`; loop-host `browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled`. |
| `cancel_recovery_executes_with_all_normal_credits_held_then_disconnect_reclaims_future_work` | Runtime `typed_execution::recovery_can_cancel_a_future_queued_typed_operation_without_delivering_it`; `typed_schedule::disconnected_results_retain_pending_and_completed_receipts_without_reexecution`. |
| `saturated_exports_invalid_presets_and_route_rejection_preserve_capture_across_repeated_lifecycles` | Loop `capture_preflight_and_optional_export_refusals_do_not_publish_partial_takes` and `media::mixed_media_node_pairs_and_saturated_exports_share_leases_and_recover_exactly_once`; DSP refusal test above; migrated mixed graph/export lifecycle. |
| `separate_control_thread_observes_exact_execution_while_render_never_waits` | Runtime `lifecycle::two_thread_stress_keeps_completion_and_state_publication_coherent`; loop-host `browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference`. |
| `disconnect_during_recording_retains_full_returns_and_pending_preset_until_stopped_cleanup` | Loop `runtime_topology::pending_program_cancellation_disconnect_and_fault_extraction_return_every_bank_owner`; DSP `disconnected_future_work_has_no_render_side_destruction`; runtime `lifecycle::control_disconnect_retains_payloads_until_non_render_shutdown`. Wrapper stop/mute phase bookkeeping is retired. |
| `fixed_service_preparation_buffers_hosts_and_applies_rounded_dsp_controls` | Runtime `integrated` tests and engine `production::full_endpoint_recording_export_and_slot_reuse_are_partition_invariant` retain larger-Q buffering, operation timing and exact media; DSP frame/receipt tests retain selection behavior. Constructor policies specific to the removed service are obsolete. |

Stage 5: the demonstrator package, service/control/render implementation, manifest,
lockfile entry, examples and wrapper-only tests are removed. The six substantive
scenario/startup/latency tests now use the existing shared loop runtime; five are
portable and one is a native reference comparison. Test support gained generic
processor operations and forwards the existing runtime's command-credit query.
Loop playback/capture scratch is prepared for Q rather than the outer callback.
Current architecture/docs/package selections point at shared test support; archived
validation records retain their original source/package names. The retirement map
above records all fourteen wrapper tests and the service-preparation test.

Verification: warning-denying workspace build and all-target check passed;
**439 affected native tests** passed, including all six migrated tests. All **five
portable migrated tests passed under Wasm/Node**. Cargo metadata contains neither
the removed package nor a dependency on it. Test-attribute, closed 390-module
tracing inventory and Wasm dependency-isolation checks passed. Full independent
media oracles and complete resource-recovery assertions remain intact. Logs:
`work/cleanup-retired-service-*` and `work/cleanup-migrated-dsp-*`.


Stage 6 (2026-11-04): the maintained architecture, runtime book/reference and loop
guides now describe the sole processing/domain interface, immutable preparation
quantum, physical node admission and whole-tick capture. Executable excerpts match
all current examples. Removed ignored adaptive-comparison flags, a duplicate example
run and the demonstrator's dead command-line oracle-corruption switches. No service
package, second processor entry point, graph decision metadata, adaptive resolver
constructor or arbitrary-frame coordinated reservation remains. Domain failure
observations, receipt authority, transactional rollback scratch and capture/lease
generations remain because they implement behavior and ownership safety.

Fresh browser execution found stale JavaScript graph-payload emitters and Python
reference tables. Migrated these to LGP2/version2 and the two signal capacity fields;
retired opcodes4/5 refuse. The regenerated graph report changes only descriptor and
graph-command/result records: complete samples, MIDI, semantic records, primitive
receipts and recovery remain identical. Miri selections include all seven loop unit
cases and the six physical-membership/quantum-admission cases; the explicit selected
inventory now declares 342 executions in 172 commands. This count is an inventory,
not a claim those interpreter executions have all run locally.

Final local gates:

- `cargo fmt --all -- --check`, independent fixture formatting and `git diff --check` passed.
- `RUSTFLAGS="-D warnings" cargo build --workspace` passed before the full suite.
- Full documented nextest command: **2,661 passed, six existing skips**, zero failures.
- All affected package tests on Wasm/Node: **434 passed**. Independent loop fixture:
  **60 passed**, preserving full native media/model/ownership oracles.
- Test-attribute policy, closed **390-module** tracing inventory, feature-parity
  coverage, runtime book links/excerpts, all **18 runnable examples**, all three
  dependency-boundary scripts across their four targets, and **133 Python tooling
  tests** passed.

Logs are `work/cleanup-final-*`, `work/cleanup-full-{native,node}.log`,
`work/cleanup-python-tests.log` and `work/cleanup-*-boundary.log`; generated logs,
tooling, artifacts and `work/` are not committed. Before this evidence entry the
consolidation relative to 52f24c02 changed 182 tracked files, adding 2,965 lines and
removing 7,466 (net **4,501 removed**, excluding binary sizes and the new journal
sidecar). This follows the preceding removal of roughly 16,000 prototype/benchmark
lines. The largest retired component is the demonstrator; useful DSP scenarios
remain in existing shared test support rather than a copied service implementation.

Additional local end-to-end evidence is available for the final stage: actual
non-isolated loop offline/live AudioWorklets pass in debug and release, including
all six deliberate failure gates per profile; the generic runtime worklets and both
failure gates also pass. The 21 production file/retiming Chromium tests pass,
including sync-family import and Q16/Q64 scenarios. Native socket/shared-memory
transport and >32 MiB file canaries pass. The fresh generic runtime journal matches
all 2,601 historical callback inputs/sample hashes, complete MIDI/graph hashes and
work counters. Only four capture revision fields differ because Q=1 publishes each
tick (35→4097,36→4225,36→4225,37→4353). Updated the golden artifact only after that
comparison and successful real-worklet/native replay; its sidecar records current
provenance and the exact changed response offsets. Historical reports are unchanged.
Native-only replay of the updated golden artifact passes.

The consolidated result remains local. Final interpreter completion, delivery/CI,
automated review and human delivery decision are still tracked in stage 7.


Final local acceptance (2026-11-04, following 24fc26c1): the full affected Wasm
suite also passes with the CI-pinned **Node22.22.2: 434 tests**. The production
hosted Chromium UI smoke passes record/play/save/clear/load, bus reconfiguration
and new session with **zero reported xruns**. This is one observed smoke run, not
a general deadline guarantee. Socket and shared-memory companions pass both the
transport workflow and >32 MiB exact WAV/MIDI persistence round-trip, including
socket attachment/recovery, corrupt input refusal, independent abandoned-owner
cleanup, fresh identities after load and exact resave.

Pinned **nightly-2026-05-04 Miri: 26 tests passed**, preserving the existing
900-second per-command watchdog and original workloads:

| Target | Passed |
| --- | ---: |
| runtime `planner_execution` | 8 |
| runtime `planner_admission` | 3 |
| runtime `capture_prepared_interval` | 3 |
| runtime `capture_group_intervals::whole_tick_capture_is_atomic_and_retained_prefixes_survive_at_larger_quanta` | 1 |
| runtime `resource_index` | 5 |
| loops `graph_guards` | 6 |

Evidence: `work/cleanup-miri/result.json` and its six command logs;
`work/cleanup-pinned-node.log`, `work/cleanup-app-browser-smoke.log`,
`work/cleanup-production-retiming-browser.log`, `work/cleanup-native-{probe,files}-*`,
and the actual worklet reports under `target/{loops-host,rt-runtime-host}/reports`.
The complete independent runtime fixture tests also pass. No remote CI or review
has been inspected or run as part of this consolidation, and nothing has been
pushed. The explicit stage-6 pause remains in force; permission to resume delivery
has been requested. Do not mark this plan completely finished until the remaining
stage-7 delivery, cross-platform/package CI and automated-review evidence exists.

Architectural outcome: the removed mechanisms were compatibility APIs and a second
composition, not the ownership foundations. Graph compilation/publication, resource
and MIDI-index pools, coordinated capture transactions, typed receipts/credits,
physical-node admission and explicit stopped reclamation still protect independent
requirements. Quantum processing simplifies their scheduling but does not make
those responsibilities redundant. The retained library/application/host boundaries
also remain useful; further crate deletion would require a separate change of
responsibility rather than this execution-model cleanup.


Delivery resumed (2026-11-04): the user explicitly approved proceeding with all
remaining delivery, CI and review work. The earlier publication pause is lifted;
merge approval remains the final human decision. Refreshed PR930 and fetched its
base: a6e2554c removes obsolete fixed JACK startup-port configuration. Merge that
base while retaining quantum configuration and the completed cleanup, then validate
and push the consolidated result before investigating the current platform matrix.


Delivery integration checkpoint (2026-11-04): merged a6e2554c in e8dc16d5,
retaining the quantum setting and upstream dynamic JACK ports. Both histories used
application protocol19 for incompatible layouts; the combined protocol is20, with
an explicit version19 rejection and updated independent canonical byte tables.
Host setup is6 because its combined layout includes the quantum field as well as
the upstream removals; engine/worklet versions remain7/3. Maintained usage guides
reflect those versions; historical checkpoints retain their original numbers.

Resolved the deferred recovery readiness race: the test child now polls until its
snapshot confirms Recording before publishing its readiness file. Admission of
Record alone did not prove RT activation. The parent's recording-survival assertion
and original workload remain unchanged. This is readiness synchronization rather
than retrying a failed survival assertion.

After integration and these fixes, the warning-denying workspace build, full
native suite (**2,661 passed, six skips**), **117 affected Node22 tests**, all
**21 production file/retiming Chromium tests**, and **133 Python tooling tests**
pass. The first protocol check correctly exposed two stale byte tables after the
version bump; the corrected final full suite above passes. Logs:
`work/cleanup-delivery-{build,native-final,node,retiming-browser,python}.log`.


Review/CI corrections after 435eb2a2 (2026-11-04): Codex review identified the
HostBuffer 64-port restriction. Default JACK preparation reserves 96 audio outputs;
the generic runtime must honor prepared counts. Removed that limit and per-tick
slice-reference arrays. A private borrowed audio interface lends either host slices
or the contiguous prepared buffers to the same graph traversal. It adds no public
processor entry point or alternate planner lifecycle. A heap-guarded independent
128-frame stream oracle verifies all 96/1,025 ports across whole/irregular/zero
callbacks, exact channel reversal and one-quantum latency. Native, Node and Miri
pass; the unchanged Miri workload completed in 583.94s, below the existing 900s limit.

Strict local JACK acceptance then reproduced a structural timing error: Stop and
activation separated by one requested frame could round onto the same tick, making
removal of playing strips Busy. Structural bases and ramp ends now align, with
activation one complete quantum after Stop settlement. A new Q16/Q32/Q64 test with
ramps 4 and Q+3 fails before the fix and passes afterward, retaining every unrelated
output sample and reusing the removed strip/loop slots. All 13 production/structural
tests, including Q1 scalar oracles, pass.

Coverage and Windows debug both exposed a UI shutdown failure. Inspected the two
uploaded Perfetto traces using the pinned repository wrapper/checksum, with its
cache relocated in an uncommitted workspace copy. Original traces show continued
UI/client work but do not record the remote host's completion/error; no timing-only
cause was inferred from them. Improved shutdown assertion diagnostics and reproduced
ShutdownFailed/MIDI-reset-not-confirmed on the first 2-CPU contention trial with
detailed tracing. The dummy owner could exit without its final reset callback when
an in-flight callback exceeded the existing grace period; reset confirmation was
also read before joining the driver. The dummy now completes that final callback
on its render owner, and confirmation is sampled after join. A deterministic
zero-grace/no-sleep regression proves reset completion during join and stopped
cleanup. Both original UI assertions pass in **10 constrained/traced trials × 2
cases** after the fix. The grace period and UI deadlines are unchanged.

Fresh strict JACK acceptance passes all seven commands, including all five session
scenarios through local/socket/shared-memory/TCP, native Master preview, companion
and UI startup. The portable affected core suite passes **436 Node tests**; the
wide-port Miri case brings local selected interpreter evidence to **27 tests**.
Detailed logs: `work/cleanup-review-*`, `work/cleanup-final-review-*`,
`work/cleanup-ui-stress-final.log` and `work/cleanup-native-jack-final`.

First remote run 41871899675: Linux debug/release, all three platform process
inspections and bare-metal gates pass. JACK and coverage/Windows shutdown failures
are addressed above. Both macOS build jobs were canceled without acquiring a runner;
GitHub annotations explicitly report hosted macOS arm64 capacity constraints. These
are unfulfilled platform gates, not passes. Restart the full matrix with the fixes
and obtain fresh automated review before completing stage 7.


Final review-fix local gate: warning-denying workspace build and full native suite
pass (**2,664 passed, six existing skips**). Fresh actual non-isolated loop worklets
pass in offline/live debug contexts with all six deliberate failure gates. Formatting,
test policy, tracing, parity/book checks and all 133 Python tooling tests pass.
