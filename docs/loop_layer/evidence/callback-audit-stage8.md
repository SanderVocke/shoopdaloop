# Stage 8 callback reachability and active-guard audit

## Result and source boundary

The Stage8 callback-audit/guard row is verified for implementation
`3d10c7a06f1e4ad01185d047e4a1c44abf525629`, unchanged through consolidated Miri
snapshot `3bf1dbca7c93f486be50cd7359555172dad723f3` and audit HEAD
`e2f808badbba7b111621c5792499ab539c30e210`. No production/test/manifest/lock/runner
change was needed. **Stage8 is5/8; Stage7 remains7/8.** This is not completion of
the broader model, consolidated interpreter, remote CI, examples/final-contract
or delivery/review obligations.

The three technical reviews in the archive cover the new supported callback call
graph, not just lexical matches:

- `transaction-review.md`: graph/topology/program/media/role owner publication,
  rejection, cancellation and callback-shared validation allowances.
- `render-resource-review.md`: executor/transport/scheduler envelope, required
  rendering, capture reservations/publication, indexes/reclamation, consumers and
  resolver progress.
- `planner-protocol-review.md`: the formerly remaining planner operation/query/
  settle/finish/cleanup descendants, complete ledger releases, codec/stager/
  validator phases, graph/export/staging owning-return/fault branches, observer
  journal and Wasm entry points.

The earlier two files retain their honest partial-review headings. Their then-open
items are completed by the third review and the execution/identity proof here;
neither historical partial review is retroactively relabeled as a complete audit.

`source-and-assets.json` compares **207 implementation/test/configuration/runner
inputs** byte-for-byte with all three committed identities above, including both
libraries' source/tests/examples, the loop fixture's source/web/manifests/lock,
shared dev allocator/macros and relevant root inputs. This number is unrelated
to the tracing inventory's coincidentally207 modules. The reviewed library source
surface is50 production modules plus four private test modules. Preparation-only
compilers/constructors and stopped cleanup are classified separately, not silently
assumed safe on a callback. The existing generic fixture callback implementation
is unchanged from the initial-runtime baseline; its allocator import now uses the
same shared dev-only shim. Its fresh bulk-candidate regression remains historical
source-scoped evidence, not a new generic-host run in this audit milestone.

## Reachability, ownership and progress findings

| Callback root/class | Descendants and audit conclusion | Executed guard/error evidence |
| --- | --- | --- |
| `Engine::process` / `GraphRuntime::process` | Zero calls are inert. One nonempty host envelope snapshots ingress/results/recovery and initializes maintenance once. Internal intervals strictly increase frame and do not refresh quotas or consume an arbitrary subdivision allowance. Returned faults clear bounded outputs without rollback or fake finish. | `planner_execution::{dense_planning_uses_one_callback_envelope_without_subrange_quota, ingress_and_recycled_results_wait_until_the_next_callback_not_next_interval, resource_allowances_are_not_replenished_by_planning_or_internal_intervals, required_render_access_and_diagnostics_share_one_callback_envelope, malformed_intervals_and_changed_view_storage_fault_before_consumers}` |
| External owning delivery/return | Prepared flags/rings/slots; one return push transfers or retains the complete packet. Recovery parks the normal head; no producer-chasing loop. Refusals keep command owners; displaced owners enter retirement. Disconnected/defensive owners survive until off-render disposal. | `lifecycle::{invalid_completion_returns_the_owned_retirement_bundle, control_disconnect_retains_payloads_until_non_render_shutdown, render_disconnect_rejects_new_ownership_and_reclaims_every_queue_on_stop, two_thread_stress_keeps_completion_and_state_publication_coherent}`; `prepared_ingress::prepared_executor_ingress_recycles_owning_chunk_buffers_without_heap_operations` |
| Scheduler/typed operation/receipt | Fixed Copy payload/event/result metadata and terminal reservations; bounded external delivery/recycling snapshots. Pending musical requests and automatic intervals remain independent of unread external results. | `typed_execution::{unread_normal_results_do_not_block_recovery_or_automatic_intervals, payload_limits_malformed_operations_and_domain_refusal_return_distinct_outcomes}` |
| Graph/planning activation | Validate scoped generations/epochs/actual capacities before owner moves. Borrowed permits have no owning allocation/custom destructor. Rejected/retired plans remain in fixed banks or reliable returns. No callback compiler/SCC construction. | `graph_activation::{refused_permits_stale_catalog_epochs_and_stage_generations_preserve_both_owners, incremental_validation_and_malformed_or_cancelled_envelopes_never_publish_early, unread_results_and_retired_graphs_do_not_gate_music_and_recovery_cannot_activate}` |
| Capture boundary/interval | Fixed Copy claims/reservations, all-member preflight/staging/commit, reserved final leases, truthful common-prefix freeze. Cancel returns logical credits, not Rust allocations. The64-byte append chunk is not a musical quantum. | `capture_group_model::generated_capture_ownership_matches_independent_pool_and_content_model`; full capture/prepared-interval/boundary/disconnect suites |
| Resource reads/index/state/reclamation | Balanced append index and strict-pretarget checkpoint lookup; positive cursor progress and bounded replay. No full captured-history scan at seek/wrap/freeze/activation. Last release queues scalar reclamation work rather than freeing/walking a full chain. Render counters saturate/report but never deny required data access as Budget. | `resource_index::{indexed_reads_match_flat_bytes_for_all_offsets_and_seeded_wraps_without_budget, foreign_and_released_indexes_and_cursors_cannot_alias_reused_slots}`; full MIDI-index/render/resource/property suites |
| Resolver/composite/program | Prepared flat scratch/state/events; finite component occurrence delivery and validated downstream actions. Preflight retries require a new finite refusal/observation; duplicate latches error. Owner swaps return candidates/displaced topology through retained transactions. No recursive callback, interaction quantum or wave/work cutoff. | Full resolver/compiler/composite/program suites; `runtime_topology` nested lifecycle and paired-publication cases |
| Audio/MIDI consumers | Prepared scratch/state/packet storage; required reads progress or return compact faults. Cleanup→indexed restore→original packets is fixed-state traversal, not history reconstruction. Sink refusal still progresses logical source state. | Full `audio`, `audio_ports`, `midi`, `midi_restore`, media-decision and capture-decision suites; all dense/media/planner host controls |
| Media ledger | Fixed tables and unique free-index stacks; validate duplicate/scoped leases before adoption/publication. Pins/exports/residency/roles are independent logical references. Last release calls deferred resource reclamation. Copy handles are not mistaken for newly acquired leases. | `media::{owner_batch_refusals_preserve_every_input_and_do_not_reserve_generation_or_capacity, seeded_owner_activation_model_checks_references_generations_and_retained_media_after_every_edit, malformed_foreign_duplicate_and_unindexed_inputs_return_owners_without_publication, seeded_ownership_model_preserves_every_live_export_and_all_fixed_pool_credits}` |
| Role/configuration/program publication | Callback-start-keyed source/record/validation allowances; complete identity/shape/claim/retirement preflight before infallible coordinated publication. Cancellation restores/explicitly releases retained inputs. Partial/faulted cleanup is stopped-only. | `configuration_refusals_and_final_resolver_failure_leave_all_three_owners_ready`, `batch_capacity_is_preflighted_without_reusing_retirement_credits_or_publishing_a_prefix`, `disconnect_and_fault_cleanup_recover_source_receivers_partial_batches_and_published_capture_claims`, `paired_retirement_unread_results_recovery_disconnect_and_render_fault_preserve_ownership` |
| Timing codec/stager/validator | Fixed canonical64-byte records, full-chunk validation before mutation, flat pre-reserved vector appends. Finite phase/index progress; unique degree-to-zero recursion queue. Taken banks cannot overwrite owners; recycle errors return owners intact. | Full topology staging, exhaustion, malformed chunk, seeded graph/program reference and runtime topology suites |
| Loop descriptor/protocol bridge | Fixed descriptor banks, actual capacities, ordered records and nonwrapping identities. Stage/Release/refusal paths recycle exact owners; failed recycling restores the completion. Export/resource/program buffers return to their original vacant slot. Unexpected owners quarantine and permanently fault before overwrite. | Fixture `unexpected_owning_completion_is_returned_intact_without_recycling_or_fabricating_a_reply`, `cancelled_owning_chunks_and_128_program_reuses_preserve_real_buffers_and_generations`, `cancelled_reads_eof_stalls_and_128_export_reuses_return_real_owners`, `busy_stale_foreign_and_malformed_records_do_not_advance_or_destroy_owners`, `ingress_refusal_stalled_returns_and_disconnect_keep_all_graph_owners_explicit` |
| Journal/observer/Wasm wrapper | Whole-boundary fixed-storage preflight; stack encodings and saturating optional loss, no musical effect. Wasm callbacks expose scalar codes/borrowed instance-local offsets/fixed responses. Outer heap measurement includes fill/render/collection/MIDI logging; nested guards do not reset it. | Fixture `all_event_variants_preserve_full_width_fields_and_canonical_padding`, `loss_is_sticky_saturating_and_empty_callback_cuts_do_not_consume_records`; actual controls/canaries below |

The reviews classify every ownership-changing `take/replace/clear/drop` by actual
stored element type and slot invariant. Flat Copy metadata clears retain capacity;
prepared plans/processors/boxes move to an empty retained slot or return intact.
Remaining `expect/unwrap/assert` sites depend on private prepared shape/state or a
preceding fallible check, not ordinary malformed transport/resource exhaustion.
The shipped supported call graph contains no callback lock/wait/external I/O,
producer-chasing retry, hidden media-history reconstruction, accidental interval
quota refresh or callback-side owning heap destruction. Scans are supporting
navigation, not a claimed automated reachability/branch-coverage proof.

## Fresh executed verification

All Cargo/Python payloads ran in `nix develop`, with warning-denied Rust/rustdoc.
The archive retains complete logs, exact native discovery, command records,
versions, source/asset identities and structured verification.

- Full debug/release library and standalone-fixture tests: **191runtime +153loop
  +59fixture per profile =806 passing native summaries**, zero failed/ignored/
  filtered. Pinned nextest0.9.116 independently discovers those exact counts;
  `verification.json` correlates named review cases with discovered test identities.
- Warning-denied standalone native and benchmark-example builds in both profiles;
  **72 helper tests** pass. Test-attribute policy, both four-target dependency
  boundary checks and closed207-module tracing inventory pass. Four-target graph
  checks are not remote Windows/macOS builds.
- Fresh both-profile native replay: all17 commands/profile, including capacity-one
  actors and complete media/receipt/semantic/export/owner checks.
- Fresh Node and Chromium public bindings: **20 groups/runtime/profile**.
- Fresh actual non-isolated offline/live controls: **16 successful contexts/profile**
  across primitive, graph, immutable export, staging, dense, transferred-return
  stall, prepared media/exhaustion and real-resolver planner scenarios. The normal
  runner validates complete downstream Web Audio/MIDI/semantic/results/content,
  fixed ordinary memory and full stopped owner/resource recovery, not just exits.
- **Six actual Rust-origin failure gates/profile**, with exact code, origin, heap
  counters, planner phase and unchanged initial/final memory. Wrong/missing witness,
  browser absence/setup exceptions or timeout cannot count as the expected fault.

Native binaries were built against the matching current source. Actual Wasm was
intentionally reused from the verified bulk candidate: unchanged207 inputs and
prior/current binary hashes match before execution. **These are fresh binding/
actual-worklet runs, not fresh Wasm builds or fresh package-Wasm unit-suite runs.**

| Reused asset | SHA256 | Bytes |
| --- | --- | ---: |
| Debug Wasm | `8c697b29f9b4c6b0c422e434201f6e966b72600645310f0978eda97674000ef1` |4118963|
| Release Wasm | `57ea62b0d11a19b19f6bdcb753050e4f59d0877f91b794a1284f3cc5db540426` |918166|

Rust1.97.1/Cargo1.97.1, Node22.22.2 and actual Chromium/ChromeDriver147.0.7727.137
are recorded. Existing default host report directories were preserved separately;
new runs used empty report directories, preventing stale failure evidence from
satisfying a missing run. Only the fresh reports are included in this archive.

### Independent negative controls

Both native profiles fail each specific benchmark-example heap canary with exit101
and `heap operation during rendering`. The proof checks the actual left-hand
counter, not merely a nonzero exit:

| Native canary | Allocations | Reallocations | Deallocations |
| --- | ---: | ---: | ---: |
| allocation |1|0|0|
| reallocation |0|7|0|
| deallocation |0|0|1|

The Wasm controls additionally exercise all three operations in real processor and
planner paths. Both profiles match this complete witness table:

| Actual failure gate | Code | Processor/planner origin | Plans/finishes | Heap alloc/realloc/dealloc |
| --- | ---: | --- | --- | --- |
| processor |10|1/0|not applicable|0/0/0|
| rust-heap |4294901761|2/0|not applicable|4/20/4|
| graph-processor |10|1/0|not applicable|0/0/0|
| graph-rust-heap |4294901761|2/0|not applicable|1/5/1|
| planner |10|0/3|1/0|0/0/0|
| planner-rust-heap |4294901761|0/4|128/128|1/5/1|

The dev-only allocator forwards original pointers/layouts to System; const TLS
bookkeeping does not allocate, unwind or inspect/change allocator ownership.
There is no production unsafe allowance. Intentional fault sessions are abandoned,
not falsely reported as successful recovery; successful controls recover separately.

## Interpreter and acceptance limits

Consolidated job `20261028T153110-784d2380c0` was still **running** at the actual
2026-10-28T16:30:51Z status checkpoint. Its frozen source remains `3bf1dbca`, complete
selection146 commands/299 executions, unchanged seeds/workloads/guards/900s
per-command watchdog. No terminal result or completed-execution count is inferred.
This source-review/evidence-only milestone launches no extra Miri invocation and
cannot replace that pending result. The earlier collected exact recovery remains
one command/one execution, not consolidated coverage or CI margin.

Downstream Processor/Planner/BoundaryObserver implementations must obey their
callback contracts; arbitrary user callbacks are not automatically safe. Rust
heap witnesses do not certify JS/GC/cloning/MessagePort scheduling, device timing,
OS locks/preemption or total RSS. Finite valid music may xrun. No timing comparison,
legacy-engine compatibility, deadline guarantee, complete model-row coverage,
remote CI acceptance or final-head review/completion is claimed.

## Artifact

`callback-audit-stage8-logs.tar.gz` contains the three technical reviews,
source/asset/verification proofs, fresh native discovery/test/build/helper/policy
logs, six native negative logs, both-profile complete native/binding/actual/failure
reports, version records and the one-off commands used for this verification.
Archive: **3,478,604 bytes /173 files**, SHA256
`805f0e4371b9bd85c057d8ee338b5cff9bc9241d4b30b44760d665257f23637c`.
The historical default reports preserved under the local `previous-host-reports/`
are deliberately excluded. All `host-reports/` entries are this milestone's fresh
runs; preliminary review/scan/policy records retain their own source boundaries.
