# Plan: native engine process with socket and shared-memory IPC

[Main plan](PLAN.md) · [Architecture](ARCHITECTURE.md#new-engine-backend) · [Prior backend plan](PLAN_RT_BACKEND.md) · [Validation](VALIDATION.md)

## Status and authority

Execution in progress on `feat/engine-ipc`: Stages 0–5 are verified locally;
Process hosting and supervision are next. Platform runtime verification remains in Stage 9. Scope agreed on 2026-10-31:
the native app supervises one engine child process on the same system, with a
choice of socket or shared-memory IPC. There is no reconnect capability and no
new mitigation for increased transport latency. This is the implementation plan
requested through [.agents/prompts/write_plan.md](.agents/prompts/write_plan.md).

User-approved lifecycle revision (2026-10-31): on disconnect the app displays a
fatal error and stays open without recovery; a healthy engine keeps running,
uncontrollable, even after an app crash. Explicit normal app shutdown still stops
the engine gracefully. The criteria and stages below incorporate this revision.

User-approved architecture revision (2026-10-31): all deployments use the same
logical engine messages. Typed in-process delivery may skip serialization;
independent-memory delivery uses the common codec and record adapters. Separate
shared transport machinery, transport implementations and engine hosts into the
role-based crate families below before adding IPC.

Investigated source: `f22dbd1c8ea72ae1bc5497b40cd7daf6066f3b39` on
`feat/loop-execution-layer`. The prior backend's local verification is recorded
in `VALIDATION.md`; it is background, not verification of this milestone.

## Goals and scope

1. Run the existing opt-in new engine backend in a separate native process,
   retaining the existing UI, application adapter and core looping feature set.
2. Offer two explicit IPC choices: loopback TCP sockets and shared memory, both
   using the existing protocol semantics and binary records.
3. Move device discovery, configuration and ownership to the engine process;
   provide clear startup, fatal disconnect and explicit shutdown behaviour, with
   autonomous engine operation after connection loss.
4. Preserve the existing in-process and browser paths and all capability refusals.
5. Make protocol, connection machinery, carriers, hosts and application mapping
   explicit crate responsibilities, with one implementation of shared behaviour.

**Included:** Linux, macOS and Windows; dummy and CPAL/midir hosts; the full
[existing core set](PLAN_RT_BACKEND.md#scope), including asynchronous audio/MIDI
detail reads; restart-required transport selection; a packaged headless engine
executable; one authenticated app/child pair and its startup control channel.
Also included: extraction of shared transport machinery and the existing browser
MessagePort carrier into the same crate structure used by the new IPC carriers.

**Excluded:** reconnect, automatic child restart, attaching to an existing engine,
session adoption after UI restart, persistent services, multiple clients,
cross-machine connections, browser-to-native connections, audio streaming over IPC,
live transport/driver switching, capacity growth and closing backend parity gaps.
There is no latency compensation, prediction, clock synchronization, extra
lookahead, scheduling-policy change or new latency target. Existing observation
lag and timing behaviour remain documented; IPC latency is allowed to delay
immediate commands or cause a command to miss an intended musical boundary.

## Immutable acceptance criteria

These must not change without explicit user approval.

- **I1 — selection and isolation.** A restart-required setting and CLI override
  select `in_process`, `socket` or `shared_memory` for the native `new_engine`
  backend. Defaults stay unchanged. Both IPC choices run the app and engine in
  distinct processes; the child owns its engine and audio/MIDI devices. Selection
  never silently falls back to another carrier or the legacy engine.
- **I2 — equivalent core behaviour.** Both IPC choices support the existing core
  workflow, capability gating, confirmations, state/meters and chunked media reads.
  Shared backend contracts pass. For identical timestamped commands admitted before
  their deadlines, deterministic rendering agrees with the in-process reference
  in samples, MIDI, transition outcomes, lengths and content. Wall-clock UI action
  times are not required to produce identical transition frames across carriers.
- **I3 — genuine carrier choice.** Socket mode sends protocol records over
  loopback TCP; shared-memory mode sends them through process-shared storage,
  including reliable, telemetry and bulk traffic. Both preserve protocol credits,
  ordering and lane retention rules with bounded storage. A startup/liveness
  channel is permitted in shared-memory mode; socket forwarding of its ordinary
  protocol traffic is not an implementation of shared-memory mode.
- **I4 — engine-side setup.** Discovery, initial driver/device/MIDI selection,
  engine capacities, startup warnings and failures cross an engine-owned startup
  protocol. The UI process need not open devices for the selected IPC backend.
  Device changes take effect on application restart.
- **I5 — one connection lifetime.** Each child serves one app connection, with
  initial handshake and state synchronization only. Startup failure, EOF, peer
  exit, malformed traffic and engine faults produce a terminal failure. On
  disconnect the app displays a fatal error, stays open and refuses engine
  operations; no recovery is offered. There is no reconnect/retry loop, automatic
  child replacement or replay of outstanding commands. Restarting the app starts
  a fresh engine and never attaches to the old one.
- **I6 — autonomous operation and explicit shutdown.** After successful startup,
  connection/control-channel loss or app crash leaves a healthy engine running
  with its existing musical state, playback, recording and internal processing;
  it accepts no further client control. Explicit normal app shutdown, including
  closing the failed app, instead stops its engine through the existing
  MIDI-reset/render-stop/cleanup path. Startup rollback cleans up incomplete
  launches. Engine exit fails pending app work; resources no longer needed by a
  surviving peer are reclaimed without destroying live engine resources. An
  engine surviving an app crash requires external termination. Actual render
  faults retain the existing fault/silence policy; a killed engine cannot send
  MIDI cleanup.
- **I7 — callback and dependency boundaries.** Render callbacks and the engine
  control endpoint retain their no-allocation/free, no-lock, no-wait contract.
  No socket, mapping or process-management operation reaches the render callback.
  Musical semantics exist once; native IPC dependencies do not enter the portable
  engine/protocol/transport/adapter or worklet build. Client stalls cannot stop
  admitted autonomous rendering; terminal disconnect preserves it as specified in I6.
- **I8 — platforms and delivery.** Both carriers pass real subprocess tests on
  Linux, macOS and Windows, with dummy and CPAL/midir mock coverage; real-device
  Linux smokes exercise both choices. Existing native/browser gates remain green.
  Packaged apps launch their bundled engine executable. Documentation and
  source-matched evidence cover I1–I9; required final-head CI and automated review
  are resolved as specified in the final stage.
- **I9 — shared protocol and crate roles.** In-process, MessagePort, socket and
  shared-memory delivery use the same logical engine messages, common connection
  machinery and engine handler. In-process delivery does not require encoding;
  the other three reuse the same codec and record client/server. Concrete carriers
  use `shoop_rt_transport_<mechanism>` names and contain both ends; hosts use
  `shoop_rt_host_<environment>` names and own engine execution/lifetime. Portable
  crates have no dependency on concrete carriers or hosts, and carriers have no
  engine/application dependency. Shared behaviour is verified across carriers;
  browser feature/build isolation preserves the import-free worklet artifact.

## Design and constraints

### Crate responsibilities and naming

`shoop_rt_transport_<mechanism>` implements communication; its `client`, `server`
and bootstrap modules contain the two ends of that mechanism. In contrast,
`shoop_rt_host_<environment>` prepares and runs the engine. A host selects a
transport through the common interfaces; a carrier never implements engine
commands or controls engine lifetime.

| Crate | Responsibility | Change from the investigated branch |
| --- | --- | --- |
| `shoop_rt_protocol` | Logical engine and shared host-control schemas, IDs, versions, capabilities and the canonical binary codec | Keep messages/codec; extract `client` and `carrier` machinery; add a separate `host` message family |
| `shoop_rt_transport` | Portable connection state, correlation/credits, endpoint/carrier interfaces, lane queues, typed in-process and encoded-record adapters | Extract `ProtocolClient`, `Carrier`, `Endpoint`, `Outbox`, `InProcess`, `RecordClient` and `RecordServer` from `shoop_rt_protocol` |
| `shoop_rt_transport_message_port` | Main-thread/worklet record delivery, batching and transport errors; both ends | Extract record-port code from `shoop_rt_host_worklet` and reusable MessagePort glue from `shoopdaloop/src/browser_rt_audio.rs` |
| `shoop_rt_transport_socket` | Socket client/server and bounded stream-to-record adaptation | New; loopback TCP in this milestone |
| `shoop_rt_transport_shared_memory` | Shared record queues, notifications and mapping setup/cleanup; both ends | New; owns the native shared-memory dependency |
| `shoop_rt_backend` | Application vocabulary, ID mapping and confirmed state mirror | Keep; import connection machinery from `shoop_rt_transport` |
| `shoop_rt_engine` | Engine semantics, common control endpoint, runtime composition and render owner | Keep; implement the endpoint interface from `shoop_rt_transport` |
| `shoop_rt_host_native` | Native devices, engine preparation, audio/MIDI callbacks and explicit host shutdown | Expose host and endpoint without fixing a carrier |
| `shoop_rt_host_worklet` | Worklet preparation, raw-Wasm ABI, audio/MIDI buffers/lanes and render execution | Keep hosting; use the extracted MessagePort transport |
| `shoop_rt_host_process` | Engine process executable plus parent launch/supervision library | New; compose the native host with either native transport |

The proposed `shoop_rt_ipc` umbrella crate is replaced by the socket and
shared-memory transport crates. Typed local delivery remains the lightweight
`shoop_rt_transport::in_process` module; no separate crate is needed for symmetry.

- Keep the codec with its schema: it has no external dependencies and typed local
  delivery need not call it. Shared transport modules own client state, queues,
  in-process delivery and record adaptation. Concrete carriers supply record ports;
  they do not duplicate the protocol client/server or codec.
- Dependency direction is `protocol <- transport <- concrete carriers`, with the
  engine and application adapter depending on protocol/transport, and hosts
  composing engine and carrier implementations. Protocol/transport depend on no
  application, engine, host or OS transport crate. Extend the boundary audit to
  enforce these rules for production/build dependencies.
- Keep common host setup requests in `shoop_rt_protocol::host`, separate from
  engine mutations. They describe discovery/configuration/start/status/stop, not
  how a carrier connects. Use plain data and host-side conversions; protocol must
  not depend on runtime `EngineDescription` or OS driver types. The common codec
  encodes this vocabulary once; unsupported host operations are explicit.
- Carrier-private setup (mapping descriptors, layout negotiation, connection
  authentication) belongs in the relevant transport crate. Process launching and
  the independent explicit-shutdown signal belong in `shoop_rt_host_process`.
  Worklet loading, its configuration/ABI and browser permissions remain host
  concerns; reuse common configuration fields where their meaning matches.
- Separate browser/main-thread and worklet-side features in the MessagePort crate.
  Only the browser half may depend on JS/browser bindings; independently build the
  worklet artifact without those features/imports. Keep Web MIDI device lanes and
  audio buffers in the host; application settings/UI orchestration stay in
  `shoopdaloop`. This work does not migrate the legacy `shoop_audio_protocol` stack.
- Socket means a communication mechanism, not a new engine protocol. Future remote
  deployment may reuse the socket carrier with different connection policy;
  cross-machine operation and extra network setup are outside this milestone.

### Preserve the execution boundary

```text
App process                              Engine child process
shoop_egui / shoop_app                    process main + native host
  -> shoop_rt_backend                       -> RecordServer / ControlEndpoint
  -> ProtocolClient / RecordClient          -> existing runtime queues
  <====== socket OR shared records =====>   -> render owner -> audio/MIDI devices

App supervisor <--- startup / liveness / stop ---> child supervisor
```

- Keep `shoop_rt_backend` as the single application mapper/mirror and
  `shoop_rt_engine` as the single engine. Do not repurpose the narrower
  `shoop_engine_service` demonstrator or build a universal protocol framework.
- Add the concrete transport crates and `shoop_rt_host_process` as specified above.
  Keep native dependencies confined to native crates and update the portable and
  native audits, including checks that the adapter cannot depend on the engine.
- Expose native host startup with a separately owned `ControlEndpoint`; preserve
  `start` and its in-process client as a convenience wrapper. Today `Started.client`
  fixes the carrier to `InProcess<ControlEndpoint>`.
- The child control thread pumps ingress, endpoint completions, telemetry and bulk
  work even when no new request arrives. Its OS I/O/wakeup work is outside endpoint
  and render calls. The existing runtime queues remain within the engine process.
- Reuse the common `RecordClient`/`RecordServer` and codec in every encoded carrier.
  Records are bounded by `MAX_RECORD_BYTES` (1280) with a little-endian length prefix.
  Shared memory carries encoded bytes, never Rust owners, pointers, `Vec`, or an
  assumed Rust enum layout. Audio rendering does not depend on either carrier.

### Bootstrap and one-shot lifecycle

- Introduce bounded, versioned host-control messages for discovery, configuration,
  start/ready/error/warnings and stop, usable before the engine is prepared.
  Put their schema/codec in the protocol's `host` family and their execution in
  the host. Share connection/framing machinery; do not add another protocol client
  per carrier. Paginate discovery rather than introducing unbounded records.
- The parent launches the packaged child using an absolute executable path and
  establishes a private control channel. Use that channel to exchange endpoint or
  mapping information and authenticate the single connection. Loopback listeners
  bind only locally and accept only the launched peer. Do not expose credentials
  in arguments or logs. Close unrelated inherited handles so peer death is visible.
- Discovery runs in the child; app settings map to engine-owned configuration data,
  which the child validates. Existing startup-only dummy-driver fallback remains
  explicit and reported; transport/startup failures never trigger a carrier fallback.
- Startup order is spawn/control handshake -> discovery/configuration -> carrier
  setup and native host preparation -> engine `Hello`/`Welcome` -> initial `Resync`
  -> normal application startup (which creates the sync track).
- Preserve initial state synchronization and generation handling, but do not build
  session adoption or expose reconnection for IPC. A child accepts one session;
  a second handshake/connection is refused. Existing in-process/browser lifecycle
  support need not be removed.
- Add an explicit receive-side transport-failure path: today `Carrier::receive` and
  `RecordPort::take` use `None` for no message, so EOF must not disappear as idle.
  Convert transport loss into terminal backend failure, clear/fail pending work,
  and show a persistent fatal error without closing the app or waiting for a
  subsequent send to discover the failure. Disable engine operations; offer no
  reconnect, retry, automatic restart or backend fallback.
- On connection loss, retire the session and refuse further clients while keeping
  the `ControlEndpoint`, render owner and device host alive. Dropping the endpoint
  currently silences rendering and must not be the disconnect path. Preserve
  existing queue-discard/in-progress completion semantics, drain completions and
  reclaim abandoned transfers off-render without waiting for absent readers or
  accumulating undeliverable telemetry/results. Continue autonomous music.
- Give explicit normal shutdown a supervisor signal independent of the failed IPC
  connection. On that signal, drop the endpoint before stopping the host so existing
  silence/MIDI reset can run; then join workers, close mappings and reap the child.
  Use bounded startup/shutdown waits and forced termination only for startup
  rollback or explicit shutdown that cannot complete gracefully.
- Parent death or channel EOF must not trigger shutdown or a kill-on-parent-exit
  policy. A surviving engine is intentional and requires external termination;
  a fresh app launches a separate engine. Shared-memory emptiness is not peer-death
  evidence; control-channel/process liveness detects loss but never stops healthy
  rendering. Release dead-connection resources while retaining live engine owners.

### Socket carrier

- Use loopback TCP on all three native platforms, with nonblocking or worker-driven
  I/O. No separate Unix-domain-socket implementation is required for this milestone.
- Assemble complete bounded records from partial reads; retain partial writes and
  offsets. `post` accepts an entire record into prepared storage or returns `Full`
  without accepting it. Retry must never duplicate a partially written record.
- Preserve each lane's order and the existing reliable/bulk retention and telemetry
  discard rules. Keep OS and application buffers bounded and propagate backpressure;
  finish a partially written frame before selecting another record. Do not add
  transport-specific musical scheduling or latency mitigation.

### Shared-memory carrier

- Use a maintained, cross-platform mapping/queue dependency with a safe public API
  and documented interprocess synchronization. Do not put raw mapping casts or
  shared pointer manipulation in the portable crates or weaken their unsafe bans.
  Select and record the dependency in Stage 0, including platform/MSRV/license fit
  and which guarantees it supplies; a process-local SPSC queue is not sufficient.
- Investigated candidate: [iceoryx2](https://github.com/eclipse-iceoryx/iceoryx2#supported-platforms),
  whose current platform table lists Linux, macOS and Windows as tier 2. Confirm
  a pinned release's bounded, non-overwriting configuration and crash cleanup in
  Stage 0; the upstream support table is not verification of this integration.
- Prepare fixed-capacity record queues with one producer and one consumer per
  direction/lane. Use the dependency's shared-storage primitives; document its
  layout/version, alignment, ready state, publication/reclamation ordering and
  counter wrap guarantees. Validate the negotiated record size and capacities
  before access; do not implement another raw shared-memory queue underneath it.
- Prove a producer cannot overwrite unread bytes and a consumer cannot observe a
  partially published record. Use process-shared acquire/release synchronization
  supported on each target, independent of virtual mapping addresses. Do not use
  process-local Rust references/locks as an interprocess contract.
- Preserve the same credits and lane semantics as sockets. Notifications or paced
  waiting belong off-render; no busy spinning in the UI or render callback. No
  zero-copy media leases, shared runtime pools or shared telemetry bypass are needed.
- Give mappings per-launch identities and explicit handle/unlink ownership; release
  OS names as soon as the supported sharing mechanism permits. Never reopen an old
  mapping to resume a session. A crash while publishing terminates that session;
  there is no abandoned-slot repair or reconnect protocol.

### Timing, testing and operating rules

- Retain the current one-callback observation lag and existing native control lead.
  Do not alter sync-delay derivation, timestamps, scheduling or buffering to
  compensate for IPC latency. Tests arrange admission before deadlines; do not
  invent a new wall-clock responsiveness gate or require latency equivalence.
- Follow `AGENTS.md` and `.agents/`; use the appropriate build environment from
  `.agents/info/build.md`, warnings-denied Rust checks, formatting, `#[shoop_test]`,
  test-usage and tracing-coverage checks for changed code.
- Retain the prior plan's shared-machine limits: low-priority heavy commands,
  a hard session memory budget of 20% of host RAM, at most eight build/test workers,
  at most four Miri batches and no concurrent heavy jobs; keep build output on
  scratch storage. Use platform equivalents where the Unix wrappers are unavailable.
- Run focused tests per stage; run the complete retained workspace and browser
  regression set at the final stage. Use release heap guards when callback paths
  change. Test shared-memory publication with a model plus actual processes: Miri
  alone cannot establish OS mapping/interprocess correctness. Register applicable
  ownership/model cases, then run the relevant Miri selection once at the final
  stage. CI still does not run Miri or Chrome; record those manual runs separately.

## Staged implementation

Order: **0 -> 1 -> 2 -> 3 -> {4, 5} -> 6 -> 7 -> 8 -> 9**. Stages 4 and 5
are independent after Stage 3; both are required before Stage 6. Extract and verify
the existing native/browser paths before adding IPC.

### Stage 0 — Implementation branch and baseline

- [x] Create `feat/engine-ipc` from the current `feat/loop-execution-layer` head;
      record the base SHA, clean/dirty state and platform/toolchain versions.
- [x] Record focused protocol, adapter-contract and native-host baselines; preserve
      prior validation's limitations rather than claiming they are newly resolved.
- [x] Record the current MessagePort/worklet contract and import/dependency audit
      baselines so crate extraction is checked against the existing deployment.
- [x] Select the shared-memory dependency and record its mapping, synchronization,
      cleanup and platform guarantees with a small two-process feasibility probe;
      settle record capacities and bootstrap envelopes.

**Verify:** baseline suites pass; the selected dependency supports both processes
on all three native OSes. Document design revisions here before relying on them.

*Recorded:* base `f22dbd1c`, with the approved plan documents uncommitted at branch
creation and committed as `b603cd11`. Protocol/dummy baseline: 14 tests. Adapter,
CPAL/mock native host and worklet baseline: 62 tests. The built worklet's Node
artifact/import contract passes. `iceoryx2 = 0.10.0` (MIT OR Apache-2.0; Rust 1.89
minimum) passed a Linux two-process bounded, non-overwriting record probe; its
supported platform implementation covers Linux/macOS/Windows, whose runtime
integration is verified later. See [evidence](VALIDATION.md#native-engine-ipc-evidence).

### Stage 1 — Extract common transport machinery

- [x] Create `shoop_rt_transport`; move the existing client state/credits,
      interfaces, queues and in-process/record adapters out of `shoop_rt_protocol`.
      Leave logical schemas and the canonical codec in the protocol crate.
- [x] Update engine, backend, native/worklet hosts and their fixtures to use the
      shared interfaces. Extract a reusable carrier-contract harness from the
      existing in-process/record tests for use by all implementation crates.
- [x] Enforce the crate dependency rules and update affected READMEs/tracing
      inventory. Keep this step a structural refactor with unchanged semantics.

**Verify:** protocol codec and shared transport suites; in-process backend/timing
contracts and native-host tests; affected Node/Wasm checks and worklet contract.
Confirm no required serialization in the in-process path, no engine/application
dependencies in protocol/transport, and no duplicate client/server implementation.

### Stage 2 — Extract the MessagePort transport

- [x] Create `shoop_rt_transport_message_port` with both record-port halves and
      reusable MessagePort batching, delivery and error glue. Reuse the common
      record adapters and framing; preserve existing browser delivery behaviour.
- [x] Remove the extracted implementations from `shoop_rt_host_worklet` and
      `browser_rt_audio.rs`; keep worklet ABI/preparation, audio/MIDI device lanes
      and rendering in the host, and app settings/orchestration in the app.
- [x] Isolate browser-binding dependencies behind the main-thread feature and
      expose a worklet-side build without them. Extend the carrier contracts and
      dependency audits to the new package and both feature configurations.

**Verify:** main-thread batching/closure tests, worklet and simulated MessagePort
contracts, applicable Node tests, import-free built worklet artifact and one actual
browser core-workflow smoke. The main-thread and worklet halves build with their
intended features; the transport depends on no engine or application crate.

### Stage 3 — Host boundary, setup schema and terminal errors

- [x] Expose the native endpoint independently of `InProcess`, preserving existing
      startup, host ownership, control lead and teardown behaviour.
- [x] Define host discovery/config/start/stop records and codecs, including bounded
      discovery pages, warnings, version rejection and startup errors in
      `shoop_rt_protocol::host`. Hosts dispatch these separately from engine
      messages; keep carrier-private bootstrap out of that schema.
- [x] Add transport-error reporting through the record/client/backend boundary and
      terminal failure of pending mutations and bulk reads. Preserve idle polling
      and reuse the same error path across all concrete transports.

**Verify:** focused protocol/adapter/native-host tests, malformed bootstrap records,
EOF without outbound traffic, full queues and setup errors; existing in-process
contracts and applicable Node/Wasm checks still pass. Heap guards cover endpoint
and render work affected by the host refactor.

### Stage 4 — Socket transport

- [x] Create `shoop_rt_transport_socket` with both client/server ends,
      authenticated loopback setup and bounded record I/O. Keep OS connection setup
      here and shared engine/host messages in the protocol/transport crates.
- [x] Wire the record client/server to it and add a real subprocess carrier fixture.

**Verify:** shared protocol tests over sockets; fragmented headers/bodies, coalesced
records, partial writes, full queues, malformed/oversized frames, wrong versions,
EOF and child exit. Reliable results/bulk chunks are retained and telemetry follows
existing discard semantics. No worker or buffer remains after fixture teardown.

### Stage 5 — Shared-memory transport

- [x] Create `shoop_rt_transport_shared_memory` with both client/server ends;
      implement fresh mapping setup, validated initialization, queue publication
      and recycling, notifications and cleanup through the selected safe dependency.
- [x] Wire the same record client/server and subprocess fixture to shared memory;
      retain the control channel for startup, liveness and shutdown only.

**Verify:** the same protocol suite over actual shared mappings; address-independent
access, full/empty and wrapped queues, layout/version/size rejection, concurrent
publication, stalled readers/writers and peer death during a transfer. Check
dead-connection handles/names are reclaimed without invalidating a surviving
engine, and model publication/recycling ordering. Demonstrate ordinary records,
including bulk media, do not travel over the control channel.

### Stage 6 — Headless process host and supervision

- [ ] Add `shoop_rt_host_process` with a launch/supervision library and engine
      executable. Compose `shoop_rt_host_native` with either transport through
      common interfaces; keep process launching out of the carrier crates.
- [ ] Add child-side host-message dispatch, discovery/configuration and the control
      loop; open dummy or CPAL/midir devices only in the child. Engine messages
      reach the existing common engine endpoint without another semantic adapter.
- [ ] Implement parent launch/readiness, one-session enforcement, peer-loss handling,
      an independent explicit shutdown signal, bounded shutdown/rollback and child
      reaping for both carriers. Parent death does not kill the running engine.
- [ ] Keep rendering independent of stalled/disconnected IPC, retain the endpoint
      after connection loss and pump completions without ingress or absent readers.

**Verify:** two-process dummy and CPAL/midir mock tests for each carrier: startup,
device/setup warnings, idle operation, client stalls, active capture/playback,
normal exit, parent crash, engine crash, render fault, malformed data and partial
startup failure. After carrier/control-channel loss and parent crash, verify the
healthy engine keeps advancing, playing/recording and completing admitted work
without accepting new control; teardown must not silence it or leak abandoned
transfers. Test explicit shutdown independently of the failed carrier, including
MIDI-reset ordering. Harnesses externally terminate intentional survivors after
verification, bound waits and clean up all test processes. Real render faults
still follow the existing fault policy.

### Stage 7 — App selection and core workflow

- [ ] Add `audio.engine_transport` and `--engine-transport` with the three choices;
      default to `in_process`, apply only to native `new_engine`, require restart
      and explicitly reject IPC selection in unsupported environments.
- [ ] Replace local-host ownership with child supervision for IPC; move its discovery
      calls across bootstrap, surface errors/warnings and wait for initial readiness
      before normal app startup. Do not silently switch carriers after failure.
- [ ] Add a fatal disconnect state that keeps the app window/event loop alive,
      visibly explains that recovery is unavailable and disables engine operations.
      Supervisor/drop handling must preserve the engine until explicit app shutdown.
- [ ] Run shared backend and timing contracts plus cooperative app workflows through
      both carriers; keep current refused capabilities and structure-only New Session.

**Verify:** application tests for settings/CLI precedence, capability gating,
fatal disconnect display with the app still open, refusal of later operations and
graceful engine shutdown when the failed app is explicitly closed; both core
workflows with audio and MIDI detail reads. Deterministic subprocess execution agrees with
in-process samples/MIDI/outcomes for timely admitted commands. No reconnect or
automatic restart occurs after loss.

### Stage 8 — Packaging, documentation and CI

- [ ] Build and package the companion executable next to the app, including required
      native runtime dependencies; resolve it without depending on cwd or `PATH`.
- [ ] Extend Linux/macOS/Windows CI with both subprocess carrier suites, CPAL/midir
      mocks and extracted-package dummy smokes. Retain browser and legacy gates;
      discover tests in the new portable/MessagePort crates and extend native
      dependency audits without pulling IPC into Wasm checks.
- [ ] Update `ARCHITECTURE.md`, `PLAN.md`, user setup/limitations, new crate READMEs,
      tracing inventory and `VALIDATION.md` with actual implementation status,
      fatal app errors, autonomous disconnected engines and external termination
      after app crashes.

**Verify:** packaging checks and launches from extracted archives, including paths
with spaces/non-ASCII characters; docs links, boundary/tracing/test-usage policies
and native/Wasm dependency checks. Missing or incompatible companion binaries
produce a clear startup error. Both carrier jobs are required, not optional skips.

### Stage 9 — Final end-to-end validation and delivery

- [ ] On the final implementation tree run the complete retained workspace suite,
      shared contracts for in-process, MessagePort, socket and shared-memory
      carriers, both IPC backend/subprocess failure suites, release
      host/endpoint heap guards, and affected ownership/model Miri cases.
- [ ] Exercise the real app's full core workflow over both carriers on dummy and
      a real Linux CPAL device; exercise MIDI routing/cleanup with mock ports and
      real hardware where available. Record hardware gaps explicitly.
- [ ] For both carriers demonstrate fatal app disconnect without app exit, continued
      healthy engine playback/recording after connection loss and app crash, no
      reconnection/control, and graceful explicit shutdown of the failed app's engine.
- [ ] Run existing Node/worklet and application browser regression gates, including
      the manual Chrome smoke. Record newly run source-pinned results; do not copy
      prior milestone counts as this milestone's evidence.
- [ ] Collect Linux/macOS/Windows subprocess and extracted-package results for both
      carriers. Verify the crate dependency/feature matrix and absence of duplicated
      protocol/connection machinery. Map I1–I9 to evidence in `VALIDATION.md`, with
      failures, skips and unavailable environments stated. Do not mark an unmet
      criterion complete.
- [ ] Commit meaningful milestones, push `feat/engine-ipc`, and open/update a PR
      against `feat/loop-execution-layer` (or the default branch if that base has
      merged). Describe the final scope, validation and remaining limitations.
- [ ] Work required CI to green on the final PR head and inspect representative
      artifacts. Check automated reviews/comments; repeatedly address applicable
      feedback, explain justified disagreements and request re-review until the
      reviewer approves. If unavailable, report that explicitly; CI is not review.
- [ ] Recheck final-head CI after any final commit, update plan/evidence and report
      the PR/head, carrier/platform results and remaining limitations. Do not merge
      without a separate request.

**Verify:** I1–I9 have source-matched evidence and required final-head CI is green;
review status and any unavailable verification are explicit. This plan does not
close unrelated historical acceptance gaps.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
