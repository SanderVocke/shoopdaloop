# Native FPGA component verification

The restored [Rust component reference](../rt_fpga_reference/README.md) is exercised
by `--test reference`. The same explicit programs, records and samples run through
independent Rust models and real RTL; jobs, state, errors, buffers and output
samples are compared exactly. Existing vector tests remain independent checks.

The shared [`rt_fpga_compiler`](../rt_fpga_compiler/README.md) emits fixed logical
programs behind `StateProgramTarget`. `target::RtlTarget` produces packed
instructions, state/loop RAM images and physical control bindings. Its typed
output is loaded directly by the state/audio drivers. The compiler and sync tests
compare these programs against `target::RustTarget`, with deliberately different
state addresses: 96 mono-loop SP quanta and 120 sync/smoothing SP/JP sample quanta.
The reference's carrier tests use the same mono-loop fixture definition. The
shared compiler also accepts the restricted mono graph descriptions used by the
reference host. Processor pacing and control endpoints remain separate from
preparation.

The reference crate also has Rust-only transition and full functional pipeline
tests: `cargo test --locked -p rt_fpga_reference --all-features`. They compare
directly configured SP plans against the native resolver and exercise
SP sync/audio passes followed by JP with logical media. The remaining Rust TP
and its request/completion bookkeeping have been removed. `--test sync` compares integrated
RTL sync/audio instruction execution against Rust (1,152 sync quanta and 120
combined SP/JP audio quanta). The ordinary component benches also retain explicit
record fixtures. There is no separate RTL transition processor.

`rt_fpga_testbench` preserves the Marlin/Verilator simulation framework and
component drivers. Its control integration can reuse the concrete queue-backed
`shoop_rt_transport::rt::Endpoint`, which directly implements `carrier::Endpoint`
for command groups and snapshots and ignores legacy requests. The program loader
uses the same command/response FIFOs for ordered program switches.
`RtlTarget::build_image` binds retained addresses and emits only fresh-state
writes; `drivers::loading::Upload` issues direct memory transactions.
`drivers::loading::Commands` bridges the switch record to the RTL boundary/fence
handshake and retains its FIFO credit until completion. The component harness
does not provide a Marlin application host or a complete RTL runtime-command/
snapshot frontend. Carrier polling remains independent from simulation clocks.

`--test loading` compares live Rust/RTL programs, retained ramps, smoothing
history, plans and frames. It stages during execution, exercises cancellation,
slot reuse, stale commands and response backpressure, and switches a media graph
from recording into playback with delayed AXI completions. See
[the loading notes](../../../docs/fpga/live-loading.md).

The simulation framework needs Verilator **5.032**, a C++ compiler and Rust.
Simulation is opt-in; default native and Wasm builds do not depend on Marlin.

CI runs the shared compiler, functional reference, command transport and the
feature-gated `shoop_host` application client in a separate Rust job. Compiler and
transport changes also trigger RTL verification. The Marlin job retains the full
component matrix, and standalone synthesis checks the generated AXI wrappers.

```sh
bash scripts/build_verilator.sh
export PATH="$PWD/work/verilator/bin:$PATH"
python3 fpga/axi_memory/build_dramsim3.py
cargo test --locked -p rt_fpga_testbench --features sim
cargo check --locked -p rt_fpga_testbench
python3 scripts/check_fpga_testbench_coverage.py
```

The DRAMsim3 builder needs CMake and patch and fetches its pinned dependency.
Use the repository build environment guidance. The local Marlin patch remains
in `third_party/marlin-verilator`.

Focused runs:

```sh
cargo test --locked -p rt_fpga_testbench --features sim --test midi -- --test-threads=1
cargo run --locked -p rt_fpga_testbench --features sim -- state --config small
cargo run --locked -p rt_fpga_testbench --features sim -- audio --config small
cargo run --locked -p rt_fpga_testbench --features sim -- pipeline --config small --memory-model fixed
cargo run --locked -p rt_fpga_testbench --features sim -- pipeline --config small --memory-model dramsim3 --axi-data-width 32
cargo run --locked -p rt_fpga_testbench --features sim -- deferred --memory-model dramsim3
```

The `midi` test target includes compiled mixed SP/JP programs, exact audio and
MIDI output/job/state comparisons at all supported quanta, publication fences,
partial output and fatal recovery. It also retains DDR service scenarios and
AXI32/64 checks. See the [coverage and limits](../../../docs/fpga/midi-shared-jobs-validation.md).

`pipeline` is the TP-free SP -> JP -> media/cache execution harness. Explicit
instructions, initial state and samples exercise exact output, recording/readback,
retained ranges, persistent state, prefetch, partial faults and reset across runs.
It does not resolve transitions, emulate ARM or execute application requests.
The separate `state` and `smoothing` suites supply logical records directly.

Other suites cover chunk, media, metadata, ownership, address resolution,
reclamation, AXI32/64, delayed memory and standalone audio I/O. `cargo test` runs
all retained test targets, including realistic-memory cases. The CLI `all` runs
its listed suites using the selected memory mode; it is not the complete test
matrix. Stress duration and random cases can be reduced for iteration.

Model compilation, elaborated port widths, wide packing, independent instances,
clock/edge agents, ready/valid monitors, watchdogs, tracing, probes, memory agents,
DRAMsim3 and reproducible reports remain reusable. Set `SHOOP_VERILATOR` to choose
the executable and `SHOOP_VERILATOR_JOBS` to limit model build concurrency.
`--output` selects report output; `--waves` enables VCD for focused debugging.
Model caches live under `work/marlin-suite`; remove that directory for a fresh run.

The coverage inventory preserves all 75 historical definition hashes: 62 retained
obligations and 13 explicitly retired TP/host/system obligations. It is checked
against actual source functions and Cargo test targets. The old transition,
production RTL, ARM control, continuous full-loop, demo and ZedBoard commands are
removed. Production CPU scenarios and numeric checks remain in
`rt_backend_testbench`, independently of hardware.

Production CPU analysis also offers `rt-backend-testbench graph`: canonical node
and connection ordering for numeric error bounds, obtained from the production
compiler. It emits no instructions, physical media layout or ARM transport data.
The independent Python float/fixed gate uses that ordering and logical sessions.
