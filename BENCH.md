# Standardized RT benchmark proposal

Current upstream baseline: `feat/backend-replacement` at
`cc410f27` (2026-11-20). The original proposal used `3cd4c4ce` on 2026-11-17.

Build a **single deterministic fixture, driven through the RT transport endpoint,
with small adapters for backend preparation, processing, and measurement**.
The fixture and entry point live in
[`shoop_rt_benchmark`](src/rust/shoop_rt_benchmark/README.md).

**Current variants (revision 5).** Supported presets are `tiny`, `quick` and
`full` (default). `full` is the former smoke workload: two lifecycle iterations,
one repetition, 298 lifecycle and 232 mixed RTL callbacks per media profile.
The former standard preset is removed, with no compatibility CLI aliases.
Historical reports retain their original labels and remain regenerable.

Tiny runs one 52-callback RTL stress sequence per media: cold audio/MIDI growth,
the full 2,560-message playback merge, quiet playback, simultaneous retake
allocation and stopping. It explicitly omits playback wraps, planned transitions
and mixed loads. Quick combines those omitted checks, full-length retakes and
half-record/half-play contention into one 172-callback sequence. Targets are
roughly five and twenty minutes with three RTL workers and warm caches; cold
compilation adds setup time. Native CPU retains separate full-pattern statistics:
up to two passes with a 30-second soft budget for tiny, three with 45 seconds for
quick. Budgets are checked between complete passes; optional skips are recorded.
The reference validates the same fixtures once. Required RTL coverage is checked
by the common runner and reporter, rather than stopping when a timer expires.

The [RT backend benchmarks workflow](.github/workflows/fpga_simulation.yml) runs
only by manual dispatch, with a `tiny`/`quick`/`full` selector. Its summary links
to a downloadable PDF, with a separate raw-data/log artifact retained for 30 days.
Automatic FPGA component checks moved to
[fpga_components.yml](.github/workflows/fpga_components.yml); they no longer launch
the benchmark campaign. Existing published captures below are historical evidence.

Revision 5 validation passed nine scoped Rust tests, including both compact
sequences on all media through both Rust backends, and 15 reporter/scheduling
tests. A complete local tiny campaign took 235.5 seconds (3:56), retaining 13,372
callbacks: 20 completed slots, 22 unsupported, no failures. The first 42 RTL
callbacks in all three media profiles exactly match the previous capture's
cycle/traffic/stage records. MIDI and combined peaks remain 1,989,492 and 2,031,651
cycles; the tiny audio retake reaches 113,777 cycles, slightly above the prior
113,424-cycle peak. Its PDF remains five pages and labels omitted RTL scenarios.
All nine full fixtures match the former smoke fixtures apart from preset name
and version. A separate quick audio RTL replay passed all 172 callbacks in
138.4 seconds, including 128 planned-slot retirements, 160 wraps and mixed
record/play coverage. The complete quick matrix's twenty-minute target remains
an estimate pending its Actions run. Formatting, warning-denying build, test-policy checks and Actions
workflow lint also passed.

**Very short RTL DDR comparisons.** The same manual workflow also offers
`rtl-audio-ddr` and `rtl-midi-ddr`. Each runs exactly two RTL cases, pairing
zero-added-delay ideal AXI service with DRAMsim3 through one 38-callback common
endpoint fixture. Cold recording crosses allocation boundaries; one full-load
playback, all-loop retake and final stop follow. Native statistics, quiet playback,
wraps, planned transitions and mixed loads are omitted from these probes.
The single-page PDF pairs callback cycles/µs, differences and ratios; raw child
runs and all paired callbacks remain available. The existing seven-cycle fixed
memory mode remains separate. Both cases must validate the same fixture, frozen
binary and functional work. See `scripts/benchmark_rt_ddr.py` for the entry point.
Complete local pairs passed in 37.6 seconds for audio and 82.9 seconds for MIDI
with warm caches. Each retains 38 paired callbacks and produces one PDF page.
Eleven scoped Rust tests, 18 reporter/comparison tests and Actions lint passed.

**Upstream refresh and MIDI matrix (revision 4).** The benchmark now selects
`audio`, `midi`, or `audio-midi`; the default campaign runs all three. Each uses
64 loops and Q=callback=32. Audio loops remain genuinely stereo, with 32-frame
chunks and 32-chunk blocks. MIDI includes bursts, tied sample timestamps, silent
intervals, synchronized allocation, event-block growth and retakes. The shared
runner validates every output packet as well as every audio sample.

Upstream audit:

- Full `GraphView` compilation now supports multichannel loops, strips, buses and
  routing. Remove the benchmark's old `GraphDescription`/`compile_with_geometry`
  extensions and use the upstream compiler unchanged.
- Native and FPGA hosts now share driver infrastructure. Remove the production
  `SteppedDummy` addition and its cycle-access workaround; step the production
  native renderer with benchmark-owned prepared buffers.
- RTL encoding/model support moved into `rt_fpga_rtl`; update imports and reuse
  upstream `apply_plan`, `clear_plan`, `loop_plan` and gain encoding. Preserve upstream's
  compilation lock in the optimized, trace-free benchmark model session.
- MIDI is now event-only data work through the common SP/JP queue. Use current
  MIDI operations, storage and connected RTL, with no retired state-tracking or
  MIDI-interpreter fixtures.
- The native CPU immediate endpoint still refuses MIDI loop transitions.
  Publish those cases as unsupported; its authored transport is a different
  control path. The Rust FPGA reference endpoint needs a small MIDI-processing
  extension that preserves its command FIFO and snapshots.
- Audio and MIDI AXI responders must share one 150 MHz timing controller, with
  one advance per fabric cycle and disjoint addresses. Independent DRAM models
  would hide contention. Validate the resulting DRAM/fabric tick ratio too.
- Adopt the new provisional Z-turn V2 DDR3-1066 x32 replay profile as the default.
  Keep `--dram-config`/`SHOOP_DRAMSIM3_CONFIG` overrides and record its hash; the
  historical report used the x16 model.
- Native Carla and physical I2S/Ethernet changes require no benchmark device or
  plugin. Keep this fixture device-free. The detached-host stderr handling is
  now upstream; remove the branch's duplicate writer implementation.

Indexed RAM observation replaces thousands of per-word probe ports. Reads are
clock-free and checked against static probes without changing RTL state or time.

The upstream [64-loop MIDI playback measurements](docs/fpga/midi-data-jobs-benchmarks.md)
exercise sparse playback (normally four or five messages per quantum), at an
assumed 100 MHz with independent ideal/fixed-delay memory ports. This fixture
adds synchronized recording, dense merges, allocation growth and retakes, at
150 MHz with shared DRAM timing. Retain the event counts and memory configuration
when interpreting the different timing results.

The compact PDF remains capped at five pages, with its first three devoted to
FPGA. Preserve separate media/pattern statistics, quiet MIDI playback, worst
callback identities and audio/MIDI traffic. Host timing repetitions run serially;
optional parallel RTL simulations start afterward. Old results remain available
as historical configurations rather than being relabeled as new measurements.

**Revision 4 validation.** The complete three-media smoke matrix captured at
`73cf1d75` accounts for all 27 requested slots: 15 completed, 12 explicitly
unsupported, none failed. It retains 8,863 callbacks, verifies every stereo
sample in 11 audio-bearing runs and exactly checks 851,968 MIDI messages across
eight MIDI-bearing runs. Fixture hashes match across available backends; every
RTL run passed shared DRAM/fabric clock validation and finished with memory idle
and zero final drain ticks. The [five-page PDF and raw bundle](benchmarks/rt-audio-midi-smoke/README.md)
are published together. Regenerating from the extracted archive preserves
`summary.json` and `callbacks.csv` byte for byte.

The observed RTL maxima are 113,424 cycles / 756.16 µs for audio, 1,989,492 cycles /
13,263.28 µs for MIDI, and 2,031,651 cycles / 13,544.34 µs for combined processing.
The MIDI peaks occur when all 64 loops begin playback and emit 2,560 messages;
their 63 balanced merges produce substantial DDR list traffic. Quiet MIDI-only
playback takes 79.43 µs, while quiet MIDI playback with stereo audio takes about
371.5 µs. These populations remain separate in the PDF and summary.

Validation also passed 112 scoped Rust tests across compiler/reference, RTL
runtime and benchmark packages, including processing heap guards, shared-clock
and clock-free observation checks; 12 reporter tests; formatting/test-policy
checks; and warning-denying release builds. The full workspace suite was not
rerun. Capture took about 54 minutes with three RTL workers and a prebuilt
binary/model cache; report-only regeneration took 2.4 seconds. This published
campaign uses the shorter smoke preset, with two lifecycle iterations and one
repetition. The then-default standard campaign retained eight iterations and three
independent CPU/reference repetitions; that preset has since been removed.

After capture, the final duplicate packed-plan decoder was replaced with
upstream's passive `loop_plan` helper. A fresh 298-callback RTL audio lifecycle
replay preserved every fabric/traffic/stage measurement, fixture hash, command
event and coverage result. Observation advances no clocks; the published matrix
retains its original measured source and binary fingerprint.

**Profiled smoke replay (2026-11-21).** A complete replay at `ce354fb1` took
39:51 with three RTL workers and warm build/model caches, including 39:37 in
RTL tasks, 12.45 seconds in CPU/reference cases and 1.34 seconds generating the
report. All 1,590 RTL callback cycle/traffic/stage records match the preceding
capture; all 8,863 callbacks and 851,968 MIDI messages passed validation.
The [runtime breakdown, sampled CPU profiles and five-page PDF](benchmarks/rt-smoke-runtime-20261121/README.md)
retain individual case timings, preparation/processing/validation phases and
the overlapping worker timeline. Signal access and its lookup/hash/comparison
callees account for 64.2% of sampled RTL-process CPU; generated RTL accounts for
15.1%, DRAM timing 3.8%, and other host work 16.9%. Sampling adds host overhead;
these host seconds remain separate from FPGA cycles at the assumed 150 MHz.
No host-speed optimization was applied. Seven scoped benchmark Rust tests and
12 reporter tests passed for the instrumentation change, along with formatting,
test-policy checks and a warning-denying release build.

**Original integration gaps (2026-11-17; superseded by the audit above).**

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

The Rust runner owns an `E: Endpoint`, its bounded outbox, and a synchronous
dummy processing driver. Backend-specific adapters do only three things:
prepare the canonical layout, execute one callback, and return measurements.
Initial preparation can use native graph transactions or FPGA compilation as
appropriate; all measured control stimulus must travel through the endpoint.

Each callback follows a fixed sequence:

1. Deliver the fixture's commands for this boundary and pump endpoint staging.
2. Release exactly one 32-frame callback.
3. Collect command responses, snapshots, and timing records.
4. Verify the expected execution frame and state before continuing.

Dummy callbacks process requested frames synchronously without pacing or
deadlines. Each adapter uses its production renderer or processor with prepared
buffers under this scheduling contract. No paced host thread is used.

Command groups contain at most **32 commands**. Starting all 64 loops therefore
requires two groups queued before processing begins. Assert that both response
headers identify the same execution frame. Sending a command, processing, and
waiting for its response repeatedly would miss the intended allocation storm.

**Canonical fixture.** Start with this deliberately simple layout:

| Property | Standard |
| --- | --- |
| Loops | 64 primitive loops: audio-only, MIDI-only, and stereo audio plus MIDI |
| Routing | One stereo strip containing all 64 loops, mixed to a stereo master |
| Timing | Q=32; callback size=32; sample rate=48 kHz |
| FPGA media | 32-frame chunks; 32-chunk blocks; 1,024 frames per block |
| Input | Deterministic, bounded, nonzero stereo signal generated in memory |
| Output | In-memory sink; occasional checks outside timing regions |
| Processing | Dummy driver, maximum speed, no deadline enforcement |
| MIDI | Deterministic 40/16-event bursts and eight silent quanta; exact bytes/order/offset checks; 256-record storage blocks |
| Features | No plugins or actual devices; no MIDI instrument-state interpretation |

One strip keeps each comparison focused on loop processing and media
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

The PDF has at most five landscape pages, prioritizing FPGA results:

1. FPGA RTL overview: completion/support status, callback counts, median, p95,
   p99, maximum in microseconds and cycles, budget exceedances, and a cumulative
   processing-time distribution.
2. FPGA stress-event and steady-population statistics, selecting the populations
   with the highest observed maxima. Cold starts and reuse remain separate.
3. FPGA worst-callback identities and first-versus-worst diagnostics: stage
   activity, memory transactions, queue peaks and memory-stall cycles. Include
   the following callback's cost and final drain status. Overlapping activity
   must not be added together.
4. CPU/reference statistics as per-repeat ranges, with endpoint-service cost
   separate from processing.
5. Correctness coverage, unsupported/failed operations and essential provenance.

Omit pages for unavailable completed backend results, while retaining status and
validation information. The page count must stay bounded as repetition counts
grow. Callback timelines and event-aligned traces stay out of the PDF; their full
source data remains in JSON and CSV.

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
  --preset quick \
  --output work/rt-benchmark
```

This command checks prerequisites, builds optimized binaries, runs backends
sequentially, validates results, and generates `report.pdf`, `summary.json`, raw
callback/event records, and a provenance manifest. Include a report-only mode and
the three preset schedules while preserving the 64-stereo layout.

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

**Initial audio-only implementation stages (2026-11-17).**

- [x] **Fixture and result contract:** define the versioned layout, patterns,
  event coverage, measurement boundaries, and artifact schema. Verify
  deterministic replay and the expected 64-command batching.
- [x] **Native CPU runner:** implement endpoint-driven stimulus with controlled
  dummy processing. Verify same-frame execution for all 64 loops, real stereo
  processing, and complete timing capture.
- [x] **FPGA reference support:** extend preparation and bindings to 64 stereo
  loops, audit capacities, and add unpaced dummy execution. Verify matching
  event/state behavior; leave unsupported grabs explicit.
- [x] **RTL adapter:** reuse simulation infrastructure through a production-facing
  command/snapshot bridge, with 150 MHz memory timing. Verify command execution
  frames, sample/state agreement, and inclusive cycle accounting.
- [x] **Report and orchestration:** implement the single entry point and PDF
  generation. Check known timing samples, conversion arithmetic, missing-backend
  reporting, and representative PDF pages.
- [x] **End-to-end acceptance:** run the full fixture through every available
  target, regenerate its PDF from raw data, and verify that all required event
  coverage and correctness checks passed.

**Historical audio-only verification (2026-11-17).** The complete 32×32 standard campaign at `2f3d34f4` accounted for all
21 runs: 17 completed and four explicitly unsupported FPGA grab runs. It retained
46,015 callback measurements. Every completed run passed exact stereo samples,
command-frame checks, allocation growth, storage release and wrapping. Mandatory
patterns verified simultaneous growth at the first 1,024-frame allocation
boundary and exact finalized take lengths. Fixture hashes matched across
backends and independent repetitions.

The RTL lifecycle run covered 1,096 callbacks, 448 take replacements and 1,024
planned-slot retirements. Its observed maximum was 133,777 cycles (891.85 µs),
with seven callbacks exceeding the hypothetical 100,000-cycle budget. Mixed
turnover covered 636 callbacks and reached 90,251 cycles (601.67 µs), with no
budget exceedances. Their median complete-callback times were 296.40 µs and
299.75 µs respectively. Both finished with memory idle and zero final DRAM drain
ticks. Shared prefixes of 266 lifecycle callbacks and 199 mixed callbacks matched
the smoke campaign's cycle counts exactly.

The five-page PDF was regenerated from saved data with byte-identical statistics
and CSV exports, and all five pages were inspected. Its first three pages focus
on FPGA statistics and peak diagnostics. A published raw
bundle retains all run JSON, manifest, CSV and summaries for independent report
regeneration. Local validation also passed 67 tests across the compiler,
benchmark and functional-reference packages, eight reporter tests, processing
heap guards, formatting/test-policy checks and warnings-as-errors release builds.
The nine-run smoke campaign retained the same layout and geometry and passed
both mandatory patterns on all three backends, plus exact native history grabs.

The native driver's advertised one-quantum output latency is included in sample
validation; signal time is relative to fixture start on every backend. Per-callback
diagnostics distinguish observed plan retirement and storage events from ordinary
steady processing, and retain overlapping SP/JP and memory-stall activity. The
memory-model seed is fixed at `20261117`.

The implementation and CI evidence are tracked in
[PR #1004](https://github.com/SanderVocke/shoopdaloop/pull/1004), targeting
`feat/backend-replacement`. The manual benchmark workflow runs and uploads the
selected campaign. Ordinary generated campaigns remain in the requested output folder.
The validated 32×32 [PDF and compressed raw bundle](benchmarks/rt-standard-32x32/README.md)
are published with this PR so the report can be downloaded and regenerated.

Full-workspace CI exposed an existing detached-host shutdown failure: logging to
the exited launcher's stderr pipe raised `BrokenPipe`, causing tracing to panic
before shutdown results could be collected. The host now tolerates that closed
diagnostic pipe. The recovery test reproduced the failure locally before the fix;
afterwards it passed ten repeat runs, the serial recovery suite passed (15 tests,
one hardware-only test ignored), and log rotation and the warnings-as-errors host
build passed. The benchmark's stepped processing path is unaffected.

Fixture revision 3 supersedes revision 2's one-frame chunks, 128-chunk blocks and
256-frame recordings. Both FPGA adapters now use 32-frame chunks and 32-chunk
blocks, with 1,056-frame mandatory recordings; mixed turnover also uses 1,088 and
1,152 frames. The complete-callback timing boundary, 64-stereo/Q32 layout and
150 MHz memory timing are unchanged. Older reports remain regenerable with their
original geometry labels, while fresh campaigns now require revision 5. Grab storms
retain adjacent 32,768-frame history windows after filling the ring. Deterministic
RTL replay defaults to one run; CPU repetition counts follow the selected preset.
Simulation uses an optimized model without waveform compilation.

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
`feat/backend-replacement`, push it, and open a PR. The user has taken over CI and
review follow-up; validate locally and push updates without polling CI. Leave
merging to the user.
