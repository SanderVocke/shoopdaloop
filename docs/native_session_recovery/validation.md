# Incremental validation

This is transport-extraction evidence only. It does not establish A1–A11 or a
completed native-session-recovery milestone. Session service, app protocol/client,
independent native roles and recovery UX remain pending in the plan.

Baseline: `cb6410f0`, with the transport extraction changes in the following commit.
Environment: managed Debian 13, x86_64, stable Rust 1.99.1 installed under
`/workspace/tools`; no Nix or preinstalled Rust. Network commands use the managed
proxy with the execution tool's network grant. No credentials were copied or logged.

Commands below ran on the extraction candidate, before its stage commit:

| Command | Result / scope |
| --- | --- |
| `cargo test -p shoop_transport -p shoop_rt_transport -p shoop_rt_protocol -p shoop_rt_transport_socket -p shoop_rt_transport_shared_memory --no-fail-fast` | Passed 28 tests: codec, carrier lifecycle/credits/bulk, socket authentication/deadlines and shared-memory delivery; moved buffer/framing tests included. |
| `cargo test --locked --no-default-features -p shoop_rt_engine -p shoop_rt_host_native -p shoop_rt_host_worklet -p shoop_rt_transport_message_port --no-fail-fast` | Passed 51 tests: engine timing/media/routing, prepared native/worklet heap guards and MessagePort contracts. CPAL feature disabled; no hardware claim. |
| `cargo test --locked -p shoop_rt_host_process --no-default-features` | Passed all 9 subprocess tests over both carriers, including crash/recording continuity and independent shutdown. These are baseline engine-host tests, not app-service recovery tests. |
| `RUSTFLAGS='-D warnings' cargo build -p shoop_transport -p shoop_transport_socket -p shoop_transport_shared_memory -p shoop_rt_protocol -p shoop_rt_transport -p shoop_rt_transport_socket -p shoop_rt_transport_shared_memory` | Passed. |
| `cargo build --target wasm32-unknown-unknown -p shoop_transport -p shoop_rt_protocol -p shoop_rt_transport` | Passed. |
| `cargo build --locked --target wasm32-unknown-unknown -p shoop_rt_host_worklet -p shoop_rt_transport_message_port` | Passed. |
| `RUSTFLAGS='-D warnings' cargo build --locked --no-default-features -p shoop_rt_host_process -p shoop_rt_host_worklet` | Passed, including the native/process/worklet consumers. |
| `python3 scripts/check_rt_engine_boundary.py` | Passed all 13 package boundaries on Linux, Windows, macOS and Wasm dependency trees. This is not cross-platform execution. |
| `python3 -m unittest scripts.tests.test_rt_engine_boundary scripts.tests.test_rt_runtime_miri scripts.tests.test_rt_runtime_miri_batches scripts.tests.test_rt_runtime_miri_jobs` | Passed 43 Python tests. Only inventory/runner tests; no Miri execution. |
| `cargo fmt --all -- --check` | Passed. |
| `python3 scripts/check_shoop_test_usage.py` | Passed. |
| `python3 scripts/check_tracing_coverage.py --require-closed` | Passed, 291 production modules accounted for. |

The Miri selection still contains 168 commands / 336 executions. Five shared byte
buffer/framing cases move from the engine transport's library target to the new
shared transport library; no workloads, seeds or assertions were weakened. Miri
execution is deferred to the final stage as requested.

Full retained-workspace native/Node/browser gates, hardware smokes, three-platform
packages, app attachment tests, performance measurements and final-head CI/review
are not covered by this checkpoint.

## Initial app protocol/client contracts

Candidate based on `b826242d`, before the following stage commit. This is portable
contract/client evidence only. No production UI or Lua path has been migrated to
the new crates, and no authoritative session service or independent app host exists
yet. Therefore no A1–A11 completion is claimed.

- `cargo test -p shoop_app_protocol -p shoop_app_client -p shoop_app_api --features shoop_app_api/serde`: 24 existing API tests plus 13 new protocol/client tests pass. Cases cover typed/encoded projections, malformed/truncated/version/oversize records, MIDI domain invariants, hostile lengths, partial snapshot rejection, lost reply reconciliation, bounded cooperative local queues, Ready ordering and correlated shutdown refusal/retry.
- `RUSTFLAGS='-D warnings' cargo build --locked -p shoop_app_protocol -p shoop_app_client`: passed; the same packages also build for `wasm32-unknown-unknown` with warnings denied.
- `RUSTFLAGS='-D warnings' cargo build --locked --target wasm32-unknown-unknown -p shoop_app -p shoop_rt_backend`: passed for the existing application/adapter consumers.
- Four-target dependency audit includes both new crates and forbids engine/runtime/device/UI/concrete-carrier dependencies.
- Python boundary/Miri-runner tests pass (44 tests). The Miri inventory adds four exact portable codec/client cases; original workloads/seeds remain intact. New total: 172 commands / 340 interpreted executions. Miri itself has not run.
- Formatting, test-attribute and closed tracing inventories are checked before the stage commit. Browser/Node execution and production service integration are not established by Wasm compilation.

The preceding transport commit's full workflow was dispatched as run
[41281596959](https://github.com/SanderVocke/shoopdaloop/actions/runs/41281596959).
The Rust coverage job failed with exit 100; its diagnostic log/artifact downloads
returned Forbidden in this environment. That failure is unresolved, not a pass.
The newer protocol/client candidate needs its own final-head CI evidence.

## Cross-lane fixture barrier correction

The completed log archive of the first workflow became accessible after that run
was superseded. Coverage ran 2,376 tests, with 2,375 passed and one failure:
`subprocess_shared_memory_uses_the_same_contract_without_socket_record_traffic`
expected a bulk chunk immediately after a reliable-lane control acknowledgment.
Independent lanes permit that acknowledgment to overtake the bulk request/reply.
The record carrier implementations had moved without changing this behavior.

A deterministic carrier now holds bulk delivery until a bulk-lane barrier. The
new regression reproduced the original panic before the correction, and passes
with the correction. The remote test fixture now waits for a bulk-lane marker as
well as its reliable control acknowledgment, within the existing five-second
deadline. Production transport scheduling, protocol credits and all media/content
assertions remain unchanged. This is not a production latency policy change.

- Before correction: actual subprocess case passed 20 unconstrained runs and 20
  runs with parent/child pinned to one allowed CPU; the deterministic regression
  failed at the exact original assertion.
- After correction: full engine transport/socket/shared-memory contracts pass
  (20 tests), as do all nine host-process tests. Actual subprocess repetition
  also passed 20 unconstrained runs and 20 runs pinned to one allowed CPU, with no
  artificial background load.
- Warning-denying build of the contract-enabled engine transport, formatting,
  test-attribute policy and relevant Miri-inventory Python tests pass.
- The carrier Miri target now includes the new deterministic regression; the full
  selected inventory is 172 commands / 341 executions. Original selections and
  workloads remain intact. Miri has not been executed.

This correction addresses the observed coverage failure. A successful CI run on
the corrected head is still required; the overall recovery plan remains incomplete.

## Engine-independent backend contract

The crate root now contains the backend contract and shared data without engine
imports. Concrete/reference implementations and legacy conversions are gated by
the legacy feature; that feature temporarily remains a default for the existing
production entry points. The RT adapter explicitly disables those defaults.
This completes the Stage 1 boundary extraction, not Stage 6 production retirement.

- Warning-denying native and Wasm builds of `shoop_backend` and `shoop_rt_backend`
  with `--no-default-features` pass. Their production dependency trees exclude
  `shoop_engine`; the adapter audit now inspects its full transitive tree instead
  of only direct dependencies.
- All 86 retained backend library tests and all 15 RT adapter tests pass. Existing
  reference assertions remain intact; backend library tests now live under the
  legacy module rather than the portable contract root.
- Native-driver warning-denying build passes. The initial attempt was blocked by
  missing JACK development metadata; the official Debian `libjack-jackd2-dev`
  1.9.22~dfsg-4 package was extracted into `/workspace/tools/sysroot` and selected
  with PKG_CONFIG_PATH/PKG_CONFIG_SYSROOT_DIR. No system configuration was changed.
- Existing application and RT-adapter Wasm consumers build with warnings denied.
- The native dummy topology/capture/same-driver-switch contract passes under its
  new `legacy::native::tests` path. Its exact nextest scheduling override moves
  with it, retaining the original scheduling protection.
- The four-target dependency audit covers 16 packages, with dedicated checks for
  the neutral backend and RT adapter. Boundary policy tests pass (11 tests).
- Formatting, test-attribute and closed tracing inventories pass. The tracing
  inventory follows the moved legacy code and classifies the contract root as data.

Native production UI/Lua composition still owns the old application model. No
session-service recovery, independent app host roles or recovery UX is implemented
by this boundary refactor. Stages 2–7 and all final acceptance evidence remain open.


## Stage 2 — authoritative service (2026-11-01)

The portable `shoop_app::Service` owns names/order, sync identity, selection/target,
shared policies, routing intent, stable mappings, admitted operations and retained
results. The same RT adapter survives every client attachment. A separate connection
identity and generation fence every mutation, detach and shutdown. The service never
waits for global engine idleness; typed delivery pumps bounded work cooperatively.

The RT adapter now reports bounded sequence-correlated outcomes, including requests
whose object mapping disappeared after another refusal. Tracking is reserved before
submission. A fresh adapter refuses unknown live structure and never deletes it.
Service history is bounded at 256 results/128 pending operations; retained actor
watermarks prevent replay after result eviction. Routing intent retains terminal
errors rather than displaying refused requests as perpetually pending.

Focused evidence on this stage's working tree:

- Ten service integration tests use the real RT engine and a controllable clock:
  typed/encoded synchronization; ordered selection/target and policy ownership;
  loss before create/remove/routing confirmation; lost replies and no replay;
  partial creation refusal with manageable survivors; snapshot/publication races;
  stale-client fencing; bounded slow-client reclamation; audio/MIDI capture and
  playback across detachment; shutdown confirmation/cleanup failure and retry.
  A carrier injects one explicit loop-creation refusal; other behavior executes
  against the real engine. The clock asserts no service synchronous waits.
- Seventeen RT-adapter tests cover contracts, refusals, correlated outcomes,
  bounded tracking, retained live structure, asynchronous media and existing
  latency/throughput workloads.
- Native and Wasm warning-denying service builds pass without the temporary
  migration feature. The default migration-feature native build also passes.
- App protocol/client tests, test attribute policy, tracing inventory, and four-target
  dependency audit pass. The audit includes the service's complete dependency tree.

The old application runtime remains behind a temporary `legacy` feature for direct
consumer migration. Production UI/Lua wiring, independent hosts, recovery/exit UX,
legacy retirement, final suites/Miri and final CI are still stages 3–7. This evidence
is not final A1–A11 acceptance. Miri has not been run early.


## Stage 3 — UI/Lua client and main-thread composition (2026-11-01)

Native and browser UI runtimes now compose the portable service with
`shoop_app_client::runtime::Runtime`. Local delivery passes typed messages and Arc
media without a codec round trip. Lua VM, timers, controller MIDI, dialogs and
resources stay in the client; portable control operations are app API data and are
resolved by the service. The client uses coherent confirmed reads: a write becomes
visible to a later callback after service confirmation, without waiting or recursive
message dispatch. Legacy standalone scripting fixtures retain their reference mode;
production clients opt into confirmed reads.

Recovery starts catalog scripts disabled. Explicit user startup permits subsequent
catalog reconciliation for that script. Detach/connection loss stops local scripts
and closes controller MIDI; controller ports are a read-only client projection and
never enter service state. Piano recipients follow ordered input-monitoring intent;
detach sends only held note-offs, preserving capture and other MIDI activity.
Held piano tracking is bounded at 512 voices. Shutdown additionally queues channel
panic before the host's actual MIDI reset/cleanup path.

Media views use a separate bounded transfer (32 MiB, 1024-byte chunks). Snapshot
transfers remain capped at 8 MiB. Adapter reads reject excessive announced totals
before allocating content, and cancellation discards client-owned read buffers.
The service handles at most 16 local commands and sends at most 64 updates per turn;
this leaves delivery capacity for acceptance, terminal results and snapshots during
bursts. A 256-operation UI burst completes in order without disconnecting an actively
pumping client. Unsupported persistence, FX, content replacement, timing and driver
switch operations retain visible capability refusals.

Focused evidence:

- Seventeen real-engine service/client integration tests pass natively, including
  Lua confirmed reads/events/timers, queue saturation, recovered script gating,
  MIDI control ownership and teardown, held note cleanup, reset completion, and
  waveform/MIDI views in addition to Stage 2 recovery cases.
- Node 22.22.2: final app run passes 133 tests (including 17 service cases and retained
  reference tests); the unchanged projection/codec/scripting suites pass 9/5/32 tests.
  The earlier all-four-package run passed 178 tests before the final client MIDI
  projection/cleanup case was added. wasm-pack 0.15.0, wasm-bindgen 0.2.127, Rust 1.99.1.
- The actual native application in-process core workflow passes: session reset,
  tracks, routes, sync and immediate audio/MIDI record/play, controls/meters, waveform,
  MIDI detail, explicit unsupported driver switch, stop and clear. Representative
  observed control/detail publication was approximately 5–57 ms; synchronized
  musical transitions include their intended cycle delay. This is focused workflow
  evidence, not the final performance matrix.
- Development Chromium 154.0.8037.57 with fake microphone passes the full new-engine
  AudioWorklet workflow at 900x600, reporting zero xruns. It includes fresh/reset
  structure, audio/MIDI capture/playback, routing, controls, waveform/MIDI detail,
  immediate stop and clear. No application worker or serialization was introduced
  into the local app connection. This smoke preceded only the final Lua-local
  projection/cleanup changes; ordinary Node and compile checks cover those changes.
- Native and Wasm warning-denying builds, formatting, closed tracing inventory,
  Rust test attribute policy, and four-target service/client/RT dependency checks pass.
  Lua-client dependencies are audited separately from the smaller projection core.

The UI-owned external engine supervisor and legacy backend/worker choices are still
transitional and are replaced in Stages 4–6. Final native role packaging/recovery,
required full suites, selected Miri, final-head CI and review remain Stage 7. No Miri
has been run early. The earlier Stage 1 head's CI rerun is green; it does not validate
these unpushed stage commits.


## Stage 4 — independent native host (2026-11-01)

`shoop_host` now owns the app service, persistent RT adapter and native engine. The
UI attaches through the app protocol; the engine connection remains internal.
Authenticated data and management listeners run bounded authentication workers,
separate from the service pump. The sibling launcher uses independent process groups
and null standard streams, verifies bounded host readiness, then exits without a
supervisor or inherited lifetime pipe. A ready host survives UI spawn failure.

Registry entries and credential files use owner-only Unix permissions and a Windows
ACL adapter. Credentials are referenced by path and never sent in argv or logs.
Discovery can list entries and prune only entries whose recorded process is gone
and both loopback listeners refuse connections; PID is never attachment authority.
Explicit endpoint/configuration and credential references are available to host-only
startup. Data connection greetings verify host/session identity before attachment.
Closing connections discard their transfer buffers and have bounded retirement.

All app records use one ordered reliable lane. Semantic results remain discrete;
queued full snapshots coalesce to the latest state. Media chunks can interleave with
snapshot chunks, using separate bounded client assemblers; semantic state updates
remain ordered after their snapshot boundary. This avoids bulk transfer starving
control results without duplicating carrier machinery.

Focused evidence:

- Real socket subprocess tests kill the controlling UI during recording, reattach,
  verify stable track/loop IDs and names, preserve recording/capture, read actual
  nonzero recorded media, reject a stale owner's management shutdown, and close the
  recovered host with confirmed MIDI reset and cleanup.
- A launcher subprocess exits while its host/UI siblings continue. Later clients
  recover the recording and close the host without original process handles.
- Socket busy-owner, bad credential/version/identity, early authenticated disconnect,
  startup refusal and UI spawn failure cases leave the correct host/session intact.
- Shared-memory startup, a core rename, second-connection refusal and graceful host
  exit pass. Shared-memory reattachment remains excluded.
- Protected credential tests verify bounds, non-overwrite, Unix permissions and
  symlink refusal. Windows ACL execution awaits Windows CI.
- Seventeen portable service/client tests pass against the merged no_std stack.
  Protocol/client/engine-transport suites pass; native and Wasm warning-denying builds
  and four-target dependency audits pass. The higher-level host is audited separately
  and has no production Lua/UI/legacy-engine dependency. Formatting, test-attribute
  and closed tracing inventories pass. No early Miri run.

This is development integration evidence. Packaged platform matrices, final-head CI,
full retained-stack suites and final performance/fault stress remain Stage 7.

## Stage 5 — recovery controls and explicit shutdown

The native UI shows attaching/snapshot/lost states, offers manual reattachment and
registered-host selection, and routes both Session menu exits and platform window
close through a choice with Cancel. UI-only exit detaches; close-both reserves one
operation on the client thread, suspends unsent controls/Lua, and waits on authenticated
management results. An uncertain retry checks the same operation; a reported cleanup
failure permits a fresh retry. No PID kill is part of product shutdown. Last-owner
authority remains usable after data loss and becomes stale on replacement.
Recovered scripts are visible but inactive; preference reconciliation avoids applying
unchanged UI audio preferences or enabling unchanged saved user scripts on recovery.

Development checks: 19 service/client workflow tests passed (including management
fencing after loss, pre-ready controls, recovered saved Lua and unsent-command
cancellation); the headless egui platform-close/Cancel/authorized-exit test passed.
All seven subprocess socket/shared-memory/launcher recovery tests passed. The
warning-denying native UI build, formatting, test-attribute and closed tracing policies
passed. Final packaged GUI workflows and other platforms remain Stage 7 gates.

## Stage 6 — sole production stack and parity handoff

Production frontend/host graphs no longer select or link the old engine, old app
model, engine-only process supervisor or legacy browser worker/worklet clients.
The old model and its 134 native behavioral tests plus two RT adapter contracts now live in
`shoop_app_reference`, still executable natively and discoverable by the Node runner.
Old frontend/browser acceptance scenarios and user behavior are preserved under
`reference/`; these frozen scenarios are restoration evidence, not passing product
coverage. The feature parity ledger links each gap, retained UI/code/test and
completion requirement; its capability coverage policy fails on untracked flags.

Native and browser entry-point warning-denying builds passed, including default
CPAL and dummy-only native builds. Focused service/client tests (20), frontend tests
(55), egui tests (223), reference model/RT contracts (136), 142 Python script tests
(including three parity-policy tests), tracing/test-usage policies and four-target
production dependency audits passed. Capacity refusals now retain error severity.
Settings migration diagnoses retired backend/transport preferences and preserves
unrelated values. Existing editors, document fields, processor code and build recipes
remain available for parity restoration.

The packaged development companion probe passed socket and shared-memory app
record/play/media/confirmed shutdown; socket also verifies recording and identities
through client replacement. Development hosted web packaging and the RT worklet JS
contract passed. Chromium completed the actual main-thread client/service and
worklet workflow with a 64-command control burst and media reads, zero xruns and
no render-time memory growth/command overflow; the mock-Web-MIDI variant also passed
with zero xruns. Script catalog delivery now waits for client readiness instead of
losing the initial catalog. Full retained-stack suites, final packaged/platform
matrices and selected Miri remain the final-stage gates. No Miri has run early.

## Final candidate ordinary checks (before selected Miri)

- Full warning-denying workspace build passed. Full native nextest 0.9.116 with
  independent legacy-native reference features passed **2404 tests**, with four
  explicit ignored canaries. The additional engine/backend reference selection also
  passed 1182 tests (a subset, not additional unique coverage).
- The real frontend test binary was used as separate host/UI processes. Killing the
  UI during recording preserved IDs/names/media; four replacements painted the
  application, UI-only exits kept the host alive, and the final UI edited/read the
  nonzero capture and confirmed close-both before exiting. All 63 frontend tests
  passed after the final script-preference reconciliation fix. Appearance-only
  settings changes no longer rescan/reconfigure running scripts.
- A 32-cycle real socket recovery workload kept recording and reclaimed departed
  descriptors: host descriptors 6 before/6 after. With low-latency TCP sends, attach
  p50/p95 was 6.79/8.65 ms and confirmed control p50/p95 was 11.81/14.11 ms in the
  focused development run. Earlier batching yielded about 52/56 and 46/51 ms.
  Scheduling and telemetry are asynchronous; fixtures now wait for observed mode
  and frame progress instead of assuming admission or attachment implies a new frame.
- Environment: x86_64 Linux 6.18.57/glibc 2.41, 5 affinity CPUs with a 4-CPU cgroup
  quota and 16 GiB memory limit. Timings include host/client control pumps and real
  dummy callbacks. They are development-build measurements under shared/container
  scheduling, not hardware latency guarantees. The typed/encoded portable state
  contract also passed; whole-workflow wall time includes preparation/rendering and
  is not an isolated codec benchmark (1782-byte structural snapshot in that case).
- Of 31 Node packages, 29 passed on the broad run; the service timing diagnostic used an unsupported Wasm clock and
  the frontend had a native-only test helper compiled for Wasm. After restricting
  only those diagnostics/helpers to native, their complete Node selections passed
  (21 service tests and 21 frontend tests). Unchanged package evidence is retained;
  final-head CI reruns the full Node selection.
- Development hosted Chromium core/burst/media and mock-MIDI workflows passed with
  zero xruns, no render memory growth and no command overflow. Browser settings
  save/reload/rejected/failing-save workflow passed after enabling output audio for
  each newly loaded page. The bare-metal runtime/loop libraries built with warnings
  denied after the no_std merge.
- Authenticated explicit ownerless shutdown now closes a shared-memory host after
  its sole data connection is lost; it refuses live owners and allocates a new
  generation. Stale UIs never invoke that claim as an automatic fallback. Service
  and subprocess fencing/cleanup tests passed. This adds management, not shared-memory
  UI reattachment. App protocol version is 6.

Final production packages/platform matrices, selected Miri and final-head CI/review
are still required. Physical audio/MIDI devices are absent here; deterministic dummy,
CPAL/midir mock and browser fake-capture tests ran. Optional hardware tests are not
claimed as exercised. The parity ledger remains OPEN and is a required handoff.
