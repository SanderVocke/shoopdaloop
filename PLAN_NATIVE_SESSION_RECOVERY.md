# Native host independence and UI session recovery

## Status and baseline

Planning only; implementation has not started. Requested through
[write_plan.md](.agents/prompts/write_plan.md). Branch:
`plan/native-session-recovery`, based on PR [#897](https://github.com/SanderVocke/shoopdaloop/pull/897)
head `e369fc257abde71a32a968aa5f922da23bed0c2a` (`feat/engine-ipc`).
This milestone supersedes that PR's deliberate prohibition on socket reattachment
and its implicit app-owned shutdown policy. Preserve its other capability limits.

Investigated implementation points:

- `shoop_rt_host_process/src/process.rs`: the app creates a listener, launches a
  child, passes credentials through stdin, and owns its supervisor/shutdown pipe.
- `shoop_rt_host_process/src/serve.rs`: one connection is accepted; its endpoint
  is retained and polled after loss, but new clients cannot attach.
- `shoop_rt_transport_socket/src/setup.rs`: the existing socket carrier already
  uses authenticated **loopback TCP**. Remote TCP/IP deployment remains future work.
- `shoop_rt_engine/src/endpoint.rs` and `resync.rs`: connection retirement preserves
  admitted music; Hello/Resync and generation-aware structural state already exist.
- `shoop_rt_backend/src/incoming.rs::apply_resync`: unknown strips are currently
  queued for removal. A fresh empty mirror must never use this policy to recover.
- `shoop_app/src/lib.rs::ApplicationModel::initialize`: creates the sync track;
  adoption needs a separate constructor. Lua and its native MIDI service live here.
- `shoopdaloop/src/main.rs`, `engine_backend.rs`, `app_args.rs`, and `shoop_egui`:
  composition, startup, fatal-disconnect UI, transport settings, and close handling.
- `scripts/check_rt_engine_boundary.py`: existing dependency boundaries forbid
  importing application semantics into the engine, protocol, carriers, or hosts.

## Goals and scope

Recover control of a surviving native engine from a newly launched UI process,
without interrupting playback, recording, or admitted work. Support independently
started host and UI roles and a normal launcher that starts them as siblings.
Expose explicit menu actions to **Close UI only (keep host running)** and
**Close UI and host**. Host lifetime must never follow an implicit UI destructor.

Implement recovery over the current local socket carrier, with one controlling
client. Put recovery semantics above concrete carriers so remote transports,
multiple UIs, and relocating Lua can be added incrementally.

Excluded: simultaneous UIs/observers, remote access/TLS, shared-memory reattachment,
engine restart, disk-backed session durability, live transport/driver switching,
new engine feature parity, latency compensation, undo-history recovery, and Lua
execution-state recovery or automatic script restart. App-side Lua/controller MIDI
can disappear while the host runs; startup callbacks must not be replayed on adoption.
Ordinary first-session script startup remains supported.

## Immutable acceptance criteria

- **A1 — independent roles:** packaged host-only and UI-only commands accept
  explicit endpoint/configuration and credential-reference arguments. The combined
  launcher starts sibling host/UI processes. After readiness, loss or exit of the
  launcher or UI does not terminate or stall the healthy host. No inherited parent
  pipe, process handle, console lifetime, or job policy is needed for later control.
- **A2 — attach:** a new UI can select a discovered host or an explicit endpoint,
  authenticate, verify host/session identity and compatibility, and acquire control.
  Refuse incompatible, unauthorized, duplicate, and stale clients without disturbing
  the live session. Never evict a healthy owner or silently start a replacement host.
- **A3 — continuity:** UI loss, detach, synchronization, and repeated reattachment
  preserve engine object IDs/generations, audio/MIDI media, routing, playback,
  active recording, and admitted work. No recovery-triggered stop/reset/reupload,
  duplicate sync track, deletion of unknown live objects, or device reopening occurs.
- **A4 — application reconstruction:** restore all confirmed shared app metadata
  required by the supported engine core: object mappings, names/order, sync identity,
  shared global settings and policies, and desired connection configuration. Combine
  this with authoritative engine structure, controls, routing, and current telemetry.
  An acknowledged change survives UI death at any following instruction boundary.
- **A5 — crash consistency:** reconcile structural mutations interrupted before or
  after engine admission/confirmation. Preserve admitted work; discard unsent work;
  never blindly replay requests with lost replies. Unacknowledged edits may be lost,
  but no live object may be deleted, duplicated, or left permanently unmanageable.
- **A6 — control isolation:** one control lease is enforced by the host, with fresh
  connection generations. Retired clients cannot mutate or shut down the session,
  including after a replacement UI attaches. Attachment alone does not transfer
  process-lifetime ownership to the UI.
- **A7 — explicit close:** both menu actions work in combined-launch and standalone
  modes, including after recovery. UI-only close releases control and leaves music
  running. Close-both performs authorized MIDI reset/render stop/cleanup, reports
  failures, and closes the UI after successful host shutdown. Window-manager close
  and platform quit route through the same explicit choice; Cancel leaves both open.
  A crash or lost connection never implies shutdown or forced host termination.
- **A8 — bounded recovery:** retain a versioned recovery document in host-side
  non-RT memory and obtain a coherent structural snapshot before enabling commands.
  Ongoing recording need not finish. Reclaim connection resources across repeated
  recovery; stale registries cannot attach a UI to a different host/session.
- **A9 — extensible boundaries:** shared attachment/session logic has no socket,
  TCP, filesystem, UI, or device dependency. Client identity, connection generation,
  session identity, and metadata revision are distinct. Existing RT constraints and
  capability refusals remain intact; no Lua or app semantics enter RT callbacks.
- **A10 — delivery:** validate both launch styles and both close actions on packaged
  Linux/macOS/Windows builds. Existing in-process, shared-memory, browser/worklet,
  and legacy paths retain their supported behavior except for explicitly documented
  external-host launch/close changes. Required CI and available automated review
  are resolved against the final implementation head, with evidence and limits recorded.

Goals and A1–A10 require explicit user approval to change.

## Design rules

1. **Session versus connection.** The running session owns engine resources,
   recovery metadata, and unresolved admitted operations. Each connection owns
   its queues, credits, subscriptions, correlations, and permissions. Never use
   per-client teardown to drop/reset the session endpoint or silence rendering.
2. **Transport-independent control.** Define logical attachment, detach, lease,
   status, and shutdown messages in a portable schema and implement their state
   machine above carriers. Socket code supplies listening/authenticated delivery
   and loss detection. Reuse engine messages/codecs; do not fork musical semantics.
   Verify shared behavior through an in-memory carrier as well as sockets.
3. **Narrow host-side session service.** Introduce a bounded non-RT coordinator
   outside the RT core for attachment, leases, recovery revisions, and pending
   operation reconciliation. Keep application metadata opaque to low-level hosts;
   application code interprets it. Extend dependency audits for the new composition
   rather than allowing UI/application imports into existing engine/host crates.
4. **Single authority.** The host retains the canonical acknowledged metadata
   revision. Only the current lease may submit changes, with an expected revision.
   UIs rebuild projections of that state. Reserve a common operation interface and
   actor identity for a future session controller/Lua actor; do not implement one
   independent authoritative application session per future UI.
5. **Recovery consistency.** Before submitting an engine-affecting structural
   mutation, retain its operation ID, object identities, and metadata intent in the
   host. Observe its terminal result independently of the UI connection, commit or
   reconcile metadata, then acknowledge the shared change. Start with one pending
   structural operation if that simplifies correctness. Bound journal/document
   storage and reject before admission when recovery capacity is exhausted.
   Cosmetic metadata updates also require host acknowledgment. This is in-memory
   retention for UI recovery, not durable exactly-once execution after host failure.
6. **Authority before adoption.** Fence the old owner, settle/reconcile admitted
   structural operations, then stream a revision-consistent structural/document
   snapshot and fresh telemetry. Do not wait for playback or recording to stop.
   Gate mutations and startup side effects until adoption completes. Preserve
   scheduled actions; audit whether existing resync/telemetry expose enough state.
7. **Stable identity.** Scope object mappings and metadata to a host/session UUID;
   preserve engine slot generations and allocator floors. Scope request IDs to
   client/connection generation. Reject stale leases for every mutation and host
   shutdown. Physical reconnection always uses a fresh connection and queue state.
8. **Discovery is optional.** Use a per-user restricted runtime registry containing
   non-secret endpoint, identity, and compatibility data. Store credentials through
   a protected reference; never put bearer tokens in argv, registry listings, or
   logs. Validate identities through the authenticated endpoint, not PID alone.
   Keep explicit endpoint selection usable without discovery; do not auto-select
   between multiple sessions. Admission/authentication must not block render work.
9. **Independent launch and explicit shutdown.** The launcher starts both roles,
   waits for bounded readiness, and may exit. Standalone host startup selects its
   devices/pools from explicit configuration. A ready empty host waits for its first
   session initialization; later UIs adopt. UI drop sends no host-stop command.
   Host shutdown is an authenticated logical operation available after parent loss;
   provide an authenticated management path for close-both if the data connection
   fails. Never automatically kill an existing host on timeout or adoption failure.
   Startup rollback may clean up only the unready host created by that launch.
10. **Future features remain separate.** Multiple-client support can replace the
    exclusive lease policy and add subscriptions/conflict handling without changing
    identities or session ownership. Lua may later run in a surviving non-RT process
    or host-side service and use the same operations. Exact Lua checkpoints and any
    RT-safe Lua execution model are separate features; no VM snapshot is required now.

## Staged implementation

Stages are sequential unless their listed dependencies permit otherwise. Commit
each completed stage or meaningful milestone; keep every verification tied to a head.

### Stage 0 — baseline, contracts, and branch

- [x] Create `plan/native-session-recovery` from the inspected PR #897 head and
  record the milestone here; no production changes in this planning stage.
- [ ] Before implementation, check the upstream PR/base and working tree; retain
  this branch or create an implementation branch from it. Record any rebase and
  re-investigate changed integration points before editing behavior.
- [ ] Map every app-owned field needed by A4 to its recovery representation; state
  which values come from engine resync versus retained metadata. Identify app startup,
  script startup, auto-connect, and global policy side effects that adoption must gate.
- [ ] Specify logical messages, version compatibility, lease-loss detection and
  revocation, metadata acknowledgment/operation ordering, snapshot boundaries,
  close semantics, and CLI contracts. Preserve loopback-only deployment initially.
- [ ] Identify exact tests/build gates from the baseline; record pre-existing
  failures and available real/virtual audio and MIDI facilities.

**Verify:** review the field/ownership and failure-state inventory against A1–A10.
Resolve implementation choices without weakening criteria; demonstrate that every
engine-affecting acknowledged edit has a defined recovery owner.

### Stage 1 — portable attachment and session ownership (depends on 0)

- [ ] Add the portable schema/coordinator and session/connection/actor identities;
  define bounded states for uninitialized, detached, synchronizing, attached,
  stopping, stopped, and faulted. Faulted engines cannot be recovered as healthy.
- [ ] Enforce one lease, generation fencing, revision checks, and explicit detach
  versus shutdown; separate connection bookkeeping from retained endpoint work.
- [ ] Allow replacing retired transports without replaying old records or blocking
  completions. Extend dependency audits and keep platform details behind adapters.

**Verify:** carrier-independent contract tests for handshake/version rejection,
duplicate owners, loss/reacquisition, stale commands/shutdown, bounded outstanding
work, host faults, and repeated connection teardown without engine reset.

### Stage 2 — independent launch, socket attachment, discovery (depends on 1)

- [ ] Refactor `shoop_rt_host_process` into a host-owned listener/service and a
  client connector. Remove lifetime/authentication dependence on the UI's stdin,
  stdout, child handle, and `Supervisor::drop` path.
- [ ] Add host-only/UI-only CLI modes with explicit endpoint, config, and protected
  credential-reference arguments; document exact commands and readiness errors.
- [ ] Make the ordinary external-host launcher start host/UI siblings with independent
  lifetime and appropriate Linux/macOS/Windows process/console/job handling. Retain
  existing shared-memory delivery without adding shared-memory reattachment.
- [ ] Publish/clean up registry entries atomically; handle stale entries, address
  reuse, concurrent launches, credentials, and package companion discovery.

**Verify:** real subprocess tests launch host first or through the launcher, attach
UI separately, terminate UI/launcher independently, and confirm host progression.
Test endpoint conflicts, missing config, failed bootstrap, identity mismatch,
unreachable hosts, bad credentials, and two competing UIs. No secret values in logs.

### Stage 3 — retained metadata and interrupted operations (depends on 1–2)

- [ ] Implement the versioned, bounded recovery document and host acknowledgment
  interface using Stage 0's field inventory; retain no live Lua execution state.
- [ ] Implement host-owned structural intents/results and revision publication.
  Retain enough information to finish/reconcile work after external client loss;
  do not discard outcome ownership with connection-scoped correlations.
- [ ] Connect current app mutations to metadata updates and engine confirmations.
  Define bounded cleanup of finished journal entries and abandoned bulk transfers.
- [ ] Provide a coherent metadata/engine snapshot boundary while rendering and
  active capture continue. Report incompatible/corrupt metadata without mutations.

**Verify:** deterministically kill/disconnect the UI before prepare, after prepare,
after engine admission, after confirmation, and before/after acknowledgment for
create/delete/rename/reorder/routing and representative global changes. Check A4–A5,
capacity refusals, continued admitted work, and bounded retained state.

### Stage 4 — backend and application adoption (depends on 3)

- [ ] Add an explicit adoption mode to `RtBackend`; reconstruct tracks, loops,
  ports, mappings, allocator generations, controls, and routes from the snapshot.
  Replace unknown-strip cleanup during adoption with safe reconciliation.
- [ ] Add `ApplicationModel` recovery construction that uses existing sync/object
  identities and retained metadata. Keep fresh-session construction separate.
- [ ] Reconcile pending actions and fresh telemetry before enabling engine commands;
  retain recorded audio/MIDI in the host and resume supported media reads lazily.
- [ ] Suppress automatic script execution and startup auto-connect/session-replace
  side effects on recovery. Make script unavailability clear without claiming Lua
  state recovery; an explicit later user script start remains an ordinary action.

**Verify:** recover sessions with multiple named/reordered tracks/loops, sync links,
controls, routes, scheduled transitions, and active audio/MIDI recording. Assert
identity/media preservation and inspect emitted requests for unintended creates,
removals, resets, reconnects, or script-origin mutations.

### Stage 5 — recovery UI and explicit close actions (depends on 2–4)

- [ ] Add host selection/discovery and explicit-endpoint startup, with progress,
  compatibility/busy/failure states. Never silently attach elsewhere or create a host
  after an attachment failure. Allow canceling attachment without stopping music.
- [ ] Replace the unconditional fatal-disconnect page for recoverable socket sessions;
  allow user-driven reattachment using the same path as a newly launched UI.
- [ ] Add menu actions **Close UI only (keep host running)** and **Close UI and host**.
  Route window close/platform quit through an explicit chooser with Cancel. Apply
  choices consistently to standalone, launched, recovered, and disconnected UIs.
- [ ] UI-only exit releases its lease without host shutdown. Close-both authenticates
  current authority and awaits reset/stop/cleanup acknowledgment. On failure, show
  the error and allow retry or UI-only exit; do not claim success or force-kill.
- [ ] Disable close-both for stale/non-owning UIs; remove implicit host termination
  from UI destruction. Keep non-external modes' ordinary exit behavior.

**Verify:** UI/application workflow tests cover both menu actions, window close,
platform quit, Cancel, data-path loss, adoption failure, host exit, and a stale UI
closing after a replacement attaches. Confirm no engine command is accepted early.

### Stage 6 — packages, regression, and documentation (depends on 5)

- [ ] Update architecture, host/transport/app READMEs, CLI help, settings descriptions,
  and tracing inventory. Document role startup, endpoint selection, metadata limits,
  Lua behavior, both close choices, and unsupported shared-memory recovery.
- [ ] Extend staged/extracted package probes and CI for sibling launch, standalone
  roles, socket recovery, and authenticated shutdown on Linux/macOS/Windows.
- [ ] Run targeted packages first, then required native/Node/browser gates from the
  checked-out test guidance and CI. Include warnings-denied builds, formatting,
  test-usage/tracing/dependency policies, applicable Miri, and existing carrier tests.
  Preserve in-process, shared-memory, legacy, and browser/worklet capabilities.

**Verify:** packaged binaries locate companions and work across paths with spaces
and non-ASCII characters. No parent-lifetime dependencies, credential leaks, stale
registry adoption, or regressions in supported startup/shutdown workflows.

### Stage 7 — final end-to-end validation and delivery (depends on 6)

- [ ] Execute a packaged host + UI workflow: create/name/reorder tracks, configure
  routes, record audio/MIDI, start playback and another recording, kill the UI,
  confirm continuous host output/capture, launch UI-only, adopt, and continue editing.
- [ ] Repeat recovery, then close UI-only, reattach again, and close both. Exercise
  the same flow with separately started host/UI and with launcher loss after readiness.
- [ ] Repeat structural crash-point cases and competing/stale-client tests through
  actual sockets. Verify media contents, identity continuity, no duplicate actions,
  lease isolation, MIDI-reset ordering, and bounded resources across repeated cycles.
  Use frame/event/acknowledgment observations instead of sleep-only success criteria.
- [ ] Exercise dummy and CPAL/midir mock paths; use virtual or physical facilities
  where available and record hardware gaps. Run all three native platform matrices
  and required browser regressions; retain failures and explain justified retries.
- [ ] Record final-head commands, outcomes, artifacts, package results, and limitations
  in `VALIDATION.md` plus an acceptance-to-evidence index. Harnesses must explicitly
  clean up intentional surviving hosts after assertions, independently of UI exit.
- [ ] Push the implementation branch and create/update a PR against `feat/engine-ipc`
  while #897 is unmerged, or its merged successor once available. Keep the diff
  limited to this milestone and identify any base change explicitly.
- [ ] Work required CI to green on the exact final head. Check for automated review;
  repeatedly address applicable feedback and recheck until approval when a reviewer
  is available. Document unavailable review or unrelated bot output honestly; do
  not claim approval from absence of feedback. Do not merge as part of this plan.

**Verify:** every A1–A10 has final-head evidence; all required checks pass, available
automated review is resolved, and the PR states actual support and remaining limits.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
