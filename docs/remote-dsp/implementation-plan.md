# Network DSP host: implementation plan for stages 1–3

Status: Complete. Stages 1–3 and both measurement campaigns are complete; implementation CI and automated review checks passed. Published as [PR #915](https://github.com/SanderVocke/shoopdaloop/pull/915); validation evidence and remaining hardware/priority limits are recorded below.

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

- [x] Use deterministic clocks/test transports to inject packet delay, reordering,
      duplicates, gaps, late replies, malformed dimensions, stale epochs, stream
      restart, control loss and sustained overload. Assert exact samples/flags,
      once-only state progression, fallback boundaries and bounded storage.
- [x] Cover allocation-free prepared codec/adapter/delay-copy paths and lifecycle
      ownership. Keep failure injection separate from the production delay-copy
      algorithm; a known sample delay must not be implemented as wall-clock delay.
- [x] Add a repeatable sweep script launching a real server and requester as
      separate processes on the same host. Use loopback sockets, not an in-process
      carrier; select delay-copy without building/loading Carla.
- [x] Run the full baseline matrix at 48 kHz stereo: `P = 8,16,32,64,128`,
      `k = 1,2,4,8`, `N = 1,2,4,8,16` (100 configurations), with `A=0`.
      Record rejected combinations explicitly. Run three repetitions of each
      admitted configuration with 0.5 s warm-up and 2 s measured source time.
- [x] Also run `A=1` and `A=C+3` correctness cases for every `P/k` pair with
      `N=64`, plus 16-channel `P=16` acceptance and `P=32` refusal at a 1500-byte
      IP MTU. Record actual misses even with this larger allowance. Confirm sample
      delay does not become extra server holding time or alter output frame tags.
- [x] Select up to six configurations around observed pass/miss boundaries for
      60 s follow-up runs. Record selection rationale, all runs and variance;
      retain missing/late responses in the denominator. Do not silently discard
      failing configurations or reclassify empirical misses as static refusals.
- [x] Save a manifest with revision, commands, configuration, seeds, run durations,
      hardware/OS/toolchain/build profile, scheduling/affinity settings, socket
      buffers, counts and environment limits. Save per-run CSV/JSON metrics and
      histogram/raw timing evidence sufficient to regenerate the report.
- [x] Report correctness failures, offered/completed packet rate, sender lateness,
      per-packet round trips, batch turnaround after final input submission,
      queue/concealment counters, deadline-miss fraction, empirical percentiles
      and maximum observed time. Label aggregation separately and disclose sample
      counts when interpreting tail percentiles. Never subtract unsynchronized
      client/server timestamps.
- [x] Write `docs/remote-dsp/localhost-results.md` with reproducible commands,
      tables/plots over `P/k/N`, lowest observed miss-free allowance per workload,
      and limitations. These are shared-host loopback results, not hard real-time
      guarantees or physical-LAN measurements. Record actual outcomes even if no
      tested allowance is miss-free; no benchmark numbers are pre-populated.
- [x] Keep a short correctness-focused CI subset. Run full timing sweeps as
      recorded experiments, without fragile CI wall-time percentile thresholds.
      Commit the test/script milestone and reviewed evidence.

## Follow-up — Scheduling and polling comparisons

Requested by the user after reviewing the original campaign; preserve that
campaign as published and add independent, reproducible evidence.

- [x] Add explicit yielding/spinning polling modes and bounded service-gap counters.
      Verify both modes, native Carla compatibility and portable-core boundaries.
- [x] Run randomized, repeated comparisons of unpinned/pinned processes, independent
      requester/worker polling, and 200/1000-microsecond input-gap policies. Probe
      elevated priority; record permission failures rather than assuming success.
- [x] Measure CPU cost and cgroup throttling alongside exact samples, input
      concealment, sender overruns, output misses and stream retirement. Keep
      timings separate from correctness; run no builds/tests during measurements.
- [x] Repeat matched baseline and selected alternatives for longer runs. Retain all
      outcomes and document selection, environment limits and reproducible commands.
- [x] Publish an additional report/artifacts on PR #915 alongside the unchanged
      original evidence and explain the measured outcome to the user.

## Final end-to-end validation and delivery

Depends on completed Stages 1–3; does not add the deferred graph/hardware stage.

- [x] Rebuild from a clean checkout. Reproduce a delay-copy process-pair workflow
      with aggregation, exact sample verification, injected failure and restart;
      reproduce a real-Carla processing/state workflow without a Shoop bridge.
- [x] Run the documented localhost sweep command and verify evidence/report
      generation from recorded data. Link every acceptance criterion to checks,
      tests or measurement artifacts; keep genuinely blocked checks incomplete.
- [x] Run `cargo fmt --all`, `python3 scripts/check_shoop_test_usage.py` when Rust
      tests change, and `RUSTFLAGS="-D warnings" cargo build --workspace`, including
      explicit new-package Carla-enabled and Carla-disabled feature builds.
- [x] Run focused new-package and existing Carla/engine regressions, followed by
      the complete Rust suite from `.agents/info/test.md` and tracing inventory.
      Use the selected build environment; do not allow missing-backend skips to
      satisfy the required real-Carla smoke test. Check shared `no_std` builds and
      existing native/WASM CI boundaries affected by extraction.
- [x] Commit completed milestones, push `feat/network-dsp-host`, and open a PR
      targeting `feat/backend-replacement`. Describe the DSP abstraction, native
      Carla reuse, protocol/deadline contract and measured localhost outcomes.
- [x] Monitor CI to green; inspect failures before targeted fixes. Check for
      automated review, address justified feedback, rerun affected checks and
      continue until approval when an automated reviewer participates.
- [x] Confirm the final diff stays within Stages 1–3 and the requested scheduling follow-up, attach reproducible evidence,
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

### Stage 3 implementation and validation

- Added deterministic parser/fault tests covering 5000 malformed/truncated
  datagrams, delayed-input state, MIDI recovery overflow, non-replayed algorithm
  faults, long elapsed-time arithmetic, and nonzero absolute stream attachment.
- Design refinement: negotiation includes `start_frame`. Epoch-relative packet
  sequences preserve the original absolute source frame, including attachment to
  an already-running FPGA timeline. Readiness uses `s - start_frame`; all planned
  zero-origin sweep configurations retain their original deadline definition.
- Retired streams publish a compact first-fault reason. The worker processes a
  ready batch before receiving another datagram, and a shutdown watchdog exits
  the dedicated process before cleanup could overlap a stuck DSP invocation.
- Added `scripts/sweep_remote_dsp.py` with full/smoke/delay/follow-up profiles,
  refusal records, three repetitions, seeded ordering, binary/runner manifests,
  histograms, CSV, compressed JSONL and report regeneration.
- Smoke experiment: 3 measured configurations and 1 structural refusal, 15,808
  checked samples with zero incorrect samples. Timing misses were recorded.
- Full native suite: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci`:
  2531 passed, 4 skipped. The first debug-info-heavy attempt exhausted disk;
  generated development caches were reclaimed and validation reran with
  `CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`, `CARGO_INCREMENTAL=0`.
  Debug assertions/allocation guards remain enabled.
- `cargo check -p shoopdaloop --no-default-features --target wasm32-unknown-unknown`
  passed; the two portable DSP crates also passed the bare-metal target build.
- Latest real-Carla and targeted DSP/protocol/process tests passed after lifecycle
  and fault-reporting refinements. Native CI now runs the correctness subset on
  Linux/macOS/Windows and the bare-metal job builds both portable crates.
- The completed matrix and follow-ups are recorded below; external gates are
  tracked separately from local validation.

### Completed measurement campaign and local delivery

- The release executable was frozen from clean source commit
  `46b9fd8d5a593cd915dc310cf37f974da9b931c7`; the manifest records its SHA-256.
  No builds or test campaigns ran concurrently with measurement.
- Executed all 100 unique baseline configurations: 70 admitted configurations
  repeated three times and 30 structural refusals (240 baseline records).
  Added all 40 delay cases, both MTU boundaries and all six 60-second follow-ups.
- Final totals: 288 records, 257 measured runs, 31 refusals, 2,149,596 measured
  packet slots, 71,534,176 verified received samples, zero mismatches and zero
  unverified samples. Histogram counts and packet-outcome sums all match.
- Preserved every run, including two `Future` retirements and all late/missing
  results. Larger output allowances do not automatically enlarge the independent
  200-microsecond input-gap policy. The report distinguishes those outcomes.
- Regenerated the report and verified identical decompressed run data. Reviewed
  CSV/archive/manifest/audit artifacts are in `docs/remote-dsp/evidence/`, with a
  reproducible scientific heatmap. Generated credential files were excluded.
- Results and limitations are documented in `docs/remote-dsp/localhost-results.md`;
  `scripts/plot_remote_dsp.py` regenerates the figure from the archived results.
- A clean detached worktree of the executed source built the entire workspace
  and passed all 31 DSP/protocol/required-real-Carla/process tests, including
  restored-state network audio. The source worktree remained clean.
- Local formatting, warning-denying builds, shared Rust test policy, tracing
  inventory, bare-metal and WebAssembly checks passed. The complete native suite
  passed 2531 tests with four skips; new/fault paths were checked again separately.

### Acceptance evidence

| Criteria | Evidence |
| --- | --- |
| 1–2: abstraction, native hosting and compatibility | Dedicated crates, dependency-tree checks, real gain/state tests and 21 retained engine/bridge regressions |
| 3–4: packet/block/allowance and fixed source timing | All 100 baseline combinations, MTU refusals, bounded return-slot and absolute-origin tests |
| 5: exact algorithm delay | Independent delay reference tests and all 40 non-aligned delay experiment cases |
| 6–7: bounded failures, memory and lifecycle | Malformed/fault/reorder/late/epoch tests, allocation guards, process lifecycle tests and recorded retirement outcomes |
| 8: MIDI and time/latency metadata | MIDI offset/overflow/reset tests, source-time snapshot tests, explicit unknown Carla latency |
| 9: actual reproducible localhost campaign | 288 archived records, consistency audit, regenerated report and plot |
| 10: native smoke and final external gates | Real pinned-Carla, local repository gates, all native/browser/bare-metal CI jobs, CodeQL and Codecov passed; the sole automated review finding is resolved |

### Public delivery

- The user explicitly approved publication of commit `77950957`, source and
  benchmark evidence to the public repository after automatic approval review
  requested disclosure consent. The branch push then succeeded.
- [PR #915](https://github.com/SanderVocke/shoopdaloop/pull/915) targets the recorded
  `feat/backend-replacement` base. It describes the abstraction, protocol and
  reviewed measurements, and links commands and reproducible artifacts.
- Implementation CI and automated review checks passed on `2080272b`; the final
  verification links and outcomes appear below. The PR remains open for review.

### Scheduling comparison preparation

- Added independent `--polling yield|spin` modes and bounded native service-gap
  counters. The comparison runner records verified process affinities, CPU usage,
  cgroup deltas and actual priority-probe outcomes. Original evidence is unchanged.
- Required real-Carla and DSP/protocol/process checks passed all 33 tests in both
  debug and release profiles. The complete native suite passed 2533 tests with
  four skips. Formatting, warning-denying workspace build, test-policy, tracing
  inventory (367 modules), portable-core build and runner smoke checks passed.
- Fixed release-only test compilation by using the repository heap tracker for
  allocation guards. Reduced ring indices before narrowing to native pointer
  width, with input/return regression coverage around the 32-bit sequence boundary.
- Addressed CodeQL's nullable-pointer finding in the time-snapshot test by checking
  the callback result before borrowing it. Remote scanning will verify the fix.
- This environment permits affinity changes but denies FIFO/RR priority and
  negative niceness. The full campaign records those refused settings explicitly;
  completed measurements will be published separately from the original results.

### Completed scheduling/polling follow-up

- Frozen source `e21ca5ee566f97724dbd11af79504e5a4ae45c80` and executable hash
  `36d952b0f15d49368913ab7b12cdf693e25bd1978bb3146432312ffdae6a4b89`.
  The source tree was clean; no builds/tests ran during measurement.
- Completed 168 randomized short comparisons (eight configurations, seven modes,
  three repetitions) and 16 randomized 60-second baseline/alternative runs.
  Verified 124441600 samples over 4495600 measured slots: zero mismatches,
  unverified samples, missing responses or retirements; 6813 late responses.
- Audited every configuration/repetition, process exit, packet/sample count,
  histogram/quantile, actual affinity and resource delta. Report regeneration
  preserved the decompressed archive hash. Original artifacts/report are unchanged.
- Reviewed comparisons, CPU costs, source-deadline lower bounds and priority
  denials appear in `docs/remote-dsp/tuning-results.md`; reproducible artifacts
  and a regenerated figure are in `docs/remote-dsp/tuning-evidence/`.
- Affinity/spinning helped selected 8-frame cases but did not establish reliable
  audio. Wider gap allowances reduced some concealment counts without consistently
  improving deadlines. Both polling modes used approximately two CPU cores per
  localhost pair. Elevated priority was unavailable and is not claimed as tested.
- The base advanced to `3aabf43789708a5640a8f5b368f737651ef1d29d` during execution.
  Reconciled its dependency/Carla completion changes, retaining native `libc` for
  completion waits and the extracted Carla dependency/compatibility adapter.
  The updated workspace build and complete 2553-test suite passed (four skips),
  as did the required 33-test DSP/Carla checks, portable-core build, WebAssembly
  check and reconciled-runner smoke checks. Final external gates remain pending.
- The later priority-runner refinement uses cooperative yielding for any permitted
  FIFO/RR/niceness conditions, avoiding a spinning FIFO peer starving control.
  It does not change the seven conditions actually measured on this host.

### CI graph-flush correction

- CodeQL passed all four language analyses with zero new alerts; its earlier
  nullable-pointer review thread is resolved.
- Coverage run `41536354052` exposed an existing controlled-dummy race: 160
  requested frames advanced a loop only 32 frames. Its finalized Perfetto trace
  shows the two 64-frame cycles before graph installation; only the last short
  cycle reached the loop. The graph apply had deferred its rebuild and armed
  another generation, but the original flush returned after the deferred attempt.
- Added a deterministic regression: the first apply arms a follow-up inside a
  30-second coalescing window; a flush must complete both. It failed before the
  correction (one apply instead of two) and passed afterward. Flush now rechecks
  the pending generation and waits through deferred follow-ups.
- The original test passed 20 unconstrained and 100 constrained baseline runs;
  the regression avoids relying on that timing-dependent failure. After the fix,
  both tests passed 100 iterations constrained to one CPU, alongside the complete
  2554-test native suite (four skips), warning-denying build, formatting, test
  policy, tracing inventory and WebAssembly check.
- This correction affects the retained legacy scheduler, which is outside the
  standalone network DSP executable. Both frozen measurement archives remain
  unchanged. Final CI is being rerun on the corrected revision.

### Required Carla coverage and fixture ordering

- Run `41540314442` passed the coverage job and all 2554 tests, but Codecov's
  patch check reported 71.31% against the unchanged 80% requirement. Most missed
  lines were in the extracted native loader and generic Carla adapter: the CI
  environment lacked Carla, so its real-runtime checks returned early.
- Added a Linux test-runtime builder using the locked source archive and native
  header checksums, a fresh source tree and headless plugin build. Coverage now
  enables `shoop_dsp_host/carla` and `SHOOP_REQUIRE_CARLA_TESTS=1`, exercising the
  existing real gain/state and network restoration tests instead of skipping them.
  The helper's fresh build, direct source download/checksum, required 33-test
  subset and complete 2555-test suite passed locally (four skips in the full suite).
- Linux debug also exposed an existing fake-FX fixture race. Its trace shows an
  in-flight automatic cycle applying staged input before the output-clear command,
  consuming input intended for the controlled cycle. The fixture now waits for
  the driver to settle after changing mode and before staging test data. Samples
  and assertions are unchanged; no fixed sleep was added.
- The fixture passed 20 ordinary and 100 constrained baseline repetitions and
  100 constrained repetitions with the explicit barrier. The final required-Carla
  workspace suite passed all 2555 tests, along with the warning-denying workspace
  build, formatting, test policy and tracing inventory. Both experiment archives
  remain immutable. The subsequent required-runtime coverage run passed, as
  recorded below.

### Final verified delivery

- Implementation revision `2080272b820982df4b63754a46d79f4852e5afb7` passed
  [Build and Test run 41545456104](https://github.com/SanderVocke/shoopdaloop/actions/runs/41545456104)
  on 2026-11-03 UTC. All ten applicable jobs passed: Linux, Windows and macOS
  debug/release, both web profiles, bare-metal builds and Linux coverage. The
  scheduled-only nightly job was skipped as configured.
- Required native-Carla coverage executed all 2555 tests successfully, with four
  configured skips. Codecov passed patch coverage at **87.73%** against
  the unchanged **80%** floor and project coverage at **89.69%**.
- All four language analyses in
  [CodeQL run 41545456197](https://github.com/SanderVocke/shoopdaloop/actions/runs/41545456197)
  passed, with no new alerts. The sole earlier CodeQL review thread is resolved;
  there are no unresolved review threads. These are automated check outcomes,
  not a claim of a human approving review. The
  [documentation run](https://github.com/SanderVocke/shoopdaloop/actions/runs/41545456075)
  also passed.
- The original 288-record campaign and additional 184 scheduling comparisons are
  published together on PR #915, with raw evidence, audits, plots and reproduction
  commands. Both frozen measurement archives remain unchanged by later CI fixes.
- This final status update changes documentation only. FPGA/session integration,
  physical-LAN measurements and tests with permitted elevated priority remain the
  documented follow-up work; the completed experiments do not establish reliable
  low-latency audio or arbitrary-plugin timing guarantees.
