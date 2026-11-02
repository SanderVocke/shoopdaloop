# Core session persistence evidence

This covers the core slice of P01/P02/P11 in the
[plan](../../../PLAN_SESSION_PERSISTENCE_MEDIA_IO.md), delivered by
[PR #906](https://github.com/SanderVocke/shoopdaloop/pull/906) into
`feat/backend-replacement`. All 16 [parity rows](../feature-parity.md) remain open.
The [contract](../persistence-contract.md) records supported fields, finite limits
and explicit refusals. Final validation is recorded in the plan.

## Acceptance coverage

Test paths below are relative to `src/rust/`. Names identify executable production
route coverage; retained reference fixtures also run in the complete native suite.

| Criterion | Implementation and evidence |
| --- | --- |
| 1. Core round-trip | `shoopdaloop/src/persistence_io/tests.rs`: `production_files_roundtrip_audio_midi_and_fresh_session_identities`; `shoop_app/tests/service.rs`: `service_captures_and_reloads_core_images_through_typed_and_wire_paths`, `empty_session_load_and_new_session_use_fresh_epochs_and_remain_usable`; packaged save/load/save compares canonical PCM and exact MIDI, controls and fresh stopped owners. |
| 2. Coherent capture | `shoop_app/tests/service.rs`: `capture_keeps_one_metadata_cut_while_controls_continue_and_view_cancellation_is_independent`, `selected_loop_capture_continues_while_another_loop_records`; `shoop_rt_backend/tests/persistence.rs`: `captured_recorded_samples_remain_exact_and_active_capture_is_refused`. Immutable exports retain old owners while playback and unrelated controls continue. |
| 3. Media workflows | `shoopdaloop/src/persistence_io/tests.rs`: `archive_resampling_channel_reorder_and_standard_midi_import_use_the_public_route`, `task_callbacks_are_fenced_and_rate_decline_preserves_the_session`; `shoop_rt_backend/tests/persistence.rs`: `media_updates_preserve_opposite_media_identity_controls_and_playing_mode`, `selected_capture_ignores_other_recordings_and_imported_views_retain_original_metadata`; `shoop_persistence_codec/src/codec/tests.rs` covers exact/standard formats and refusal of unrestorable MIDI state. |
| 4. Transactional mutation | `shoop_rt_engine/tests/replacement.rs` tests full-owner activation, failed preparation and old-owner exports; `shoop_rt_backend/tests/persistence.rs`: `cancelled_or_invalid_replacement_preserves_all_live_identities_and_media`, `exhausted_old_plus_new_headroom_preserves_old_playback_and_reclaims_failed_stages`; the public file round-trip refuses a candidate one frame over the 256 MiB rendered budget, preserves the playing session, then loads valid media. |
| 5. Operation lifetime | `shoop_app/tests/service.rs`: `accepted_media_bearing_load_survives_ui_loss_and_reconciles_without_replay`; packaged socket probe abandons partial upload and capture, then loses the accepted load acknowledgment and reconciles/retransmits without a second mutation. `shoopdaloop/src/persistence_io/tests.rs` checks stale callbacks across runtimes and real platform output completion after backend loss; native atomic output and browser Cancel/download-link paths are exercised. |
| 6. Bounded work | `shoop_app_protocol/tests/persistence.rs` and `persistence_image.rs` cover declared totals, identity, ordered bounded records and shared immutable ownership. `shoop_rt_backend/tests/persistence.rs`: `large_private_validation_yields_and_checks_samples_outside_the_render_window`, upload-window negotiation and sparse MIDI budgets. `shoop_rt_engine/tests/content.rs` heap-guards repeated eight/64-record upload, disconnect and reuse; 32 physical buffers have a separate finite prepared callback copy allowance. Codec/file readers enforce limits before media allocation. Both release browser forms map 33 MiB mono input to 66 MiB canonical media without whole-media ordinary messages. |
| 7. Compatibility/refusal | Existing `shoop_session` documents, migrations and standalone codecs remain authoritative. `shoop_persistence_codec/document` validates the complete requirement set before preparation; corrupt/unsupported files preserve the old session. `shoop_rt_backend/tests/persistence.rs`: `missing_external_connection_intent_survives_load_and_save_with_a_diagnostic`, `untracked_initial_parameters_refuse_but_a_later_system_reset_is_preserved`. |
| 8. Deployment | Public typed/wire service tests; packaged native `--probe-session` and `--probe-persistence` for socket/shared memory; actual hosted and direct-file Chrome AudioWorklet workflows. The browser service remains on the main thread, codecs run in a dedicated worker, and `application.rs` checks progression without paints and weak pump ownership after drop. The public Lua control inventory has no session-file/New Session/import/export API; existing shared client controls and Lua API/invocation fixtures pass. |
| 9. Parity/delivery | P01/P02/P11 identify the restored slice and all deferred dependencies. PR #906 targets the integration feature branch; final-head CI and review status are linked from the plan and PR. No parity row is closed by this workstream. |

## Deployment scope

Implementation validation is recorded against
`957975a6c52db5185e151ff41742a547b586dde4`. The native suite passes 2486 tests
(four optional/platform skips); CI's complete Node suite passes 2106 tests across
32 packages; the frontend Chrome suite passes 39 tests. The completion regressions
also withhold a confirmed load/New Session projection and verify readiness waits
for that revision, while known commit results survive connection loss.

The [selected Miri report](miri/session-persistence/result.json) and its twelve
hashed logs cover six complete protocol/resource/codec cases with seeds 0 and 1.
They use nightly-2026-05-04 and default borrow/isolation checks. The
[additional engine report](miri/session-persistence/extended-engine.json) preserves
the earlier full engine staging pass and its unchanged-source provenance.

Local native validation uses Linux and the production dummy companion, including
bounded regular-file input and atomic file replacement. Local Chrome 147 uses fake
microphone input and a real AudioWorklet; download completion means the user clicked
the browser download link, not confirmed disk durability. Hosted and self-contained
release canaries also exercise a 33 MiB WAV, exact MIDI, cancellation, fresh load,
exact resave and playback. The ZIP and self-contained HTML are verified artifacts.

macOS, Windows and Firefox validation comes from their actual CI jobs. Local tests
do not claim physical audio-device coverage. Shared-memory operations are covered;
shared-memory reattachment, simultaneous clients, host/page crash recovery and Lua
VM restoration remain outside this workstream.
