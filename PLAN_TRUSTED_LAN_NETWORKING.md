# Trusted-LAN endpoint connections and host management

## Status and authorization

Revision authorized by the user on 2026-11-02: remove host/session identifiers,
use identical fresh synchronization for every connection, and expose host
settings/status through management and the existing UI. This supersedes the
original pinned-profile and cross-connection recovery requirements. Earlier
implementation and validation evidence remains in Git history.

Branch `feat/trusted-lan-networking`; PR https://github.com/SanderVocke/shoopdaloop/pull/912
against `feat/backend-replacement` (base `202fe46f`). Implementation and local
validation are complete. Final delivery is gated by the PR's live current-head
checks as specified below. Leave it unmerged.

## Goals, scope and immutable acceptance criteria

1. Native Linux/macOS/Windows UIs connect over explicit trusted-LAN TCP data and
   management endpoints. Profiles contain endpoints and credential references,
   with no host/session IDs. A profile remains reusable after a server restart
   or replacement at those endpoints. Local defaults remain loopback.
2. Every attachment performs the same authentication/version check, acquires
   exclusive control and receives a coherent current snapshot before writes.
   Clear previous projections, transfers and pending request assumptions; never
   replay commands, initialize server state or restart scripts on connection.
3. Retain connection-scoped authority so stale traffic/shutdown cannot affect a
   replacement controller. A healthy controller cannot be evicted. Accepted host
   work continues independently; interrupted requests have uncertain outcomes.
   New clients observe current host state/work rather than assume continuity.
4. Host management reports requested/effective host settings, runtime status and
   diagnostics. The existing UI displays connected-host information separately
   from local user preferences. Share driver/pool configuration contracts and
   validation. Settings that need restart are explicit; no automatic device or
   pool reconfiguration. A separate host-management GUI remains future work.
5. Preserve bounded portable stream/handshake/heartbeat/session progression:
   `no_std` + `alloc`, both Cortex-A library targets, injected time, finite queues
   and work budgets, and no networking/allocation/file work on render callbacks.
   Default heartbeat remains 1 s / 5 s; network snapshots <=30 Hz. Keep the
   deterministic 1 MiB/s / 20 ms RTT / 10 ms pump control-latency gate <250 ms.
6. Preserve remote controls, exact audio/MIDI and >32 MiB segmented persistence,
   pre-commit cleanup, once-only accepted work, UI-owned files, UI-only exit and
   explicit confirmed host shutdown. Browser, local socket/shared-memory and RT
   heap/boundary checks remain regression gates.
7. Finish with a PR into `feat/backend-replacement`, green final-head CI and
   honest platform evidence. No actual mixed-OS hardware or RTOS execution claim.

TLS, advanced authentication, UDP, public networks, browser remote connections,
automatic discovery, multiple controlling UIs, audio streaming and a standalone
management GUI remain out of scope.

## Design rules

- Endpoint tuples select a target. Do not introduce a replacement persistent
  server identity or a special recovery attachment mode.
- Short-lived leases authorize only their live controlling connection. Native
  hosts supply fresh connection nonces; the portable layer does not need OS RNG.
- Request IDs and revisions correlate current work and coherent transfers; they
  do not establish continuity across connections. Host jobs are host-owned state.
- Preserve local PID registry cleanup as a native concern, using endpoints and
  process metadata instead of application host/session identity.
- UI settings remain local launch defaults. Connected-host settings are queried
  from the host. Native JSON and UI preferences adapt the same configuration
  types. Host/device changes requiring restart are reported, not applied as a
  side effect of attachment. Optional management windows will be ordinary clients.
- Version the incompatible wire change deliberately and update canonical fixtures.
- Keep the existing token check and native credential permission validation.

## Stages

- [x] R0: inspect current protocol, authority, configuration and native/UI paths;
  record this user-authorized revision and preserve the existing PR target.
- [x] R1: remove host/session IDs from protocol, clients, service, profiles,
  registry, CLI and greetings; retain fresh connection authority. Update typed,
  socket and browser consumers and canonical wire fixtures. Verify portable and
  focused native/client/service suites, warning-denying build and Cortex-A builds.
- [x] R2: unify fresh attachment semantics, including restart at the same endpoint,
  clean projection/transfers, no replay/initialization and stale-owner fencing.
  Replace continuity-dependent tests/probes with current-state observation and
  explicit uncertain outcomes. Verify restart/replacement and accepted-work cases.
- [x] R3: add bounded host settings/status queries, effective settings/diagnostics
  and existing-UI display, reusing shared settings validation. Verify queried
  settings belong to the target and cannot overwrite local preferences or mutate
  devices during attachment. Keep extra GUI and driver hot swapping out of scope.
- [x] R4a: update setup/profile/configuration documentation and PR usage examples;
  run full native suite, >32 MiB LAN/local/shared-memory probes, browser checks,
  Cortex-A builds, formatting, tracing/test policies and boundary audits.
- [x] R4b: push milestone commits and inspect automated review. No code-review
  submissions or inline feedback arrived at the delivery checkpoint; no approval
  is claimed. Leave unmerged.

### Mandatory live delivery gate

Before handoff, every required check on the PR's **current head** must pass.
Fix any arriving CI/review failure and repeat verification. This gate is tracked
by the live PR checks and final PR validation report, so recording completion does
not itself require another source commit and another identical platform run.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Baseline evidence

Before this revision: full native suite 2507 passed / four intentional canaries
skipped; both Cortex-A builds, local Chromium (zero xruns), dependency/RT guards
and large LAN persistence passed. Previous CI passed Linux/Windows/web debug and
release, coverage, Docs, CodeQL and bare-metal. macOS caught a dummy-renderer
wall-clock throughput assumption; the corrected recording-continuity test passes
locally, including a 15 ms Linux timer-slack experiment. Current revision requires
fresh final-head checks after implementing the new contract.

## Endpoint revision evidence

- Protocol version 10 removes host/session identity from profiles, greetings,
  attachment and management. Data connections receive fresh control nonces and
  operation namespaces. Native PID discovery remains separate from LAN profiles.
- Every remote connection uses the same fresh-snapshot path; configured scripts
  are listed disabled and local preference saves do not alter the remote host.
- The same saved profile and Client reconnect to a restarted host at the same
  ports, clear pending state, accept a lower fresh revision and reject the old
  shutdown lease. Exporting a matching existing profile is idempotent.
- Shared HostSettings validation is used by native JSON and local native launch.
  Bounded management reports include requested/effective settings, current host
  status, diagnostics, restart requirements and pending host-work counts. The
  existing UI exposes Session → Connected host settings… with explicit refresh.
- Full native CI-profile suite: **2511 passed; four intentional canaries skipped**.
  Focused portable/native suites and segmented-report backpressure tests pass.
  Both Cortex-A library builds, warning-denying workspace build, formatting,
  tracing/test policy, four-target dependency audit and 17 boundary tests pass.
- LAN packaged workflow and >32 MiB exact audio/MIDI persistence pass, including
  abandoned transfer cleanup and fresh-snapshot observation of accepted work.
  Browser and final-head platform CI remain delivery gates.

### Final local delivery gates

- Chromium hosted workflow passes with zero xruns after the endpoint revision.
  Local socket and shared-memory workflows and >32 MiB persistence probes pass,
  alongside the LAN equivalents. All report exact audio/MIDI round trips.
- The real remote-profile UI test now opens/renders the connected-host panel,
  saves different local audio preferences and verifies the host remains at 48 kHz,
  3 ms smoothing and Master auto-connect enabled. It then retains recording
  across UI replacement and confirms explicit shutdown. This regression passes.
- First endpoint implementation CI passes coverage, Codecov patch/project, Docs,
  CodeQL and bare-metal gates. Native/browser matrix completion and final-head
  review remain tracked by PR #912's checks.

### Final UI boundary and platform checkpoint

- All platform checks at `c5c90d42d2a2da165247a15274c46c02dcc501ee` passed:
  https://github.com/SanderVocke/shoopdaloop/actions/runs/41462251480.
  This includes Linux/macOS/Windows and web debug/release, coverage, bare-metal,
  canonical wire fixtures, package workflows and local/shared-memory regressions.
  Docs, CodeQL and Codecov patch/project checks also passed.
- Final review found session-specific widget drafts and platform file callbacks
  surviving runtime replacement. All three UI connection paths now share one
  reset: cancel old readers, replace callback channels, clear session dialogs,
  request correlations, transient edit history/focus and pending startup actions.
  Preserve local settings/drafts, tracing artifacts and finishing file writes.
- Final native suite: **2513 passed; four intentional canaries skipped**.
  Explicit regressions verify old callbacks cannot target a new connection,
  readers release their reservation before reuse, finishing writes retain their
  reservation, and local preference drafts survive session-widget reset.
- Browser rebuild and hosted Chromium workflow pass with zero xruns after this
  final UI refinement. Formatting, warning-denying build, test policy, tracing
  inventory and 17 boundary-policy tests pass. The final refinement is subject to
  the mandatory current-head gate above before handoff.
