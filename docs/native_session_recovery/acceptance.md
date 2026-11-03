# Native session recovery acceptance

Completed code/test candidate: `929ed71b4800e208620262f1bafd12acd970735d` on
`plan/native-session-recovery`, pushed to origin in
[PR #899](https://github.com/SanderVocke/shoopdaloop/pull/899), based on
`feat/engine-ipc`. All A1–A11 have evidence below; the full platform matrix is green
in [run 41306455452](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455452).
The completion follow-up retains production sources and corrects the native stress
fixture's media budget;
[current-head checks](https://github.com/SanderVocke/shoopdaloop/pull/899/checks)
are required before delivery.
The [plan](../../PLAN_NATIVE_SESSION_RECOVERY.md) retains the fixed A1–A11 contract.
The [feature parity ledger](feature-parity.md) is the mandatory next-workstream
handoff; all 16 gaps remain OPEN. A recovery pass does not close a parity gap.

## Acceptance evidence index

| Criterion | Implementation and executable evidence |
| --- | --- |
| A1 — universal architecture | Production frontend/host use `shoop_app_client` / `shoop_app::Service` / RT backend. `scripts/check_rt_engine_boundary.py` audits Linux, Windows, macOS and Wasm graphs; old app/engine/supervisor/worklet clients are excluded. UI/Lua ordered-service tests and actual browser core workflow pass. |
| A2 — authority | `shoop_app/tests/service.rs`: shared state/order/selection/target, admitted create/remove/routing, pending results and stable mappings survive replacement; sync initialization is fresh-session-only. |
| A3 — independent roles | `shoop_host/tests/recovery.rs`: siblings survive launcher exit; later control has no original handles; invalid startup rolls back; a ready host survives UI spawn failure. Packaged public-protocol companion probes cover socket and shared-memory startup. |
| A4 — attachment | Host tests cover incompatible greetings/versions, changed identity, early disconnect and busy owner without eviction. Client projection tests require a coherent snapshot and Ready acknowledgment before writes, reject partial/stale transfers and reconcile lost results without replay. |
| A5 — continuity | Service audio/MIDI tests preserve capture/playback/media across loss. Host subprocess recording/replacement test and 32-cycle recovery stress preserve IDs/names/progress/nonzero media. Real frontend subprocess tests kill the UI, paint four replacements, continue editing/media reads, then close explicitly. |
| A6 — operation consistency | Service tests inject loss across acceptance/submission/confirmation/publication; correlated outcomes, revision conflicts, retained watermarks, partial refusals and result eviction do not replay work or orphan survivors. |
| A7 — authority fencing | Service and host tests reject stale mutation/detach/shutdown; management survives data loss, replacement fences old authority. Explicit ownerless shutdown cannot evict a live lease and supports an abandoned shared-memory host without adding shared-memory UI reattachment. |
| A8 — explicit exit | Egui window-close/Cancel/choice tests and real frontend subprocess tests exercise both exits. Service/host tests verify MIDI release, render stop, actual cleanup before confirmation, uncertain retry and cleanup refusal. UI destruction has no host-stop path. |
| A9 — browser execution | Actual Chromium main-thread client/service plus RT AudioWorklet core, 64-command burst, waveform/MIDI reads, mocked Web MIDI and settings workflows pass. Worklet contracts/heap guards pass. Typed local delivery does not run a codec or wait synchronously. |
| A10 — portable/bounded | Portable service/client/protocol native and Node tests cover queue/operation/history/media limits, interrupted transfers, overload and departed-client reclamation. Four-target graphs exclude UI/Lua/app semantics from engine/worklet. Selected Miri cases below pass. |
| A11 — delivery | Full CI run [41306455452](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455452) passes all eight Linux/macOS/Windows/web debug/release jobs, coverage and bare-metal checks. Public packaged companion probes, real frontend recovery/exit tests, all 31 Node packages, hosted/direct-file Chromium and Firefox pass. CodeQL/docs/Codecov pass; no independent code-review verdict is available. |

## Full local gates and provenance

These checks ran on the Stage 7 candidate ending at `d69dea15`; subsequent
changes correct browser harness permissions/artifact paths and expose Windows ACL
adapter module lookup/diagnostics and make the native heap fixture's rendering
deterministic, then budget the recovery stress recording for Windows attachment
latency. Production and portable Miri sources are unchanged by that budget correction. The final frontend
preference reconciliation received its complete 63-test rerun after the broader
native run. Remote CI reruns the whole integrated tree.

- Warning-denying full workspace build; native nextest: **2404 passed**, four
  deliberately ignored failure canaries. Engine/backend reference subset:
  1182 passed, overlapping that total. Reference model remains executable (134
  native model tests plus two RT contracts), and old browser scenarios remain
  frozen under `reference/` for parity restoration.
- Fresh full Node suite: **2026 passed across 31 packages** with pinned Node
  22.22.2. Python script suite: **142 passed**. Formatting, Rust test-attribute
  policy, closed tracing inventory (315 production modules), capability-ledger
  policy and all four dependency-tree audits pass.
- `rt_runtime` and `shoop_loops` warning-denying bare-metal build passes after
  their no_std merge. Default CPAL, dummy-only native, Wasm and Trunk builds pass.
- Packaged Linux release public-protocol canaries pass recording, piano MIDI,
  playback, media reads, socket replacement/shared-memory startup and confirmed
  shutdown. Production package graphs exclude legacy runtime payloads.
- Hosted Chromium core/burst/media, mocked Web MIDI and settings save/reload/
  refusal/failing-save workflows pass. Microphone denial/retry passes after the
  harness clears its injected denial before granting capture. These use a real
  AudioWorklet with fake capture, not physical audio/MIDI hardware.
- Single-file HTML builds locally. Managed Chromium blocks `file:` navigation by
  organization policy; local direct-file execution is not claimed. Unmanaged CI
  validates the distributed direct-file artifact. Serving the embedded bundle
  over HTTP passes the complete workflow. Candidate CI's direct-file failure was
  a missing file: the harness used the default dist path rather than the identified
  release artifact. Passing its actual packaged HTML path corrected this; the
  complete CI web release job, including direct-file Chromium and Firefox, passes
  at `a6f73a18`.

## Measurements and limits

Linux 6.18.57 / glibc 2.41, x86_64, five affinity CPUs with a four-CPU cgroup quota,
16 GiB memory, Rust 1.99.1. Measurements use development builds and real dummy
callbacks in a shared managed container; they are not hardware latency guarantees.

- 32 recovery cycles: attach p50/p95 **6.79/8.65 ms**, confirmed control
  **11.81/14.11 ms**, host descriptors **6 before / 6 after**. Enabling TCP_NODELAY
  reduced earlier roughly 52/56 ms attachment and 46/51 ms control observations.
  Scheduling and telemetry remain asynchronous; admission is not render progress.
- 128 structural snapshot iterations: 3 tracks/17 loops, 1782 encoded bytes:
  typed queue 26 microseconds total, codec round trip 26,275 microseconds total.
  Synthetic 64 tracks/256 loops, 18,676 bytes: 27 versus 239,632 microseconds.
  This isolates Arc typed delivery versus codec work; it is not end-to-end latency.
  Whole fixture times 461/453 ms include setup/rendering and are not a codec benchmark.
- Chromium's core + 64-command burst + media reads completed with **zero xruns**,
  no render-time memory growth and no command overflow; mocked MIDI also reported
  zero xruns. Cooperative control work is bounded per pump turn; engine/worklet
  allocation/destructor guards remain in their native/Node suites.
- Physical audio/MIDI devices are absent. Dummy, CPAL/midir mocks and browser fake
  capture ran; optional physical-device checks are not claimed. Shared-memory
  reattachment, host-failure recovery and Lua VM recovery remain explicit exclusions.

## Selected Miri, run last

All **35 test executions in 11 commands** passed using `nightly-2026-05-04`,
Miri `0.1.0 (ad3a598ca4 2026-05-03)`, source `d69dea15`, seed 0 and the existing
900-second watchdog. Reports and command logs are retained under
[evidence/miri](evidence/miri/). This is a selected ownership/codec/carrier check,
not the entire Miri inventory or a browser/OS/hardware claim. Workloads, assertions,
seeds and interpreter flags were not weakened.

| Package | Selection | Passed |
| --- | --- | ---: |
| `rt_runtime` | contracts, shard 1/34 | 12 |
| `shoop_loops` | media_decisions, shard 36/54 | 4 |
| `shoop_app_protocol` | version/discriminator/identity and hostile lengths | 2 |
| `shoop_app_client` | local Ready ordering and interrupted transfers | 2 |
| `shoop_transport` | library buffer/framing cases | 5 |
| `shoop_rt_protocol` | protocol and host targets | 4 |
| `shoop_rt_transport` | carrier and failure targets | 6 |

## Platform correction history

PR #899 is open against `feat/engine-ipc`; no merge is requested. The failed
candidate run was stopped while its last Node job was running so GitHub would
provide its completed log archive; per-job log/artifact downloads returned 403.
Windows package probes refused ACL validation. Diagnostic run
[41304154241](https://github.com/SanderVocke/shoopdaloop/actions/runs/41304154241)
identified an inherited PowerShell 7 module search path preventing Windows
PowerShell from loading its security module. The adapter clears `PSModulePath`
for that child; ownership and ACL checks remain enforced. These candidate failures
are resolved by the completed checkpoint below. Codecov reports 82.33%
patch coverage / 89.43% project coverage on `d69dea15`, satisfying the configured
80% patch floor and project tolerance. No code-review verdict has been received;
absence of feedback is not approval. Delivery checks for the completion
completion use the current PR head, retaining this executable-source evidence.

The next run at `a6f73a18` passed Windows debug and all its native host suites.
macOS release ran 1676 native tests: 1675 passed; the driver heap fixture received
`Refused(Busy)` for loop removal. Its automatic 32-frame render period gives the
control thread less than one millisecond per deadline; successful edits are not
guaranteed under arbitrary OS preemption. The fixture now requests each render
quantum after pumping control, executes the identical create/route/record/play/
remove sequence and exactly 960 recording frames (20 ms at 48 kHz), and retains
every `Done`, MIDI-reset, cleanup, guarded-cycle and zero-heap assertion. Independent
automatic-driver tests continue to exercise real-time pacing and telemetry.

The original case passed 20 unconstrained local release repetitions, 20 pinned to
one CPU and 20 with two CPU competitors on that CPU; the CI failure was not locally
reproduced. The corrected case passes the same 60 repetitions and all seven release
dummy-driver tests. This removes a wall-clock assumption from an allocation test;
it does not claim a production scheduling or latency fix. The completed integration
checkpoint below confirms the corrected fixture on all three native platforms.

A subsequent full native rerun passed 2403/2404 tests and caught the automatic
driver test comparing independently published counters during a callback
(51 callbacks, 50 completed buffers). That test now waits, within its existing
five-second bound, for a completed-callback observation before asserting the same
exact counter/frame equality and real-time pacing. Recorded workload/provenance
is retained in [native-heap-fixture.json](evidence/native-heap-fixture.json).
The complete corrected native suite passes **2404/2404**, with the same four
intentional failure canaries skipped; the release dummy-driver suite passes 7/7.

## Completed integration checkpoint

At `929ed71b`, all build/test jobs pass in
[41306455452](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455452).
CI independently confirms **31 Node packages / 2026 tests / zero failures**, both
hosted core and capture-denial/retry Chromium workflows with zero xruns, and the
release direct-file/Firefox workflows. Linux/macOS/Windows debug and release
package and native test jobs pass. The scheduled/manual CPAL-free nightly job is
intentionally skipped for this pull-request event; local CPAL-free builds pass.
The compact [CI evidence](evidence/ci-929ed71b.json) records source/run/job provenance.

All four CodeQL languages pass in
[41306455509](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455509),
documentation passes in
[41306455459](https://github.com/SanderVocke/shoopdaloop/actions/runs/41306455459),
and both Codecov checks pass. PR #899 has no review requests or code-review verdicts;
no independent reviewer was available in this execution and no approval is inferred.
The completion follow-up changes documentation/evidence and the native stress
fixture's recording budget, preserving production sources. Its
[current-head checks](https://github.com/SanderVocke/shoopdaloop/pull/899/checks)
remain the final delivery gate; the pinned executable evidence above is unchanged.

## Windows stress recording budget

The documentation completion `2dcf153e` received a Windows release failure in
[41309817206](https://github.com/SanderVocke/shoopdaloop/actions/runs/41309817206):
after 62.72 seconds, the 32-cycle test observed `Stopped` instead of `Recording`.
Its explicitly reduced 16 MiB media pool holds about 43.675 seconds for that
prepared two-channel loop. A [controlled native probe](evidence/recording-capacity.md)
reproduces exhaustion with one continuously connected client, and stays recording
through 120 rendered seconds with the normal 256 MiB default.

The stress fixture now uses that default while retaining 48 kHz, all 32 cycles,
every identity/recording/progress assertion, operation/result bounds and descriptor
reclamation. Other fixtures retain 16 MiB. Production defaults and exhaustion
behavior are unchanged, and all 2404 native tests pass after the correction.
The final-head CI link above gates delivery of this native-test-only follow-up.
