# Standardized RT benchmark proposal

Baseline: `feat/backend-replacement` at commit
`3cd4c4cedf2e8258b67d4cdc311e5a9812bc0579`.

Build a **single deterministic fixture, driven through the RT transport endpoint,
with small adapters for backend preparation, processing, and measurement**. The
fixture describes musical operations at exact processing frames and retains every
callback's timing so transition spikes remain visible.

Implementation is in progress on `feat/standard-rt-benchmark`. See
[`shoop_rt_benchmark`](src/rust/shoop_rt_benchmark/README.md) for the entry point,
fixture contract, supported operations, and measurement boundaries.

**Current integration points and prerequisites.**

- Both `shoop_rt_engine::ControlEndpoint` and the FPGA reference endpoint implement
  [`shoop_rt_transport::carrier::Endpoint`](src/rust/shoop_rt_transport/src/carrier.rs).
  Immediate command groups and snapshots are their useful common control
  interface.
- The [FPGA graph frontend](src/rust/rt_fpga_compiler/src/graph.rs) currently supports
  up to **16 mono loops**. Supporting 64 genuinely stereo loops requires compiler,
  binding, and capacity work.
- The FPGA reference currently supports transitions, clearing planned transitions,
  and loop gain through that interface; grabs remain unsupported.
- The [RTL infrastructure](src/rust/rt_fpga_testbench/README.md) provides reusable
  simulation components, but lacks a complete runtime-command/snapshot frontend.

Expose those gaps explicitly. A 16-mono run must never be presented as the
requested 64-stereo result.

**Connection and execution model.** Add a dedicated `shoop_rt_benchmark` crate and
`scripts/benchmark_rt.py`. Keep this independent of the application, UI, and
existing backend test scenarios.

The Rust runner owns a protocol client over `InProcess<E>`, where `E: Endpoint`,
plus a dummy processing driver. Backend-specific adapters do only three things:
prepare the canonical layout, execute one callback, and return measurements.
Initial preparation can use native graph transactions or FPGA compilation as
appropriate; all measured control stimulus must travel through the endpoint.

Each callback follows a fixed sequence:

1. Deliver the fixture's commands for this boundary and pump endpoint staging.
2. Release exactly one 32-frame callback.
3. Collect command responses, snapshots, and timing records.
4. Verify the expected execution frame and state before continuing.

The existing native `DummyMode::Controlled` provides a useful starting point:
requested frames process without pacing or deadlines. Extend or factor its cycle
machinery so the same dummy scheduling contract can drive the FPGA reference and
RTL adapters. The reference host's current paced thread should not be used for
this benchmark.

Command groups contain at most **32 commands**. Starting all 64 loops therefore
requires two groups queued before processing begins. Assert that both response
headers identify the same execution frame. Sending a command, processing, and
waiting for its response repeatedly would miss the intended allocation storm.

**Canonical fixture.** Start with this deliberately simple layout:

| Property | Standard |
| --- | --- |
| Loops | 64 primitive stereo audio loops |
| Routing | One stereo strip containing all 64 loops, mixed to a stereo master |
| Timing | Q=32; callback size=32; sample rate=48 kHz |
| Input | Deterministic, bounded, nonzero stereo signal generated in memory |
| Output | In-memory sink; occasional checks outside timing regions |
| Processing | Dummy driver, maximum speed, no deadline enforcement |
| Features | No MIDI, plugins, or actual devices |

One strip keeps the initial comparison focused on loop processing and media
access. Its input history also provides a common source for simultaneous grabs.
Additional strip layouts can become separately versioned fixtures later.

Use distinct left/right input signals so a backend that duplicates mono into
stereo cannot accidentally pass. Scale mixing gains to avoid clipping. Silence
can be an additional diagnostic, but should not be the main input because silence
optimizations may avoid important memory work.

Freeze the layout, signal generator, event schedule, history retention, smoothing
settings, and logical storage budget in a versioned fixture manifest. Record
backend-specific pool sizes and physical memory geometry alongside it.

**Stress patterns.** Use two mandatory patterns and one capability-dependent
pattern, all operating on that same fixture:

| Pattern | Stimulus | Intended pressure |
| --- | --- | --- |
| Simultaneous lifecycle | Start all 64 recordings together; record across multiple allocation boundaries; switch all to playback together; run through repeated wraps; stop and re-record together | Initial allocation, growing recordings, capture finalization, playback reads, synchronized wrapping, release and reuse |
| Mixed turnover | Keep half playing while the other half repeatedly records and switches to playback; exchange roles; use several fixed quantum-aligned lengths and offsets | Concurrent reads/writes, allocation during playback, reclamation, less uniform memory access |
| Grab storm | Fill input history; grab into all 64 loops at one boundary; replace those takes repeatedly while playback continues; include windows crossing history-ring wrap | View/metadata allocation, shared ownership, retained history, replacement and reclamation |

Within the lifecycle pattern, add scheduled transitions that become due on the
same source pulse, and a separate all-loop gain-change event. These exercise
boundary resolution and control application without making every event an
indistinguishable combined stress case.

Record **event coverage**, not just the schedule: how many loops actually started
recording at the target boundary, whether allocation growth occurred, how many
wraps happened, and whether reclamation and reuse occurred. Choose recording
lengths that cross current storage boundaries and fail the coverage check if
future implementation changes make the pattern stop exercising them.

Repeat lifecycle operations within one prepared instance to expose accumulated
storage pressure. Also repeat from fresh prepared media state to distinguish
initial allocation from reuse. Warm-up must not consume and then discard the cold
recording event.

For unsupported grabs, record `unsupported` explicitly and continue the other
patterns. Do not substitute a cheaper operation. Full grab comparison becomes
available as those backends implement it.

These are repeatable searches for expensive cases. Report their maxima as
**observed maxima**, without claiming a proven worst-case execution bound.

**Measurements and interpretation.** Keep these measurements separate:

- **CPU callback time:** elapsed time for one dummy callback, including RT command
  application and processing. Measure endpoint delivery/polling separately, since
  that work can occur outside rendering.
- **FPGA reference time:** host CPU time running the Rust functional model. This
  tracks that implementation, but is not an FPGA performance estimate.
- **RTL processing cycles:** fabric cycles for the complete processing quantum,
  including relevant control transactions, allocation, memory stalls, and
  required completion work.

For RTL, define the completion boundary precisely from its processing contract.
Attribute deferred work to subsequent callbacks, and report any final drain
separately so costs cannot disappear at the measurement-window edge. Identify any
ARM-side work whose execution time is not modeled.

Use one global constant:

```text
FPGA_CLOCK_HZ = 150_000_000
microseconds = fabric_cycles / 150
```

The realistic memory model must also use **150 MHz as its fabric-clock
configuration**. Converting results from a simulation configured for 100 MHz would
leave the DRAM-to-fabric timing ratio inconsistent.

Use DRAMsim3 as the principal FPGA memory profile, with its configuration and seed
recorded. Fixed-latency memory can remain a diagnostic profile. Report fabric
cycles separately from DRAM ticks.

At 48 kHz, the hypothetical 32-frame callback budget is **666.67 microseconds**, or
**100,000 cycles at 150 MHz**. Show utilization and exceedance counts against that
reference while continuing to process without deadlines.

**PDF and raw results.** Generate the PDF from saved machine-readable data,
allowing it to be regenerated without rerunning the benchmarks. Python with
Matplotlib's PDF backend should be sufficient.

The report contains:

- A first-page backend/pattern comparison: count, median, p95, p99, maximum, budget
  utilization, and completion/support status.
- Callback timelines annotated with record starts, playback switches, grabs, and
  other events.
- Event-aligned views showing the triggering callback and following callbacks.
  This prevents rare spikes being diluted by steady playback.
- Separate summaries for steady processing, event callbacks, fresh allocation,
  and reuse.
- FPGA diagnostics where available: stage activity, memory traffic, allocation
  counts, queue peaks, and memory-stall cycles. Overlapping stage times must not be
  added together.
- Run provenance and correctness results.

Retain individual samples and identify the worst callback by run, pattern, frame,
and triggering event. Show sample counts beside percentiles; avoid presenting
extreme percentiles from tiny event populations. Keep repeat-level results visible
rather than pooling away host variability.

Preallocate timing storage and write artifacts outside measured callbacks. The
existing native recorder is useful precedent, but dropped measurements must
invalidate a supposedly complete benchmark result. An additional instrumented
pass can collect expensive diagnostics without contaminating the primary timing
pass.

**Entry point.** For example:

```bash
python3 scripts/benchmark_rt.py \
  --backends cpu,fpga-reference,fpga-rtl \
  --preset standard \
  --output work/rt-benchmark
```

This command checks prerequisites, builds optimized binaries, runs backends
sequentially, validates results, and generates `report.pdf`, `summary.json`, raw
callback/event records, and a provenance manifest. Include a report-only mode and
a smoke preset that reduces repetitions while preserving the 64-stereo layout.

Missing prerequisites, unsupported operations, incorrect results, and processing
failures need distinct statuses. Finish reports for completed runs, but return a
nonzero exit status when an explicitly requested mandatory comparison is
incomplete.

**Goals and acceptance criteria.**

- One versioned fixture and common endpoint-driven measured stimulus exercise the
  available backends, without UI, actual devices, or per-backend test scenarios.
- The standard layout remains 64 genuinely stereo audio loops, Q=32, and one
  quantum per callback; reduced backend capabilities are reported explicitly.
- All 64 recording transitions are verified to execute at the same processing
  frame, and required stress-event coverage and correctness checks pass.
- Dummy processing runs as fast as possible without deadline enforcement.
- FPGA results retain fabric cycles and present microseconds using a global
  150 MHz assumption; the memory model uses the matching fabric clock.
- A single command runs the selected benchmark set and produces complete raw
  measurements, provenance, and a PDF that can be regenerated from saved data.

**Implementation stages, in dependency order.**

- [x] **Fixture and result contract:** define the versioned layout, patterns,
  event coverage, measurement boundaries, and artifact schema. Verify
  deterministic replay and the expected 64-command batching.
- [x] **Native CPU runner:** implement endpoint-driven stimulus with controlled
  dummy processing. Verify same-frame execution for all 64 loops, real stereo
  processing, and complete timing capture.
- [x] **FPGA reference support:** extend preparation and bindings to 64 stereo
  loops, audit capacities, and add unpaced dummy execution. Verify matching
  event/state behavior; leave unsupported grabs explicit.
- [ ] **RTL adapter:** reuse simulation infrastructure through a production-facing
  command/snapshot bridge, with 150 MHz memory timing. Verify command execution
  frames, sample/state agreement, and inclusive cycle accounting.
- [x] **Report and orchestration:** implement the single entry point and PDF
  generation. Check known timing samples, conversion arithmetic, missing-backend
  reporting, and representative PDF pages.
- [ ] **End-to-end acceptance:** run the full fixture through every available
  target, regenerate its PDF from raw data, and verify that all required event
  coverage and correctness checks passed.

Verification so far: both mandatory smoke patterns pass on both Rust backends,
with exact stereo sample checks and a heap guard around each processing call.
The CPU grab campaign also completes with exact stereo checks, and FPGA grabs are explicitly unsupported.
The native driver's advertised one-quantum output latency is included in sample
validation; signal time is relative to fixture start on every backend. Reporter
tests verify cycle conversion, incomplete/reduced-result rejection, and PDF/CSV
regeneration from saved measurements. Initial RTL campaigns pass both mandatory
patterns; the final instrumented campaign and CI remain in progress. Per-callback
diagnostics distinguish observed plan retirement and storage events from ordinary
steady processing, and retain overlapping SP/JP and memory-stall activity.

Fixture revision 2 uses 256-frame recordings in the mandatory patterns, still
crossing the current 128-frame allocation blocks, to increase transition-event
density and reduce the substantial RTL simulation cost observed during validation.
The 64-stereo/Q32 acceptance criteria are unchanged. Grab storms use adjacent
32,768-frame history windows after filling the input ring, and RTL replay defaults
to one deterministic run while CPU timing retains three independent repetitions.
Simulation uses an optimized model without waveform compilation; physical cycle
counts and the 150 MHz memory timing remain unchanged.

Use targeted tests and smoke runs during implementation; reserve expensive full
RTL campaigns for completed integration milestones. Final acceptance requires the
unchanged fixture contract, endpoint-driven measured operations, verified
simultaneous starts, complete raw measurements, and the PDF.

**Execution contract.**

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user
  approval.

For implementation, create a benchmark feature branch based on
`feat/backend-replacement`, push it, and open a PR. Work through CI failures until
CI is green. Check for automated review and repeatedly address its feedback as
appropriate until the reviewer approves. Leave merging to the user.
