# Trusted-LAN UI connections with a portable server connection layer

## Status and base

Execution in progress on `feat/trusted-lan-networking`. Written under
[write_plan.md](.agents/prompts/write_plan.md).

- Integration branch: `feat/backend-replacement`, refreshed and inspected at
  `202fe46f766cba9bc7e63969e93e0932917430bd` on 2026-11-02.
- Implementation branch: create `feat/trusted-lan-networking` from the latest
  integration head. Compare any intervening changes with this baseline first.
- Deliver a PR into **`feat/backend-replacement`**, with passing final-head CI.
  Do not target `master` or merge the PR without instructions.

## Updated findings

The socket carrier already transports bounded records over TCP, with retained
partial writes and incremental reads. `shoop_transport_socket::setup` deliberately
rejects non-loopback bind addresses, peers and destinations. Its token handshake
and record pump use `std::net`, wall-clock deadlines and sleeps.

`shoop_host` supplies two endpoints: session data and management. Management is
currently fixed to ephemeral loopback. Its registry contains local PIDs and
credential paths; `HostEntry::read` rejects non-loopback endpoints. Session data
handling, upload progression and connection retirement still live in the native
host. The service owns the exclusive controlling generation, but has no network
heartbeat or lease-expiry policy.

Two changes since the earlier analysis materially affect implementation:

- `c8268c60` restored core persistence and media I/O. Remote connections must carry
  the current segmented upload/capture protocol, including images above 32 MiB,
  pre-commit cleanup and accepted-commit reconciliation. Files, dialogs, codecs and
  resampling remain client-side. Reuse the
  [persistence contract](docs/native_session_recovery/persistence-contract.md).
- `202fe46f` made the engine and session-service libraries usable with `no_std`
  and `alloc`. `shoop_transport` is already `no_std`; the application protocol,
  service and RT adapter have portable configurations. The client projection is
  still a desktop/browser crate; portable upload progression will not depend on it. Existing CI builds
  the server libraries for `armv7a-none-eabi` and `aarch64-unknown-none`.

Relevant implementation areas are `shoop_transport::{framing,buffer}`,
`shoop_transport_socket::{setup,Port}`, `shoop_host::{serve,connection,registry}`,
`shoop_app_protocol`, `shoop_app_client`, `shoopdaloop::{application,recovery,
persistence_io,app_args}`, and the dependency-boundary scripts.

Planning baseline: the focused socket, application-protocol and client suites
passed **32 tests** at this head. Cortex-A targets were not installed locally;
their existing CI configuration was inspected, not locally executed.

## Goals and scope

Support one native desktop UI controlling an independently running host across a
trusted LAN using TCP. Preserve the current session, persistence, media, recovery
and explicit-shutdown semantics on Linux, macOS and Windows.

Make the byte-stream and session-connection machinery reusable by a future
Cortex-A RTOS composition root. Supply the native TCP implementation now; prove
portability through bare-metal compilation and a host-driven simulated adapter.
No RTOS implementation, board support or network stack is required.

Keep the existing manually provisioned 32-byte token check. TLS, certificates,
accounts, pairing and advanced authentication are out of scope. Also exclude UDP,
browser remote connectivity, automatic discovery, NAT traversal, multiple
controlling UIs, live audio streaming, new disk formats and unrelated parity work.
Existing browser and local modes remain regression targets.

## Immutable acceptance criteria

These goals and criteria require explicit user approval to change.

1. **Explicit LAN connections:** a user can start a host on a configured interface
   with configured data and management ports, then attach from a separate machine
   using a saved profile. Defaults remain loopback. Internal engine/process
   sockets retain their local-only setup policy. IPv4 and IPv6 literals work;
   native profiles also accept DNS hostnames with a bounded connection deadline.
2. **Simple credentials and identity:** both endpoints require the existing token;
   its bytes stay out of arguments, profiles and logs. Profiles reference a
   client-local credential file. Host/session identities and protocol version are
   verified before control; wrong credentials, wrong identities, incompatible
   versions and a second live controller are refused without eviction.
3. **Reusable server connection handling:** framing, partial I/O, bounded queues,
   handshake progression, heartbeat/deadline policy and session data/management
   progression compile with `no_std` plus `alloc` for both Cortex-A targets. The
   same implementation runs through native TCP and a simulated non-OS adapter.
   The reusable layer has no thread, socket-handle, filesystem, process, OS-clock,
   DNS or particular RTOS/network-stack dependency.
4. **Preserved RT boundary:** networking, serialization, allocation and file work
   stay outside render callbacks and the bounded engine endpoint. A stalled peer
   cannot stall rendering. Existing heap guards and engine boundary checks pass.
5. **Predictable loss and recovery:** default heartbeat interval is one second
   and response timeout is five seconds, configurable with validated bounds.
   Missing responses retire the connection by its deadline plus one host pump.
   Playback/recording and accepted work continue; held client piano notes receive
   the existing disconnect cleanup. Reattachment acquires a fresh generation,
   obtains coherent state/results, and never automatically replays commands or
   restarts scripts. An expired owner cannot control or shut down its replacement.
6. **Current-core workflows:** remote controls, waveform/MIDI reads, session
   save/load and audio/MIDI import/export work through the existing APIs. A
   segmented image above 32 MiB round-trips exact audio and exact MIDI data within
   existing capacity limits. Pre-commit loss discards staging; an accepted commit
   finishes once and is reconciled after loss. UI files never require a server
   filesystem path. Explicit UI-only close and confirmed host shutdown both work.
7. **Bounded progression and responsiveness:** connection count, queue bytes,
   handshake buffers and work per pump have explicit finite budgets. The stream
   framer/queue pump does not allocate after initialization. Service/projection
   and persistence allocations remain allowed on the control task within their
   existing budgets. Periodic network snapshots default to at most 30 per second;
   initial synchronization and operation confirmations are not held behind a
   periodic timer. Full snapshots remain coherent; incremental state is deferred.
   In the deterministic acceptance fixture, an ongoing upload/download cannot
   starve heartbeats or controls: a control round-trip completes within 250 ms
   of simulated time at 1 MiB/s in each direction, 20 ms RTT and 10 ms pump steps.
8. **Cross-platform and compatibility evidence:** Linux/macOS/Windows CI runs
   the native two-process connection/recovery suite. Canonical wire fixtures agree
   across these jobs and use fixed-width values and defined byte order. Both
   Cortex-A builds and existing in-process, local socket, shared-memory and browser
   checks pass. Report actual mixed-OS machine testing separately if available;
   compilation and same-OS CI are not claims of RTOS or mixed-hardware execution.

## Design rules

- Keep the network boundary between UI and authoritative session service. Leave
  service-to-engine communication and render ownership unchanged.
- Extend `shoop_transport` with protocol-independent stream state, reusing its
  framing and buffers. Accept byte slices or a minimal nonblocking I/O interface
  with explicit progress, would-block, EOF and failure outcomes. Preserve whole
  records on retries and distinguish idle from terminal loss. Do not duplicate
  framing in a future RTOS adapter.
- Extract reusable application connection handling into a small
  `shoop_app_connection` crate (`no_std` + `alloc`). It may depend on portable
  service/protocol/client contracts and `shoop_transport`, but not concrete
  carriers, engine/device hosts or Lua. Extract server data/management pumping and
  client segmented-upload progression from `shoop_host`; avoid a second protocol
  implementation. The host ticks the service once, independently of client count.
- Hosts provide elapsed monotonic time, scheduling, I/O, entropy or a provisioned
  token, connection admission and storage. Cooperative steps never sleep or wait
  internally and return progress/deadline information. Use saturating elapsed-time
  arithmetic; an RTOS tick adapter owns counter rollover conversion.
- Native code owns TCP/DNS, filesystem permissions/Windows ACLs, process discovery,
  cleanup and UI integration. Preserve the socket wrapper's stream access needed
  by shared-memory liveness. Existing local-only setup APIs remain local-only;
  LAN listeners/connectors use an explicit separate policy.
- The connection layer is a control-task component. `no_std` with `alloc` does not
  imply heap-free service execution or hard-real-time networking. Cortex-A hosts
  still provide an allocator and the atomics required by the existing engine.
- Keep token bytes and the existing simple handshake; make its progression
  cooperative and deadline-bound. Provisioning a token is a platform concern.
  A remote profile contains no server PID, registry-cleanup authority or server
  filesystem credential path. Separate bind addresses from advertised addresses;
  never advertise wildcard addresses as client destinations.
- Add heartbeat messages to the application wire family and advance its version
  deliberately. Consume heartbeats in the connection layer before semantic
  client/service dispatch. Replies must match an outstanding connection-local
  probe; queued outbound bytes alone are not evidence of peer liveness. Use the
  same host/session checks on management replies as on session attachment.
- Budget control and bulk work in both directions, including persistence credits.
  Prioritize controls/heartbeats only where the protocol permits interleaving;
  never reorder chunks, accepted operations or revision barriers. Coalesce only
  unsent periodic snapshots and never drop a partially sent snapshot. Bound
  read-ahead and queued bytes so a whole file cannot enter the socket backlog.
  TCP head-of-line blocking remains; do not promise latency on an arbitrary link.
- Keep existing persistence ownership, operation IDs, epochs, transfer ordering,
  limits and exactly-once accepted-commit semantics. A timeout is not proof of
  rollback or shutdown completion. Keep capacity refusals explicit.

## Staged implementation

Execute stages in order. Each stage includes verification and a commit; later
stages must preserve earlier contracts.

### Stage 0 — implementation branch and baseline

- [x] Refresh `feat/backend-replacement`, inspect changes since the pinned base,
  then create `feat/trusted-lan-networking`. Carry this plan onto that branch.
- [x] Read current build/test/CI guidance; record the baseline focused tests,
  boundary checks, native recovery/persistence probes and both Cortex-A builds.
  Record existing failures separately without weakening gates.
- [x] Map AC1–AC8 to named tests/probes and document initial connection/queue/pump
  budgets and the deterministic responsiveness fixture parameters.
- [x] **Verify:** baseline evidence and acceptance mapping are recorded; no
  implementation is credited to the pre-existing planning tests.

### Stage 1 — portable byte-stream and token-handshake core

- [x] Move stream framing, partial-write retention and queue progression into
  `shoop_transport`; add a bounded cooperative token-handshake state machine.
  Preserve current local carrier behavior and the 1280-byte record ceiling.
- [x] Make `shoop_transport_socket::Port` a thin native adapter over that core.
  Preserve lane/backpressure semantics and shared-memory liveness access. Keep
  entropy, TCP setup/options and error diagnostics outside the portable core.
- [x] **Verify:** use a scripted adapter to exercise fragmented/coalesced frames,
  partial token/ack writes, would-block, interruption, EOF, malformed lengths,
  queue saturation and deadline expiry. Assert finite per-step work and no stream
  pump allocation after preparation. Run existing socket/shared-memory tests and
  build the core for both bare-metal Cortex-A targets.

### Stage 2 — portable application connection pump

- [x] Create `shoop_app_connection` and extract data/management message progression,
  snapshot/media streaming and client upload validation/credit handling from the
  native host. Accept a `RecordPort`, elapsed time and explicit budgets.
- [x] Keep host/device shutdown execution in the composition root; portable
  management processing reports actions/results through the existing service
  authority. Keep local shared-memory attachment restrictions intact.
- [x] Extend dependency-boundary rules/tests and the existing bare-metal CI job
  to include the new crate with default features disabled throughout its graph.
- [x] **Verify:** the portable pump runs the service workflow with a simulated
  carrier and fake clock. Native recovery, large-image persistence and the
  withheld-upload-acknowledgment test retain their semantics. Both Cortex-A
  library builds pass without importing `std` through feature unification.

### Stage 3 — heartbeats, deadlines and ownership cleanup

- [x] Add versioned probe/reply records and implement client/server liveness in
  the portable connection layer. Validate heartbeat settings and use remaining
  deadlines across setup, greeting, attachment, management and drain phases.
- [x] On expiry retire transport ownership and call the existing service detach
  path once. Preserve accepted work, staged-upload cleanup, stale-authority
  fencing and independent management shutdown behavior.
- [x] **Verify:** fake-clock tests cover bidirectional and one-way blackholes,
  delayed/stale replies, large elapsed steps, a suspended UI, missing upload
  acknowledgments, reconnect-before-old-traffic-arrives and lost commit/shutdown
  replies. Assert AC5 and pre/post-commit ownership without wall-clock sleeps.

### Stage 4 — LAN endpoints and remote profiles

- [x] Add explicit LAN opt-in and independently configurable data/management bind
  and advertised endpoints to `shoop_host` and the application host-only wrapper.
  Keep default/local engine socket guards; reject remote shared-memory profiles.
- [x] Introduce versioned remote profiles, separate from `HostEntry` and the local
  registry. Include addresses, expected host/session identities and a client-local
  token-file reference. Supply a profile template/export path and token generation
  instructions so users do not need to hand-author identity integers.
- [x] Share native DNS/connection deadline handling across attach, reattach and
  management. Bound resolver admission and discard late results; do not spawn
  unbounded resolver threads on retries. Bound pending handshakes and total
  connections. Keep credential-file protection checks; remote profiles never
  participate in PID pruning.
- [x] **Verify:** two-process connections use a real non-loopback interface in
  required Linux CI; exercise IPv4, IPv6 and DNS paths without silently skipping
  unavailable required coverage. Other native jobs run the same portable/native
  contracts with recorded address-family coverage. Test wildcard publication,
  endpoint conflicts, wrong token/identity/version and occupied-controller refusal.

### Stage 5 — traffic budgets and desktop integration

- [x] Implement configurable periodic network snapshot cadence, retaining prompt
  initial and confirmation state. Apply bounded scheduling for controls, probes,
  snapshots, media and segmented persistence in both directions. Avoid adding a
  delta protocol or separate bulk socket unless evidence requires revising design.
- [x] Add explicit remote-profile opening to CLI and the Session UI, with connection
  progress, loss diagnostics, manual reattachment and existing close/shutdown
  actions. Reuse existing recovery and persistence UI paths. Keep files, scripts
  and controller bindings client-local and startup initialization fresh-session-only.
- [x] **Verify:** meet AC7 in the deterministic throttled-link fixture, including
  concurrent controls and large-transfer progression; assert memory/work budgets.
  Exercise remote UI attachment, file dialogs, cancellation, reattachment and both
  exit actions. Verify local settings/registry behavior is unchanged.

### Stage 6 — final end-to-end validation and delivery

- [ ] Run a native two-process remote-profile workflow: create/route/record/play,
  inspect media, save/load and import/export audio/MIDI including a segmented image
  above 32 MiB. Compare exact samples and exact MIDI events/order/frame positions.
- [ ] Inject loss before upload completion, before and after commit admission,
  after commit but before its result, during snapshot synchronization and during
  shutdown. Confirm playback continuity, cleanup, bounded retained resources and
  correct reconciliation through a replacement UI. Verify a healthy owner cannot
  be evicted and a stale owner cannot shut down the replacement.
- [ ] Run the Linux/macOS/Windows CI matrix, canonical wire fixtures, both Cortex-A
  builds, existing local/shared-memory recovery and persistence probes, browser
  regression workflows and RT heap/boundary guards. Record which physical/mixed-OS
  pairings were actually exercised; RTOS runtime validation remains future work.
- [ ] Run required repository checks in its selected build environment:
  `cargo fmt --all -- --check`, `RUSTFLAGS="-D warnings" cargo build --workspace`,
  the complete nextest suite from `.agents/info/test.md`,
  `python3 scripts/check_shoop_test_usage.py`, boundary audits and their tests,
  and `python3 scripts/check_tracing_coverage.py --require-closed`.
  Before committing Rust changes, run `cargo fmt --all` and the warning-denying
  build; run targeted verification at each earlier stage too.
- [ ] Document trusted-LAN setup, both ports, manual credential provisioning,
  identity/version expectations, heartbeat settings, file ownership and the
  future RTOS adapter contract. Record AC1–AC8 evidence and limitations.
- [ ] Push milestone commits and open/update the PR against
  `feat/backend-replacement`. Fix final-head CI failures and rerun affected gates.
  Check for automated review; address arriving feedback and repeat until that
  reviewer approves. If no reviewer responds, report that fact without claiming
  approval or waiting indefinitely. Leave the PR unmerged.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

Routine implementation choices, reversible fixes, stage commits, pushing this work
branch, creating its PR and addressing CI/review feedback are part of execution.
No additional checkpoint approval is needed unless a goal or acceptance criterion
must change. Implement only when execution is requested; this document is the
planning deliverable.

## Execution evidence

### Stage 0 baseline (202fe46f)

- Implementation branch created; no intervening integration changes.
- Focused socket/protocol/client suites: 32 passed. Native dependency-boundary
  audit passed; warning-denying workspace build passed.
- Design clarification: `shoop_app_client` itself still depends on `std`.
  The new portable connection crate returns decoded wire records to the native
  client wrapper instead of depending on that crate. This preserves the goals.
- Initial connection budgets: four active data peers and four management peers;
  four pending authentication peers per listener; per stream bounded lane storage;
  network session record backlog capped at eight records. Each stream pump does
  at most 32 writes or four reads per invocation. Token state is 32 bytes plus
  offsets and a deadline. Control-task and persistence budgets remain distinct.
- Acceptance mapping: AC1/AC2/AC6 use native host/profile tests and workflow probes;
  AC3 uses portable simulated adapters and both bare-metal builds; AC4 uses the
  existing RT guards/boundary audits; AC5 uses fake-clock liveness/recovery tests;
  AC7 uses the deterministic throttled-link fixture; AC8 uses native CI, wire
  fixtures, bare-metal gates and browser/local regression workflows.

### Stage 1 evidence

- Portable stream/framing and cooperative token/ack state now back the native
  socket carrier; existing local-only connection policy remains unchanged.
- Nine transport tests (including allocator-guarded prepared progression), seven
  socket tests and seven shared-memory tests pass. Both Cortex-A stream builds,
  the warning-denying workspace build and test-attribute checks pass.

### Stage 2 evidence

- Native socket/shared-memory workflow and >32 MiB persistence probes pass at
  baseline. Both original Cortex-A server stacks build successfully.
- `shoop_app_connection` now contains generic server/management and upload pumps;
  `shoop_host` retains native device lifetime and shared-memory handoff only.
- Simulated service attach/control/detach passes; all nine native recovery tests
  and the withheld-upload-acknowledgment test pass after extraction.
- Both new Cortex-A library builds, workspace warning-denying build, dependency
  audit, twelve boundary-policy tests and test-attribute check pass. CI includes
  the portable connection crate in the existing bare-metal gate.

### Stage 3 heartbeat milestone

- Wire version 9 adds connection-consumed Ping/Pong records. The portable
  heartbeat uses injected elapsed time and validates configurable interval/timeout.
- Fake-clock tests cover matching/stale/late replies, full carriers, one-way
  blackholes, a suspended peer and expiration followed by replacement authority.
- All focused client/protocol/connection/host tests pass, including nine native
  recovery tests. Workspace warning-denying build and test-attribute checks pass.
- LAN profile wiring and end-to-end remaining-deadline handling follow in stage 4;
  existing local-only transports retain their original behavior.

### Stage 4 evidence

- Explicit LAN listeners and separate data/management endpoints are implemented;
  local-only setup and discovery policies are unchanged. Both endpoints issue an
  identity greeting before accepting management authority or session attachment.
- Profiles carry no PID or server credential path. Native resolution uses one
  bounded worker and four queued requests; late results are discarded. Setup,
  authentication and greeting share the caller's remaining deadline. Four pending
  handshakes per listener progress cooperatively without serial blocking.
- Four LAN tests pass, including actual non-loopback Linux traffic, IPv6, DNS,
  identity/version/token refusal, busy-controller refusal, replacement fencing,
  wildcard/shared-memory refusal and ownerless shutdown. All nine local recovery
  tests and the canonical heartbeat wire fixture pass.
- CLI and Session-menu profile opening are wired into existing recovery/file UI.
  Native application tests compile; workspace build and test-attribute check pass.

### Stage 5 evidence

- Network sessions coalesce unsent periodic snapshots (30 Hz default), preserve
  active transfers and prioritize operation replies between bounded bulk chunks.
  Native LAN stream queues and requested socket send buffers are bounded.
- The deterministic real-service fixture uploads and reads back 1 MiB of exact
  audio over a 1 MiB/s, 20 ms RTT link with 10 ms pump steps. Ordinary controls
  complete in 40 ms during upload and 20 ms during download; heartbeats remain
  live, queued records never exceed eight and periodic snapshots stay <=30 Hz.
  The import commit is separately awaited through its actual engine preparation.
- The real UI remote-profile test preserves recording across UI-only close and
  reattachment, then confirms the close-both action through management.
- Remote-profile packaged workflow and >32 MiB persistence probes pass, including
  exact MIDI, corrupt input refusal, native file output, fresh load/exact resave,
  abandoned upload/capture cleanup and accepted-load reconciliation without replay.
  Package CI now runs these LAN probes on every native platform.
- Focused native/portable suites, Cortex-A connection build, warning-denying
  workspace build and test-attribute check pass.

### Stage 6 validation checkpoint

- Full native nextest suite: **2504 passed; four intentional canaries skipped**,
  using the repository CI profile (four threads; zero retries). Missing physical
  audio/MIDI facilities use the documented `SHOOP_ALLOW_MISSING_BACKENDS=1` mode.
- Both complete Cortex-A server/connection library builds pass. Formatting,
  test-attribute policy, tracing inventory (353 modules) and 17 boundary-policy
  tests pass. The native blackhole test keeps TCP open while discarding traffic;
  its replacement sees the same actively recording loop after lease expiration.
- Local compilation initially filled the 32 GiB volume with generated debug and
  incremental files. Only `target/debug` artifacts were removed. Final local
  builds use `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0` and
  `CARGO_PROFILE_TEST_DEBUG=0`; assertions, optimization level, workload and
  nextest settings are unchanged. The warning-denying workspace build passes.
- Setup/credentials, both endpoints, profile identities and the future Cortex-A
  adapter contract are documented in `docs/trusted-lan-networking.md`.

### Final-review and first-CI corrections

- Local hosted Chromium workflow passes with zero xruns. The complete four-target
  native/browser dependency audit passes. PR #912 targets `feat/backend-replacement`;
  its description includes server/client startup and profile/credential handling.
- Added explicit interruption-during-snapshot and lost-management-reply tests;
  replacement projection and same-operation shutdown reconciliation both pass.
  Remote profiles also retain bounded startup diagnostics for the client UI.
- First-head CI passed bare-metal, web release, Rust coverage (85.50% patch), Docs
  and CodeQL. Windows package probes exposed profile publication before JSON was
  written: the credential/ACL helper created the final path before its contents.
  Profiles now use a completed same-directory temporary file and atomic
  no-clobber persistence. A concurrent reader regression checks complete JSON and
  non-overwrite behavior; all seven native LAN tests pass locally.
- The initial local native checkpoint was 2504 tests; the next checkpoint with
  snapshot/shutdown additions passed 2506. Final native and final-head platform
  gates are being rerun after the atomic-publication fix.
