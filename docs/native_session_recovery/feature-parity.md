# Feature parity ledger

This is the handoff from the session-service migration to the forthcoming feature
parity work. These are temporary implementation gaps, not decisions to remove
features from ShoopDaLoop. Full parity is outside the recovery milestone; preserving
this backlog and its evidence is part of that milestone.

Status: recovery milestone complete on code/test candidate `929ed71b`, with a green
full platform matrix and all 16 parity gaps still OPEN. Recheck the production capability set and client/service
handlers whenever implementation changes. A green recovery CI does not close these
rows. Every row remains **OPEN** until its completion evidence is linked here.

## What is already available

The service supports direct audio/MIDI/trigger tracks, primitive loop creation and
removal, record/play/stop and scheduled transitions, names/order, musical selection
and targeting, supported global policies, gain/balance/monitoring/mute, Master
controls, host connections and mixer routes, piano MIDI injection, and bounded
waveform/MIDI reads. UI and Lua use the same confirmed service projection. Socket
UI recovery retains these states and admitted operations. These are the regression
floor for parity work, not a claim that all old behavior has been ported.

## Open gaps

Paths in this table are relative to `src/rust/`. Reference test names are searchable
in `shoop_app_reference/src/lib.rs` unless another location is given. The old model
is executable reference evidence; it is not a production dependency. Engine primitives
may already exist in `shoop_loops`/`shoop_rt_engine`; each restoration must establish
what is missing rather than assume the whole feature needs a new engine primitive.

| ID / status | Feature and present production limit | Preserved behavior, UI and code | Work needed to close the gap |
| --- | --- | --- | --- |
| P01 OPEN | Session capture/save and loop audio/MIDI export are refused. UI recovery has no disk durability. | `shoop_session`; `shoop_app_reference` session encoding/media mapping; tests `cooperative_session_round_trip_warns_before_resampling_and_rejects_old_files`, `dry_wet_media_io_maps_and_exports_role_order_without_flattening`; file/menu intents in `shoop_app_api` and `shoop_egui/app_widget`. | Service-owned coherent capture with recording policy, bounded media/encoding and cancel/result semantics; client-local file output/export workflow. Round-trip structure, content, controls, routing, FX and session scripts as their rows become available. |
| P02 OPEN | All session loads are currently refused by the client, including structure-only documents. This is broader than the `SessionContentLoad` capability name. | `shoop_session` document validation/migrations/resampling; reference `begin_session_load`/`apply_loaded_session`; tests `unsupported_deferred_topology_is_rejected_without_replacing_the_session`, `session_documents_list_every_unsupported_capability_they_need`, `cancelling_committing_session_load_releases_backend_replacement`. | Staged service admission/validation/replacement, transactional failure/cancel behavior and stable app/engine mappings; separate structure-only versus media-bearing loads. Preserve the running session when any required feature is unavailable. |
| P03 OPEN | Regular and Lua-scheduled composite loops, their editor operations, and composite-dependent auto-arm behavior are refused. Lua remains client-side. | `shoop_egui/{composite_loop_widget,details_pane}`; composite documents in `shoop_session`; reference composite planners/schedulers; tests `public_intents_drive_three_section_engine_composite_and_isolate_child_control`, `regular_script_composition_plays_serial_sections_and_wraps_without_sync`, `auto_arm_reconciliation_follows_engine_composite_boundaries_and_preserves_prearmed_tracks`. | Composite protocol/state/operations, RT scheduling integration and service-owned accepted work; retain editor semantics and child independence. Explicitly settle scheduling ownership without making recovery depend on a lost Lua VM. |
| P04 OPEN | Grab and grab-with-target policies are refused. | `shoop_app_api` loop/global actions; `shoop_egui/{loop_widget,global_controls}`; reference `grab_targets_inner`; tests `grab_policy_covers_targeted_selection_solo_and_immediate_completion`, `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`. | Bounded grab/ringbuffer integration with authoritative target/selection resolution, sync policy, outcome reporting and media continuity. |
| P05 OPEN | Replacing mode is unavailable. | `BackendCapability::ReplacingMode`; loop mode/actions/UI; reference `expanded_loop_actions_route_modes_grab_and_balance` and engine/loop tests. | Service/protocol admission and adapter/RT replacement semantics; cover selected versus explicit loop control, stop, sync and recorded content. |
| P06 OPEN | Dry/wet tracks, dry-through-wet playback, dry-into-wet recording and their default actions are unavailable. | Track topology/port-role contracts; `shoop_egui/app_widget` add-track forms and details; reference tests `application_adds_external_dry_wet_tracks_from_processor_capabilities`, `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`, `dry_wet_media_io_maps_and_exports_role_order_without_flattening`. | Track graph/role mapping and mode integration across service/protocol/adapter/engine; preserve channel-role order, processor routes and explicit/default action distinctions. |
| P07 OPEN | Track FX, Built-in FX, Built-in Synth/OxiSynth, external dry/wet processing and native Carla hosting/editor controls are unavailable. | `shoop_egui/{builtin_fx_editor,oxisynth_editor,track_controls}`; processor catalog/state contracts; reusable DSP/Carla code in `shoop_engine`, `shoop_plugin_protocol` and `third_party/carla`; reference tests `builtin_fx_session_round_trip_preserves_n_channels_controls_and_assignments`, `oxisynth_session_round_trip_preserves_selected_preset`, `processed_track_session_round_trip_preserves_roles_state_and_recorded_take`, `failed_recorded_fx_restore_leaves_the_processed_track_usable`. | Processor preparation/lifetime and graph integration, service-owned state/operations, native UI-thread adapter, control MIDI/learn, editor lifecycle, persistence and resource packaging. Restore native/browser features according to their original platform scope; Carla was native. |
| P08 OPEN | Bus FX and global FX control MIDI routing are unavailable. | `shoop_egui/bus_controls`, FX dialogs/contracts; reference bus FX handlers and `global_fx_port_round_trips_legacy_migrates_and_malformed_load_is_transactional`; independent processor tests. | Bounded bus processor graph/state, MIDI routing and confirmation across service/RT and persistence; avoid duplicate control traffic through combined track/bus routes. |
| P09 OPEN | User-configured latency compensation is refused. | `shoop_latency`; latency document/API fields and details UI; reference `latency_intents_update_future_operations_without_moving_the_active_take`. | Define service/engine application boundaries for track/processor latency, pending future operations and active takes, with protocol projection and round-trip persistence. |
| P10 OPEN | Recording offsets, take alignment and processor-latency adjustment controls are refused. | Take/offset contracts and UI; `shoop_latency`; same reference latency test plus engine recording tests. | Restore explicit/automatic adjustment semantics and future-take policy without moving an active take; cover audio and MIDI frames and save/load. |
| P11 OPEN | Audio/MIDI content import/replacement, content duplication and generated click-track insertion are refused. | `shoop_session` media codecs; `shoop_egui/{details_pane,click_track_dialog}`; reference `click_generation_and_preview_preserve_opposite_media_and_stable_identity`, `generated_click_update_resets_target_offsets_and_preserves_other_media`, `click_generation_rejects_conflicts_and_failed_update_keeps_content`. | Bounded upload/staging/commit protocol, service-owned admission and tracking, adapter/RT content replacement and correct opposite-media/identity/offset preservation. Keep partial import failure transactional. |
| P12 OPEN | Loop length, start offset, pre-play and timeline edits are refused. | `shoop_app_api` timing contracts; `shoop_egui/{details_pane,waveform_widget,midi_sequence_widget}`; reference timing/content handlers and loop-library tests. | Authoritative timing edits and validation, RT execution/publication and persistence; verify edits to stopped, playing, recording and synced loops. |
| P13 OPEN | Additional buses, their lifecycle/order, and non-Master routing shapes are unavailable. Master and supported routes are available. | `shoop_egui/{bus_controls,connection_dialog,app_widget}`; reference tests `bus_lifecycle_and_visual_order_use_stable_identity_without_backend_reorder`, `bus_structure_rejection_and_timeout_reconcile_without_false_state`, `bus_controls_reconcile_confirmation_failure_timeout_stale_ids_and_peaks`, `new_tracks_route_to_named_bus_with_equal_channels_and_mono_fanout`. | Service/protocol bus structures and identities, prepared engine graph capacity/lifecycle, route confirmation, controls/meters and capture/load. Preserve rejected/partial-operation recovery. |
| P14 OPEN | Live audio-driver switching is refused. Saved preferences apply on the next host start; JACK preferences map to CPAL with a diagnostic. | Audio profiles and confirmation/settings UI; independent driver/device code; reference tests `audio_driver_switch_requires_confirmation_and_resamples_transactionally`, `failed_audio_driver_switch_restores_prior_runtime_and_reports_failure`, `recording_blocks_confirmed_audio_driver_switch_without_stopping_recording`, `persistence_failure_keeps_new_driver_active_and_enables_save_retry`. | Host-managed device/engine transition with service-owned capture/restore, preflight/confirmation, resampling and rollback. Keep preference persistence and its retry separate from actual driver-switch success. Track native driver/backend equivalence explicitly. |
| P15 OPEN | System/Master click preview and click generation UI-local workflow are currently unavailable; preview state/assets are not populated by the new client. | `shoop_egui/click_track_dialog`; `shoopdaloop/{src/native_preview.rs,src/browser_preview.rs}`; click DSP/assets and reference tests named in P11. Original frontend preview canaries are retained in Git provenance below. | Restore local generation/cancel/stale-result handling and system output; use explicit service operations for Master playback/content insertion. A useful client-only preview can be restored independently of P11. |
| P16 OPEN | Session/filesystem script ownership conversion and related session resource workflows are unavailable. Script source export, fresh scripts, local dialogs, timers, controller MIDI and explicit starts are available. | `shoop_scripting`, `shoop_script_resources`, `shoop_egui/script_dialogs`; reference tests `scripts_export_exact_source_and_convert_session_ownership`, `filesystem_script_conversion_captures_resources_off_actor_and_survives_source_removal`, `session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts`. | Restore client-local file/resource workflows and explicit session-owned script data through service commands, without moving VM execution to the host. Respect the recovery no-auto-restart rule. |

## Preserved evidence and retirement record

- `shoop_app_reference` retains the old model and its executable behavioral tests,
  including its RT adapter contract tests. It is excluded from production dependency
  graphs. `shoop_backend`'s legacy feature and the independent old engine/worklet
  packages remain available to these fixtures; they are not fallback backends.
- `reference/legacy_browser/` retains the old browser host JS/Rust glue and integrated
  acceptance scenario as migration evidence. These sources are not shipped or
  compiled into the product. Independent worklet/client/codec tests remain runnable.
- `reference/legacy_application_readme.md` preserves detailed old-stack user behavior,
  processor/driver configuration and platform descriptions for restoration.
- `reference/legacy_frontend_acceptance.rs` preserves the original driver-switch,
  preview and broad dummy UI scenarios that currently require unsupported features.
  Their runnable model-level counterparts remain in `shoop_app_reference`.
- Complete source provenance before this retirement is commit `925db5b2`:
  [the old app model](https://github.com/SanderVocke/shoopdaloop/blob/925db5b2f05d4fac9fd34fbcc8cdd4d0aa488819/src/rust/shoop_app/src/legacy.rs)
  and [the original frontend](https://github.com/SanderVocke/shoopdaloop/blob/925db5b2f05d4fac9fd34fbcc8cdd4d0aa488819/src/rust/shoopdaloop/src/main.rs).
  The latter preserves original frontend canaries, notably
  `native_application_builtin_fx_save_reload_smoke`,
  `confirmed_driver_switch_is_saved_once_and_completed_after_persistence`,
  `failed_driver_settings_save_retries_without_switching_backend_again`,
  `native_preview_buttons_share_cancellation_and_ignore_stale_results`, and
  `native_dummy_workflow_creates_records_and_controls_tracks_and_loops`.
  Port these UI-bound expectations when their feature is restored; reference-model
  coverage alone is not a substitute for exercising the new production route.
- Obsolete backend-selection, engine-child fatal-disconnect and Carla-worker-entry
  canaries are superseded by sole-backend, independent-host recovery and authenticated
  shutdown tests. The removed launch architecture itself is not a parity target;
  its useful processor/driver behavior belongs to P07/P14.
- Keep existing feature editors, intents, configuration/document fields, assets and
  meaningful tests unless a replacement or provenance entry is recorded here.
  Unsupported status alone is insufficient reason to delete them. Production package
  cleanup may remove unused runtime payloads while preserving their build/source
  recipe under the relevant row (notably Carla and legacy worklets).

## Closing a row

For each restored feature, add implementation links and evidence here before marking
it complete. Verify the public UI and Lua behavior against the reference; service
ownership, admission/results and coherent projection; bounded protocol/media work;
RT scheduling/heap constraints; native typed/socket and applicable browser behavior;
and persistence and recovery interaction where relevant. Name narrower partial
restorations explicitly. Run the old behavioral fixtures as well as new production
contract/UI tests. Do not advertise support merely because a UI control exists or
an engine primitive compiles.

The following remain deliberate recovery-milestone exclusions rather than missing
old-stack features: recovery of a Lua VM or UI draft, automatic script restart after
reattachment, host-crash recovery/disk durability, browser-page crash recovery,
simultaneous controlling clients, remote deployment, and shared-memory reattachment.
Changing those limits is a separate scope decision. Planned parity may restore old
features without changing these recovery rules.

## Capability coverage

Every capability flag must point to an open restoration item or current production
support. Client-only gaps P15/P16 remain tracked separately. The coverage check
fails when a capability is added or removed without reconciling this table.

| Capability | Ledger status |
| --- | --- |
| `SessionCapture` | P01 |
| `SessionContentLoad` | P02 |
| `CompositeLoops` | P03 |
| `Grab` | P04 |
| `ReplacingMode` | P05 |
| `DryWetModes` | P06 |
| `DryWetTracks` | P06 |
| `TrackFx` | P07 |
| `BusFx` | P08 |
| `LatencyCompensation` | P09 |
| `TakeAlignment` | P10 |
| `LoopContentImport` | P11 |
| `LoopTimingEdits` | P12 |
| `BusManagement` | P13 |
| `AudioDriverSwitching` | P14 |
| `MidiInjection` | AVAILABLE |
