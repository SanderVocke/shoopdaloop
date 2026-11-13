# Application feature backlog

These are remaining behavior gaps in the replacement backend. Open rows are
requirements to restore features, not decisions to remove them. The legacy
reference implementation and its behavioral tests remain available until the
corresponding behavior has production coverage. Completed rows identify the
supported scope; dependent modes can remain open in other rows.

Keep this backlog aligned with production capabilities and client/service
handlers. Record validation in PRs and CI artifacts; historical links below point
to the implementation records in Git history.

## What is already available

The service supports direct audio/MIDI/trigger tracks, primitive loop creation and
removal, record/play/stop and scheduled transitions, names/order, musical selection
and targeting, supported global policies, gain/balance/monitoring/mute, ordinary
bus controls/lifecycle/order, host connections and mixer routes, piano MIDI injection, and bounded
waveform/MIDI reads. UI and Lua use the same confirmed service projection. Socket
UI recovery retains these states and admitted operations. These are the regression
floor for parity work, not a claim that all old behavior has been ported.
Audio/MIDI click generation and insertion, client-local System preview and RT-host
Master preview are also available. Grab and Replacing are available through the
normal UI, bundled keyboard and Lua routes on negotiated hosts. Firmware can
enable the same features through its explicit [engine policy](../../platforms/zynq7000/README.md);
disabled configurations advertise unavailable capabilities.
Host-advertised built-in FunDSP FX, OxiSynth and External dry/wet tracks now include
role-aware media, dry-through-wet playback, wet rerecord, editors/MIDI Learn,
recorded settings and per-track UI/Lua defaults. The
[hosted processor plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_HOSTED_TRACK_PROCESSORS.md) is closed at the
user's direction; [closure evidence](track-processors.md#delivery-limitations)
records passing checks and the remaining CI failures/unfinished jobs.

Recording uses a finite prepared host media pool, normally 256 MiB shared across
loops. Budget capacity for recording duration and channel count; exhaustion ends
capture. The [capacity diagnostic](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/recording-capacity.md) records this
existing limit and the recovery stress fixture's corresponding budget.

## Missing features

**Five feature items remain open:** Carla (P07), latency compensation (P09),
take alignment (P10), remaining duplication/MIDI state cases (P11), and session
scripts/resources (P16). Saving, loading, storing and applicable import/export
requirements belong to each feature and are part of its completion criteria.

Paths below are relative to `src/rust/`. Reference tests are searchable in
`shoop_app_reference/src/lib.rs` unless another location is given. Reference code
is behavioral evidence, not a production dependency. Check existing engine
primitives before introducing new ones.

| ID / status | Feature and present production limit | Preserved behavior, UI and code | Work needed to close the gap |
| --- | --- | --- | --- |
| P07 OPEN | Built-in FunDSP FX, OxiSynth and External dry/wet processing run in the host. Build/startup optionality, embedded editors, MIDI Learn, track global-control MIDI, role media and state restoration are implemented. Carla remains unavailable. | `shoop_egui/{builtin_fx_editor,oxisynth_editor,track_controls}`; processor catalog/state contracts; reusable DSP in `shoop_dsp` and prepared backends in `shoop_host_processors`; retained Carla code in `shoop_engine`, `shoop_plugin_protocol` and `third_party/carla`; reference tests `builtin_fx_session_round_trip_preserves_n_channels_controls_and_assignments`, `oxisynth_session_round_trip_preserves_selected_preset`, `processed_track_session_round_trip_preserves_roles_state_and_recorded_take`, `failed_recorded_fx_restore_leaves_the_processed_track_usable`. | Restore native Carla plugins and their processor lifecycle, service-owned controls/editors, persistence and packaging. Built-in native/browser restoration is closed under [the hosted processor plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_HOSTED_TRACK_PROCESSORS.md), with [qualified CI evidence](track-processors.md#delivery-limitations); Built-in bus FX is complete under P08. **Saving/loading/storing:** capture plugin identity, settings, MIDI mappings, recorded-take processor state and packaged resources; restore privately before atomic activation, and refuse missing/unavailable plugins without altering the session. Preserve role-aware audio/MIDI import/export. Carla bus hosting remains outside the completed Built-in P08 scope; do not imply it is supported. |
| P09 OPEN | User-configured latency compensation is refused. | `shoop_latency`; latency document/API fields and details UI; reference `latency_intents_update_future_operations_without_moving_the_active_take`. | Define service/engine application boundaries for track/processor latency, pending future operations and active takes, with protocol projection and round-trip persistence. **Saving/loading/storing:** persist configured compensation and its track/processor associations; validate and convert frame-based values on rate changes, and round-trip future-operation policy without shifting an active take. Preserve the resulting timing in session capture and media export. |
| P10 OPEN | Recording offsets, take alignment and processor-latency adjustment controls are refused. | Take/offset contracts and UI; `shoop_latency`; same reference latency test plus engine recording tests. | Restore explicit/automatic adjustment semantics and future-take policy without moving an active take; cover audio and MIDI frames and save/load. **Saving/loading/storing:** retain manual/automatic adjustment policy, take offsets and audio/MIDI alignment in session archives and exact-media workflows; restore and resample coherently without moving an active take. P12 timeline edits are already supported and are not part of this gap. |
| P11 OPEN | Core WAV/loop-audio/exact-MIDI/SMF import and replacement are enabled, including channel mapping, rate consent and optional length updates. Audio/MIDI click generation and insertion are enabled through the same bounded import path. Primitive and P03 composite copying use a host-owned capture/Stop/apply/control sequence, independent of file providers. Occupied cross-kind conversion and P17 definition duplication remain refused. Vendor/system initial or preroll MIDI and untracked parameter history remain refused; limits are explicit in the persistence contract. | Production file provider/portable codecs, bounded service staging and atomic RT content apply; opposite-media/identity/control preservation, logical-window exports and cancellation tests. `LoopDuplication` and `ClickTrackInsertion` now have separate flags. Production `generated_clicks_commit_and_export_through_the_public_route` covers generated audio and MIDI content, commit and export; `shoop_app_client/src/runtime.rs` enables `ClickTrackInsertion` when the file provider and content import are available. Reference duplication UI and tests remain intact. | Complete occupied cross-kind/P17 duplication and MIDI state/preroll/seek handling without dropping packets or parameter history. System/Master click preview is complete under P15; dry/wet role imports and exports are implemented under P06/P07. **Saving/loading/storing:** preserve complete MIDI packet order, initial/preroll/seek state and parameter history through capture, import, export and session round trips. Keep exact-format guarantees distinct from reported SMF loss. Extend copying to occupied cross-kind destinations and P17 definitions with fresh definition identities and preserved mode/role metadata; P17 session save/load already works. |
| P16 OPEN | Client-owned include-in-session/conversion and script resource save/load/store workflows are implemented and locally qualified, with stopped loads and exact source export. Final CI and review are in progress; see the [acceptance evidence](evidence/p16-client-scripts.md). | `shoop_scripting`, `shoop_script_resources`, `shoop_egui/script_dialogs`; reference tests `scripts_export_exact_source_and_convert_session_ownership`, `filesystem_script_conversion_captures_resources_off_actor_and_survives_source_removal`, `session_scripts_stage_before_commit_round_trip_and_preserve_machine_scripts`. | Restore [client-owned session scripts/resources](p16-client-scripts.md), following the [implementation plan](../../PLAN_P16_CLIENT_SCRIPT_PERSISTENCE.md). **Saving/loading/storing:** the client combines a confirmed musical capture with its script source, ownership/preferences and embedded resources; on load it validates and retains the script section locally and submits only musical data to the service. Prepare all local replacements before commit, install them stopped after confirmed success, and retain machine-owned scripts. Preserve exact source export and conversion even after source removal. No backend script catalog/resource storage, script transfer on attachment, VM serialization or automatic start after load/recovery. A replacement client recovers music, not the previous client's scripts. |

## Supported features and completion evidence

Session save/store, structure-only/media-bearing load, New Session and
WAV/loop-audio/exact-MIDI/SMF export are implemented for the supported content,
within the [persistence contract](persistence-contract.md)'s capacity and MIDI
limits. This includes built-in track/bus processors, dry/wet roles, recorded-take
settings, timing edits and P03/P17 composites. Unsupported documents refuse as a
whole. Active/scheduled recording or Replacing blocks capture; loads use fresh
runtime identities and stopped loops. Transient transport, input histories and
Lua VM state are not stored. Browser download completion means download handoff.

Historical P01 (save/export) and P02 (load) are retired as standalone backlog
items: their remaining requirements are assigned to P07/P09/P10/P11/P16 above.
This is a tracking reorganization, not a claim that deferred content is supported.
Existing coverage includes `production_files_roundtrip_audio_midi_and_fresh_session_identities`,
`accepted_media_bearing_load_survives_ui_loss_and_reconciles_without_replay`,
immutable capture/playing-export tests and the
[original persistence acceptance](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/session-persistence.md).
Feature-specific evidence and completion qualifications follow.

| ID / status | Feature and present production limit | Preserved behavior, UI and code | Work needed to close the gap |
| --- | --- | --- | --- |
| P03 COMPLETE | **Direct-track composite loops.** Regular and explicit Play/Stop/Record programs, nested UI/Lua authoring, live edits, automatic monitoring, files and accepted-work recovery are supported. Lua VMs remain client-local. | Frame resolver, prepared RT plans, service-owned documents and production editor; [execution contract](composites.md), [criterion/test map and final acceptance](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/composites.md#final-acceptance-and-closure), [completed plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_P03_COMPOSITES.md). Retained reference fixtures remain historical evidence. | Complete under the user-approved direct-track scope. Dependent replacing/dry-wet modes and their interaction validation are complete under [P17](#p17-composite-mode-integration). |
| P04 COMPLETE | Grab supports targeted musical windows, selected gestures, exact-list Control/Lua and unsynced prefix continuation with render-owned completion. Availability follows negotiated configuration on native, browser and firmware hosts. | `shoop_app/src/service/commands/input.rs`, `shoop_rt_backend/src/input.rs`, `shoop_rt_engine/src/input.rs`, `shoop_loops/src/runtime/input_batch.rs`; normal loop controls and bundled keyboard/Lua; public exact-media/cancellation/recovery tests and packaged native/browser/firmware workflows. [Acceptance and final CI](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md), [behavior contract](grab-replace.md), [completed plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_FINISH_GRAB_REPLACE.md). | Complete within the primitive-track scope. Explicit scripts use exactly their supplied lists; raw adoption retains explicit padding. Histories have separate finite reservations and truthful density/warmup/capacity limits. |
| P05 COMPLETE | Primitive Replacing supports immediate/scheduled UI/Lua transitions, continuous paired audio/MIDI overwrite, phase and sync waits, complete immutable cuts and truthful incomplete-capture outcomes. | Normal “Replace input” menu; service/protocol/adapter input ownership; `shoop_loops/src/runtime/shared/writer.rs`, semantic MIDI indexes and COW audio; exact stereo/SysEx/erasure/state oracles, synchronized two-writer regression, long-recording/prefix entry, save/load/re-edit, pressure and teardown. [Application acceptance, budgets and final CI](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md). | Complete within the primitive-track scope and measured finite capacities. Composite authored Replacing and FX/dry-wet input editing are integrated under P17. |
| P06 COMPLETE | Host-advertised dry/wet tracks, role-aware media, dry-through-wet playback, wet rerecord and per-track UI/Lua defaults are restored. Closed at the user's direction with explicit CI follow-up. | Track topology/port-role contracts; `shoop_egui/app_widget` add-track forms and details; reference tests `application_adds_external_dry_wet_tracks_from_processor_capabilities`, `dry_wet_track_default_action_is_dynamic_while_explicit_play_stays_explicit`, `dry_wet_media_io_maps_and_exports_role_order_without_flattening`. | Complete under the [user-directed closure](track-processors.md#delivery-limitations). The [original closed plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_HOSTED_TRACK_PROCESSORS.md) retains its qualified delivery evidence. Current processor/composite interaction and platform acceptance is recorded under P17. |
| P08 COMPLETE | Built-in bus FX and global-control MIDI destinations on buses are supported within negotiated FX widths and shared track/bus instance limits. Carla is explicitly outside the user-approved scope. | Service/adapter/RT graph and generation-fenced state; public bus controls and MIDI Learn; atomic persistence/recovery; packaged native typed/socket/shared-memory workflows and actual browser AudioWorklet checks. See the [bus FX contract and verification commands](bus-fx.md) and [implementation plan](../../PLAN_BUSFX.md). | Complete within the user-approved Built-in FX scope, independently of P07 or other feature integrations. [PR #954](https://github.com/SanderVocke/shoopdaloop/pull/954) records CI evidence and review disposition. Any future Carla bus integration is separate work. |
| P12 COMPLETE | Length, start-offset, per-channel timeline and real pre-play edits are supported for primitive audio/MIDI, mixed and timing-only loops. Length edits refuse active/all-armed recording; offsets and pre-play preserve capture clocks. Logical periods retain Q alignment. | Production client/service/RT commands, retained raw windows, editor/reference fixtures, all-Q output, persistence/recovery, native/browser acceptance and [evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/loop-timing.md); [completed plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_P12.md). | Complete within the fixed-quantum precision contract. Raw offsets/media/MIDI remain frame precise; unrelated parity gates retain their scope. |
| P13 COMPLETE | Ordinary buses, lifecycle/order, controls/meters, explicit routing and zero-bus save/load are supported within negotiated replacement limits (default eight buses, eight channels per bus). Master is an ordinary removable default. | Production engine/adapter/service/native/browser tests, selected Miri and retained reference fixtures; [acceptance evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/output-buses.md), [completed plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_OUTPUT_BUSES.md), [green platform matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542902). | Complete within the user-approved capacity qualification. Built-in bus FX is supported under P08; legacy 64-channel capacity is outside this restoration. |
| P14 COMPLETE | Confirmed host-managed native switching is restored for Dummy, built CPAL hosts/devices and direct JACK. The service preserves public session identities, converts media/timing, restores stopped loops and rolls back failures. Preference saving and retry remain separate. | Production service/native/controller and both-carrier tests; retained reference switch fixtures; [switching contract and acceptance map](audio-driver-switching.md); [completed implementation plan](../../PLAN_P14_AUDIO_DRIVER_SWITCHING.md). CPAL JACK and direct JACK remain distinct choices. | Complete within native configuration and capacity limits. Software/mock and virtual JACK/ALSA qualification are distinct from physical-device qualification. Browser/firmware live device switching remains outside P14. |
| P15 COMPLETE | System/Master audio click preview is supported. System uses client-local native/Web Audio output; Master uses bounded service staging and transient RT bus playback, including split hosts. Master identity survives rename, reorder and save/load; Stop, switching and recovery preserve loop media and transport. | `shoopdaloop/persistence_io/preview`, native/browser players and production application mailbox; service/adapter/engine preview handlers. [Preview contract and completion evidence](preview.md) records output, cancellation, resource, wire-transport and persistence tests, Linux System/CPAL/JACK measurements and hosted Chromium acceptance. | Complete under the user-approved scope and existing evidence. No additional physical-platform or multi-machine testing is required for closure. Built-in bus FX is supported under P08; audio/MIDI generation and insertion are already supported under P11. |
| P17 COMPLETE | Authored composite Replacing, dry playback and wet rerecord execute through UI/Lua, service, adapter, engine and files on eligible Direct, External, built-in FX and OxiSynth targets. Nested processed-track containers, automatic monitoring, prepared paired writers and immediate playback after load are supported. Unsupported modes/topologies refuse atomically. | [Execution contract](composites.md), [completed plan](../../PLAN_COMPOSITE_COMPLETION.md) and [PR #952 acceptance and final-head CI](https://github.com/SanderVocke/shoopdaloop/pull/952). Production interaction tests and actual packaged native/AudioWorklet workflows cover exact media/roles, nested edits, capacity refusal, files and accepted-work recovery. | Complete within negotiated host capabilities and finite capacities, with the user-directed web-debug CI exception recorded in the completion plan. Carla, general latency controls and expanded recovery remain separate parity items; Built-in bus FX is complete under P08. |

## P17 composite mode integration

This is the dependent-mode work formerly tracked by Stage 9 of
[PLAN_P03_COMPOSITES.md](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_P03_COMPOSITES.md). It is a separate completed parity
item under the user-approved scope split on 2026-11-05. P03 is complete for its
direct-track core; P17 depends on replacing (P05, #940), dry/wet modes and tracks
(P06), and track processors/topology (P07, #946).

The [completion plan](../../PLAN_COMPOSITE_COMPLETION.md) tracks execution.
Dry/wet integration has native production tests in `shoop_app`, `shoop_rt_backend`,
`shoop_rt_engine` and `shoop_host_processors`, including real session archives
for External, built-in FX and OxiSynth. Replacing includes autonomous writer
preparation, Record→Replace and finite-capacity refusal.
[PR #952](https://github.com/SanderVocke/shoopdaloop/pull/952) records packaged
native, hosted/self-contained AudioWorklet and final-head platform acceptance,
including explicit limits of optional checks.
The completion plan records the user's explicit waiver of the web-debug browser
acceptance step.

- [x] Integrate replacing and dry/wet script-event modes with those production
  capabilities. Extend loop/RT mode representation, compiler and protocol mappings,
  service admission, default-action/role mapping, automatic monitoring and capture.
  Preserve the difference between explicit event modes and dynamic default actions;
  never translate an unsupported mode into ordinary playback or recording.
- [x] Validate nested explicit/regular combinations, mixed selections, event-mode
  edits, capture/monitoring, child independence, persistence and recovery for every
  restored dependent mode and its original platform scope. Include exact media and
  role-order round trips, unsupported whole-document refusal and the existing
  frame-clock/frozen-timing contract.

Verification uses focused local composite interaction cases and CI's dependent
feature/platform suites. Keep per-mode capability gates and atomic refusal until
support exists end to end. Restoring an individual mode flag or encoding its tag
does not establish that the composite interaction is executable.

The P04/P05 application implementation uses the shared audio and semantic MIDI
stores through normal negotiated service/adapter input transactions. Continuous
writers preserve complete cuts without suffix repair; musical finishes belong to
the render owner. Public routes, exact media/state, global live-credit protection,
persistence/recovery and measured limits are linked in the
[application evidence](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/docs/native_session_recovery/evidence/grab-replace.md). The
[completed delivery plan](https://github.com/SanderVocke/shoopdaloop/blob/a0304ac59f842b790337b914f52f404dcbfa0ccd/PLAN_FINISH_GRAB_REPLACE.md) records final source/base,
CI, review disposition and measured limits. Older foundation evidence remains
provenance. The remaining OPEN rows retain their existing scopes.

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
and persistence and recovery interaction where relevant. Saving, loading, storing
and applicable import/export are required parts of each feature, not separate
follow-up backlog items. Name narrower partial
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
| `SessionCapture` | AVAILABLE |
| `SessionContentLoad` | AVAILABLE |
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
