# Feature parity ledger

This is the handoff from the session-service migration to the forthcoming feature
parity work. These are temporary implementation gaps, not decisions to remove
features from ShoopDaLoop. Full parity is outside the recovery milestone; preserving
this backlog and its evidence is part of that milestone.

Status: recovery milestone complete on code/test candidate `929ed71b`, with a green
full platform matrix. P03 is COMPLETE for the user-approved direct-track composite
core; dependent composite modes are tracked separately in P17. P12, P13 and P15
are also COMPLETE within their recorded timing, bus and preview scopes. The other
**12 of 17 parity rows remain OPEN**. Recheck the production
capability set and client/service handlers whenever implementation changes. A green
recovery CI does not close these rows. A row remains **OPEN** until its completion evidence is linked here.

The first parity workstream is
[core session persistence and media I/O](../../PLAN_SESSION_PERSISTENCE_MEDIA_IO.md),
on `feat/session-persistence-media-io`, targeting `feat/backend-replacement`.
It restores current-core save/load/export (P01/P02) and standalone audio/MIDI
import/replacement (part of P11). These file workflows are enabled and validated
on this branch;
[acceptance evidence](evidence/session-persistence.md) records native Linux/macOS/Windows,
hosted/self-contained browsers, socket recovery and selected Miri coverage.
Carla/bus FX, dependent composite modes, session scripts and other unsupported requirements still
refuse as a whole. Audio/MIDI click generation and insertion are also enabled through the production
file provider and bounded media-import path. Content duplication and the remaining
MIDI initial/preroll state cases stay open in P11. System/Master click preview is
COMPLETE under P15; its [contract and completion evidence](preview.md) record the
accepted testing boundary. P01/P02/P11 remain OPEN for their deferred dependencies.
The [output bus work](evidence/output-buses.md) closes P13, including zero-bus
save/load. Bus FX remains P08.

The [master reconciliation](master-reconciliation.md) preserves the newer range
editor and per-channel timing behavior in the reference stack.
[P12 acceptance](evidence/loop-timing.md) closes production timing edits, including
real pre-play, atomic single/all-channel updates, retained raw media, persistence
and recovery under the existing Q-aligned logical-period contract.

P03 is COMPLETE under [PLAN_P03_COMPOSITES.md](../../PLAN_P03_COMPOSITES.md).
The accepted [composite execution contract](composites.md) uses frame-driven
arrangements with launch/repeat alignment, superseding source-tick progression.
The user approved moving replacing/dry-wet interactions into the separate
[P17 item](#p17-composite-mode-integration) on 2026-11-05. See
[composite evidence](evidence/composites.md#final-acceptance-and-closure) for the
completed direct-track scope, green final implementation checks and validation pins.
Core composite file support also extends the P01/P02 capture/load paths; their
remaining dependencies and OPEN status are unchanged.

## What is already available

The service supports direct audio/MIDI/trigger tracks, primitive loop creation and
removal, record/play/stop and scheduled transitions, names/order, musical selection
and targeting, supported global policies, gain/balance/monitoring/mute, ordinary
bus controls/lifecycle/order, host connections and mixer routes, piano MIDI injection, and bounded
waveform/MIDI reads. UI and Lua use the same confirmed service projection. Socket
UI recovery retains these states and admitted operations. These are the regression
floor for parity work, not a claim that all old behavior has been ported.
Audio/MIDI click generation and insertion, client-local System preview and RT-host
Master preview are also available.
Host-advertised built-in FunDSP FX, OxiSynth and External dry/wet tracks now include
role-aware media, dry-through-wet playback, wet rerecord, editors/MIDI Learn,
recorded settings and per-track UI/Lua defaults. The
[hosted processor plan](../../PLAN_HOSTED_TRACK_PROCESSORS.md) is closed at the
user's direction; [closure evidence](track-processors.md#user-directed-closure-and-remaining-ci)
records passing checks and the remaining CI failures/unfinished jobs.

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
| P01 OPEN | Coherent session save and WAV/loop-audio/exact-MIDI/SMF export include dry/wet roles, built-in processor state/mappings, recorded-take settings and zero or multiple supported buses. Active/scheduled recording refuses without stopping it. Browser completion reports download handoff. | `shoopdaloop/persistence_io`, `shoop_persistence_codec`, service persistence jobs and retained reference codecs; production `production_files_roundtrip_audio_midi_and_fresh_session_identities`; immutable capture/old-owner/playing-export tests in `shoop_app` and `shoop_rt_backend`. See [persistence acceptance evidence](evidence/session-persistence.md) and [bus evidence](evidence/output-buses.md) for deployment scope and limits. | Extend capture/file conversion as Carla/bus FX (P07/P08), dependent composite modes (P17), general latency/timeline settings (P09/P10) and session scripts/resources (P16) become available. Direct-track composite persistence is complete under P03. UI recovery itself still has no disk durability. |
| P02 OPEN | Structure-only and media-bearing loads plus New Session use fenced, atomic replacement and fresh live IDs, including built-in/External dry/wet tracks and zero or multiple supported buses. Unsupported documents refuse intact; loaded loops start stopped. | Existing `shoop_session` formats/migrations/resampling; `shoop_persistence_codec/document`; service/adapter private candidates; `accepted_media_bearing_load_survives_ui_loss_and_reconciles_without_replay`, production save/load/save tests and [bus evidence](evidence/output-buses.md). | Restore each deferred document feature under its own row. Keep whole-document capability diagnostics, sample-rate consent, stable source/live mappings, missing-port intent and transactional refusal as coverage expands. |
| P03 COMPLETE | **Direct-track composite loops.** Regular and explicit Play/Stop/Record programs, nested UI/Lua authoring, live edits, automatic monitoring, files and accepted-work recovery are supported. Lua VMs remain client-local. | Frame resolver, prepared RT plans, service-owned documents and production editor; [execution contract](composites.md), [criterion/test map and final acceptance](evidence/composites.md#final-acceptance-and-closure), [completed plan](../../PLAN_P03_COMPOSITES.md). Retained reference fixtures remain historical evidence. | Complete under the user-approved direct-track scope. Dependent replacing/dry-wet modes and their interaction validation are preserved separately in [P17](#p17-composite-mode-integration). |
| P04 OPEN | Grab and grab-with-target policies are refused. | `shoop_app_api` loop/global actions; `shoop_egui/{loop_widget,global_controls}`; reference `grab_targets_inner`; tests `grab_policy_covers_targeted_selection_solo_and_immediate_completion`, `production_keyboard_default_action_covers_stopped_cancel_and_grab_policy`. | Bounded grab/ringbuffer integration with authoritative target/selection resolution, sync policy, outcome reporting and media continuity. |
| P05 OPEN | Replacing mode is unavailable. | `BackendCapability::ReplacingMode`; loop mode/actions/UI; reference `expanded_loop_actions_route_modes_grab_and_balance` and engine/loop tests. | Service/protocol admission and adapter/RT replacement semantics; cover selected versus explicit loop control, stop, sync and recorded content. |
| P06 COMPLETE | Host-advertised dry/wet tracks, role-aware media, dry-through-wet playback, wet rerecord and per-track UI/Lua defaults are restored. Closed at the user's direction with explicit CI follow-up. | Track topology/port-role contracts; `shoop_egui/app_widget` add-track forms and details; reference tests `application_adds_external_dry_wet_tracks_from_processor_capabilities`, `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`, `dry_wet_media_io_maps_and_exports_role_order_without_flattening`. | Complete under the [user-directed closure](track-processors.md#user-directed-closure-and-remaining-ci). The [closed plan](../../PLAN_HOSTED_TRACK_PROCESSORS.md) and evidence retain passing checks, known independent-fixture build failures and unfinished platform CI. Dependent composite modes remain P17. |
| P07 OPEN | Built-in FunDSP FX, OxiSynth and External dry/wet processing run in the host. Build/startup optionality, embedded editors, MIDI Learn, track global-control MIDI, role media and state restoration are implemented. Carla remains unavailable. | `shoop_egui/{builtin_fx_editor,oxisynth_editor,track_controls}`; processor catalog/state contracts; reusable DSP in `shoop_dsp` and prepared backends in `shoop_host_processors`; retained Carla code in `shoop_engine`, `shoop_plugin_protocol` and `third_party/carla`; reference tests `builtin_fx_session_round_trip_preserves_n_channels_controls_and_assignments`, `oxisynth_session_round_trip_preserves_selected_preset`, `processed_track_session_round_trip_preserves_roles_state_and_recorded_take`, `failed_recorded_fx_restore_leaves_the_processed_track_usable`. | Restore native Carla plugins and their processor lifecycle, service-owned controls/editors, persistence and packaging. Built-in native/browser restoration is closed under [the hosted processor plan](../../PLAN_HOSTED_TRACK_PROCESSORS.md), with [qualified CI evidence](track-processors.md#user-directed-closure-and-remaining-ci); bus FX remains P08. |
| P08 OPEN | Bus FX and global-control MIDI destinations on buses are unavailable. Track global FX-control routing is supported under P07. | `shoop_egui/bus_controls`, FX dialogs/contracts; reference bus FX handlers and `global_fx_port_round_trips_legacy_migrates_and_malformed_load_is_transactional`; independent processor tests. | Bounded bus processor graph/state, MIDI routing and confirmation across service/RT and persistence; avoid duplicate control traffic through combined track/bus routes. |
| P09 OPEN | User-configured latency compensation is refused. | `shoop_latency`; latency document/API fields and details UI; reference `latency_intents_update_future_operations_without_moving_the_active_take`. | Define service/engine application boundaries for track/processor latency, pending future operations and active takes, with protocol projection and round-trip persistence. |
| P10 OPEN | Recording offsets, take alignment and processor-latency adjustment controls are refused. | Take/offset contracts and UI; `shoop_latency`; same reference latency test plus engine recording tests. | Restore explicit/automatic adjustment semantics and future-take policy without moving an active take; cover audio and MIDI frames and save/load. |
| P11 OPEN | Core WAV/loop-audio/exact-MIDI/SMF import and replacement are enabled, including channel mapping, rate consent and optional length updates. Audio/MIDI click generation and insertion are enabled through the same bounded import path. Duplication remains refused. Vendor/system initial or preroll MIDI and untracked parameter history remain refused; limits are explicit in the persistence contract. | Production file provider/portable codecs, bounded service staging and atomic RT content apply; opposite-media/identity/control preservation, logical-window exports and cancellation tests. `LoopDuplication` and `ClickTrackInsertion` now have separate flags. Production `generated_clicks_commit_and_export_through_the_public_route` covers generated audio and MIDI content, commit and export; `shoop_app_client/src/runtime.rs` enables `ClickTrackInsertion` when the file provider and content import are available. Reference duplication UI and tests remain intact. | Restore content duplication and complete MIDI state/preroll/seek handling without dropping packets or parameter history. System/Master click preview is complete under P15; dry/wet role imports and exports are implemented under P06/P07. |
| P12 COMPLETE | Length, start-offset, per-channel timeline and real pre-play edits are supported for primitive audio/MIDI, mixed and timing-only loops. Length edits refuse active/all-armed recording; offsets and pre-play preserve capture clocks. Logical periods retain Q alignment. | Production client/service/RT commands, retained raw windows, editor/reference fixtures, all-Q output, persistence/recovery, native/browser acceptance and [evidence](evidence/loop-timing.md); [completed plan](../../PLAN_P12.md). | Complete within the fixed-quantum precision contract. Raw offsets/media/MIDI remain frame precise; unrelated parity gates retain their scope. |
| P13 COMPLETE | Ordinary buses, lifecycle/order, controls/meters, explicit routing and zero-bus save/load are supported within negotiated replacement limits (default eight buses, eight channels per bus). Master is an ordinary removable default. | Production engine/adapter/service/native/browser tests, selected Miri and retained reference fixtures; [acceptance evidence](evidence/output-buses.md), [completed plan](../../PLAN_OUTPUT_BUSES.md), [green platform matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542902). | Complete within the user-approved capacity qualification. Bus FX remains unavailable under P08; legacy 64-channel capacity is outside this restoration. |
| P14 OPEN | Live audio-driver switching is refused. Saved preferences apply on the next host start. Native JACK audio/MIDI supports dynamic track/bus publication, live discovery/connections and session restoration over native session carriers; CPAL remains separate. See [dynamic port graphs](port-graph.md). See [native JACK evidence](evidence/native-jack.md). | Audio profiles and confirmation/settings UI; independent driver/device code; reference tests `audio_driver_switch_requires_confirmation_and_resamples_transactionally`, `failed_audio_driver_switch_restores_prior_runtime_and_reports_failure`, `recording_blocks_confirmed_audio_driver_switch_without_stopping_recording`, `persistence_failure_keeps_new_driver_active_and_enables_save_retry`. | Host-managed device/engine transition with service-owned capture/restore, preflight/confirmation, resampling and rollback. Keep preference persistence and its retry separate from actual driver-switch success. Track native driver/backend equivalence explicitly. |
| P15 COMPLETE | System/Master audio click preview is supported. System uses client-local native/Web Audio output; Master uses bounded service staging and transient RT bus playback, including split hosts. Master identity survives rename, reorder and save/load; Stop, switching and recovery preserve loop media and transport. | `shoopdaloop/persistence_io/preview`, native/browser players and production application mailbox; service/adapter/engine preview handlers. [Preview contract and completion evidence](preview.md) records output, cancellation, resource, wire-transport and persistence tests, Linux System/CPAL/JACK measurements and hosted Chromium acceptance. | Complete under the user-approved scope and existing evidence. No additional physical-platform or multi-machine testing is required for closure. Bus FX stays P08; audio/MIDI generation and insertion are already supported under P11. |
| P16 OPEN | Session/filesystem script ownership conversion and related session resource workflows are unavailable. Script source export, fresh scripts, local dialogs, timers, controller MIDI and explicit starts are available. | `shoop_scripting`, `shoop_script_resources`, `shoop_egui/script_dialogs`; reference tests `scripts_export_exact_source_and_convert_session_ownership`, `filesystem_script_conversion_captures_resources_off_actor_and_survives_source_removal`, `session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts`. | Restore client-local file/resource workflows and explicit session-owned script data through service commands, without moving VM execution to the host. Respect the recovery no-auto-restart rule. |
| P17 OPEN | Composite replacing and dry/wet script-event modes/topologies remain unavailable. The completed P03 core supports direct-track Play/Stop/Record events; dependent modes refuse atomically, including whole-document load. | P03's [execution contract](composites.md), service-owned composite documents, reference script-event/default-action/track-role behavior and [surface audit](evidence/composites.md#reference-surface-and-dependency-integration). Former P03 Stage 9, moved here with user approval on 2026-11-05. | Integrate P05 replacing (#940) and P06/P07 dry/wet/processor support (#946) through compilation, service/protocol/engine mappings, admission, monitoring and capture. Complete the [interaction validation below](#p17-composite-mode-integration) before closing this item. |

## P17 composite mode integration

This is the dependent-mode work formerly tracked by Stage 9 of
[PLAN_P03_COMPOSITES.md](../../PLAN_P03_COMPOSITES.md). It is a separate OPEN parity
item under the user-approved scope split on 2026-11-05. P03 is complete for its
direct-track core; P17 depends on replacing (P05, #940), dry/wet modes and tracks
(P06), and track processors/topology (P07, #946).

- [ ] Integrate replacing and dry/wet script-event modes with those production
  capabilities. Extend loop/RT mode representation, compiler and protocol mappings,
  service admission, default-action/role mapping, automatic monitoring and capture.
  Preserve the difference between explicit event modes and dynamic default actions;
  never translate an unsupported mode into ordinary playback or recording.
- [ ] Validate nested explicit/regular combinations, mixed selections, event-mode
  edits, capture/monitoring, child independence, persistence and recovery for every
  restored dependent mode and its original platform scope. Include exact media and
  role-order round trips, unsupported whole-document refusal and the existing
  frame-clock/frozen-timing contract.

Verification uses focused local composite interaction cases and CI's dependent
feature/platform suites. Keep per-mode capability gates and atomic refusal until
support exists end to end. Restoring an individual mode flag or encoding its tag
does not establish that the composite interaction is executable.

The P04/P05 storage foundations have library evidence for both
[shared audio](../../src/rust/rt_runtime/docs/reference/shared-audio.md) and
[shared MIDI](../../src/rust/rt_runtime/docs/reference/shared-midi.md), including
immutable overlapping grabs, independent edits, ordinary loop-resource playback
and persistence round trips. MIDI replacement prepares a complete private state
index before an explicit publication cut. The [fixed integration contract](../../src/rust/shoop_loops/docs/shared-history.md) covers completed input watermarks, bounded private preparation and ordinary media publication; final-head validation is tracked in [PLAN_RECONCILE_GRAB.md](../../PLAN_RECONCILE_GRAB.md). These foundations do not enable the
application grab policies or Replacing mode; both rows remain open.

## Preserved evidence and retirement record

- The temporary fixed JACK startup bank is superseded by dynamic session-owned
  track/audio-bus publication. CLI, settings, profile fields and host setup messages
  for that bank are removed. Profiles containing the obsolete fields are rejected;
  no migration is provided for this development branch. Native signal, graph,
  lifecycle, capacity and session-restoration tests exercise dynamic publication.

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
  preview and broad dummy UI scenarios as migration evidence. Production preview
  coverage is restored under P15; driver switching and other deferred features
  retain their own rows. Runnable model-level counterparts remain in
  `shoop_app_reference`.
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
  canaries are superseded by sole-backend, independent-host recovery and generation-fenced
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
support. Client-only preview is complete under P15; the remaining client-only gap
is P16. The coverage check fails when a capability is added or removed without
reconciling this table.

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
| `ClickTrackInsertion` | AVAILABLE |
| `LoopTimingEdits` | AVAILABLE |
| `BusManagement` | P13 |
| `AudioDriverSwitching` | P14 |
| `MidiInjection` | AVAILABLE |
