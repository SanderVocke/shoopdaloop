# Runtime adapter milestone validation

2026-11-12, Linux x86_64, Rust 1.99.1, GCC 14.2, Verilator 5.032,
Marlin 0.21.1 with the documented owned-context bridge patch. Upstream FPGA
branch changes through `2b099206` were merged during implementation.

## Differential and protocol checks

- Five Marlin tests pass together, without serializing the simulations: direct
  gain/silence, odd-depth wide FIFO, queue-driven chained/parallel gains, original
  record/playback replay, and an injected real RTL fault.
- Both queued fixtures compare all 320 raw output samples exactly, plus transport
  completions, execution receipts, sequences, epochs and effective/actual frames.
  Each accepted graph compiles once. Eight typed gains write state without graph
  reloading. Inputs, graph and actions are checked-in replayable JSON.
- The original record/stop/play/wrap fixture compares nine quanta, logical loop
  state and all 64 captured samples with production Q8.24. This is a direct driver
  regression, not looping support in the queue adapter.
- Six adapter tests use recording/stalling devices without sample DSP. They check
  ownership return, activation acknowledgement, capacity/credit retention, unread
  receipts, cancellation, stale IDs/epochs, unsupported operations, invalid plans,
  silent lowering, live-edit rejection and device faults.
- The shared production-fixture tests pass (four tests), including the Q8.24/float
  oracle comparison. The affected runtime library tests pass (95 tests), with
  additional transport/scheduler integration coverage in the workspace run.

## Example measurements

A fresh `work/marlin-adapter-acceptance` directory built/loaded the RTL model in
29.17 s. The next model load reused its library in 85.64 ms, and the direct gain
demo reused it in 80.90 ms. No C++ rebuild occurred for graph or gain data.

| Queue replay | Graph validation/lowering | Configuration | Instructions | State words | Payload bytes | Total clocks |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Parallel gains | 404.49 µs | 1.106 ms | 5 | 2 | 334 | 6260 |
| Chained gains | 28.48 µs | 477.54 µs | 4 | 2 | 322 | 5531 |

These are individual host observations taken while the native workspace suite
was running, not latency distributions or ARM deadline claims. Configuration
includes handshakes, verification readbacks and deliberate idle stalls. Payload
bytes exclude framing/readbacks; total clocks include reset/configuration, raw
sample transfers, TP commands, deterministic stalls and ten quanta of processing.
They are not isolated job-processor cycle counts. The direct gain replay passed
ten quanta in 5067 total clocks.

## Reproduction

From the repository root with the documented Rust/C++/Verilator prerequisites:

```sh
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo -- \
  src/rust/rt_fpga_sim/fixtures/parallel.json work/marlin-replay work/parallel.vcd
cargo run -p rt_fpga_sim --features sim --bin fpga-runtime-demo -- \
  src/rust/rt_fpga_sim/fixtures/chain.json work/marlin-replay
cargo run -p rt_fpga_sim --features sim --bin fpga-marlin
cargo test -p rt_fpga_sim --features sim --test marlin
cargo test -p rt_fpga_adapter -p rt_backend_testbench --lib
```

The new run/build path invokes Rust, Verilator, Perl and C++/make; it invokes no
Python or cocotb. The existing cocotb suites remain independent. A fresh artifact
directory rebuilds the model; reusing it caches C++ independently of graph data.

## Workspace gates

Formatting, the Rust test-attribute policy and warning-denying workspace build
pass. Native `shoopdaloop` and Wasm `shoop_rt_host_worklet` dependency trees contain
no Marlin/Verilator. Both `shoopdaloop` and `shoop_rt_host_worklet` build for
`wasm32-unknown-unknown` with warnings denied. The feature-enabled simulator also
builds with warnings denied.

The broad native run used:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace \
  --features shoop_engine/app_backend,shoop_backend/native-fx --profile ci
```

It exercised 3220 tests: 3209 passed, 11 failed, 11 skipped. Seven failures were
LAN host startup deadlines, one a recovery startup deadline, one a settings-write
deadline, and two involved a host test binary overwritten by an overlapping
workspace build (including a missing `testing`-feature configuration field).
All 11 failed tests passed on one isolated retry with the same feature selection,
`--test-threads 1` and no concurrent builds (142.79 s). The added stalled-device
activation test also passed in its focused six-test adapter gate. This records
the original failure rather than presenting the first broad run as clean.

No PR CI was inspected, and the previously waived 64-loop benchmark was not
repeated. PR #979 targets `proto/fpga-rust-reference`; only the implementation
branch was pushed. No automated review findings were present at delivery.

## Offline application Sync loop — 2026-11-13

The application milestone uses the actual fresh `Service`, ordinary application
client, `RtBackend`, production engine endpoint and owning runtime queues.
`engine::Adapter` consumes the real engine command types and compiles the submitted
ordered graph/topology. TP owns transitions; SP/JP and FPGA media own audio and
recordings. Native planner/DSP execution is absent from the hardware composition.
The optional window uses the ordinary application widgets and an unpaced worker:
finish an iteration, immediately start the next. UI polling is independent.
There is no host process or physical audio/MIDI I/O.

The common version-1 app trace covers startup, routes, record/play/wrap, loop and
strip gain, mute, waveform selection/read, stop, clear/re-record and scheduled
stop. Additional cases cover default global sync, repeat/self-pulses, gain/balance
retargeting during ramps, monitoring, Master controls, supersession/cancellation,
reconnect and worker shutdown. State-only controls preserve the installed program.
Protocol doubles stall/fail device configuration without executing audio, and
exercise candidate ownership, stale generations, incomplete payloads, normal-queue
pressure, recovery and terminal receipts after partial activation failure.

Exact arithmetic checks use production Q8.24 `Gains` and `AudioRenderer` against
RTL, including extreme signed words, 97/55-frame ramps, 144-frame smoothing,
short loops and restart/stop. The common actual-app replay compares float audio,
loop/revision/pending observations and waveform samples. Its bounds were declared
before RTL acceptance: maximum absolute error 2e-5, RMS error 5e-6. A measured
16-action, 13776-frame run had maximum error 1.6018748e-7 and RMS 1.7223865e-8.
The CLI reports the first differing action/frame/channel and rejects nonfinite
samples. `--vcd` owns tracing on the model worker thread.

The full app capacity case passed in 1000.08 seconds. It records five seconds at
48 kHz, loads/retains that waveform, records a second five-second silent take,
checks both contents, clears and records/plays again. The allocator high-water
mark was 30032 of 32768 physical 32-sample slots; clear/re-record reuses storage.
Both prepared channels consume slots even though the session is mono. A separate
host-port metadata fixture retains both stereo-prepared takes and their snapshots.
This exposed a chunk-table capacity shortage before sample slots were exhausted;
the preset now reserves mappings for twice the physical slot count plus view
overhead. Odd-byte/page reads, EOF, retained snapshots, release and stale cursor
identities have independent coverage.

One warm-cache headless run measured:

| Measurement | Value |
| --- | ---: |
| Model load | 150.98 ms |
| Device/adapter preparation | 19.61 ms |
| App readiness | 2.569 s |
| Graph compilation, four structural activations | 330.76 µs total |
| Device configuration for those activations | 4.305 ms total |
| Installed instructions / state words / scratch buffers | 65 / 23 / 61 |
| Accumulated callback execution wall time | 20.666 s |
| Total simulated clocks | 4412127 |

A fresh model compilation took 20.18 s in the long-test run. These are individual
host observations, not timing distributions or board estimates. Clocks include
configuration, TP commands, sample transfers, reads and processing, rather than
isolated JP execution. Stepped replay admits explicit logical frames; every batch
runs at maximum speed. Continuous execution retains no output/command history.

Snapshot/retirement/capture-bank reset still walk bounded chunk metadata. The
watchdog scales with the arena for those operations; it is a hang guard, not a
deadline. Board worst-case transition time and DRAM timing remain unvalidated in
this offline composition. General graph/session replacement, other tracks/loops,
MIDI, FX, imported media, file persistence and live audio adapters remain outside
the fixed-session profile. The FPGA endpoint does not invent the native backend's
new resource-memory report; simulator allocator counters are reported separately.

Reproduce from the repository root:

```sh
cargo run -p rt_fpga_sim --features app --bin fpga-app-demo -- \
  --compare src/rust/rt_fpga_sim/fixtures/app-sync-loop.json
cargo run -p rt_fpga_sim --features app-ui --bin fpga-app-demo
cargo test -p rt_fpga_sim --features app -- --skip application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
cargo test -p rt_fpga_sim --features app --test app_rtl \
  application_store_holds_a_five_second_take_while_recording_another_and_reuses_it
```

Add `--vcd work/app.vcd` to replay/comparison for a diagnostic trace. VCD can be
large. The long app case and isolated metadata capacity test need not be repeated
for documentation-only changes; their storage RTL/preset remains unchanged.

After merging upstream through `e8c43c6d`, all 38 short simulator tests pass.
The affected adapter/testbench/runtime/loop/engine library gate passes 192 tests
(one pre-existing ignored engine test). Warning-denying workspace and optional
demo builds pass. The normal Wasm app and worklet build successfully, and their
dependency trees remain free of Marlin/Verilator. Formatting and the test-attribute
policy pass. The final merged-state CLI replay reproduces the same 13776 output
frames, 4412127 clocks and numeric errors above. A separate one-action VCD smoke
passed, producing a 17 MiB trace through normal model destruction.

The broad native workspace run reported 3219 passes, four failures and 11 skips.
The first isolated retry, concurrent with the long RTL simulation, still failed
all four. With no concurrent simulation/build, the final targeted native retry
passes the legacy graph test and both app driver/settings tests. One host driver
recovery test still times out at startup. That same test also fails on untouched
`821ba859`, at a later recovery deadline; the other three pass there. This is not
a clean broad native gate, and the different failure cuts do not establish a
single root cause. The baseline reproduction and unchanged native construction
semantics support leaving that driver investigation outside this offline feature.
No test deadlines or expectations were relaxed. Local logs are under `work/app-*`.
