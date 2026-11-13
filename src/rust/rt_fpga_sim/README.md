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

Marlin options cap C++ build parallelism at two jobs. Set `SHOOP_VERILATOR` and
`SHOOP_VERILATOR_JOBS` to override the executable and job count. Marlin caches the
compiled model separately from graph/configuration data. Loading another graph
or changing a gain does not rebuild C++.

Build the validated Verilator locally with `bash scripts/build_verilator.sh`.
It requires autoconf, flex, bison, a C++ compiler and zlib headers. On NixOS run
`nix develop --command bash scripts/build_verilator.sh`; the development shell
supplies these build dependencies.
Then add `$PWD/work/verilator/bin` to `PATH`, or set `SHOOP_VERILATOR` to that
absolute executable path. `SHOOP_VERILATOR_JOBS` defaults to two. CI uses the
same script and pinned source revision. Runtime startup rejects versions other
than 5.032 with a diagnostic naming the supported version.

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
trees contain no Marlin; the optional regular application backend uses the RTL consumer.

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

## Regular application backend

Build and select the optional backend on Linux:

```sh
bash scripts/build_verilator.sh
export PATH="$PWD/work/verilator/bin:$PATH"
cargo build -p shoop_host -p shoopdaloop --features fpga-sim
# In-process host:
target/debug/shoopdaloop --audio-backend fpga_rtl_sim --simulated-input synthetic
# Independent host, then the regular application:
target/debug/shoop_host --audio-backend fpga_rtl_sim --simulated-input synthetic \
  --export-profile work/fpga-host.json
target/debug/shoopdaloop --host-profile work/fpga-host.json --ui-only
# The regular companion-host path:
target/debug/shoopdaloop --audio-backend fpga_rtl_sim --session-transport socket
```

The restart-required Audio backend setting accepts `fpga_rtl_sim`; native
processing remains `new_engine`. Selecting RTL on the CLI defaults to its
Simulated clock. An explicitly selected CPAL/JACK/dummy driver is refused.
The input mode is `silence` (default) or `synthetic`, configured through
`--simulated-input` or `driver.Simulated.input` in host JSON. Output is discarded.
There are no RTL-specific windows or controls.

`SHOOP_FPGA_RTL` selects the FPGA source directory (default: repository `fpga/`),
`SHOOP_FPGA_CACHE` selects the model cache (default: `work/marlin-host`), and
`SHOOP_VERILATOR` selects the executable. Startup validates Verilator 5.032 and
logs model preparation. Companion readiness allows three minutes for a cold
build. Builds on other platforms return an unsupported-platform startup error.

The ordinary connections dialog connects logical `audio_in_1` to Sync and routes
Sync to Master; Master connects to the logical outputs. The supported fresh
session is one mono Sync strip and loop with stereo Master, Q=32 and audio only.
Extra tracks/loops, MIDI, FX, composites, preview and metronome audio are outside
the fixed profile. Prepared pools and host ports use the adapter's profile.
The status overlay reports Audio backend and Audio driver independently. MEM
shows allocator pressure, refreshed every 200 ms of simulated audio (several
seconds of wall time). No audio device is opened and there is no audible output.

The deterministic replay harness uses the same host worker through
`shoop_rt_host_fpga_sim` tests, with the real service, client, endpoint and queues.
It compiles the submitted graph and topology to RTL without a CPU graph runtime
or native loop planner. Numeric bounds remain in [APP_CONTRACT.md](APP_CONTRACT.md).

A worker owns the Marlin runtime, model and queue consumer. The regular application
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
actual device registers. The replay tests record these alongside compiler and
activation timings. This is bounded FPGA storage, independently of native
elastic media storage.

Snapshot, retirement and capture-bank reset currently walk chunk metadata. The
simulator's cycle watchdog scales with bounded arena size for those operations;
it is a hang guard, not a real-time deadline. These operations need separate
worst-case analysis before claiming hard real-time recording transitions on a
board. No DRAM timing simulation is used by this composition.

Focused application validation:

```sh
cargo test -p shoop_rt_host_fpga_sim --features sim --test app_native --test app_protocol \
  --test app_arithmetic --test app_media
cargo test -p shoop_rt_host_fpga_sim --features sim --test app_rtl -- \
  --skip application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
cargo test -p shoop_rt_host_fpga_sim --features sim --test app_rtl \
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
