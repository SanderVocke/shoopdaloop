# Unified session service, independent hosts, and UI recovery

## Status and approved scope

Implementation started on 2026-11-01; Stage 0 inventory and contract are recorded in
[ownership-and-contract.md](docs/native_session_recovery/ownership-and-contract.md).
Stages 0–4 are implemented: ownership/contract inventory, neutral
backend contracts, shared carriers, bounded app protocol/client projection, the
authoritative service, UI/Lua clients with typed local composition, and independent
native hosts with authenticated attachment. Recovery/exit UX, legacy retirement and
final validation remain incomplete in Stages 5–7.
See [incremental validation](docs/native_session_recovery/validation.md).

Current checkpoint (2026-11-01):

- Stage 2: `95c14b8b` — authoritative service and session-owned admitted work.
- Stage 3: `62afd7b6` — UI/Lua client, typed local composition and bounded media.
  Focused native/Node checks and native/browser core workflows passed; detailed
  workloads and evidence limits are recorded in the incremental validation document.
- PR [#898](https://github.com/SanderVocke/shoopdaloop/pull/898), making `rt_runtime`
  and `shoop_loops` unconditionally `no_std` with `alloc`, was merged locally as
  `4b9367f2` from `a32c9a57c79a9cd744b268e9aec3f523e05b1206`. The merge had no
  conflicts. Its native affected-crate warning-denying build, all 361 affected tests,
  formatting and Rust test-attribute policy passed. The paused Stage 4 changes were
  restored afterward; these merge checks do not validate that unfinished work.
- Stage 2/3 commits and the merge have not been pushed. The last verified green CI
  is the earlier Stage 1 head `2ec1abec`, run
  [41284581244](https://github.com/SanderVocke/shoopdaloop/actions/runs/41284581244);
  it does not validate the current head or working tree. Final-head CI, review and
  A1–A11 acceptance remain outstanding. Miri has not been run early.
- The user has resumed execution through completion, including push and final-head CI.

Originally written under
[write_plan.md](.agents/prompts/write_plan.md) on `plan/native-session-recovery`,
based on PR [#897](https://github.com/SanderVocke/shoopdaloop/pull/897) head
`e369fc257abde71a32a968aa5f922da23bed0c2a` (`feat/engine-ipc`).

This revision incorporates the user's subsequent approvals: adopt the session-service
architecture everywhere; make the new RT engine the sole production backend;
move essential shared session state and its handlers into the service; keep Lua
client-side; use the browser main thread for both app client and app service;
retain the AudioWorklet as the RT engine host; and test incrementally, with Miri last.
It replaces the earlier recovery-document/adoption design and legacy-preservation
criteria. The new criteria below are the approved milestone's fixed contract.

## Goals and product limits

One authoritative session service owns essential application state and operations.
On native IPC deployments it lives beside the engine in a surviving host process.
A new UI reconnects to that live service and receives its current state, preserving
playback, recording, identities, and admitted work. The normal external-host launcher
starts host/UI siblings; either role can also start independently with explicit
endpoint/configuration arguments. Menus offer **Close UI only (keep host running)**
and **Close UI and host**.

Use the same app model, handlers, and logical app protocol in every deployment.
Socket IPC supports reattachment initially. Retain new-engine in-process,
shared-memory, and browser/AudioWorklet deployments; shared-memory reattachment
is excluded. Native external modes use independent processes. Native in-process
mode has ordinary process-exit behavior and cannot leave a host behind.

Retire legacy production backend selection, fallback, launch paths, and obsolete
packaging. The new engine becomes the sole default backend, without taking feature
parity into scope. At the inspected head, unavailable features include complete
session capture/save and content loading, composite loops, grab, replacing/dry-wet
modes, track/bus FX, additional buses, content import, timing edits, latency/take
alignment, and live driver switching. Preserve explicit capability refusals;
UI recovery is not disk persistence. Keep useful behavioral tests/reference evidence
and independent reusable code; isolate any retained legacy fixtures from production.

Also excluded: simultaneous clients/observers, remote TCP/IP deployment/TLS, automatic
host restart, recovery after host failure, disk-backed durability, a browser Worker,
browser page/renderer-crash recovery, undo-history recovery, Lua VM recovery or
automatic script restart, moving Lua to the host/RT callback, and new latency compensation.
Normal fresh-session scripting remains available through the app client.

## Baseline findings

Paths below are relative to `src/rust/` unless otherwise stated.

- `shoop_app/src/lib.rs`: `ApplicationModel` mixes shared state/handlers with
  `ScriptManager`, file/UI workflows, and allocated snapshots. `initialize` creates
  the sync track. Extract ownership and behavior; do not move this monolith into RT.
- `shoop_app_api`: already contains application intents/state. Names, musical
  selection/targeting, ordering, and global controls are currently app-owned.
- `shoop_backend`: portable contracts currently import legacy `shoop_engine` types.
  Separate these dependencies before removing legacy production paths.
- `shoop_rt_backend`: the engine adapter/mirror. `apply_resync` removes unknown
  strips; reconnecting a fresh adapter is unsafe for live session recovery.
- `shoop_rt_host_process/src/{process,serve}.rs`: app-owned listener, child,
  inherited credential/shutdown pipes, and one permanently retired UI connection.
- `shoop_rt_transport_socket/src/setup.rs`: already uses authenticated loopback TCP.
  Recovery semantics must be portable even though remote deployment is excluded.
- `shoop_rt_engine/src/{endpoint,resync}.rs`: retains admitted music and supports
  structural resync. Engine operation outcomes and application publication remain
  asynchronous and need explicit ordering.
- `shoop_rt_host_worklet/src/host.rs`: processes records and renders on the audio
  thread with prepared storage; its tests heap-guard Rust execution after preparation.
- `shoopdaloop` and `shoop_egui`: backend selection, launcher/runtime composition,
  Lua integration, browser setup, menus, and fatal-disconnect behavior.
- `scripts/check_rt_engine_boundary.py`, workspace manifests, packaging, and CI:
  encode existing crate names, dependency boundaries, and legacy build/test assumptions.

## Architecture and naming

| Crate/component | Responsibility |
| --- | --- |
| `shoop_app` | Portable authoritative session service: state, handlers, revisions, and pending operations; no UI or Lua runtime ownership |
| `shoop_app_api` | Shared app-facing domain/API types; retain the existing crate |
| `shoop_app_protocol` | Versioned app commands, results, snapshots/updates, attachment and lifecycle messages; bounded wire codec |
| `shoop_app_client` | UI/Lua-facing client, confirmed projection, subscriptions, and local/remote delivery integration |
| `shoop_backend` | Engine-independent backend contract and shared data types, extracted from legacy coupling |
| `shoop_rt_backend` | Service-owned adapter from application operations to the RT engine protocol |
| `shoop_host` | Native composition executable/service hosting the app service and RT engine; succeeds `shoop_rt_host_process` |
| `shoop_rt_host_native` | Native RT engine execution and device hosting |
| `shoop_rt_host_worklet` | AudioWorklet RT engine execution and audio/MIDI bridge; retain this name |
| `shoop_transport*` | Extracted reusable record delivery/buffers/carriers, such as `shoop_transport_socket`; no app or engine semantics |
| `shoop_rt_protocol`, engine-specific connection code | Engine messages and engine-specific connection behavior; retain appropriate `shoop_rt_*` names |
| `shoop_session` | Existing session-file formats/persistence utilities; do not repurpose its name for the live service |

Extract shared transport machinery when it serves both protocols; do not duplicate
socket/shared-memory implementations or rename engine-specific behavior as generic.
Keep low-level hosts free of application semantics. `shoop_host` is the higher-level
composition allowed to depend on both the app service and the engine host. No new
browser host crate is needed solely to run the app service on the main thread.

```text
Native socket/shared-memory deployment:
  UI process: UI + Lua -> app client
                            |
                     app protocol over IPC
                            |
  Host process: app service -> RT backend -> engine protocol -> native RT host
                control side                                  audio thread

Browser:
  Main thread: UI + Lua -> app client -> app service -> RT backend
                         typed local app delivery         |
                                               engine protocol / MessagePort
                                                         |
  Audio thread:                                  RT worklet host
```

The native host retains the service, RT adapter/mappings, and engine connection
across UI detach. UI reconnection does not reconstruct an RT backend or rerun its
engine resync cleanup. Native in-process delivery uses the same service/client
contract locally. App protocol and engine protocol have distinct responsibilities;
transport settings/help must identify which connection they configure.

## Immutable acceptance criteria

- **A1 — universal architecture:** all production UI and Lua control goes through
  the app client and one authoritative service implementation. The new RT engine
  is the sole production backend; legacy selection/fallback and production
  dependencies are removed. Existing new-engine capability limits remain explicit.
- **A2 — session authority:** essential shared state and mutation handlers survive
  native UI loss: stable app/engine mappings, names/order, sync identity, musical
  selection/target, shared global policies, connection intent, and pending engine
  operations. UI-local presentation/edit drafts and Lua state are outside this
  authority. There is no separate shadow recovery document synchronized by the UI.
- **A3 — independent roles:** packaged host-only/UI-only commands accept explicit
  endpoint/configuration and protected credential references. The external-mode
  launcher starts siblings; after readiness neither launcher nor UI lifetime controls
  the healthy host. Later control needs no original parent pipe or child handle.
- **A4 — attach and synchronization:** a new UI can discover/select a socket host
  or use an explicit endpoint, authenticate, verify compatibility/session identity,
  acquire exclusive control, and obtain a coherent snapshot plus subsequent updates.
  Refuse unauthorized/incompatible/duplicate clients without disturbing the session.
- **A5 — continuity:** detach, UI crash, and repeated attachment preserve playback,
  audio/MIDI recording and media, routing, scheduled/admitted work, and object
  identities/generations. Recovery causes no reset, media reupload, device reopening,
  sync-track creation, or removal of unrecognized live objects.
- **A6 — operation consistency:** acknowledged shared changes survive subsequent
  UI death while the host survives. Service-accepted operations remain owned until
  completion/refusal independent of client connection. Lost replies never cause blind
  replay, duplicate objects, or unmanageable engine state. Unsent edits may be lost.
- **A7 — exclusive authority:** one controlling-client lease with connection-generation
  fencing. Stale clients cannot mutate or shut down a replacement's session. Client,
  connection, session, operation, and state-revision identities remain distinct so
  future multiple UIs or host-side Lua do not require replacing the ownership model.
- **A8 — explicit exit:** both close menu actions work for independent native hosts,
  including after recovery. UI-only close leaves music running; close-both performs
  authorized MIDI reset/render stop/cleanup and reports failure. Window/platform quit
  uses the same explicit choice and Cancel. No UI destructor/crash implies host stop.
  Same-process/browser deployments present honest local-exit behavior.
- **A9 — browser execution:** app client/service run cooperatively on the main thread
  with typed local delivery and no mandatory serialization or extra Worker. No
  synchronous waits for same-thread progress, recursive dispatch, or app-model work
  in the worklet. Preserve worklet RT/heap constraints and bounded engine messaging.
- **A10 — portable and bounded:** app session/attachment semantics do not depend on
  sockets, devices, filesystem, or UI. Bound queues, in-flight work, and snapshot/bulk
  transfer; reclaim departed-client resources. Snapshot consistency does not wait for
  active recording to end. No production engine/worklet dependency on app/Lua semantics.
- **A11 — delivery:** packaged Linux/macOS/Windows launch, recovery, and close flows
  pass; supported new-engine in-process/shared-memory/browser workflows pass. Updated
  required CI is green on the final head, applicable automated review is resolved,
  and acceptance evidence/performance limits are recorded.

Goals and A1–A11 require explicit user approval to change.

## Design rules

1. **Extract a session service, not an RT application model.** Keep its mutation
   logic together with its canonical data. Native hosting uses a non-RT control
   thread; browser hosting uses the main-thread cooperative pump. UI owns widgets,
   dialog drafts, presentation, and platform file dialogs. Inject platform services
   explicitly. The engine remains authoritative for actual musical execution.
2. **State ownership must be enumerated.** Inventory every `ApplicationModel` field
   and relevant UI/Lua field: service-owned, engine-owned, client projection,
   UI-local, or Lua-local. Include auto-arm/solo/default actions and all policies that
   affect supported musical behavior. Current musical selection/targeting belong
   to one default session interaction context; editor-local highlights stay local.
   Preserve room for identified interaction contexts without implementing multiple UIs.
3. **Commands determine behavior at the authority.** Resolve operations such as
   PlaySelected/record-with-target against ordered service state. UI predictions
   are provisional projections, never another authority. Changes carry stable
   operation IDs, expected revisions where needed, and explicit accepted/confirmed/
   refused outcomes. Do not serialize the existing monolithic `AppSnapshot` blindly.
4. **Keep pending work in the service.** Retain adapter mappings and operation
   state before submitting engine work; reject before admission if tracking capacity
   is exhausted. Resolve engine outcomes even when no UI is attached. Publish
   confirmed revisions/results independently of delivery; bound result retention.
   Distinguish session-owned work from abandoned client bulk reads. No disk journal
   or independent recovery copy is required for this host-survival milestone.
5. **Snapshots are ordinary service synchronization.** Attach with a fresh generation,
   capture a consistent structural revision, and order/buffer subsequent updates so
   none disappear between snapshot and live subscription. Report pending operations
   or reconcile their structural publication at a defined boundary; never await
   global musical idleness. Stream large snapshots/media in bounded chunks, obtain
   fresh telemetry, and enable commands only when the client projection is ready.
6. **Separate two connection lifetimes.** UI-to-service detach leaves the service's
   engine connection alive. Session initialization creates the starter/sync structure
   exactly once under service authority. Reconnecting a UI only subscribes. Engine
   faults remain faults, and host crashes remain unrecoverable in this milestone.
7. **Lua becomes a client.** Keep VM, timers, controller MIDI ports, script UI, and
   resource handling on the UI side. Reads use a coherent confirmed client view;
   writes/events use ordered app operations/subscriptions. Specify asynchronous
   read-after-write and callback timing without blocking. Preserve healthy-session
   scripting semantics within supported capabilities; recovery never automatically
   reruns scripts. Future host-side Lua uses the same actor/operation interface.
8. **Local delivery is a first-class implementation.** Deliver typed app messages
   without encoding or byte-buffer copies when local. Use scheduled queues/pumps,
   bounded work per turn, and incremental/coalesced display updates. Never coalesce
   away semantic operations. Worklet rendering does not wait for the session service.
   Measure main-thread responsiveness/control latency and audio overruns separately.
9. **Host discovery/authentication are platform adapters.** Provide a protected
   per-user registry and credential references; never log credentials or put bearer
   tokens in argv. Verify host/session identity via authenticated communication, not
   PID alone. Explicit endpoints work without discovery; multiple candidates require
   selection. Fence stale authority on every mutation/shutdown; do not evict a healthy
   owner or silently start another host after attachment failure.
10. **Launch and exit are explicit.** Host-only startup owns device/pool configuration
    and one-time service initialization. The launcher waits for bounded readiness
    and can exit without killing either ready role. Scope rollback to its own unready
    launch. Provide authorized shutdown/status independent of the failed data channel;
    a stale UI cannot use it to bypass fencing. Failed close-both reports an error and
    permits retry or UI-only exit, never automatic force-kill of an existing host.
11. **Retire production legacy paths deliberately.** Remove legacy backend selection,
    old native/browser launch paths and obsolete artifacts/features after each
    replacement is wired. Extract reusable data types/helpers into appropriate neutral
    crates. Retain useful fixtures only as isolated development references. Update
    unsupported-feature UX and docs; do not accidentally claim session save/load parity.

## Testing strategy and execution cadence

The user's targeted-testing instruction governs this plan: local stage commits use
focused verification. This explicitly replaces the blanket full-suite-before-every-
behavior-commit cadence in `.agents/info/test.md`. Preserve formatting, warning-denying
builds, test-usage rules, required pre-push suites, and final validation. Miri belongs
only to the final stage; an actual memory-safety failure may justify a focused early
Miri investigation, not routine repeated runs.

| Point in development | Required verification |
| --- | --- |
| Inner loop | Relevant test/filter plus changed package and direct-consumer compile checks; deterministic dummy/fake services; no full matrix, long stress, package rebuild, or Miri |
| Meaningful stage commit | Stage-specific tests below; `cargo fmt --all`; warning-denying build of affected packages/targets; test-usage check when Rust tests change; relevant dependency/tracing checks when their inputs change |
| Integration checkpoint | One representative end-to-end path for the connected layers; broader impacted suites and one development browser smoke when browser wiring changes |
| Behavior-changing push | Complete applicable suite groups as required by `.agents/rules/mandates.md`; batch stages into reviewable integration pushes rather than pushing every tiny edit |
| Final candidate | Updated full native/Node/browser gates, supported-platform package matrices, bounded recovery stress/performance checks, then selected Miri; CI/review on the final head |

Use existing harnesses and choose package/test filters before each change. Typical
forms are `cargo nextest run -p <package> -E 'test(<filter>)' --profile ci`,
`RUSTFLAGS='-D warnings' cargo build -p <affected-package>`, and
`python3 scripts/run_wasm_tests.py --runtime node --package <package>`. Use the
repository build environment. Resolve exact feature sets from current manifests;
remove legacy `shoop_engine/app_backend` assumptions when retiring those packages.

Prefer shared behavioral contract cases run through typed and encoded app delivery,
plus thin platform transport tests. Test real outcomes (identity/media preservation,
operation ordering, continued frames, safe exit), not duplicate implementations.
Use observable acknowledgments/frames and bounded deadlines instead of sleep-only
assertions. Keep mandatory deterministic coverage separate from optional hardware
smokes; unavailable hardware must never turn required mock coverage into a skip.

Record command, head, outcome, and scope for meaningful validation. Do not rerun
unaffected expensive suites after documentation-only edits. After code fixes, rerun
affected tests and final gates whose evidence is invalidated. Preserve Miri workloads,
seeds, watchdogs, and test assertions; runtime budget problems need documented
scheduling/sharding rather than weakened checks.

## Staged implementation

Stages are ordered by dependency. All verification is targeted unless explicitly
assigned to an integration checkpoint or the final stage.

### Stage 0 — baseline and state/contract inventory

- [x] Create the planning branch from PR #897 and record the approved milestone.
- [x] Rewrite this plan for universal session authority, agreed names, browser-main
  hosting, legacy retirement, and targeted testing with final-stage Miri.
- [x] Recheck upstream/base and working tree before implementation; retain this branch
  or create an implementation branch from it, recording any rebase/base changes.
- [x] Inventory state ownership, supported operations/capabilities, production legacy
  dependencies, and existing useful tests. Identify every startup/autoconnect/script
  side effect that must be owned by the service or gated on attachment.
- [x] Specify app API/protocol, local delivery, snapshot revision ordering, accepted
  versus confirmed operations, lease loss/fencing, CLI/transport settings migration,
  and exit behavior. Record the exact targeted tests and final gates to use.

**Verify:** trace representative rename, select/target/play, create/remove, routing,
Lua control, and media-read paths to their proposed owners. Map each A1–A11 to later
checks. Inventory existing performance evidence; avoid a full baseline matrix run.

### Stage 1 — portable contracts, crate boundaries, and transport extraction (after 0)

- [x] Extract engine-independent `shoop_backend` contracts/shared types from legacy
  imports; establish `shoop_app_protocol` and `shoop_app_client` boundaries.
- [x] Add versioned app messages/codec, stable identities, results and revisioned
  state updates, bounded transfer, and typed local delivery interfaces.
- [x] Extract shared record primitives/carriers to `shoop_transport*` as needed;
  keep engine-specific protocol/credit semantics separate and reuse carrier code.
- [x] Update manifests/dependency audits and direct consumers incrementally; keep
  app/Lua/UI code out of RT and concrete carrier crates.

**Verify:** focused codec malformed/truncated/oversize/version tests, local-versus-
encoded contract cases, affected dependency audits, and native/Wasm compile checks
for changed portable crates. Run existing carrier tests affected by extraction.

### Stage 2 — authoritative service and persistent engine adapter (after 1)

- [x] Extract `shoop_app` service state and handlers using the ownership inventory;
  retain its RT adapter/mappings and pending operations for the session lifetime.
- [x] Move naming/order, selection/target, global policies and shared operation
  resolution into the service. Separate UI/file-dialog and Lua-local workflows.
- [x] Implement confirmed revisions, session-owned accepted operations, engine
  outcome reconciliation, bounded result retention, and snapshot/update subscriptions.
- [x] Initialize the session/sync track exactly once. Remove UI attachment from
  RT adapter construction and unknown-strip cleanup; retain valid engine invariants.

**Verify:** service tests with a deterministic backend/clock cover state ownership,
select-then-play/target ordering, engine refusal, interrupted client delivery during
create/remove/routing, snapshot races, and retained pending work. One real in-process
RT-engine workflow verifies the extracted service against actual engine behavior.

### Stage 3 — UI/Lua clients and browser-main composition (after 2)

- [x] Route UI actions and Lua control through `shoop_app_client`; make UI state a
  confirmed projection with explicitly provisional edits. Preserve supported Lua
  reads/events/timers/MIDI behavior while keeping script execution client-side.
- [x] Use typed local app delivery for native in-process and browser-main modes.
  Pump client/service cooperatively, without recursive dispatch or blocking waits.
- [x] Keep browser audio/device glue and RT MessagePort connection to the worklet;
  add no Worker or application model to the worklet. Maintain supported Web MIDI flow.
- [x] Gate initial scripts to fresh-session startup; reconnecting clients do not
  rerun initialization, saved startup session loads, or autoconnection policies.

**Verify:** focused client/UI/Lua tests for ordering, delayed/refused replies, event
updates and projection readiness; Node tests for changed portable/browser code.
**Integration checkpoint:** one native in-process core workflow and one development
Chromium worklet workflow. Check basic main-thread latency/queue bounds before moving on.

### Stage 4 — independent native host and portable attachment (after 3)

The independent `shoop_host` executable owns the service, adapter and native engine.
The external-mode launcher starts independent host/UI siblings and exits after
bounded authenticated host readiness. Explicit host-only, UI-only and host listing
roles use protected registry/credential references; socket clients can replace a
lost UI without rebuilding the adapter. Shared-memory startup uses the common
carrier and deliberately refuses replacement after its one connection. Management
requests preserve the last owner's generation fence after data loss. Old engine-only
probe/reference paths and package wiring are retired in Stage 6; final platform
package validation remains Stage 7.

- [x] Replace `shoop_rt_host_process` composition with `shoop_host`, owning app
  service + RT adapter + native engine host. Move launch duties to the launcher;
  eliminate UI-owned child/supervisor destruction and inherited control pipes.
- [x] Add host-only/UI-only roles and explicit endpoint/config/credential-reference
  arguments. IPC now carries app protocol; the host's RT connection remains internal.
  Preserve shared-memory delivery using common carriers, without adding recovery to it.
- [x] Add the portable attachment/lease lifecycle and platform discovery/authentication
  adapters. Retain session state across socket connection replacement, clean up stale
  entries, and enforce version/host/session identity checks.
- [x] Implement combined sibling launch, readiness/rollback, and independent lifetimes
  across native platforms. Remove control dependency on launcher stdout or process handles.

**Verify:** focused real socket/subprocess tests for host-first/UI-only and combined
launch, UI/launcher loss, repeated attachment, bad credentials, busy/version/stale
identity rejection, startup failure, and generation fencing. One shared-memory
startup/core/exit smoke checks the migrated composition; no long stress or Miri here.

### Stage 5 — recovery UX and explicit shutdown (after 4)

- [ ] Add host discovery/explicit selection, attachment progress and failure states,
  user-driven reattachment, and coherent snapshot-to-live subscription handling.
- [ ] Add both close menu actions and window/platform quit choice with Cancel for
  external hosts. Document mode limits; local/browser close cannot preserve its host.
- [ ] UI-only exit releases control. Close-both uses authenticated current authority,
  including the management path after data loss, and waits for reset/cleanup results.
  Offer retry/UI-only exit on failure; deny stale/non-owner shutdown.
- [ ] Show Lua unavailability after recovery and allow explicit user script startup.
  Never run scripts or mutate the session merely because a new UI attached.

**Verify:** targeted GUI/application tests for fresh/recovered/disconnected clients,
both menu actions, window close/Cancel, stale-owner close, failed shutdown, and
commands gated before readiness. **Integration checkpoint:** one socket record/play,
UI-kill, reattach, UI-only-close, reattach, close-both workflow with actual service clients.

### Stage 6 — sole new-engine production stack and documentation (after 5)

- [ ] Remove legacy backend selector/fallback, native/browser launch paths, obsolete
  worklet/worker artifacts, and unused feature/dependency/package wiring. Keep useful
  independent code and dev-only reference fixtures explicitly separated.
- [ ] Make the new engine the sole production backend for every deployment; migrate
  persisted backend settings deliberately with diagnostics instead of silent fallback.
  Update transport setting names/help to reflect the app versus engine boundaries.
- [ ] Keep unavailable features explicitly disabled/refused; update user docs,
  architecture, crate READMEs, CLI examples, tracing inventory, and agent test guidance.
- [ ] Update CI coverage/build commands, package manifests/probes and Miri targets
  for the new names/dependencies. Remove obsolete legacy-only gates while preserving
  relevant behavioral coverage and failure canaries for the surviving stack.

**Verify:** production dependency/artifact audits prove legacy removal; focused
settings-migration/capability tests and packaged development startup smoke pass.
Compile changed native/browser entry points and run affected scripts' own tests.
Queue the complete retained-stack suites for the integration push, not every edit.

### Stage 7 — final validation, Miri, CI, and review (after 6)

- [ ] On a complete candidate, run formatting, full warning-denying build, full retained
  native and Node suites, required browser/AudioWorklet workflows, dependency/tracing/
  test-usage policies, and updated Linux/macOS/Windows debug/release package matrices.
  Adapt obsolete feature flags without weakening coverage for surviving behavior.
- [ ] Validate packaged host-only/UI-only and sibling-launch workflows: create/name/
  reorder/select/target tracks/loops, set controls/routes, record audio/MIDI, start
  playback and another recording, kill UI, confirm output/capture progression, reattach,
  and continue editing. Repeat detach/reattach and both close actions, with launcher loss.
- [ ] Inject disconnects at service acceptance, engine submission/confirmation,
  publication, snapshot transfer, and acknowledgment delivery. Verify stable media/IDs,
  pending operation outcomes, no replay/duplicate startup, and stale-client fencing.
  Run a bounded repeated-recovery stress suite and check resource reclamation.
- [ ] Measure typed local versus encoded behavior, native IPC control responsiveness,
  browser main-thread responsiveness under command/state/media bursts, snapshot cost,
  and worklet callback/overrun and heap/memory-growth invariants. Compare with the
  baseline and record workloads, capacities, hardware, results, and limitations.
- [ ] Exercise dummy and CPAL/midir mocks; add available virtual/physical facilities
  and document gaps. Harnesses explicitly clean up surviving hosts after assertions.
- [ ] Run selected Miri checks **only now**, after ordinary integration checks are
  stable: affected portable ownership/queue/codec lifetimes and applicable existing
  runtime/transport cases. Preserve established seeds/workloads/watchdogs. After fixes,
  rerun affected checks; avoid repeating unrelated Miri batches.
- [ ] Record final-head evidence in `VALIDATION.md` and an A1–A11 evidence index,
  including package/performance/Miri results, remaining capabilities, and review status.
- [ ] Push the implementation branch and create/update the PR against `feat/engine-ipc`
  while #897 is unmerged, or its merged successor. Use batched integration pushes;
  satisfy pre-push suite requirements and keep the PR focused on the final scope.
- [ ] Work required CI to green on the final head. Check automated review and address
  applicable feedback iteratively until approval if a reviewer is available. Record
  unavailable review honestly; absence of feedback is not approval. Do not merge.

**Verify:** all A1–A11 have final-head evidence, required checks pass, and available
review is resolved. Re-run gates invalidated by later code changes; evidence for
unchanged expensive checks can be retained with explicit provenance.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone, using the testing cadence above.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
