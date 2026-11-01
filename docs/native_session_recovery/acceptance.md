# Native session recovery acceptance

Implementation source: `d69dea15301e1ddfcc9b4c75a2d11e496ce39b8f` on
`plan/native-session-recovery`, pushed to origin in
[PR #899](https://github.com/SanderVocke/shoopdaloop/pull/899), based on
`feat/engine-ipc`. Final platform CI is **pending fixes**, not accepted yet.
The [plan](../../PLAN_NATIVE_SESSION_RECOVERY.md) retains the fixed A1–A11 contract.
The [feature parity ledger](feature-parity.md) is the mandatory next-workstream
handoff; all 16 gaps remain OPEN. A recovery pass does not close a parity gap.

## Acceptance evidence index

| Criterion | Implementation and executable evidence |
| --- | --- |
| A1 — universal architecture | Production frontend/host use `shoop_app_client` / `shoop_app::Service` / RT backend. `scripts/check_rt_engine_boundary.py` audits Linux, Windows, macOS and Wasm graphs; old app/engine/supervisor/worklet clients are excluded. UI/Lua ordered-service tests and actual browser core workflow pass. |
| A2 — authority | `shoop_app/tests/service.rs`: shared state/order/selection/target, admitted create/remove/routing, pending results and stable mappings survive replacement; sync initialization is fresh-session-only. |
| A3 — independent roles | `shoop_host/tests/recovery.rs`: siblings survive launcher exit; later control has no original handles; invalid startup rolls back; a ready host survives UI spawn failure. Packaged public-protocol companion probes cover socket and shared-memory startup. |
| A4 — attachment | Host tests cover bad credentials/version/identity, early disconnect and busy owner without eviction. Client projection tests require a coherent snapshot and Ready acknowledgment before writes, reject partial/stale transfers and reconcile lost results without replay. |
| A5 — continuity | Service audio/MIDI tests preserve capture/playback/media across loss. Host subprocess recording/replacement test and 32-cycle recovery stress preserve IDs/names/progress/nonzero media. Real frontend subprocess tests kill the UI, paint four replacements, continue editing/media reads, then close explicitly. |
| A6 — operation consistency | Service tests inject loss across acceptance/submission/confirmation/publication; correlated outcomes, revision conflicts, retained watermarks, partial refusals and result eviction do not replay work or orphan survivors. |
| A7 — authority fencing | Service and host tests reject stale mutation/detach/shutdown; management survives data loss, replacement fences old authority. Explicit ownerless shutdown cannot evict a live lease and supports an abandoned shared-memory host without adding shared-memory UI reattachment. |
| A8 — explicit exit | Egui window-close/Cancel/choice tests and real frontend subprocess tests exercise both exits. Service/host tests verify MIDI release, render stop, actual cleanup before confirmation, uncertain retry and cleanup refusal. UI destruction has no host-stop path. |
| A9 — browser execution | Actual Chromium main-thread client/service plus RT AudioWorklet core, 64-command burst, waveform/MIDI reads, mocked Web MIDI and settings workflows pass. Worklet contracts/heap guards pass. Typed local delivery does not run a codec or wait synchronously. |
| A10 — portable/bounded | Portable service/client/protocol native and Node tests cover queue/operation/history/media limits, interrupted transfers, overload and departed-client reclamation. Four-target graphs exclude UI/Lua/app semantics from engine/worklet. Selected Miri cases below pass. |
| A11 — delivery | Local Linux debug/release packages and public companion workflows pass. Candidate CI run [41302375014](https://github.com/SanderVocke/shoopdaloop/actions/runs/41302375014) passes Linux/macOS, coverage and bare-metal jobs but exposes Windows packaging and web release smoke failures. Final-head CI/review remain open. |

## Full local gates and provenance

These checks ran on the Stage 7 candidate ending at `d69dea15`; subsequent
changes correct browser harness permissions/artifact paths and expose Windows ACL
adapter diagnostics. Portable Miri sources are unchanged. The final frontend
preference reconciliation received its complete 63-test rerun after the broader
native run. Remote CI reruns the whole integrated tree.

- Warning-denying full workspace build; native nextest: **2404 passed**, four
  deliberately ignored failure/hardware canaries. Engine/backend reference subset:
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
  must validate the distributed direct-file artifact. Serving the embedded bundle
  over HTTP passes the complete workflow. Candidate CI's direct-file failure was
  a missing file: the harness used the default dist path rather than the identified
  release artifact. The workflow now passes its actual packaged HTML path.

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

## Delivery and review

PR #899 is open against `feat/engine-ipc`; no merge is requested. The failed
candidate run was stopped while its last Node job was running so GitHub would
provide its completed log archive; per-job log/artifact downloads returned 403.
Windows package probes refused ACL validation; the next run preserves and reports
the adapter's underlying error. Candidate CI failures are being corrected before
final acceptance. Codecov reports 82.33%
patch coverage / 89.43% project coverage on `d69dea15`, satisfying the configured
80% patch floor and project tolerance. No code-review verdict has been received;
absence of feedback is not approval. Final status will name the final branch head
and its CI run rather than treating an earlier green head as proof.
