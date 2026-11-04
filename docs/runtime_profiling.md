# Runtime profiling

Hover over or click the backend status LED at the bottom of the application and
choose **Profiling…**, below **Host Manager…**. The dedicated window has an
immediate on/off switch and an overall DSP load bar showing the same reading as
the bottom status bar. Profiling defaults to off. A switch sends its runtime
request immediately and saves the preference in the background; no Save action
or audio restart is needed. The window reports the engine's acknowledgement. Profiling is independent
of developer tracing. There are no recording levels or xrun arming controls.

![Profiling window using quantum-budget units](runtime_profiling/window-budget.png)

The screenshot uses the native dummy driver in a debug build; its live readings
illustrate the interface. [Microsecond presentation](runtime_profiling/window-us.png).
Release performance measurements are linked below.

The table contains **Node / Avg / Worst / Stddev** in graph execution order.
Choose **µs** or **% budget**. Percentages use one fixed quantum's time budget:
`100 × duration_ns / (quantum_frames × 1e9 / sample_rate)`. They represent a share
of the available quantum period, rather than a share of the currently consumed
DSP time. For example, at 48 kHz with Q=32, a 10 µs step uses 1.50% of the
666.667 µs budget. Formatting changes preserve the measured data. Names identify prepared strip, loop and bus slots and their
roles, so tracks with identical display names remain distinguishable. A node
handling both audio and MIDI has one combined row.

Each sample measures one complete fixed-tick node step: local input preparation,
incoming routes, MIDI handling, processing, output recovery and executor
bookkeeping. Avg is the arithmetic mean, Worst is the maximum, and Stddev is the
population standard deviation. An unfinished step contributes no duration; its
row shows an em dash until a complete sample exists. Elapsed time includes OS
preemption and profiling work. It does not measure exclusive CPU time or the
latency until sound reaches the device.

Live statistics accumulate from enable or the latest reset, over accepted complete
records, through the displayed interval boundary. Every application graph build
resets tracking, including an identical rebuild or a build that fails. Activation,
engine replacement and re-enable also reset. Queued records and retained captures
from older generations are discarded; node IDs are never reconciled across
layouts. Disabling retains the final report until a reset or disconnection.

While enabled, callback budget overruns and host xrun notifications automatically
freeze recent raw history plus following work. **Latest xrun** shows statistics for
that actual interval using the same table. History is finite: the view reports its
actual bounds and incomplete/lost data. Host notifications may arrive late and do
not identify the exact offending node. Captures already leased to a collector are
protected; additional incidents that cannot be retained are counted. A large
callback or small quantum can exhaust post-trigger space, ending the capture
early and marking it incomplete. Changing the graph invalidates the capture.

## Clocks and browser support

Native hosts supply nanoseconds from a primed `Instant` epoch. A browser worklet
imports `shoop_profiling.now`, using its realm's `performance.now()` when present.
The observed positive clock step is shown; zero means resolution is unknown.
Complete native/worklet driver callbacks supply the deadline budget; split engine
parts are never treated as independent device deadlines. Offline hosts without
outer callback timing use their complete engine callback budget.
Clock origins differ by host instance and cannot be compared across connections.
Logical sample frames identify audio intervals, not elapsed execution time. Raw
host records preserve both engine stream frames and the external AudioContext
callback frame, which can start at a different origin.

Chrome for Testing 147.0.7727.117 exposes no `performance` object in its
AudioWorkletGlobalScope. On that browser the setting is acknowledged as requested,
but timing is explicitly unavailable and rendering continues. `Date.now()` and
logical audio time are not substituted for a monotonic execution clock. The raw
worklet contract independently exercises the production clock import, toggle,
timestamps and automatic captures with an injected monotonic clock. Browser
support needs a host-provided monotonic clock in that realm; it cannot be inferred
from `performance.now()` working in the main window.

## Runtime and transport bounds

Targets without 64-bit atomics report timing unavailable; the core still builds
for bare-metal targets and never substitutes locks for those atomics. This is a
hardware capability check, with no profiling Cargo feature or build option.

The portable recorder is `no_std`, uses no unsafe code and allocates its owners
before rendering. Off checks cached enable/reset state and performs no profiling
clock reads, sample writes or rolling history maintenance. Toggle/reset boundaries
can finish or invalidate a pending capture once. On reads one timestamp at each
node completion plus explicit callback, tick and phase boundaries. Storage copies,
publication and capture ownership swaps are bounded and never wait for a reader.

For a prepared pool of N nodes, each trace reserves `2*N + 24` 16-byte samples,
bounded by 65,536. Two circular banks reserve approximately 2 MiB each including
trace overhead (2–4,096 traces per bank), plus 64–1,024 independent live stream pages selected from an 8 MiB byte budget,
a callback trace and a spare page. `storage_bytes` reports prepared sample-array
bytes; it excludes owner/header/channel metadata. `high_water` is the maximum ready live-stream queue occupancy. Capacity exhaustion drops live batches with a
counter and does not borrow command, recovery or media capacity. Reserved history
remains independent of a stalled collector. Unsupported pool sizes report timing
unavailable rather than failing audio preparation.

The endpoint forwards at most 128 chunks per poll, with at most 24 samples per
chunk and independent profiling admission. Reliable messages have priority;
bulk, metering and profiling rotate fairly. Engine protocol version 8, application
protocol version 21, and SDM2 shared-memory descriptors reject incompatible peers.
The application backend collects and computes Welford statistics outside rendering;
live reports refresh at most five times per second. The worklet endpoint performs
bounded forwarding on the audio thread, including the existing record encoding
and JavaScript MessagePort work. These whole-host costs are distinct from the
allocation-free render kernel.

Raw `Packet` records retain generation, capture/sequence/callback IDs, tick and
callback frames, graph epoch, nanosecond boundaries, completion/fault flags and
trigger causes. Node keys are dense prepared slots; high bits distinguish phase
and layout markers. `Collector::raw_capture()` exposes only the latest current
capture. No timings are stored in session files. Timeline UI and export formats
are outside this feature.

## Reproducing validation

Release timing data, raw callback measurements, clock and UI costs, methodology
and platform limits are in the [measurement report](runtime_profiling/README.md).

The native production benchmark is
`cargo run --release -p shoop_rt_engine --example profiling_benchmark -- <strips> <quantum> <callbacks> <off|on|stalled> <steady|midi|recording|edits|partitions>`.
It times the whole offline callback with an independent clock, and control/draining
separately. Each strip has stereo routes, MIDI and a loop; the midpoint injects a
host xrun. The stalled mode withholds collection. It prints raw callback durations,
period budgets, misses, wire bytes, accepted samples, storage and loss counters.
Compare against the same harness on the recorded pre-feature commit; removing
profiling setup/collection from that harness does not change the workload.

Focused tests prescribe clock values independently and verify statistics,
completion boundaries, all five quanta, arbitrary host partitions, audio/MIDI,
faults, no-heap execution, saturation, capture leases, toggles and generation
invalidation. The packaged socket/shared-memory probe exercises application
projection and UI loss while recording. The browser core workflow checks requested
state, available/unavailable acknowledgement, graph resets and toggles through the
same application intents as the UI. The worklet artifact contract checks the raw
ABI and JavaScript processor together.
