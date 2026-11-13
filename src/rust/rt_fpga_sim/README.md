# Rust / Verilator integration

This optional native crate drives the existing TP/SP/JP pipeline and functional
AXI memory model through Marlin 0.21.1. There is no Python in its build or run
path. Existing cocotb suites remain independent.

From the repository root, with Rust, a C++ compiler and Verilator on PATH:

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-marlin
```

Optional positional arguments: scenario JSON, cached build directory, VCD path.
The default replay is `fixtures/instant-gains.json`. This direct demonstrator
accepts only current-boundary instant gain actions; it checks each output word
and the frame against the production Q8.24 oracle. It introduces deterministic
job stalls and holds TP responses under backpressure. Configuration transfers
verify full-width readback. Waits have cycle limits and return errors.

The simulation top has explicit widths, a Rust-driven clock and fixed small
capacities. Configuration and execution use the existing hardware ports; no
Rust sample executor substitutes for RTL. Each cycle evaluates low/high/low,
and a VCD records all three phases with monotonic timestamps. Reset drops host
valids before asserting reset. Models borrow their Marlin runtime; they must be
dropped before it. Create/build the runtime before starting any audio callback.

The wrapper caps C++ build parallelism at two jobs. Set `SHOOP_VERILATOR` and
`SHOOP_VERILATOR_JOBS` to override the executable and job count. Marlin caches the
compiled model separately from graph/configuration data. Loading another graph
or changing a gain does not rebuild C++.

Validated with Rust 1.99.1, Verilator 5.032 and GCC 14.2 on Linux x86_64.
Marlin is behind `sim`; ordinary production and Wasm builds do not depend on it.
Simulation wall time is not evidence of a board deadline.

## Queue-driven comparison and replay

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo
cargo test -p rt_fpga_sim --features sim --test marlin
```

The demo accepts the same optional positional arguments (scenario, cache, VCD).
The default is `fixtures/parallel.json`; `fixtures/chain.json` exercises successive
rounding stages. Both include signed low-bit samples, future and late aligned
controls. The same producer submits ordinary queue requests to native Q8.24 and
`rt_fpga_adapter`, including slot retirement/installation and the production plan.
The adapter receives no fixture image. The comparison checks raw samples,
completions, owning retirement presence, execution receipts, epochs and frames.
On failure, rerun the printed scenario with an explicit VCD path. Device faults
retain the underlying driver diagnostic in `Simulator::last_error`.

The integration tests also cover silence, reset, backpressure, bounded timeouts,
746-bit odd-depth FIFO wrap, and an injected real RTL fault. One original
record/stop/playback/wrap fixture runs directly through Marlin, checking audio,
loop state and captured media against production Q8.24. The direct media loader
supports initially empty views for this regression; importing arbitrary stored
media remains outside scope for this profile. The separate mono queue profile
below routes loop commands and media operations through the runtime transport.

Each model owns a separate Verilator context. A small vendored patch to the
pinned bridge fixes stale trace callbacks retained by the default context; see
[patch details](../../../third_party/marlin-verilator/SHOOP_PATCH.md). Keep models
and traces on their creating service thread. Traces close before models, and
models/context are destroyed before their Rust runtime. Parallel independent
runtime/model tests verify this ownership; no test serialization is required.

[Validation measurements](VALIDATION.md) distinguish C++ build/load, runtime graph
validation/lowering, configuration and clock counts. Ordinary CPU/Wasm dependency
trees contain no Marlin; normal app code still uses its native runtime.

## Queue-driven mono looper

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-mono-demo
cargo test -p rt_fpga_sim --features sim --test mono_native --test mono_protocol --test mono_rtl --test mono_replay
```

The default trace is `fixtures/mono-loop.json`. Optional arguments are the trace,
C++ cache/output directory, and VCD path:

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-mono-demo -- \
  src/rust/rt_fpga_sim/fixtures/mono-loop.json work/marlin-mono work/mono.vcd
```

Each version-1 JSON step provides exactly 32 signed raw Q8.24 input words and a
list of named commands. Both backends receive the same producer's production
preparation, initial queued graph commit, loop/gain commands and owning reads.
No expected image, jobs, state or samples are supplied to the hardware adapter.
Operations include record/stop/play, gain, state, export, cursor, read, release,
domain cancel and pre-dequeue ticket cancellation. `at` is an optional absolute
frame, `recovery` selects the recovery lane, and `hold_results` leaves completions
unread to exercise queue pressure. Reads/cursors/releases reference earlier
named results. The final step must drain results. Unknown references or unsupported
trace versions fail before they can manufacture a successful comparison.

Checked-in fixtures cover retained takes across re-recording and wraps,
unaligned byte reads/EOF, release/stale handles and storage reuse; empty playback
and supersession; unread-result pressure; pending/export/read-buffer capacities;
distinct latest/playback media after recording restarts; and delivered cancellation
with recovery/normal ordering. Two seeded traces
exercise 48 additional quanta each. Small device protocol tests gate settlement
acknowledgement and inject failures without implementing any DSP. Physical RTL
fault and arena exhaustion tests check sticky fault/receipt behavior separately
from native elastic-storage comparisons.

The runner checks every output word, exported byte, post-quantum state, query cut,
revision, event, completion, execution receipt, epoch and requested/effective/
terminal frame. Scoped opaque handles use a checked bijective correspondence.
It writes canonical outcomes and counters to `result.json`; the first mismatch
includes quantum/operation context. The CLI prints a replay command with a VCD
path on failure. Every trace asserts one compilation/activation and stable
bindings across transitions/gains. Deterministic job/response stalls are enabled.

The new path uses Rust, Marlin/Verilator and C++ only. Existing Python RTL gates
are independent regressions, not dependencies of this runner. Supported protocol
and capacity details are in the adapter README; local results and separate model,
graph/configuration measurements are in [mono validation](MONO_VALIDATION.md).

## Offline application Sync loop

The optional application composition uses the real application service, client,
engine endpoint and owning runtime queues. Its external consumer compiles the
submitted production graph and topology for the RTL pipeline. It neither runs a
CPU graph runtime nor executes the native loop planner. The real prepared node
objects supply their fixed signal descriptions and control validation.

```sh
cargo run -p rt_fpga_sim --features app-ui --bin fpga-app-demo
cargo run -p rt_fpga_sim --features app --bin fpga-app-demo -- \
  --compare src/rust/rt_fpga_sim/fixtures/app-sync-loop.json
```

Use `--replay` instead of `--compare` to run hardware alone. Add `--vcd work/app.vcd`
to either command for a trace on the worker's model thread; comparisons report the
first differing action/frame/channel. VCD files can be large.

The window uses the ordinary application widgets. It starts with the default
mono Sync strip and loop. Use the connections dialog to connect logical
`audio_in_1` to Sync and route Sync to Master; Master connects to the logical
outputs. The synthetic-input checkbox switches between deterministic in-memory
samples and silence. There is no host process, JACK/CPAL connection, device
enumeration or audible output. The engine reports a simulated-clock driver, and
the status overlay names the RTL simulation. The MEM headline shows RTL storage
pressure from the hardware allocators, refreshed every 200 ms of audio time; at
simulation speed that is several seconds of wall time. Additional tracks, MIDI,
FX, imported sessions, composites, preview and metronome audio are outside this
fixed-session profile.

A worker owns the Marlin runtime, model and queue consumer. The normal window
runs continuously, starting each 32-frame iteration immediately after its
predecessor. UI polling uses its own elapsed time and imposes no sample-rate
pacing or quantum deadline on the worker. Deterministic replays use bounded
batches, also processed as fast as possible. Continuous mode retains no sample
or command-history log. Dropping the backend stops and joins the worker.

Hardware owns transitions, audio arithmetic and media contents. Host bookkeeping
retains candidates and media leases, applies monitoring policy to observed TP
state, publishes observations and streams FPGA samples through the existing
reader ring. App-facing audio is float; hardware samples and coefficients are
Q8.24. The reader converts that numeric encoding explicitly. The 240-frame
control ramps and default 144-frame restart smoothing remain enabled. Route
activation preserves the TP clock, captured media and smoothing history; mode
and scalar controls leave the program installed.

The shared model has 256 instructions, 256 state words, 128 scratch buffers,
32 requests, 64 events, 32 media views and 32768 physical 32-sample slots.
Ordinary small regressions keep a 128-slot logical arena. The application preset
uses all 32768 slots (4 MiB of sample words) and reserves chunk-table pages for
twice that many mappings, including snapshot/primary-view overlap. It includes
both prepared channels; the second channel is inactive in the mono session. This admits two five-second
takes at 48 kHz while retaining the previous take. `Handle::store_usage` reports
issued views, the slot high-water mark, free slots and the logical limit from
actual device registers. The headless demo prints these alongside compiler and
activation timings. This is bounded FPGA storage, independently of native
elastic media storage.

Snapshot, retirement and capture-bank reset currently walk chunk metadata. The
simulator's cycle watchdog scales with bounded arena size for those operations;
it is a hang guard, not a real-time deadline. These operations need separate
worst-case analysis before claiming hard real-time recording transitions on a
board. No DRAM timing simulation is used by this composition.

Focused application validation:

```sh
cargo test -p rt_fpga_sim --features app --test app_native --test app_protocol \
  --test app_arithmetic --test app_media
cargo test -p rt_fpga_sim --features app --test app_rtl -- \
  --skip application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
cargo test -p rt_fpga_sim --features app --test app_rtl \
  application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
```

The long test executes ten seconds of recording plus waveform transfers in RTL
and can take several minutes. The common app replay compares native float audio,
loop observations, pending transitions and waveform samples. Exact arithmetic
checks use the production Q8.24 `Gains` and `AudioRenderer`, including extreme
signed samples, interrupted ramps, short loops and changing smoothing lengths.
EOF, byte/page alignment, retained takes, stale scopes and generation reuse are
covered separately. Queue tests use recording/failing devices without a DSP
implementation, so they cannot supply substitute audio results.
