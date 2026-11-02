# Network DSP host: implementation plan for stages 1–3

Status: Stages 1–2 implemented and verified; Stage 3 and full sweeps are pending.

Base: `feat/backend-replacement`, inspected at
`540e863f8c6b1d9ecfd6f18c6ff49bc9fdda6ef6` on 2026-11-02.
Implementation branch: `feat/network-dsp-host`, created from that branch, never
from `master`. Follow `.agents/prompts/write_plan.md` throughout execution.

## Goals and scope

Build a standalone network processing service that hosts Carla through its native
library and can also run direct DSP or other plugin hosts through the same DSP
interface. Build a simulated FPGA requester, a known-delay copy DSP implementation,
and repeatable localhost experiments before involving FPGA hardware.

This plan implements the first three agreed stages: extract the native adapter;
build the service and requester; validate timing/failure semantics. The abstraction,
delay-copy processor, and actual localhost parameter sweeps are included in those
stages. Writing this plan does not execute them.

In scope: native Carla reuse, generic DSP preparation/processing, bounded packet
and block adaptation, portable UDP, a separate reliable control connection,
audio/MIDI timestamp semantics, lifecycle/state handling, adversarial tests, and
same-machine measurements with separate client/server processes.

Out of scope: integration into `shoop_rt_engine`/`shoop_rt_backend` session graphs,
FPGA drivers or board execution, AF_XDP/RIO, hardware-NIC tuning, physical LAN or
cross-platform performance campaigns, automatic plugin-latency discovery and
graph compensation. Preserve supported native platform builds and existing Carla
behavior; these exclusions do not permit Linux-only assumptions in shared code.

## Repository starting points

- `src/rust/shoop_engine/src/carla_native.rs`: native library loading, descriptors,
  audio/MIDI buffers, state handling, synchronous processing, and main-thread UI.
- `src/rust/shoop_engine/src/carla_processor.rs`: useful buffer/processor contracts,
  mixed with legacy bridge and worker lifecycle concepts that should stay outside
  the new service's dependency graph.
- `third_party/carla/`: pinned runtime, ABI fixtures and packaging. Its README
  explicitly documents the absence of automatic Carla latency discovery.
- `src/rust/rt_runtime/src/processor.rs`: future integration boundary; render
  processing forbids external I/O or waiting and can receive sub-callback intervals.
- `docs/trusted-lan-networking.md`: existing session control, not audio streaming.
  Reuse suitable bounded framing/handshake utilities, not its session semantics.

## Immutable acceptance criteria

1. One DSP interface supports both a direct delay-copy processor and native Carla;
   codecs, block adaptation and deadline logic contain no Carla-specific behavior.
   Delay-copy builds and runs without Carla libraries, audio devices or the legacy
   engine. Carla is optional at build time.
2. The service loads Carla natively and calls it synchronously in its own process,
   without the Shoop processor bridge, shared-memory audio bridge, or an additional
   Shoop Carla worker subprocess. Existing Carla behavior and runtime checks remain
   covered through compatibility adapters where needed.
3. Every admitted stream supports configurable packet frames `P`, Carla/DSP frames
   `C = kP`, and pipeline allowance `N` packet periods. Unsupported shapes, limits,
   MTU sizes and structurally impossible deadlines are explicitly refused.
4. Returned audio retains its source frame identity. The requester consumes it at
   its predetermined deadline, independently of response arrival order. Results
   are emitted immediately when ready; `N` never means holding completed output
   on the server. Neither side adds an implicit audio-device clock or resampling.
5. Delay-copy produces exactly `y_c[n] = x_c[n-A]`, with zero for `n < A`, for each
   channel and configured integer delay `A >= 0`. Delay state survives processing
   and packet boundaries, resets on a new stream epoch, and has no artificial
   wall-clock sleep. Its known sample delay is distinct from network allowance.
6. Duplicate, reordered, missing, malformed, late and stale-epoch traffic follows
   explicit bounded rules. DSP input advances once in source order; deadline misses
   produce defined requester fallback without shifting subsequent sample slots.
   Startup, sustained overload, restart and stop/reconfigure behavior are covered.
7. Prepared codecs, adapters and delay-copy processing have bounded work/storage
   and no steady-state heap allocation. DSP ownership prevents concurrent unsafe
   state mutation; UI/state operations cannot block the processing worker. This
   does not assert real-time behavior for arbitrary third-party plugin code.
8. MIDI event offsets survive aggregation/splitting, overflow/loss has defined
   recovery, and Carla receives source-derived frame/transport information.
   Unknown algorithm latency is represented as unknown, not inferred to be zero.
9. Separate native processes exchange real UDP traffic on localhost. A script
   executes the specified `P/k/N` sweep with delay-copy and saves reproducible
   machine-readable measurements plus a reviewed report. Correctness and
   deadline misses are reported separately; no invented fixed latency target
   or hardware-LAN guarantee is required for acceptance.
10. A real pinned-Carla smoke run loads a lightweight plugin, processes known
    audio and exercises state save/restore; fake descriptors or an empty rack
    alone do not satisfy this criterion. Relevant regression tests, repository
    gates, supported-platform CI and automated review
    are completed with explicit evidence and any environment limitations recorded.

## Design rules and contracts

### DSP boundary and dependency direction

Use a small processor-neutral interface with preparation outside processing and
exclusive mutable ownership of each prepared instance. A factory prepares from
sample rate, fixed processing frames, port shape and algorithm configuration.
Its result advertises bounded audio/MIDI shape, supported operations, and
`Known(A)` or `Unknown` algorithm latency.

Processing borrows planar audio and bounded MIDI input/output buffers, together
with absolute source frame and optional source transport context. It cannot retain
those borrows. Both implementations use this same entry point. Optional state/UI
controls have an explicit control-side lifecycle; save/restore and reconfiguration
quiesce processing when the underlying implementation requires it.

Proposed crate boundaries; names may change with documented justification:

| Component | Responsibility |
| --- | --- |
| `shoop_dsp` | DSP contract, shape/capabilities, delay-copy, bounded processing errors |
| `shoop_carla_native` | Extracted native loader/adapter, state and UI ownership; no networking |
| `shoop_remote_dsp` | Wire schema, codecs, bounded packet/block adapters and stream state machines |
| `shoop_dsp_host` | Native server/requester executable, sockets, process lifecycle and measurement hooks |

Keep the shared DSP/protocol core usable with `no_std` + `alloc`; preparation may
allocate, prepared progression may not. Native sockets, clocks and Carla live in
native adapters. `shoop_carla_native` implements the generic DSP contract; legacy
`FXChainType` and worker lifecycle conversions remain in engine-facing adapters.
Do not make the generic interface depend on `shoop_engine` or its worker protocol.

### Timeline and buffering

- All audio uses one source sample rate. `P` and `C` are block sizes, not rates.
- For this standalone requester, packet `[s,s+P)` is nominally ready at
  `r(s) = t0 + (s+P)/fs`; its consumption deadline is
  `d(s) = r(s) + N*P/fs`. Use these nominal times even if the sender wakes late.
  A later FPGA integration must translate its rendering cadence to this contract.
- Aggregation consumes `(k-1)*P/fs` after the first packet is ready. Thus that
  accumulation plus turnaround and margin must fit `N*P/fs`; `N <= k-1` has no
  positive turnaround budget. Refuse such configurations and record the refusal
  in sweeps rather than counting them as passing runs.
- The requester owns final consumption decisions. The server uses bounded reorder
  storage, source progress and receiver-relative gap timers, without comparing
  clocks across machines. Expose reorder limits separately; their waits consume
  the total deadline allowance. A complete next block processes immediately.
- Delay-copy's `A` changes sample content, not timestamps or packet deadlines.
  Keep the same distinction for Carla's potentially unknown internal latency.
- Queues and memory ownership are explicit and preallocated. Reject excess future
  packets, discard duplicates/stale epochs, and never let backlog grow unbounded.
- After an input gap expires, fill its missing samples with zero, mark concealment,
  and advance the DSP once. Specify whether concealed results are accepted; the
  initial requester accepts marked zero-filled processing and counts it separately.
  Missing/late output uses silence. Define MIDI reset behavior for loss and restart.
- A short interruption preserves DSP order/state where bounded recovery permits;
  sustained overload or discontinuity resets through an explicit new epoch.
  Do not skip DSP calls merely because their output is late and silently compress
  the algorithm's timeline. Stop processing when the source stream is retired.

### Transport and native lifecycle

- Start with portable UDP and a separate bounded, versioned reliable control
  connection for stream negotiation, state transfer, status and stop/reconfigure.
  Use explicit endpoints and the repository's trusted-LAN credential conventions.
- Include stream incarnation, source frame, sequence, dimensions, flags and lengths
  in the audio schema; specify float32 layout/byte order and bounded MIDI records.
  Version and validate every field. No native structs/pointers on the wire.
- Enforce negotiated path-MTU payload limits, including audio/event headers, in
  both directions; reject oversized configurations initially. Do not depend on IP
  fragmentation. Channel sharding and custom Ethernet framing are deferred.
- The server worker receives, assembles, invokes the DSP, splits and sends. It has
  no periodic audio-backend callback. Socket waiting while idle is allowed here;
  user callbacks, filesystem work, UI work and unbounded control handling are not.
- Use control-side preparation and explicit handoff/quiescence for state changes.
  Preserve Carla main-thread UI rules, ABI/runtime validation and bundled helpers.
  Changing `P`, `C`, rate or port shape creates a new stream epoch.
- A future transport backend can reuse the same schema/state machines; introduce
  only the abstraction needed for UDP and test transports in this scope.

## Preparation

- [x] Create remote branch `feat/network-dsp-host` directly from the recorded
      `feat/backend-replacement` SHA using the GitHub connector.
- [x] Fetch that branch into a clean checkout/worktree before implementation.
      Reconcile any base-branch movement against the recorded SHA.
- [x] Read the target branch's `AGENTS.md`, `.agents/index.md`, mandates, style,
      build and test guidance; establish the required native/Carla baseline.
- [x] Add this plan on the implementation branch; code and benchmark steps remain
      unchecked. Record exact package/test commands as the components land.

## Stage 1 — Generic DSP, known-delay reference and native Carla extraction

- [x] Define the prepared DSP interface, metadata, error/overflow rules and
      control/processing ownership. Implement delay-copy first with preallocated
      per-channel history and explicit reset. Support delay zero and delays not
      divisible by either packet or processing frames.
- [x] Verify delay-copy independently using impulses and reproducible multichannel
      sample sequences, including `A = 0, 1, C-1, C, C+3`, wraparound, arbitrary
      preparation limits and reset. Use an independent sample-index reference.
- [x] Extract the native Carla adapter into its own optional dependency. Preserve
      library/resource resolution, descriptor validation, state formats, MIDI and
      main-thread UI behavior. Keep legacy callers working through thin adapters.
- [x] Implement the same DSP interface for Carla. Configure its fixed `C` once;
      populate source-derived frame/transport context without fabricating tempo
      or transport fields absent from the source. Represent latency as unknown.
- [x] Verify allocator-guarded core processing, the delay-copy-only dependency
      graph, existing Carla ABI/fake/native tests and a real pinned-runtime audio
      plus state-round-trip smoke test. Exercise existing UI lifecycle coverage.
- [x] Commit this milestone after its checks pass.

## Stage 2 — Standalone service, requester and block adaptation

Depends on Stage 1's DSP contract and both implementations.

- [x] Implement bounded codecs, negotiation and per-stream epochs. Negotiate
      `fs/P/k/N`, shape, DSP choice/configuration, reorder limits and capacities.
      Define startup/priming, gap, stop and reconfiguration semantics in protocol
      documentation with boundary examples.
- [x] Implement packet-to-`C` assembly and `C`-to-packet splitting by source frame,
      independent of socket receive chunking. Rebase MIDI offsets in both
      directions and cover partial/malformed messages and capacity exhaustion.
- [x] Add `shoop_dsp_host serve --processor delay-copy|carla` and a native requester
      mode. The requester generates known samples, maintains multiple requests in
      flight, checks returned content and consumes every scheduled output slot.
- [x] Pace the requester from nominal source readiness with monotonic elapsed
      time. Record sender lateness separately; bound catch-up and mark source
      overruns instead of silently moving deadlines or bursting indefinitely.
- [x] Add source-relative and worker-duration instrumentation, bounded counters
      and off-worker reporting. Distinguish input concealment, sender overruns,
      output deadline misses, stale traffic and queue/algorithm failures.
- [x] Verify real localhost UDP between separate processes with both DSP choices,
      `k=1` and aggregation, startup silence, channel ordering, state restoration,
      control progress and clean shutdown. Impulses must land at exact sample
      indices; processing must not acquire an extra queued packet period.
- [x] Commit this milestone after focused codec/adapter/process tests pass.

## Stage 3 — Failure semantics and actual localhost sweeps

Depends on Stage 2's real process pair and independent sample reference.

- [ ] Use deterministic clocks/test transports to inject packet delay, reordering,
      duplicates, gaps, late replies, malformed dimensions, stale epochs, stream
      restart, control loss and sustained overload. Assert exact samples/flags,
      once-only state progression, fallback boundaries and bounded storage.
- [ ] Cover allocation-free prepared codec/adapter/delay-copy paths and lifecycle
      ownership. Keep failure injection separate from the production delay-copy
      algorithm; a known sample delay must not be implemented as wall-clock delay.
- [ ] Add a repeatable sweep script launching a real server and requester as
      separate processes on the same host. Use loopback sockets, not an in-process
      carrier; select delay-copy without building/loading Carla.
- [ ] Run the full baseline matrix at 48 kHz stereo: `P = 8,16,32,64,128`,
      `k = 1,2,4,8`, `N = 1,2,4,8,16` (100 configurations), with `A=0`.
      Record rejected combinations explicitly. Run three repetitions of each
      admitted configuration with 0.5 s warm-up and 2 s measured source time.
- [ ] Also run `A=1` and `A=C+3` correctness cases for every `P/k` pair with
      `N=64`, plus 16-channel `P=16` acceptance and `P=32` refusal at a 1500-byte
      IP MTU. Record actual misses even with this larger allowance. Confirm sample
      delay does not become extra server holding time or alter output frame tags.
- [ ] Select up to six configurations around observed pass/miss boundaries for
      60 s follow-up runs. Record selection rationale, all runs and variance;
      retain missing/late responses in the denominator. Do not silently discard
      failing configurations or reclassify empirical misses as static refusals.
- [ ] Save a manifest with revision, commands, configuration, seeds, run durations,
      hardware/OS/toolchain/build profile, scheduling/affinity settings, socket
      buffers, counts and environment limits. Save per-run CSV/JSON metrics and
      histogram/raw timing evidence sufficient to regenerate the report.
- [ ] Report correctness failures, offered/completed packet rate, sender lateness,
      per-packet round trips, batch turnaround after final input submission,
      queue/concealment counters, deadline-miss fraction, empirical percentiles
      and maximum observed time. Label aggregation separately and disclose sample
      counts when interpreting tail percentiles. Never subtract unsynchronized
      client/server timestamps.
- [ ] Write `docs/remote-dsp/localhost-results.md` with reproducible commands,
      tables/plots over `P/k/N`, lowest observed miss-free allowance per workload,
      and limitations. These are shared-host loopback results, not hard real-time
      guarantees or physical-LAN measurements. Record actual outcomes even if no
      tested allowance is miss-free; no benchmark numbers are pre-populated.
- [ ] Keep a short correctness-focused CI subset. Run full timing sweeps as
      recorded experiments, without fragile CI wall-time percentile thresholds.
      Commit the test/script milestone and reviewed evidence.

## Final end-to-end validation and delivery

Depends on completed Stages 1–3; does not add the deferred graph/hardware stage.

- [ ] Rebuild from a clean checkout. Reproduce a delay-copy process-pair workflow
      with aggregation, exact sample verification, injected failure and restart;
      reproduce a real-Carla processing/state workflow without a Shoop bridge.
- [ ] Run the documented localhost sweep command and verify evidence/report
      generation from recorded data. Link every acceptance criterion to checks,
      tests or measurement artifacts; keep genuinely blocked checks incomplete.
- [ ] Run `cargo fmt --all`, `python3 scripts/check_shoop_test_usage.py` when Rust
      tests change, and `RUSTFLAGS="-D warnings" cargo build --workspace`, including
      explicit new-package Carla-enabled and Carla-disabled feature builds.
- [ ] Run focused new-package and existing Carla/engine regressions, followed by
      the complete Rust suite from `.agents/info/test.md` and tracing inventory.
      Use the selected build environment; do not allow missing-backend skips to
      satisfy the required real-Carla smoke test. Check shared `no_std` builds and
      existing native/WASM CI boundaries affected by extraction.
- [ ] Commit completed milestones, push `feat/network-dsp-host`, and open a PR
      targeting `feat/backend-replacement`. Describe the DSP abstraction, native
      Carla reuse, protocol/deadline contract and measured localhost outcomes.
- [ ] Monitor CI to green; inspect failures before targeted fixes. Check for
      automated review, address justified feedback, rerun affected checks and
      continue until approval when an automated reviewer participates.
- [ ] Confirm the final diff stays within Stages 1–3, attach reproducible evidence,
      and update this plan with completed steps, remaining limits and final status.

## Execution contract

- Keep the plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.

## Execution evidence

### Stage 1

- Exact base preserved; local checkout is `feat/network-dsp-host`.
- Debian workspace toolchain activated through `/workspace/.setup/activate.sh`;
  baseline warning-denying workspace build passed, and 12 baseline native Carla
  tests passed (runtime-optional probes were supplemented below).
- Built Carla 2.5.10 from the lock file's source archive; SHA-256 matches
  `ae2835b12081f7271a6b0b25d34b87d36b022c40370028ca4a10f90fcedfa661`.
- `SHOOP_REQUIRE_CARLA_TESTS=1 cargo test -p shoop_dsp -p shoop_carla_native`:
  16 tests passed, including real internal Audio Gain processing at gain 0.5,
  state save/restore, source-time snapshots and UI lifecycle tests.
- `cargo tree -p shoop_dsp --edges normal`: no dependencies.
- `cargo nextest run -p shoop_engine --features carla -E 'test(carla) | test(native_dummy_bus_fx)'`:
  21 existing bridge/session/worker regressions passed using the compatibility layer.
- `cargo fmt --all`, warning-denying workspace build and shared test policy passed.
- Extraction retains ABI/state/UI implementation and moves its tests with it;
  native tests use shared source fixtures and no legacy-engine dependency.

### Stage 2

- Added processor-independent UDP codecs, bounded source-ordered stream adaptation
  and a bounded return-slot scheduler. Deadline slots emit processed/concealed
  audio or silence without accepting retroactive late replacement.
- Added a dedicated processing worker and control/main-thread lifecycle with
  authenticated TCP, epoch fencing, quiescent state/UI operations and source-idle
  retirement. Pause/resume retains the worker's elapsed-time domain.
- Extracted the existing protected-file helper to `shoop_transport_socket` and
  re-exported it from `shoop_host`; both hosts now share Unix/Windows checks.
- `SHOOP_REQUIRE_CARLA_TESTS=1 cargo test -p shoop_dsp -p shoop_remote_dsp
  -p shoop_dsp_host --features shoop_dsp_host/carla -p shoop_transport_socket
  -p shoop_carla_native`: 26 tests passed. Follow-up targeted runs after lifecycle
  hardening passed all 9 protocol/process tests.
- Real separate-process gain-0.5 Carla workflows verified 19,200 samples per run,
  first from project XML and then from network-saved state; no correctness errors.
- `cargo build --lib -p shoop_dsp -p shoop_remote_dsp --target
  thumbv7em-none-eabihf`: passed without a standard library.
- `cargo tree -p shoop_dsp_host --no-default-features --edges normal` confirms no
  dependency on Carla or the legacy engine. Both host feature builds passed.
- Warning-denying workspace build, formatting, test policy and tracing inventory
  passed. Short smoke timing results are not substituted for the planned sweeps.
