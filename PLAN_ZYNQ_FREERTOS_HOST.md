# Zynq-7000 FreeRTOS RT host and QEMU CI

## Status and delivery target

Execution authorized on 2026-11-02. Investigation used
`feat/trusted-lan-networking` at `5aeadd0b52229312349053617677a714d2d4e437`.
Implemented on `feat/zynq-freertos-rt-host` in
[PR #914](https://github.com/SanderVocke/shoopdaloop/pull/914),
**targeting `feat/backend-replacement`**, left unmerged. All three gates and implementation-head CI pass.
The final documentation head must pass the same checks before handoff.
The user retargeted delivery after networking merged as `540e863f`; this is the
implementation base. Its tree matches the investigated networking head.
The target subsequently advanced to `3aabf437`; that revision is merged into the
implementation branch. Its session protocol is version 11. Both firmware
launchers derive profile versions from the workspace protocol definition, so
future compatible rebuilds do not retain a stale version number.

The hardware assumption is **1 GiB of PS DDR (1024 MiB, 1,073,741,824 bytes)**.
Configure QEMU explicitly with `-m 1G`; its default is not the memory contract.
Use one Cortex-A9 core, `xilinx-zynq-a9`, FreeRTOS and AMD's lwIP port. The three
ordered gates are: platform TCP echo, complete Shoop host with dummy audio, then
recovery/resource CI.

## Goals and scope

- Run the existing authoritative session service and RT engine on the emulated
  Zynq processing system, controllable by the existing native UI over TCP.
- Supply a reproducible firmware build, bounded memory profile, dummy audio/MIDI
  integration, and headless QEMU integration tests on ordinary Linux CI runners.
- Preserve the trusted-LAN branch's endpoint ownership, fresh synchronization,
  exclusive authority, accepted-work lifetime and confirmed-shutdown semantics.
- Deliver documented development commands, reproducible evidence and a green PR
  against `feat/backend-replacement`.

Physical audio/codec/FPGA integration, physical-board certification, flash/SD
production boot packaging, SMP/AMP, plugins, new engine features, network audio,
TLS, discovery and browser remote connectivity are outside scope. Firmware uses
RAM for live state; session files/codecs/scripts stay on the UI machine. Hardware
performance, cache/DMA correctness and hard real-time deadlines require later
board testing and must not be claimed from QEMU results.

## Immutable acceptance criteria

1. **Gate 1:** A clean scripted build produces an ELF that boots FreeRTOS and
   AMD lwIP on QEMU `xilinx-zynq-a9` with one core and exactly 1 GiB of DDR. A host
   process exchanges exact TCP payloads through the emulated GEM controller.
   Serial output alone, mocked sockets, Linux, and FreeRTOS+TCP do not pass.
2. **Gate 2:** The full Shoop host exposes the existing authenticated data and
   management endpoints. The native UI attaches without an embedded-specific
   protocol fork, displays host settings/state, controls primitive loops and
   saves/loads supported sessions. Actual engine rendering, recording, playback
   and sample/MIDI content are verified with deterministic dummy I/O.
3. UI loss leaves rendering, recording and already accepted operations running.
   A fresh attachment observes current state without replay. Wrong credentials,
   incompatible versions, competing controllers and stale authority are refused;
   a healthy controller is not displaced. Shutdown confirms render stop, dummy
   MIDI cleanup and resource cleanup before reporting success.
4. All target-side Rust production dependencies support `no_std` + `alloc`.
   Firmware links with a consistent Cortex-A9 hard-float ABI, working allocation,
   atomics, FPU context preservation, monotonic time and diagnostic failure paths.
   Rendering performs no allocation/deallocation, networking, blocking or
   unbounded control work after preparation.
5. Firmware sections, stacks, all heaps and DMA/network reservations fit the
   1 GiB profile without overlaps or reliance on extra emulated RAM. Resource
   admission accounts for peak simultaneous ownership. Oversized/over-budget
   requests receive explicit refusal before unsafe allocation or live-state
   mutation; expected capacity exhaustion does not crash or silence the session.
6. The accepted profile supports an exact session round trip containing **40 MiB
   of audio plus representative MIDI** through segmented persistence, exceeding
   the old 32 MiB message limit. This does not promise the desktop's 256 MiB
   transfer ceiling or any recording duration beyond available pool capacity.
7. **Gate 3:** PR CI builds and boots real firmware, exercises both a production
   dummy-host smoke test and deterministic end-to-end/recovery/resource tests,
   and fails on timeouts, protocol/content mismatches or exceeded budgets.
   Existing portable/native/browser regression gates remain green.
8. Finish with the implementation PR targeting `feat/backend-replacement`.
   Required and applicable checks pass on its final head; arriving automated
   review is inspected and addressed. Evidence distinguishes library builds,
   simulated portable tests, actual QEMU firmware execution and untested hardware.

## Existing foundation and integration points

Investigation already passed warning-denying library builds for
`armv7a-none-eabi` and `armv7a-none-eabihf` with `target-cpu=cortex-a9`, plus all
16 tests in `shoop_transport` and `shoop_app_connection`. Those investigation-only results predated firmware integration; the actual
firmware evidence below covers all three gates.

Reuse `shoop_app_connection::{Session, Management}`, the `shoop_app` service,
`shoop_rt_backend`, and the prepared engine/runtime/loop crates. Implement
`shoop_transport::stream::ByteIo`; reuse `Stream` and `handshake::Handshake`.
Use `shoop_host/src/serve.rs` and `shoop_rt_host_native` as lifecycle references,
not target dependencies: their socket, thread, file and clock implementations
depend on `std`.

Resource policy changes belong at portable ingress/service/backend admission
points, including `shoop_app_protocol/src/persistence.rs`, service persistence
preparation/capture and `shoop_rt_backend` preparation. The current desktop pool
default is 256 MiB; persistence also permits 256 MiB of decoded media. Neither
is a safe board-wide peak-memory budget by itself. Preserve the existing
`docs/native_session_recovery/persistence-contract.md` semantics.

The current `build_and_test.yml`, `codeql.yml` and `docs.yml` PR base filters
already include the revised target `feat/backend-replacement`. Preserve these
checks and add the firmware job for that target; absent checks are not passes.

## Design rules and important constraints

- Keep AMD startup, exception vectors, FreeRTOS, lwIP and GEM/cache setup in a
  small C/BSP layer. Link a Rust static library through a narrow C ABI with
  explicit ownership, buffer lifetimes and error mapping; never unwind over FFI.
- Add a `no_std` embedded host composition library, provisionally
  `src/rust/shoop_host_freertos`, plus board/build material under
  `platforms/zynq7000`. Keep the firmware entry point, panic handler and global
  allocator target-specific so native workspace builds/tests remain usable.
  Reuse shared behavior; keep platform policy out of the engine and protocols.
- Start with single-core preemption. One high-priority render task owns `Render`;
  a lower-priority control task owns the service, RT backend, engine endpoint and
  session pumps. lwIP input/TCP-IP tasks and interrupts retain their required
  execution contexts. Use prepared queues/atomics, not a mutex shared with render.
- Use `armv7a-none-eabihf`, `target-cpu=cortex-a9`, compatible C floating-point
  flags, and `panic=abort`. Preserve FPU context for all tasks that may use it,
  including compiler-generated instructions; verify interrupt/context-switch
  handling. Preserve the existing pointer/32/64-bit atomic requirements.
- The dummy profile is 48 kHz, stereo, 256-frame quanta with dummy MIDI ports.
  Timer interrupts notify the render task; fractional-period accounting avoids
  treating 5.333 ms as a fixed five-tick delay. Real elapsed time drives leases;
  sample frames drive audio. Handle counter wrap without backward time jumps.
- Test-only frame control, input injection and output inspection must be absent
  from production firmware. Use them to assert exact samples/MIDI through the
  real render task, not to replace FreeRTOS, lwIP or the engine with mocks. Test
  the production periodic driver separately for continued progress.
- Socket calls return immediately, preserve partial I/O/EOF/error semantics and
  copy/retain buffers safely. Use bounded listeners, pending handshakes and
  per-connection queues; start with the native LAN's 4/4 connection limits and
  8/1/8 reliable/telemetry/bulk records. Size lwIP PCB/pbuf/mailbox/send pools to
  include pending sockets and closing/TIME_WAIT connections, not just live owners.
- Emit the data greeting, supply fresh connection nonces/heartbeat seeds, call
  `Service::tick` once per service turn, and detach retired connections. Keep the
  engine endpoint alive across UI loss. Honor the portable shutdown state machine.
- Preserve the existing wire format where possible. Any necessary shared schema
  change must deliberately update versions, both consumers and canonical fixtures.
  Keep UI-local credential-file validation and the existing trusted-LAN policy.
  Provision the board token explicitly; known CI tokens/seeds are test fixtures,
  never production defaults. Document nonce provisioning without assuming OS RNG.
- Pin Rust, the Arm C toolchain, QEMU, AMD embeddedsw/FreeRTOS/lwIP and build
  inputs. Use a scripted headless build without an interactive Vitis installation
  or licensed FPGA synthesis in PR CI. Keep any necessary generated BSP/platform
  inputs with provenance, licenses and a documented regeneration path.
- Resolve PHY address/negotiation, reset/clock assumptions and startup differences
  at the BSP boundary. Do not bypass the GEM/lwIP path to make CI pass. Prefer
  upstream QEMU; any required AMD QEMU version or small patch must be pinned,
  justified and reproducible.

### Measured memory policy

The implemented subdivisions below include the documented arena/stack revisions
in the execution evidence. The 1 GiB ceiling and acceptance criteria remain fixed.

| PS DDR allocation | Maximum |
| --- | ---: |
| Firmware, BSP/C/FreeRTOS heaps, task/exception stacks, DMA and all lwIP pools | 128 MiB |
| Rust arena, including engine recording pages and all service/staging allocations | 768 MiB |
| Unassigned headroom, unavailable to either allocator | 128 MiB |
| Total | 1024 MiB |

Start with 8 tracks, 32 loop slots, 2 channels and a **128 MiB recording-page
pool inside the Rust arena**, not in addition to it. Start with a 64 MiB aggregate
decoded-media job limit and retain protocol ceilings for metadata/object/event
counts as outer bounds; impose smaller local limits where the measured peak
requires them. Account for raw ingress, assembly, canonical copies, retained old
content, replacement resources, snapshots, MIDI indexes and allocator overhead.

Derive actual linker/arena placement from the board-valid DDR window, reserving
startup areas and cache-aligned DMA memory explicitly. Assert region bounds and
non-overlap from the link map. Measure C/lwIP and Rust heaps separately and task
stack high-water marks. The global allocator must honor Rust layout alignment;
an OOM hook is diagnostic containment, not normal admission control. Changes to
these initial subdivisions/limits require recorded measurement, while still
satisfying every immutable criterion.

## Staged implementation

### Stage 0 — Establish the implementation branch and build contract

- [x] Fetch/inspect the current base and local changes; create
  `feat/zynq-freertos-rt-host` from the now-merged `feat/backend-replacement`. Record the actual
  base SHA and reconcile any movement since the investigation without discarding
  unrelated work. Carry this plan onto the implementation branch.
- [x] Establish the platform/firmware layout and lock/build-input strategy. Record
  version pins, source hashes/licenses, the reference PS configuration and the
  single command each for build, run and test. Use `work/` for transient outputs.
- [x] Re-run the portable bare-metal build and focused connection/transport tests
  to establish the branch baseline; record outcomes and commit the setup milestone.

Verification: all selected production dependencies resolve without `std` for the
target; the memory/ABI/CI commands and source provenance are reviewable. Any
separate firmware manifest must have its own reproducible locked resolution.

### Stage 1 — Gate 1: FreeRTOS + AMD lwIP TCP echo

Depends on Stage 0; prove this gate before integrating Shoop.

- [x] Build the smallest AMD FreeRTOS socket-mode echo application with PS UART,
  SCU private timer/GIC, GEM and 1 GiB-compatible BSP/linker settings. Park the second core.
  Record ELF entry/load addresses, startup ownership and stack/heap regions.
- [x] Add the QEMU launcher using `-M xilinx-zynq-a9 -accel tcg -smp 1 -m 1G`,
  direct ELF loading, captured serial output and a local QMP endpoint. Use
  unprivileged user networking with loopback-only TCP forwarding and a compatible
  guest IPv4 address, initially `10.0.2.15`. Avoid privileged TAP/KVM requirements.
- [x] Resolve real BSP/QEMU incompatibilities, including PHY address/negotiation
  and reset/clock polling. Keep adaptations explicit and confined to the platform.
- [x] Add a host echo probe, readiness/overall deadlines, unconditional process
  cleanup, and link-map checks. Verify exact fragmented payloads, connection
  close/reopen and exchanges exceeding a single packet/socket buffer.
- [x] Add an initial Linux CI job that builds and executes this probe from source;
  make its workflow run for PRs targeting the agreed base. Commit Gate 1 evidence.

Verification: clean and cached builds both pass; QEMU RAM size is queried/checked,
all mapped application regions fit, and TCP exchange succeeds via actual GEM/lwIP.
Broken boot, unavailable listener or wrong echo bytes fail the job. Retain serial,
versions, command line, ELF/map and probe results as CI artifacts.

### Stage 2 — Gate 2 foundation: Rust runtime, dummy render and memory admission

Depends on Gate 1; complete before exposing unrestricted persistence ingress.

- [x] Link the Rust static library into the proven C runtime. Implement allocator
  synchronization/alignment, panic/OOM/assert reporting, monotonic time and the
  narrow FFI/task handoff. Audit cache/DMA regions and FPU/task context settings.
- [x] Prepare the real engine, bounded buffers and MIDI cleanup outputs. Run its
  render owner in the timer-notified task; expose driver status, frames, xruns
  and cleanup completion. Provide a FreeRTOS-aware `EngineClock` where waiting
  is needed, without busy-waiting and starving render/network tasks.
- [x] Implement the initial memory profile and explicit configurable admission
  limits across ingress, service capture/assembly and backend preparation. Preserve
  native defaults. Reject excessive declarations and cumulative object/event
  growth before allocation, including capture/export and replacement overlap.
- [x] Add fixture-only deterministic input, frame control and output inspection;
  guard allocation/deallocation during rendering and report heap/stack usage.
  Keep both periodic-production and controlled-test modes exercised.
- [x] Verify prepared render calls, scheduling progress and actual sample/MIDI
  processing under task switching. Test allocator alignment, time wrap, pool
  exhaustion and failed preparation leaving the current session intact. Commit.

Verification: a complete hard-float firmware links without Rust `std`, produces
deterministic render output, preserves floating-point state across preemption,
and stays within the measured 1 GiB profile. Capacity tests produce refusals
rather than relying on a fatal allocator failure.

### Stage 3 — Gate 2 completion: networked Shoop host

Depends on Stage 2.

- [x] Implement the nonblocking lwIP `ByteIo` adapter and bounded accept/auth
  progression. Integrate the existing stream, handshake, session and management
  pumps with their greetings, authority, heartbeat and scheduling contracts.
- [x] Compose `shoop_app`, `shoop_rt_backend` and the in-process engine endpoint;
  implement target initialization, settings/status reporting, detach and confirmed
  stop/cleanup. Provide explicit token/profile generation for development without
  requiring a guest filesystem or native PID registry.
- [x] Add a headless native integration client using the public app protocol/client
  path, not only the lower-level engine protocol. Verify attach, coherent snapshot,
  track/loop creation, controls, deterministic record/playback, exact media and
  management queries. Check dummy output samples/MIDI as well as reported state.
- [x] Demonstrate the existing native UI connecting via a remote profile, editing
  loops, displaying host settings and saving/loading a supported session. Keep
  settings local/remote ownership and unsupported-capability reporting intact.
- [x] Round-trip 40 MiB of audio plus MIDI through actual firmware TCP persistence
  and compare exact content and supported session structure. Verify controls and
  heartbeats continue during transfers; run the production dummy-host smoke.
- [x] Document build/run/connect/shutdown commands and the supported core feature
  set, initial limits and measured memory. Commit Gate 2 evidence.

Verification: all Gate 2 acceptance criteria have actual firmware/client evidence;
the periodic driver advances with no UI attached. UI files stay client-side;
unsupported sessions are refused without partial replacement. No test hook is
present in the production image.

### Stage 4 — Gate 3: recovery, resource limits and permanent CI

Depends on Gate 2; extend the Stage 1 CI job to the complete firmware.

- [x] Test bad tokens, protocol versions, slow handshakes, competing controllers,
  stale requests and bounded listener/closing-socket exhaustion. Prove an existing
  healthy owner remains usable and resources recover after peers close.
- [x] Drop/blackhole one and both TCP directions while recording and during
  upload/download. Verify heartbeat expiry, continued engine progress, fresh
  reattachment, released pre-commit staging and host-owned post-commit completion.
  Cover lost confirmations, host restart at the same endpoints and no replay.
- [x] Test authenticated shutdown and ownerless shutdown under existing authority
  rules, including cleanup failure reporting and retry behavior. Prove stopped
  rendering and dummy MIDI/resource cleanup rather than inferring them from exit.
- [x] Exercise oversized declarations, inconsistent totals, many small objects,
  MIDI limits, full recording pools, replacement overlap and capture/export
  pressure. Assert explicit refusals, unchanged usable playback and reclaimed
  resources; guard render allocations during active control/transfers.
- [x] Run at least 100 small attach/transfer/detach or cancelled-load cycles with
  warmed-up post-cleanup heap/object/socket counters; reject retained live-resource
  growth. Keep a longer optional soak separate from bounded PR tests.
- [x] Keep deterministic protocol/content assertions independent of host wall
  speed. Use guest monotonic time for network expiry and bounded host deadlines
  for hangs; do not assert physical audio latency or desktop fixture throughput
  against TCG. Preserve the existing portable latency fixture separately.
- [x] Add firmware hard-float linking and dependency/boundary checks alongside
  existing bare-metal gates. Enable applicable build/test, CodeQL and docs PR
  filters for `feat/backend-replacement`; retain native and browser coverage.
- [x] Archive firmware/map, pins, serial logs, QEMU/QMP launch facts, test summaries
  and heap/stack maxima on failures; retain compact pass evidence. Cache only
  reproducible builds, enforce job timeouts and clean up QEMU/forwarded ports.
  Commit Gate 3 implementation and evidence.

Verification: clean-runner CI exercises actual FreeRTOS/lwIP firmware, both
production smoke and deterministic fixtures, all three gates and negative paths.
Deliberately break an echo/content/boot assertion once to verify job failure and
diagnostic capture, then restore it before committing the green milestone.

### Stage 5 — Final end-to-end validation and PR delivery

Depends on all three gates; use the repository-prescribed build environment.

- [x] Rebuild from clean inputs and execute the entire QEMU suite at `-m 1G`.
  Repeat the native UI workflow: attach, create/record/play, close UI only,
  reconnect to ongoing state, save/load, query settings and confirm host shutdown.
- [x] Run `cargo fmt --all` and the warning-denying build before Rust commits;
  final checks include `cargo fmt --all -- --check`,
  `RUSTFLAGS='-D warnings' cargo build --workspace`, firmware build/link and the
  existing Cortex-A bare-metal library builds. Run
  `python3 scripts/check_shoop_test_usage.py` if Rust tests changed, relevant
  boundary audits and `python3 scripts/check_tracing_coverage.py --require-closed`.
- [x] Run the complete Rust suite with
  `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci`,
  plus affected native LAN/persistence and browser regression gates. Use no
  missing-backend allowance to substitute for the actual embedded integration.
- [x] Review the memory ledger, image/map, production/test feature separation,
  dependency boundaries, scope and acceptance evidence; update this plan and the
  LAN/platform documentation. Commit each remaining meaningful milestone.
- [x] Push `feat/zynq-freertos-rt-host` and create/update one PR with
  **base `feat/backend-replacement`**. Explain the resulting firmware behavior,
  usage, three gate results, memory maxima and hardware validation limits. Do not
  retarget the existing networking PR or merge either PR as part of this task.
- [x] Monitor all required/applicable checks on the PR's current head. Inspect
  failure logs, fix failures and rerun affected checks until green. Reconcile
  target-branch movement and repeat affected validation when necessary.
- [x] Inspect automated review submissions, inline comments and check feedback;
  address substantive findings, push fixes and repeat review/CI until the reviewer
  approves. If no automated reviewer responds, record that fact without claiming
  approval. Final handoff includes the PR URL, final head/check results, evidence
  for every immutable criterion and a clean worktree. Leave the PR unmerged.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Don't reformat code or apply unnecessary whitespace changes.
- Comments:
  - Use sparingly. In most cases the code should explain itself.
  - Don't refer to code in other files or even to other filenames that use code;
    this leads to synchronization issues later. When referring to usage patterns,
    speak in generic terms as opposed to specific callsites in the codebase.
  - Be concise.
  - Don't refer to how the code was before your changes.
  - Don't refer to the instructions you were given to write the code.

## Platform references

- [QEMU Zynq model](https://www.qemu.org/docs/master/system/arm/xlnx-zynq.html)
- [QEMU networking](https://www.qemu.org/docs/master/system/devices/net.html)
- [AMD FreeRTOS support and lwIP examples](https://xilinx-wiki.atlassian.net/wiki/spaces/A/pages/18842141/FreeRTOS)
- [Rust Armv7-A bare-metal targets](https://doc.rust-lang.org/rustc/platform-support/armv7a-none-eabi.html)

Pin the actual working source/tool releases during Stage 0/1; these rolling
documentation links are background, not reproducible build inputs.

## Execution evidence

- Delivery: [PR #914](https://github.com/SanderVocke/shoopdaloop/pull/914), base
  `feat/backend-replacement`, originally at `540e863f` (the investigated
  `5aeadd0b` tree). Target movement to `3aabf437` is merged and requalified.
- Milestones: `ba4e2adf` establishes pins/layout; `ddd7cd4e` proves echo;
  `64360a1c` introduces configurable admission; `dee441c3` integrates the complete
  host and CI. Subsequent fixes cover the runner container's checkout ownership,
  orderly fixture TCP close, monotonic transfer keys and explicit non-null
  allocation evidence. The complete implementation and fixture synchronization are at `9d1003cc`,
  before the final evidence/documentation commit.
- Full local `suite.py --fetch`: all seven phases passed in one run, with rebuilt
  firmware and the pinned AMD checkout: echo build, echo, intentional failing
  content canary, production/restart, native UI, recovery and the complete fixture.
  Total host time was 12 minutes 46 seconds. Each VM's QMP facts verified one CPU
  and exactly 1,073,741,824 RAM bytes. Evidence is under `work/zynq/suite/` and
  the corresponding per-phase directories; CI archives the same evidence.

| Immutable criterion | Executed evidence |
| --- | --- |
| 1 — platform | Actual FreeRTOS/AMD lwIP/GEM exchanged 3,198,120 exact bytes over three TCP connections, including fragmentation and reopen. Corrupt echo bytes intentionally fail with captured diagnostics. |
| 2 — complete host/UI | Production ELF, public native app client and real native UI paint/settings/edit/record/play/close/reattach/save/load/shutdown paths passed. Exact stereo PCM, MIDI payloads and their 512-frame separation reached real engine dummy outputs. |
| 3 — authority/recovery | Wrong credentials/version, 12 slow handshakes and a competitor preserve the owner. All three TCP direction blackholes expire leases while recording continues. Upload/download loss reclaims staging or completes accepted commit, then recovers exact media. Restart refuses old authority; lost ownerless shutdown reply reconciles the same operation. All 32 MIDI reset packets are acknowledged before confirmed cleanup; stopped-owner failure/retry passes. |
| 4 — runtime/boundaries | Cortex-A9 soft/hard-float and AArch64 no_std builds passed. Actual firmware verifies allocator alignment through 4096 bytes, 64-bit atomic carry, FPU task/IRQ preservation and the monotonic timer low-word carry. Every render call guards allocation and deallocation. Production ELF audits exclude fixture symbols and markers. |
| 5 — memory/admission | Linker/load checks enforce the 1 GiB layout. Firmware refuses excess metadata/media/objects/MIDI events and inconsistent totals. A real full recording pool preserves playback; over-budget export and a small raw image with oversized normalized duration are refused without changing the session. Teardown leaves zero Rust allocations, socket slots and netconns. |
| 6 — segmented persistence | 41,943,040 audio bytes plus MIDI and supported session structure round-trip exactly; the loaded session also renders its distinct stereo pattern through the engine. |
| 7 — permanent CI | The pinned-container firmware job runs all seven phases, with bounded process ownership/timeouts and compact pass evidence plus failure-only packet/register captures. Existing native/browser/coverage/bare-metal/docs/CodeQL jobs remain enabled. Clean-runner firmware CI passes all seven phases. |
| 8 — delivery/review | The PR targets the requested base and remains unmerged. CodeQL's two allocator-probe findings were fixed with an explicit NonNull guard; its rescan passes and both automated threads are resolved. All implementation-head checks pass; the documentation head is checked again before handoff. |

### Memory ledger and measured margins

| Allocation/accounting | Bound or observed value |
| --- | ---: |
| Platform reservation, including low startup area | 128 MiB |
| Highest platform ELF load end | 48,234,496 bytes (46 MiB) |
| Rust arena | 0x08000000–0x38000000: 768 MiB |
| Unavailable headroom above the Rust arena | 128 MiB |
| Recording pages, inside the Rust arena | 128 MiB |
| Newlib / FreeRTOS / lwIP heap reservations | 16 / 16 / 4 MiB |
| Prepared startup Rust use | 344,480,136 bytes (328.52 MiB) |
| Combined-suite Rust peak | 407,460,812 bytes (388.58 MiB) |
| Largest qualified-fixture Rust peak observed | 407,466,188 bytes (388.59 MiB) |
| Minimum FreeRTOS free heap observed | 16,168,840 bytes |
| Minimum control / render stack free observed in local/CI fixtures | 37,431 / 41,901 words |
| Minimum other task stack free | 929 words |
| Combined-suite lwIP heap peak | 6,032 bytes |
| Final Rust use / application sockets / netconns | 0 / 0 / 0 |

The board ceilings are 64 MiB decoded media, 1 MiB metadata, 512 objects and
65,536 MIDI events. One persistence job owns ingress/assembly; immutable content
is shared with Arc and engine uploads stream fixed chunks into the prepared
pool. Admission includes aggregate canonical content and normalized duration,
rather than granting the desktop's 256 MiB ceiling. Native defaults are unchanged.
A 100-cycle measured soak after ten warm-up cycles, with the 40 MiB session still
loaded, retained exactly 386,474,392 Rust bytes before and after, with unchanged
FreeRTOS free heap and socket count.

### Supported design revisions and integration findings

- The measured prepared baseline was 328.52 MiB. Increase the original 512 MiB
  Rust arena to 768 MiB to allow retained content plus ingress/assembly ownership;
  reduce unassigned headroom to 128 MiB. The total stays 1 GiB. A 64 KiB render
  stack initially left only 249 words, so both owner tasks now have 256 KiB stacks.
- Use AMD's SCU private timer for the RTOS tick and the 64-bit PS global timer for
  elapsed time. The fractional render notification phase implements 48 kHz/256
  without approximating the quantum to five ticks.
- Recorded AMD patches fix a duplicate clear-on-read GEM ISR read and preserve
  FPU registers through the FreeRTOS IRQ assembly wrapper. Source pins, licenses,
  patch hashes, ELF hashes and map/launch facts are retained; QEMU is unmodified.
- Nonblocking BSD descriptors can still wait on lwIP's API semaphore. A separate
  broker owns them; Rust ByteIo uses bounded SPSC rings. Orderly receive draining
  and write-half close prevent a fixture from aborting its Detach with unread
  telemetry; normal reconnects leave no guest TIME_WAIT accumulation.
- The global allocator's alignment probe now constructs NonNull before pointer
  access. The updated probe passes in QEMU; CodeQL rescan passes and resolves its
  two findings without suppressions or coverage-policy changes.

### Regressions and final CI

Local checks passed: all 2,517 native tests (five intentionally skipped), all 81
affected Node tests, hosted Chromium's core audio/MIDI/file workflow, the three
Cortex-A library targets, warning-denying workspace/firmware builds, formatting,
Rust test-attribute policy, all boundary audits and the closed 355-module tracing
inventory. The ignored external-firmware UI test executes through the actual
QEMU harness. No missing-backend allowance substitutes for firmware integration.

All required/applicable checks passed on implementation head `9d1003cc`:
[firmware](https://github.com/SanderVocke/shoopdaloop/actions/runs/41522777971),
[native/browser/coverage](https://github.com/SanderVocke/shoopdaloop/actions/runs/41522778021),
[CodeQL](https://github.com/SanderVocke/shoopdaloop/actions/runs/41522777973) and
[docs](https://github.com/SanderVocke/shoopdaloop/actions/runs/41522777993).
The complete check list includes 19 successful checks; only the scheduled/manual
CPAL-free nightly is intentionally skipped for a PR. Codecov patch status is
91.66% against its unchanged 80% floor. CodeQL's two review threads are resolved;
no additional substantive automated review arrived on that head.

The clean CI firmware suite completed all seven phases in 16 minutes 21 seconds.
Its [verified execution artifact](https://github.com/SanderVocke/shoopdaloop/actions/runs/41522777971/artifacts/13178787967)
contains ELF/map/pins, launch/QMP facts, serial output, per-phase results and
heap/stack accounting. The artifact ZIP digest is
`5a8392a157cc0948f70481de3d3ef7dca544bc9243f29786ad25e227dae4a13a`.
Its production, UI, recovery and full-fixture result records all pass and report
zero final Rust allocations, application socket slots and netconns.

CI packet evidence also identified a fixture revision-fence race after a Playing
command was confirmed but before its snapshot became visible. Waiting for
observed Playing makes the subsequent capacity probes valid; they still require
the exact Overloaded refusal. The corrected complete firmware CI passes.
The final workflow artifact paths additionally include nested restart logs,
QMP facts and failure diagnostics. Final-documentation-head checks must pass
before handoff and remain visible on the PR's current check list.

The target branch subsequently merged master and advanced its application
protocol from 10 to 11. CI correctly rejected the stale launcher-generated
profile. The target update is merged; interactive and test launchers now share
profile generation using the workspace's authoritative protocol constant.
The complete firmware suite and native regressions are rerun after this update;
the PR's final head must pass the same complete CI set before handoff.
On the merged target, the warning-denying workspace build, formatting, Rust
test-attribute policy and closed 356-module tracing inventory pass. All 2,537
native tests pass (five intentionally skipped).
The merged-target local firmware suite passes all seven phases in 14 minutes
27 seconds, including both production boots, native UI, recovery, the full
40 MiB media gate, recording-pool exhaustion and 100 warmed reconnect cycles.
Peak Rust use is 407,460,812 bytes; final Rust use, sockets and netconns are zero.

Physical audio/FPGA wiring, silicon cache/DMA validation, physical real-time
performance and production boot packaging remain outside the agreed scope.
