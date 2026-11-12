# Production Rust / RTL behavioral contract

The common runner executes production `rt_runtime::GraphRuntime` in Current mode,
the production `shoop_loops` planner/resolver and production audio DSP using Q8.24.
The other runner executes `loop_pipeline_axi`. Both consume the same scenario;
neither runner supplies the other's execution results. Default application
processing remains floating point.

## Inputs

A versioned document names the numeric profile, quantum, chunk size, capacities,
logical signal nodes and ordered connections, primitive loops and sync sources,
initial logical media, timestamped actions, input sample channels and observation
cuts. Q8.24 samples and control values are signed raw i32 integers. Time is an
unsigned sample-frame count. No RTL state addresses, predicted decisions or job
lists are source inputs.

Each action has a scenario identity independent of its backend request slot.
Actions include play/retrigger, stop, record, seek, immediate/absolute or
source-tick timing, cancellation and completion collection, and supported gain
changes. A retained completion continues occupying its declared request credit.
Requested/effective/terminal frames are distinct. Source ticks follow production
resolver ordering, separately from audio dependencies.

Production graph compilation determines ordered nodes and incoming connections.
The RTL compiler exports that order and lowers supported node internals into
concrete instructions, persistent state bindings and buffers. It may expand nodes
but cannot regroup additions or combine successive gains. Timing topology uses
the production timing compiler's canonical component order.

## Outputs

At every requested cut, compare the complete quantum's raw output samples and
the state after logical advancement to the next boundary: loop mode, position,
length, cycle/pulse state, supported revisions and pending/action outcomes.
Record admission, delivery/application and terminal outcomes with their real
frames. Media exports compare logical samples and lengths under scenario names,
including retained older content. Never compare physical chunk numbers or row
addresses as semantic identities.

Internal jobs, instructions, scratch samples, request slots, cache traffic and
cycles are optional diagnostics. State normalization is limited to documented
identity translation and equivalent observation phases. No numerical tolerance,
automatic sample alignment, dropped prefixes/tails, or endpoint-only ramp checks.
The runner reports the first differing sample/event/frame and writes a replayable
scenario, generated image, actual/expected results, seed and tool/profile metadata.

Direct runtime calls have no device latency. Host-buffered partition tests declare
their fixed latency separately and use the same command admission cuts. They do
not shift outputs after observing them.

## Numeric contract

Production float remains the ultimate oracle. The common runner also executes
the same sessions in float, comparing exact logical behavior and checked bounded
audio error against Q8.24. Record maximum absolute/RMS error, signed bias and drift;
derive error budgets from conversion and the actual ordered arithmetic stages.
Cover long sessions and interrupted/short ramps. Clipping cases are explicit:
float may exceed the fixed range, so report the intended saturation difference
and clipping locations/counts separately. This comparison never weakens the
bit-exact Q8.24/RTL sample and media comparisons.

Samples and gains are signed Q8.24. Every binary mix saturates to i32. Gain
multiplication uses a signed widened product, division by 2^24 truncated toward
zero, then saturation. Preserve expression grouping and each intermediate
rounding point. Float device/persistence conversion occurs only at an explicit
boundary; raw fixed samples and controls never round-trip through f32.

For duration D > 0, ramp gain at integer offset n is
`start + trunc0((target - start) * min(n, D) / D)` in raw units. The first sample
uses start; offset D and later use target; D=0 applies target immediately. Keep
start/target/duration/elapsed across quanta. A new ramp starts at the value of the
interrupted ramp at that boundary. Widen subtraction and multiplication before
division; the difference of two i32 endpoints need not fit i32.

## Supported matrix and required observations

| Feature | Required directed evidence |
| --- | --- |
| Signal routing | Host input/output, fan-out, ordered fan-in, gains before/after mixes; saturating adversarial operands |
| Recording | Empty first take, several quanta, retake, stop/play, exact captured media and uninterrupted unrelated playback |
| Playback | Whole-quantum chunks, multi-chunk windows, loop wrap, start/stop/retrigger and aligned seeks |
| Controls | Unity, zero, negative/extreme gains; zero/one/short/long ramps, both slopes, interruption and target hold |
| Scheduling | Absolute timestamps, quantum rounding, source skip counts, same-frame conflicts and retained outcomes |
| Synchronization | Chains, self-sync, legal cycles, repeat waits and canonical pulses/wraps |
| Request lifecycle | Cancellation before application, supersession, collected/uncollected completion capacity, stale IDs |
| Media lifetime | Read-only retained ranges, retake preserving retained content, reuse and read/write/read dependencies |
| Shapes | Mandatory Q=32/chunk=32; valid alternate quantum/chunk ratios and small/odd physical capacities |
| Memory | Same functional output for instant/fixed/stalled AXI; selected DRAMsim3 runs; AXI32/64 |
| Fault containment | FPGA-only allocation/descriptor exhaustion, late memory errors, partial output, sticky silence, coordinated reset |

The initial full-pipeline gate is mono Q=32 record -> stop -> play -> multiple
wraps. Grow to every matrix row and seeded sessions before deleting old tests.
The existing 64-loop workload remains a required scale/timing check.

No MIDI, composites, FX/plugins, dry/wet, grab/replacing, live topology changes,
cross-quantum prefetch or board interface is implied. Unsupported profile requests
must be reported, not quietly omitted. Hardware-only failures are checked against
their declared contract; the production planner's graceful resource refusals are
not presented as identical to the RTL's fatal processing failures.

## Current upstream execution contract

Baseline `e5c015be` follows `SIMPLIFY.md`: transition settlement updates one bank,
SP/JP execute with stable settled records, clocks advance after successful sample
work. BeginCapture automatically creates/clears a stable recording view. There
are no public observations, preview/commit/retry tokens or external executors.
Ordinary results are observed between completed processing operations. A failed
block is silent and requires reset/reload; partial state is diagnostic only.
