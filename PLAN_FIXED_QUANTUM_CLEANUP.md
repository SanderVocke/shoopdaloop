# Plan: consolidate quantum processing

Status: implementation and local acceptance complete; final CI in progress. Baseline: `52f24c02` on
`feat/fixed-quantum-runtime`, targeting `feat/backend-replacement` through existing
PR #930 (now ready for review). Continue that branch and PR rather than opening a
parallel migration.

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
- [x] Complete affected packaged native/browser and transport scenarios: typed local,
  socket/shared-memory and MessagePort paths, arbitrary callback adaptation and
  admitted work surviving controller loss. Run selected existing Miri ownership/
  resource tests appropriate to the changed core paths. Require evidence at larger
  quanta as well as exact-frame fixtures; retain heap guards and recovery assertions.
- [x] Resolve remaining validation failures, including previously deferred journal/
  recovery issues, with root-cause evidence. Update fixture journals only for
  explained representation/revision changes; preserve samples, MIDI and semantic
  expectations. Do not replace assertions with retries or weaken them to pass.
- [x] Push the finished branch and update existing PR #930 against
  `feat/backend-replacement`. If the PR has already been superseded, use its successor
  rather than opening a duplicate. Describe the final implementation and validation.
- [ ] Run Linux, Windows and web CI in debug/release, investigate/fix failures and
  repeat affected gates until green. macOS is excluded from this acceptance pass by
  the user's explicit 2026-11-04 instruction; retain its CI definitions. Check
  automated PR reviews and address feedback until the reviewer approves; rerun
  affected validation after changes.
- [ ] Present the final outcome, evidence, residual limits and architectural analysis
  for human review. Keep any merge approval as the final delivery decision; do not
  merge without authorization. Mark this plan complete only with acceptance evidence.

Verification: link final native/browser/Miri/CI evidence and PR review outcomes here.
Human checks or unavailable facilities must be stated explicitly, never checked off
on the basis of a proposed command or a test in a different execution environment.

## Execution evidence and coverage map

Implementation stages 0–6 are complete. The user authorized the remaining delivery,
CI and review work on 2026-11-04. The current implementation is committed through `09a92496`; final CI, renewed
automated review and handoff remain.

| Milestone | Commit | Result |
| --- | --- | --- |
| Plan and regression floor | `816e88b5`, `de30e43e` | Recorded callers and 459 affected native baseline tests; preserved independent media oracles. |
| Sole processor/planner interface | `aa63718e` | Direct borrowed domain; signal-only graph metadata; physical owner/role/membership admission. |
| Mandatory resolver quantum | `bf363eb8` | Immutable validated Q; one preview/commit/finish path; no adaptive constructor or boundary search. |
| Whole-tick coordinated capture | `ab25eddf` | `CaptureTickId`; Q-sized scratch; atomic all-member publication and retained prefix leases. |
| Demonstrator retirement | `bcebee10` | Removed `shoop_engine_service`; six useful startup/DSP/latency tests moved to existing test support. |
| Consolidation and local acceptance | `24fc26c1`, `3f5acc59` | Maintained guides/examples, versioned independent fixtures, actual worklets and selected Miri. |
| Delivery integration and review fixes | `e8dc16d5`, `435eb2a2`, `d43871fb`, `3ca192e1` | Current base, incompatible-peer fences, real JACK acceptance and recovery corrections below. |
| Final source audit and CI regression | `8fa62d6d`, `09a92496` | Removed the ignored interval budget; retirement admission cannot discard physical port ownership. |

The final audit also removes the ignored `Budgets::subranges` field, its settings
and compatibility documentation. Diagnostic tick counts and dense callback-envelope
assertions remain. No production wire format changes are needed for this removal. All2666 native
tests, 437 core Node tests, 35 application-service Node tests, 60 independent fixture
tests, all17 inventoried examples, workspace all-target checks and strict JACK
acceptance pass after the final corrections. The regression reproduces the exact
physical-name reuse on the original implementation and passes with the fix:
`work/cleanup-publication-{before-exact,after}.log`.

The removed metadata-only tests exercised planning fields that no longer exist.
Canonical/incremental validation, stale epochs, descriptor capacity/reuse and domain
admission remain in runtime `graph`, `graph_activation`, `planner_admission` and
loop `graph_guards`. All sample, MIDI, receipt and callback-budget oracles remain.

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


### Integration and validation corrections

Merged target `a6e2554c` while retaining quantum configuration and upstream dynamic
JACK ports. Both histories used app protocol19 for incompatible layouts. The
combined app/engine/host-setup/worklet versions are **20/7/6/3**; old app19 refuses
explicitly. Host setup6 fences the combined quantum field and startup-port removals.
This necessary incompatibility fence is the documented exception to avoiding
production schema changes. Canonical byte tables and maintained guides agree.

Independent graph fixtures use **LGP2/version2** and graph command/result version2;
retired planning opcodes4/5 refuse. Updated JavaScript emitters, Python tables and
graph reports change only those records; complete media and semantic results match.
The generic runtime golden journal preserves all **2,601** callback requests/sample
hashes, complete MIDI/graph hashes and work counters. Only four capture revisions
change from arbitrary intervals to Q1 publications: **35→4097, 36→4225 twice,
37→4353**. Updated it only after actual worklet/native replay agreement. Its sidecar
records changed offsets/provenance; SHA256 is
`873203b4be670722b02fdca740157f7342b8b74e0eeb6cc20c6faff0b68c4e47`.
Historical reports remain unchanged; native-only golden replay passes.

| Observed issue | Correction and regression evidence |
| --- | --- |
| Recovery fixture announced readiness before recording activated | Child waits for a confirmed Recording snapshot before publishing readiness. Parent survival/media assertions are unchanged. |
| Buffering rejected default JACK's 96 outputs at an artificial 64-port limit | Private borrowed audio views feed the same graph kernel without reference arrays or a port cap. Heap-guarded 96/1,025-port oracle checks every reversed channel/sample across whole, irregular and zero callbacks; native, Node and Miri pass. |
| Stop and structural activation rounded onto one tick, refusing playing-strip removal | Align base/ramp end; activate one complete tick after Stop settlement. Fail-first Q16/32/64 tests cover ramps4 and Q+3, exact unrelated audio and reused slots. |
| Coverage/Windows UI shutdown reported MIDI reset unconfirmed | Perfetto inspection plus a constrained reproduction identified the error. Dummy owner now completes its final reset callback; confirmation is read after driver join. Deterministic zero-grace regression and 10 contention/tracing trials × 2 original UI tests pass; deadlines/grace unchanged. |
| Delayed MIDI note survived an acknowledged reset | Pending recovery clears current and delayed output per port. Fail-first heap-guarded Q16/32/64 tests cover explicit reset, output overflow, immediate/one-packet sinks, exact 32-packet order, continuous audio and fresh-note latency. Input/recording history is preserved. |
| Reset acknowledged before a partial input tick rendered | Retain input for recording and defer acknowledgment until the unfinished whole tick completes, with stale output suppressed. One adapter flag arms only on actual reset/overflow requests; fresh traffic cannot starve recovery. Fail-first Q16/32/64 tests cover received and scheduled MIDI (including empty external input), immediate/one-packet sinks, repeated reset during unfinished recovery, exact input-history timestamps/counts, continuous audio and every fresh packet at exact Q latency. Expanded Miri case passes in29.50s. |
| Session restore collided with an existing JACK port name | Refused unpublish admission could retain an old ready flag, discard ownership bookkeeping, then reuse a live name. Transient Unavailable now defers; hard errors clear readiness and retain ownership until successful retirement. A fail-first captured-session regression reproduces the exact physical-name collision, then passes transient/hard errors over typed/encoded delivery without sleeps or weakened assertions. |
| CodeQL mistook a synthetic `nonce` for cryptography | Renamed the deterministic node-generation argument to `seed`. Maximum epoch/sequence tests remain. Fresh CodeQL has no new alerts and automatically resolved its thread; no rule was disabled. |

### Local acceptance

| Gate | Evidence |
| --- | --- |
| Warning-denying workspace build; full native suite | **2,667 passed, six existing skips** on final implementation; `work/cleanup-pending-tick-build.log`, `work/cleanup-pending-tick-native.log`. |
| Complete affected Wasm suite, pinned Node22.22.2 | **473 passed**: 438 core plus all35 application-service tests; `work/cleanup-pending-tick-node.log`. |
| Independent loop fixture | **60 native tests passed**; final seeded descriptor model also passes in `work/cleanup-final-graph-model.log`. |
| Actual non-isolated loop AudioWorklets | Fresh final-code **debug and release**, offline/live, all six deliberate failure gates per profile; `work/cleanup-pending-tick-browser-{debug,release}.log`. |
| Generic runtime worklets | Offline/live media/replay agreement and both deliberate failure gates; `target/rt-runtime-host/reports`. |
| Production browser workflows | **21 Chromium file/retiming tests**, including sync families and Q16/Q64; hosted UI record/play/save/clear/load, bus changes and new session; `work/cleanup-delivery-retiming-browser.log`, `work/cleanup-app-browser-smoke.log`. Zero reported xruns in that smoke is not a deadline guarantee. |
| Native transports and persistence | Socket/shared-memory/TCP workflows, >32 MiB exact WAV/MIDI round trips, corrupt refusal, abandoned-owner cleanup and fresh load identities; `work/cleanup-native-{probe,files}-*`. |
| Strict JACK acceptance | All seven commands pass, including five session scenarios, Master preview, companion and UI startup; `work/cleanup-pending-tick-jack-final.log`, `work/cleanup-pending-tick-jack-final`. |
| Miri, nightly-2026-05-04 | **29 selected tests across this cleanup**, original workloads and 900s watchdog retained; breakdown below. |
| Structural checks | Formatting, test policy, closed **390-module** tracing inventory, book/excerpts, **17 inventoried runnable examples**, parity ledger, dependency boundaries and **133 Python tooling tests** pass; `work/cleanup-final-*`, `work/cleanup-python-tests.log`. |

| Selected Miri target | Passed |
| --- | ---: |
| runtime `planner_execution` | 8 |
| runtime `planner_admission` | 3 |
| runtime `capture_prepared_interval` | 3 |
| larger-Q atomic capture/retained prefixes | 1 |
| runtime `resource_index` | 5 |
| loops `graph_guards` | 6 |
| wide-port buffered-audio oracle | 1 |
| delayed MIDI-output recovery oracle | 1 |
| pending MIDI-input recovery/history oracle | 1 |

Miri logs: `work/cleanup-miri/result.json`, `work/cleanup-review-buffer-miri.log`
(583.94s) and `work/cleanup-midi-miri.log` (20.13s). After removing the ignored budget,
reran all eight planner-execution cases and the MIDI recovery case unchanged;
`work/cleanup-quota-miri-{planner_execution,integrated}.log` pass. The global selected inventory
of342 executions in172 commands is not a claim that all ran locally. Generated
logs/artifacts/tooling stay untracked. The first final native launch lacked socket
permission; its unchanged rerun with socket/shared-memory permissions passes.
A Wasm link exhausted workspace disk; rebuilding generated artifacts with debug
symbols off produced the passing 437-test run. Neither changed source assertions.

### Remote acceptance and handoff

[PR930](https://github.com/SanderVocke/shoopdaloop/pull/930) targets
`feat/backend-replacement`, is ready and is pushed through `820fb239` (implementation
`09a92496` followed only by plan consolidation).
[Automated review](https://github.com/SanderVocke/shoopdaloop/pull/930#issuecomment-6636594400)
completed clean on `820fb239`. Review of the subsequent CI/plan update found the
pending-input reset case above; address it and obtain fresh review on the fix.
[Docs](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890787505), strict
JACK including driver feature combinations, bare-metal and Windows/Linux process
inspection pass. [Current implementation CI](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890787503)
and [CodeQL](https://github.com/SanderVocke/shoopdaloop/actions/runs/41890787410)
are still running. The user explicitly waived macOS validation on2026-11-04
because of hosted-runner contention. Linux, Windows and web debug/release builds,
Windows/Linux inspection and all portable/Linux acceptance remain required.
Removed the build matrix's unnecessary dependency on the inspection matrix, so
queued macOS inspection cannot serialize other platform builds. All macOS jobs
remain defined for future execution; their result is excluded from this acceptance
pass. The required non-macOS gates still need to complete.

The preceding run's macOS arm64 inspection was canceled before acquiring a runner;
GitHub annotations explicitly identify hosted capacity constraints. Selective retry
obtained the runner. The initial JACK package download took16m45s for52.7MB; its
selective retry exposed the retirement-admission bug reproduced and fixed above.
No platform was removed, no assertions/deadlines were weakened and no required
workload was shortened to address those failures.

At implementation checkpoint `820fb239`, the PR changes313 tracked files against
`a6e2554c`, adding11,676 lines and removing12,432: **756 net lines removed**. Relative to this plan’s `52f24c02` starting point,
that checkpoint adds4,107 lines and removes8,591: **4,484 net lines removed**.

### Architectural outcome

Removed the compatibility mechanisms and the second demonstrator composition.
Graph compilation/publication, resource and MIDI-index pools, coordinated capture
transactions, physical-node admission, typed receipts/credits and explicit stopped
reclamation still enforce independent lifetime and recovery requirements. The
library/application/host boundaries still separate useful responsibilities.
Further major crate removal is not justified by quantum processing alone. Broader
feature restoration in the parity ledger remains outside this cleanup.

Latest review follow-up: added the pending-input recovery guard and permanent
regression, preserving all input for recording while suppressing stale output.
Final local build/full native passes **2,667 tests, six existing skips**; all
**473 Node tests** pass. New Miri evidence: `work/cleanup-input-reset-miri.log`
(14.77s), bringing selected cleanup cases to29. The preceding push’s Linux debug,
Windows debug and web release gates passed; rerun required gates after this fix.

Fresh strict JACK acceptance and actual debug/release offline/live worklets with
all six deliberate failure gates pass after the pending-input guard; logs are
`work/cleanup-input-reset-jack.log` and
`work/cleanup-pending-tick-browser-{debug,release}.log`.

Follow-up self-review extends the same guard to the entire partially collected
tick: due MIDI injections with empty external input otherwise reproduce the same
stale acknowledgment. The fail-first expansion is
`work/cleanup-pending-tick-{before,after}.log`. The guard is now an O(1) fill check,
using the same single bit; no additional state or graph processing mode. Complete
native2667/Node473 suites, expanded Miri29.50s, and actual debug/release worklets
with all failure gates pass on the strengthened code.

The final strict JACK rerun passes all seven commands. Its first local attempt
failed before the nested server-loss fixture started: JACK reported too many
active servers. Inspection with the same shared-memory namespace found no live
JACK server and seven dead fixture registrations left by repeated intentional
SIGKILL tests. Backed up the registry, verified dead PIDs and current ownership,
and cleared only those28 dead test objects and their registry. The unchanged
suite then passed (`work/cleanup-pending-tick-jack-final.log`); failure assertions
and deadlines remain intact.
