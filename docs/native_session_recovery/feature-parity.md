# Feature parity ledger

This is the handoff from the session-service migration to the forthcoming feature
parity work. These are temporary implementation gaps, not decisions to remove
features from ShoopDaLoop. Full parity is outside the recovery milestone; preserving
this backlog and its evidence is part of that milestone.

Status: recovery milestone complete on code/test candidate `929ed71b`, with a green
full platform matrix. P13 is now COMPLETE within negotiated replacement-engine
bus limits; the other 15 parity rows remain OPEN. Recheck the production capability set and client/service
handlers whenever implementation changes. A green recovery CI does not close these
rows. A row remains **OPEN** until its completion evidence is linked here.

The first parity workstream is
[core session persistence and media I/O](../../PLAN_SESSION_PERSISTENCE_MEDIA_IO.md),
on `feat/session-persistence-media-io`, targeting `feat/backend-replacement`.
It restores current-core save/load/export (P01/P02) and standalone audio/MIDI
import/replacement (part of P11). These file workflows are enabled and validated
on this branch;
[acceptance evidence](evidence/session-persistence.md) records native Linux/macOS/Windows,
hosted/self-contained browsers, socket recovery and selected Miri coverage.
FX, composites, session scripts and other unsupported document requirements still
refuse as a whole. Content duplication, generated click insertion and the remaining
MIDI initial/preroll state cases stay open in P11; click generation/preview stays
open in P15. P01/P02/P11 remain OPEN for their deferred dependencies. The
[output bus work](evidence/output-buses.md) closes P13, including zero-bus
save/load; **15 rows remain OPEN**. Bus FX remains P08.

The [master reconciliation](master-reconciliation.md) preserves the newer range
editor and per-channel timing behavior in the reference stack. P12 still requires
authoritative service admission, atomic single/all-channel updates, RT execution,
persistence and recovery coverage before production timing edits can be enabled.

## What is already available

The service supports direct audio/MIDI/trigger tracks, primitive loop creation and
removal, record/play/stop and scheduled transitions, names/order, musical selection
and targeting, supported global policies, gain/balance/monitoring/mute, ordinary
bus controls/lifecycle/order, host connections and mixer routes, piano MIDI injection, and bounded
waveform/MIDI reads. UI and Lua use the same confirmed service projection. Socket
UI recovery retains these states and admitted operations. These are the regression
floor for parity work, not a claim that all old behavior has been ported.

Recording uses a finite prepared host media pool, normally 256 MiB shared across
loops. Budget capacity for recording duration and channel count; exhaustion ends
capture. The [capacity diagnostic](evidence/recording-capacity.md) records this
existing limit and the recovery stress fixture's corresponding budget.

## Parity status

Paths in this table are relative to `src/rust/`. Reference test names are searchable
in `shoop_app_reference/src/lib.rs` unless another location is given. The old model
is executable reference evidence; it is not a production dependency. Engine primitives
may already exist in `shoop_loops`/`shoop_rt_engine`; each restoration must establish
what is missing rather than assume the whole feature needs a new engine primitive.

| ID / status | Feature and present production limit | Preserved behavior, UI and code | Work needed to close the gap |
| --- | --- | --- | --- |
| P01 OPEN | Core coherent session save and WAV/loop-audio/exact-MIDI/SMF export are enabled, including zero or multiple supported buses. Active/scheduled recording refuses without stopping it. Browser completion reports download handoff. | `shoopdaloop/persistence_io`, `shoop_persistence_codec`, service persistence jobs and retained reference codecs; production `production_files_roundtrip_audio_midi_and_fresh_session_identities`; immutable capture/old-owner/playing-export tests in `shoop_app` and `shoop_rt_backend`. See [persistence acceptance evidence](evidence/session-persistence.md) and [bus evidence](evidence/output-buses.md) for deployment scope and limits. | Extend capture/file conversion as FX (P07/P08), composites (P03), dry/wet (P06), latency/take state (P09/P10) and session scripts/resources (P16) become available. UI recovery itself still has no disk durability. |
| P02 OPEN | Core structure-only and media-bearing loads plus New Session use fenced, atomic replacement and fresh live IDs, including zero or multiple supported buses. Unsupported documents refuse intact; loaded loops start stopped. | Existing `shoop_session` formats/migrations/resampling; `shoop_persistence_codec/document`; service/adapter private candidates; `accepted_media_bearing_load_survives_ui_loss_and_reconciles_without_replay`, production save/load/save tests and [bus evidence](evidence/output-buses.md). | Restore each deferred document feature under its own row. Keep whole-document capability diagnostics, sample-rate consent, stable source/live mappings, missing-port intent and transactional refusal as coverage expands. |
| P03 OPEN | Regular and Lua-scheduled composite loops, their editor operations, and composite-dependent auto-arm behavior are refused. Lua remains client-side. | `shoop_egui/{composite_loop_widget,details_pane}`; composite documents in `shoop_session`; reference composite planners/schedulers; tests `public_intents_drive_three_section_engine_composite_and_isolate_child_control`, `regular_script_composition_plays_serial_sections_and_wraps_without_sync`, `auto_arm_reconciliation_follows_engine_composite_boundaries_and_preserves_prearmed_tracks`. | Composite protocol/state/operations, RT scheduling integration and service-owned accepted work; retain editor semantics and child independence. Explicitly settle scheduling ownership without making recovery depend on a lost Lua VM. |
| P04 OPEN | Grab and grab-with-target policies are refused. | `shoop_app_api` loop/global actions; `shoop_egui/{loop_widget,global_controls}`; reference `grab_targets_inner`; tests `grab_policy_covers_targeted_selection_solo_and_immediate_completion`, `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`. | Bounded grab/ringbuffer integration with authoritative target/selection resolution, sync policy, outcome reporting and media continuity. |
| P05 OPEN | Replacing mode is unavailable. | `BackendCapability::ReplacingMode`; loop mode/actions/UI; reference `expanded_loop_actions_route_modes_grab_and_balance` and engine/loop tests. | Service/protocol admission and adapter/RT replacement semantics; cover selected versus explicit loop control, stop, sync and recorded content. |
| P06 OPEN | Dry/wet tracks, dry-through-wet playback, dry-into-wet recording and their default actions are unavailable. | Track topology/port-role contracts; `shoop_egui/app_widget` add-track forms and details; reference tests `application_adds_external_dry_wet_tracks_from_processor_capabilities`, `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`, `dry_wet_media_io_maps_and_exports_role_order_without_flattening`. | Track graph/role mapping and mode integration across service/protocol/adapter/engine; preserve channel-role order, processor routes and explicit/default action distinctions. |
| P07 OPEN | Track FX, Built-in FX, Built-in Synth/OxiSynth, external dry/wet processing and native Carla hosting/editor controls are unavailable. | `shoop_egui/{builtin_fx_editor,oxisynth_editor,track_controls}`; processor catalog/state contracts; reusable DSP/Carla code in `shoop_engine`, `shoop_plugin_protocol` and `third_party/carla`; reference tests `builtin_fx_session_round_trip_preserves_n_channels_controls_and_assignments`, `oxisynth_session_round_trip_preserves_selected_preset`, `processed_track_session_round_trip_preserves_roles_state_and_recorded_take`, `failed_recorded_fx_restore_leaves_the_processed_track_usable`. | Processor preparation/lifetime and graph integration, service-owned state/operations, native UI-thread adapter, control MIDI/learn, editor lifecycle, persistence and resource packaging. Restore native/browser features according to their original platform scope; Carla was native. |
| P08 OPEN | Bus FX and global FX control MIDI routing are unavailable. | `shoop_egui/bus_controls`, FX dialogs/contracts; reference bus FX handlers and `global_fx_port_round_trips_legacy_migrates_and_malformed_load_is_transactional`; independent processor tests. | Bounded bus processor graph/state, MIDI routing and confirmation across service/RT and persistence; avoid duplicate control traffic through combined track/bus routes. |
| P09 OPEN | User-configured latency compensation is refused. | `shoop_latency`; latency document/API fields and details UI; reference `latency_intents_update_future_operations_without_moving_the_active_take`. | Define service/engine application boundaries for track/processor latency, pending future operations and active takes, with protocol projection and round-trip persistence. |
| P10 OPEN | Recording offsets, take alignment and processor-latency adjustment controls are refused. | Take/offset contracts and UI; `shoop_latency`; same reference latency test plus engine recording tests. | Restore explicit/automatic adjustment semantics and future-take policy without moving an active take; cover audio and MIDI frames and save/load. |
| P11 OPEN | Core WAV/loop-audio/exact-MIDI/SMF import and replacement are enabled, including channel mapping, rate consent and optional length updates. Duplication and generated click insertion remain refused. Vendor/system initial or preroll MIDI and untracked parameter history remain refused; limits are explicit in the persistence contract. | Production file provider/portable codecs, bounded service staging and atomic RT content apply; opposite-media/identity/control preservation, logical-window exports and cancellation tests. `LoopDuplication` and `ClickTrackInsertion` now have separate flags. Reference click/duplication UI, assets and tests remain intact. | Restore content duplication, generated click insertion, and complete MIDI state/preroll/seek handling without dropping packets or parameter history. Click generation/preview remains P15; wider/dry-wet roles depend on P06/P07. |
| P12 OPEN | Loop length, start offset, pre-play and timeline edits are refused. | `shoop_app_api` timing contracts; `shoop_egui/{details_pane,waveform_widget,midi_sequence_widget}`; reference timing/content handlers and loop-library tests. | Authoritative timing edits and validation, RT execution/publication and persistence; verify edits to stopped, playing, recording and synced loops. |
| P13 COMPLETE | Ordinary buses, lifecycle/order, controls/meters, explicit routing and zero-bus save/load are supported within negotiated replacement limits (default eight buses, eight channels per bus). Master is an ordinary removable default. | Production engine/adapter/service/native/browser tests, selected Miri and retained reference fixtures; [acceptance evidence](evidence/output-buses.md), [completed plan](../../PLAN_OUTPUT_BUSES.md), [green platform matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542902). | Complete within the user-approved capacity qualification. Bus FX remains unavailable under P08; legacy 64-channel capacity is outside this restoration. |
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
| `LoopDuplication` | P11 |
| `ClickTrackInsertion` | P11 |
| `LoopTimingEdits` | P12 |
| `BusManagement` | P13 |
| `AudioDriverSwitching` | P14 |
| `MidiInjection` | AVAILABLE |
