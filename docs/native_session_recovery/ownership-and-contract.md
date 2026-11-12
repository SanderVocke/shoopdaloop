# Session authority inventory and delivery contract

Implementation baseline: `fe65cc2d`, based on `e369fc25` from PR #897.
On 2026-11-01 the working tree was clean on `plan/native-session-recovery`;
#897 remained open against `feat/loop-execution-layer`. The implementation stays
on this branch; no rebase or merge of unrelated `origin/master` changes is intended.
This inventory describes the extraction target, not implemented recovery behavior.
Acceptance criteria remain those in `PLAN_NATIVE_SESSION_RECOVERY.md`.

P16 ownership clarification agreed on 2026-11-12: script source, resources,
session-inclusion metadata and execution are client-owned. The client assembles
them with the service's musical capture in a session file, and separates them
before staging a musical load. The service owns neither a script catalog nor
script-resource storage. Reattachment does not transfer scripts from the previous
client; their unsaved recovery is outside backend recovery. See the
[P16 contract](p16-client-scripts.md) for confirmed-load reconciliation and the
explicit-start rule. This is a contract change; P16 implementation remains open.

The output-bus restoration follows this ownership split: the service retains bus,
channel and port identities, logical order, display order and admitted results
across UI loss. The adapter owns generation-safe engine mappings and confirmed
controls/routes; the RT engine owns prepared bus slots. Session capture stores
logical bus order separately from presentation; replacement gives all buses fresh
live IDs. Zero active buses does not trigger default-bus bootstrap on attachment.

## ApplicationModel field ownership

Every field at the implementation baseline is covered below, with its service or
client owner. For service-owned fields, an allocated view is only a cache of
service state. Backend telemetry is engine-owned, while mappings and the last
confirmed projection live in the service. Client script state remains client-owned.

| Owner | Fields | Extraction rule |
| --- | --- | --- |
| Service identity/structure | `revision`, `next_track_id`, `next_bus_id`, `next_bus_channel_id`, `next_loop_id`, `next_port_id`, `tracks`, `buses`, `bus_order`, `loops`, `connection_ports` | Keep mappings, names, order, sync identity and generations for the complete host lifetime. |
| Service connection intent | `preferred_playback_ports`, `confirmed_connections`, `pending_connections`, `confirmed_mixer_routes`, `pending_mixer_routes`, `mixer_route_errors`, `connection_errors`, `connection_revision`, `backend_connection_revision`, `backend_mixer_revision`, `connection_backend_available` | Reconcile engine observations independently of subscriber presence. |
| Service pending controls | `desired_bus_controls`, `desired_bus_fx_controls`, `desired_track_controls`, `desired_track_default_playback_modes`, `desired_fx_controls`, `desired_loop_controls` | Admission reserves tracking before submitting; detach must not clear it. |
| Service policies/interaction | `master_auto_connect`, `master_bootstrap_pending`, `master_bootstrap_after_load`, `auto_arm_owned_tracks`, `active_piano_notes`, `global` | Musical selection and target are service state in the default interaction context. Explicit detach releases client-held piano notes; a client crash does not reset recorded MIDI. |
| Service publication caches | `bus_view`, `bus_processors`, `connection_view`, `track_processors`, `track_creation_results`, `bus_creation_results` | Publish confirmed state and bounded results; never create a second recovery authority. |
| Engine observations / service projection | `host_ports`, `status`, `audio_drivers`, `capabilities`, `last_callback_budget_overruns` | Device state, frames, media and actual execution stay engine-owned; service publishes fresh observations. |
| Service diagnostics | `notifications`, `notification_view`, `next_notification_id`, `active_periodic_failures` | Bound retention and exclude Lua-local diagnostics from shared publication. |
| Service admitted I/O | `next_task_id`, `next_audio_switch_id`, `pending_audio_switch`, `io_task`, `pending_io`, `session_encoding`, `background_session_encoding` | Keep admitted engine work until its result, even if delivery fails. Client-owned bulk reads may be canceled; unsupported capture/load/switch operations must continue to refuse explicitly. |
| UI-local preview/file workflow | `click_track`, `next_preview_request_id`, `file_outputs`, `preview_outputs` | Dialog drafts, local audio preview and save destinations remain client-owned; engine preview playback is an explicit service command where supported. |
| Lua-local | `scripting_view`, `script_manager`, `script_last_snapshot`, `next_script_conversion_request`, `pending_script_conversions` | VM, files, timers, MIDI controller registrations, dialogs and diagnostics move to the client. No automatic installation on reattachment. |
| Reference-only elapsed composite scheduler | `script_composition_playback`, `script_composition_frame_remainder` | Retain historical reference evidence. Production uses immutable prepared plans and the render frame clock; accepted documents and operations are service-owned. Lua VMs remain client-local. See the [composite contract](composites.md). |

`TrackModel`/`LoopModel` hold backend IDs, names, ordering, selection, targets,
controls and structural state: service-owned. Their actual mode, position, media,
routing execution, scheduled transitions and processor observations are engine-owned.
Waveform/MIDI detail data is a bounded, cancelable read, not attachment structure.
`ScriptManager` fields (`next_id`, `scripts`, `dialog_ids`, `control`, `midi`,
`midi_endpoints`, `midi_refresh_remaining`, `midi_initialized`) all remain Lua-local.
The control bridge changes to ordered client operations plus a confirmed projection.

All `AppWidget` fields are UI-local: widget instances, bottom-pane/open state,
add-track/bus drafts and errors, request counters, pending/default settings drafts,
logo textures, channel mappings/selections, key state, ephemeral script queue,
confirmation state, tracing UI/status, callback-display timing and test rectangles.
A widget's loop highlight is local only if it is an editor highlight; musical
selection used by playback/record targeting must be a service operation.
Native/browser runtime settings stores, file dialogs, preview players, downloads,
script resource loaders, browser permission/device glue and egui state stay local.

## Representative handler traces

| Operation | Baseline route | Target owner / publication |
| --- | --- | --- |
| Rename / reorder | UI `AppIntent::Track/Loop` -> `handle_intent` -> model mutation | Service mutates canonical ordered names and increments structural revision. |
| Select / target / play | `LoopAction::IconClicked` and loop actions, Lua `ControlOperation` -> model selection/action helpers | Same service command queue; resolve selected/targeted IDs there, never from a stale client list. |
| Create / remove | `AddTrack*`, `AddLoop`, `TrackAction::Remove` -> backend request and model mapping | Service reserves operation/mapping capacity before engine admission; final state comes from outcome reconciliation. |
| Routing | `SetPortConnected`, `SetMixerRouteConnected` -> pending connection maps -> backend -> `apply_backend_snapshot` | Service retains desired/confirmed state and timeout/refusal result across UI loss. |
| Lua | `advance_scripting` -> `take_control_operations` -> `apply_control_operation` | Client VM reads confirmed projection and enqueues ordered commands. After write, callbacks observe changes only on a later confirmed client pump. |
| Media read | selected detail refresh / `PendingIo` -> async backend reads | Service schedules bounded chunks; attachment never waits for recording to stop; detach reclaims read buffers. |

## Startup and production legacy inventory

`ApplicationModel::initialize` unconditionally creates the sync track and waits for
backend idle. It must run once during fresh service creation, never on attach.
`try_bootstrap_master` and its pending flags own master autoconnection. Persist them
in the service. Runtime startup calls `install_startup_scripts`; move these to the
fresh client path only. Saved-session loads and script catalog reconciliation are
client workflows and must never be triggered by a recovered attachment.
`RtBackend::apply_resync` removes unknown strips: do not construct a new adapter
on UI attachment. Preserve the existing adapter, mirror, pending requests and
engine connection in the service host. Host faults remain faults.

The current legacy production dependency is `shoop_backend -> shoop_engine`,
including conversion helpers, native driver composition, FX catalogs/encoding and
MIDI canonicalization. `shoopdaloop` enables backend native-driver/FX features,
selects `AudioBackendKind::Legacy`, and owns external-engine children. Browser
legacy paths use `browser_audio`, `browser_worker`, `audio_worker.js`,
`audio_worklet.js`, `shoop_worklet_client`, `shoop_audio_protocol` and
`shoop_audio_worklet`. Audit feature unification and packaging, not just imports.
Retain the new worklet (`rt_audio_worklet.js`, `shoop_rt_host_worklet`) and device
bridges. Extract reusable MIDI/type helpers before isolating legacy implementations.
Useful existing evidence: `shoop_app/tests/rt_backend.rs`, adapter integration
contracts, transport socket/shared-memory tests, process tests, worklet heap guards,
`engine_backend` application workflows, Node contracts and browser smoke harnesses.

## Protocol, attachment and local delivery

Use separate newtypes for host, session, client, connection generation, operation,
snapshot transfer and structural revision. A per-attachment authority includes
session/client/generation. Authenticate before leasing. A busy healthy lease is
refused without eviction. A new generation fences every mutation and shutdown;
a late disconnect may only release its own generation. Never derive identity from
PID or an operation sequence number. Version mismatch refuses before state transfer.

A command carries authority, monotonic client operation identity, and an optional
expected structural revision. The service reserves bounded pending/result capacity
before returning Accepted. Accepted means retained session-owned work, not engine
confirmation. Confirmed/Refused is terminal and published independently of the
originating connection. Duplicate submission is refused or returns the retained
result; it never resubmits work. Lost replies are reconciled by snapshot/results,
not automatic replay. A client cannot mutate until its projection is ready.

Capture a structural revision R and its immutable structural snapshot. Publish a
bounded begin/chunk/end transfer; queue subsequent semantic changes after R in
order. If buffering cannot keep up, abort the attachment and require resync without
affecting the service. Do not silently omit a delta. Telemetry may coalesce; semantic
operations/results may not. A final snapshot boundary includes pending operations.
Fresh telemetry follows; media is separate bounded bulk. Limit encoded records,
chunk count/bytes, structural snapshot bytes, queues, in-flight commands and retained
results. Reclaim snapshot/bulk resources on detach, while retaining accepted work.

Typed local queues use the same messages and state machine without a codec round
trip. Pump a fixed number of messages per turn, with no recursive callbacks or
synchronous waits. Browser app client and service remain on the main thread;
MessagePort carries only engine protocol to the AudioWorklet.

Generic record buffers/framing/carriers belong in `shoop_transport*`; engine
credit/connection logic stays engine-specific. Both app and engine adapters reuse
carriers. App protocol must not depend on concrete transport, engine, filesystem,
UI, devices or Lua runtime. Native endpoint discovery and optional profile handling sit outside it.

## Roles, settings and exit migration

Host-only accepts optional listen endpoints and device/pool configuration. UI-only
accepts a live host address or an advanced connection profile. Combined launch
exchanges bounded configuration/readiness messages over pipes and then leaves
independent host/UI siblings. Manager and sibling UI launches pass the endpoint
through a bounded stdin message, using fixed role flags on the UI command line.
Rollback applies only to the launcher's own unready
host. No original pipe or child handle is required by later status/shutdown.

The manager discovers standalone hosts through live OS process and owned-socket
inspection. Compatibility inspection occurs before attachment admission and cannot
consume a control lease or shared-memory attachment. It keeps no registry, fixed
port range, discovery files or persisted host list. Advanced profiles are explicit
endpoint imports/exports; normal operations create none. Native connections are
keyless; compatibility and generation-fenced ownership remain mandatory.

Replace the backend selector with the sole RT engine and diagnose migrated legacy
settings. App transport settings configure UI/service IPC; engine/device settings
configure the host's internal RT connection. Preserve local/shared-memory/socket
and browser modes, but advertise reattachment only for socket mode.

External-host close/window/platform quit offers UI only, both, or Cancel. UI only
releases the current lease and exits; destructors never stop the host. Close both
uses current fenced authority (also over a separate management path
if data delivery is lost), waits for MIDI reset/render stop/cleanup, and reports
failure with retry/UI-only options. Local/browser exit honestly terminates its
co-located host. Explicit local Force stop requires confirmation and revalidates the OS process identity; it performs no orderly session cleanup. There is no automatic force-kill or replacement host.

## Verification and acceptance mapping

Stage commands are run with the repository's stable toolchain. Focused package
Cargo tests are acceptable when nextest is not provisioned; final native gates must
use nextest 0.9.116. Use `RUSTFLAGS='-D warnings' cargo build -p <changed packages>`,
`cargo fmt --all`, `python3 scripts/check_shoop_test_usage.py`, and changed portable
crate builds for `wasm32-unknown-unknown`. Node uses
`python3 scripts/run_wasm_tests.py --runtime node --package <package>`.

| Criterion | Required automated evidence |
| --- | --- |
| A1 | Production dependency/artifact audits, all entry-point builds, UI/Lua command routing tests, unsupported capability tests. |
| A2 | Ownership field inventory above; rename/order/policy/select/target survive detach tests using actual service. |
| A3 | Real packaged host-first/UI-only and sibling startup, launcher exit/kill, readiness failure/rollback across Linux/macOS/Windows. |
| A4 | Credential/version/session/busy rejection and coherent snapshot race/chunk tests. |
| A5 | Real engine audio/MIDI media/output/frame continuity through UI kill and repeated attachment, no device reopen/reset. |
| A6 | Fault injection before/after admission, submission, confirmation/publication/reply; no duplicate identities or replay. |
| A7 | Stale mutation, disconnect and management shutdown rejected after a replacement generation attaches. |
| A8 | Both menus and window/platform quit/Cancel; shutdown error and retry; MIDI reset, stop/cleanup completion. |
| A9 | Native typed contracts, Node and Chromium worklet workflow, bounded main-thread pumps, RT heap/overrun guards. |
| A10 | Malformed/truncated/oversize/version codec tests, bounded admission/chunk/result/subscriber resources, active-recording snapshot. |
| A11 | Full retained native/Node/browser gates, three-platform debug/release packages, final selected Miri and CI/review at the final SHA. |

Historical engine-transport measurements and Miri/CI records remain in Git
history. They do not validate the current application service. New evidence must record head, command,
workload/capacities, timing, hardware and limitations. Compare local typed/encoded
control, snapshot bursts and active-recording reattachment. Miri runs only after
ordinary final integration gates stabilize. No baseline full matrix was run.
