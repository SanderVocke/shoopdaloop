# Host manager simplification and process discovery

## Status and delivery

Completed on the existing PR; it remains open and unmerged. Written using
[write_plan.md](.agents/prompts/write_plan.md).

- Continue on `feat/host-manager` and update the existing
  [PR #916](https://github.com/SanderVocke/shoopdaloop/pull/916), targeting
  `feat/backend-replacement`. No new implementation branch or PR is needed.
- Inspected head: `c415774190f3a41ebfba15284405157724c38860`; inspected base:
  `3aabf43789708a5640a8f5b368f737651ef1d29d`. Recheck both when execution starts.
- At the initial baseline, CI passed Linux debug/release, Rust coverage, web debug/release,
  bare-metal, Docs and CodeQL. Windows and macOS native tests fail on the log-path
  assertion in `audio_capacity_midi_and_lan_controls_feed_the_shared_configuration`:
  Windows expands short/verbatim paths; macOS resolves `/var` to `/private/var`.
  Subsequent missing replay-artifact failures follow the aborted test stage.
- No submitted reviews or review threads were present at the initial baseline.

## Goals and scope

Make local host management automatic: open the manager, select a running host or
configure a new detached host, then open its UI. Remove key authentication
throughout the native connection paths. Discover hosts through OS process and
socket inspection, keeping discovery state in memory. Connection profiles become
an explicit advanced feature.

This supersedes earlier credential and profile-only discovery requirements for
these native host workflows.
Include Linux, macOS and Windows, normal and packaged launches, socket and
shared-memory sessions, graceful Stop and explicit local Force stop. Preserve the
compact administrative UI and optional rotating logs. Security design, tray
integration, LAN-wide discovery, automatic host restart, multiple controlling UIs
and live audio-driver reconfiguration are outside scope.

## Immutable acceptance criteria

These goals and criteria require explicit user approval to change.

1. **Keyless connections:** hosts accept compatible clients and clients connect to
   compatible hosts without credentials, certificates, key files or trust records.
   Remove authentication from app data, management and the shared engine-process
   bootstrap paths. No dummy token or optional authentication bypass remains.
   Compatibility/version checks and operational session ownership still apply.
2. **Discovery from live OS state:** on all three desktop platforms, list compatible
   standalone hosts visible under normal OS inspection permissions, including hosts
   started by the manager, CLI and detached looper launcher. Find their actual listening
   sockets by PID; retain OS-assigned ports. Discovery uses no registry, advertisement
   files, remembered host list, fixed port/range or additional discovery service.
3. **Harmless inspection:** discovery and status probes never acquire a controller,
   consume a shared-memory attachment, reset a session or interrupt playback,
   recording or an existing UI. Incompatible and unresponsive processes are never
   presented as ready compatible hosts.
4. **File-free normal workflow:** New host, detached sibling launch, manager Open UI
   and local shutdown work without creating or selecting a connection profile or
   host configuration file. The form configures audio, MIDI, capacity and connection
   settings. Only explicitly requested logs, profile exports, session saves and
   other existing user-requested outputs create their respective files.
5. **Host lifetime:** closing or terminating the manager leaves ready hosts running.
   A fresh manager rediscovers the same processes and sessions without saved state.
   Reopening the manager does not launch replacement hosts. Exited hosts disappear
   through fresh OS observation, without stale-file cleanup.
6. **Advanced profiles:** import/export and manual address connection remain available
   under Advanced. Profiles are keyless, optional and never automatically saved or
   restored. Local discoveries and manual connections to the same host appear once.
7. **Correct termination:** normal Stop retains orderly cleanup and live-controller
   protection. Explicit Force stop can terminate an identified local host when the
   OS permits it, including an unresponsive host. Changed process identity refuses
   the action; remote descriptor PIDs cannot select local processes for termination.
8. **Complete delivery:** the existing PR contains the final implementation, current
   documentation, refreshed native screenshots and verification evidence. Required
   and relevant CI checks, including patch coverage, pass on the final head.

## Design rules and constraints

- Identify a local process by PID, creation/start identity and executable path;
  correlate protocol replies with that identity. Runtime instance identifiers are
  public bookkeeping, not credentials, and exported profiles must not pin them.
- Recognize standalone host executable names and installed/build companion paths.
  The looper launcher's actual `shoop_host` child participates. Ordinary in-process
  looper sessions without external endpoints are outside the discovery target.
- Inspect only listening sockets belonging to candidate host processes. Handle IPv4,
  IPv6 and wildcard/local-interface bindings. Do not scan unrelated sockets or guess
  ports. Reads of virtual OS interfaces such as Linux `/proc` are permissible.
- Distinguish inspection failure from process exit. Surface OS permission and API
  errors; keep advanced manual connection usable. Do not substitute registry files
  or a reserved port range if a platform implementation proves difficult.
- Run enumeration and bounded, concurrent probes off the UI and audio/render threads.
  Coalesce refreshes, ignore obsolete scan results and preserve selection by process
  identity. Batch work so additional running hosts are not silently omitted.
- Keep native inspection dependencies out of browser/no-std graphs. Respect existing
  `forbid(unsafe_code)` boundaries: use safe OS wrappers, or isolate unavoidable FFI
  behind a small reviewed native-only helper. Shared transports remain independent
  of app semantics and native process APIs.
- Classify compatibility, endpoint role and probe/attach intent before session
  admission. Today `serve.rs` calls `shared.take()` on data admission; a discovery
  connection must never reach that operation.
- Retain protocol bounds, deadlines, heartbeats, exclusive controller ownership,
  operation IDs and generation-based stale-owner handling. Removing authentication
  must not remove these session/lifetime correctness mechanisms.
- Keep advanced remote connection working without local process-inspection rights.
  Use actual local socket addresses for local inspection and advertised addresses
  where appropriate for remote clients.
- Keep default form settings in memory. Do not introduce automatic saved launch
  presets or host history. Simplify log I/O to ordinary rotating files and remove
  authentication-specific permission/ACL machinery. Existing user-created files
  are not migration cleanup targets.

## Staged implementation

### Stage 0 — Establish the existing PR baseline

- [x] Confirm the worktree, branch and PR head; fetch and merge the latest
  `feat/backend-replacement` into this branch as needed, preserving existing work.
- [x] Recheck checks and review threads. Correct the known canonical-path assertion
  using a canonical temporary-directory parent for the expected log destination,
  and inspect any additional failures
  from the exact current head before changing product behavior.
- [x] Record baseline evidence and commit the baseline correction separately.

Verification: the affected form test passes with canonical/non-canonical temporary
directories on macOS and Windows; capture the relevant CI results. Follow the
repository build/test guidance and inspect logs before classifying a failure.

### Stage 1 — Prove OS process/socket inspection on each platform

Depends on Stage 0.

- [x] Add a native inspection abstraction returning process identity, executable
  information and owned listening TCP addresses. Evaluate a small safe dependency
  against the actual platform APIs before choosing it.
- [x] Implement Linux process/socket-inode inspection, macOS `libproc` process/FD
  inspection, and Windows process information plus owner-PID TCP tables, directly
  or through verified wrappers. Include both IPv4 and IPv6.
- [x] Add real child-process fixtures binding arbitrary OS-assigned sockets, covering
  multiple processes, path aliases, exited processes and unavailable inspection.
- [x] Establish the process-identity/termination abstraction, using stable OS handles
  where available and rechecking identity before a termination request.

Verification: native tests on Linux, macOS and Windows associate each fixture with
its actual listening sockets and start identity, exclude unrelated processes, and
report unsupported/denied inspection explicitly. Treat this as an early feasibility
gate before integrating automatic discovery into the manager.

### Stage 2 — Remove authentication and add non-mutating host description

Depends on Stage 1's inspection abstraction. Platform feasibility CI runs in
parallel with this independent transport work; Stage 4 integration remains gated
on passing inspection tests on all three platforms.

- [x] Remove token generation/comparison, credential fields, key APIs, key-related
  CLI arguments and UI controls. Update `shoop_transport`, `shoop_transport_socket`,
  `shoop_host`, engine-process bootstrap callers and socket/shared-memory fixtures.
  Delete auth-only helpers/dependencies; retain randomness used for operational IDs.
- [x] Define a bounded keyless host description containing name, compatibility,
  endpoint roles/addresses, transport information, process/instance identity and
  optional log location. Keep OS-specific implementation outside portable protocols.
- [x] Add compatibility and probe/attach negotiation before handing sockets to data
  or management sessions. A probe must obtain a description or identify the
  management endpoint without allocating controller/shared-memory attachments.
- [x] Version the changed connection/bootstrap exchanges explicitly, updating affected
  codecs and fixtures. Old incompatible wire formats fail with a compatibility error.
- [x] Remove private-log ACL/ownership gates and authentication-only filesystem code;
  retain log rotation, optional logging and useful I/O errors.

Verification: compatible app, management and engine-process connections work with
no key material; removed flags are rejected. Repeated probes before and during a
controller connection leave session ownership and musical state unchanged. Probing
a fresh shared-memory host leaves its first attachment available. Exercise partial,
malformed and incompatible greetings and timeouts.

### Stage 3 — Make launching and attachment independent of profile files

Depends on Stage 2.

- [x] Return and consume the live target/description from `launch::spawn` directly.
  Adapt sibling launch and manager Open UI to pass an endpoint or bounded in-memory
  descriptor to the UI without temporary/exported JSON.
- [x] Remove mandatory credential/profile/config-file requirements from normal native
  entry points. Preserve CLI defaults and GUI audio defaults; keep explicit config
  input and profile import/export available to advanced callers.
- [x] Simplify New host to name/audio settings and Start; retain expandable capacity,
  MIDI, network and optional logging controls. Show ready/failure state and prevent
  duplicate submissions without requiring file paths for a normal start.
- [x] Move Import profile, Export profile and Connect to address under Advanced.
  Update the keyless profile schema and argument validation; opening a manual target
  obtains the endpoint's current description without a saved instance identity.

Verification: real detached launch, sibling launch, UI-only attach and shutdown work
without profile/config/key files. Exercise failed startup and failed UI spawn;
preserve a ready host when its UI fails. Headless form tests verify configuration,
validation, cancellation and submission behavior. Explicit profile import/export
and direct remote address connection still work.

### Stage 4 — Integrate discovery, status and process termination

Depends on Stages 1–3.

- [x] Replace the manager's profile-source list with live OS discoveries plus
  explicitly added advanced targets. Probe only candidate-owned endpoints, correlate
  identities and deduplicate aliases. Select newly started hosts immediately.
- [x] Populate on startup and refresh in the background. Represent starting,
  unresponsive, incompatible and inspection-error states accurately; remove confirmed
  exited processes and handle process restart/PID reuse without selecting a new
  process as the old one.
- [x] Preserve Open UI, audio/fallback status, diagnostics and log viewing. Keep
  socket reattachment and the shared-memory single-attachment limit truthful.
- [x] Keep graceful Stop through the host protocol. Add Force stop to local process
  actions with explicit confirmation, identity revalidation, OS error reporting and
  observed-exit confirmation. Never report forced termination as graceful cleanup.

Verification: launch several GUI/CLI hosts with different ports and transports,
close/terminate the manager, then rediscover the same instances and sessions from a
new manager. Test duplicate names, aliases, busy controllers, interrupted scans,
process exit/restart and PID-reuse simulations. Force-stop only disposable test
children; prove changed identities and manual remote PIDs cannot terminate a local
replacement. Show an unresponsive recognized host even when description fails.

### Stage 5 — Final end-to-end validation and delivery to PR #916

Depends on Stages 0–4.

- [x] Run the full native workflow on Linux, macOS and Windows using built and
  packaged companions: discover CLI hosts; configure/start from the manager; Open UI;
  record/play; exit and reopen the manager; verify uninterrupted state; close the UI
  and reattach where supported; graceful Stop; unresponsive-process Force stop.
- [x] Compare isolated application-state directories before/after default operations
  with logging disabled. Confirm there are no new credentials, profiles, registry,
  advertisement or remembered-host files. Verify explicit log/profile output works.
- [x] Refresh all relevant native screenshots, including automatic discovery after
  reopening, the simpler New host form and Advanced actions. Update the application
  and host READMEs, architecture/networking/CLI documentation and affected tracing
  inventory entries; remove obsolete profile-only/authentication claims.
- [x] Run formatting, a warning-denying workspace build and the no-default-features
  application build. Run the full suite and appropriate architecture/tracing checks:

  ```sh
  cargo fmt --all -- --check
  RUSTFLAGS='-D warnings' cargo build --locked --workspace
  RUSTFLAGS='-D warnings' cargo build --locked -p shoopdaloop --no-default-features
  SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
  python3 scripts/check_shoop_test_usage.py
  python3 scripts/check_tracing_coverage.py --require-closed
  python3 scripts/check_rt_engine_boundary.py
  ```

- [x] Reproduce CI's ordinary `umask 0022` and tracing settings, also checking restrictive
  defaults. Run socket/shared-memory and LAN transport canaries, engine-process
  bootstrap tests, and browser/no-std checks affected by shared transport changes.
  Measure patch coverage without weakening the existing gate.
- [x] Commit each completed milestone, push to `feat/host-manager`, and update this
  existing PR's title/body around the final behavior, screenshots and evidence.
  Recheck the target branch for updates and validate any resulting merge.
- [x] Work through failures until required/relevant checks, including Codecov, pass
  on the final head. Inspect logs/artifacts, make targeted fixes and rerun the checks
  they affect; queued/running jobs are not evidence of a green final result.
- [x] Check incoming automated reviews after pushes. Repeatedly address justified
  findings and recheck until participating automated reviewers approve. If no review
  is provided, record that fact. Do not merge the PR without a separate instruction.
- [x] Map each immutable acceptance criterion to final evidence and update this plan's
  checkboxes and status. Record any remaining blocker explicitly.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution evidence

- Stage 0: fetched the integration branch; `3aabf437` remains an ancestor of the
  work branch. PR #916 is open with no review threads/submissions. Reproduced the
  canonical-path failure on Linux using a symlinked `TMPDIR`; the same test passes
  after canonicalizing the expected temporary-directory parent. Warning-denying
  workspace build, formatting and test-attribute policy pass. Native CI verification
  of the correction will accompany the OS inspection milestone.

- Stage 1: added the native-only `shoop_host_processes` adapter. `sysinfo` supplies
  executable/process information, with precise OS creation identities; Linux joins
  candidate FD inodes to procfs TCP tables, while macOS/Windows use `netstat2`'s
  libproc/owner-PID implementations. A generic Linux netlink experiment returned
  empty socket tables in this environment, so direct procfs inspection is used.
  Linux pidfds and Windows owned handles bind termination to the selected process.
  Real child fixtures cover IPv4, IPv6, wildcard listeners, exit and changed identity.
  All 2,560 workspace tests pass (4 skipped) with CI tracing and `umask 0022`; the
  warning-denying build, formatting, tracing inventory and test policy pass.
  Native inspection tests were added to every desktop CI matrix job. Windows/macOS
  verification is pending that push.
- Ordering refinement: develop Stage 2's transport changes while the Stage 1 native
  matrix runs. These implementations are independent, and the long matrix build
  latency need not block transport work. Stage 4's discovery integration still
  requires platform feasibility evidence; goals and acceptance criteria are unchanged.

- Stage 2: removed authentication across native app/management, portable socket
  negotiation and engine-process bootstrap. Versioned the keyless greeting and
  app/engine exchanges. Inspection returns a bounded live description before
  session admission; shared-memory availability updates after first attachment.
  Repeated probes exercise both endpoint roles before and during a controller.
  Removed key CLI/UI fields and private file/ACL code; optional rotating logs remain.
  Warning-denying workspace build and policy checks pass; full CI-shaped suite is
  running. Early coverage CI exposed an exit-observation race in Stage 1: retry
  transient inspection errors after termination, and require observed exit/change.

- Stage 2 verification: CI-shaped full suite ran 2,559 tests: 2,557 passed,
  with only two expected-byte fixtures still pinning protocol version 11. Updated
  the canonical fixtures to version 12 and reran their complete protocol package.
  Architecture isolation passes for all four target graphs. Added small dedicated
  Linux/macOS/Windows inspection jobs so the feasibility gate no longer waits for
  full application packaging and companion canaries.

- Native feasibility gate: real inspection fixtures passed Linux, macOS 15 and
  Windows 2025 before enabling manager discovery (run `41638204897`, jobs
  `125925106693`, `125925106559`, `125925106592`). The dedicated Windows job uses
  the same Perfetto C++ settings as the existing native jobs.
- Stages 3–4: normal creation/attachment and sibling launch use configuration pipes
  and live addresses, with no profile/config output. New host has no profile/key
  fields; logging remains opt-in. Advanced imports/exports/manual addresses stay
  in memory. Profiles omit runtime process/instance/shared-memory resource IDs and
  resolve a fresh description at connection time. A ready host survives UI spawn
  failure and launcher exit.
- Integrated process discovery runs four concurrent probes, handles both endpoint
  roles/wildcards, deduplicates live instances, preserves selection by OS identity,
  rejects obsolete scan generations and surfaces inspection errors. Graceful Stop
  retains server-side live-owner protection. Force stop requires an OS source and
  separate confirmation; tests terminate only disposable children and reject changed
  identities and manual remote sources.
- Two CI-shaped complete workspace runs passed 2,565 tests (4 skipped), including
  actual child discovery, duplicate names/aliases, recording through fresh managers,
  socket reattachment/playback, shared attachment availability, and the Force stop
  dialog/action. Warning-denying workspace build and tracing/test policies pass.
  File-free socket/shared/LAN canaries passed; packaged canaries now also drive a
  fresh manager model before attachment, during recording and after process exit.
- Final inspection refinement: macOS now queries only candidate processes' libproc
  descriptors rather than enumerating unrelated processes' sockets through netstat2.
  The checked union accesses remain isolated in the native helper. Final native CI
  will re-prove this refinement and the complete manager implementation.

- Final local delivery checks: the Linux native GUI created a host through the
  file-free form, attached/closed its UI, terminated/reopened the manager and
  rediscovered the same PID, creation identity and runtime instance. All four
  isolated XDG directories remained empty (Mesa cache disabled for this check).
  Explicit CLI logging, local Force stop and protocol-based graceful Stop were
  exercised; eight refreshed native screenshots cover the resulting UI.
- Screenshot verification found Linux worker threads appearing as host processes:
  disabled sysinfo task enumeration and added an executable/PID regression check.
  Detached UI/manager children now have wait-only reapers, preserving independent
  lifetime while avoiding zombies under a still-running parent.
- Native app admission now checks protocol version and data/management role before
  allocating an attachment. Real shared-memory tests reject wrong roles/versions,
  preserve availability through a partial preface and accept its valid completion.
- Local full-suite runs, including LLVM instrumentation, passed 2,565 tests with
  four skips before the final admission/parser additions. The measured patch
  coverage was 89.95% without gate changes. Bare-metal, Wasm protocol, no-default
  build and all four-target dependency-boundary checks pass. New admission tests
  pass all 24 host tests; final full-suite verification includes the added cases.
- Incoming CodeQL review flagged a user-controlled endpoint command argument.
  Binding it to a single `--host-address=value` argument prevented argument
  injection, but did not clear CodeQL's tainted-command finding. Manager and sibling
  launches now pass the endpoint through the bounded startup pipe and use fixed
  `--ui-only --host-stdin` arguments. Real sibling tests and native GUI attachment
  exercise this path; manual `--host-address` remains available under Advanced.
  Recheck CodeQL and its thread after final analysis.
- Target branch recheck found `64a4ad06` (click-track generation) on top of `3aabf437`.
  Merge and revalidate it before the final push. Remaining delivery work is the
  final native/web/coverage matrix, automated review and final evidence mapping.

- Final source merge: integrated `64a4ad06` without conflicts. The merged head
  `a290de8b` passes the warning-denying workspace and no-default builds and all
  2,569 workspace tests (4 skipped), with CI tracing and `umask 0022`.
  Screenshots and documentation are committed; PR metadata and final remote checks
  remain to be recorded.
- Final process-lifetime regression: moving a running Linux host's executable
  reproduced an OS/description identity mismatch. Inspection now reports the live
  OS executable path. The regression proves discovery retains the same runtime
  instance after moving and unlinking its executable, without any disk metadata.
- Native UI handoff check: manager Open UI attached over the stdin pipe; closing
  only the UI left the host available and its exited UI child was reaped. Graceful
  Stop completed afterward. Isolated configuration/data/state/cache directories
  remained empty. The log screenshot now shows an explicitly enabled real log.
- Final handoff/path-fix candidate passes all 2,569 workspace tests (4 intentional
  canary skips), the warning-denying workspace/no-default builds, formatting and
  tracing/test-attribute policies. A separate `umask 0077` run passes all 38
  launch/discovery/logging/role tests; the ordinary full run uses `umask 0022`.
- Source candidate `66d29342` passes all 2,569 tests under LLVM instrumentation;
  changed executable lines measure 3,396/3,776 (89.94%) against `64a4ad06`. Remote
  [coverage and native workflow](https://github.com/SanderVocke/shoopdaloop/actions/runs/41652717130)
  confirms the coverage job and both Codecov checks are green, with the existing
  80% patch floor unchanged. All six latest socket/shared-memory/LAN engine and
  large-file persistence canaries pass locally, including interrupted transfers.
- [CodeQL](https://github.com/SanderVocke/shoopdaloop/actions/runs/41652716928)
  passes on `66d29342`. GitHub Advanced Security automatically resolved both
  command-line findings (threads `PRRT_kwDOJz8wF86zIn8z` and
  `PRRT_kwDOJz8wF86zJx4I`); no unresolved threads remain. No separate general
  automated review or approval was provided; absence of review is not approval.
  The final documentation-only push will be checked again before delivery.
- The full Windows release application suite on `66d29342` exposed a retained
  process-object race in the native inspector after termination (job `125978500768`).
  Windows identity now checks the retained handle's signaled exit state instead of
  trusting an enumerated object to be alive. An exited object is omitted from both
  direct identity and socket inspection. Regression fixtures keep parent handles
  open after normal exit, including exit code 259, and retain the original Force
  stop assertions. The early Windows gate now exercises optimized fixtures too.
  Local process tests and a warning-denying Windows MSVC-target library check pass;
  the full-suite/native CI recheck covers the corrected candidate.
- The Windows-exit fix passes all 2,570 instrumented workspace tests (4 skips).
  A simultaneous ordinary run hit the unchanged runtime stress fixture's render
  iteration bound while competing with compilation/coverage work; 20 isolated
  repetitions pass. Recheck the ordinary suite after integrating the new base,
  without simultaneous workspace compilation. No runtime assertions were weakened.
- Rechecked the target and merged `e68b15ac` (output buses, PR #926) cleanly.
  Both branches had independently selected the same app/engine-host versions for
  incompatible changes. The combined protocols use app version 13 and engine-host
  version 3, keeping earlier keyless binaries from attaching with the wrong schema.
  Canonical byte fixtures follow the new app version. Final checks cover this merge.
- The merged form exposes output-bus count/channel capacity alongside the existing
  optional capacity settings. Discovery and no-output fixtures use the new default
  bus capacities. The merged warning-denying workspace build and all 2,578 workspace
  tests pass (4 intentional skips), including the unchanged runtime stress fixture.
- Merged candidate `6f8ea03f` also passes all 2,578 instrumented tests; measured
  patch coverage is 3,408/3,792 (89.87%) against the new base `e68b15ac`.
  The no-default build and four-target dependency-boundary audit pass. Native GUI
  verification started a host with four buses/six bus channels, terminated/reopened
  the manager and rediscovered the same PID/start/instance, attached its UI through
  the startup pipe, closed only the UI and confirmed it was reaped, then completed
  graceful Stop. A fresh manager showed no hosts afterward; all four isolated XDG
  directories remained empty. Refreshed New host/empty captures and a ninth native
  capacity screenshot accompany the completion record.
- The merged candidate's six desktop CI jobs pass (Linux/macOS/Windows,
  debug/release). Coverage includes native process fixtures, headless manager
  forms/actions, real detached siblings and extracted-companion socket/shared/LAN
  engine and persistence canaries. Interactive GUI checks and all nine captures
  were performed on Linux; the other desktop platforms use their native CI tests.
  Windows release's original exit-observation failure is fixed without weakening
  its assertions; both ordinary and optimized early Windows fixtures pass.

## Acceptance evidence map

| Criterion | Implementation and verification |
| --- | --- |
| 1. Keyless connections | Versioned socket negotiation, app admission and engine bootstrap; real app/management/shared-memory/socket/LAN tests and packaged engine canaries. Removed credential flags are rejected. |
| 2. Live OS discovery | `shoop_host_processes` native fixtures passed Linux/macOS/Windows; manager discovery tests use real candidate-owned sockets and arbitrary ports. Linux executable move/unlink regression retains the same instance. |
| 3. Harmless inspection | Shared-attachment admission tests reject incompatible roles/versions; repeated probes and fresh managers run through recording/playback without acquiring control or consuming the first attachment. |
| 4. File-free workflow | Configuration/readiness/UI endpoint pipes, actual native New host/Open UI/Stop, sibling-launch and failed-UI-spawn tests; isolated application-state directories remain empty. |
| 5. Host lifetime | Real manager termination/reopening preserves PID/start/instance; recording and socket reattachment tests use fresh manager models. Child reapers wait without controlling ready-host lifetime. |
| 6. Advanced profiles | Advanced menu screenshots and form/action tests; endpoint-only schema excludes runtime IDs; no automatic load/save. Manual aliases deduplicate with local discoveries. |
| 7. Correct termination | Real child identity/termination fixtures, unresponsive-host tests, local Force stop dialog/action and native GUI verification. Changed identities and manual remote PIDs refuse OS termination; graceful Stop retains live-controller protection. |
| 8. Complete delivery | Nine native screenshots, current app/host/network/architecture docs and PR #916 metadata. Final CI, review results and source provenance are recorded below. |

## Completion record — 2026-11-03

- Final implementation: `6f8ea03f0c93c3a8483a17c2793d010af63c3d57`, including
  integration of `e68b15ac551ff1899554311ddfe2d5e4dda1d3ea`. A final fetch confirmed
  that integration revision is still the target head.
- [Build and Test run 41660892573](https://github.com/SanderVocke/shoopdaloop/actions/runs/41660892573)
  passes all eight debug/release desktop/web jobs, the three native inspection jobs
  (including optimized Windows lifetime fixtures), Rust coverage and bare-metal
  checks. Both Codecov project and patch checks pass with the 80% patch floor intact.
- [CodeQL run 41660892918](https://github.com/SanderVocke/shoopdaloop/actions/runs/41660892918)
  and [Docs run 41660893534](https://github.com/SanderVocke/shoopdaloop/actions/runs/41660893534)
  pass. Both earlier automated findings are resolved; no unresolved review threads
  or separate general automated-review verdict are present.
- Local warning-denying workspace/no-default builds, all 2,578 ordinary and
  instrumented tests (4 intentional skips), formatting, closed tracing inventory,
  test-attribute policy and four-target dependency boundaries pass. Local patch
  coverage is 89.87% against the final integration base. The native workflow and
  isolated-directory checks above verify discovery, detached lifetime, keyless
  attachment, explicit outputs and termination behavior.
- PR #916 contains the implementation and documentation. Nine native captures
  include default creation, optional capacity settings, discovery/status, Advanced,
  connected UI, audio fallback, logs, Force stop and the empty manager.
- The completion follow-up changes only this record and native captures. Its
  [final-head checks](https://github.com/SanderVocke/shoopdaloop/pull/916/checks)
  are rechecked before delivery. There are no remaining implementation blockers;
  merging the PR requires a separate instruction.

## Follow-up — explicit native executables

The user approved three public entry points: `shoopdaloop`, `shoop_host`, and
`shoop_host_manager`. The manager now owns a native crate/entry point and optional
advanced CLI inputs. The two redundant looper role flags and wrapper-only host
arguments are removed. Shared companion lookup handles ordinary sibling paths and
macOS app-entry aliases; both GUI launch directions retain independent lifetimes.

Packages carry all three binaries plus a macOS manager app entry, a relative Windows
shortcut and a Linux desktop entry. Native CI builds/tests the manager and smoke-tests
its entry point after archive extraction. Existing discovery and presentation tests
move with the implementation; the manager's normal dependency graph contains no Lua
runtime. Local verification includes focused manager/launch tests, package relocation
checks and real GUI launches in both directions with a separately started CLI host.
The complete local suite passes 2,580 Rust tests (4 intentional skips) and 145
Python tests. Formatting, warning-denying workspace build, no-default-features,
closed tracing inventory and dependency-boundary checks pass. An actual Linux
archive passes strict manifest/binary/icon validation. The GUI check confirms
discovery, both cross-application launch paths and orderly cleanup with empty
isolated state directories. CI results for this follow-up are attached to PR #916.
